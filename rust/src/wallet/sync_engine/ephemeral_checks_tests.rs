use super::*;
use std::cell::Cell;
use std::time::{Duration, SystemTime};

use zcash_client_backend::{
    data_api::{wallet::decrypt_and_store_transaction, TransactionDataRequest},
    proto::service::RawTransaction,
};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::BranchId;

const TIP: u32 = 2_000_100;

fn legacy_transaction(prevout: OutPoint, recipient: TransparentAddress, value: u64) -> Transaction {
    // Pre-Overwinter v1 transparent transaction, as in the recovery tests.
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.push(1);
    bytes.extend_from_slice(prevout.hash());
    bytes.extend_from_slice(&prevout.n().to_le_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&value.to_le_bytes());
    let script: Script = recipient.script().into();
    bytes.push(script.0 .0.len() as u8);
    bytes.extend_from_slice(&script.0 .0);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    Transaction::read(&bytes[..], BranchId::Sprout).unwrap()
}

fn raw(tx: &Transaction, height: u32) -> RawTransaction {
    let mut data = Vec::new();
    tx.write(&mut data).unwrap();
    RawTransaction {
        data,
        height: u64::from(height),
    }
}

struct Wallet {
    _dir: tempfile::TempDir,
    path: String,
    network: WalletNetwork,
    db: WalletDatabase,
    ephemeral: Vec<TransparentAddress>,
}

fn wallet() -> Wallet {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wallet.db").to_str().unwrap().to_string();
    let network = WalletNetwork::Main;
    let seed = keys::mnemonic_to_seed(&keys::generate_mnemonic()).unwrap();
    keys::init_db_and_create_account(&path, network, &seed, Some(2_000_000), "ephemeral").unwrap();
    let mut db = open_wallet_db_with_timeout(&path, network, SYNC_DB_BUSY_TIMEOUT).unwrap();
    db.update_chain_tip(BlockHeight::from_u32(TIP)).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    let ephemeral = conn
        .prepare(
            "SELECT cached_transparent_receiver_address FROM addresses
             WHERE key_scope = 2 ORDER BY transparent_child_index",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|a| TransparentAddress::decode(&network, &a.unwrap()).unwrap())
        .collect::<Vec<_>>();
    assert!(
        ephemeral.len() >= 2,
        "account creation reserves ephemeral addresses"
    );
    Wallet {
        _dir: dir,
        path,
        network,
        db,
        ephemeral,
    }
}

impl Wallet {
    /// Records the first leg of a ZIP 320 pair funding `address`.
    fn use_address(&mut self, address: TransparentAddress, n: u8) {
        let tx = legacy_transaction(OutPoint::new([n; 32], 0), address, 50_000);
        decrypt_and_store_transaction(
            &self.network,
            &mut self.db,
            &tx,
            Some(BlockHeight::from_u32(TIP - 50)),
        )
        .unwrap();
    }

    fn set_check_time(&self, address: &TransparentAddress, at: SystemTime) {
        let secs = at.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
        let conn = rusqlite::Connection::open(&self.path).unwrap();
        let changed = conn
            .execute(
                "UPDATE addresses SET transparent_receiver_next_check_time = ?1
                 WHERE cached_transparent_receiver_address = ?2",
                rusqlite::params![secs, address.encode(&self.network)],
            )
            .unwrap();
        assert_eq!(changed, 1);
    }

    fn request_at(&self, address: &TransparentAddress) -> Option<SystemTime> {
        self.db
            .transaction_data_requests()
            .unwrap()
            .into_iter()
            .find_map(|r| match r {
                TransactionDataRequest::TransactionsInvolvingAddress(req)
                    if req.block_range_end().is_none() && req.address() == *address =>
                {
                    Some(req.request_at())
                }
                _ => None,
            })
            .expect("ephemeral check request queued")
    }

    fn received_value(&self, address: &TransparentAddress) -> i64 {
        let conn = rusqlite::Connection::open(&self.path).unwrap();
        conn.query_row(
            "SELECT COALESCE(SUM(tro.value_zat), 0) FROM transparent_received_outputs tro
             JOIN addresses a ON a.id = tro.address_id
             WHERE a.cached_transparent_receiver_address = ?1",
            [address.encode(&self.network)],
            |row| row.get(0),
        )
        .unwrap()
    }

    async fn check(
        &mut self,
        now: SystemTime,
        response: Vec<RawTransaction>,
        fetched: &Cell<Vec<(String, u64, u64)>>,
    ) -> bool {
        ephemeral_checks::run_with(
            &mut self.db,
            &self.path.clone(),
            self.network,
            BlockHeight::from_u32(TIP),
            now,
            &|| false,
            |address, start, end| {
                let mut calls = fetched.take();
                calls.push((address, start, end));
                fetched.set(calls);
                async move { Ok(response) }
            },
        )
        .await
        .unwrap()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn first_pass_schedules_without_querying() {
    let mut w = wallet();
    let used = w.ephemeral[0];
    w.use_address(used, 1);
    assert_eq!(w.request_at(&used), None);

    let fetched = Cell::new(Vec::new());
    let now = SystemTime::now();
    assert!(!w.check(now, Vec::new(), &fetched).await);

    assert!(fetched.take().is_empty());
    let scheduled = w.request_at(&used).expect("scheduled after the pass");
    assert!(scheduled >= now && scheduled <= now + Duration::from_secs(10 * 24 * 3600));
}

#[tokio::test(flavor = "current_thread")]
async fn due_used_address_is_queried_stored_and_rescheduled() {
    let mut w = wallet();
    let used = w.ephemeral[0];
    w.use_address(used, 1);
    let now = SystemTime::now();
    w.set_check_time(&used, now - Duration::from_secs(60));

    let returned = legacy_transaction(OutPoint::new([9; 32], 0), used, 70_000);
    let fetched = Cell::new(Vec::new());
    assert!(w.check(now, vec![raw(&returned, TIP - 5)], &fetched).await);

    let calls = fetched.take();
    assert_eq!(calls.len(), 1, "one address per pass");
    assert_eq!(calls[0].0, used.encode(&w.network));
    assert_eq!(calls[0].2, u64::from(TIP));
    assert_eq!(w.received_value(&used), 50_000 + 70_000);
    assert!(w.request_at(&used).expect("rescheduled") > now);
}

#[tokio::test(flavor = "current_thread")]
async fn known_history_is_not_reported_as_new() {
    let mut w = wallet();
    let used = w.ephemeral[0];
    w.use_address(used, 1);
    let now = SystemTime::now();
    w.set_check_time(&used, now - Duration::from_secs(60));

    let first_leg = legacy_transaction(OutPoint::new([1; 32], 0), used, 50_000);
    let fetched = Cell::new(Vec::new());
    assert!(
        !w.check(now, vec![raw(&first_leg, TIP - 50)], &fetched)
            .await
    );
    assert_eq!(fetched.take().len(), 1);
    assert_eq!(w.received_value(&used), 50_000);
}

#[tokio::test(flavor = "current_thread")]
async fn never_used_addresses_are_not_queried() {
    let mut w = wallet();
    let unused = w.ephemeral[1];
    let now = SystemTime::now();
    // Give the unused address a due schedule directly.
    w.set_check_time(&unused, now - Duration::from_secs(60));

    let fetched = Cell::new(Vec::new());
    assert!(!w.check(now, Vec::new(), &fetched).await);
    assert!(fetched.take().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn unmined_first_leg_addresses_are_not_queried() {
    let mut w = wallet();
    let stale = w.ephemeral[0];
    // A first leg stored at creation whose broadcast never mined.
    let tx = legacy_transaction(OutPoint::new([1; 32], 0), stale, 50_000);
    decrypt_and_store_transaction(&w.network, &mut w.db, &tx, None).unwrap();
    let now = SystemTime::now();
    w.set_check_time(&stale, now - Duration::from_secs(60));
    assert!(
        w.request_at(&stale).is_some(),
        "the backend still queues it"
    );

    let fetched = Cell::new(Vec::new());
    assert!(!w.check(now, Vec::new(), &fetched).await);
    assert!(fetched.take().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn remaining_overdue_address_keeps_its_slot() {
    let mut w = wallet();
    let (a, b) = (w.ephemeral[0], w.ephemeral[1]);
    w.use_address(a, 1);
    w.use_address(b, 2);
    let now = SystemTime::now();
    w.set_check_time(&a, now - Duration::from_secs(120));
    w.set_check_time(&b, now - Duration::from_secs(60));

    let fetched = Cell::new(Vec::new());
    w.check(now, Vec::new(), &fetched).await;
    let calls = fetched.take();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, a.encode(&w.network), "earliest due first");
    // Rescheduling would push `b` into the future and skip its check.
    assert!(w.request_at(&b).unwrap() <= now);

    w.check(now, Vec::new(), &fetched).await;
    let calls = fetched.take();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, b.encode(&w.network));
    assert!(w.request_at(&a).unwrap() > now);
    assert!(w.request_at(&b).unwrap() > now);
}

#[tokio::test(flavor = "current_thread")]
async fn a_failing_address_is_deferred_behind_the_others() {
    let mut w = wallet();
    let (a, b) = (w.ephemeral[0], w.ephemeral[1]);
    w.use_address(a, 1);
    w.use_address(b, 2);
    let now = SystemTime::now();
    w.set_check_time(&a, now - Duration::from_secs(120));
    w.set_check_time(&b, now - Duration::from_secs(60));

    let path = w.path.clone();
    let result = ephemeral_checks::run_with(
        &mut w.db,
        &path,
        w.network,
        BlockHeight::from_u32(TIP),
        now,
        &|| false,
        |_, _, _| async { Err(SyncError::net("unavailable")) },
    )
    .await;
    assert!(result.is_err());
    assert!(w.request_at(&a).unwrap() > now, "failed address deferred");
    assert!(w.request_at(&b).unwrap() <= now, "next address stays due");
}
