//! Worker status management for the builder module (builder-side, no DB).
//!
//! The DB-dependent status functions (resolve_commit_context, build_task_description)
//! remain in cf-server where sqlx is available.

use std::time::Instant;

/// Resolved commit context for formatting task descriptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommitContext {
    None,
    Unresolved { commit_id: i32 },
    Resolved {
        short_hash: String,
        distance_from_head: Option<i32>,
    },
}

/// Format a task description from a derivation name and its commit context.
pub(crate) fn format_task_description(derivation_name: &str, ctx: CommitContext) -> String {
    match ctx {
        CommitContext::None => derivation_name.to_owned(),
        CommitContext::Unresolved { commit_id } => {
            format!("{} @ commit#{}", derivation_name, commit_id)
        }
        CommitContext::Resolved {
            short_hash,
            distance_from_head,
        } => match distance_from_head {
            Some(distance) => {
                format!("{} @ {} (HEAD~{})", derivation_name, short_hash, distance)
            }
            None => format!("{} @ {}", derivation_name, short_hash),
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Worker status tracking (in-memory, no DB)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    Idle,
    Working,
    Sleeping,
}

#[derive(Debug, Clone)]
pub struct WorkerStatus {
    pub worker_id: usize,
    pub state: WorkerState,
    pub current_task: Option<String>,
    pub started_at: Option<Instant>,
}

/// Apply a worker status update to a slice of worker statuses.
pub(crate) fn apply_worker_status_update(
    statuses: &mut [WorkerStatus],
    worker_id: usize,
    state: WorkerState,
    current_task: Option<String>,
) -> bool {
    if let Some(status) = statuses.iter_mut().find(|s| s.worker_id == worker_id) {
        status.state = state;
        status.current_task = current_task;
        status.started_at = if state == WorkerState::Idle {
            None
        } else {
            Some(Instant::now())
        };
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_workers(ids: &[usize]) -> Vec<WorkerStatus> {
        ids.iter()
            .map(|&id| WorkerStatus {
                worker_id: id,
                current_task: None,
                started_at: None,
                state: WorkerState::Idle,
            })
            .collect()
    }

    #[test]
    fn no_commit_returns_derivation_name_only() {
        let result = format_task_description("my-system", CommitContext::None);
        assert_eq!(result, "my-system");
    }

    #[test]
    fn resolved_commit_with_distance_shows_head_notation() {
        let result = format_task_description(
            "db-primary",
            CommitContext::Resolved {
                short_hash: "abc123de".into(),
                distance_from_head: Some(5),
            },
        );
        assert_eq!(result, "db-primary @ abc123de (HEAD~5)");
    }

    #[test]
    fn idle_to_working_sets_state_and_task() {
        let mut workers = make_workers(&[0]);
        let updated = apply_worker_status_update(
            &mut workers,
            0,
            WorkerState::Working,
            Some("building foo".into()),
        );
        assert!(updated);
        assert_eq!(workers[0].state, WorkerState::Working);
        assert!(workers[0].started_at.is_some());
    }
}
