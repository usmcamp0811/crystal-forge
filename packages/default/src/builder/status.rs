//! Worker status management for the builder module.
//!
//! This module provides utilities for tracking and updating worker status,
//! including task descriptions with commit context.

use crate::derivations::Derivation;
use crate::log::{WorkerState, WorkerStatus, get_build_status};
use sqlx::PgPool;

/// Resolved commit context for formatting task descriptions.
///
/// Separates data fetching from formatting so the pure formatting logic
/// is testable without a database connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommitContext {
    /// No commit associated with this derivation.
    None,
    /// Commit ID exists but lookup failed (e.g. DB error).
    Unresolved { commit_id: i32 },
    /// Commit resolved; distance from HEAD is optional.
    Resolved {
        short_hash: String,
        distance_from_head: Option<i32>,
    },
}

/// Format a task description from a derivation name and its commit context.
///
/// This is a pure function (no I/O) so it can be thoroughly unit tested.
///
/// # Examples
///
/// ```text
/// format_task_description("my-system", CommitContext::None)
///   → "my-system"
///
/// format_task_description("my-system", CommitContext::Resolved { short_hash: "abc123de", distance_from_head: Some(3) })
///   → "my-system @ abc123de (HEAD~3)"
/// ```
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

/// Resolve commit context for a derivation by querying the database.
///
/// Returns a [`CommitContext`] that can be passed to [`format_task_description`].
pub(crate) async fn resolve_commit_context(
    pool: &PgPool,
    derivation: &Derivation,
) -> CommitContext {
    let Some(commit_id) = derivation.commit_id else {
        return CommitContext::None;
    };

    let commit = match crate::queries::commits::get_commit_by_id(pool, commit_id).await {
        Ok(c) => c,
        Err(_) => return CommitContext::Unresolved { commit_id },
    };

    let short_hash = if commit.git_commit_hash.len() >= 8 {
        commit.git_commit_hash[..8].to_owned()
    } else {
        commit.git_commit_hash.clone()
    };

    let distance_from_head = match commit.get_flake(pool).await {
        Ok(flake) => crate::queries::commits::get_commit_distance_from_head(pool, &flake, &commit)
            .await
            .ok(),
        Err(_) => None,
    };

    CommitContext::Resolved {
        short_hash,
        distance_from_head,
    }
}

/// Build a task description for display/logging.
///
/// This is a thin async wrapper that resolves commit info from the database
/// and delegates to the pure [`format_task_description`] for formatting.
#[allow(dead_code)] // Currently not used, but kept for future use
pub(super) async fn build_task_description(pool: &PgPool, derivation: &Derivation) -> String {
    let ctx = resolve_commit_context(pool, derivation).await;
    format_task_description(&derivation.derivation_name, ctx)
}

/// Apply a worker status update to a slice of worker statuses.
///
/// This is a pure function (no I/O, no global state) for testability.
/// Returns `true` if a matching worker was found and updated, `false` otherwise.
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
            Some(std::time::Instant::now())
        };
        true
    } else {
        false
    }
}

/// Update worker status (helper to reduce boilerplate).
///
/// This is a non-blocking wrapper that spawns a task to update the global
/// worker status. The actual mutation logic lives in [`apply_worker_status_update`].
pub(super) fn update_worker_status(
    worker_id: usize,
    state: WorkerState,
    current_task: Option<String>,
) {
    tokio::spawn(async move {
        let mut statuses = get_build_status().write().await;
        apply_worker_status_update(&mut statuses, worker_id, state, current_task);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_task_description ──────────────────────────────────────────

    mod format_task_description_tests {
        use super::*;

        #[test]
        fn no_commit_returns_derivation_name_only() {
            let result = format_task_description("my-system", CommitContext::None);
            assert_eq!(result, "my-system");
        }

        #[test]
        fn no_commit_preserves_empty_name() {
            let result = format_task_description("", CommitContext::None);
            assert_eq!(result, "");
        }

        #[test]
        fn unresolved_commit_includes_commit_id() {
            let result =
                format_task_description("web-server", CommitContext::Unresolved { commit_id: 42 });
            assert_eq!(result, "web-server @ commit#42");
        }

        #[test]
        fn resolved_commit_without_distance_shows_hash_only() {
            let result = format_task_description(
                "db-primary",
                CommitContext::Resolved {
                    short_hash: "abc123de".into(),
                    distance_from_head: None,
                },
            );
            assert_eq!(result, "db-primary @ abc123de");
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
        fn resolved_commit_at_head_shows_zero_distance() {
            let result = format_task_description(
                "edge-node",
                CommitContext::Resolved {
                    short_hash: "deadbeef".into(),
                    distance_from_head: Some(0),
                },
            );
            assert_eq!(result, "edge-node @ deadbeef (HEAD~0)");
        }

        #[test]
        fn special_characters_in_name_preserved() {
            let result = format_task_description(
                "nixos-system-web.example.com",
                CommitContext::Resolved {
                    short_hash: "1a2b3c4d".into(),
                    distance_from_head: Some(1),
                },
            );
            assert_eq!(result, "nixos-system-web.example.com @ 1a2b3c4d (HEAD~1)");
        }

        #[test]
        fn short_hash_is_used_as_provided() {
            // The caller is responsible for truncating; format just uses what it gets.
            let result = format_task_description(
                "sys",
                CommitContext::Resolved {
                    short_hash: "ab".into(),
                    distance_from_head: None,
                },
            );
            assert_eq!(result, "sys @ ab");
        }
    }

    // ── CommitContext ────────────────────────────────────────────────────

    mod commit_context_tests {
        use super::*;

        #[test]
        fn commit_context_none_is_equal() {
            assert_eq!(CommitContext::None, CommitContext::None);
        }

        #[test]
        fn commit_context_unresolved_equality() {
            assert_eq!(
                CommitContext::Unresolved { commit_id: 1 },
                CommitContext::Unresolved { commit_id: 1 }
            );
            assert_ne!(
                CommitContext::Unresolved { commit_id: 1 },
                CommitContext::Unresolved { commit_id: 2 }
            );
        }

        #[test]
        fn commit_context_resolved_equality() {
            let a = CommitContext::Resolved {
                short_hash: "abcd1234".into(),
                distance_from_head: Some(0),
            };
            let b = CommitContext::Resolved {
                short_hash: "abcd1234".into(),
                distance_from_head: Some(0),
            };
            assert_eq!(a, b);
        }

        #[test]
        fn commit_context_variants_are_not_equal() {
            assert_ne!(
                CommitContext::None,
                CommitContext::Unresolved { commit_id: 1 }
            );
        }
    }

    // ── apply_worker_status_update ───────────────────────────────────────

    mod apply_worker_status_update_tests {
        use super::*;

        /// Helper: create a Vec<WorkerStatus> with the given worker IDs, all Idle.
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
            assert_eq!(workers[0].current_task.as_deref(), Some("building foo"));
            assert!(
                workers[0].started_at.is_some(),
                "started_at should be set when Working"
            );
        }

        #[test]
        fn working_to_idle_clears_task_and_started_at() {
            let mut workers = make_workers(&[0]);
            // First transition to Working
            apply_worker_status_update(
                &mut workers,
                0,
                WorkerState::Working,
                Some("building bar".into()),
            );
            assert!(workers[0].started_at.is_some());

            // Then back to Idle
            let updated = apply_worker_status_update(&mut workers, 0, WorkerState::Idle, None);

            assert!(updated);
            assert_eq!(workers[0].state, WorkerState::Idle);
            assert_eq!(workers[0].current_task, None);
            assert!(
                workers[0].started_at.is_none(),
                "started_at should be cleared when Idle"
            );
        }

        #[test]
        fn full_lifecycle_idle_working_idle() {
            let mut workers = make_workers(&[0]);

            // Start idle
            assert_eq!(workers[0].state, WorkerState::Idle);
            assert!(workers[0].started_at.is_none());

            // Transition to Working
            apply_worker_status_update(
                &mut workers,
                0,
                WorkerState::Working,
                Some("claiming work".into()),
            );
            assert_eq!(workers[0].state, WorkerState::Working);
            assert!(workers[0].started_at.is_some());

            // Update task while still Working
            apply_worker_status_update(
                &mut workers,
                0,
                WorkerState::Working,
                Some("building nixos-system".into()),
            );
            assert_eq!(workers[0].state, WorkerState::Working);
            assert_eq!(
                workers[0].current_task.as_deref(),
                Some("building nixos-system")
            );

            // Back to Idle
            apply_worker_status_update(&mut workers, 0, WorkerState::Idle, None);
            assert_eq!(workers[0].state, WorkerState::Idle);
            assert!(workers[0].started_at.is_none());
            assert_eq!(workers[0].current_task, None);
        }

        #[test]
        fn unknown_worker_id_returns_false_and_is_noop() {
            let mut workers = make_workers(&[0, 1, 2]);
            let original_states: Vec<_> = workers.iter().map(|w| w.state).collect();

            let updated = apply_worker_status_update(
                &mut workers,
                999,
                WorkerState::Working,
                Some("should not appear".into()),
            );

            assert!(!updated);
            // All workers unchanged
            for (i, w) in workers.iter().enumerate() {
                assert_eq!(w.state, original_states[i]);
                assert_eq!(w.current_task, None);
                assert!(w.started_at.is_none());
            }
        }

        #[test]
        fn empty_worker_list_returns_false() {
            let mut workers: Vec<WorkerStatus> = vec![];
            let updated = apply_worker_status_update(
                &mut workers,
                0,
                WorkerState::Working,
                Some("test".into()),
            );
            assert!(!updated);
        }

        #[test]
        fn updates_correct_worker_in_multi_worker_pool() {
            let mut workers = make_workers(&[0, 1, 2]);

            // Update worker 1 only
            apply_worker_status_update(
                &mut workers,
                1,
                WorkerState::Working,
                Some("building package-x".into()),
            );

            // Worker 0: unchanged
            assert_eq!(workers[0].state, WorkerState::Idle);
            assert_eq!(workers[0].current_task, None);

            // Worker 1: updated
            assert_eq!(workers[1].state, WorkerState::Working);
            assert_eq!(
                workers[1].current_task.as_deref(),
                Some("building package-x")
            );
            assert!(workers[1].started_at.is_some());

            // Worker 2: unchanged
            assert_eq!(workers[2].state, WorkerState::Idle);
            assert_eq!(workers[2].current_task, None);
        }

        #[test]
        fn multiple_workers_can_be_updated_independently() {
            let mut workers = make_workers(&[0, 1, 2]);

            apply_worker_status_update(
                &mut workers,
                0,
                WorkerState::Working,
                Some("task-a".into()),
            );
            apply_worker_status_update(
                &mut workers,
                2,
                WorkerState::Working,
                Some("task-c".into()),
            );

            assert_eq!(workers[0].state, WorkerState::Working);
            assert_eq!(workers[0].current_task.as_deref(), Some("task-a"));

            assert_eq!(workers[1].state, WorkerState::Idle);
            assert_eq!(workers[1].current_task, None);

            assert_eq!(workers[2].state, WorkerState::Working);
            assert_eq!(workers[2].current_task.as_deref(), Some("task-c"));
        }

        #[test]
        fn sleeping_state_sets_started_at() {
            let mut workers = make_workers(&[0]);
            apply_worker_status_update(
                &mut workers,
                0,
                WorkerState::Sleeping,
                Some("waiting for work".into()),
            );

            assert_eq!(workers[0].state, WorkerState::Sleeping);
            assert!(
                workers[0].started_at.is_some(),
                "started_at should be set for non-Idle states"
            );
        }

        #[test]
        fn working_with_none_task_is_valid() {
            let mut workers = make_workers(&[0]);
            let updated = apply_worker_status_update(&mut workers, 0, WorkerState::Working, None);

            assert!(updated);
            assert_eq!(workers[0].state, WorkerState::Working);
            assert_eq!(workers[0].current_task, None);
            assert!(workers[0].started_at.is_some());
        }
    }
}
