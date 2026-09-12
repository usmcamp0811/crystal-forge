//! Persists scoped Config Explorer requests and immutable observations.
//!
//! This module reads a certified V2 selector only to adapt existing evidence.
//! It never writes evaluation snapshot selectors. Request creation resolves an
//! exact completed NixOS carrier before cache lookup or V2 adaptation.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::models::config_inspector::{InspectionTarget, option_key};
use crate::models::config_observations::{
    CONFIG_OBSERVATION_SCHEMA_VERSION, ConfigObservationKind, ConfigObservationLifecycle,
    ConfigObservationRequestResponse, ConfigObservationResponse,
    validate_config_observation_identity, validate_config_observation_payload,
};
use crate::models::config_snapshot_artifact::{
    ConfigOptionArtifactV2, ConfigOptionMetadataArtifactV2, config_option_v2_from_persisted,
};

const MAX_CONFIG_OBSERVATION_ERROR_CHARS: usize = 4096;

fn normalize_legacy_tree_payload(
    kind: ConfigObservationKind,
    child_offset: u32,
    payload: &mut Value,
) {
    if child_offset != 0
        || !matches!(
            kind,
            ConfigObservationKind::Root | ConfigObservationKind::Prefix
        )
    {
        return;
    }
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    // COMPATIBILITY: Migration 0255 payloads predate child-page identity. Their
    // immutable root and prefix contents represent offset zero. Add the field
    // only to the API projection; do not rewrite content-addressed storage.
    object
        .entry("child_offset")
        .or_insert_with(|| Value::from(0));
}

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
    /// Zero-based immediate-child offset.
    pub child_offset: u32,
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
/// NixOS derivation, and carrier before it checks immutable cache identity. A
/// certified selected V2 artifact can satisfy truthful scoped operations before
/// queue insertion. Active identical requests coalesce through a partial unique
/// index, and the child offset is part of every persistence identity.
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
    child_offset: u32,
) -> Result<CreateConfigObservationOutcome> {
    validate_config_observation_identity(kind, path_components, child_offset)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Config observation request")?;
    let target = sqlx::query(
        r#"
        SELECT commit.id AS commit_id, flake.repo_url,
               COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname) AS configuration_name
        FROM systems system
        JOIN flakes flake ON flake.id = system.flake_id
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
    let repo_url: String = target.try_get("repo_url")?;
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
    // CONCURRENCY: Serialize one exact scoped identity so V2 adaptation and
    // queue insertion cannot leave both a succeeded and an active request.
    let lock_identity = serde_json::to_string(&(
        commit_id,
        &configuration_name,
        derivation_id,
        &carrier_drv_path,
        CONFIG_OBSERVATION_SCHEMA_VERSION,
        path_components,
        kind.as_str(),
        child_offset,
    ))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('config_observation'), hashtext($1))")
        .bind(lock_identity)
        .execute(&mut *tx)
        .await
        .context("lock exact Config observation identity")?;

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
          AND request.child_offset = $8
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
    .bind(i32::try_from(child_offset)?)
    .fetch_optional(&mut *tx)
    .await
    .context("find reusable Config observation request")?;

    let mut reused = existing.is_some();
    let row = if let Some(existing) = existing {
        if existing.try_get::<String, _>("status")? == "succeeded" {
            let mut payload: Value = existing.try_get("observation_payload")?;
            normalize_legacy_tree_payload(kind, child_offset, &mut payload);
            validate_config_observation_payload(kind, path_components, child_offset, &payload)
                .context("validate reusable Config observation payload")?;
        }
        existing
    } else if let Some(row) = adapt_selected_v2_observation_tx(
        &mut tx,
        commit_id,
        derivation_id,
        &configuration_name,
        &carrier_drv_path,
        &repo_url,
        revision,
        kind,
        path_components,
        child_offset,
    )
    .await?
    {
        reused = true;
        row
    } else {
        sqlx::query(
            r#"
            INSERT INTO config_observation_requests (
                commit_id, derivation_id, configuration_name, carrier_drv_path,
                schema_version, path_components, kind, child_offset, priority, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'queued')
            ON CONFLICT (
                commit_id, configuration_name, derivation_id, carrier_drv_path,
                schema_version, path_components, kind, child_offset
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
        .bind(i32::try_from(child_offset)?)
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
        child_offset,
        reused,
    )?;
    tx.commit()
        .await
        .context("commit Config observation request")?;
    Ok(CreateConfigObservationOutcome::Resolved(response))
}

async fn adapt_selected_v2_observation_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    derivation_id: i32,
    configuration_name: &str,
    carrier_drv_path: &str,
    repo_url: &str,
    revision: &str,
    kind: ConfigObservationKind,
    path_components: &[String],
    child_offset: u32,
) -> Result<Option<sqlx::postgres::PgRow>> {
    if kind == ConfigObservationKind::ConfiguredIndex {
        return Ok(None);
    }
    let flake_ref = crate::derivations::utils::build_flake_reference(repo_url, revision);
    let target_key = InspectionTarget::new(&flake_ref, configuration_name).target_key;
    let snapshot = sqlx::query_as::<_, (Uuid, bool)>(
        r#"
        SELECT snapshot.id, snapshot.option_inventory_complete
        FROM config_snapshot_selections selection
        JOIN evaluation_snapshots snapshot
          ON snapshot.id = selection.current_snapshot_id
         AND snapshot.commit_id = selection.commit_id
         AND snapshot.configuration_name = selection.configuration_name
        WHERE selection.commit_id = $1
          AND selection.configuration_name = $2
          AND snapshot.lifecycle = 'available'
          AND snapshot.schema_version = 2
          AND snapshot.integrity_version = 2
          AND snapshot.carrier_drv_path = $3
          AND snapshot.target_key = $4
        FOR SHARE OF selection, snapshot
        "#,
    )
    .bind(commit_id)
    .bind(configuration_name)
    .bind(carrier_drv_path)
    .bind(target_key)
    .fetch_optional(&mut **tx)
    .await
    .context("select exact certified V2 Config observation source")?;
    let Some((snapshot_id, inventory_complete)) = snapshot else {
        return Ok(None);
    };

    let payload = match kind {
        ConfigObservationKind::Root | ConfigObservationKind::Prefix => {
            let identities = sqlx::query_as::<_, (String, Vec<String>)>(
                r#"
                SELECT option_key, path_components
                FROM evaluation_snapshot_options
                WHERE snapshot_id = $1
                  AND (cardinality($2::text[]) = 0
                       OR path_components[1:cardinality($2::text[])] = $2)
                "#,
            )
            .bind(snapshot_id)
            .bind(path_components)
            .fetch_all(&mut **tx)
            .await
            .context("load certified V2 tree identities")?;
            adapt_v2_tree_page(
                inventory_complete,
                kind,
                path_components,
                child_offset,
                &identities,
            )?
        }
        ConfigObservationKind::Option | ConfigObservationKind::Provenance => {
            let option = sqlx::query_as::<_, (String, Vec<String>, Value)>(
                r#"
                SELECT item.option_key, item.path_components, content.payload
                FROM evaluation_snapshot_options item
                JOIN evaluation_option_contents content
                  ON content.digest = item.content_digest
                 AND content.schema_version = 2
                WHERE item.snapshot_id = $1 AND item.path_components = $2
                "#,
            )
            .bind(snapshot_id)
            .bind(path_components)
            .fetch_optional(&mut **tx)
            .await
            .context("load exact certified V2 option")?;
            let Some((option_key, option_path, payload)) = option else {
                return Ok(None);
            };
            let option = config_option_v2_from_persisted(option_key, option_path, payload)
                .context("decode exact certified V2 option")?;
            adapt_v2_exact_option(kind, path_components, &option)?
        }
        ConfigObservationKind::ConfiguredIndex => unreachable!("handled above"),
    };
    let Some(payload) = payload else {
        return Ok(None);
    };
    persist_adapted_v2_observation_tx(
        tx,
        commit_id,
        derivation_id,
        configuration_name,
        carrier_drv_path,
        kind,
        path_components,
        child_offset,
        &payload,
    )
    .await
    .map(Some)
}

fn adapt_v2_tree_page(
    inventory_complete: bool,
    kind: ConfigObservationKind,
    path_components: &[String],
    child_offset: u32,
    identities: &[(String, Vec<String>)],
) -> Result<Option<Value>> {
    if !inventory_complete
        || !matches!(
            kind,
            ConfigObservationKind::Root | ConfigObservationKind::Prefix
        )
    {
        return Ok(None);
    }
    let mut exact_target = false;
    let mut children = BTreeMap::<String, (bool, bool)>::new();
    for (stored_key, option_path) in identities {
        if option_key(option_path) != *stored_key {
            return Ok(None);
        }
        if !option_path.starts_with(path_components) {
            continue;
        }
        if option_path.len() == path_components.len() {
            exact_target = true;
            continue;
        }
        let child = option_path[path_components.len()].clone();
        let state = children.entry(child).or_default();
        if option_path.len() == path_components.len() + 1 {
            state.0 = true;
        } else {
            state.1 = true;
        }
    }
    if (kind == ConfigObservationKind::Prefix && (exact_target || children.is_empty()))
        || children
            .values()
            .any(|(is_option, is_prefix)| *is_option && *is_prefix)
    {
        return Ok(None);
    }

    let total_children = children.len();
    let offset = usize::try_from(child_offset)?;
    let page = children
        .into_iter()
        .skip(offset)
        .take(crate::models::config_observations::MAX_CONFIG_OBSERVATION_ITEMS)
        .map(|(name, (is_option, _))| {
            let mut child_path = path_components.to_vec();
            child_path.push(name);
            json!({
                "key": option_key(&child_path),
                "kind": if is_option { "option" } else { "prefix" },
                "path_components": child_path,
            })
        })
        .collect::<Vec<_>>();
    Ok(Some(json!({
        "kind": kind.as_str(),
        "path_components": path_components,
        "child_offset": child_offset,
        "children": page,
        "children_truncated": total_children > offset.saturating_add(crate::models::config_observations::MAX_CONFIG_OBSERVATION_ITEMS),
        "total_children": total_children,
    })))
}

fn adapt_v2_exact_option(
    kind: ConfigObservationKind,
    path_components: &[String],
    option: &ConfigOptionArtifactV2,
) -> Result<Option<Value>> {
    if option.path_components != path_components || option.option_key != option_key(path_components)
    {
        return Ok(None);
    }
    match kind {
        ConfigObservationKind::Option => {
            let ConfigOptionMetadataArtifactV2::Available {
                declared_type,
                highest_prio,
                is_defined,
                ..
            } = &option.metadata
            else {
                return Ok(None);
            };
            Ok(Some(json!({
                "kind": "option",
                "path_components": path_components,
                "key": option.option_key,
                "declared_type": declared_type,
                "is_defined": is_defined,
                "highest_prio": highest_prio,
                "value": serde_json::to_value(&option.effective_value)?,
            })))
        }
        ConfigObservationKind::Provenance => {
            let ConfigOptionMetadataArtifactV2::Available {
                surviving_definition_sources,
                ..
            } = &option.metadata
            else {
                return Ok(None);
            };
            // COMPATIBILITY: Scoped provenance represents the module system's
            // definitionsWithLocations survivors. V2 raw provenance also
            // contains priority-discarded definitions and must not be projected
            // into this narrower contract.
            let total_definitions = surviving_definition_sources.len();
            let definitions = surviving_definition_sources
                .iter()
                .take(crate::models::config_observations::MAX_CONFIG_OBSERVATION_ITEMS)
                .map(|definition| {
                    json!({
                        "source_path": definition.source_path,
                        "priority": definition.priority,
                    })
                })
                .collect::<Vec<_>>();
            let definitions_truncated = total_definitions > definitions.len();
            Ok(Some(json!({
                "kind": "provenance",
                "path_components": path_components,
                "key": option.option_key,
                "definitions": definitions,
                "definitions_truncated": definitions_truncated,
                "total_definitions": total_definitions,
            })))
        }
        _ => Ok(None),
    }
}

async fn persist_adapted_v2_observation_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    derivation_id: i32,
    configuration_name: &str,
    carrier_drv_path: &str,
    kind: ConfigObservationKind,
    path_components: &[String],
    child_offset: u32,
    payload: &Value,
) -> Result<sqlx::postgres::PgRow> {
    validate_config_observation_payload(kind, path_components, child_offset, payload)?;
    let payload_bytes = serde_json::to_vec(payload)?;
    if payload_bytes.len() > 8 * 1024 * 1024 {
        bail!("adapted Config observation payload exceeds its persistence contract");
    }
    let digest = Sha256::digest(&payload_bytes).to_vec();
    sqlx::query(
        "INSERT INTO config_observation_contents (digest, schema_version, payload) VALUES ($1, $2, $3) ON CONFLICT (digest) DO NOTHING",
    )
    .bind(&digest)
    .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .context("persist adapted V2 observation content")?;
    let observation_id: Uuid = sqlx::query_scalar(
        r#"
        WITH inserted AS (
            INSERT INTO config_observations (
                commit_id, derivation_id, configuration_name, carrier_drv_path,
                schema_version, path_components, kind, child_offset, content_digest
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (
                commit_id, configuration_name, derivation_id, carrier_drv_path,
                schema_version, path_components, kind, child_offset
            ) DO NOTHING
            RETURNING id
        )
        SELECT id FROM inserted
        UNION ALL
        SELECT id FROM config_observations
        WHERE commit_id = $1 AND derivation_id = $2
          AND configuration_name = $3 AND carrier_drv_path = $4
          AND schema_version = $5 AND path_components = $6 AND kind = $7
          AND child_offset = $8
        LIMIT 1
        "#,
    )
    .bind(commit_id)
    .bind(derivation_id)
    .bind(configuration_name)
    .bind(carrier_drv_path)
    .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
    .bind(serde_json::to_value(path_components)?)
    .bind(kind.as_str())
    .bind(i32::try_from(child_offset)?)
    .bind(&digest)
    .fetch_one(&mut **tx)
    .await
    .context("persist adapted V2 immutable observation")?;
    sqlx::query(
        r#"
        INSERT INTO config_observation_requests (
            commit_id, derivation_id, configuration_name, carrier_drv_path,
            schema_version, path_components, kind, child_offset, priority,
            status, attempts, observation_id, completed_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9,
                  'succeeded', 0, $10, now())
        RETURNING id, status, attempts, execution_heartbeat_at, observation_id, error
        "#,
    )
    .bind(commit_id)
    .bind(derivation_id)
    .bind(configuration_name)
    .bind(carrier_drv_path)
    .bind(CONFIG_OBSERVATION_SCHEMA_VERSION)
    .bind(serde_json::to_value(path_components)?)
    .bind(kind.as_str())
    .bind(i32::try_from(child_offset)?)
    .bind(kind.priority())
    .bind(observation_id)
    .fetch_one(&mut **tx)
    .await
    .context("persist adapted V2 succeeded request")
}

fn request_response_from_row(
    row: sqlx::postgres::PgRow,
    revision: &str,
    configuration_name: &str,
    kind: ConfigObservationKind,
    path_components: &[String],
    child_offset: u32,
    reused: bool,
) -> Result<ConfigObservationRequestResponse> {
    Ok(ConfigObservationRequestResponse {
        request_id: row.try_get("id")?,
        revision: revision.to_string(),
        configuration_name: configuration_name.to_string(),
        kind,
        path_components: path_components.to_vec(),
        child_offset,
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
                request.child_offset,
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
        let child_offset = u32::try_from(row.try_get::<i32, _>("child_offset")?)?;
        request_response_from_row(
            row,
            &revision,
            &configuration_name,
            kind,
            &path,
            child_offset,
            false,
        )
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
                observation.child_offset,
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
        let child_offset = u32::try_from(row.try_get::<i32, _>("child_offset")?)?;
        let mut payload: Value = row.try_get("payload")?;
        normalize_legacy_tree_payload(kind, child_offset, &mut payload);
        validate_config_observation_payload(kind, &path_components, child_offset, &payload)
            .context("validate stored Config observation payload")?;
        Ok(ConfigObservationResponse {
            observation_id: row.try_get("id")?,
            revision: row.try_get("revision")?,
            configuration_name: row.try_get("configuration_name")?,
            schema_version: row.try_get("schema_version")?,
            kind,
            path_components,
            child_offset,
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
                  , request.child_offset
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
        child_offset: u32::try_from(row.try_get::<i32, _>("child_offset")?)?,
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
        execution.target.child_offset,
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
                schema_version, path_components, kind, child_offset, content_digest
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (
                commit_id, configuration_name, derivation_id, carrier_drv_path,
                schema_version, path_components, kind, child_offset
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
          AND child_offset = $8
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
    .bind(i32::try_from(execution.target.child_offset)?)
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
    use crate::models::config_inspector::OptionInventoryDiagnostic;
    use crate::models::config_snapshot_artifact::{
        ConfigDefinitionArtifactV2, ConfigDefinitionSourceArtifactV2, ConfigDefinitionStatusV2,
        ConfigInspectionArtifactV2, ConfigOptionMetadataArtifactV2,
        ConfigOptionProvenanceArtifactV2, ConfigProvenanceArtifactStateV2,
        DefinitionValueArtifactStateV2,
    };
    use crate::models::evaluation_snapshots::SafeOptionValue;

    fn v2_identity(path: &[&str]) -> (String, Vec<String>) {
        let path = path
            .iter()
            .map(|part| (*part).to_string())
            .collect::<Vec<_>>();
        (option_key(&path), path)
    }

    #[test]
    fn legacy_tree_payload_projects_as_offset_zero_without_mutating_storage() {
        let path = vec!["services".to_string()];
        let child_path = vec!["services".to_string(), "nginx".to_string()];
        let stored = json!({
            "kind": "prefix",
            "path_components": path,
            "children": [{
                "path_components": child_path,
                "key": option_key(&child_path),
                "kind": "option"
            }],
            "children_truncated": false,
            "total_children": 1
        });
        let mut projected = stored.clone();

        normalize_legacy_tree_payload(ConfigObservationKind::Prefix, 0, &mut projected);

        assert_eq!(projected["child_offset"], 0);
        assert!(stored.get("child_offset").is_none());
        validate_config_observation_payload(ConfigObservationKind::Prefix, &path, 0, &projected)
            .expect("legacy offset-zero payload should remain reusable");
    }

    fn persisted_v2_option(
        metadata: Value,
        effective_value: Value,
        provenance: Value,
    ) -> ConfigOptionArtifactV2 {
        let path = vec!["services".to_string(), "example".to_string()];
        config_option_v2_from_persisted(
            option_key(&path),
            path,
            json!({
                "metadata": metadata,
                "effective_value": effective_value,
                "provenance": provenance,
            }),
        )
        .expect("fixture option should decode")
    }

    fn available_v2_metadata() -> Value {
        json!({
            "state": "available",
            "option_type": "option",
            "loc": ["services", "example"],
            "declared_type": "boolean",
            "declarations": ["/flake/module.nix"],
            "declaration_positions": [],
            "highest_prio": 100,
            "is_defined": true,
            "surviving_definition_sources": [{
                "source_path": "/flake/module.nix",
                "priority": 100,
                "source_input": "self",
                "source_revision": "a"
            }]
        })
    }

    fn available_v2_provenance() -> Value {
        json!({
            "state": "available",
            "definitions": [{
                "ordinal": 0,
                "source_path": "/flake/module.nix",
                "source_input": "self",
                "source_revision": "a",
                "module_key": "module",
                "priority": 100,
                "status": "active_surviving",
                "surviving_merge_order": 0,
                "value": {"kind": "scalar", "value": true}
            }],
            "override_state": false
        })
    }

    #[test]
    fn complete_v2_tree_adapter_derives_exact_immediate_children_and_pages() {
        let mut identities = (0..1469)
            .map(|index| v2_identity(&["services", &format!("option-{index:04}")]))
            .collect::<Vec<_>>();
        identities.push(v2_identity(&["networking", "hostName"]));

        let root = adapt_v2_tree_page(true, ConfigObservationKind::Root, &[], 0, &identities)
            .unwrap()
            .expect("complete V2 should satisfy root");
        assert_eq!(root["total_children"], 2);
        assert_eq!(
            root["children"][0]["path_components"],
            json!(["networking"])
        );
        assert_eq!(root["children"][1]["path_components"], json!(["services"]));

        let path = vec!["services".to_string()];
        let middle =
            adapt_v2_tree_page(true, ConfigObservationKind::Prefix, &path, 512, &identities)
                .unwrap()
                .expect("complete V2 should satisfy a prefix page");
        assert_eq!(middle["child_offset"], 512);
        assert_eq!(middle["total_children"], 1469);
        assert_eq!(middle["children"].as_array().unwrap().len(), 512);
        assert_eq!(middle["children"][0]["path_components"][1], "option-0512");
        assert_eq!(middle["children_truncated"], true);

        let final_page = adapt_v2_tree_page(
            true,
            ConfigObservationKind::Prefix,
            &path,
            1024,
            &identities,
        )
        .unwrap()
        .expect("complete V2 should satisfy the final prefix page");
        assert_eq!(final_page["children"].as_array().unwrap().len(), 445);
        assert_eq!(final_page["children_truncated"], false);
        validate_config_observation_payload(
            ConfigObservationKind::Prefix,
            &path,
            1024,
            &final_page,
        )
        .expect("derived final page should satisfy the scoped contract");
    }

    #[test]
    fn v2_tree_adapter_refuses_partial_and_structural_collisions() {
        let path = vec!["services".to_string()];
        let identities = vec![v2_identity(&["services", "nginx"])];
        assert!(
            adapt_v2_tree_page(false, ConfigObservationKind::Prefix, &path, 0, &identities,)
                .unwrap()
                .is_none()
        );

        let collision = vec![
            v2_identity(&["services", "collision"]),
            v2_identity(&["services", "collision", "nested"]),
        ];
        assert!(
            adapt_v2_tree_page(true, ConfigObservationKind::Prefix, &path, 0, &collision,)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn complete_or_partial_v2_exact_adapter_requires_truthful_metadata() {
        let path = vec!["services".to_string(), "example".to_string()];
        let option = persisted_v2_option(
            available_v2_metadata(),
            json!({
                "kind": "failed",
                "value": {"code": "not_evaluated", "message": "Value was not evaluated"}
            }),
            available_v2_provenance(),
        );
        let detail = adapt_v2_exact_option(ConfigObservationKind::Option, &path, &option)
            .unwrap()
            .expect("available metadata should adapt exact failed value");
        validate_config_observation_payload(ConfigObservationKind::Option, &path, 0, &detail)
            .expect("failed SafeOptionValue should remain truthful");
        let provenance = adapt_v2_exact_option(ConfigObservationKind::Provenance, &path, &option)
            .unwrap()
            .expect("available exact provenance should adapt");
        validate_config_observation_payload(
            ConfigObservationKind::Provenance,
            &path,
            0,
            &provenance,
        )
        .expect("adapted provenance should validate");

        let failed_metadata = persisted_v2_option(
            json!({
                "state": "failed",
                "error": {"code": "metadata_failed", "message": "Metadata failed"}
            }),
            json!({"kind": "scalar", "value": true}),
            available_v2_provenance(),
        );
        assert!(
            adapt_v2_exact_option(ConfigObservationKind::Option, &path, &failed_metadata)
                .unwrap()
                .is_none()
        );
        assert!(
            adapt_v2_exact_option(ConfigObservationKind::Provenance, &path, &failed_metadata)
                .unwrap()
                .is_none()
        );
    }

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

    struct V2ObservationFixture {
        system_id: Uuid,
        commit_id: i32,
        revision: String,
        configuration_name: String,
        carrier_drv_path: String,
        snapshot_id: Uuid,
    }

    async fn insert_v2_observation_fixture(
        pool: &PgPool,
        complete: bool,
        service_count: usize,
        target_matches: bool,
    ) -> V2ObservationFixture {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/v2-observation-{suffix}.git");
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("v2-observation-{suffix}"))
        .bind(&repo_url)
        .fetch_one(pool)
        .await
        .unwrap();
        let revision = format!("{:0>40}", &suffix[..32]);
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_status) VALUES ($1, $2, now(), 'complete') RETURNING id",
        )
        .bind(flake_id)
        .bind(&revision)
        .fetch_one(pool)
        .await
        .unwrap();
        let configuration_name = format!("config-{suffix}");
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, flake_id, derivation, system_configuration_name) VALUES ($1, 'test-key', $2, '', $3) RETURNING id",
        )
        .bind(format!("host-{suffix}"))
        .bind(flake_id)
        .bind(&configuration_name)
        .fetch_one(pool)
        .await
        .unwrap();
        let carrier_drv_path = format!("/nix/store/{suffix}-{configuration_name}.drv");
        sqlx::query(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, completed_at) VALUES ($1, 'nixos', $2, $3, 5, now())",
        )
        .bind(commit_id)
        .bind(&configuration_name)
        .bind(&carrier_drv_path)
        .execute(pool)
        .await
        .unwrap();

        let mut paths = (0..service_count)
            .map(|index| vec!["services".to_string(), format!("service-{index:04}")])
            .collect::<Vec<_>>();
        paths.push(vec!["networking".to_string(), "hostName".to_string()]);
        let options = paths
            .into_iter()
            .map(|path_components| {
                let option_key = option_key(&path_components);
                let is_example = path_components == ["services", "service-0000"];
                ConfigOptionArtifactV2 {
                    option_key: option_key.clone(),
                    path_components: path_components.clone(),
                    metadata: ConfigOptionMetadataArtifactV2::Available {
                        option_type: Some("option".to_string()),
                        loc: path_components,
                        declared_type: Some("boolean".to_string()),
                        declarations: vec!["/flake/module.nix".to_string()],
                        declaration_positions: Vec::new(),
                        highest_prio: Some(100),
                        is_defined: true,
                        surviving_definition_sources: vec![ConfigDefinitionSourceArtifactV2 {
                            source_path: "/flake/module.nix".to_string(),
                            priority: Some(100),
                            source_input: Some("self".to_string()),
                            source_revision: Some(revision.clone()),
                        }],
                    },
                    effective_value: SafeOptionValue::Scalar(json!(true)),
                    provenance: ConfigOptionProvenanceArtifactV2::Available {
                        definitions: is_example
                            .then(|| ConfigDefinitionArtifactV2 {
                                option_key,
                                ordinal: 0,
                                source_path: Some("/flake/module.nix".to_string()),
                                source_input: Some("self".to_string()),
                                source_revision: Some(revision.clone()),
                                module_key: Some("module".to_string()),
                                priority: 100,
                                status: ConfigDefinitionStatusV2::ActiveSurviving,
                                surviving_merge_order: Some(0),
                                value: Some(SafeOptionValue::Scalar(json!(true))),
                            })
                            .into_iter()
                            .collect(),
                        override_state: false,
                    },
                }
            })
            .collect();
        let provenance_digest = "b".repeat(64);
        let artifact = ConfigInspectionArtifactV2 {
            artifact_version: 2,
            target_key: if target_matches {
                InspectionTarget::new(
                    &crate::derivations::utils::build_flake_reference(&repo_url, &revision),
                    &configuration_name,
                )
                .target_key
            } else {
                "c".repeat(64)
            },
            source_out_path: format!("/nix/store/{suffix}-source"),
            carrier_drv_path: carrier_drv_path.clone(),
            option_inventory_complete: complete,
            option_inventory_diagnostics: if complete {
                Vec::new()
            } else {
                vec![OptionInventoryDiagnostic {
                    path: vec!["unreadable".to_string()],
                    code: "unreadable_option_subtree".to_string(),
                    message: "Option subtree could not be inspected".to_string(),
                }]
            },
            option_inventory_diagnostics_truncated: false,
            provenance_state: ConfigProvenanceArtifactStateV2::Available {
                adapter_version: 1,
                target_lib_version: None,
                target_module_system_path: None,
                provenance_digest: provenance_digest.clone(),
                definition_value_enrichment: DefinitionValueArtifactStateV2::Available {
                    adapter_version: 1,
                    provenance_digest,
                },
            },
            options,
        };
        let mut tx = pool.begin().await.unwrap();
        let snapshot_id = crate::queries::evaluation_snapshots::persist_config_artifact_v2_tx(
            &mut tx,
            commit_id,
            &configuration_name,
            artifact,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        V2ObservationFixture {
            system_id,
            commit_id,
            revision,
            configuration_name,
            carrier_drv_path,
            snapshot_id,
        }
    }

    async fn resolved_request(
        pool: &PgPool,
        fixture: &V2ObservationFixture,
        kind: ConfigObservationKind,
        path: &[String],
        child_offset: u32,
    ) -> ConfigObservationRequestResponse {
        let CreateConfigObservationOutcome::Resolved(response) =
            create_or_reuse_config_observation_request(
                pool,
                fixture.system_id,
                &fixture.revision,
                kind,
                path,
                child_offset,
            )
            .await
            .unwrap()
        else {
            panic!("fixture request should resolve");
        };
        response
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn certified_v2_adapter_preserves_queue_and_selector_isolation(pool: PgPool) {
        let complete = insert_v2_observation_fixture(&pool, true, 520, true).await;
        let selector_before: Uuid = sqlx::query_scalar(
            "SELECT current_snapshot_id FROM config_snapshot_selections WHERE commit_id = $1 AND configuration_name = $2",
        )
        .bind(complete.commit_id)
        .bind(&complete.configuration_name)
        .fetch_one(&pool)
        .await
        .unwrap();

        let root = resolved_request(&pool, &complete, ConfigObservationKind::Root, &[], 0).await;
        let prefix_path = vec!["services".to_string()];
        let prefix = resolved_request(
            &pool,
            &complete,
            ConfigObservationKind::Prefix,
            &prefix_path,
            512,
        )
        .await;
        let prefix_first = resolved_request(
            &pool,
            &complete,
            ConfigObservationKind::Prefix,
            &prefix_path,
            0,
        )
        .await;
        assert_ne!(prefix.request_id, prefix_first.request_id);
        assert_ne!(prefix.observation_id, prefix_first.observation_id);
        let option_path = vec!["services".to_string(), "service-0000".to_string()];
        let option = resolved_request(
            &pool,
            &complete,
            ConfigObservationKind::Option,
            &option_path,
            0,
        )
        .await;
        let provenance = resolved_request(
            &pool,
            &complete,
            ConfigObservationKind::Provenance,
            &option_path,
            0,
        )
        .await;
        for request in [&root, &prefix, &prefix_first, &option, &provenance] {
            assert_eq!(request.lifecycle, ConfigObservationLifecycle::Succeeded);
            assert_eq!(request.attempts, 0);
            assert!(request.observation_id.is_some());
        }
        let prefix_observation =
            get_config_observation(&pool, complete.system_id, prefix.observation_id.unwrap())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(prefix_observation.child_offset, 512);
        assert_eq!(prefix_observation.payload["total_children"], 520);
        assert_eq!(
            prefix_observation.payload["children"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(prefix_observation.payload["children_truncated"], false);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM config_observation_requests WHERE status IN ('queued', 'waiting_for_capacity', 'running')",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM config_inspection_jobs")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(selector_before, complete.snapshot_id);
        assert_eq!(
            sqlx::query_scalar::<_, Uuid>(
                "SELECT current_snapshot_id FROM config_snapshot_selections WHERE commit_id = $1 AND configuration_name = $2",
            )
            .bind(complete.commit_id)
            .bind(&complete.configuration_name)
            .fetch_one(&pool)
            .await
            .unwrap(),
            selector_before
        );

        let configured = resolved_request(
            &pool,
            &complete,
            ConfigObservationKind::ConfiguredIndex,
            &[],
            0,
        )
        .await;
        assert_eq!(configured.lifecycle, ConfigObservationLifecycle::Queued);
        assert_eq!(configured.attempts, 0);

        let partial = insert_v2_observation_fixture(&pool, false, 1, true).await;
        let partial_tree =
            resolved_request(&pool, &partial, ConfigObservationKind::Root, &[], 0).await;
        assert_eq!(partial_tree.lifecycle, ConfigObservationLifecycle::Queued);
        let partial_path = vec!["services".to_string(), "service-0000".to_string()];
        for kind in [
            ConfigObservationKind::Option,
            ConfigObservationKind::Provenance,
        ] {
            let request = resolved_request(&pool, &partial, kind, &partial_path, 0).await;
            assert_eq!(request.lifecycle, ConfigObservationLifecycle::Succeeded);
            assert_eq!(request.attempts, 0);
        }

        let carrier_mismatch = insert_v2_observation_fixture(&pool, true, 1, true).await;
        sqlx::query("UPDATE derivations SET derivation_path = $2 WHERE commit_id = $1")
            .bind(carrier_mismatch.commit_id)
            .bind(format!("{}-replacement", carrier_mismatch.carrier_drv_path))
            .execute(&pool)
            .await
            .unwrap();
        let carrier_mismatch_path = vec!["networking".to_string(), "hostName".to_string()];
        let carrier_mismatch_request = resolved_request(
            &pool,
            &carrier_mismatch,
            ConfigObservationKind::Option,
            &carrier_mismatch_path,
            0,
        )
        .await;
        assert_eq!(
            carrier_mismatch_request.lifecycle,
            ConfigObservationLifecycle::Queued
        );
        let target_mismatch = insert_v2_observation_fixture(&pool, true, 1, false).await;
        let target_mismatch_request =
            resolved_request(&pool, &target_mismatch, ConfigObservationKind::Root, &[], 0).await;
        assert_eq!(
            target_mismatch_request.lifecycle,
            ConfigObservationLifecycle::Queued
        );
        assert!(matches!(
            create_or_reuse_config_observation_request(
                &pool,
                complete.system_id,
                &"f".repeat(40),
                ConfigObservationKind::Root,
                &[],
                0,
            )
            .await
            .unwrap(),
            CreateConfigObservationOutcome::NotFound
        ));
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
            0,
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
            0,
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
                "child_offset": 0,
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
            0,
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
            0,
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
