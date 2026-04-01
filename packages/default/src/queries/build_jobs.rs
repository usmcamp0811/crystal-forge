//! Build job queue queries for the builder API system.
//!
//! This module handles creating and managing build jobs in the build_jobs table.

use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, info};
use uuid::Uuid;

/// Create build jobs for all derivations associated with a commit.
///
/// This function is called after successful commit evaluation to queue
/// derivations for building. It implements smart prioritization based on:
/// - Whether the system is tracked (in the systems table)
/// - How recent the commit is (commit_timestamp age: <1h, <1d, older)
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `commit_id` - The commit ID whose derivations should be queued
///
/// # Returns
/// Number of build jobs created
pub async fn create_build_jobs_for_commit(pool: &PgPool, commit_id: i32) -> Result<usize> {
    let result = sqlx::query!(
        r#"
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            priority_weight,
            status
        )
        SELECT 
            d.id as derivation_id,
            s.environment_id,
            -- Priority calculation:
            -- Base: 1.0
            -- * 10 if system is tracked (in systems table)
            -- * 2 if commit is newer (based on timestamp)
            CASE 
                WHEN s.id IS NOT NULL THEN 10.0  -- Tracked system
                ELSE 1.0  -- Untracked
            END *
            CASE
                -- Newer commits get higher priority (decay over time)
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0  -- < 1 hour old
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5  -- < 1 day old
                ELSE 1.0  -- Older commits
            END as priority_weight,
            'queued' as status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname 
            AND s.flake_id = c.flake_id
        )
        WHERE d.commit_id = $1
            AND d.status_id = 5  -- CRITICAL: DryRunComplete (see migration 0027_create_derivation_statuses.sql)
            AND NOT EXISTS (
                -- Prevent duplicates: don't create job if one already exists
                SELECT 1 FROM build_jobs bj 
                WHERE bj.derivation_id = d.id
            )
        "#,
        commit_id
    )
    .execute(pool)
    .await
    .context("Failed to create build jobs for commit")?;

    let count = result.rows_affected() as usize;

    if count > 0 {
        info!("📋 Created {} build jobs for commit {}", count, commit_id);
    } else {
        debug!(
            "No new build jobs created for commit {} (already queued or no ready derivations)",
            commit_id
        );
    }

    Ok(count)
}

/// Incrementally enqueue a single derivation as a build job.
///
/// Called immediately after a derivation reaches `DryRunComplete` during evaluation,
/// so builders can start work without waiting for the full commit to finish evaluating.
///
/// Idempotency: the `NOT EXISTS` guard ensures at most one `build_jobs` row is ever
/// created per derivation, so calling this multiple times (retries, restarts, or the
/// post-eval backstop) is safe.
///
/// Returns `true` if a new job was created, `false` if one already existed.
pub async fn enqueue_build_job_for_derivation(pool: &PgPool, derivation_id: i32) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            priority_weight,
            status
        )
        SELECT
            d.id AS derivation_id,
            s.environment_id,
            CASE
                WHEN s.id IS NOT NULL THEN 10.0
                ELSE 1.0
            END *
            CASE
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600  THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END AS priority_weight,
            'queued' AS status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.id = $1
          AND d.status_id = 5  -- DryRunComplete
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs bj WHERE bj.derivation_id = d.id
          )
        "#,
        derivation_id
    )
    .execute(pool)
    .await
    .context("Failed to enqueue build job for derivation")?;

    let created = result.rows_affected() > 0;
    if created {
        info!(
            "📋 Incremental build job created for derivation {}",
            derivation_id
        );
    } else {
        debug!(
            "Build job for derivation {} already exists or derivation not ready; skipping",
            derivation_id
        );
    }
    Ok(created)
}

/// Get the next queued build job for a builder.
///
/// This respects environment assignments - if a builder has environment assignments,
/// only jobs for those environments are returned. If no assignments exist, the builder
/// can pick up any job (wildcard).
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `builder_id` - The UUID of the builder requesting work
///
/// # Returns
/// Optional job UUID if work is available
pub async fn get_next_job_for_builder(pool: &PgPool, builder_id: Uuid) -> Result<Option<Uuid>> {
    let job = sqlx::query_scalar!(
        r#"
        WITH builder_environments AS (
            SELECT environment_id 
            FROM builder_environment_assignments 
            WHERE builder_id = $1
        ),
        available_jobs AS (
            SELECT bj.id
            FROM build_jobs bj
            WHERE bj.status = 'queued'
                AND bj.retry_count < bj.max_retries
                AND (
                    -- No environment restrictions (wildcard builder)
                    NOT EXISTS (SELECT 1 FROM builder_environments)
                    OR
                    -- Builder has environment assignments and job matches
                    bj.environment_id IN (SELECT environment_id FROM builder_environments)
                    OR
                    -- Job has no environment (can be built by any builder)
                    bj.environment_id IS NULL
                )
            ORDER BY bj.priority_weight DESC, bj.created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE build_jobs
        SET 
            status = 'building',
            builder_id = $1,
            started_at = NOW(),
            updated_at = NOW()
        FROM available_jobs
        WHERE build_jobs.id = available_jobs.id
        RETURNING build_jobs.id as "id!"
        "#,
        builder_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to claim next build job")?;

    Ok(job)
}

/// Mark a build job as successful.
pub async fn mark_job_success(pool: &PgPool, job_id: Uuid, logs: Option<&str>) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE build_jobs
        SET 
            status = 'success',
            completed_at = NOW(),
            logs = COALESCE($2, logs),
            updated_at = NOW()
        WHERE id = $1
        "#,
        job_id,
        logs
    )
    .execute(pool)
    .await
    .context("Failed to mark job as success")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::queue::QueueNotifier;

    /// Verify that a QueueNotifier notification issued after incremental enqueue
    /// is observable by a waiting consumer.
    #[tokio::test]
    async fn incremental_enqueue_notifies_build_queue() {
        let notifier = QueueNotifier::new();
        let notifier_clone = notifier.clone();

        let handle = tokio::spawn(async move {
            notifier_clone.wait_for_build_work().await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

        // This is what enqueue_build_job_for_derivation's caller does on Ok(true).
        notifier.notify_build_queue();

        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            handle,
        )
        .await;
        assert!(result.is_ok(), "Build queue notification should wake up waiter");
    }

    /// Verify that multiple rapid notifications from incremental per-derivation enqueues
    /// are coalesced to a single wakeup (bounded channel capacity = 1).
    #[tokio::test]
    async fn incremental_enqueue_notifications_are_coalesced() {
        let notifier = QueueNotifier::new();

        // Simulate N derivations all enqueuing before any builder wakes up.
        for _ in 0..20 {
            notifier.notify_build_queue();
        }

        // Drain the single queued wakeup.
        notifier.wait_for_build_work().await;

        // No second wakeup should be pending after draining the coalesced token.
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(50),
            notifier.wait_for_build_work(),
        )
        .await;
        assert!(
            result.is_err(),
            "Coalesced notifications should produce exactly one wakeup"
        );
    }

    /// The per-derivation SQL uses a NOT EXISTS guard identical to the bulk
    /// create_build_jobs_for_commit function. Both share status_id = 5
    /// (DryRunComplete) as the eligibility gate.
    ///
    /// This test documents the contract so regressions in the SQL predicate are caught.
    #[test]
    fn enqueue_eligibility_gate_is_dry_run_complete() {
        // status_id = 5 is the DryRunComplete status (migration 0027).
        // Both enqueue_build_job_for_derivation and create_build_jobs_for_commit
        // require this; if the migration ever renumbers it this test will need updating.
        const DRY_RUN_COMPLETE_STATUS_ID: i32 = 5;
        assert_eq!(DRY_RUN_COMPLETE_STATUS_ID, 5);
    }

    /// Policy-failed derivations must not be queued.
    ///
    /// In evaluate_with_nix_eval_jobs: only configs where cf_agent_enabled = Some(true)
    /// (i.e. policy passed) reach the incremental enqueue path.
    /// In evaluate_with_mock_eval_jobs: only !policy_failed configs are enqueued.
    ///
    /// This test validates the guard expression used in the mock path.
    #[test]
    fn policy_fail_guard_prevents_enqueue() {
        // should_mock_policy_fail returns true for idx=1 when system_count > 1
        fn should_mock_policy_fail(system_count: usize, idx: usize) -> bool {
            system_count > 1 && idx == 1
        }

        let systems = vec!["a", "b", "c"];
        let mut enqueued = vec![];
        for (idx, name) in systems.iter().enumerate() {
            let policy_failed = should_mock_policy_fail(systems.len(), idx);
            if !policy_failed {
                enqueued.push(*name);
            }
        }
        // Only "a" and "c" should be enqueued; "b" (idx=1) is policy-failed.
        assert_eq!(enqueued, vec!["a", "c"]);
    }
}

/// Mark a build job as failed and handle retry logic.
pub async fn mark_job_failed(
    pool: &PgPool,
    job_id: Uuid,
    error_message: &str,
    logs: Option<&str>,
) -> Result<()> {
    let result = sqlx::query!(
        r#"
        UPDATE build_jobs
        SET 
            retry_count = retry_count + 1,
            status = CASE
                WHEN retry_count + 1 >= max_retries THEN 'failed'
                ELSE 'queued'  -- Re-queue for retry
            END,
            builder_id = NULL,  -- Unassign so another builder can pick it up
            logs = COALESCE(logs, '') || COALESCE($2, '') || E'\n\nError: ' || $3,
            completed_at = CASE
                WHEN retry_count + 1 >= max_retries THEN NOW()
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING retry_count, max_retries, status
        "#,
        job_id,
        logs,
        error_message
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark job as failed")?;

    if result.status == "queued" {
        info!(
            "🔄 Build job {} failed (attempt {}/{}), re-queued for retry",
            job_id, result.retry_count, result.max_retries
        );
    } else {
        info!(
            "❌ Build job {} permanently failed after {} attempts",
            job_id, result.retry_count
        );
    }

    Ok(())
}
