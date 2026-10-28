//! Refreshes the selected drawer from already-loaded queue/history snapshots.
//!
//! Summary windows can omit a selected commit because of filters or pagination.
//! Absence is not a terminal result. Keep the selection until matching evidence
//! arrives, and never combine one commit's identity with another commit's data.

use super::{EvalDrawerTarget, EvalHistoryItem, EvalQueueItem};

fn terminal(status: &str) -> bool {
    matches!(status, "complete" | "failed" | "cancelled")
}

/// Returns the newest available snapshot of the selected commit.
///
/// Attempt numbers never decrease. Within one attempt, terminal evidence takes
/// precedence over active evidence; terminal history takes precedence over queue
/// data because only history carries completion time. A newer retry can return a
/// history selection to the queue. Missing or older candidates preserve the
/// selection and cannot establish completion or supply a completion timestamp.
pub(super) fn refresh(
    selected: &EvalDrawerTarget,
    queue: &[EvalQueueItem],
    history: &[EvalHistoryItem],
) -> EvalDrawerTarget {
    let (commit_id, attempt, status) = match selected {
        EvalDrawerTarget::Queue(row) => (
            row.commit_id,
            row.attempt_number,
            row.evaluation_status.as_str(),
        ),
        EvalDrawerTarget::History(row) => (
            row.commit_id,
            row.attempt_number,
            row.evaluation_status.as_str(),
        ),
    };
    let queue = queue
        .iter()
        .filter(|row| row.commit_id == commit_id && row.attempt_number >= attempt)
        .max_by_key(|row| (row.attempt_number, terminal(&row.evaluation_status)));
    let history = history
        .iter()
        .filter(|row| row.commit_id == commit_id && row.attempt_number >= attempt)
        .max_by_key(|row| (row.attempt_number, terminal(&row.evaluation_status)));
    let newest = attempt
        .max(queue.map_or(attempt, |row| row.attempt_number))
        .max(history.map_or(attempt, |row| row.attempt_number));
    let queue = queue.filter(|row| row.attempt_number == newest);
    let history = history.filter(|row| row.attempt_number == newest);

    if let Some(row) = history.filter(|row| terminal(&row.evaluation_status)) {
        return EvalDrawerTarget::History(row.clone());
    }
    // Independent summary refreshes can arrive out of order. Retain known
    // terminal history rather than replace it with a stale same-attempt queue.
    if attempt == newest && terminal(status) && matches!(selected, EvalDrawerTarget::History(_)) {
        return selected.clone();
    }
    if let Some(row) = queue.filter(|row| terminal(&row.evaluation_status)) {
        return EvalDrawerTarget::Queue(row.clone());
    }
    if attempt == newest && terminal(status) {
        return selected.clone();
    }
    if let Some(row) = queue {
        return EvalDrawerTarget::Queue(row.clone());
    }
    if let Some(row) = history {
        return EvalDrawerTarget::History(row.clone());
    }
    selected.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn queue(commit_id: i32, attempt: i32, status: &str) -> EvalQueueItem {
        serde_json::from_value(json!({
            "commit_id": commit_id, "attempt_number": attempt,
            "evaluation_status": status, "flake_id": 1, "flake_name": "fixture",
            "branch": "main", "commit_hash": "fixture-sha",
            "committed_at": "2026-10-16T00:00:00Z",
            "enqueued_at": "2026-10-16T01:00:00Z", "queue_position": 1,
            "systems": ["fixture-host"], "system_count": 1,
            "passed_count": 0, "policy_failed_count": 0, "eval_failed_count": 0
        }))
        .expect("valid queue fixture")
    }

    fn history(commit_id: i32, attempt: i32, status: &str) -> EvalHistoryItem {
        let mut value = serde_json::to_value(queue(commit_id, attempt, status))
            .expect("serializable queue fixture");
        value["evaluation_completed_at"] = json!("2026-10-16T01:05:00Z");
        value["evaluation_duration_ms"] = json!(300_000);
        value["alert_occurrence_id"] = json!("fixture-alert");
        serde_json::from_value(value).expect("valid history fixture")
    }

    #[test]
    fn selection_is_isolated_by_exact_commit_id() {
        let selected = EvalDrawerTarget::Queue(queue(7, 1, "pending"));
        let unrelated_queue = [queue(8, 20, "complete")];
        let unrelated_history = [history(8, 21, "complete")];
        assert!(refresh(&selected, &unrelated_queue, &unrelated_history) == selected);
        let completed = EvalDrawerTarget::History(history(7, 1, "complete"));
        assert!(refresh(&completed, &unrelated_queue, &unrelated_history) == completed);
        let updated = queue(7, 1, "in_progress");
        let rows = [queue(8, 20, "complete"), updated.clone()];
        assert!(refresh(&selected, &rows, &[]) == EvalDrawerTarget::Queue(updated));
    }

    #[test]
    fn matching_active_queue_updates_same_attempt_metrics() {
        let selected = EvalDrawerTarget::Queue(queue(7, 2, "pending"));
        let mut updated = queue(7, 2, "in_progress");
        updated.passed_count = 4;
        updated.system_count = 5;
        assert!(refresh(&selected, &[updated.clone()], &[]) == EvalDrawerTarget::Queue(updated));
    }

    #[test]
    fn terminal_history_supplies_actual_completion_time() {
        let selected = EvalDrawerTarget::Queue(queue(7, 2, "in_progress"));
        let completed = history(7, 2, "complete");
        assert!(completed.evaluation_completed_at.is_some());
        for status in ["in_progress", "complete"] {
            let actual = refresh(&selected, &[queue(7, 2, status)], &[completed.clone()]);
            assert!(actual == EvalDrawerTarget::History(completed.clone()));
        }
    }

    #[test]
    fn terminal_queue_stops_live_without_inventing_history() {
        let selected = EvalDrawerTarget::Queue(queue(7, 2, "in_progress"));
        for status in ["complete", "failed", "cancelled"] {
            let completed = queue(7, 2, status);
            let old_history = [history(7, 1, "complete")];
            let actual = refresh(&selected, &[completed.clone()], &old_history);
            assert!(actual == EvalDrawerTarget::Queue(completed));
        }
    }

    #[test]
    fn newer_retry_replaces_completed_history_and_older_history_cannot_win() {
        let selected = EvalDrawerTarget::History(history(7, 2, "complete"));
        let retry = queue(7, 3, "pending");
        let old_history = [history(7, 2, "complete")];
        let actual = refresh(&selected, &[retry.clone()], &old_history);
        assert!(actual == EvalDrawerTarget::Queue(retry));
    }

    #[test]
    fn newest_matching_attempt_wins_independent_of_summary_order() {
        let selected = EvalDrawerTarget::Queue(queue(7, 1, "complete"));
        let retry = queue(7, 3, "in_progress");
        let rows = [retry.clone(), queue(7, 2, "complete")];
        let actual = refresh(&selected, &rows, &[history(7, 2, "complete")]);
        assert!(actual == EvalDrawerTarget::Queue(retry));
        let completed = history(7, 4, "complete");
        let actual = refresh(&selected, &rows, &[completed.clone()]);
        assert!(actual == EvalDrawerTarget::History(completed));
    }

    #[test]
    fn delayed_history_enriches_terminal_queue_and_then_stays_stable() {
        let selected = EvalDrawerTarget::Queue(queue(7, 2, "complete"));
        let completed = history(7, 2, "complete");
        let actual = refresh(&selected, &[], &[completed.clone()]);
        assert!(actual == EvalDrawerTarget::History(completed.clone()));
        assert!(refresh(&actual, &[queue(7, 2, "complete")], &[completed]) == actual);
    }

    #[test]
    fn older_attempts_and_same_attempt_active_data_cannot_regress_completion() {
        let selected = EvalDrawerTarget::History(history(7, 3, "complete"));
        for attempt in [2, 3] {
            let rows = [queue(7, attempt, "in_progress")];
            assert!(refresh(&selected, &rows, &[history(7, 2, "failed")]) == selected);
        }
        let selected = EvalDrawerTarget::Queue(queue(7, 3, "complete"));
        assert!(refresh(&selected, &[queue(7, 3, "in_progress")], &[]) == selected);
    }

    #[test]
    fn missing_or_only_older_rows_preserve_selection() {
        for selected in [
            EvalDrawerTarget::Queue(queue(7, 3, "in_progress")),
            EvalDrawerTarget::History(history(7, 3, "complete")),
        ] {
            assert!(refresh(&selected, &[], &[]) == selected);
            let old_queue = [queue(7, 2, "complete")];
            let old_history = [history(7, 2, "complete")];
            assert!(refresh(&selected, &old_queue, &old_history) == selected);
        }
    }
}
