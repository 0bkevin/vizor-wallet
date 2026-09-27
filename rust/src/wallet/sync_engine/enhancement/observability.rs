//! Test-wallet observability for transaction enhancement.
//!
//! Every line starts with `enhance[w=<tag>]`, where the tag is a short stable
//! hash of the wallet DB file name, so interleaved logs from several test
//! wallets can be separated with `grep`. Lines carry counts, phases,
//! latencies, and public PIR server metadata (generation, epoch, coverage,
//! anchor). They never carry txids, note positions, addresses, amounts, or raw
//! error text.
//!
//! Counters are process-global and reset when a checkpoint or payload-recovery
//! pass begins. Foreground sync is single-flight, so one pass owns them at a
//! time; status lookups from other callers (send, iOS FFI) can only inflate
//! the next summary.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::super::SyncError;

/// Logs at `$level` with the current wallet tag prefixed.
macro_rules! enhance_log {
    ($level:ident, $($arg:tt)*) => {
        log::$level!(
            "enhance[w={}] {}",
            $crate::wallet::sync_engine::enhancement::observability::tag(),
            format_args!($($arg)*)
        )
    };
}
pub(super) use enhance_log;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Http {
    requests: u32,
    errors: u32,
    bytes: u64,
    total: Duration,
    max: Duration,
}

impl Http {
    fn render(&self) -> String {
        if self.requests == 0 {
            return "0".into();
        }
        format!(
            "{}req/{}err/{}B avg={}ms max={}ms",
            self.requests,
            self.errors,
            self.bytes,
            (self.total / self.requests).as_millis(),
            self.max.as_millis(),
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum Lane {
    Enhance,
    Status,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Counters {
    pub(super) fee_backfill: u32,
    pub(super) fee_failed: u32,
    pub(super) fee_prevout_fetches: u32,

    pub(super) status_private: u32,
    pub(super) status_public: u32,
    pub(super) status_mined: u32,
    pub(super) status_mempool: u32,
    pub(super) status_forked: u32,
    pub(super) status_not_found: u32,
    pub(super) status_incomplete: u32,
    pub(super) status_failed: u32,
    pub(super) status_refreshes: u32,

    pub(super) history_ranges: u32,
    pub(super) history_acked: u32,
    pub(super) history_txs: u32,
    pub(super) history_failed: u32,

    pub(super) private_queries: u32,
    pub(super) private_applied: u32,
    pub(super) private_outside: u32,
    pub(super) private_rediscovered: u32,
    pub(super) private_stale_refreshes: u32,
    pub(super) private_deferred: u32,

    pub(super) public_requests: u32,
    pub(super) public_stored: u32,
    pub(super) public_not_found: u32,
    pub(super) public_failed: u32,

    enhance_http: Http,
    status_http: Http,
}

#[derive(Default)]
struct State {
    tag: String,
    counters: Counters,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut state = STATE.lock().unwrap_or_else(|poison| poison.into_inner());
    f(state.get_or_insert_with(State::default))
}

/// FNV-1a over the DB file name: stable across runs and Rust versions.
fn wallet_tag(db_path: &str) -> String {
    let name = Path::new(db_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| db_path.to_owned());
    let hash = name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    format!("{:06x}", hash & 0x00ff_ffff)
}

pub(super) fn tag() -> String {
    with_state(|state| {
        if state.tag.is_empty() {
            "-".into()
        } else {
            state.tag.clone()
        }
    })
}

/// Hostname only; paths and query strings are dropped.
pub(super) fn endpoint_host(url: &str) -> String {
    url.parse::<http::Uri>()
        .ok()
        .and_then(|uri| uri.host().map(str::to_owned))
        .unwrap_or_else(|| "<invalid>".into())
}

pub(super) fn record(update: impl FnOnce(&mut Counters)) {
    with_state(|state| update(&mut state.counters));
}

/// Records one PIR HTTP exchange. `bytes` is the response body size when it
/// was read.
pub(super) fn record_http(lane: Lane, started: Instant, bytes: Option<usize>, error: bool) {
    let elapsed = started.elapsed();
    with_state(|state| {
        let http = match lane {
            Lane::Enhance => &mut state.counters.enhance_http,
            Lane::Status => &mut state.counters.status_http,
        };
        http.requests += 1;
        http.errors += u32::from(error);
        http.bytes += bytes.unwrap_or(0) as u64;
        http.total += elapsed;
        http.max = http.max.max(elapsed);
    });
}

/// Binds the wallet tag and logs the immutable source decision for one sync.
pub(super) fn begin_session(
    db_path: &str,
    network: crate::wallet::network::WalletNetwork,
    private: bool,
    status_endpoint: &str,
    payload_endpoint: &str,
) {
    with_state(|state| state.tag = wallet_tag(db_path));
    let tor = if crate::network_privacy::is_tor_desired() {
        "tor"
    } else {
        "direct"
    };
    if private {
        enhance_log!(
            info,
            "session network={network:?} policy=private route={tor} status_pir={} enhance_pir={}",
            endpoint_host(status_endpoint),
            endpoint_host(payload_endpoint),
        );
    } else {
        enhance_log!(
            info,
            "session network={network:?} policy=public route={tor}"
        );
    }
}

pub(super) fn error_kind(error: &SyncError) -> &'static str {
    match error {
        SyncError::Continuity { .. } => "continuity",
        SyncError::Network(_) => "network",
        SyncError::PrivateStatusCoverageIncomplete => "private_status_coverage",
        SyncError::Db(_) => "db",
        SyncError::Parse(_) => "parse",
        SyncError::Other(_) => "other",
    }
}

/// One checkpoint or payload-recovery pass. Resets counters on begin and
/// logs a single summary on finish unless the pass did nothing.
pub(super) struct Pass {
    kind: &'static str,
    started: Instant,
}

impl Pass {
    pub(super) fn begin(kind: &'static str) -> Self {
        with_state(|state| state.counters = Counters::default());
        Self {
            kind,
            started: Instant::now(),
        }
    }

    pub(super) fn finish(self, result: &Result<bool, SyncError>, phase: &str) {
        let c = with_state(|state| std::mem::take(&mut state.counters));
        let outcome = match result {
            Ok(false) => "ok",
            Ok(true) => "exit",
            Err(error) => error_kind(error),
        };
        if c == Counters::default() && outcome == "ok" {
            return;
        }
        let phase = if phase.is_empty() { "idle" } else { phase };
        enhance_log!(
            info,
            "{} result={outcome} {}ms phase={phase} \
             fees{{backfill={} failed={} prevout_fetches={}}} \
             status{{private={} public={} mined={} mempool={} forked={} notfound={} incomplete={} failed={} refresh={}}} \
             history{{ranges={} acked={} txs={} failed={}}} \
             private{{queries={} applied={} outside={} rediscovered={} stale_refresh={} deferred={}}} \
             public{{requests={} stored={} notfound={} failed={}}} \
             http{{enhance={} status={}}}",
            self.kind,
            self.started.elapsed().as_millis(),
            c.fee_backfill,
            c.fee_failed,
            c.fee_prevout_fetches,
            c.status_private,
            c.status_public,
            c.status_mined,
            c.status_mempool,
            c.status_forked,
            c.status_not_found,
            c.status_incomplete,
            c.status_failed,
            c.status_refreshes,
            c.history_ranges,
            c.history_acked,
            c.history_txs,
            c.history_failed,
            c.private_queries,
            c.private_applied,
            c.private_outside,
            c.private_rediscovered,
            c.private_stale_refreshes,
            c.private_deferred,
            c.public_requests,
            c.public_stored,
            c.public_not_found,
            c.public_failed,
            c.enhance_http.render(),
            c.status_http.render(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_tag_is_stable_and_uses_only_the_file_name() {
        assert_eq!(
            wallet_tag("/a/b/zcash_wallet_main.db"),
            wallet_tag("/c/zcash_wallet_main.db")
        );
        assert_ne!(
            wallet_tag("/a/zcash_wallet_main.db"),
            wallet_tag("/a/zcash_wallet_main_2.db")
        );
        assert_eq!(wallet_tag("x").len(), 6);
    }

    #[test]
    fn endpoint_host_drops_path_and_query() {
        assert_eq!(
            endpoint_host("https://enhance-pir.valargroup.dev/v1/x?y=1"),
            "enhance-pir.valargroup.dev"
        );
        assert_eq!(endpoint_host("not a url"), "<invalid>");
    }
}
