//! Durable scheduling for targeted Config Inspector enrichment.
//!
//! This module owns only the database job substrate. It does not evaluate Nix,
//! inspect flakes, create snapshots, or advance either snapshot selector.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::models::evaluate_with_policies::SuccessfulSystemResult;

/// Describes the lifecycle state of one durable Config Inspector job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigInspectionJobStatus {
    /// The target is waiting for a future worker.
    Queued,
    /// A future worker has claimed the target.
    Running,
    /// The target completed successfully.
    Succeeded,
    /// The target completed with an error.
    Failed,
}

impl ConfigInspectionJobStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => bail!("unknown Config Inspector job status {other:?}"),
        }
    }
}

/// Represents a persisted Config Inspector target and its lifecycle audit data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigInspectionJob {
    /// Stable job identity.
    pub id: Uuid,
    /// Commit containing the exact configuration target.
    pub commit_id: i32,
    /// NixOS carrier derivation targeted by the job.
    pub derivation_id: i32,
    /// Exact NixOS configuration name.
    pub configuration_name: String,
    /// Exact carrier `.drv` path certified by finalization.
    pub carrier_drv_path: String,
    /// Durable lifecycle state.
    pub status: ConfigInspectionJobStatus,
    /// Number of worker attempts recorded for this job.
    pub attempts: i32,
    /// Redacted terminal failure, when the job failed.
    pub error: Option<String>,
    /// Time at which the job became eligible for a worker.
    pub scheduled_at: DateTime<Utc>,
    /// Time at which a worker started the job.
    pub started_at: Option<DateTime<Utc>>,
    /// Time at which the job reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last lifecycle update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Summarizes one set-based enqueue operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConfigInspectionEnqueueSummary {
    /// Number of exact successful targets supplied by the caller.
    pub requested_targets: usize,
    /// Number of new queued rows inserted by this call.
    pub inserted_jobs: usize,
}

/// Returns whether automatic Config Inspector scheduling is enabled for a mode.
pub(crate) fn should_schedule_config_inspections(execution_mode_is_mock: bool) -> bool {
    !execution_mode_is_mock
}

/// Enqueues exact successful NixOS systems for later Config Inspector work.
///
/// The function resolves every supplied `(configuration_name, carrier_drv_path)`
/// against the specified commit and locks the matching derivation rows for the
/// complete validation and insert transaction. A ready V2 artifact for the
/// same carrier suppresses new work. Active rows are deduplicated by the
/// database partial unique index; terminal history does not suppress a later
/// attempt.
///
/// This function performs database work only. It does not spawn processes,
/// evaluate Nix, access Git, mutate snapshots, or advance selectors.
///
/// # Errors
///
/// Returns an error when a supplied target does not exactly match a NixOS
/// derivation for `commit_id`, or when database access fails. If validation
/// fails, the transaction inserts no inspection jobs.
pub(crate) async fn enqueue_config_inspection_jobs_for_successful_systems(
    pool: &PgPool,
    commit_id: i32,
    successful_systems: &[SuccessfulSystemResult],
) -> Result<ConfigInspectionEnqueueSummary> {
    let mut targets = BTreeMap::new();
    for successful in successful_systems {
        if let Some(existing) = targets.get(&successful.system_name)
            && existing != &successful.drv_path
        {
            bail!(
                "multiple successful derivations supplied for configuration {:?}",
                successful.system_name
            );
        }
        targets.insert(successful.system_name.clone(), successful.drv_path.clone());
    }

    if targets.is_empty() {
        return Ok(ConfigInspectionEnqueueSummary {
            requested_targets: 0,
            inserted_jobs: 0,
        });
    }

    let configuration_names: Vec<String> = targets.keys().cloned().collect();
    let carrier_drv_paths: Vec<String> = targets.values().cloned().collect();

    let mut tx = pool
        .begin()
        .await
        .context("begin Config Inspector enqueue")?;
    let resolved: Vec<(i32, String, String)> = sqlx::query_as(
        r#"
        WITH supplied AS (
            SELECT *
            FROM unnest($1::text[], $2::text[])
                AS target(configuration_name, carrier_drv_path)
        )
        SELECT derivation.id, derivation.derivation_name, derivation.derivation_path
        FROM supplied
        JOIN derivations derivation
          ON derivation.commit_id = $3
         AND derivation.derivation_type = 'nixos'
         AND derivation.derivation_name = supplied.configuration_name
         AND derivation.derivation_path = supplied.carrier_drv_path
        ORDER BY derivation.derivation_name
        FOR SHARE OF derivation
        "#,
    )
    .bind(&configuration_names)
    .bind(&carrier_drv_paths)
    .bind(commit_id)
    .fetch_all(&mut *tx)
    .await
    .context("resolve and lock successful Config Inspector targets")?;
    if resolved.len() != targets.len() {
        bail!(
            "successful Config Inspector targets do not exactly match NixOS derivations for commit {commit_id}"
        );
    }

    let derivation_ids: Vec<i32> = resolved.iter().map(|(id, _, _)| *id).collect();
    let resolved_configuration_names: Vec<&str> =
        resolved.iter().map(|(_, name, _)| name.as_str()).collect();
    let resolved_carrier_drv_paths: Vec<&str> =
        resolved.iter().map(|(_, _, path)| path.as_str()).collect();

    let inserted: Vec<Uuid> = sqlx::query_scalar(
        r#"
        WITH supplied AS (
            SELECT *
            FROM unnest($1::integer[], $2::text[], $3::text[])
                AS target(derivation_id, configuration_name, carrier_drv_path)
        ),
        ready AS (
            SELECT supplied.configuration_name
            FROM supplied
            JOIN config_snapshot_selections selection
              ON selection.commit_id = $4
             AND selection.configuration_name = supplied.configuration_name
            JOIN evaluation_snapshots snapshot
              ON snapshot.id = selection.current_snapshot_id
             AND snapshot.commit_id = $4
             AND snapshot.configuration_name = supplied.configuration_name
            WHERE snapshot.schema_version = 2
              AND snapshot.integrity_version = 2
              AND snapshot.lifecycle = 'available'
              AND snapshot.comparison_ready = TRUE
              AND snapshot.carrier_drv_path = supplied.carrier_drv_path
        )
        INSERT INTO config_inspection_jobs (
            commit_id, derivation_id, configuration_name, carrier_drv_path,
            status
        )
        SELECT $4, supplied.derivation_id, supplied.configuration_name,
               supplied.carrier_drv_path, 'queued'
        FROM supplied
        WHERE NOT EXISTS (
                  SELECT 1
                  FROM ready
                  WHERE ready.configuration_name = supplied.configuration_name
              )
        ON CONFLICT (commit_id, configuration_name)
            WHERE status IN ('queued', 'running')
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(&derivation_ids)
    .bind(&resolved_configuration_names)
    .bind(&resolved_carrier_drv_paths)
    .bind(commit_id)
    .fetch_all(&mut *tx)
    .await
    .context("enqueue Config Inspector jobs")?;

    tx.commit()
        .await
        .context("commit Config Inspector enqueue")?;

    Ok(ConfigInspectionEnqueueSummary {
        requested_targets: resolved.len(),
        inserted_jobs: inserted.len(),
    })
}

/// Loads a durable job by ID for the future worker and reconciliation paths.
pub(crate) async fn get_config_inspection_job(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ConfigInspectionJob>> {
    let row = sqlx::query(
        r#"
        SELECT id, commit_id, derivation_id, configuration_name,
               carrier_drv_path, status, attempts, error, scheduled_at,
               started_at, completed_at, created_at, updated_at
        FROM config_inspection_jobs
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("load Config Inspector job")?;
    row.map(|row| {
        Ok(ConfigInspectionJob {
            id: row.try_get("id")?,
            commit_id: row.try_get("commit_id")?,
            derivation_id: row.try_get("derivation_id")?,
            configuration_name: row.try_get("configuration_name")?,
            carrier_drv_path: row.try_get("carrier_drv_path")?,
            status: ConfigInspectionJobStatus::parse(row.try_get::<String, _>("status")?.as_str())?,
            attempts: row.try_get("attempts")?,
            error: row.try_get("error")?,
            scheduled_at: row.try_get("scheduled_at")?,
            started_at: row.try_get("started_at")?,
            completed_at: row.try_get("completed_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    })
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn fixture(pool: &PgPool, name: &str) -> (i32, i32, String) {
        let suffix = Uuid::new_v4().simple().to_string();
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("config-inspection-{suffix}"))
        .bind(format!(
            "https://example.test/config-inspection-{suffix}.git"
        ))
        .fetch_one(pool)
        .await
        .expect("inspection fixture flake should persist");
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("{suffix:0>40}"))
        .fetch_one(pool)
        .await
        .expect("inspection fixture commit should persist");
        let drv_path = format!("/nix/store/{suffix}-{name}.drv");
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count) VALUES ($1, 'nixos', $2, $3, (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1), 0) RETURNING id",
        )
        .bind(commit_id)
        .bind(name)
        .bind(&drv_path)
        .fetch_one(pool)
        .await
        .expect("inspection fixture derivation should persist");
        (commit_id, derivation_id, drv_path)
    }

    fn successful_system(
        _derivation_id: i32,
        name: &str,
        drv_path: &str,
    ) -> SuccessfulSystemResult {
        SuccessfulSystemResult {
            system_name: name.to_string(),
            derivation_target: format!("test://nixosConfigurations.{name}"),
            drv_path: drv_path.to_string(),
            expected_store_path: None,
            cf_agent_enabled: Some(true),
            build_eligible: true,
        }
    }

    async fn job_count(pool: &PgPool, commit_id: i32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM config_inspection_jobs WHERE commit_id = $1")
            .bind(commit_id)
            .fetch_one(pool)
            .await
            .expect("inspection job count should load")
    }

    async fn add_snapshot_selector(
        pool: &PgPool,
        commit_id: i32,
        configuration_name: &str,
        snapshot_id: Uuid,
    ) {
        sqlx::query(
            "INSERT INTO config_snapshot_selections (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
        )
        .bind(commit_id)
        .bind(configuration_name)
        .bind(snapshot_id)
        .execute(pool)
               .await
        .expect("snapshot selector should persist");
    }

    async fn insert_v2_snapshot(
        pool: &PgPool,
        commit_id: i32,
        configuration_name: &str,
        carrier_drv_path: &str,
        comparison_ready: bool,
    ) {
        let snapshot_id = Uuid::new_v4();
        let digest = vec![7_u8; 32];
        let payload = json!({
            "metadata": {
                "state": "available",
                "option_type": "string",
                "loc": [],
                "declared_type": "str",
                "declarations": [],
                "declaration_positions": [],
                "highest_prio": 100,
                "is_defined": true,
                "surviving_definition_sources": [{
                    "source_path": "modules/0.nix",
                    "priority": 100,
                    "source_input": null,
                    "source_revision": null
                }]
            },
            "effective_value": {"kind": "scalar", "value": "safe"},
            "provenance": {
                "state": "available",
                "definitions": [{
                    "ordinal": 0,
                    "source_path": "modules/0.nix",
                    "source_input": null,
                    "source_revision": null,
                    "module_key": null,
                    "priority": 100,
                    "status": "active_surviving",
                    "surviving_merge_order": 0,
                    "value": if comparison_ready {
                        json!({"kind": "scalar", "value": "safe"})
                    } else {
                        json!(null)
                    }
                }],
                "override_state": false
            }
        });
        let provenance_state = if comparison_ready {
            json!({
                "state": "available",
                "adapter_version": 1,
                "target_lib_version": null,
                "target_module_system_path": null,
                "provenance_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "definition_value_enrichment": {
                    "state": "available",
                    "adapter_version": 1,
                    "provenance_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                }
            })
        } else {
            json!({
                "state": "available",
                "adapter_version": 1,
                "target_lib_version": null,
                "target_module_system_path": null,
                "provenance_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "definition_value_enrichment": {
                    "state": "unavailable",
                    "reason_code": "not_evaluated",
                    "diagnostic": null
                }
            })
        };
        sqlx::query(
            "INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, 2, $2, 'test')",
        )
        .bind(&digest)
        .bind(payload)
        .execute(pool)
        .await
        .expect("V2 option content should persist");
        sqlx::query(
            "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes, target_key, source_out_path, carrier_drv_path, provenance_state, comparison_ready) VALUES ($1, $2, $3, 2, 'available', 1, 1, 1, $4, $5, $6, $7, $8)",
        )
        .bind(snapshot_id)
        .bind(commit_id)
        .bind(configuration_name)
        .bind("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .bind("/nix/store/config-out")
        .bind(carrier_drv_path)
        .bind(provenance_state)
        .bind(comparison_ready)
        .execute(pool)
        .await
        .expect("V2 snapshot should persist");
        sqlx::query(
            "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'services.test.value', $2, false, $3, ARRAY['services', 'test', 'value'])",
        )
        .bind(snapshot_id)
        .bind(&digest)
        .bind("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        .execute(pool)
        .await
        .expect("V2 option reference should persist");
        sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1")
            .bind(snapshot_id)
            .execute(pool)
            .await
            .expect("V2 snapshot should certify");
        sqlx::query(
            "INSERT INTO config_snapshot_selections (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
        )
        .bind(commit_id)
        .bind(configuration_name)
        .bind(snapshot_id)
        .execute(pool)
        .await
        .expect("V2 selector should persist");
    }

    #[test]
    fn execution_mode_gate_only_allows_real_mode() {
        assert!(should_schedule_config_inspections(false));
        assert!(!should_schedule_config_inspections(true));
    }

    #[test]
    fn status_values_match_migration_contract() {
        for status in [
            ConfigInspectionJobStatus::Queued,
            ConfigInspectionJobStatus::Running,
            ConfigInspectionJobStatus::Succeeded,
            ConfigInspectionJobStatus::Failed,
        ] {
            assert_eq!(
                ConfigInspectionJobStatus::parse(status.as_str()).unwrap(),
                status
            );
        }
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_uses_exact_targets_and_is_idempotent(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);

        let first = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[target.clone()],
        )
        .await
        .expect("first inspection enqueue should succeed");
        assert_eq!(first.requested_targets, 1);
        assert_eq!(first.inserted_jobs, 1);

        let second =
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                .await
                .expect("duplicate inspection enqueue should succeed");
        assert_eq!(second.requested_targets, 1);
        assert_eq!(second.inserted_jobs, 0);
        assert_eq!(job_count(&pool, commit_id).await, 1);

        sqlx::query(
            "UPDATE config_inspection_jobs SET status = 'running', started_at = now(), updated_at = now() WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("inspection job should enter running state");
        let third = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[successful_system(derivation_id, "host", &drv_path)],
        )
        .await
        .expect("running duplicate inspection enqueue should succeed");
        assert_eq!(third.inserted_jobs, 0);
        assert_eq!(job_count(&pool, commit_id).await, 1);

        let row = sqlx::query(
            "SELECT derivation_id, configuration_name, carrier_drv_path, status FROM config_inspection_jobs WHERE commit_id = $1",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("inspection job should load");
        assert_eq!(row.get::<i32, _>("derivation_id"), derivation_id);
        assert_eq!(row.get::<String, _>("configuration_name"), "host");
        assert_eq!(row.get::<String, _>("carrier_drv_path"), drv_path);
        assert_eq!(row.get::<String, _>("status"), "running");
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_batches_multiple_configs_and_rejects_mismatches(pool: PgPool) {
        let (commit_id, first_id, first_drv) = fixture(&pool, "first").await;
        let (_, second_id, second_drv) = fixture(&pool, "second").await;
        sqlx::query("UPDATE derivations SET commit_id = $1 WHERE id = $2")
            .bind(commit_id)
            .bind(second_id)
            .execute(&pool)
            .await
            .expect("second derivation should move to the fixture commit");

        let summary = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[
                successful_system(first_id, "first", &first_drv),
                successful_system(second_id, "second", &second_drv),
            ],
        )
        .await
        .expect("batched inspection enqueue should succeed");
        assert_eq!(summary.inserted_jobs, 2);
        assert_eq!(job_count(&pool, commit_id).await, 2);

        let wrong = successful_system(first_id, "wrong-name", &first_drv);
        assert!(
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[wrong])
                .await
                .is_err()
        );
        assert_eq!(job_count(&pool, commit_id).await, 2);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_suppresses_only_ready_same_carrier_v2(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        insert_v2_snapshot(&pool, commit_id, "host", &drv_path, true).await;

        let summary =
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                .await
                .expect("ready V2 artifact should suppress work");
        assert_eq!(summary.inserted_jobs, 0);
        assert_eq!(job_count(&pool, commit_id).await, 0);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_allows_incomplete_or_different_v2_and_terminal_retries(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        insert_v2_snapshot(&pool, commit_id, "host", &drv_path, false).await;

        let first = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[target.clone()],
        )
        .await
        .expect("incomplete V2 should enqueue");
        assert_eq!(first.inserted_jobs, 1);
        sqlx::query(
            "UPDATE config_inspection_jobs SET status = 'failed', started_at = now(), completed_at = now(), error = 'test failure', updated_at = now() WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("terminal test job should update");
        let retry =
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                .await
                .expect("terminal history should allow retry");
        assert_eq!(retry.inserted_jobs, 1);
        assert_eq!(job_count(&pool, commit_id).await, 2);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_matrix_does_not_accept_non_satisfying_snapshot_states(pool: PgPool) {
        let cases = [
            ("v1", false, "v1"),
            ("different-carrier", true, "different-carrier"),
            ("unavailable", true, "unavailable"),
            ("uncertified", true, "uncertified"),
        ];
        for (case_name, insert_v2, configuration_name) in cases {
            let (commit_id, derivation_id, drv_path) = fixture(&pool, case_name).await;
            let snapshot_id = Uuid::new_v4();
            if case_name == "v1" {
                sqlx::query(
                    "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle) VALUES ($1, $2, $3, 1, 'available')",
                )
                .bind(snapshot_id)
                .bind(commit_id)
                .bind(configuration_name)
                .execute(&pool)
                .await
                .expect("V1 snapshot should persist");
                sqlx::query(
                    "INSERT INTO evaluation_snapshot_selections (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
                )
                .bind(commit_id)
                .bind(configuration_name)
                .bind(snapshot_id)
                .execute(&pool)
                .await
                .expect("V1 evaluation selector should persist");
            } else if case_name == "different-carrier" {
                insert_v2_snapshot(
                    &pool,
                    commit_id,
                    configuration_name,
                    &format!("{drv_path}-other"),
                    true,
                )
                .await;
                let target = successful_system(derivation_id, configuration_name, &drv_path);
                let summary = enqueue_config_inspection_jobs_for_successful_systems(
                    &pool,
                    commit_id,
                    &[target],
                )
                .await
                .expect("different-carrier V2 should enqueue");
                assert_eq!(summary.inserted_jobs, 1);
                continue;
            } else if insert_v2 {
                sqlx::query(
                    "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, carrier_drv_path, comparison_ready) VALUES ($1, $2, $3, 2, $4, $5, $6)",
                )
                .bind(snapshot_id)
                .bind(commit_id)
                .bind(configuration_name)
                .bind(if case_name == "unavailable" { "unavailable" } else { "available" })
                .bind(&drv_path)
                .bind(case_name != "unavailable")
                .execute(&pool)
                .await
                .expect("non-certified V2 snapshot should persist");
            }
            if case_name != "v1" {
                add_snapshot_selector(&pool, commit_id, configuration_name, snapshot_id).await;
            }
            let summary = enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                &[successful_system(
                    derivation_id,
                    configuration_name,
                    &drv_path,
                )],
            )
            .await
            .expect("non-satisfying snapshot should enqueue");
            assert_eq!(summary.inserted_jobs, 1, "case {case_name}");
        }
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_rejects_each_finalized_target_mismatch_atomically(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let (_, other_commit_derivation_id, other_drv_path) = fixture(&pool, "other-commit").await;
        let package_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count) VALUES ($1, 'package', 'package', $2, (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1), 0) RETURNING id",
        )
        .bind(commit_id)
        .bind(format!("{drv_path}-package"))
        .fetch_one(&pool)
        .await
        .expect("package derivation should persist");
        let cases = [
            successful_system(other_commit_derivation_id, "other-commit", &other_drv_path),
            successful_system(derivation_id, "wrong-name", &drv_path),
            successful_system(derivation_id, "host", &format!("{drv_path}-wrong")),
            successful_system(package_id, "package", &format!("{drv_path}-package")),
        ];
        for target in cases {
            let result =
                enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                    .await;
            assert!(result.is_err());
            assert_eq!(job_count(&pool, commit_id).await, 0);
        }
        let valid = successful_system(derivation_id, "host", &drv_path);
        let invalid = successful_system(derivation_id, "wrong-name", &drv_path);
        assert!(
            enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                &[valid, invalid]
            )
            .await
            .is_err()
        );
        assert_eq!(job_count(&pool, commit_id).await, 0);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn config_inspection_jobs_reject_target_mutation_and_bad_lifecycle(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
            .await
            .expect("inspection job should enqueue");
        let mutation = sqlx::query(
            "UPDATE config_inspection_jobs SET carrier_drv_path = '/nix/store/other.drv' WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await;
        assert!(mutation.is_err());
        let lifecycle = sqlx::query(
            "UPDATE config_inspection_jobs SET status = 'succeeded', completed_at = now() WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await;
        assert!(lifecycle.is_err());
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn concurrent_enqueue_calls_share_one_active_row(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        let (left, right) = tokio::join!(
            enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                std::slice::from_ref(&target)
            ),
            enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                std::slice::from_ref(&target)
            ),
        );
        assert!(left.is_ok());
        assert!(right.is_ok());
        assert_eq!(job_count(&pool, commit_id).await, 1);
    }
}
