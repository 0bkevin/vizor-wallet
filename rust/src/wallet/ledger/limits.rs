//! Ledger Zcash app constraints shared by USB and Bluetooth signing.
//!
//! The supported 3.9.3 and 3.9.4 apps have the same record/review limits.
//! Actions are bounded **per pool**; reviewed shielded payments are bounded
//! **across both pools**, excluding change. These are independent budgets.
//! See `docs/ledger/limitations.md` for capabilities and enforcement points.
//!
//! Source: LedgerHQ/app-zcash 3.9.4, `1a0f6495458ecb77abf97c8cff25b0a1a344daaa`,
//! `src/consts.rs` and `src/parser/pczt.rs`. A version bump is not evidence
//! that these limits or the Orchard-to-Ironwood restriction have changed.

pub(crate) const MAX_TRANSPARENT_INPUTS: usize = 32;
pub(crate) const MAX_TRANSPARENT_OUTPUTS: usize = 10;
pub(crate) const MAX_SHIELDED_ACTIONS_PER_POOL: usize = 32;

pub(super) fn ensure_count(label: &str, count: usize, maximum: usize) -> Result<(), String> {
    if count > maximum {
        Err(format!(
            "ledger_capacity: Ledger supports at most {maximum} {label}; found {count}"
        ))
    } else {
        Ok(())
    }
}
