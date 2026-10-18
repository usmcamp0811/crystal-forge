//! Persists synthetic policy outcomes without production evaluation side effects.
//!
//! Only the mock evaluator calls this boundary. Commit ownership and cancellation
//! are checked under the same row lock as each outcome write. Outcomes carry an
//! attempt marker and explicit mock labels; they are not real Nix evidence.

use anyhow::Result;
use sqlx::{PgPool, Postgres, Transaction};

use super::{EvaluationCancelled, EvaluationSuperseded};
use crate::models::deployment_policies::{
    AssignedPolicy, PolicyCheckResult, policy_requirements_met, policy_results_json,
};

// CONCURRENCY: Check attempt ownership first, so a cancelled newer attempt never
// makes an old worker report cancellation on behalf of that newer attempt.
// Holding the commit lock through the write serializes retry and cancellation
// with publication. An error drops the transaction without any outcome writes.
async fn lock_active(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<uuid::Uuid> {
    let state: Option<(Option<String>, Option<i32>, Option<bool>)> = sqlx::query_as(
        "SELECT evaluation_status, evaluation_attempt_count, cancellation_requested
         FROM commits WHERE id = $1 FOR UPDATE",
    )
    .bind(commit_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((status, attempt, cancelled)) = state else {
        return Err(EvaluationSuperseded.into());
    };
    if attempt != Some(expected_attempt)
        || !matches!(status.as_deref(), Some("in_progress" | "cancelling"))
    {
        return Err(EvaluationSuperseded.into());
    }
    let attempt_id = sqlx::query_scalar(
        "SELECT id FROM evaluation_attempts WHERE commit_id = $1
         AND attempt_number = $2 AND status = 'in_progress' FOR UPDATE",
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_optional(&mut **tx)
    .await?;
    let attempt_id = attempt_id.ok_or_else(|| anyhow::Error::new(EvaluationSuperseded))?;
    if cancelled == Some(true) || status.as_deref() == Some("cancelling") {
        return Err(EvaluationCancelled.into());
    }
    Ok(attempt_id)
}

/// Checks mock attempt ownership before another simulated stage starts.
///
/// # Errors
/// Returns typed cancellation/supersession or database errors. Writes nothing.
pub(super) async fn verify_active(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    lock_active(&mut tx, commit_id, expected_attempt).await?;
    tx.rollback().await?;
    Ok(())
}

fn mock_results(
    expected_attempt: i32,
    check: &PolicyCheckResult,
    assigned: &[AssignedPolicy],
) -> serde_json::Value {
    let mut results = policy_results_json(check, assigned);
    results["evaluation_attempt"] = expected_attempt.into();
    results["execution_mode"] = "mock".into();
    // Keep UUID identity, blocking, strictness and enforcement from the shared
    // serializer. Prefix every detail, including unknown/non-Nix outcomes, so
    // no synthetic assertion is presented as evidence from an actual evaluator.
    if let Some(entries) = results["assigned"].as_object_mut() {
        for entry in entries.values_mut() {
            let detail = match entry["passed"].as_bool() {
                Some(passed) => format!(
                    "[MOCK] Deterministic policy outcome: {}",
                    if passed { "pass" } else { "fail" }
                ),
                None => "[MOCK] Policy outcome unavailable; no evaluator evidence".to_string(),
            };
            entry["details"] = detail.into();
        }
    }
    results["global"]["cfAgentEnabled"]["details"] =
        "[MOCK] Synthetic Crystal Forge agent gate".into();
    results
}

/// Writes one mock configuration's outcomes within its active commit attempt.
///
/// Callers MUST supply the captured assigned policy set used to create `check`.
/// Evidence is first-write immutable per exact attempt UUID/configuration. A
/// repeated call retains the first result, even if the supplied check differs.
/// Only the isolated mock table is written. No executable artifact identity,
/// option snapshot, observation, build, GC root, or assessment is published.
///
/// # Errors
/// Returns typed cancellation/supersession before writing, or database errors.
/// Any failed transaction leaves the configuration's previous outcomes intact.
pub(super) async fn persist_outcome(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    check: &PolicyCheckResult,
    assigned: &[AssignedPolicy],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let attempt_id = lock_active(&mut tx, commit_id, expected_attempt).await?;
    sqlx::query(
        "INSERT INTO evaluation_mock_outcomes
             (evaluation_attempt_id, commit_id, attempt_number, configuration_name,
              policy_results, cf_agent_enabled, policy_requirements_met)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (evaluation_attempt_id, configuration_name) DO NOTHING",
    )
    .bind(attempt_id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(&check.system_name)
    .bind(mock_results(expected_attempt, check, assigned))
    .bind(check.cf_agent_enabled)
    .bind(policy_requirements_met(check))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::deployment_policies::{AssignedPolicyEnforcementMode, DeploymentPolicy};

    fn assigned() -> Vec<AssignedPolicy> {
        (1..=50)
            .map(|id| AssignedPolicy {
                policy_id: uuid::Uuid::from_u128(id),
                policy_name: format!("mock-policy-{id}"),
                enforcement_mode: AssignedPolicyEnforcementMode::ReportOnly,
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            })
            .collect()
    }

    #[test]
    fn mock_metadata_preserves_uuid_outcomes_and_labels_all_details() {
        let assigned = assigned();
        let check = super::super::mock_policy_check("web-01", &assigned, false);
        let results = mock_results(7, &check, &assigned);
        assert_eq!(results["evaluation_attempt"], 7);
        assert_eq!(results["execution_mode"], "mock");
        assert!(
            check
                .assigned_results
                .values()
                .any(|r| r.passed == Some(false))
        );
        assert!(
            check
                .assigned_results
                .values()
                .any(|r| r.passed == Some(true))
        );
        for policy in &assigned {
            let entry = &results["assigned"][policy.policy_id.to_string()];
            assert_eq!(
                entry["passed"],
                check.assigned_results[&policy.policy_id].passed.unwrap()
            );
            assert_eq!(entry["blocking"], false);
            assert_eq!(entry["enforcement_mode"], "report_only");
            assert!(entry["details"].as_str().unwrap().starts_with("[MOCK]"));
        }
    }

    async fn active_commit(pool: &PgPool) -> i32 {
        let flake: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ($1, 'https://example.invalid/mock') RETURNING id",
        )
        .bind(format!("mock-persistence-{}", uuid::Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap();
        let commit = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp,
                 evaluation_status, evaluation_attempt_count, cancellation_requested)
             VALUES ($1, $2, NOW(), 'in_progress', 7, false) RETURNING id",
        )
        .bind(flake)
        .bind(uuid::Uuid::new_v4().to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        // Commit insertion creates the initial queued attempt via a trigger.
        sqlx::query(
            "UPDATE evaluation_attempts SET attempt_number = 7, status = 'in_progress'
                     WHERE commit_id = $1",
        )
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
        commit
    }

    async fn side_effect_counts(pool: &PgPool) -> (i64, i64, i64, i64, i64, i64) {
        sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM config_observation_requests),
                    (SELECT COUNT(*) FROM config_observations),
                    (SELECT COUNT(*) FROM build_jobs),
                    (SELECT COUNT(*) FROM composite_policy_assessments),
                    (SELECT COUNT(*) FROM derivations),
                    (SELECT COUNT(*) FROM view_buildable_derivations)",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn inventory(pool: &PgPool, commit: i32, names: &[&str]) {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        crate::queries::evaluation_inventory::persist_inventory(
            pool,
            commit,
            7,
            &names,
            &names,
            crate::queries::evaluation_inventory::InventoryExecutionMode::Mock,
        )
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn mock_persistence_records_attempt_outcomes_without_production_work(pool: PgPool) {
        let commit = active_commit(&pool).await;
        let assigned = assigned();
        let check = super::super::mock_policy_check("web-01", &assigned, false);
        inventory(&pool, commit, &["web-01"]).await;
        let before = side_effect_counts(&pool).await;
        persist_outcome(&pool, commit, 7, &check, &assigned)
            .await
            .unwrap();
        // Repeated writes cannot replace first-write evidence, even with a
        // different synthetic result for the same configuration and attempt.
        let changed = super::super::mock_policy_check("web-01", &assigned, true);
        persist_outcome(&pool, commit, 7, &changed, &assigned)
            .await
            .unwrap();
        let rows: Vec<(serde_json::Value, uuid::Uuid)> = sqlx::query_as(
            "SELECT m.policy_results, m.evaluation_attempt_id FROM evaluation_mock_outcomes m
             JOIN evaluation_attempts a ON a.id = m.evaluation_attempt_id
                 AND a.commit_id = m.commit_id AND a.attempt_number = m.attempt_number
             WHERE m.commit_id = $1",
        )
        .bind(commit)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, mock_results(7, &check, &assigned));
        assert!(sqlx::query("UPDATE evaluation_mock_outcomes SET policy_results = policy_results WHERE commit_id = $1")
            .bind(commit).execute(&pool).await.is_err());
        assert!(sqlx::query("INSERT INTO evaluation_mock_outcomes
                (evaluation_attempt_id, commit_id, attempt_number, configuration_name, policy_results, policy_requirements_met)
                VALUES ($1, $2, 8, 'wrong-owner', $3, TRUE)")
            .bind(rows[0].1).bind(commit).bind(mock_results(8, &check, &assigned))
            .execute(&pool).await.is_err());
        assert!(sqlx::query("INSERT INTO evaluation_mock_outcomes
                (evaluation_attempt_id, commit_id, attempt_number, configuration_name, policy_results, policy_requirements_met)
                VALUES ($1, $2, 7, 'wrong-marker', $3, TRUE)")
            .bind(rows[0].1).bind(commit).bind(mock_results(8, &check, &assigned))
            .execute(&pool).await.is_err());
        assert_eq!(side_effect_counts(&pool).await, before);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn mock_persistence_cancelled_or_superseded_attempts_write_nothing(pool: PgPool) {
        let commit = active_commit(&pool).await;
        let assigned = assigned();
        let check = super::super::mock_policy_check("web-01", &assigned, false);
        let before = side_effect_counts(&pool).await;
        for (status, attempt, cancelled, expected_cancel) in [
            ("in_progress", 7, true, true),
            ("cancelling", 7, false, true),
            ("in_progress", 8, false, false),
            ("cancelling", 8, true, false),
            ("complete", 7, false, false),
        ] {
            sqlx::query(
                "UPDATE commits SET evaluation_status = $2, evaluation_attempt_count = $3,
                         cancellation_requested = $4 WHERE id = $1",
            )
            .bind(commit)
            .bind(status)
            .bind(attempt)
            .bind(cancelled)
            .execute(&pool)
            .await
            .unwrap();
            let error = persist_outcome(&pool, commit, 7, &check, &assigned)
                .await
                .unwrap_err();
            assert_eq!(error.is::<EvaluationCancelled>(), expected_cancel);
            assert_eq!(error.is::<EvaluationSuperseded>(), !expected_cancel);
            let stage_error = verify_active(&pool, commit, 7).await.unwrap_err();
            assert_eq!(stage_error.is::<EvaluationCancelled>(), expected_cancel);
            assert_eq!(stage_error.is::<EvaluationSuperseded>(), !expected_cancel);
        }
        let missing = persist_outcome(&pool, -1, 7, &check, &assigned)
            .await
            .unwrap_err();
        assert!(missing.is::<EvaluationSuperseded>());
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM evaluation_mock_outcomes WHERE commit_id = $1",
        )
        .bind(commit)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
        assert_eq!(side_effect_counts(&pool).await, before);
    }

    async fn queue_counts(pool: &PgPool, commit: i32) -> (i64, i64, i64, bool) {
        let queue = crate::queries::commits::list_eval_queue(
            pool,
            &crate::api::models::EvalQueueParams {
                limit: 200,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let row = queue
            .rows
            .iter()
            .find(|row| row.commit_id == commit)
            .unwrap();
        (
            row.passed_count,
            row.policy_failed_count,
            row.eval_failed_count,
            row.policy_counts_available,
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn isolated_mock_counts_match_queue_history_and_exclude_queued_retry(pool: PgPool) {
        let commit = active_commit(&pool).await;
        // Markerless native carriers cannot establish current-attempt counts.
        sqlx::query(
            "INSERT INTO derivations (commit_id, derivation_name, derivation_type,
                     status_id, cf_agent_enabled, policy_requirements_met)
                     VALUES ($1, 'legacy-pass', 'nixos', 5, TRUE, TRUE),
                            ($1, 'legacy-error', 'nixos', 6, NULL, NULL)",
        )
        .bind(commit)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(queue_counts(&pool, commit).await, (0, 0, 0, false));
        inventory(&pool, commit, &["fail", "pass"]).await;
        persist_outcome(
            &pool,
            commit,
            7,
            &super::super::mock_policy_check("pass", &[], false),
            &[],
        )
        .await
        .unwrap();
        persist_outcome(
            &pool,
            commit,
            7,
            &super::super::mock_policy_check("fail", &[], true),
            &[],
        )
        .await
        .unwrap();
        assert_eq!(queue_counts(&pool, commit).await, (1, 1, 0, true));
        sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'complete' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let history = crate::queries::commits::list_eval_history(
            &pool,
            &crate::api::models::EvalHistoryParams {
                page: 1,
                limit: 200,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let row = history
            .items
            .iter()
            .find(|row| row.commit_id == commit)
            .unwrap();
        assert_eq!(
            (
                row.passed_count,
                row.policy_failed_count,
                row.eval_failed_count
            ),
            (1, 1, 0)
        );
        assert!(row.policy_counts_available);
        sqlx::query(
            "INSERT INTO evaluation_attempts (commit_id, attempt_number, status)
                     VALUES ($1, 8, 'queued')",
        )
        .bind(commit)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE commits SET evaluation_status = 'pending' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(queue_counts(&pool, commit).await, (0, 0, 0, false));
        // Starting a native retry cannot restore markerless legacy counters or
        // inherit the earlier attempt's isolated mock evidence.
        sqlx::query(
            "UPDATE evaluation_attempts SET status = 'in_progress'
                     WHERE commit_id = $1 AND attempt_number = 8",
        )
        .bind(commit)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'in_progress',
                     evaluation_attempt_count = 8 WHERE id = $1",
        )
        .bind(commit)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(queue_counts(&pool, commit).await, (0, 0, 0, false));
    }
}
