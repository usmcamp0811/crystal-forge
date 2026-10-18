//! Retains native terminal evidence independently of mutable derivations.
//!
//! This sidecar supplies historical presentation only. Writers require exact
//! native discovery and current ownership; they never create runnable artifacts.

use anyhow::{Result, ensure};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::models::evaluate_with_policies::{EvaluationCancelled, EvaluationSuperseded};

/// Identifies the terminal native evaluation result, independent of build state.
#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeEvalState {
    /// Nix evaluation completed, even if a policy gate failed.
    Evaluated,
    /// Nix evaluation returned a confirmed configuration failure.
    EvalFailed,
}

impl NativeEvalState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Evaluated => "evaluated",
            Self::EvalFailed => "eval_failed",
        }
    }
}

/// Reports differing evidence for an already captured exact configuration.
#[derive(Debug)]
pub(crate) struct NativeOutcomeConflict;

impl std::fmt::Display for NativeOutcomeConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native evaluation outcome differs from its immutable first write")
    }
}

impl std::error::Error for NativeOutcomeConflict {}

/// Locks the latest active owner before producer-specific persistence locks.
///
/// Retains snapshot -> commit -> attempt locks until the caller ends `tx`.
///
/// # Errors
/// Returns [`EvaluationSuperseded`] for missing, stale, or inactive ownership,
/// [`EvaluationCancelled`] for cancellation of the current owner, or SQL errors.
pub(crate) async fn lock_native_owner_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<Uuid> {
    super::evaluation_snapshots::lock_snapshot_writer_tx(tx).await?;
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
    let id: Option<Uuid> = sqlx::query_scalar(
        "SELECT a.id FROM evaluation_attempts a
         WHERE a.commit_id = $1 AND a.attempt_number = $2 AND a.status = 'in_progress'
           AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
               WHERE newer.commit_id = a.commit_id AND newer.attempt_number > a.attempt_number)
         FOR UPDATE OF a",
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_optional(&mut **tx)
    .await?;
    let id = id.ok_or_else(|| anyhow::Error::new(EvaluationSuperseded))?;
    // Ownership precedes cancellation so stale workers cannot cancel a retry.
    if cancelled == Some(true) || status.as_deref() == Some("cancelling") {
        return Err(EvaluationCancelled.into());
    }
    Ok(id)
}

/// Captures immutable native evidence in the producer's existing transaction.
///
/// Requires the exact configuration in the owner's selected Native inventory.
/// Identical writes are idempotent only while ownership remains active. The
/// caller MUST roll back on error and MUST acquire owner locks before taking
/// producer-specific locks. No artifact, build, or observation rows are written.
///
/// # Errors
/// Returns ownership/cancellation errors from [`lock_native_owner_tx`], validation
/// errors for absent Native selection or mismatched JSON attempt markers,
/// [`NativeOutcomeConflict`] for differing evidence, and database errors. Never
/// overwrites an existing record. The caller owns commit and rollback.
///
/// # Examples
/// ```ignore
/// persist_native_outcome_tx(
///     &mut tx, commit_id, attempt, configuration_name,
///     NativeEvalState::Evaluated, Some(true), Some(false), &policy_results, None,
/// ).await?;
/// ```
pub(crate) async fn persist_native_outcome_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    expected_attempt: i32,
    configuration_name: &str,
    state: NativeEvalState,
    cf_agent_enabled: Option<bool>,
    policy_requirements_met: Option<bool>,
    policy_results: &Value,
    evaluation_failure_error: Option<&str>,
) -> Result<()> {
    let id = lock_native_owner_tx(tx, commit_id, expected_attempt).await?;
    let selected: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM evaluation_attempt_inventory
         WHERE evaluation_attempt_id = $1 AND commit_id = $2 AND attempt_number = $3
           AND execution_mode = 'native' AND $4 = ANY(selected_configuration_names))",
    )
    .bind(id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(configuration_name)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(
        selected,
        "native outcome requires exact selected Native inventory"
    );
    ensure!(
        policy_results.is_object()
            && policy_results
                .get("evaluation_attempt")
                .and_then(Value::as_i64)
                == Some(i64::from(expected_attempt)),
        "native outcome requires exact evaluation_attempt marker"
    );
    ensure!(
        !policy_results
            .get("execution_mode")
            .and_then(Value::as_str)
            .is_some_and(|mode| mode.eq_ignore_ascii_case("mock")),
        "native outcome cannot contain simulated result metadata"
    );
    sqlx::query(
        "INSERT INTO evaluation_native_outcomes (evaluation_attempt_id, commit_id,
             attempt_number, configuration_name, eval_state, cf_agent_enabled,
             policy_requirements_met, policy_results, evaluation_failure_error)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT (evaluation_attempt_id, configuration_name) DO NOTHING",
    )
    .bind(id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(configuration_name)
    .bind(state.as_str())
    .bind(cf_agent_enabled)
    .bind(policy_requirements_met)
    .bind(policy_results)
    .bind(evaluation_failure_error)
    .execute(&mut **tx)
    .await?;
    // Compare without UPDATE: even a no-op UPDATE violates immutability.
    let identical: bool = sqlx::query_scalar(
        "SELECT commit_id = $2 AND attempt_number = $3 AND eval_state = $5
             AND cf_agent_enabled IS NOT DISTINCT FROM $6
             AND policy_requirements_met IS NOT DISTINCT FROM $7
             AND policy_results = $8
             AND evaluation_failure_error IS NOT DISTINCT FROM $9
         FROM evaluation_native_outcomes WHERE evaluation_attempt_id = $1
             AND configuration_name = $4",
    )
    .bind(id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(configuration_name)
    .bind(state.as_str())
    .bind(cf_agent_enabled)
    .bind(policy_requirements_met)
    .bind(policy_results)
    .bind(evaluation_failure_error)
    .fetch_one(&mut **tx)
    .await?;
    if !identical {
        return Err(NativeOutcomeConflict.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::evaluation_inventory::{InventoryExecutionMode, persist_inventory};
    use serde_json::json;
    use sqlx::PgPool;

    async fn fixture(pool: &PgPool) -> (i32, Uuid) {
        let flake: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("native-{}", Uuid::new_v4()))
                .bind(format!("https://example.invalid/{}", Uuid::new_v4()))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp,
                 evaluation_status, evaluation_attempt_count, cancellation_requested)
             VALUES ($1, $2, NOW(), 'in_progress', 7, false) RETURNING id",
        )
        .bind(flake)
        .bind(Uuid::new_v4().to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        let id = sqlx::query_scalar(
            "UPDATE evaluation_attempts SET attempt_number = 7, status = 'in_progress',
                 started_at = NOW() WHERE commit_id = $1 RETURNING id",
        )
        .bind(commit)
        .fetch_one(pool)
        .await
        .unwrap();
        (commit, id)
    }

    async fn inventory(pool: &PgPool, commit: i32, attempt: i32, mode: InventoryExecutionMode) {
        persist_inventory(
            pool,
            commit,
            attempt,
            &[" Exact ".into(), "excluded".into()],
            &[" Exact ".into()],
            mode,
        )
        .await
        .unwrap();
    }

    async fn write(pool: &PgPool, commit: i32, attempt: i32, passed: bool) -> Result<()> {
        let mut tx = pool.begin().await?;
        let result = persist_native_outcome_tx(
            &mut tx,
            commit,
            attempt,
            " Exact ",
            NativeEvalState::Evaluated,
            Some(true),
            Some(passed),
            &json!({"evaluation_attempt": attempt, "assigned": {"policy": {"passed": passed}}}),
            None,
        )
        .await;
        match result {
            Ok(()) => {
                tx.commit().await?;
                Ok(())
            }
            Err(error) => {
                tx.rollback().await?;
                Err(error)
            }
        }
    }

    async fn evidence(pool: &PgPool, commit: i32) -> Vec<(Uuid, i32, String, Option<bool>, Value)> {
        sqlx::query_as(
            "SELECT evaluation_attempt_id, attempt_number, eval_state,
                 policy_requirements_met, policy_results FROM evaluation_native_outcomes
             WHERE commit_id = $1 ORDER BY attempt_number",
        )
        .bind(commit)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    // Replay only the shipped data statement. SQLx has already applied its DDL.
    async fn retain_legacy_native_history(pool: &PgPool) {
        let migration =
            include_str!("../../migrations/0308_retain_attributable_native_history.sql");
        let (_, insert) = migration.split_once("WITH marked AS").unwrap();
        let (insert, _) = insert.split_once("COMMENT ON TABLE").unwrap();
        sqlx::query(&format!("WITH marked AS{insert}"))
            .execute(pool)
            .await
            .unwrap();
    }

    async fn legacy_carrier(
        pool: &PgPool,
        commit: i32,
        configuration: &str,
        status: i32,
        policy_results: Value,
    ) -> i32 {
        sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name,
                 derivation_path, status_id, cf_agent_enabled, policy_requirements_met,
                 policy_results, error_message)
             VALUES ($1, 'nixos', $2, $3, $4, FALSE, TRUE, $5, 'legacy Nix failure')
             RETURNING id",
        )
        .bind(commit)
        .bind(configuration)
        .bind(format!("/nix/store/{}-legacy.drv", Uuid::new_v4().simple()))
        .bind(status)
        .bind(policy_results)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn retained_record(pool: &PgPool, owner: Uuid, configuration: &str) -> Value {
        sqlx::query_scalar(
            "SELECT to_jsonb(n) FROM evaluation_native_outcomes n
             WHERE evaluation_attempt_id = $1 AND configuration_name = $2",
        )
        .bind(owner)
        .bind(configuration)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_outcome_rejects_cancelled_superseded_and_nonactive_owners(pool: PgPool) {
        let (commit, _) = fixture(&pool).await;
        inventory(&pool, commit, 7, InventoryExecutionMode::Native).await;
        for (status, attempt, cancelled, cancellation_error) in [
            ("in_progress", 7, true, true),
            ("cancelling", 7, false, true),
            ("cancelling", 8, true, false),
            ("complete", 7, false, false),
            ("pending", 7, false, false),
        ] {
            sqlx::query("UPDATE commits SET evaluation_status = $2, evaluation_attempt_count = $3, cancellation_requested = $4 WHERE id = $1")
                .bind(commit).bind(status).bind(attempt).bind(cancelled).execute(&pool).await.unwrap();
            let error = write(&pool, commit, 7, true).await.unwrap_err();
            assert_eq!(error.is::<EvaluationCancelled>(), cancellation_error);
            assert_eq!(error.is::<EvaluationSuperseded>(), !cancellation_error);
        }
        sqlx::query("UPDATE commits SET evaluation_status = 'in_progress', cancellation_requested = false WHERE id = $1")
            .bind(commit).execute(&pool).await.unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            write(&pool, commit, 7, true)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert!(
            write(&pool, -1, 7, true)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert!(evidence(&pool, commit).await.is_empty());
        // Migration retention is retrospective, unlike the active writer. A
        // failed owner with exact Native discovery still owns its terminal fact.
        legacy_carrier(
            &pool,
            commit,
            " Exact ",
            5,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        retain_legacy_native_history(&pool).await;
        let retained = evidence(&pool, commit).await;
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].1, 7);
        assert_eq!(retained[0].2, "evaluated");
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_outcome_requires_exact_selected_native_inventory(pool: PgPool) {
        let (commit, _) = fixture(&pool).await;
        assert!(write(&pool, commit, 7, true).await.is_err());
        inventory(&pool, commit, 7, InventoryExecutionMode::Native).await;
        for (name, marker) in [
            ("Exact", 7),
            (" exact ", 7),
            ("excluded", 7),
            (" Exact ", 8),
        ] {
            let mut tx = pool.begin().await.unwrap();
            assert!(
                persist_native_outcome_tx(
                    &mut tx,
                    commit,
                    7,
                    name,
                    NativeEvalState::EvalFailed,
                    None,
                    None,
                    &json!({"evaluation_attempt": marker}),
                    Some("confirmed Nix failure"),
                )
                .await
                .is_err()
            );
            tx.rollback().await.unwrap();
        }
        let carrier = legacy_carrier(
            &pool,
            commit,
            " Exact ",
            5,
            json!({"evaluation_attempt": 7, "execution_mode": "MoCk"}),
        )
        .await;
        legacy_carrier(
            &pool,
            commit,
            "excluded",
            5,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        legacy_carrier(
            &pool,
            commit,
            "outside-inventory",
            5,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        retain_legacy_native_history(&pool).await;
        assert!(
            evidence(&pool, commit).await.is_empty(),
            "mock and unselected carriers are not native history"
        );
        for marker in [
            json!("7"),
            json!("07"),
            json!(7.0),
            Value::Null,
            json!(-7),
            json!({}),
            json!(true),
        ] {
            sqlx::query("UPDATE derivations SET policy_results = $2 WHERE id = $1")
                .bind(carrier)
                .bind(json!({"evaluation_attempt": marker}))
                .execute(&pool)
                .await
                .unwrap();
            retain_legacy_native_history(&pool).await;
            assert!(
                evidence(&pool, commit).await.is_empty(),
                "malformed marker must not be retained: {marker}"
            );
        }
        for malformed_failure in [
            json!({"evaluation_attempt": 7, "evaluation_failure": false}),
            json!({"evaluation_attempt": 7, "evaluation_failure": true}),
            json!({"evaluation_attempt": 7, "evaluation_failure_error": "diagnostic without confirmation"}),
            json!({"evaluation_attempt": 7, "evaluation_failure": true, "evaluation_failure_error": "  "}),
            json!({"evaluation_attempt": 7, "evaluation_failure": true, "evaluation_failure_error": 17}),
        ] {
            sqlx::query("UPDATE derivations SET policy_results = $2 WHERE id = $1")
                .bind(carrier)
                .bind(&malformed_failure)
                .execute(&pool)
                .await
                .unwrap();
            retain_legacy_native_history(&pool).await;
            assert!(
                evidence(&pool, commit).await.is_empty(),
                "malformed failure must not become a terminal fact: {malformed_failure}"
            );
        }
        // A new owner's synthetic discovery is not proof of native execution.
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, 8, 'in_progress')")
            .bind(commit).execute(&pool).await.unwrap();
        sqlx::query("UPDATE commits SET evaluation_attempt_count = 8 WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        inventory(&pool, commit, 8, InventoryExecutionMode::Mock).await;
        assert!(write(&pool, commit, 8, true).await.is_err());
        assert!(evidence(&pool, commit).await.is_empty());
        sqlx::query("UPDATE derivations SET policy_results = $2 WHERE id = $1")
            .bind(carrier)
            .bind(json!({"evaluation_attempt": 8}))
            .execute(&pool)
            .await
            .unwrap();
        retain_legacy_native_history(&pool).await;
        assert!(
            evidence(&pool, commit).await.is_empty(),
            "Mock inventory cannot attribute native history"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_outcome_first_write_schema_immutability_and_parent_cascade(pool: PgPool) {
        let (commit, id) = fixture(&pool).await;
        inventory(&pool, commit, 7, InventoryExecutionMode::Native).await;
        write(&pool, commit, 7, false).await.unwrap();
        let before = evidence(&pool, commit).await;
        let captured: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT captured_at FROM evaluation_native_outcomes WHERE evaluation_attempt_id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        write(&pool, commit, 7, false).await.unwrap();
        assert!(
            write(&pool, commit, 7, true)
                .await
                .unwrap_err()
                .is::<NativeOutcomeConflict>()
        );
        for query in [
            "UPDATE evaluation_native_outcomes SET policy_results = policy_results WHERE commit_id = $1",
            "DELETE FROM evaluation_native_outcomes WHERE commit_id = $1",
        ] {
            assert!(
                sqlx::query(query)
                    .bind(commit)
                    .execute(&pool)
                    .await
                    .is_err()
            );
        }
        for (owner, number, state, marker) in [
            (commit, 8, "evaluated", 8),
            (-1, 7, "evaluated", 7),
            (commit, 7, "building", 7),
            (commit, 7, "evaluated", 8),
        ] {
            assert!(sqlx::query("INSERT INTO evaluation_native_outcomes (evaluation_attempt_id, commit_id, attempt_number, configuration_name, eval_state, policy_results) VALUES ($1,$2,$3,'other',$4,$5)")
                .bind(id).bind(owner).bind(number).bind(state).bind(json!({"evaluation_attempt": marker})).execute(&pool).await.is_err());
        }
        assert_eq!(evidence(&pool, commit).await, before);
        assert_eq!(sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
            "SELECT captured_at FROM evaluation_native_outcomes WHERE evaluation_attempt_id = $1",
        ).bind(id).fetch_one(&pool).await.unwrap(), captured);
        let immutable_before = retained_record(&pool, id, " Exact ").await;
        legacy_carrier(
            &pool,
            commit,
            " Exact ",
            6,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        retain_legacy_native_history(&pool).await;
        assert_eq!(
            retained_record(&pool, id, " Exact ").await,
            immutable_before
        );
        // Explicit unavailable legacy ambiguity may carry no gate or diagnostic.
        // A second native carrier for one commit/configuration is forbidden by
        // the derivations uniqueness constraint; do not disable it to seed one.
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO evaluation_native_outcomes (evaluation_attempt_id, commit_id, attempt_number, configuration_name, eval_state, policy_results) VALUES ($1,$2,7,'unavailable','unavailable',$3)")
            .bind(id).bind(commit).bind(json!({"evaluation_attempt": 7}))
            .execute(&mut *tx).await.unwrap();
        let unavailable: (String, Option<bool>, Option<bool>, Option<String>) = sqlx::query_as(
            "SELECT eval_state, cf_agent_enabled, policy_requirements_met, evaluation_failure_error
             FROM evaluation_native_outcomes WHERE evaluation_attempt_id = $1 AND configuration_name = 'unavailable'",
        ).bind(id).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(unavailable, ("unavailable".into(), None, None, None));
        tx.rollback().await.unwrap();
        for (agent, policy, diagnostic) in [
            (Some(false), None, None),
            (None, Some(true), None),
            (None, None, Some("not permitted")),
        ] {
            assert!(sqlx::query("INSERT INTO evaluation_native_outcomes (evaluation_attempt_id, commit_id, attempt_number, configuration_name, eval_state, policy_results, cf_agent_enabled, policy_requirements_met, evaluation_failure_error) VALUES ($1,$2,7,'unavailable','unavailable',$3,$4,$5,$6)")
                .bind(id).bind(commit).bind(json!({"evaluation_attempt": 7}))
                .bind(agent).bind(policy).bind(diagnostic).execute(&pool).await.is_err());
        }
        assert!(sqlx::query("INSERT INTO evaluation_native_outcomes (evaluation_attempt_id, commit_id, attempt_number, configuration_name, eval_state, policy_results) VALUES ($1,$2,7,'mock-source','evaluated',$3)")
            .bind(id).bind(commit).bind(json!({"evaluation_attempt": 7, "execution_mode": "MoCk"}))
            .execute(&pool).await.is_err());
        sqlx::query("DELETE FROM evaluation_attempts WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(evidence(&pool, commit).await.is_empty());
        sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let (other, _) = fixture(&pool).await;
        inventory(&pool, other, 7, InventoryExecutionMode::Native).await;
        write(&pool, other, 7, true).await.unwrap();
        sqlx::query("DELETE FROM commits WHERE id = $1")
            .bind(other)
            .execute(&pool)
            .await
            .unwrap();
        assert!(evidence(&pool, other).await.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_outcome_sidecar_has_no_artifact_build_or_observation_side_effects(
        pool: PgPool,
    ) {
        let (commit, _) = fixture(&pool).await;
        inventory(&pool, commit, 7, InventoryExecutionMode::Native).await;
        let counts = "SELECT (SELECT COUNT(*) FROM derivations),
            (SELECT COUNT(*) FROM build_jobs), (SELECT COUNT(*) FROM config_observations),
            (SELECT COUNT(*) FROM config_observation_requests),
            (SELECT COUNT(*) FROM evaluation_snapshots)";
        let before: (i64, i64, i64, i64, i64) =
            sqlx::query_as(counts).fetch_one(&pool).await.unwrap();
        write(&pool, commit, 7, false).await.unwrap();
        assert_eq!(
            sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(counts)
                .fetch_one(&pool)
                .await
                .unwrap(),
            before
        );
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name::text FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'evaluation_native_outcomes'
             ORDER BY ordinal_position",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "evaluation_attempt_id",
                "commit_id",
                "attempt_number",
                "configuration_name",
                "cf_agent_enabled",
                "policy_requirements_met",
                "policy_results",
                "eval_state",
                "evaluation_failure_error",
                "captured_at"
            ]
        );
        legacy_carrier(
            &pool,
            commit,
            " Exact ",
            5,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        let before_migration: (i64, i64, i64, i64, i64) =
            sqlx::query_as(counts).fetch_one(&pool).await.unwrap();
        retain_legacy_native_history(&pool).await;
        assert_eq!(
            sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(counts)
                .fetch_one(&pool)
                .await
                .unwrap(),
            before_migration
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_outcome_retry_retains_historical_values_under_new_ea(pool: PgPool) {
        let (commit, old_id) = fixture(&pool).await;
        inventory(&pool, commit, 7, InventoryExecutionMode::Native).await;
        write(&pool, commit, 7, false).await.unwrap();
        let old = evidence(&pool, commit).await;
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let new_id: Uuid = sqlx::query_scalar("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, 8, 'queued') RETURNING id")
            .bind(commit).fetch_one(&pool).await.unwrap();
        // The commit's started counter still names seven while eight is queued.
        assert!(
            write(&pool, commit, 7, false)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        assert!(
            write(&pool, commit, 8, true)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE id = $1")
            .bind(old_id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE evaluation_attempts SET status = 'in_progress', started_at = NOW() WHERE id = $1")
            .bind(new_id).execute(&pool).await.unwrap();
        sqlx::query("UPDATE commits SET evaluation_attempt_count = 8 WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        inventory(&pool, commit, 8, InventoryExecutionMode::Native).await;
        write(&pool, commit, 8, true).await.unwrap();
        let retained = evidence(&pool, commit).await;
        assert_eq!(retained[0], old[0]);
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0].0, old_id);
        assert_eq!(retained[0].3, Some(false));
        assert_eq!(retained[1].0, new_id);
        assert_eq!(retained[1].3, Some(true));
        assert_ne!(old_id, new_id);
        assert!(
            write(&pool, commit, 7, false)
                .await
                .unwrap_err()
                .is::<EvaluationSuperseded>()
        );
        // Exercise pre-sidecar native carriers with the actual 0308 statement,
        // independently of the live writer assertions above.
        sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let (legacy_commit, legacy_owner) = fixture(&pool).await;
        let selected: Vec<String> = [
            "legacy",
            "ambiguous",
            "failed-status",
            "confirmed-failure",
            "retained-unavailable",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        persist_inventory(
            &pool,
            legacy_commit,
            7,
            &selected,
            &selected,
            InventoryExecutionMode::Native,
        )
        .await
        .unwrap();
        let original = json!({
            "evaluation_attempt": 7,
            "cfAgentEnabled": true,
            "assigned": {"legacy-policy": {"passed": false, "name": "historical policy"}},
        });
        let legacy_carrier_id =
            legacy_carrier(&pool, legacy_commit, "legacy", 10, original.clone()).await;
        let ambiguous_carrier_id = legacy_carrier(
            &pool,
            legacy_commit,
            "ambiguous",
            5,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        legacy_carrier(
            &pool,
            legacy_commit,
            "failed-status",
            6,
            json!({"evaluation_attempt": 7}),
        )
        .await;
        legacy_carrier(
            &pool,
            legacy_commit,
            "confirmed-failure",
            12,
            json!({"evaluation_attempt": 7, "evaluation_failure": true,
                "evaluation_failure_error": "confirmed historical failure"}),
        )
        .await;
        assert!(evidence(&pool, legacy_commit).await.is_empty());
        // A schema-valid retained ambiguity must remain unavailable even if
        // a second source later claims a passing outcome for the same attempt.
        sqlx::query("INSERT INTO evaluation_native_outcomes (evaluation_attempt_id,commit_id,attempt_number,configuration_name,eval_state,policy_results) VALUES ($1,$2,7,'retained-unavailable','unavailable',$3)")
            .bind(legacy_owner).bind(legacy_commit).bind(json!({"evaluation_attempt":7}))
            .execute(&pool).await.unwrap();
        let side_effect_counts = "SELECT (SELECT COUNT(*) FROM derivations),
            (SELECT COUNT(*) FROM build_jobs), (SELECT COUNT(*) FROM config_observations),
            (SELECT COUNT(*) FROM config_observation_requests),
            (SELECT COUNT(*) FROM evaluation_snapshots)";
        let before_retention: (i64, i64, i64, i64, i64) = sqlx::query_as(side_effect_counts)
            .fetch_one(&pool)
            .await
            .unwrap();
        retain_legacy_native_history(&pool).await;
        assert_eq!(
            sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(side_effect_counts)
                .fetch_one(&pool)
                .await
                .unwrap(),
            before_retention
        );
        let legacy = retained_record(&pool, legacy_owner, "legacy").await;
        assert_eq!(legacy["evaluation_attempt_id"], json!(legacy_owner));
        assert_eq!(legacy["commit_id"], legacy_commit);
        assert_eq!(legacy["attempt_number"], 7);
        assert_eq!(legacy["eval_state"], "evaluated");
        assert_eq!(legacy["cf_agent_enabled"], false);
        assert_eq!(legacy["policy_requirements_met"], true);
        assert_eq!(legacy["policy_results"], original);
        assert!(legacy["evaluation_failure_error"].is_null());
        assert!(legacy["captured_at"].is_string());
        let ambiguous = retained_record(&pool, legacy_owner, "ambiguous").await;
        for (configuration, diagnostic) in [
            ("failed-status", "legacy Nix failure"),
            ("confirmed-failure", "confirmed historical failure"),
        ] {
            let failure = retained_record(&pool, legacy_owner, configuration).await;
            assert_eq!(failure["eval_state"], "eval_failed");
            assert!(failure["cf_agent_enabled"].is_null());
            assert!(failure["policy_requirements_met"].is_null());
            assert_eq!(failure["evaluation_failure_error"], diagnostic);
        }
        for configuration in ["ambiguous", "retained-unavailable"] {
            sqlx::query("INSERT INTO evaluation_mock_outcomes (evaluation_attempt_id, commit_id, attempt_number, configuration_name, policy_results, cf_agent_enabled, policy_requirements_met) VALUES ($1,$2,7,$3,$4,TRUE,TRUE)")
                .bind(legacy_owner).bind(legacy_commit).bind(configuration)
                .bind(json!({"evaluation_attempt": 7, "execution_mode": "mock"}))
                .execute(&pool).await.unwrap();
        }
        let (before_retry, before_matrix) =
            crate::queries::eval_policy_plans::fetch_matrix_for_user_at_attempt(
                &pool,
                legacy_commit,
                None,
                legacy_owner,
            )
            .await
            .unwrap()
            .unwrap();
        let ambiguous_before = before_retry
            .iter()
            .find(|row| row.system_name == "ambiguous")
            .unwrap();
        // The compatibility envelope normalizes unavailable to evaluated, but
        // clears the attempt marker and both gates so it proves no outcome.
        assert_eq!(ambiguous_before.eval_status, "evaluated");
        assert_eq!(ambiguous_before.cf_agent_enabled, None);
        assert_eq!(ambiguous_before.policy_requirements_met, None);
        assert_eq!(ambiguous_before.policy_results, json!({}));
        let retained_unavailable = before_retry
            .iter()
            .find(|row| row.system_name == "retained-unavailable")
            .unwrap();
        assert_eq!(retained_unavailable.cf_agent_enabled, None);
        assert_eq!(retained_unavailable.policy_requirements_met, None);
        assert_eq!(retained_unavailable.policy_results, json!({}));
        let summary = before_matrix.configuration_summary.unwrap();
        assert_eq!(
            (
                summary.total,
                summary.passed,
                summary.failed,
                summary.evaluation_failed,
                summary.pending,
                summary.unavailable
            ),
            (5, 0, 1, 2, 0, 2)
        );

        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE id = $1")
            .bind(legacy_owner)
            .execute(&pool)
            .await
            .unwrap();
        let retry_owner: Uuid = sqlx::query_scalar("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1,8,'in_progress') RETURNING id")
            .bind(legacy_commit).fetch_one(&pool).await.unwrap();
        sqlx::query("UPDATE commits SET evaluation_attempt_count = 8 WHERE id = $1")
            .bind(legacy_commit)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE derivations SET policy_results = $2, cf_agent_enabled = TRUE, policy_requirements_met = FALSE WHERE id = ANY($1)")
            .bind(vec![legacy_carrier_id, ambiguous_carrier_id])
            .bind(json!({"evaluation_attempt": 8, "assigned": {"new-policy": {"passed": true}}}))
            .execute(&pool).await.unwrap();
        retain_legacy_native_history(&pool).await;
        assert_eq!(
            evidence(&pool, legacy_commit).await.len(),
            5,
            "retry has no Native inventory yet"
        );
        persist_inventory(
            &pool,
            legacy_commit,
            8,
            &selected,
            &selected,
            InventoryExecutionMode::Native,
        )
        .await
        .unwrap();
        retain_legacy_native_history(&pool).await;
        assert_eq!(retained_record(&pool, legacy_owner, "legacy").await, legacy);
        assert_eq!(
            retained_record(&pool, legacy_owner, "ambiguous").await,
            ambiguous
        );
        let retry = retained_record(&pool, retry_owner, "legacy").await;
        assert_eq!(retry["attempt_number"], 8);
        assert_eq!(retry["cf_agent_enabled"], true);
        assert_eq!(retry["policy_requirements_met"], false);
        assert_ne!(retry["policy_results"], legacy["policy_results"]);
        let (historical, _) = crate::queries::eval_policy_plans::fetch_matrix_for_user_at_attempt(
            &pool,
            legacy_commit,
            None,
            legacy_owner,
        )
        .await
        .unwrap()
        .unwrap();
        let historical_native = historical
            .iter()
            .find(|row| row.system_name == "legacy")
            .unwrap();
        assert_eq!(historical_native.eval_status, "evaluated");
        assert_eq!(historical_native.cf_agent_enabled, Some(false));
        assert_eq!(historical_native.policy_requirements_met, Some(true));
        assert_eq!(historical_native.policy_results, original);
        let historical_ambiguous = historical
            .iter()
            .find(|row| row.system_name == "ambiguous")
            .unwrap();
        assert_eq!(historical_ambiguous.eval_status, "evaluated");
        assert_eq!(historical_ambiguous.cf_agent_enabled, None);
        assert_eq!(historical_ambiguous.policy_requirements_met, None);
        assert_eq!(historical_ambiguous.policy_results, json!({}));
        let historical_unavailable = historical
            .iter()
            .find(|row| row.system_name == "retained-unavailable")
            .unwrap();
        assert_eq!(historical_unavailable.cf_agent_enabled, None);
        assert_eq!(historical_unavailable.policy_requirements_met, None);
        assert_eq!(historical_unavailable.policy_results, json!({}));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_producers_atomically_retain_success_and_confirmed_failure_across_retry(
        pool: PgPool,
    ) {
        use crate::models::deployment_policies::PolicyCheckResult;
        use crate::models::evaluate_with_policies::{
            ConfirmedSystemFailure, EvaluationFinalizeOutcome, EvaluationPlan,
            SuccessfulSystemResult, finalize_evaluation_attempt, persist_evaluated_system,
        };
        use std::collections::HashMap;

        let (commit, old_id) = fixture(&pool).await;
        inventory(&pool, commit, 7, InventoryExecutionMode::Native).await;
        let result = SuccessfulSystemResult {
            system_name: " Exact ".into(),
            derivation_target: "git+https://example.invalid/native#nixosConfigurations.\" Exact \""
                .into(),
            drv_path: format!("/nix/store/{}-native.drv", "a".repeat(32)),
            expected_store_path: None,
            cf_agent_enabled: Some(false),
            build_eligible: false,
        };
        let check = PolicyCheckResult::from_assigned(
            " Exact ".into(),
            &json!({"cfAgentEnabled": false}),
            &[],
        )
        .unwrap();
        persist_evaluated_system(&pool, commit, 7, &result, &check, &[])
            .await
            .unwrap();
        let old = evidence(&pool, commit).await;
        assert_eq!(old[0].0, old_id);
        assert_eq!(old[0].2, "evaluated");
        let changed = PolicyCheckResult::from_assigned(
            " Exact ".into(),
            &json!({"cfAgentEnabled": true}),
            &[],
        )
        .unwrap();
        assert!(
            persist_evaluated_system(&pool, commit, 7, &result, &changed, &[])
                .await
                .unwrap_err()
                .is::<NativeOutcomeConflict>()
        );
        let mutable_before: Value =
            sqlx::query_scalar("SELECT policy_results FROM derivations WHERE commit_id = $1")
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mutable_before, old[0].4);
        sqlx::query("UPDATE evaluation_attempts SET status = 'failed' WHERE id = $1")
            .bind(old_id)
            .execute(&pool)
            .await
            .unwrap();
        let new_id: Uuid = sqlx::query_scalar("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, 8, 'in_progress') RETURNING id")
            .bind(commit).fetch_one(&pool).await.unwrap();
        sqlx::query("UPDATE commits SET evaluation_attempt_count = 8 WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        inventory(&pool, commit, 8, InventoryExecutionMode::Native).await;
        let mut plan = EvaluationPlan {
            results: vec![],
            policy_checks: vec![],
            successful_systems: vec![],
            confirmed_failures: vec![ConfirmedSystemFailure {
                system_name: " Exact ".into(),
                derivation_target: result.derivation_target,
                error: "confirmed native module failure".into(),
            }],
            evaluation_snapshots: HashMap::new(),
            snapshot_capture_failures: HashMap::new(),
            flake_output_snapshot: None,
            had_system_eval_errors: true,
            force_build_job_insert_failure: true,
        };
        assert!(
            finalize_evaluation_attempt(&pool, commit, 8, &plan)
                .await
                .is_err()
        );
        assert_eq!(evidence(&pool, commit).await, old);
        assert_eq!(
            sqlx::query_scalar::<_, Value>(
                "SELECT policy_results FROM derivations WHERE commit_id = $1",
            )
            .bind(commit)
            .fetch_one(&pool)
            .await
            .unwrap(),
            mutable_before
        );
        plan.force_build_job_insert_failure = false;
        assert!(matches!(
            finalize_evaluation_attempt(&pool, commit, 8, &plan)
                .await
                .unwrap(),
            EvaluationFinalizeOutcome::Completed { .. }
        ));
        let retained = evidence(&pool, commit).await;
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0], old[0]);
        assert_eq!(retained[1].0, new_id);
        assert_eq!(retained[1].2, "eval_failed");
        assert_eq!(retained[1].3, None);
        assert_eq!(
            retained[1].4["evaluation_failure_error"],
            "confirmed native module failure"
        );
        let gates_and_diagnostic: (Option<bool>, Option<bool>, Option<String>) = sqlx::query_as(
            "SELECT cf_agent_enabled, policy_requirements_met, evaluation_failure_error
             FROM evaluation_native_outcomes WHERE evaluation_attempt_id = $1",
        )
        .bind(new_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            gates_and_diagnostic,
            (None, None, Some("confirmed native module failure".into()))
        );
        let mutable_after: Value =
            sqlx::query_scalar("SELECT policy_results FROM derivations WHERE commit_id = $1")
                .bind(commit)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mutable_after, retained[1].4);
        assert_ne!(mutable_after, old[0].4);
        let old_before_replay = retained_record(&pool, old_id, " Exact ").await;
        let new_before_replay = retained_record(&pool, new_id, " Exact ").await;
        retain_legacy_native_history(&pool).await;
        assert_eq!(
            retained_record(&pool, old_id, " Exact ").await,
            old_before_replay
        );
        assert_eq!(
            retained_record(&pool, new_id, " Exact ").await,
            new_before_replay
        );
    }
}
