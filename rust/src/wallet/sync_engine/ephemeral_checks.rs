//! ZIP 320 returned-funds detection for used ephemeral (TEX) addresses.
//!
//! A TEX recipient sees the pair's ephemeral address as the funding source and
//! may return funds to it; ZIP 320 requires wallets to recognize them. The
//! backend queues an unbounded `TransactionsInvolvingAddress` request per
//! ephemeral address and randomizes when each becomes due, so that a server
//! cannot cluster the addresses by request time. This pass honors that
//! schedule: it checks at most one due, previously used address per sync, over
//! its own channel (an isolated circuit when Tor is enabled), and only
//! reschedules once no used address is overdue. Rescheduling pushes every
//! overdue address into the future, so running it first would drop the check.
//! Ephemeral addresses without a mined output are skipped: nothing on chain
//! points at them, and querying them would disclose future TEX sources.
use std::collections::HashSet;
use std::future::Future;
use std::time::SystemTime;

use futures::{stream::BoxStream, StreamExt as _, TryStreamExt as _};
use transparent::address::TransparentAddress;
use zcash_client_backend::{
    data_api::{TransactionDataRequest, TransactionsInvolvingAddress, WalletRead, WalletWrite},
    proto::service::RawTransaction,
};
use zcash_keys::encoding::{encode_transparent_address_p, AddressCodec as _};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::{BlockHeight, BranchId};

use crate::wallet::db::{
    open_readonly_conn_with_timeout, with_wallet_db_write_lock, SYNC_DB_BUSY_TIMEOUT,
};
use crate::wallet::network::WalletNetwork;

use super::{enhancement, lwd, SyncError, WalletDatabase};

/// `KeyScope::Ephemeral` as encoded in the `addresses.key_scope` column.
const EPHEMERAL_KEY_SCOPE: i64 = 2;
/// Mean delay before re-checking an address, matching the backend's daily cadence.
const CHECK_INTERVAL_SECS: u32 = 24 * 60 * 60;

/// An address history, one transaction per item.
pub(super) type History = BoxStream<'static, Result<RawTransaction, SyncError>>;

/// Checks one due ephemeral address, then reschedules if none remain due.
/// Returns whether a transaction new to the wallet was stored.
pub(super) async fn run(
    lightwalletd_url: &str,
    db: &mut WalletDatabase,
    db_path: &str,
    network: WalletNetwork,
    tip: BlockHeight,
    should_exit: &impl Fn() -> bool,
) -> Result<bool, SyncError> {
    run_with(
        db,
        db_path,
        network,
        tip,
        SystemTime::now(),
        should_exit,
        |address, start, end| open_history(lightwalletd_url, address, start, end),
    )
    .await
}

async fn open_history(
    lightwalletd_url: &str,
    address: String,
    start: u64,
    end: u64,
) -> Result<History, SyncError> {
    let mut client = lwd::open_isolated_lwd_channel(lightwalletd_url).await?;
    let stream = lwd::get_taddress_txids(&mut client, address, start, end).await?;
    // The client lives as long as its stream is read.
    Ok(
        futures::stream::try_unfold((client, stream), |(client, mut stream)| async move {
            Ok(
                lwd::next_stream_message(&mut stream, "ephemeral check get_taddress_txids stream")
                    .await?
                    .map(|raw| (raw, (client, stream))),
            )
        })
        .boxed(),
    )
}

/// Stores each transaction as it arrives: the server decides how many there are.
async fn store_history(
    network: &WalletNetwork,
    db: &mut WalletDatabase,
    open: impl Future<Output = Result<History, SyncError>>,
) -> Result<bool, SyncError> {
    let mut history = open.await?;
    let mut stored = false;
    while let Some(raw) = history.try_next().await? {
        let known = Transaction::read(&raw.data[..], BranchId::Sapling)
            .ok()
            .and_then(|tx| db.get_transaction(tx.txid()).ok().flatten())
            .is_some();
        enhancement::store_address_transaction(network, db, &raw.data, raw.height)?;
        stored |= !known;
    }
    Ok(stored)
}

pub(super) async fn run_with<F, Fut>(
    db: &mut WalletDatabase,
    db_path: &str,
    network: WalletNetwork,
    tip: BlockHeight,
    now: SystemTime,
    should_exit: &impl Fn() -> bool,
    fetch: F,
) -> Result<bool, SyncError>
where
    F: FnOnce(String, u64, u64) -> Fut,
    Fut: Future<Output = Result<History, SyncError>>,
{
    let used = used_ephemeral_addresses(db_path, network)?;
    let mut due = due_requests(db, &used, now)?;
    let Some(request) = due.pop() else {
        reschedule(db)?;
        return Ok(false);
    };
    if should_exit() {
        return Ok(false);
    }

    let address = encode_transparent_address_p(&network, &request.address());
    let start = u64::from(u32::from(request.block_range_start()));
    let end = u64::from(u32::from(tip));
    // Drop the fetch on exit; a lock or reset must not wait for the stream.
    let result = tokio::select! {
        biased;
        _ = super::watch_for_exit(should_exit) => return Ok(false),
        result = store_history(&network, db, fetch(address, start, end)) => result,
    };
    let stored = match result {
        Ok(stored) => stored,
        Err(error) => {
            // Defer this address so a persistent failure cannot starve the others.
            let _ = with_wallet_db_write_lock("sync_engine.ephemeral_checks.defer", || {
                db.schedule_next_check(&request.address(), CHECK_INTERVAL_SECS)
            });
            return Err(error);
        }
    };
    if should_exit() {
        return Ok(false);
    }

    let checked = request.address();
    with_wallet_db_write_lock(
        "sync_engine.ephemeral_checks.notify_address_checked",
        || {
            db.notify_address_checked(request, tip)?;
            // Move only this address forward; the others keep their overdue slots.
            db.schedule_next_check(&checked, CHECK_INTERVAL_SECS)
        },
    )
    .map_err(|e| SyncError::db(format!("complete ephemeral check: {e}")))?;

    if due.is_empty() {
        reschedule(db)?;
    }
    Ok(stored)
}

/// Due requests for used ephemeral addresses, earliest-due last.
fn due_requests(
    db: &WalletDatabase,
    used: &HashSet<TransparentAddress>,
    now: SystemTime,
) -> Result<Vec<TransactionsInvolvingAddress>, SyncError> {
    let mut due = db
        .transaction_data_requests()
        .map_err(|e| SyncError::db(format!("transaction_data_requests: {e}")))?
        .into_iter()
        .filter_map(|request| match request {
            TransactionDataRequest::TransactionsInvolvingAddress(req)
                if req.block_range_end().is_none() && used.contains(&req.address()) =>
            {
                req.request_at()
                    .filter(|at| is_due(*at, now))
                    .map(|at| (at, req))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    due.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    Ok(due.into_iter().map(|(_, req)| req).collect())
}

fn is_due(request_at: SystemTime, now: SystemTime) -> bool {
    #[cfg(debug_assertions)]
    {
        // Regtest E2E cannot wait out the randomized daily schedule.
        if std::env::var_os("ZCASH_E2E_EPHEMERAL_CHECKS_DUE_NOW").is_some() {
            return true;
        }
    }
    request_at <= now
}

fn reschedule(db: &mut WalletDatabase) -> Result<(), SyncError> {
    with_wallet_db_write_lock("sync_engine.ephemeral_checks.schedule", || {
        db.schedule_ephemeral_address_checks()
    })
    .map_err(|e| SyncError::db(format!("schedule_ephemeral_address_checks: {e}")))
}

/// Ephemeral addresses funded by a mined transaction, i.e. put on chain by the
/// first leg of a ZIP 320 pair; the backend's gap limit counts the same uses.
/// A first leg that was stored but never mined exposed nothing, and the backend
/// would query it from height 0.
fn used_ephemeral_addresses(
    db_path: &str,
    network: WalletNetwork,
) -> Result<HashSet<TransparentAddress>, SyncError> {
    let conn = open_readonly_conn_with_timeout(db_path, Some(SYNC_DB_BUSY_TIMEOUT))
        .map_err(SyncError::db)?;
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT a.cached_transparent_receiver_address
             FROM addresses a
             JOIN transparent_received_outputs tro ON tro.address_id = a.id
             JOIN transactions t ON t.id_tx = tro.transaction_id
             WHERE a.key_scope = ?1
               AND a.cached_transparent_receiver_address IS NOT NULL
               AND t.mined_height IS NOT NULL",
        )
        .map_err(|e| SyncError::db(format!("prepare used ephemeral query: {e}")))?;
    let rows = stmt
        .query_map([EPHEMERAL_KEY_SCOPE], |row| row.get::<_, String>(0))
        .map_err(|e| SyncError::db(format!("query used ephemeral addresses: {e}")))?;
    let mut used = HashSet::new();
    for row in rows {
        let encoded = row.map_err(|e| SyncError::db(format!("read ephemeral address: {e}")))?;
        let address = TransparentAddress::decode(&network, &encoded)
            .map_err(|e| SyncError::parse(format!("decode ephemeral address {encoded}: {e}")))?;
        used.insert(address);
    }
    Ok(used)
}
