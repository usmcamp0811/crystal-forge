//! Persists scoped Config Explorer requests and immutable observations.
//!
//! This module never reads or writes evaluation snapshot selectors. Request
//! creation resolves an exact completed NixOS carrier before cache lookup.

use anyhow::{Context, Result, bail};
use chrono::{Duration, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::models::config_observations::{
    CONFIG_OBSERVATION_SCHEMA_VERSION, ConfigObservationKind, ConfigObservationLifecycle,
    ConfigObservationRequestResponse, ConfigObservationResponse, validate_config_observation_path,
    validate_config_observation_payload,
};

const MAX_CONFIG_OBSERVATION_ERROR_CHARS: usize = 4096;

/// Contains the immutable target needed by one scoped execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigObservationExecutionTarget {
    /// Durable request identity.
    pub request_id: Uuid,
    /// Exact commit identity.
    pub commit_id: i32,
    /// Exact derivation identity.
    pub derivation_id: i32,
    /// Effective configuration name.
    pub configuration_name: String,
    /// Exact carrier derivation path.
    pub carrier_drv_path: String,
    /// Full immutable commit SHA.
    pub revision: String,
    /// Owning flake repository URL.
    pub repo_url: String,
    /// Owning flake ID for server-side credential lookup.
    pub flake_id: i32,
    /// Scoped operation.
    pub kind: ConfigObservationKind,
    /// Exact structured path components.
    pub path_components: Vec<String>,
}

/// Contains a running scoped execution after capacity is held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigObservationExecution {
    /// Immutable target.
    pub target: ConfigObservationExecutionTarget,
    /// Fencing token for heartbeat and completion.
    pub execution_id: Uuid,
    /// Number of attempts that acquired capacity.
    pub attempts: i32,
}

/// Describes create-or-reuse target resolution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CreateConfigObservationOutcome {
    /// The system or revision is outside the exact target scope.
    NotFound,
    /// The primary evaluator has not completed the exact NixOS carrier.
    PrerequisiteMissing,
    /// A cache entry or active request was returned.
    Resolved(ConfigObservationRequestResponse),
}

/// Creates or reuses one exact scoped request.
///
/// The transaction resolves the system configuration, full revision, completed
/// NixOS derivation, and carrier before it checks immutable cache identity.
/// Active identical requests coalesce through a partial unique index.
///
/// # Errors
///
/// Returns an error when the path is invalid or PostgreSQL cannot resolve or
/// persist the request.
pub(crate) async fn create_or_reuse_config_observation_request(
    pool: &PgPool,
    system_id: Uuid,
    revision: &str,
    kind: ConfigObservationKind,
    path_components: &[String],
) -> Result<CreateConfigObservationOutcome> {
    validate_config_observation_path(kind, path_components)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Config observation request")?;
    let target = sqlx::query(
        r#"
        SELECT commit.id AS commit_id,
               COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname) AS configuration_name
        FROM systems system
        JOIN commits commit
          ON commit.flake_id = system.flake_id
         AND commit.git_commit_hash = $2
         AND commit.source_archived = FALSE
        WHERE system.id = $1
        FOR SHARE OF system, commit
        "#,
    )
    .bind(system_id)
    .bind(revision)
    .fetch_optional(&mut *tx)
    .await
    .context("resolve exact Config observation target")?;
    let Some(target) = target else {
        tx.rollback().await.ok();
        return Ok(CreateConfigObservationOutcome::NotFound);
    };
    let commit_id: i32 = target.try_get("commit_id")?;
    let configuration_name: String = target.try_get("configuration_name")?;
    let derivation = sqlx::query_as::<_, (i32, String)>(
        r#"
        SELECT derivation.id, derivation.derivation_path
        FROM derivations derivation
        JOIN commits commit
          ON commit.id = derivation.commit_id
         AND commit.evaluation_status = 'complete'
        WHERE derivation.commit_id = $1
          AND derivation.derivation_type = 'nixos'
          AND derivation.derivation_name = $2
          AND derivation.completed_at IS NOT NULL
          AND NULLIF(BTRIM(derivation.derivation_path), '') IS NOT NULL
        FOR SHARE OF derivation
        "#,
    )
    .bind(commit_id)
    .bind(&configuration_name)
    .fetch_optional(&mut *tx)
    .await
    .context("resolve exact completed Config observation carrier")?;
    let Some((derivation_id, carrier_drv_path)) = derivation else {
        tx.rollback().await.ok();
        return Ok(CreateConfigObservationOutcome::PrerequisiteMissing);
    };
    let path = serde_json::to_value(path_components)?;

    let existing = sqlx::query(
        r#"
        SELECT request.id, request.status, request.attempts,
               request.execution_heartbeat_at, request.observation_id, request.error,
               content.payload AS observation_payload
        FROM config_observation_requests request
        LEFT JOIN config_observations observation ON observation.id = request.observation_id
        LEFT JOIN config_observation_contents content ON content.digest = observation.content_digest
        WHERE request.commit_id = $1
          AND request.configuration_name = $2
          AND request.derivation_id = $3
          AND request.carrier_drv_path = $4
          AND request.schema_version = $5
          AND request.path_components = $6
          AND request.kind = $7
          AND request.status IN ('queued', 'waiting_for_capacity', 'running', 'succeeded')
        ORDER BY CASE WHEN request.status = 'succeeded' THEN 0 ELSE 1 END,
                 request.created_at DESC, request.id DESC
        LIMIT 1
        "#,
    )
    .bind(commit_id)
    .bind(&configuration_name)
    .bind(derivation_id)
    .bind(&carrier_drv_path)
    .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
    .bind(&path)
    .bind(kind.as_str())
    .fetch_optional(&mut *tx)
    .await
    .context("find reusable Config observation request")?;

    let reused = existing.is_some();
    let row = if let Some(existing) = existing {
        if existing.try_get::<String, _>("status")? == "succeeded" {
            let payload: Value = existing.try_get("observation_payload")?;
            validate_config_observation_payload(kind, path_components, &payload)
                .context("validate reusable Config observation payload")?;
        }
        existing
    } else {
        sqlx::query(
            r#"
            INSERT INTO config_observation_requests (
                commit_id, derivation_id, configuration_name, carrier_drv_path,
                schema_version, path_components, kind, priority, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'queued')
            ON CONFLICT (
                commit_id, configuration_name, derivation_id, carrier_drv_path,
                schema_version, path_components, kind
            ) WHERE status IN ('queued', 'waiting_for_capacity', 'running')
            DO UPDATE SET updated_at = config_observation_requests.updated_at
            RETURNING id, status, attempts, execution_heartbeat_at, observation_id, error
            "#,
        )
        .bind(commit_id)
        .bind(derivation_id)
        .bind(&configuration_name)
        .bind(&carrier_drv_path)
        .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
        .bind(&path)
        .bind(kind.as_str())
        .bind(kind.priority())
        .fetch_one(&mut *tx)
        .await
        .context("insert or coalesce Config observation request")?
    };
    let response = request_response_from_row(
        row,
        revision,
        &configuration_name,
        kind,
        path_components,
        reused,
    )?;
    tx.commit()
        .await
        .context("commit Config observation request")?;
    Ok(CreateConfigObservationOutcome::Resolved(response))
}

fn request_response_from_row(
    row: sqlx::postgres::PgRow,
    revision: &str,
    configuration_name: &str,
    kind: ConfigObservationKind,
    path_components: &[String],
    reused: bool,
) -> Result<ConfigObservationRequestResponse> {
    Ok(ConfigObservationRequestResponse {
        request_id: row.try_get("id")?,
        revision: revision.to_string(),
        configuration_name: configuration_name.to_string(),
        kind,
        path_components: path_components.to_vec(),
        lifecycle: ConfigObservationLifecycle::parse(row.try_get::<String, _>("status")?.as_str())?,
        observation_id: row.try_get("observation_id")?,
        error: row.try_get("error")?,
        attempts: row.try_get("attempts")?,
        heartbeat_at: row.try_get("execution_heartbeat_at")?,
        reused,
    })
}

/// Loads one request only when it belongs to the specified system identity.
///
/// This read never inserts work or changes lifecycle state.
///
/// # Errors
///
/// Returns an error when PostgreSQL data is malformed or unavailable.
pub(crate) async fn get_config_observation_request(
    pool: &PgPool,
    system_id: Uuid,
    request_id: Uuid,
) -> Result<Option<ConfigObservationRequestResponse>> {
    let row = sqlx::query(
        r#"
        SELECT request.id, commit.git_commit_hash AS revision,
               request.configuration_name, request.kind, request.path_components,
               request.status, request.attempts, request.execution_heartbeat_at,
               request.observation_id, request.error
        FROM config_observation_requests request
        JOIN commits commit ON commit.id = request.commit_id
        JOIN systems system
          ON system.id = $1
         AND system.flake_id = commit.flake_id
         AND COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname) = request.configuration_name
        WHERE request.id = $2
        "#,
    )
    .bind(system_id)
    .bind(request_id)
    .fetch_optional(pool)
    .await
    .context("load scoped Config observation request")?;
    row.map(|row| {
        let kind = ConfigObservationKind::parse(row.try_get::<String, _>("kind")?.as_str())?;
        let path: Vec<String> = serde_json::from_value(row.try_get("path_components")?)?;
        let revision: String = row.try_get("revision")?;
        let configuration_name: String = row.try_get("configuration_name")?;
        request_response_from_row(row, &revision, &configuration_name, kind, &path, false)
    })
    .transpose()
}

/// Loads one immutable observation only when it belongs to the specified system.
///
/// This read never inserts work or changes lifecycle state.
///
/// # Errors
///
/// Returns an error when PostgreSQL data is malformed or unavailable.
pub(crate) async fn get_config_observation(
    pool: &PgPool,
    system_id: Uuid,
    observation_id: Uuid,
) -> Result<Option<ConfigObservationResponse>> {
    let row = sqlx::query(
        r#"
        SELECT observation.id, commit.git_commit_hash AS revision,
               observation.configuration_name, observation.schema_version,
               observation.kind, observation.path_components, content.payload,
               observation.created_at
        FROM config_observations observation
        JOIN config_observation_contents content ON content.digest = observation.content_digest
        JOIN commits commit ON commit.id = observation.commit_id
        JOIN systems system
          ON system.id = $1
         AND system.flake_id = commit.flake_id
         AND COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname) = observation.configuration_name
        WHERE observation.id = $2
        "#,
    )
    .bind(system_id)
    .bind(observation_id)
    .fetch_optional(pool)
    .await
    .context("load immutable Config observation")?;
    row.map(|row| {
        let kind = ConfigObservationKind::parse(row.try_get::<String, _>("kind")?.as_str())?;
        let path_components: Vec<String> = serde_json::from_value(row.try_get("path_components")?)?;
        let payload: Value = row.try_get("payload")?;
        validate_config_observation_payload(kind, &path_components, &payload)
            .context("validate stored Config observation payload")?;
        Ok(ConfigObservationResponse {
            observation_id: row.try_get("id")?,
            revision: row.try_get("revision")?,
            configuration_name: row.try_get("configuration_name")?,
            schema_version: row.try_get("schema_version")?,
            kind,
            path_components,
            payload,
            created_at: row.try_get("created_at")?,
        })
    })
    .transpose()
}

/// Marks the highest-priority candidate as waiting without consuming an attempt.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot reserve the candidate.
pub(crate) async fn reserve_next_config_observation(
    pool: &PgPool,
) -> Result<Option<ConfigObservationExecutionTarget>> {
    let row = sqlx::query(
        r#"
        WITH candidate AS (
            SELECT id
            FROM config_observation_requests
            WHERE status IN ('queued', 'waiting_for_capacity')
            ORDER BY priority, (status = 'waiting_for_capacity') ASC,
                     scheduled_at, created_at, id
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE config_observation_requests request
        SET status = 'waiting_for_capacity', updated_at = now()
        FROM candidate, commits commit, flakes flake
        WHERE request.id = candidate.id
          AND commit.id = request.commit_id
          AND flake.id = commit.flake_id
        RETURNING request.id, request.commit_id, request.derivation_id,
                  request.configuration_name, request.carrier_drv_path,
                  commit.git_commit_hash AS revision, flake.repo_url, flake.id AS flake_id,
                  request.kind, request.path_components
        "#,
    )
    .fetch_optional(pool)
    .await
    .context("reserve Config observation candidate")?;
    row.map(execution_target_from_row).transpose()
}

fn execution_target_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<ConfigObservationExecutionTarget> {
    Ok(ConfigObservationExecutionTarget {
        request_id: row.try_get("id")?,
        commit_id: row.try_get("commit_id")?,
        derivation_id: row.try_get("derivation_id")?,
        configuration_name: row.try_get("configuration_name")?,
        carrier_drv_path: row.try_get("carrier_drv_path")?,
        revision: row.try_get("revision")?,
        repo_url: row.try_get("repo_url")?,
        flake_id: row.try_get("flake_id")?,
        kind: ConfigObservationKind::parse(row.try_get::<String, _>("kind")?.as_str())?,
        path_components: serde_json::from_value(row.try_get("path_components")?)?,
    })
}

/// Defers a capacity miss without changing execution attempts.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot update scheduling order.
pub(crate) async fn defer_config_observation_capacity(
    pool: &PgPool,
    request_id: Uuid,
) -> Result<()> {
    sqlx::query(
        "UPDATE config_observation_requests SET scheduled_at = now(), updated_at = now() WHERE id = $1 AND status = 'waiting_for_capacity'",
    )
    .bind(request_id)
    .execute(pool)
    .await
    .context("defer Config observation capacity")?;
    Ok(())
}

/// Starts one waiting request after the caller has acquired Nix capacity.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot perform the fenced transition.
pub(crate) async fn start_config_observation_execution(
    pool: &PgPool,
    target: ConfigObservationExecutionTarget,
    execution_id: Uuid,
) -> Result<Option<ConfigObservationExecution>> {
    let attempts = sqlx::query_scalar::<_, i32>(
        r#"
        UPDATE config_observation_requests
        SET status = 'running', attempts = attempts + 1, execution_id = $2,
            execution_heartbeat_at = now(), started_at = now(), updated_at = now()
        WHERE id = $1 AND status = 'waiting_for_capacity' AND attempts < 3
        RETURNING attempts
        "#,
    )
    .bind(target.request_id)
    .bind(execution_id)
    .fetch_optional(pool)
    .await
    .context("start Config observation execution")?;
    Ok(attempts.map(|attempts| ConfigObservationExecution {
        target,
        execution_id,
        attempts,
    }))
}

/// Refreshes the heartbeat for one exact running execution.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot update the heartbeat.
pub(crate) async fn heartbeat_config_observation_execution(
    pool: &PgPool,
    request_id: Uuid,
    execution_id: Uuid,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE config_observation_requests SET execution_heartbeat_at = now(), updated_at = now() WHERE id = $1 AND status = 'running' AND execution_id = $2",
    )
    .bind(request_id)
    .bind(execution_id)
    .execute(pool)
    .await
    .context("heartbeat Config observation execution")?
    .rows_affected() == 1)
}

/// Persists content and an immutable observation before completing its request.
///
/// Content is redacted before this function. The SHA-256 digest deduplicates
/// identical payloads, while observation uniqueness preserves exact target identity.
///
/// # Errors
///
/// Returns an error when payload bounds or PostgreSQL persistence fail.
pub(crate) async fn complete_config_observation_success(
    pool: &PgPool,
    execution: &ConfigObservationExecution,
    payload: &Value,
) -> Result<Option<Uuid>> {
    validate_config_observation_payload(
        execution.target.kind,
        &execution.target.path_components,
        payload,
    )?;
    let payload_bytes = serde_json::to_vec(payload)?;
    if !payload.is_object() || payload_bytes.len() > 8 * 1024 * 1024 {
        bail!("Config observation payload exceeds its persistence contract");
    }
    let digest = Sha256::digest(&payload_bytes).to_vec();
    let canonical_payload: Value = serde_json::from_slice(&payload_bytes)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Config observation persistence")?;
    sqlx::query(
        "INSERT INTO config_observation_contents (digest, schema_version, payload) VALUES ($1, $2, $3) ON CONFLICT (digest) DO NOTHING",
    )
    .bind(&digest)
    .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
    .bind(&canonical_payload)
    .execute(&mut *tx)
    .await
    .context("persist Config observation content")?;
    let observation_id: Uuid = sqlx::query_scalar(
        r#"
        WITH inserted AS (
            INSERT INTO config_observations (
                commit_id, derivation_id, configuration_name, carrier_drv_path,
                schema_version, path_components, kind, content_digest
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (
                commit_id, configuration_name, derivation_id, carrier_drv_path,
                schema_version, path_components, kind
            ) DO NOTHING
            RETURNING id
        )
        SELECT id FROM inserted
        UNION ALL
        SELECT id
        FROM config_observations
        WHERE commit_id = $1 AND derivation_id = $2
          AND configuration_name = $3 AND carrier_drv_path = $4
          AND schema_version = $5 AND path_components = $6 AND kind = $7
        LIMIT 1
        "#,
    )
    .bind(execution.target.commit_id)
    .bind(execution.target.derivation_id)
    .bind(&execution.target.configuration_name)
    .bind(&execution.target.carrier_drv_path)
    .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
    .bind(serde_json::to_value(&execution.target.path_components)?)
    .bind(execution.target.kind.as_str())
    .bind(&digest)
    .fetch_one(&mut *tx)
    .await
    .context("persist immutable Config observation")?;
    let completed = sqlx::query(
        "UPDATE config_observation_requests SET status = 'succeeded', observation_id = $3, completed_at = now(), updated_at = now() WHERE id = $1 AND status = 'running' AND execution_id = $2",
    )
    .bind(execution.target.request_id)
    .bind(execution.execution_id)
    .bind(observation_id)
    .execute(&mut *tx)
    .await
    .context("complete Config observation request")?
    .rows_affected() == 1;
    if completed {
        tx.commit()
            .await
            .context("commit Config observation persistence")?;
        Ok(Some(observation_id))
    } else {
        tx.rollback().await.ok();
        Ok(None)
    }
}

/// Completes one exact execution with a stable bounded redacted error.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot perform the fenced update.
pub(crate) async fn complete_config_observation_failure(
    pool: &PgPool,
    execution: &ConfigObservationExecution,
    error: &str,
) -> Result<bool> {
    let redacted = crate::security::snapshot_redaction::redact_text(error);
    let bounded: String = redacted
        .chars()
        .take(MAX_CONFIG_OBSERVATION_ERROR_CHARS)
        .collect();
    let safe = if bounded.trim().is_empty() {
        "Config observation execution failed".to_string()
    } else {
        bounded
    };
    Ok(sqlx::query(
        "UPDATE config_observation_requests SET status = 'failed', error = $3, completed_at = now(), updated_at = now() WHERE id = $1 AND status = 'running' AND execution_id = $2",
    )
    .bind(execution.target.request_id)
    .bind(execution.execution_id)
    .bind(safe)
    .execute(pool)
    .await
    .context("fail Config observation execution")?
    .rows_affected() == 1)
}

/// Terminalizes abandoned running executions after checking execution locks.
///
/// The session-level advisory lock is authoritative over heartbeat age. A
/// stale-looking request remains running while its owner still holds that lock.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot inspect ownership or update state.
pub(crate) async fn recover_stale_config_observations(
    pool: &PgPool,
    stale_threshold: Duration,
) -> Result<usize> {
    let stale_before = Utc::now() - stale_threshold;
    let candidates: Vec<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        SELECT id, execution_id
        FROM config_observation_requests
        WHERE status = 'running' AND execution_heartbeat_at < $1
        ORDER BY execution_heartbeat_at, id
        LIMIT 32
        "#,
    )
    .bind(stale_before)
    .fetch_all(pool)
    .await
    .context("list stale Config observation executions")?;
    let mut recovered = 0;
    for (request_id, execution_id) in candidates {
        if crate::queries::cve_scans::execution_lock_is_held(pool, execution_id).await? {
            continue;
        }
        recovered += sqlx::query(
            r#"
            UPDATE config_observation_requests
            SET status = 'failed', error = 'Config observation executor stopped',
                completed_at = now(), updated_at = now()
            WHERE id = $1 AND status = 'running' AND execution_id = $2
              AND execution_heartbeat_at < $3
            "#,
        )
        .bind(request_id)
        .bind(execution_id)
        .bind(stale_before)
        .execute(pool)
        .await
        .context("recover stale Config observation execution")?
        .rows_affected() as usize;
    }
    Ok(recovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_priorities_keep_interactive_work_ahead_of_indexes() {
        assert!(
            ConfigObservationKind::Root.priority()
                < ConfigObservationKind::ConfiguredIndex.priority()
        );
        assert_eq!(
            ConfigObservationKind::Prefix.priority(),
            ConfigObservationKind::Option.priority()
        );
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn scoped_requests_coalesce_cache_exact_identity_and_gets_are_read_only(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("scoped-observation-{suffix}"))
        .bind(format!(
            "https://example.test/scoped-observation-{suffix}.git"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        let revision = format!("{:0>40}", &suffix[..32]);
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_status) VALUES ($1, $2, now(), 'complete') RETURNING id",
        )
        .bind(flake_id)
        .bind(&revision)
        .fetch_one(&pool)
        .await
        .unwrap();
        let configuration_name = format!("config-{suffix}");
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, flake_id, derivation, system_configuration_name) VALUES ($1, 'test-key', $2, '', $3) RETURNING id",
        )
        .bind(format!("host-{suffix}"))
        .bind(flake_id)
        .bind(&configuration_name)
        .fetch_one(&pool)
        .await
        .unwrap();
        let carrier = format!("/nix/store/{suffix}-{configuration_name}.drv");
        sqlx::query(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, completed_at) VALUES ($1, 'nixos', $2, $3, 5, now())",
        )
        .bind(commit_id)
        .bind(&configuration_name)
        .bind(&carrier)
        .execute(&pool)
        .await
        .unwrap();

        let path = vec!["services".to_string()];
        let first = create_or_reuse_config_observation_request(
            &pool,
            system_id,
            &revision,
            ConfigObservationKind::Prefix,
            &path,
        )
        .await
        .unwrap();
        let CreateConfigObservationOutcome::Resolved(first) = first else {
            panic!("exact target should resolve");
        };
        assert_eq!(first.lifecycle, ConfigObservationLifecycle::Queued);
        assert!(!first.reused);
        let second = create_or_reuse_config_observation_request(
            &pool,
            system_id,
            &revision,
            ConfigObservationKind::Prefix,
            &path,
        )
        .await
        .unwrap();
        let CreateConfigObservationOutcome::Resolved(second) = second else {
            panic!("active target should resolve");
        };
        assert_eq!(second.request_id, first.request_id);
        assert!(second.reused);

        let target = reserve_next_config_observation(&pool)
            .await
            .unwrap()
            .expect("queued request should reserve");
        let waiting: (String, i32, Option<Uuid>) = sqlx::query_as(
            "SELECT status, attempts, execution_id FROM config_observation_requests WHERE id = $1",
        )
        .bind(first.request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(waiting, ("waiting_for_capacity".to_string(), 0, None));
        defer_config_observation_capacity(&pool, first.request_id)
            .await
            .unwrap();
        let execution_id = Uuid::new_v4();
        let execution = start_config_observation_execution(&pool, target, execution_id)
            .await
            .unwrap()
            .expect("capacity-held request should start");
        assert_eq!(execution.attempts, 1);
        let observation_id = complete_config_observation_success(
            &pool,
            &execution,
            &serde_json::json!({
                "kind": "prefix",
                "path_components": ["services"],
                "children": [],
                "children_truncated": false,
                "total_children": 0
            }),
        )
        .await
        .unwrap()
        .expect("owned execution should complete");

        let before_reads: (i64, String, i32) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM config_observation_requests), status, attempts FROM config_observation_requests WHERE id = $1",
        )
        .bind(first.request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let request = get_config_observation_request(&pool, system_id, first.request_id)
            .await
            .unwrap()
            .expect("request should remain visible");
        let observation = get_config_observation(&pool, system_id, observation_id)
            .await
            .unwrap()
            .expect("observation should remain visible");
        assert_eq!(request.lifecycle, ConfigObservationLifecycle::Succeeded);
        assert_eq!(observation.path_components, path);
        assert_eq!(
            sqlx::query_as::<_, (i64, String, i32)>(
                "SELECT (SELECT COUNT(*) FROM config_observation_requests), status, attempts FROM config_observation_requests WHERE id = $1",
            )
            .bind(first.request_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
            before_reads
        );

        let cached = create_or_reuse_config_observation_request(
            &pool,
            system_id,
            &revision,
            ConfigObservationKind::Prefix,
            &["services".to_string()],
        )
        .await
        .unwrap();
        let CreateConfigObservationOutcome::Resolved(cached) = cached else {
            panic!("cached target should resolve");
        };
        assert_eq!(cached.request_id, first.request_id);
        assert_eq!(cached.observation_id, Some(observation_id));
        assert!(cached.reused);

        let different_path = create_or_reuse_config_observation_request(
            &pool,
            system_id,
            &revision,
            ConfigObservationKind::Prefix,
            &["networking".to_string()],
        )
        .await
        .unwrap();
        let CreateConfigObservationOutcome::Resolved(different_path) = different_path else {
            panic!("different path should resolve independently");
        };
        assert_ne!(different_path.request_id, first.request_id);

        let stale_target = reserve_next_config_observation(&pool)
            .await
            .unwrap()
            .expect("different path should reserve");
        let stale_execution =
            start_config_observation_execution(&pool, stale_target, Uuid::new_v4())
                .await
                .unwrap()
                .expect("different path should start");
        sqlx::query(
            "UPDATE config_observation_requests SET execution_heartbeat_at = now() - interval '1 hour' WHERE id = $1",
        )
        .bind(stale_execution.target.request_id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            recover_stale_config_observations(&pool, Duration::minutes(10))
                .await
                .unwrap(),
            1
        );
        let stale_state: (String, i32, String) = sqlx::query_as(
            "SELECT status, attempts, error FROM config_observation_requests WHERE id = $1",
        )
        .bind(stale_execution.target.request_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            stale_state,
            (
                "failed".to_string(),
                1,
                "Config observation executor stopped".to_string()
            )
        );

        assert!(
            sqlx::query(
                "INSERT INTO config_observation_requests (commit_id, derivation_id, configuration_name, carrier_drv_path, schema_version, path_components, kind, priority, status) SELECT commit_id, derivation_id, configuration_name, carrier_drv_path, 1, '[\"not-root\"]'::jsonb, 'root', 10, 'queued' FROM config_observation_requests WHERE id = $1",
            )
            .bind(first.request_id)
            .execute(&pool)
            .await
            .is_err()
        );
        assert!(
            sqlx::query(
                "INSERT INTO config_observation_requests (commit_id, derivation_id, configuration_name, carrier_drv_path, schema_version, path_components, kind, priority, status) SELECT commit_id, derivation_id, configuration_name, '/nix/store/wrong-carrier.drv', 1, '[\"services\"]'::jsonb, 'prefix', 10, 'queued' FROM config_observation_requests WHERE id = $1",
            )
            .bind(first.request_id)
            .execute(&pool)
            .await
            .is_err()
        );
        assert!(
            sqlx::query(
                "UPDATE config_observations SET configuration_name = 'changed' WHERE id = $1"
            )
            .bind(observation_id)
            .execute(&pool)
            .await
            .is_err()
        );
    }
}
