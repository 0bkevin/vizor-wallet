use super::{CheckingProgress, SyncEventKind, SyncProgressEvent, TAIL_REPAIR_MAX_START_PERCENTAGE};

/// Each attempt measures only its remaining scan work. Keep the completed
/// portion of this sync session when a transient failure starts another attempt.
#[derive(Default)]
pub(super) struct RetryProgress {
    attempt_base: f64,
    completed: f64,
    checking_cycle: u32,
    checking_completed: f64,
    checking_attempt_base: f64,
}

impl RetryProgress {
    /// Carry completed scan work forward, leaving room for unfinished repairs.
    pub(super) fn start_attempt(&mut self) {
        // A drained scan can still queue repairs and fail before completion.
        // Reuse the tail-repair allowance instead of pinning the retry at 100%.
        self.completed = if self.completed < 1.0 {
            self.completed
        } else {
            TAIL_REPAIR_MAX_START_PERCENTAGE
        };
        self.attempt_base = self.completed;
        if self.checking_completed >= 1.0 {
            self.checking_completed = TAIL_REPAIR_MAX_START_PERCENTAGE;
        }
        self.checking_attempt_base = self.checking_completed;
    }

    pub(super) fn checking_cycle(&self) -> u32 {
        self.checking_cycle
    }

    /// Map attempt-local progress and its download target into this session.
    pub(super) fn report(&mut self, mut event: SyncProgressEvent) -> SyncProgressEvent {
        if event.kind != SyncEventKind::Progress {
            event.checking = None;
        } else if let Some(checking) = event.checking.as_mut() {
            if checking.cycle != self.checking_cycle {
                self.checking_cycle = checking.cycle;
                self.checking_completed = 0.0;
                self.checking_attempt_base = 0.0;
            }
            let remaining = 1.0 - self.checking_attempt_base;
            checking.percentage =
                self.checking_attempt_base + remaining * checking.percentage.clamp(0.0, 1.0);
            checking.display_target_percentage = self.checking_attempt_base
                + remaining * checking.display_target_percentage.clamp(0.0, 1.0);
            self.checking_completed = self.checking_completed.max(checking.percentage);
        } else if self.checking_cycle > 0 {
            // Retry preparation precedes queue discovery. Keep the stage and
            // committed fraction, without interpolating the previous batch.
            event.checking = Some(CheckingProgress {
                cycle: self.checking_cycle,
                percentage: self.checking_completed,
                display_target_percentage: self.checking_completed,
            });
            event.display_target_blocks = 0;
        }
        let remaining = 1.0 - self.attempt_base;
        event.percentage = self.attempt_base + remaining * event.percentage.clamp(0.0, 1.0);
        event.display_target_percentage =
            self.attempt_base + remaining * event.display_target_percentage.clamp(0.0, 1.0);
        // A download target is an estimate, not completed work. Carry forward
        // only reported progress, including across multiple failed attempts.
        self.completed = self.completed.max(event.percentage);
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::sync_engine::preparation_progress_event;

    fn scan(percentage: f64, target: f64) -> SyncProgressEvent {
        SyncProgressEvent {
            percentage,
            display_target_percentage: target,
            display_target_blocks: 100,
            phase: "scan".into(),
            ..preparation_progress_event(3_500_000, "chain_prepare", 0, 0)
        }
    }

    fn checking(cycle: u32, percentage: f64, target: f64) -> SyncProgressEvent {
        SyncProgressEvent {
            checking: Some(CheckingProgress {
                cycle,
                percentage,
                display_target_percentage: target,
            }),
            ..scan(1.0, 1.0)
        }
    }

    #[test]
    fn checking_retries_keep_cycle_and_committed_work_without_carrying_estimates() {
        let mut progress = RetryProgress::default();
        progress.start_attempt();
        progress.report(scan(1.0, 1.0));
        let before = progress.report(checking(1, 0.25, 0.9)).checking.unwrap();
        assert_eq!(before.percentage, 0.25);
        progress.start_attempt();
        assert_eq!(progress.checking_cycle(), 1);
        let prep = progress.report(preparation_progress_event(100, "chain_prepare", 0, 0));
        assert_eq!(prep.checking.unwrap().percentage, 0.25);
        assert_eq!(prep.display_target_blocks, 0);
        let after = progress.report(checking(1, 0.5, 0.8)).checking.unwrap();
        assert_eq!(after.percentage, 0.625);
        assert!((after.display_target_percentage - 0.85).abs() < 1e-12);
        progress.start_attempt();
        let repeated = progress.report(checking(1, 0.5, 1.0)).checking.unwrap();
        assert_eq!(repeated.percentage, 0.8125);
        let next_cycle = progress.report(checking(2, 0.0, 0.25)).checking.unwrap();
        assert_eq!(next_cycle.cycle, 2);
        assert_eq!(next_cycle.percentage, 0.0);
        assert_eq!(next_cycle.display_target_percentage, 0.25);
        let done = progress.report(SyncProgressEvent {
            kind: SyncEventKind::Completed,
            is_syncing: false,
            ..scan(1.0, 1.0)
        });
        assert!(done.checking.is_none());
    }

    #[test]
    fn retry_after_checking_drains_still_has_room_for_unfinished_work() {
        let mut progress = RetryProgress::default();
        progress.report(checking(1, 1.0, 1.0));
        progress.start_attempt();
        let next = progress.report(checking(1, 0.5, 0.8)).checking.unwrap();
        assert!(next.percentage > 0.95 && next.percentage < 1.0);
        assert!(next.display_target_percentage > next.percentage);
    }

    #[test]
    fn retry_reports_newly_scanned_work_above_previous_progress() {
        let mut progress = RetryProgress::default();
        progress.start_attempt();
        let before = progress.report(scan(0.44, 0.45));
        assert_eq!(before.percentage, 0.44);
        progress.start_attempt();
        let preparation =
            progress.report(preparation_progress_event(3_500_000, "chain_prepare", 0, 0));
        assert_eq!(preparation.percentage, before.percentage);
        assert_eq!(preparation.kind, SyncEventKind::Progress);

        // 100 blocks out of the remaining 10,000 finish after the retry.
        // The UI must advance from 46.36%, not stay there until 44% of
        // the remaining work has been scanned.
        let after = progress.report(scan(0.01, 0.02));
        assert!((after.percentage - 0.4456).abs() < 1e-12);
        assert!((after.display_target_percentage - 0.4512).abs() < 1e-12);
        assert_eq!(after.display_target_blocks, 100);
        assert_eq!(after.kind, SyncEventKind::Progress);
    }

    #[test]
    fn repeated_retries_carry_completed_work_but_not_speculative_targets() {
        let mut progress = RetryProgress::default();
        progress.start_attempt();
        progress.report(scan(0.5, 0.9));
        progress.start_attempt();
        assert_eq!(progress.report(scan(0.2, 0.4)).percentage, 0.6);
        progress.start_attempt();
        assert_eq!(progress.report(scan(0.5, 0.5)).percentage, 0.8);
        // A fresh run has its own progress budget.
        let mut fresh = RetryProgress::default();
        fresh.start_attempt();
        assert_eq!(fresh.report(scan(0.1, 0.2)).percentage, 0.1);
    }

    #[test]
    fn retry_after_scan_drains_leaves_room_for_unfinished_repairs() {
        let mut progress = RetryProgress::default();
        progress.start_attempt();
        let drained = progress.report(scan(1.0, 1.0));
        assert_eq!(drained.percentage, 1.0);
        assert_eq!(drained.kind, SyncEventKind::Progress);

        // Repair scanning can fail after the original scan queue drained.
        progress.report(scan(0.95, 0.96));
        let mut previous_repair_percentage = TAIL_REPAIR_MAX_START_PERCENTAGE;
        for _ in 0..3 {
            progress.start_attempt();
            let preparation =
                progress.report(preparation_progress_event(3_500_000, "chain_prepare", 0, 0));
            assert_eq!(preparation.percentage, previous_repair_percentage);
            let repair = progress.report(scan(0.5, 0.8));
            assert!(preparation.percentage < repair.percentage);
            assert!(repair.percentage < repair.display_target_percentage);
            assert!(repair.display_target_percentage < 1.0);
            assert!(repair.is_syncing);
            assert_eq!(repair.kind, SyncEventKind::Progress);
            previous_repair_percentage = repair.percentage;
        }

        let done = progress.report(SyncProgressEvent {
            kind: SyncEventKind::Completed,
            is_syncing: false,
            ..scan(1.0, 1.0)
        });
        assert_eq!(done.percentage, 1.0);
        assert_eq!(done.display_target_percentage, 1.0);
        assert_eq!(done.kind, SyncEventKind::Completed);
        assert!(!done.is_syncing);
    }

    #[test]
    fn retry_preserves_completion_and_does_not_complete_on_scan_progress_alone() {
        let mut progress = RetryProgress::default();
        progress.report(scan(0.5, 0.5));
        progress.start_attempt();
        let scanned = progress.report(scan(1.0, 1.0));
        assert_eq!(scanned.percentage, 1.0);
        assert_eq!(scanned.kind, SyncEventKind::Progress);
        let done = progress.report(SyncProgressEvent {
            kind: SyncEventKind::Completed,
            is_syncing: false,
            ..scan(1.0, 1.0)
        });
        assert_eq!(done.percentage, 1.0);
        assert_eq!(done.kind, SyncEventKind::Completed);
        assert!(!done.is_syncing);
    }
}
