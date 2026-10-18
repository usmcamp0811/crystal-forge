//! Shared queue/history projection of represented-attempt policy counters.

// INVARIANT: Both callers select the latest attempt as `ea` before this lateral
// join. Native markers must be JSON integers equal to that attempt; isolated
// mock evidence must also match its UUID. The `inv` join distinguishes visible
// selected identities from those authorized for configuration-wide evidence.
// Partial shared ownership grants the intrinsic agent gate, not hidden hosts'
// requirement outcomes. Only an actual false agent gate can then prove failure.
// Two current carriers are ambiguous and remain unavailable. Missing or malformed
// evidence cannot turn a discovered configuration into a pass.
/// Joins selected, authorized exact-inventory counters after `ea` and `inv`.
pub(super) const JOIN: &str = r#"
LEFT JOIN LATERAL (
    WITH mock AS (
        SELECT m.configuration_name, m.cf_agent_enabled,
            CASE WHEN m.configuration_name=ANY(inv.evidence_configurations)
                 THEN m.policy_requirements_met ELSE NULL END AS policy_requirements_met
        FROM evaluation_mock_outcomes m
        WHERE m.commit_id = c.id AND m.attempt_number = ea.attempt_number
          AND m.evaluation_attempt_id = ea.id
          AND m.configuration_name = ANY(inv.agent_configurations)
    ), native AS (
        SELECT n.configuration_name AS derivation_name,
            CASE WHEN n.eval_state='eval_failed' THEN 6
                 WHEN n.eval_state='evaluated' THEN 5 ELSE 0 END AS status_id,
            n.cf_agent_enabled,
            CASE WHEN n.configuration_name=ANY(inv.evidence_configurations)
                 THEN n.policy_requirements_met ELSE NULL END AS policy_requirements_met,
            NULL::jsonb AS policy_results, n.eval_state='eval_failed' AS confirmed_failure,
            TRUE AS immutable
        FROM evaluation_native_outcomes n
        WHERE n.commit_id=c.id AND n.evaluation_attempt_id=ea.id
            AND n.attempt_number=ea.attempt_number
            AND n.configuration_name=ANY(inv.agent_configurations)
        UNION ALL
        SELECT d.derivation_name,d.status_id,d.cf_agent_enabled,
            CASE WHEN d.derivation_name=ANY(inv.evidence_configurations)
                 THEN d.policy_requirements_met ELSE NULL END,
            d.policy_results, COALESCE(
            d.policy_results->'evaluation_failure' = 'true'::jsonb
            AND jsonb_typeof(d.policy_results->'evaluation_failure_error') = 'string'
            AND btrim(d.policy_results->>'evaluation_failure_error') <> '', FALSE
        ) AS confirmed_failure, FALSE AS immutable
        FROM derivations d
        WHERE d.commit_id = c.id AND d.derivation_type = 'nixos'
          AND lower(COALESCE(d.policy_results->>'execution_mode','real')) <> 'mock'
          AND d.derivation_name = ANY(inv.agent_configurations)
          AND NOT EXISTS (SELECT 1 FROM evaluation_native_outcomes n
              WHERE n.commit_id=c.id AND n.evaluation_attempt_id=ea.id
                  AND n.attempt_number=ea.attempt_number
                  AND n.configuration_name=d.derivation_name)
    ), evidence AS (
        SELECT d.derivation_name AS configuration_name,
            CASE WHEN d.immutable AND d.status_id=0 THEN 'unknown'
            WHEN d.status_id = 6 OR d.confirmed_failure THEN 'eval_failed'
            WHEN d.cf_agent_enabled IS TRUE AND d.policy_requirements_met IS TRUE THEN 'pass'
            WHEN d.cf_agent_enabled IS FALSE OR d.policy_requirements_met IS FALSE THEN 'policy_failed'
            ELSE 'unknown' END AS outcome
        FROM native d
        WHERE d.immutable OR (d.policy_results->'evaluation_attempt' = to_jsonb(ea.attempt_number)
          AND (d.policy_results->>'evaluation_attempt') ~ '^[1-9][0-9]*$'
          AND (d.status_id IN (5, 6, 7, 8, 10, 12) OR d.confirmed_failure)
          AND (d.confirmed_failure OR NOT (
               d.policy_results ? 'evaluation_failure' OR d.policy_results ? 'evaluation_failure_error')))
        UNION ALL
        SELECT m.configuration_name,
            CASE WHEN m.cf_agent_enabled IS TRUE AND m.policy_requirements_met IS TRUE THEN 'pass'
                 WHEN m.cf_agent_enabled IS FALSE OR m.policy_requirements_met IS FALSE THEN 'policy_failed'
                 ELSE 'unknown' END
        FROM mock m
    ), configurations AS (
        SELECT configuration_name, COUNT(*) AS carriers, MIN(outcome) AS outcome
        FROM evidence GROUP BY configuration_name
    )
    SELECT COUNT(*) FILTER (WHERE carriers = 1 AND outcome = 'pass') AS passed_count,
        COUNT(*) FILTER (WHERE carriers = 1 AND outcome = 'policy_failed') AS policy_failed_count,
        COUNT(*) FILTER (WHERE carriers = 1 AND outcome = 'eval_failed') AS eval_failed_count,
        CASE WHEN inv.can_complete THEN cardinality(inv.policy_configurations)::bigint
            - COUNT(*) FILTER (WHERE carriers = 1 AND outcome IN ('pass', 'policy_failed', 'eval_failed'))
            - COUNT(*) FILTER (WHERE carriers <> 1 OR outcome='unknown')
            - cardinality(inv.missing_configurations) ELSE 0 END AS policy_pending_count,
        CASE WHEN inv.can_complete THEN COUNT(*) FILTER (WHERE carriers <> 1 OR outcome='unknown')
            + cardinality(inv.missing_configurations)
            ELSE cardinality(inv.policy_configurations)::bigint
                - COUNT(*) FILTER (WHERE carriers=1 AND outcome IN ('pass','policy_failed','eval_failed'))
            END AS policy_unavailable_count,
        cardinality(inv.excluded_configurations)::bigint AS unselected_count,
        inv.inventory_available
        AND NOT EXISTS (SELECT 1 FROM configurations WHERE carriers <> 1)
        AND NOT EXISTS (
            SELECT 1 FROM native d
            WHERE NOT d.immutable AND (jsonb_typeof(d.policy_results->'evaluation_attempt') IS DISTINCT FROM 'number'
                OR (d.policy_results->>'evaluation_attempt') !~ '^[1-9][0-9]*$')
              AND NOT EXISTS (SELECT 1 FROM mock m WHERE m.configuration_name = d.derivation_name)
        ) AS policy_counts_available
    FROM configurations
) pc ON TRUE
"#;

#[cfg(test)]
mod tests {
    use crate::api::models::{EvalHistoryParams, EvalQueueParams};
    use crate::queries::commits::{list_eval_history, list_eval_queue};
    use serde_json::{Value, json};
    use sqlx::PgPool;

    async fn assert_counts(
        pool: &PgPool,
        commit: i32,
        expected: (i64, i64, i64, bool),
        attempt: i32,
    ) {
        let queue = list_eval_queue(pool, &EvalQueueParams::default())
            .await
            .unwrap();
        let row = queue
            .rows
            .iter()
            .find(|row| row.commit_id == commit)
            .unwrap();
        assert_eq!(row.attempt_number, attempt);
        assert_eq!(
            (
                row.passed_count,
                row.policy_failed_count,
                row.eval_failed_count,
                row.policy_counts_available
            ),
            expected
        );
        let history = list_eval_history(pool, &EvalHistoryParams::default())
            .await
            .unwrap();
        if let Some(row) = history.items.iter().find(|row| row.commit_id == commit) {
            assert_eq!(row.attempt_number, attempt);
            assert_eq!(
                (
                    row.passed_count,
                    row.policy_failed_count,
                    row.eval_failed_count,
                    row.policy_counts_available
                ),
                expected
            );
        } else {
            let status: String =
                sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id=$1")
                    .bind(commit)
                    .fetch_one(pool)
                    .await
                    .unwrap();
            assert_eq!(status, "pending");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn native_ten_passes_then_queued_retry_has_zero_current_counts(pool: PgPool) {
        let commit = super::super::tests::derivation_count_commit(&pool).await;
        let names: Vec<String> = (1..=10).map(|n| format!("native-{n}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        super::super::inventory_summary_tests::inventory(&pool, commit, &names, &names).await;
        sqlx::query("INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,policy_results,cf_agent_enabled,policy_requirements_met) SELECT $1,'native-'||n,'nixos',5,'{\"evaluation_attempt\":7}'::jsonb,TRUE,TRUE FROM generate_series(1,10) n")
            .bind(commit).execute(&pool).await.unwrap();
        assert_counts(&pool, commit, (10, 0, 0, true), 7).await;
        sqlx::query("INSERT INTO evaluation_attempts(commit_id,attempt_number,status) VALUES($1,8,'queued')")
            .bind(commit).execute(&pool).await.unwrap();
        // Check both read paths with the queued retry represented, including a
        // terminal commit status lagging the new attempt, then the active queue.
        assert_counts(&pool, commit, (0, 0, 0, false), 8).await;
        sqlx::query("UPDATE commits SET evaluation_status='pending' WHERE id=$1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        assert_counts(&pool, commit, (0, 0, 0, false), 8).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn current_partial_native_and_mock_counts_exclude_stale_and_pending(pool: PgPool) {
        let commit = super::super::tests::derivation_count_commit(&pool).await;
        let names = [
            "pass",
            "policy",
            "nix",
            "pending",
            "stale-pass",
            "stale-policy",
            "stale-nix",
            "preserved-build",
            "mock",
        ];
        super::super::inventory_summary_tests::inventory(&pool, commit, &names, &names).await;
        sqlx::query("INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,policy_results,cf_agent_enabled,policy_requirements_met) VALUES
            ($1,'pass','nixos',5,'{\"evaluation_attempt\":7}',TRUE,TRUE),
            ($1,'policy','nixos',5,'{\"evaluation_attempt\":7}',TRUE,FALSE),
            ($1,'nix','nixos',6,'{\"evaluation_attempt\":7}',NULL,NULL),
            ($1,'pending','nixos',3,'{\"evaluation_attempt\":7}',TRUE,TRUE),
            ($1,'stale-pass','nixos',5,'{\"evaluation_attempt\":6}',TRUE,TRUE),
            ($1,'stale-policy','nixos',5,'{\"evaluation_attempt\":6}',TRUE,FALSE),
            ($1,'stale-nix','nixos',6,'{\"evaluation_attempt\":6}',NULL,NULL),
            ($1,'preserved-build','nixos',10,'{\"evaluation_attempt\":7,\"evaluation_failure\":true,\"evaluation_failure_error\":\"current failure\"}',TRUE,TRUE)")
            .bind(commit).execute(&pool).await.unwrap();
        assert_counts(&pool, commit, (1, 1, 2, true), 7).await;
        sqlx::query("INSERT INTO evaluation_mock_outcomes(evaluation_attempt_id,commit_id,attempt_number,configuration_name,policy_results,cf_agent_enabled,policy_requirements_met) SELECT id,commit_id,attempt_number,'mock','{\"evaluation_attempt\":7,\"execution_mode\":\"mock\"}',TRUE,TRUE FROM evaluation_attempts WHERE commit_id=$1")
            .bind(commit).execute(&pool).await.unwrap();
        assert_counts(&pool, commit, (2, 1, 2, true), 7).await;
        // Dual current carriers supply neither a pass nor a fabricated failure.
        sqlx::query("INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,policy_results,cf_agent_enabled,policy_requirements_met) VALUES($1,'mock','nixos',5,'{\"evaluation_attempt\":7}',TRUE,TRUE)")
            .bind(commit).execute(&pool).await.unwrap();
        assert_counts(&pool, commit, (1, 1, 2, false), 7).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn markerless_legacy_counts_are_unavailable_not_passing(pool: PgPool) {
        let commit = super::super::tests::derivation_count_commit(&pool).await;
        super::super::inventory_summary_tests::inventory(&pool, commit, &["legacy"], &["legacy"])
            .await;
        sqlx::query("INSERT INTO derivations(commit_id,derivation_name,derivation_type,status_id,cf_agent_enabled,policy_requirements_met) VALUES($1,'legacy','nixos',5,TRUE,TRUE)")
            .bind(commit).execute(&pool).await.unwrap();
        for marker in [Value::Null, json!("7"), json!(7.5), json!(7.0)] {
            sqlx::query("UPDATE derivations SET policy_results=$2 WHERE commit_id=$1")
                .bind(commit)
                .bind(json!({"evaluation_attempt":marker}))
                .execute(&pool)
                .await
                .unwrap();
            assert_counts(&pool, commit, (0, 0, 0, false), 7).await;
        }
        sqlx::query("UPDATE derivations SET policy_results='{}' WHERE commit_id=$1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        assert_counts(&pool, commit, (0, 0, 0, false), 7).await;
    }
}
