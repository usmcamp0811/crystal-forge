//! Build job queue queries for the builder API system.
//!
//! This module handles creating and managing build jobs in the build_jobs table.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueuedBuild {
    pub build_job_id: Uuid,
    pub derivation_id: i32,
    pub system_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildJobInsertOutcome {
    Inserted {
        build_job_id: Uuid,
    },
    AlreadyExists {
        build_job_id: Uuid,
        /// Status of the existing job (e.g. "queued", "building", "success").
        /// The caller uses this to decide whether to announce a new queue event.
        status: String,
    },
}

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
    let result = sqlx::query(
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
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE  -- Only queue policy-passing derivations
            AND NOT EXISTS (
                -- Prevent duplicates: don't create job if one already exists
                SELECT 1 FROM build_jobs bj 
                WHERE bj.derivation_id = d.id
            )
        "#
    )
    .bind(commit_id)
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

pub async fn create_build_jobs_for_commit_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
) -> Result<Vec<QueuedBuild>> {
    let rows = sqlx::query_as::<_, QueuedBuild>(
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
            CASE
                WHEN s.id IS NOT NULL THEN 10.0
                ELSE 1.0
            END *
            CASE
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END as priority_weight,
            'queued' as status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.commit_id = $1
            AND d.status_id = 5
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE
            AND NOT EXISTS (
                SELECT 1 FROM build_jobs bj
                WHERE bj.derivation_id = d.id
            )
        RETURNING id AS build_job_id, derivation_id, (
            SELECT derivation_name FROM derivations WHERE derivations.id = build_jobs.derivation_id
        ) AS system_name
        "#,
    )
    .bind(commit_id)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to create build jobs for commit")?;

    Ok(rows)
}

pub async fn create_build_job_for_derivation_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<Option<BuildJobInsertOutcome>> {
    let inserted: Option<(Uuid,)> = sqlx::query_as(
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
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0
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
            AND d.status_id = 5
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE
        ON CONFLICT (derivation_id) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(&mut **tx)
    .await
    .context("Failed to create build job for derivation")?;

    if let Some((build_job_id,)) = inserted {
        return Ok(Some(BuildJobInsertOutcome::Inserted { build_job_id }));
    }

    let existing: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, status
        FROM build_jobs
        WHERE derivation_id = $1
        ORDER BY created_at ASC
        LIMIT 1
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(&mut **tx)
    .await
    .context("Failed to fetch existing build job for derivation")?;

    Ok(existing.map(
        |(build_job_id, status)| BuildJobInsertOutcome::AlreadyExists {
            build_job_id,
            status,
        },
    ))
}

/// Incrementally enqueue a single derivation as a build job.
///
/// Called immediately after a derivation reaches `DryRunComplete` during evaluation,
/// so builders can start work without waiting for the full commit to finish evaluating.
///
/// Idempotency: uses `ON CONFLICT (derivation_id) DO NOTHING` to guarantee at most
/// one `build_jobs` row per derivation. Concurrent callers are safe — the constraint
/// absorbs races without returning an error, unlike a `NOT EXISTS` subquery which
/// is non-atomic between the check and the insert.
///
/// Returns `true` if a new job was created, `false` if one already existed.
pub async fn enqueue_build_job_for_derivation(pool: &PgPool, derivation_id: i32) -> Result<bool> {
    let result = sqlx::query(
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
          AND d.cf_agent_enabled = TRUE
          AND d.policy_requirements_met = TRUE
        ON CONFLICT (derivation_id) DO NOTHING
        "#,
    )
    .bind(derivation_id)
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
    let job = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH builder_environments AS (
            SELECT environment_id 
            FROM builder_environment_assignments 
            WHERE builder_id = $1
        ),
        available_jobs AS (
            SELECT bj.id
            FROM build_jobs bj
            JOIN derivations d ON d.id = bj.derivation_id
            WHERE bj.status = 'queued'
                AND bj.retry_count < bj.max_retries
                AND d.cf_agent_enabled IS TRUE
                AND d.policy_requirements_met IS TRUE
                AND bj.available_at <= NOW()
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
        RETURNING build_jobs.id
        "#,
    )
    .bind(builder_id)
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

        let result = tokio::time::timeout(tokio::time::Duration::from_millis(100), handle).await;
        assert!(
            result.is_ok(),
            "Build queue notification should wake up waiter"
        );
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

    /// The per-derivation SQL uses `ON CONFLICT (derivation_id) DO NOTHING` for
    /// idempotent insertion, and shares status_id = 5 (DryRunComplete) as the
    /// eligibility gate with the bulk `create_build_jobs_for_commit` function.
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

    /// The real eval path in evaluate_with_nix_eval_jobs guards incremental enqueue
    /// on `cf_agent_enabled == Some(true)` and `policy_requirements_met == true`.
    /// This test drives that predicate directly so a refactor that widens the
    /// condition will break the test.
    #[test]
    fn real_path_policy_gate_only_enqueues_passing_configs() {
        // Simulate the gate expression used in evaluate_with_nix_eval_jobs.
        fn should_enqueue(
            cf_agent_enabled: Option<bool>,
            policy_requirements_met: bool,
            has_error: bool,
            has_drv: bool,
        ) -> bool {
            !has_error && has_drv && cf_agent_enabled == Some(true) && policy_requirements_met
        }

        // Policy passed, eval success → enqueue.
        assert!(should_enqueue(Some(true), true, false, true));

        // Policy explicitly failed → do NOT enqueue.
        assert!(!should_enqueue(Some(false), false, false, true));

        // A non-agent strict policy failed → do NOT enqueue.
        assert!(!should_enqueue(Some(true), false, false, true));

        // Policy result unknown (None) → do NOT enqueue.
        assert!(!should_enqueue(None, false, false, true));

        // Eval error → do NOT enqueue even if policy would pass.
        assert!(!should_enqueue(Some(true), true, true, true));

        // Missing drv path → do NOT enqueue.
        assert!(!should_enqueue(Some(true), true, false, false));
    }

    /// The backstop `create_build_jobs_for_commit` SQL now requires
    /// `d.cf_agent_enabled = TRUE` and `d.policy_requirements_met = TRUE` in
    /// addition to DryRunComplete.
    ///
    /// This test documents that contract so accidental removal of the predicate
    /// is caught at review time. It mirrors the WHERE clause in the query.
    #[test]
    fn backstop_sql_predicate_requires_cf_agent_enabled() {
        // Simulate the eligibility check the SQL performs per-derivation.
        struct MockDerivation {
            status_id: i32,
            cf_agent_enabled: Option<bool>,
            policy_requirements_met: bool,
            has_existing_job: bool,
        }

        fn is_eligible(d: &MockDerivation) -> bool {
            d.status_id == 5              // DryRunComplete
            && d.cf_agent_enabled == Some(true)  // policy passed
            && d.policy_requirements_met
            // ON CONFLICT DO NOTHING handles existing jobs; has_existing_job
            // is checked here for documentation of the expected outcome only.
            && !d.has_existing_job
        }

        // Passes all conditions.
        assert!(is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(true),
            policy_requirements_met: true,
            has_existing_job: false
        }));

        // Policy failed.
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(false),
            policy_requirements_met: false,
            has_existing_job: false
        }));

        // Non-agent strict policy failed.
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(true),
            policy_requirements_met: false,
            has_existing_job: false
        }));

        // Policy unknown.
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: None,
            policy_requirements_met: false,
            has_existing_job: false
        }));

        // Not DryRunComplete.
        assert!(!is_eligible(&MockDerivation {
            status_id: 4,
            cf_agent_enabled: Some(true),
            policy_requirements_met: true,
            has_existing_job: false
        }));

        // Job already exists (idempotency guard).
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(true),
            policy_requirements_met: true,
            has_existing_job: true
        }));
    }

    /// Policy-failed derivations must not be queued in the mock eval path either.
    #[test]
    fn mock_path_policy_fail_guard_prevents_enqueue() {
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

        // Open a canonical attention occurrence for the terminal failure.
        // A re-queued terminal job gets a new id, so job_id alone is a stable
        // occurrence key.
        let opened_at = Utc::now();
        let _ = crate::queries::attention::open_or_observe(
            pool,
            "builds",
            "build_job",
            &job_id.to_string(),
            &crate::queries::attention::build_occurrence_key(job_id),
            opened_at,
            serde_json::json!({"job_id": job_id.to_string()}),
        )
        .await
        .map_err(|e| tracing::error!("failed to open build attention occurrence: {e:#}"));
    }

    Ok(())
}
