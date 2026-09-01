//! Persists and queries revision-specific evaluation snapshots.
//!
//! Every read in this module is database-only. Nix, Git, and network work must
//! remain in the authorized evaluation job path.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Executor, PgPool, Postgres, QueryBuilder, Row, Transaction};
use uuid::Uuid;

use crate::models::evaluation_snapshots::{
    AgentFingerprintStatus, EvaluatedOption, EvaluatedOptionCounts, EvaluatedOptionFilter,
    EvaluatedOptionRow, EvaluatedOptionsPage, EvaluationDrift, EvaluationModuleSourcesPage,
    EvaluationModuleSummary, FlakeModuleDeclarationsPage, FlakeOutputPagination,
    FlakeOutputSnapshotResponse, FlakeSystemFilter, QueueEvaluationResponse, ReconciledFlakeSystem,
    SelectedEvaluationSummary, SevenDayDriftStatus, SnapshotLifecycle, TrackedFlakeIdentity,
    flake_output_delta, typed_option_diff,
};

/// Maximum option rows returned by one request.
pub const OPTIONS_PAGE_LIMIT: i64 = 100;
/// Maximum offset accepted by the options endpoint.
pub const OPTIONS_OFFSET_LIMIT: i64 = 100_000;
/// Maximum literal search length accepted by the options query.
pub const OPTIONS_SEARCH_LIMIT: usize = 256;
/// Maximum rows returned from each flake-output collection.
pub const FLAKE_OUTPUT_PAGE_LIMIT: usize = 100;
/// Maximum declaration offset accepted by the exported-module endpoint.
pub const FLAKE_MODULE_DECLARATION_OFFSET_LIMIT: usize = 100_000;
/// Maximum encoded bytes accepted for one option payload.
pub const OPTION_CONTENT_BYTES_LIMIT: usize = 256 * 1024;
/// Maximum encoded bytes accepted for one complete configuration snapshot.
pub const SNAPSHOT_CONTENT_BYTES_LIMIT: i64 = 64 * 1024 * 1024;
/// Maximum encoded bytes accepted for one revision-wide flake output snapshot.
pub const FLAKE_OUTPUT_CONTENT_BYTES_LIMIT: usize = 8 * 1024 * 1024;
/// Maximum encoded bytes returned by one flake-output response.
pub const FLAKE_OUTPUT_RESPONSE_BYTES_LIMIT: usize = 2 * 1024 * 1024;
/// Stable error marker returned when a supplied flake-output token is stale or malformed.
pub const FLAKE_OUTPUT_SNAPSHOT_CHANGED: &str = "flake output snapshot changed";

/// Identifies a persisted snapshot selected for a system and revision.
#[derive(Debug, Clone)]
pub struct SelectedEvaluationSnapshot {
    /// Snapshot identifier.
    pub id: Uuid,
    /// Full selected commit SHA.
    pub revision: String,
    /// Explicit persisted lifecycle.
    pub lifecycle: SnapshotLifecycle,
    /// Safe persisted evaluation error.
    pub error: Option<String>,
    /// Baseline snapshot identifier when comparison is valid.
    pub baseline_id: Option<Uuid>,
    /// Full baseline SHA when comparison is valid.
    pub baseline_revision: Option<String>,
    /// Selected generation in generation mode.
    pub generation: Option<i32>,
    /// Durable generation-snapshot identity.
    pub generation_snapshot_id: Option<Uuid>,
    /// Number of distinct module sources in the snapshot.
    pub module_count: i64,
    /// Persisted evaluator duration in milliseconds.
    pub evaluation_duration_ms: Option<i64>,
}

/// Classifies a database-only exported-module declaration lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum FlakeModuleDeclarationsQuery {
    /// The revision and module produced a lifecycle-aware page.
    Page(FlakeModuleDeclarationsPage),
    /// The active revision or exported module does not exist.
    NotFound,
    /// The supplied continuation token no longer identifies the selected snapshot.
    SnapshotChanged,
}

/// Classifies a database-only evaluation module-source lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationModuleSourcesQuery {
    /// The selected snapshot produced a lifecycle-aware page.
    Page(EvaluationModuleSourcesPage),
    /// The continuation token identifies an older snapshot replacement.
    SnapshotChanged,
}

/// Inserts a complete available snapshot in the caller's evaluation transaction.
///
/// Content rows are shared by digest. Replacing a snapshot updates only its
/// lightweight references and does not duplicate shared option payloads.
///
/// # Errors
///
/// Returns an error when an option cannot be serialized or PostgreSQL cannot
/// persist the snapshot in the caller's transaction.
pub async fn persist_available_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    configuration_name: &str,
    options: Vec<EvaluatedOption>,
) -> Result<Uuid> {
    let snapshot_id =
        persist_available_snapshot_deferred_tx(tx, commit_id, configuration_name, options).await?;
    recompute_host_deltas_tx(tx, commit_id).await?;
    Ok(snapshot_id)
}

/// Persists one available snapshot without recomputing commit-wide host deltas.
///
/// The bulk finalizer uses this function for each configuration, then invokes
/// [`recompute_host_deltas_tx`] exactly once in the same transaction. This keeps
/// finalization atomic without repeating a complete corpus scan per configuration.
///
/// # Errors
///
/// Returns an error when serialization or persistence fails.
pub(crate) async fn persist_available_snapshot_deferred_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    configuration_name: &str,
    options: Vec<EvaluatedOption>,
) -> Result<Uuid> {
    // SECURITY: Count the exact tuples that will be persisted. Redaction can
    // collapse credential-bearing source metadata, and response-only tracked
    // identities must not affect the durable scalar.
    let options = options
        .into_iter()
        .map(EvaluatedOption::redacted)
        .map(bounded_option_payload)
        .collect::<Result<Vec<_>>>()?;
    let module_count = options
        .iter()
        .flat_map(|option| {
            option.definitions.iter().map(|definition| {
                (
                    definition.source_input.as_deref(),
                    definition.source_revision.as_deref(),
                    definition.source_path.as_str(),
                )
            })
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let snapshot_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO evaluation_snapshots (
            commit_id, configuration_name, lifecycle, first_parent_sha,
            option_count, module_count, evaluation_duration_ms, completed_at
        )
        SELECT $1, $2, 'available', c.first_parent_sha, $3, $4,
               GREATEST(0, (EXTRACT(EPOCH FROM (now() - c.evaluation_started_at)) * 1000)::bigint),
               now()
        FROM commits c
        WHERE c.id = $1
        ON CONFLICT (commit_id, configuration_name) DO UPDATE
        SET lifecycle = 'available',
            first_parent_sha = EXCLUDED.first_parent_sha,
            error = NULL,
            option_count = EXCLUDED.option_count,
            module_count = EXCLUDED.module_count,
            evaluation_duration_ms = EXCLUDED.evaluation_duration_ms,
            completed_at = now()
        RETURNING id
        "#,
    )
    .bind(commit_id)
    .bind(configuration_name)
    .bind(i32::try_from(options.len()).context("snapshot option count exceeds i32")?)
    .bind(i32::try_from(module_count).context("snapshot module count exceeds i32")?)
    .fetch_one(&mut **tx)
    .await?;

    sqlx::query("DELETE FROM evaluation_snapshot_options WHERE snapshot_id = $1")
        .bind(snapshot_id)
        .execute(&mut **tx)
        .await?;

    let mut content_bytes = 0_i64;
    for option in options {
        let digest = option.content_digest();
        let search_text = option.search_text();
        let payload = serde_json::json!({
            "declared_type": option.declared_type,
            "value": option.value,
            "definitions": option.definitions,
            "overridden": option.overridden,
        });
        let payload_bytes = serde_json::to_vec(&payload)?.len();
        content_bytes = content_bytes.saturating_add(
            i64::try_from(payload_bytes).context("option payload size exceeds i64")?,
        );
        if content_bytes > SNAPSHOT_CONTENT_BYTES_LIMIT {
            sqlx::query("DELETE FROM evaluation_snapshot_options WHERE snapshot_id = $1")
                .bind(snapshot_id)
                .execute(&mut **tx)
                .await?;
            sqlx::query(
                "UPDATE evaluation_snapshots
                 SET lifecycle = 'unavailable', option_count = 0, module_count = 0,
                     content_bytes = 0, completed_at = NOW()
                 WHERE id = $1",
            )
            .bind(snapshot_id)
            .execute(&mut **tx)
            .await?;
            return Ok(snapshot_id);
        }
        sqlx::query(
            r#"
            INSERT INTO evaluation_option_contents (digest, payload, search_text)
            VALUES ($1, $2, $3)
            ON CONFLICT (digest) DO NOTHING
            "#,
        )
        .bind(digest.as_slice())
        .bind(payload)
        .bind(search_text)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO evaluation_snapshot_options (
                snapshot_id, option_path, content_digest, is_overridden
            )
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(snapshot_id)
        .bind(option.path)
        .bind(digest.as_slice())
        .bind(option.overridden)
        .execute(&mut **tx)
        .await?;
    }

    sqlx::query("UPDATE evaluation_snapshots SET content_bytes = $2 WHERE id = $1")
        .bind(snapshot_id)
        .bind(content_bytes)
        .execute(&mut **tx)
        .await?;

    // INVARIANT: A generation can be observed before or after its commit
    // snapshot is finalized. This side of the link handles the former order;
    // retain_generation_snapshot_tx handles the latter.
    sqlx::query(
        r#"
        INSERT INTO evaluation_generation_snapshots (
            system_id, generation, snapshot_id, derivation_id, commit_id, source_store_path
        )
        SELECT DISTINCT ON (s.id, ss.generation)
               s.id, ss.generation, $1, d.id, d.commit_id, ss.store_path
        FROM systems s
        JOIN system_states ss ON ss.hostname = s.hostname
        JOIN LATERAL (
          SELECT pending.requested_commit_id
          FROM pending_system_deployments pending
          WHERE pending.system_id = s.id
            AND pending.target_store_path = ss.store_path
            AND pending.status IN ('pending', 'succeeded')
          ORDER BY pending.issued_at DESC, pending.id DESC
          LIMIT 1
        ) deployment ON true
        JOIN derivations d
          ON d.commit_id = deployment.requested_commit_id
         AND d.derivation_type = 'nixos'
         AND d.derivation_name = $3
         AND ss.store_path = COALESCE(d.store_path, d.expected_store_path)
        WHERE ss.generation IS NOT NULL
          AND ss.store_path IS NOT NULL
          AND deployment.requested_commit_id = $2
          AND COALESCE(NULLIF(btrim(s.system_configuration_name), ''), s.hostname) = $3
        ORDER BY s.id, ss.generation, ss.timestamp ASC
        ON CONFLICT (system_id, generation) DO NOTHING
        "#,
    )
    .bind(snapshot_id)
    .bind(commit_id)
    .bind(configuration_name)
    .execute(&mut **tx)
    .await?;

    Ok(snapshot_id)
}

fn bounded_option_payload(mut option: EvaluatedOption) -> Result<EvaluatedOption> {
    let encoded_len = serde_json::to_vec(&serde_json::json!({
        "declared_type": option.declared_type,
        "value": option.value,
        "definitions": option.definitions,
        "overridden": option.overridden,
    }))?
    .len();
    if encoded_len <= OPTION_CONTENT_BYTES_LIMIT {
        return Ok(option);
    }

    option.declared_type = "unknown (oversized evaluator payload)".to_string();
    option.value = crate::models::evaluation_snapshots::SafeOptionValue::Opaque {
        type_name: "oversized".to_string(),
    };
    option.definitions.clear();
    option.overridden = false;
    Ok(option)
}

/// Persists a redacted failed lifecycle for one configuration.
///
/// # Errors
///
/// Returns an error when the commit does not exist or PostgreSQL rejects the
/// lifecycle update. The diagnostic is redacted before the first write.
pub async fn persist_failed_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    configuration_name: &str,
    error: &str,
) -> Result<()> {
    persist_failed_snapshot_deferred_tx(tx, commit_id, configuration_name, error).await?;
    recompute_host_deltas_tx(tx, commit_id).await
}

/// Persists one failed snapshot without recomputing commit-wide host deltas.
///
/// # Errors
///
/// Returns an error when PostgreSQL rejects the lifecycle update.
pub(crate) async fn persist_failed_snapshot_deferred_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    configuration_name: &str,
    error: &str,
) -> Result<()> {
    let error = crate::security::snapshot_redaction::redact_evaluation_error(error);
    sqlx::query(
        r#"
        INSERT INTO evaluation_snapshots (
            commit_id, configuration_name, lifecycle, first_parent_sha, error,
            completed_at
        )
        SELECT $1, $2, 'failed', first_parent_sha, $3, now()
        FROM commits WHERE id = $1
        ON CONFLICT (commit_id, configuration_name) DO UPDATE
        SET lifecycle = 'failed', error = EXCLUDED.error,
            first_parent_sha = EXCLUDED.first_parent_sha,
            option_count = 0, module_count = 0, content_bytes = 0,
            completed_at = now()
        "#,
    )
    .bind(commit_id)
    .bind(configuration_name)
    .bind(error)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Recomputes every materialized host delta affected by one snapshot mutation.
///
/// INVARIANT: Callers invoke this function in the same transaction that changes
/// snapshot metadata or option references. Readers therefore cannot observe a
/// new corpus with stale modal counts.
pub(crate) async fn recompute_host_deltas_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
) -> Result<()> {
    sqlx::query("SELECT recompute_evaluation_host_deltas($1)")
        .bind(commit_id)
        .execute(&mut **tx)
        .await
        .context("failed to recompute materialized evaluation host deltas")?;
    Ok(())
}

/// Persists one content-addressed flake output snapshot per commit.
///
/// The function redacts evaluator output before it calculates the digest or
/// writes data. The caller owns the evaluation finalization transaction.
///
/// # Errors
///
/// Returns an error when the payload cannot be serialized or PostgreSQL cannot
/// persist the snapshot in the caller's transaction.
pub async fn persist_flake_output_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
    payload: &Value,
) -> Result<()> {
    let payload = crate::security::snapshot_redaction::redact_flake_output(payload);
    let bytes = serde_json::to_vec(&payload).context("failed to encode flake output snapshot")?;
    if bytes.len() > FLAKE_OUTPUT_CONTENT_BYTES_LIMIT {
        sqlx::query(
            r#"
            INSERT INTO flake_output_snapshots (
                commit_id, lifecycle, first_parent_sha, content_digest, completed_at
            )
            SELECT $1, 'unavailable', c.first_parent_sha, NULL, now()
            FROM commits c WHERE c.id = $1
            ON CONFLICT (commit_id) DO UPDATE
            SET lifecycle = 'unavailable', first_parent_sha = EXCLUDED.first_parent_sha,
                content_digest = NULL, error = NULL, completed_at = now()
            "#,
        )
        .bind(commit_id)
        .execute(&mut **tx)
        .await?;
        return Ok(());
    }
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    sqlx::query(
        "INSERT INTO flake_output_contents (digest, payload) VALUES ($1, $2) \
         ON CONFLICT (digest) DO NOTHING",
    )
    .bind(digest.as_slice())
    .bind(&payload)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO flake_output_snapshots (
            commit_id, lifecycle, first_parent_sha, content_digest, completed_at
        )
        SELECT $1, 'available', c.first_parent_sha, $2, now()
        FROM commits c
        WHERE c.id = $1
        ON CONFLICT (commit_id) DO UPDATE
        SET lifecycle = 'available',
            first_parent_sha = EXCLUDED.first_parent_sha,
            content_digest = EXCLUDED.content_digest,
            error = NULL,
            completed_at = now()
        "#,
    )
    .bind(commit_id)
    .bind(digest.as_slice())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Retains the snapshot that produced one observed system generation.
///
/// The lookup uses the deployment request's authoritative commit identity and
/// its exact NixOS derivation. A store path alone is never used to choose among
/// commits because distinct commits can evaluate to the same closure.
/// Missing legacy data is a safe no-op and can be linked later when snapshot
/// finalization runs the reciprocal backfill.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot resolve or retain the snapshot.
pub async fn retain_generation_snapshot_tx(
    tx: &mut Transaction<'_, Postgres>,
    hostname: &str,
    generation: Option<i32>,
    store_path: Option<&str>,
) -> Result<bool> {
    let (Some(generation), Some(store_path)) = (generation, store_path.map(str::trim)) else {
        return Ok(false);
    };
    if store_path.is_empty() {
        return Ok(false);
    }

    let inserted = sqlx::query(
        r#"
        INSERT INTO evaluation_generation_snapshots (
            system_id, generation, snapshot_id, derivation_id, commit_id, source_store_path
        )
        SELECT s.id, $2, es.id, d.id, d.commit_id, $3
        FROM systems s
        JOIN LATERAL (
          SELECT pending.requested_commit_id
          FROM pending_system_deployments pending
          WHERE pending.system_id = s.id
            AND pending.target_store_path = $3
            AND pending.requested_commit_id IS NOT NULL
            AND pending.status IN ('pending', 'succeeded')
          ORDER BY pending.issued_at DESC, pending.id DESC
          LIMIT 1
        ) deployment ON true
        JOIN derivations d
          ON d.derivation_type = 'nixos'
         AND d.commit_id = deployment.requested_commit_id
         AND d.derivation_name = COALESCE(
             NULLIF(btrim(s.system_configuration_name), ''), s.hostname
         )
         AND COALESCE(d.store_path, d.expected_store_path) = $3
        JOIN evaluation_snapshots es
          ON es.commit_id = d.commit_id
         AND es.configuration_name = d.derivation_name
         AND es.lifecycle = 'available'
        WHERE s.hostname = $1
        ORDER BY d.id DESC
        LIMIT 1
        ON CONFLICT (system_id, generation) DO NOTHING
        "#,
    )
    .bind(hostname)
    .bind(generation)
    .bind(store_path)
    .execute(&mut **tx)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

/// Selects a commit-mode snapshot and its first-parent baseline.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the snapshot or a persisted
/// lifecycle value is invalid.
pub async fn select_commit_snapshot(
    pool: &PgPool,
    system_id: Uuid,
    revision: &str,
) -> Result<Option<SelectedEvaluationSnapshot>> {
    let row = sqlx::query(
        r#"
        WITH usable_snapshots AS (
            SELECT candidate.id
            FROM evaluation_snapshots candidate
            WHERE candidate.lifecycle = 'available'
              AND candidate.schema_version = 1
              AND candidate.option_count = (
                  SELECT COUNT(*) FROM evaluation_snapshot_options item
                  WHERE item.snapshot_id = candidate.id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM evaluation_snapshot_options item
                  LEFT JOIN evaluation_option_contents content
                    ON content.digest = item.content_digest
                  WHERE item.snapshot_id = candidate.id
                    AND (content.digest IS NULL OR content.schema_version <> 1
                      OR jsonb_typeof(content.payload) <> 'object'
                      OR NOT content.payload ?& ARRAY[
                          'declared_type', 'value', 'definitions', 'overridden'
                      ])
              )
        )
        SELECT es.id, c.git_commit_hash,
               CASE
               WHEN es.lifecycle = 'available' AND NOT (
                   es.schema_version <> 1
                   OR es.option_count <> (SELECT COUNT(*) FROM evaluation_snapshot_options item WHERE item.snapshot_id = es.id)
                   OR EXISTS (
                        SELECT 1 FROM evaluation_snapshot_options item
                        LEFT JOIN evaluation_option_contents content ON content.digest = item.content_digest
                        WHERE item.snapshot_id = es.id
                          AND (content.digest IS NULL OR content.schema_version <> 1
                            OR jsonb_typeof(content.payload) <> 'object'
                            OR NOT content.payload ?& ARRAY['declared_type', 'value', 'definitions', 'overridden'])
                    )
               ) THEN 'available'
               WHEN c.evaluation_status = 'pending' THEN 'queued'
               WHEN c.evaluation_status IN ('in_progress', 'cancelling') THEN 'running'
               WHEN es.lifecycle = 'available' THEN 'unavailable'
               ELSE es.lifecycle END AS lifecycle,
               CASE
               WHEN c.evaluation_status IN ('pending', 'in_progress', 'cancelling')
                 AND (es.lifecycle <> 'available'
                   OR es.schema_version <> 1
                   OR es.option_count <> (SELECT COUNT(*) FROM evaluation_snapshot_options item WHERE item.snapshot_id = es.id)
                   OR EXISTS (
                       SELECT 1 FROM evaluation_snapshot_options item
                       LEFT JOIN evaluation_option_contents content ON content.digest = item.content_digest
                       WHERE item.snapshot_id = es.id
                         AND (content.digest IS NULL OR content.schema_version <> 1
                           OR jsonb_typeof(content.payload) <> 'object'
                           OR NOT content.payload ?& ARRAY['declared_type', 'value', 'definitions', 'overridden'])
                   )) THEN NULL
               WHEN es.lifecycle = 'available' AND (
                   es.schema_version <> 1
                   OR es.option_count <> (SELECT COUNT(*) FROM evaluation_snapshot_options item WHERE item.snapshot_id = es.id)
                   OR EXISTS (
                        SELECT 1 FROM evaluation_snapshot_options item
                        LEFT JOIN evaluation_option_contents content ON content.digest = item.content_digest
                        WHERE item.snapshot_id = es.id
                          AND (content.digest IS NULL OR content.schema_version <> 1
                            OR jsonb_typeof(content.payload) <> 'object'
                            OR NOT content.payload ?& ARRAY['declared_type', 'value', 'definitions', 'overridden'])
                    )
               ) THEN 'Snapshot data is unavailable or corrupt' ELSE es.error END AS error,
               parent_snapshot.id AS baseline_id,
               parent_commit.git_commit_hash AS baseline_revision,
               NULL::integer AS generation,
                NULL::uuid AS generation_snapshot_id,
                es.module_count::bigint AS module_count, es.evaluation_duration_ms
        FROM systems s
        JOIN commits c
          ON c.flake_id = s.flake_id AND c.git_commit_hash = $2
         AND c.source_archived = false
        JOIN evaluation_snapshots es
          ON es.commit_id = c.id
         AND es.configuration_name = COALESCE(NULLIF(btrim(s.system_configuration_name), ''), s.hostname)
         LEFT JOIN commits parent_commit
           ON parent_commit.flake_id = c.flake_id
          AND parent_commit.git_commit_hash = c.first_parent_sha
          AND c.first_parent_resolved = true
         LEFT JOIN evaluation_snapshots parent_snapshot
           ON parent_snapshot.commit_id = parent_commit.id
          AND parent_snapshot.configuration_name = es.configuration_name
          AND parent_snapshot.id IN (SELECT id FROM usable_snapshots)
        WHERE s.id = $1
        "#,
    )
    .bind(system_id)
    .bind(revision)
    .fetch_optional(pool)
    .await?;

    let Some(mut selected) = row.map(selected_snapshot_from_row).transpose()? else {
        return Ok(None);
    };
    if let Some(baseline_id) = selected.baseline_id
        && !snapshot_is_usable(pool, baseline_id).await?
    {
        selected.baseline_id = None;
        selected.baseline_revision = None;
    }
    Ok(Some(selected))
}

/// Selects a retained generation and its immediately preceding retained snapshot.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the snapshot or a persisted
/// lifecycle value is invalid.
pub async fn select_generation_snapshot(
    pool: &PgPool,
    system_id: Uuid,
    generation: i32,
) -> Result<Option<SelectedEvaluationSnapshot>> {
    let row = sqlx::query(
        r#"
        WITH usable_snapshots AS (
            SELECT candidate.id
            FROM evaluation_snapshots candidate
            WHERE candidate.lifecycle = 'available'
              AND candidate.schema_version = 1
              AND candidate.option_count = (
                  SELECT COUNT(*) FROM evaluation_snapshot_options item
                  WHERE item.snapshot_id = candidate.id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM evaluation_snapshot_options item
                  LEFT JOIN evaluation_option_contents content
                    ON content.digest = item.content_digest
                  WHERE item.snapshot_id = candidate.id
                    AND (content.digest IS NULL OR content.schema_version <> 1
                      OR jsonb_typeof(content.payload) <> 'object'
                      OR NOT content.payload ?& ARRAY[
                          'declared_type', 'value', 'definitions', 'overridden'
                      ])
              )
        )
        SELECT es.id, c.git_commit_hash,
               CASE WHEN es.lifecycle = 'available' AND (
                   es.schema_version <> 1
                   OR es.option_count <> (SELECT COUNT(*) FROM evaluation_snapshot_options item WHERE item.snapshot_id = es.id)
                   OR EXISTS (
                       SELECT 1 FROM evaluation_snapshot_options item
                        LEFT JOIN evaluation_option_contents content ON content.digest = item.content_digest
                        WHERE item.snapshot_id = es.id
                          AND (content.digest IS NULL OR content.schema_version <> 1
                            OR jsonb_typeof(content.payload) <> 'object'
                            OR NOT content.payload ?& ARRAY['declared_type', 'value', 'definitions', 'overridden'])
                   )
               ) THEN 'unavailable' ELSE es.lifecycle END AS lifecycle,
               CASE WHEN es.lifecycle = 'available' AND (
                   es.schema_version <> 1
                   OR es.option_count <> (SELECT COUNT(*) FROM evaluation_snapshot_options item WHERE item.snapshot_id = es.id)
                   OR EXISTS (
                       SELECT 1 FROM evaluation_snapshot_options item
                        LEFT JOIN evaluation_option_contents content ON content.digest = item.content_digest
                        WHERE item.snapshot_id = es.id
                          AND (content.digest IS NULL OR content.schema_version <> 1
                            OR jsonb_typeof(content.payload) <> 'object'
                            OR NOT content.payload ?& ARRAY['declared_type', 'value', 'definitions', 'overridden'])
                   )
               ) THEN 'Snapshot data is unavailable or corrupt' ELSE es.error END AS error,
               previous.snapshot_id AS baseline_id,
               previous_commit.git_commit_hash AS baseline_revision,
               selected.generation,
                selected.id AS generation_snapshot_id,
                es.module_count::bigint AS module_count, es.evaluation_duration_ms
        FROM evaluation_generation_snapshots selected
        JOIN evaluation_snapshots es ON es.id = selected.snapshot_id
        JOIN commits c ON c.id = es.commit_id
        LEFT JOIN LATERAL (
            SELECT retained.snapshot_id
            FROM evaluation_generation_snapshots retained
            WHERE retained.system_id = selected.system_id
              AND retained.generation < selected.generation
              AND retained.snapshot_id IN (SELECT id FROM usable_snapshots)
            ORDER BY retained.generation DESC
            LIMIT 1
        ) previous ON true
        LEFT JOIN evaluation_snapshots previous_snapshot ON previous_snapshot.id = previous.snapshot_id
        LEFT JOIN commits previous_commit ON previous_commit.id = previous_snapshot.commit_id
        WHERE selected.system_id = $1 AND selected.generation = $2
        "#,
    )
    .bind(system_id)
    .bind(generation)
    .fetch_optional(pool)
    .await?;

    let Some(mut selected) = row.map(selected_snapshot_from_row).transpose()? else {
        return Ok(None);
    };
    if let Some(baseline_id) = selected.baseline_id
        && snapshot_is_usable(pool, baseline_id).await?
    {
        return Ok(Some(selected));
    }

    selected.baseline_id = None;
    selected.baseline_revision = None;
    // PERFORMANCE: PostgreSQL selects and validates the nearest usable retained
    // generation in one statement. The application does not issue one complete
    // corpus query for each older generation.
    let candidate = sqlx::query(
        r#"
        SELECT retained.snapshot_id, c.git_commit_hash
        FROM evaluation_generation_snapshots retained
        JOIN evaluation_snapshots snapshot ON snapshot.id = retained.snapshot_id
        JOIN commits c ON c.id = snapshot.commit_id
        WHERE retained.system_id = $1 AND retained.generation < $2
          AND snapshot.lifecycle = 'available'
          AND snapshot.schema_version = 1
          AND snapshot.option_count = (
              SELECT COUNT(*) FROM evaluation_snapshot_options item
              WHERE item.snapshot_id = snapshot.id
          )
          AND NOT EXISTS (
              SELECT 1 FROM evaluation_snapshot_options item
              LEFT JOIN evaluation_option_contents content
                ON content.digest = item.content_digest
              WHERE item.snapshot_id = snapshot.id
                AND (content.digest IS NULL OR content.schema_version <> 1
                  OR jsonb_typeof(content.payload) <> 'object'
                  OR NOT content.payload ?& ARRAY[
                      'declared_type', 'value', 'definitions', 'overridden'
                  ]
                  OR jsonb_typeof(content.payload->'declared_type') <> 'string'
                  OR jsonb_typeof(content.payload->'overridden') <> 'boolean'
                  OR jsonb_typeof(content.payload->'definitions') <> 'array'
                  OR jsonb_typeof(content.payload->'value') <> 'object'
                  OR content.payload->'value'->>'kind' NOT IN (
                      'scalar', 'package', 'list', 'attribute_set',
                      'submodule', 'opaque', 'failed'
                  )
                  OR NOT (content.payload->'value' ? 'value')
                  OR EXISTS (
                      SELECT 1
                      FROM jsonb_array_elements(
                          CASE WHEN jsonb_typeof(content.payload->'definitions') = 'array'
                               THEN content.payload->'definitions' ELSE '[]'::jsonb END
                      ) definition(value)
                      WHERE jsonb_typeof(definition.value) <> 'object'
                         OR jsonb_typeof(definition.value->'source_path') <> 'string'
                         OR jsonb_typeof(definition.value->'winning') <> 'boolean'
                  ))
          )
        ORDER BY retained.generation DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .bind(generation)
    .fetch_optional(pool)
    .await?;
    if let Some(candidate) = candidate {
        let snapshot_id: Uuid = candidate.try_get("snapshot_id")?;
        selected.baseline_id = Some(snapshot_id);
        selected.baseline_revision = Some(candidate.try_get("git_commit_hash")?);
    }
    Ok(Some(selected))
}

/// Returns the scalar database-only summary for one selected evaluation.
///
/// Selected and running store paths are compared as exact identities. Host delta
/// uses content digests from usable snapshots at the same commit. Seven-day drift
/// uses persisted state and heartbeat observations and fails closed on a coverage
/// gap. Call [`get_evaluation_module_sources_page`] for bounded source rows.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load or decode persisted summary data.
pub async fn get_selected_evaluation_summary(
    pool: &PgPool,
    system_id: Uuid,
    selected: &SelectedEvaluationSnapshot,
) -> Result<SelectedEvaluationSummary> {
    if selected.lifecycle != SnapshotLifecycle::Available
        || !snapshot_is_usable(pool, selected.id).await?
    {
        let lifecycle = if selected.lifecycle == SnapshotLifecycle::Available {
            SnapshotLifecycle::Unavailable
        } else {
            selected.lifecycle
        };
        return Ok(empty_selected_evaluation_summary(
            selected,
            lifecycle,
            selected.error.clone().or_else(|| {
                (lifecycle == SnapshotLifecycle::Unavailable)
                    .then(|| "Snapshot data is unavailable or corrupt".to_string())
            }),
        ));
    }

    let metadata = sqlx::query(
        r#"
        WITH selected_snapshot AS (
            SELECT snapshot.id, snapshot.commit_id, snapshot.completed_at,
                    snapshot.evaluation_duration_ms, snapshot.option_count,
                    snapshot.module_count, snapshot.host_delta_count,
                   snapshot.configuration_name
            FROM evaluation_snapshots snapshot
            WHERE snapshot.id = $2
        ), selected_generation AS (
            SELECT retained.id, retained.derivation_id, retained.source_store_path
            FROM evaluation_generation_snapshots retained
            WHERE retained.system_id = $1 AND retained.snapshot_id = $2
              AND $3::integer IS NOT NULL AND retained.generation = $3
        ), selected_derivation AS (
            SELECT candidate.store_path, candidate.expected_store_path,
                   candidate.closure_total, candidate.closure_size_bytes
            FROM (
                SELECT derivation.store_path, derivation.expected_store_path,
                        derivation.closure_total, derivation.closure_size_bytes,
                        0 AS preference,
                       derivation.completed_at, derivation.id
                FROM selected_generation retained
                JOIN derivations derivation ON derivation.id = retained.derivation_id
                UNION ALL
                SELECT derivation.store_path, derivation.expected_store_path,
                        derivation.closure_total, derivation.closure_size_bytes,
                        1 AS preference,
                       derivation.completed_at, derivation.id
                FROM selected_snapshot snapshot
                JOIN derivations derivation
                  ON derivation.commit_id = snapshot.commit_id
                 AND derivation.derivation_type = 'nixos'
                 AND derivation.derivation_name = snapshot.configuration_name
                WHERE NOT EXISTS (SELECT 1 FROM selected_generation)
            ) candidate
            ORDER BY candidate.preference, candidate.completed_at DESC NULLS LAST,
                     candidate.id DESC
            LIMIT 1
        ), latest_state AS (
            SELECT state.store_path, state.generation_matches_current_store_path
            FROM systems system
            JOIN system_states state ON state.hostname = system.hostname
            WHERE system.id = $1
            ORDER BY state.timestamp DESC, state.id DESC
            LIMIT 1
        )
        SELECT snapshot.completed_at, snapshot.evaluation_duration_ms,
                snapshot.option_count, snapshot.module_count,
                snapshot.host_delta_count,
               COALESCE(retained.source_store_path, derivation.store_path,
                        derivation.expected_store_path) AS selected_store_path,
                derivation.closure_total, derivation.closure_size_bytes,
               state.store_path AS running_store_path,
               state.generation_matches_current_store_path AS running_profile_matches
        FROM selected_snapshot snapshot
        LEFT JOIN selected_generation retained ON true
        LEFT JOIN selected_derivation derivation ON true
        LEFT JOIN latest_state state ON true
        "#,
    )
    .bind(system_id)
    .bind(selected.id)
    .bind(selected.generation)
    .fetch_one(pool)
    .await?;

    let completed_at: Option<DateTime<Utc>> = metadata.try_get("completed_at")?;
    let evaluation_duration_ms: Option<i64> = metadata.try_get("evaluation_duration_ms")?;
    let option_total: i32 = metadata.try_get("option_count")?;
    let module_source_total: i32 = metadata.try_get("module_count")?;
    let selected_store_path: Option<String> = metadata.try_get("selected_store_path")?;
    let running_store_path: Option<String> = metadata.try_get("running_store_path")?;
    let drift = match (&selected_store_path, &running_store_path) {
        (Some(selected), Some(running)) if selected == running => EvaluationDrift::Matches,
        (Some(_), Some(_)) => EvaluationDrift::Differs,
        _ => EvaluationDrift::Unavailable,
    };
    let agent_fingerprint = match (&selected_store_path, &running_store_path) {
        (Some(selected), Some(running)) if selected == running => AgentFingerprintStatus::Matches,
        (Some(_), Some(_)) => AgentFingerprintStatus::Differs,
        _ => AgentFingerprintStatus::Unavailable,
    };
    let seven_day_drift =
        seven_day_drift_status(pool, system_id, selected_store_path.as_deref()).await?;

    Ok(SelectedEvaluationSummary {
        lifecycle: selected.lifecycle,
        revision: selected.revision.clone(),
        generation: selected.generation,
        error: selected.error.clone(),
        module_source_total: i64::from(module_source_total),
        completed_at,
        evaluation_duration_ms,
        option_total: i64::from(option_total),
        selected_store_path,
        closure_package_count: metadata.try_get("closure_total")?,
        closure_size_bytes: metadata.try_get("closure_size_bytes")?,
        running_store_path,
        running_profile_matches: metadata.try_get("running_profile_matches")?,
        host_delta_count: metadata.try_get("host_delta_count")?,
        agent_fingerprint,
        seven_day_drift,
        drift,
    })
}

/// Classifies seven-day drift from exact persisted agent observations.
///
/// Coverage is complete only when observations span the full window and every
/// adjacent or boundary gap is at most the established fixed four-hour offline
/// boundary. Heartbeat configuration does not change this product boundary. The
/// observation before the window establishes coverage but does not contribute
/// drift. Any missing exact path makes coverage insufficient.
async fn seven_day_drift_status(
    pool: &PgPool,
    system_id: Uuid,
    selected_store_path: Option<&str>,
) -> Result<SevenDayDriftStatus> {
    let Some(selected_store_path) = selected_store_path else {
        return Ok(SevenDayDriftStatus::InsufficientCoverage);
    };
    let row = sqlx::query(
        r#"
        WITH parameters AS (
            SELECT system.hostname, now() AS end_at,
                   now() - interval '7 days' AS start_at,
                   interval '4 hours' AS max_gap
            FROM systems system WHERE system.id = $1
        ), observations AS (
            SELECT state.timestamp AS observed_at, state.store_path
            FROM parameters parameter
            JOIN system_states state ON state.hostname = parameter.hostname
            WHERE state.timestamp >= parameter.start_at - parameter.max_gap
              AND state.timestamp <= parameter.end_at
            UNION ALL
            SELECT heartbeat.timestamp, state.store_path
            FROM parameters parameter
            JOIN system_states state ON state.hostname = parameter.hostname
            JOIN agent_heartbeats heartbeat ON heartbeat.system_state_id = state.id
            WHERE heartbeat.timestamp >= parameter.start_at - parameter.max_gap
              AND heartbeat.timestamp <= parameter.end_at
        ), ordered AS (
            SELECT observed_at, store_path,
                   LAG(observed_at) OVER (ORDER BY observed_at) AS previous_at
            FROM observations
        )
        SELECT COALESCE(
                   MIN(ordered.observed_at) <= parameter.start_at
                   AND MAX(ordered.observed_at) >= parameter.end_at - parameter.max_gap
                   AND BOOL_AND(ordered.store_path IS NOT NULL)
                   AND COALESCE(MAX(ordered.observed_at - ordered.previous_at), interval '0')
                       <= parameter.max_gap,
                   false
               ) AS complete_coverage,
               COALESCE(BOOL_OR(
                   ordered.store_path IS DISTINCT FROM $2
                   AND ordered.observed_at >= parameter.start_at
               ), false)
                   AS observed_drift
        FROM parameters parameter
        LEFT JOIN ordered ON true
        GROUP BY parameter.start_at, parameter.end_at, parameter.max_gap
        "#,
    )
    .bind(system_id)
    .bind(selected_store_path)
    .fetch_one(pool)
    .await
    .context("failed to classify seven-day store-path drift")?;
    if !row.try_get::<bool, _>("complete_coverage")? {
        return Ok(SevenDayDriftStatus::InsufficientCoverage);
    }
    Ok(if row.try_get::<bool, _>("observed_drift")? {
        SevenDayDriftStatus::ObservedDrift
    } else {
        SevenDayDriftStatus::NoObservedDrift
    })
}

/// Returns commit evaluation lifecycle when no reusable snapshot exists.
///
/// The system-to-flake join prevents disclosure of revisions from another
/// flake. Callers must perform environment authorization first.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the commit lifecycle.
pub async fn missing_snapshot_lifecycle(
    pool: &PgPool,
    system_id: Uuid,
    revision: &str,
) -> Result<Option<(SnapshotLifecycle, Option<String>)>> {
    let row = sqlx::query(
        r#"
        SELECT c.evaluation_status, c.evaluation_error_message
        FROM systems s
        JOIN commits c ON c.flake_id = s.flake_id
        WHERE s.id = $1 AND c.git_commit_hash = $2
          AND c.source_archived = false
        "#,
    )
    .bind(system_id)
    .bind(revision)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        let status: String = row.try_get("evaluation_status")?;
        let error: Option<String> = row.try_get("evaluation_error_message")?;
        let lifecycle = match status.as_str() {
            "pending" => SnapshotLifecycle::Queued,
            "in_progress" => SnapshotLifecycle::Running,
            "failed" => SnapshotLifecycle::Failed,
            _ => SnapshotLifecycle::Unavailable,
        };
        Ok((
            lifecycle,
            error.map(|value| crate::security::snapshot_redaction::redact_evaluation_error(&value)),
        ))
    })
    .transpose()
}

/// Queues missing snapshot evaluation or reuses active or available work.
///
/// The state transition is idempotent. Pending and running commits are reused,
/// and this action never resets an available configuration snapshot.
///
/// # Errors
///
/// Returns an error when the system or revision is not eligible for evaluation,
/// or when PostgreSQL cannot commit the lifecycle transition.
pub async fn queue_or_reuse_evaluation(
    pool: &PgPool,
    system_id: Uuid,
    revision: &str,
) -> Result<Option<QueueEvaluationResponse>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        r#"
        SELECT c.id, c.evaluation_status,
                EXISTS (
                    SELECT 1
                    FROM evaluation_snapshots es
                    WHERE es.commit_id = c.id
                     AND es.configuration_name = COALESCE(
                         NULLIF(btrim(s.system_configuration_name), ''), s.hostname
                      )
                      AND es.lifecycle = 'available'
                      AND es.schema_version = 1
                      AND es.option_count = (
                          SELECT COUNT(*) FROM evaluation_snapshot_options item
                          WHERE item.snapshot_id = es.id
                      )
                      AND NOT EXISTS (
                          SELECT 1
                          FROM evaluation_snapshot_options item
                          LEFT JOIN evaluation_option_contents content
                            ON content.digest = item.content_digest
                          WHERE item.snapshot_id = es.id
                            AND (content.digest IS NULL OR content.schema_version <> 1
                              OR jsonb_typeof(content.payload) <> 'object'
                              OR NOT content.payload ?& ARRAY['declared_type', 'value', 'definitions', 'overridden'])
                      )
                ) AS snapshot_available
        FROM systems s
        JOIN commits c ON c.flake_id = s.flake_id
        WHERE s.id = $1 AND c.git_commit_hash = $2
          AND c.source_archived = false
        FOR UPDATE OF c
        "#,
    )
    .bind(system_id)
    .bind(revision)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };

    let commit_id: i32 = row.try_get("id")?;
    let status: String = row.try_get("evaluation_status")?;
    let snapshot_available: bool = row.try_get("snapshot_available")?;
    let (lifecycle, queued) = if snapshot_available {
        (SnapshotLifecycle::Available, false)
    } else if status == "pending" {
        (SnapshotLifecycle::Queued, false)
    } else if matches!(status.as_str(), "in_progress" | "cancelling") {
        (SnapshotLifecycle::Running, false)
    } else {
        // LIFECYCLE: A retry creates a new lineage row in the same transaction
        // as the commit transition. Setting only commits.evaluation_status
        // would advertise queued work that no evaluator can claim.
        sqlx::query(
            r#"
            UPDATE commits
            SET evaluation_status = 'pending',
                evaluation_started_at = NULL,
                evaluation_completed_at = NULL,
                evaluation_error_message = NULL,
                cancellation_requested = false
            WHERE id = $1
            "#,
        )
        .bind(commit_id)
        .execute(&mut *tx)
        .await?;
        let attempt = sqlx::query(
            r#"
            WITH source AS (
                SELECT id, COALESCE(root_attempt_id, id) AS root_attempt_id,
                       attempt_number
                FROM evaluation_attempts
                WHERE commit_id = $1
                ORDER BY attempt_number DESC, created_at DESC
                LIMIT 1
            )
            INSERT INTO evaluation_attempts (
                commit_id, parent_attempt_id, root_attempt_id, attempt_number,
                automatic_retry_count, available_at
            )
            SELECT $1, id, root_attempt_id, attempt_number + 1, 0, NOW()
            FROM source
            "#,
        )
        .bind(commit_id)
        .execute(&mut *tx)
        .await?;
        anyhow::ensure!(
            attempt.rows_affected() == 1,
            "commit {commit_id} has no evaluation attempt lineage"
        );
        (SnapshotLifecycle::Queued, true)
    };
    tx.commit().await?;

    Ok(Some(QueueEvaluationResponse {
        revision: revision.to_string(),
        lifecycle,
        queued,
    }))
}

/// Returns one cached flake output snapshot and authoritative system reconciliation.
///
/// This function performs database reads only. The caller must authorize flake
/// visibility before calling it.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the snapshot or persisted output
/// data has an invalid shape.
pub async fn get_flake_output_snapshot(
    pool: &PgPool,
    flake_id: i32,
    revision: &str,
    visibility_user: Option<Uuid>,
    system_filter: FlakeSystemFilter,
    limit: usize,
    offset: usize,
) -> Result<Option<FlakeOutputSnapshotResponse>> {
    get_flake_output_snapshot_with_token(
        pool,
        flake_id,
        revision,
        None,
        visibility_user,
        system_filter,
        limit,
        offset,
    )
    .await
}

/// Returns one optionally token-bound page from a cached flake output snapshot.
///
/// The opaque token covers the selected content digest, first-parent identity
/// and resolution state, and the usable first-parent content digest. Omitting
/// the token preserves bounded offset behavior for existing clients.
///
/// # Errors
///
/// Returns [`FLAKE_OUTPUT_SNAPSHOT_CHANGED`] when a supplied `requested_token`
/// does not identify the current selected and first-parent state. Other errors
/// report persistence or shape failures.
pub async fn get_flake_output_snapshot_with_token(
    pool: &PgPool,
    flake_id: i32,
    revision: &str,
    requested_token: Option<&str>,
    visibility_user: Option<Uuid>,
    system_filter: FlakeSystemFilter,
    limit: usize,
    offset: usize,
) -> Result<Option<FlakeOutputSnapshotResponse>> {
    let row = sqlx::query(
        r#"
        SELECT c.evaluation_status, c.evaluation_error_message,
                CASE WHEN snapshot.lifecycle = 'available' AND (
                    snapshot.schema_version <> 1 OR content.digest IS NULL
                    OR content.schema_version <> 1
                ) THEN 'unavailable' ELSE snapshot.lifecycle END AS lifecycle,
                c.first_parent_sha, c.first_parent_resolved,
               snapshot.content_digest AS selected_content_digest,
               previous_snapshot.content_digest AS previous_content_digest,
               content.payload,
               previous_content.payload AS previous_payload
        FROM commits c
        LEFT JOIN flake_output_snapshots snapshot ON snapshot.commit_id = c.id
        LEFT JOIN flake_output_contents content ON content.digest = snapshot.content_digest
        LEFT JOIN commits previous_commit
          ON previous_commit.flake_id = c.flake_id
         AND previous_commit.git_commit_hash = c.first_parent_sha
        LEFT JOIN flake_output_snapshots previous_snapshot
          ON previous_snapshot.commit_id = previous_commit.id
          AND previous_snapshot.lifecycle = 'available'
          AND previous_snapshot.schema_version = 1
        LEFT JOIN flake_output_contents previous_content
          ON previous_content.digest = previous_snapshot.content_digest
          AND previous_content.schema_version = 1
        WHERE c.flake_id = $1 AND c.git_commit_hash = $2
          AND c.source_archived = false
        "#,
    )
    .bind(flake_id)
    .bind(revision)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };

    let limit = limit.clamp(1, FLAKE_OUTPUT_PAGE_LIMIT);
    let offset = offset.min(100_000);
    let selected_content_digest: Option<Vec<u8>> = row.try_get("selected_content_digest")?;
    let previous_content_digest: Option<Vec<u8>> = row.try_get("previous_content_digest")?;
    let mut full_payload: Option<Value> = row.try_get("payload")?;
    if let Some(payload) = &mut full_payload {
        add_read_time_input_age(payload);
    }
    let first_parent_revision: Option<String> = row.try_get("first_parent_sha")?;
    let first_parent_resolved: bool = row.try_get("first_parent_resolved")?;
    let mut full_previous_outputs: Option<Value> = row.try_get("previous_payload")?;
    if let Some(payload) = &mut full_previous_outputs {
        add_read_time_input_age(payload);
    }
    if full_previous_outputs
        .as_ref()
        .is_some_and(|payload| !valid_flake_payload(payload))
    {
        full_previous_outputs = None;
    }
    let persisted_lifecycle: Option<String> = row.try_get("lifecycle")?;
    let evaluation_status: String = row.try_get("evaluation_status")?;
    let mut lifecycle = match persisted_lifecycle.as_deref() {
        Some(value) => parse_lifecycle(value.to_string())?,
        None if evaluation_status == "pending" => SnapshotLifecycle::Queued,
        None if evaluation_status == "in_progress" => SnapshotLifecycle::Running,
        None if evaluation_status == "failed" => SnapshotLifecycle::Failed,
        None => SnapshotLifecycle::Unavailable,
    };
    if lifecycle == SnapshotLifecycle::Available
        && full_payload
            .as_ref()
            .is_none_or(|payload| !valid_flake_payload(payload))
    {
        lifecycle = SnapshotLifecycle::Unavailable;
        full_payload = None;
        full_previous_outputs = None;
    }
    let usable_previous_digest = (first_parent_resolved
        && first_parent_revision.is_some()
        && full_previous_outputs.is_some())
    .then_some(previous_content_digest.as_deref())
    .flatten();
    let snapshot_token = (lifecycle == SnapshotLifecycle::Available)
        .then(|| {
            flake_output_snapshot_token(
                selected_content_digest.as_deref(),
                first_parent_resolved,
                first_parent_revision.as_deref(),
                usable_previous_digest,
            )
        })
        .flatten();
    if requested_token.is_some_and(|token| Some(token) != snapshot_token.as_deref()) {
        anyhow::bail!(FLAKE_OUTPUT_SNAPSHOT_CHANGED);
    }
    let mut error = match lifecycle {
        SnapshotLifecycle::Failed => row
            .try_get::<Option<String>, _>("evaluation_error_message")?
            .map(|value| crate::security::snapshot_redaction::redact_evaluation_error(&value)),
        SnapshotLifecycle::Unavailable if persisted_lifecycle.is_some() => {
            Some("Snapshot data is unavailable or corrupt".to_string())
        }
        _ => None,
    };

    // SECURITY: Visibility, reconciliation, filtering, totals, ordering, and
    // pagination stay in one set-based statement. No full fleet is transferred
    // to Rust, including when the requested offset is beyond the last row.
    let reconciliation = sqlx::query(
        r#"
        WITH parameters AS (
          SELECT $1::integer AS flake_id, $2::uuid AS visibility_user,
                 $3::text AS revision, $4::jsonb AS payload,
                 $5::jsonb AS previous_payload, $6::text AS system_filter,
                 $7::bigint AS page_limit, $8::bigint AS page_offset
        ), active_systems AS (
          SELECT system.*,
                 COALESCE(NULLIF(btrim(system.system_configuration_name), ''),
                          system.hostname) AS configuration_name
          FROM systems system, parameters parameter
          WHERE system.flake_id = parameter.flake_id AND system.is_active = true
        ), visible_systems AS (
          SELECT system.*, environment.name AS environment_name,
                 environment.color_hex AS environment_color,
                 deployment.current_commit_hash AS deployed_revision,
                 COUNT(*) OVER (PARTITION BY system.configuration_name) AS output_host_count
          FROM active_systems system
          CROSS JOIN parameters parameter
          LEFT JOIN environments environment ON environment.id = system.environment_id
          LEFT JOIN view_system_deployment_status deployment
            ON deployment.hostname = system.hostname
          WHERE parameter.visibility_user IS NULL OR EXISTS (
                SELECT 1 FROM user_environment_memberships membership
                WHERE membership.user_id = parameter.visibility_user
                  AND membership.environment_id = system.environment_id
          )
        ), declared_raw AS (
          SELECT DISTINCT declaration.configuration_name
          FROM parameters parameter
          CROSS JOIN LATERAL jsonb_array_elements_text(
            CASE WHEN jsonb_typeof(parameter.payload->'declared_systems') = 'array'
                 THEN parameter.payload->'declared_systems' ELSE '[]'::jsonb END
          ) declaration(configuration_name)
        ), declared AS (
          SELECT declaration.configuration_name
          FROM declared_raw declaration
          WHERE NOT EXISTS (
                  SELECT 1 FROM active_systems system
                  WHERE system.configuration_name = declaration.configuration_name
                )
             OR EXISTS (
                  SELECT 1 FROM visible_systems system
                  WHERE system.configuration_name = declaration.configuration_name
                )
        ), previous_declared AS (
          SELECT DISTINCT declaration.configuration_name
          FROM parameters parameter
          CROSS JOIN LATERAL jsonb_array_elements_text(
            CASE WHEN jsonb_typeof(parameter.previous_payload->'declared_systems') = 'array'
                 THEN parameter.previous_payload->'declared_systems' ELSE '[]'::jsonb END
          ) declaration(configuration_name)
          WHERE NOT EXISTS (
                  SELECT 1 FROM active_systems system
                  WHERE system.configuration_name = declaration.configuration_name
                )
             OR EXISTS (
                  SELECT 1 FROM visible_systems system
                  WHERE system.configuration_name = declaration.configuration_name
                )
        ), reconciliation AS (
          SELECT system.configuration_name, system.id AS system_id, system.hostname,
                 system.environment_name, system.environment_color,
                 CASE WHEN declaration.configuration_name IS NULL
                      THEN 'managed_undeclared' ELSE 'managed' END AS state,
                 CASE WHEN system.deployed_revision IS DISTINCT FROM parameter.revision
                      THEN system.deployed_revision END AS deployed_revision,
                 system.output_host_count > 1 AS output_collapsed
          FROM visible_systems system
          CROSS JOIN parameters parameter
          LEFT JOIN declared declaration USING (configuration_name)
          UNION ALL
          SELECT declaration.configuration_name, NULL::uuid, NULL::text,
                 NULL::text, NULL::text, 'declared_unmanaged', NULL::text, false
          FROM declared declaration
          WHERE NOT EXISTS (
              SELECT 1 FROM visible_systems system
              WHERE system.configuration_name = declaration.configuration_name
          )
        ), filtered AS (
          SELECT reconciliation.*
          FROM reconciliation, parameters parameter
          WHERE parameter.system_filter = 'all'
             OR reconciliation.state = parameter.system_filter
        ), page AS (
          SELECT filtered.*
          FROM filtered, parameters parameter
          ORDER BY filtered.configuration_name COLLATE "C",
                   filtered.hostname COLLATE "C" NULLS LAST,
                   filtered.system_id NULLS LAST
          LIMIT (SELECT page_limit FROM parameters)
          OFFSET (SELECT page_offset FROM parameters)
        ), input_stats AS (
          SELECT COUNT(*) FILTER (
              WHERE input.value->>'direct' = 'true'
                AND (input.value->>'last_modified') ~ '^[0-9]+$'
                AND to_timestamp((input.value->>'last_modified')::double precision)
                    < now() - interval '90 days'
          )::bigint AS stale_direct_input_count
          FROM parameters parameter
          LEFT JOIN LATERAL jsonb_array_elements(
            CASE WHEN jsonb_typeof(parameter.payload->'inputs') = 'array'
                 THEN parameter.payload->'inputs' ELSE '[]'::jsonb END
          ) input(value) ON true
        )
        SELECT COALESCE((
                   SELECT jsonb_agg(jsonb_build_object(
                       'configuration_name', page.configuration_name,
                       'system_id', page.system_id,
                       'hostname', page.hostname,
                       'environment_name', page.environment_name,
                       'environment_color', page.environment_color,
                       'state', page.state,
                       'deployed_revision', page.deployed_revision,
                       'output_collapsed', page.output_collapsed
                   ) ORDER BY page.configuration_name COLLATE "C",
                              page.hostname COLLATE "C" NULLS LAST,
                              page.system_id NULLS LAST)
                   FROM page
               ), '[]'::jsonb) AS systems,
               (SELECT COUNT(*)::bigint FROM visible_systems) AS managed_system_count,
               (SELECT COUNT(*)::bigint FROM declared) AS declared_system_count,
               (SELECT COUNT(*)::bigint FROM reconciliation
                WHERE state = 'declared_unmanaged') AS declared_unmanaged_count,
               (SELECT COUNT(*)::bigint FROM reconciliation
                WHERE state = 'managed_undeclared') AS managed_undeclared_count,
               (SELECT COUNT(DISTINCT configuration_name)::bigint
                FROM reconciliation WHERE output_collapsed) AS output_collapsed_count,
               (SELECT COUNT(*)::bigint FROM reconciliation, parameters parameter
                WHERE system_id IS NOT NULL AND deployed_revision IS NOT NULL)
                   AS pinned_revision_count,
               input_stats.stale_direct_input_count,
               (SELECT COUNT(*)::bigint FROM filtered) AS system_total,
               (SELECT COALESCE(jsonb_agg(configuration_name ORDER BY configuration_name COLLATE "C"),
                                '[]'::jsonb) FROM declared) AS visible_declared,
               (SELECT COALESCE(jsonb_agg(configuration_name ORDER BY configuration_name COLLATE "C"),
                                '[]'::jsonb) FROM previous_declared) AS visible_previous_declared
        FROM input_stats
        "#,
    )
    .bind(flake_id)
    .bind(visibility_user)
    .bind(revision)
    .bind(full_payload.as_ref().unwrap_or(&Value::Null))
    .bind(full_previous_outputs.as_ref().unwrap_or(&Value::Null))
    .bind(match system_filter {
        FlakeSystemFilter::All => "all",
        FlakeSystemFilter::DeclaredUnmanaged => "declared_unmanaged",
        FlakeSystemFilter::ManagedUndeclared => "managed_undeclared",
    })
    .bind(i64::try_from(limit).context("flake page limit exceeds i64")?)
    .bind(i64::try_from(offset).context("flake page offset exceeds i64")?)
    .fetch_one(pool)
    .await?;
    let mut systems_page: Vec<ReconciledFlakeSystem> =
        serde_json::from_value(reconciliation.try_get("systems")?)
            .context("failed to decode bounded flake reconciliation page")?;
    let managed_system_count: i64 = reconciliation.try_get("managed_system_count")?;
    let declared_system_count: i64 = reconciliation.try_get("declared_system_count")?;
    let previous_declared_system_count = if full_previous_outputs.is_some() {
        let visible_previous_declared: Value =
            reconciliation.try_get("visible_previous_declared")?;
        Some(
            i64::try_from(visible_previous_declared.as_array().map_or(0, Vec::len))
                .context("previous declared count exceeds i64")?,
        )
    } else {
        None
    };
    let declared_unmanaged_count: i64 = reconciliation.try_get("declared_unmanaged_count")?;
    let managed_undeclared_count: i64 = reconciliation.try_get("managed_undeclared_count")?;
    let output_collapsed_count: i64 = reconciliation.try_get("output_collapsed_count")?;
    let pinned_revision_count: i64 = reconciliation.try_get("pinned_revision_count")?;
    let stale_direct_input_count: i64 = reconciliation.try_get("stale_direct_input_count")?;
    let system_total: i64 = reconciliation.try_get("system_total")?;
    if visibility_user.is_some() {
        let visible_declared: Value = reconciliation.try_get("visible_declared")?;
        let visible_declared = visible_declared
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        let visible_previous_declared: Value =
            reconciliation.try_get("visible_previous_declared")?;
        let visible_previous_declared = visible_previous_declared
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(payload) = &mut full_payload {
            filter_payload_configurations(payload, &visible_declared);
        }
        if let Some(payload) = &mut full_previous_outputs {
            filter_payload_configurations(payload, &visible_previous_declared);
        }
    }
    let exported_module_count = full_payload
        .as_ref()
        .and_then(|value| value.get("exported_modules"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);

    // Revision deltas are global metadata, not collection pages. Return them
    // once on the first page so continuation requests cannot duplicate changes
    // or use delta length as a collection has-more signal.
    let mut delta = (offset == 0)
        .then(|| {
            full_previous_outputs
                .as_ref()
                .zip(full_payload.as_ref())
                .map(|(before, after)| flake_output_delta(before, after))
        })
        .flatten();
    if let Some(delta) = &mut delta {
        delta.systems_added.truncate(limit);
        delta.systems_removed.truncate(limit);
        delta.modules_added.truncate(limit);
        delta.modules_removed.truncate(limit);
        delta.inputs_added.truncate(limit);
        delta.inputs_removed.truncate(limit);
        delta.input_revision_bumps.truncate(limit);
    }
    let mut payload = (lifecycle == SnapshotLifecycle::Available)
        .then_some(full_payload)
        .flatten()
        .map(|mut value| {
            paginate_flake_payload(&mut value, offset, limit);
            value
        });
    let mut previous_outputs = full_previous_outputs.map(|mut value| {
        paginate_flake_payload(&mut value, offset, limit);
        value
    });
    let response_bytes = serde_json::to_vec(&serde_json::json!({
        "outputs": &payload,
        "previous_outputs": &previous_outputs,
        "systems": &systems_page,
        "delta": &delta,
    }))
    .context("failed to size flake output response")?
    .len();
    if response_bytes > FLAKE_OUTPUT_RESPONSE_BYTES_LIMIT {
        lifecycle = SnapshotLifecycle::Unavailable;
        error = Some("Snapshot response exceeds the safe response-size limit".to_string());
        payload = None;
        previous_outputs = None;
        systems_page.clear();
        delta = None;
    }
    let systems_has_more = system_total
        > i64::try_from(offset.saturating_add(systems_page.len()))
            .context("flake page end exceeds i64")?;
    Ok(Some(FlakeOutputSnapshotResponse {
        lifecycle,
        revision: revision.to_string(),
        first_parent_revision,
        first_parent_resolved,
        comparison_available: first_parent_resolved && previous_outputs.is_some(),
        error,
        snapshot_token: (lifecycle == SnapshotLifecycle::Available)
            .then_some(snapshot_token)
            .flatten(),
        outputs: payload,
        previous_outputs,
        delta,
        systems: systems_page,
        managed_system_count,
        declared_system_count,
        previous_declared_system_count,
        declared_unmanaged_count,
        managed_undeclared_count,
        output_collapsed_count,
        pinned_revision_count,
        stale_direct_input_count,
        exported_module_count: i64::try_from(exported_module_count)
            .context("exported module count exceeds i64")?,
        pagination: FlakeOutputPagination {
            offset,
            limit,
            system_total,
            systems_has_more,
        },
    }))
}

fn flake_output_snapshot_token(
    selected_digest: Option<&[u8]>,
    first_parent_resolved: bool,
    first_parent_revision: Option<&str>,
    usable_previous_digest: Option<&[u8]>,
) -> Option<String> {
    let selected_digest = selected_digest?;
    let mut token = Sha256::new();
    token.update(b"crystal-forge:flake-output-pagination:v1\0");
    token.update((selected_digest.len() as u64).to_be_bytes());
    token.update(selected_digest);
    token.update([u8::from(first_parent_resolved)]);
    update_snapshot_token_part(&mut token, first_parent_revision.map(str::as_bytes));
    update_snapshot_token_part(&mut token, usable_previous_digest);
    Some(hex::encode(token.finalize()))
}

fn update_snapshot_token_part(token: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            token.update([1]);
            token.update((value.len() as u64).to_be_bytes());
            token.update(value);
        }
        None => token.update([0]),
    }
}

/// Returns one bounded declaration page from one persisted flake-output snapshot.
///
/// The function executes one SQL statement against one MVCC-consistent JSONB
/// snapshot. It performs no evaluation, Git, network, queue, or persistence work.
/// The content digest token binds continuation pages to the same complete payload.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot execute the bounded query or a stored
/// declaration does not match the documented safe declaration shape.
pub async fn get_flake_module_declarations(
    pool: &PgPool,
    flake_id: i32,
    revision: &str,
    module_name: &str,
    requested_token: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<FlakeModuleDeclarationsQuery> {
    let limit = limit.clamp(1, FLAKE_OUTPUT_PAGE_LIMIT);
    let offset = offset.min(FLAKE_MODULE_DECLARATION_OFFSET_LIMIT);
    let row = sqlx::query(
        r#"
        WITH selected AS (
            SELECT c.evaluation_status, c.evaluation_error_message,
                   snapshot.lifecycle AS persisted_lifecycle,
                   snapshot.schema_version AS snapshot_schema_version,
                   content.schema_version AS content_schema_version,
                   content.payload,
                   encode(snapshot.content_digest, 'hex') AS snapshot_token
            FROM commits c
            LEFT JOIN flake_output_snapshots snapshot ON snapshot.commit_id = c.id
            LEFT JOIN flake_output_contents content ON content.digest = snapshot.content_digest
            WHERE c.flake_id = $1 AND c.git_commit_hash = $2
              AND c.source_archived = false
        ), selected_module AS (
            SELECT selected.*, module.value AS module_payload
            FROM selected
            LEFT JOIN LATERAL (
                SELECT item.value
                FROM jsonb_array_elements(
                    CASE
                    WHEN jsonb_typeof(selected.payload->'exported_modules') = 'array'
                    THEN selected.payload->'exported_modules'
                    ELSE '[]'::jsonb
                    END
                ) item(value)
                WHERE item.value->>'name' = $3
                LIMIT 1
            ) module ON true
        )
        SELECT evaluation_status, evaluation_error_message, persisted_lifecycle,
               snapshot_schema_version, content_schema_version, payload,
               snapshot_token, module_payload IS NOT NULL AS module_found,
               jsonb_typeof(module_payload->'declarations') = 'array'
                   AND NOT EXISTS (
                       SELECT 1
                       FROM jsonb_array_elements(
                           CASE
                           WHEN jsonb_typeof(module_payload->'declarations') = 'array'
                           THEN module_payload->'declarations'
                           ELSE '[]'::jsonb
                           END
                       ) declaration(value)
                       WHERE jsonb_typeof(declaration.value) <> 'object'
                          OR jsonb_typeof(declaration.value->'path') <> 'string'
                          OR jsonb_typeof(declaration.value->'declared_type') <> 'string'
                          OR jsonb_typeof(declaration.value->'has_default') <> 'boolean'
                          OR jsonb_typeof(declaration.value->'source_paths') <> 'array'
                          OR EXISTS (
                              SELECT 1
                              FROM jsonb_array_elements(
                                  CASE
                                  WHEN jsonb_typeof(declaration.value->'source_paths') = 'array'
                                  THEN declaration.value->'source_paths'
                                  ELSE '[]'::jsonb
                                  END
                              ) source_path(value)
                              WHERE jsonb_typeof(source_path.value) <> 'string'
                          )
                   ) AS declarations_valid,
               CASE WHEN jsonb_typeof(module_payload->'declarations') = 'array'
                    THEN jsonb_array_length(module_payload->'declarations')
                    ELSE 0 END::bigint AS total,
               COALESCE((
                   SELECT jsonb_agg(page.declaration ORDER BY page.path,
                       page.declared_type, page.canonical, page.ordinality)
                   FROM (
                       SELECT declaration.value AS declaration,
                              COALESCE(declaration.value->>'path', '') AS path,
                              COALESCE(declaration.value->>'declared_type', '') AS declared_type,
                              declaration.value::text AS canonical,
                              declaration.ordinality
                       FROM jsonb_array_elements(
                           CASE
                           WHEN jsonb_typeof(module_payload->'declarations') = 'array'
                           THEN module_payload->'declarations'
                           ELSE '[]'::jsonb
                           END
                       ) WITH ORDINALITY declaration(value, ordinality)
                       ORDER BY path, declared_type, canonical, ordinality
                       LIMIT $4 OFFSET $5
                   ) page
               ), '[]'::jsonb) AS declarations
        FROM selected_module
        "#,
    )
    .bind(flake_id)
    .bind(revision)
    .bind(module_name)
    .bind(i64::try_from(limit).context("declaration page limit exceeds i64")?)
    .bind(i64::try_from(offset).context("declaration page offset exceeds i64")?)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(FlakeModuleDeclarationsQuery::NotFound);
    };

    let persisted_lifecycle: Option<String> = row.try_get("persisted_lifecycle")?;
    let evaluation_status: String = row.try_get("evaluation_status")?;
    let payload: Option<Value> = row.try_get("payload")?;
    let schema_valid = row.try_get::<Option<i32>, _>("snapshot_schema_version")? == Some(1)
        && row.try_get::<Option<i32>, _>("content_schema_version")? == Some(1)
        && payload.as_ref().is_some_and(valid_flake_payload);
    let mut lifecycle = match persisted_lifecycle.as_deref() {
        Some(value) => parse_lifecycle(value.to_string())?,
        None if evaluation_status == "pending" => SnapshotLifecycle::Queued,
        None if evaluation_status == "in_progress" => SnapshotLifecycle::Running,
        None if evaluation_status == "failed" => SnapshotLifecycle::Failed,
        None => SnapshotLifecycle::Unavailable,
    };
    if lifecycle == SnapshotLifecycle::Available && !schema_valid {
        lifecycle = SnapshotLifecycle::Unavailable;
    }
    let mut error = match lifecycle {
        SnapshotLifecycle::Failed => row
            .try_get::<Option<String>, _>("evaluation_error_message")?
            .map(|value| crate::security::snapshot_redaction::redact_evaluation_error(&value)),
        SnapshotLifecycle::Unavailable if persisted_lifecycle.is_some() => {
            Some("Snapshot data is unavailable or corrupt".to_string())
        }
        _ => None,
    };
    let snapshot_token: Option<String> = row.try_get("snapshot_token")?;
    if lifecycle == SnapshotLifecycle::Available
        && requested_token.is_some_and(|token| Some(token) != snapshot_token.as_deref())
    {
        return Ok(FlakeModuleDeclarationsQuery::SnapshotChanged);
    }
    let module_found: bool = row.try_get("module_found")?;
    if lifecycle == SnapshotLifecycle::Available && !module_found {
        return Ok(FlakeModuleDeclarationsQuery::NotFound);
    }
    if lifecycle == SnapshotLifecycle::Available
        && !row
            .try_get::<Option<bool>, _>("declarations_valid")?
            .unwrap_or(false)
    {
        lifecycle = SnapshotLifecycle::Unavailable;
        error = Some("Snapshot module declarations are unavailable or corrupt".to_string());
    }
    let declarations = if lifecycle == SnapshotLifecycle::Available {
        match serde_json::from_value(row.try_get("declarations")?) {
            Ok(declarations) => declarations,
            Err(_) => {
                lifecycle = SnapshotLifecycle::Unavailable;
                error = Some("Snapshot module declarations are unavailable or corrupt".to_string());
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    Ok(FlakeModuleDeclarationsQuery::Page(
        FlakeModuleDeclarationsPage {
            lifecycle,
            revision: revision.to_string(),
            module_name: module_name.to_string(),
            error,
            snapshot_token: (lifecycle == SnapshotLifecycle::Available)
                .then_some(snapshot_token)
                .flatten(),
            total: if lifecycle == SnapshotLifecycle::Available {
                row.try_get("total")?
            } else {
                0
            },
            offset,
            limit,
            declarations,
        },
    ))
}

/// Reclaims unreferenced content-addressed payloads in bounded batches.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot complete either bounded delete.
pub async fn reclaim_orphaned_snapshot_content(pool: &PgPool) -> Result<(u64, u64)> {
    let option_rows = sqlx::query(
        r#"
        DELETE FROM evaluation_option_contents content
        WHERE content.ctid IN (
            SELECT candidate.ctid
            FROM evaluation_option_contents candidate
            WHERE NOT EXISTS (
                SELECT 1 FROM evaluation_snapshot_options reference
                WHERE reference.content_digest = candidate.digest
            )
            LIMIT 1000
        )
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();
    let flake_rows = sqlx::query(
        r#"
        DELETE FROM flake_output_contents content
        WHERE content.ctid IN (
            SELECT candidate.ctid
            FROM flake_output_contents candidate
            WHERE NOT EXISTS (
                SELECT 1 FROM flake_output_snapshots reference
                WHERE reference.content_digest = candidate.digest
            )
            LIMIT 1000
        )
        "#,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok((option_rows, flake_rows))
}

fn selected_snapshot_from_row(row: sqlx::postgres::PgRow) -> Result<SelectedEvaluationSnapshot> {
    Ok(SelectedEvaluationSnapshot {
        id: row.try_get("id")?,
        revision: row.try_get("git_commit_hash")?,
        lifecycle: parse_lifecycle(row.try_get("lifecycle")?)?,
        error: row.try_get("error")?,
        baseline_id: row.try_get("baseline_id")?,
        baseline_revision: row.try_get("baseline_revision")?,
        generation: row.try_get("generation")?,
        generation_snapshot_id: row.try_get("generation_snapshot_id")?,
        module_count: row.try_get("module_count")?,
        evaluation_duration_ms: row.try_get("evaluation_duration_ms")?,
    })
}

async fn snapshot_is_usable<'e, E>(executor: E, snapshot_id: Uuid) -> Result<bool>
where
    E: Executor<'e, Database = Postgres>,
{
    // INTEGRITY: PostgreSQL validates the complete reference set and safe-value
    // envelope without transferring the option corpus to the application. Full
    // Serde decoding remains bounded to rows returned by query_options_page.
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM evaluation_snapshots snapshot
            WHERE snapshot.id = $1
              AND snapshot.lifecycle = 'available'
              AND snapshot.schema_version = 1
              AND snapshot.option_count = (
                  SELECT COUNT(*) FROM evaluation_snapshot_options item
                  WHERE item.snapshot_id = snapshot.id
              )
              AND NOT EXISTS (
                  SELECT 1 FROM evaluation_snapshot_options item
                  LEFT JOIN evaluation_option_contents content
                    ON content.digest = item.content_digest
                  WHERE item.snapshot_id = snapshot.id
                    AND (content.digest IS NULL OR content.schema_version <> 1
                      OR jsonb_typeof(content.payload) <> 'object'
                      OR NOT content.payload ?& ARRAY[
                          'declared_type', 'value', 'definitions', 'overridden'
                      ]
                      OR jsonb_typeof(content.payload->'declared_type') <> 'string'
                      OR jsonb_typeof(content.payload->'overridden') <> 'boolean'
                      OR jsonb_typeof(content.payload->'definitions') <> 'array'
                      OR jsonb_typeof(content.payload->'value') <> 'object'
                      OR content.payload->'value'->>'kind' NOT IN (
                          'scalar', 'package', 'list', 'attribute_set',
                          'submodule', 'opaque', 'failed'
                      )
                      OR NOT (content.payload->'value' ? 'value')
                      OR EXISTS (
                          SELECT 1
                          FROM jsonb_array_elements(
                              CASE WHEN jsonb_typeof(content.payload->'definitions') = 'array'
                                   THEN content.payload->'definitions' ELSE '[]'::jsonb END
                          ) definition(value)
                          WHERE jsonb_typeof(definition.value) <> 'object'
                             OR jsonb_typeof(definition.value->'source_path') <> 'string'
                             OR jsonb_typeof(definition.value->'winning') <> 'boolean'
                      ))
              )
        )
        "#,
    )
    .bind(snapshot_id)
    .fetch_one(executor)
    .await
    .context("failed to validate evaluation snapshot integrity")
}

fn empty_selected_evaluation_summary(
    selected: &SelectedEvaluationSnapshot,
    lifecycle: SnapshotLifecycle,
    error: Option<String>,
) -> SelectedEvaluationSummary {
    SelectedEvaluationSummary {
        lifecycle,
        revision: selected.revision.clone(),
        generation: selected.generation,
        error,
        module_source_total: 0,
        completed_at: None,
        evaluation_duration_ms: None,
        option_total: 0,
        selected_store_path: None,
        closure_package_count: None,
        closure_size_bytes: None,
        running_store_path: None,
        running_profile_matches: None,
        host_delta_count: None,
        agent_fingerprint: AgentFingerprintStatus::Unavailable,
        seven_day_drift: SevenDayDriftStatus::InsufficientCoverage,
        drift: EvaluationDrift::Unavailable,
    }
}

#[derive(Debug)]
struct ProvenanceResolutionRequest<'a> {
    context_revision: &'a str,
    source_input: Option<&'a str>,
    source_revision: Option<&'a str>,
}

/// Resolves a bounded set of provenance tuples in one database query.
///
/// SECURITY: A result requires an exact active repository/revision match and,
/// for non-admin callers, at least one active system in a visible environment.
/// Multiple matching registered identities deliberately resolve to `None`.
async fn resolve_tracked_provenance<'e, E>(
    executor: E,
    system_id: Uuid,
    visibility_user: Option<Uuid>,
    requests: &[ProvenanceResolutionRequest<'_>],
) -> Result<Vec<Option<TrackedFlakeIdentity>>>
where
    E: Executor<'e, Database = Postgres>,
{
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let encoded = Value::Array(
        requests
            .iter()
            .enumerate()
            .map(|(ordinal, request)| {
                serde_json::json!({
                    "ordinal": ordinal,
                    "context_revision": request.context_revision,
                    "source_input": request.source_input,
                    "source_revision": request.source_revision,
                })
            })
            .collect(),
    );
    let rows = sqlx::query(
        r#"
        WITH requested AS (
            SELECT *
            FROM jsonb_to_recordset($2::jsonb) AS request(
                ordinal bigint, context_revision text, source_input text,
                source_revision text
            )
        ), context AS (
            SELECT request.*, system.flake_id AS source_flake_id,
                   output_content.payload AS output_payload
            FROM requested request
            JOIN systems system ON system.id = $1
            JOIN commits selected_commit
              ON selected_commit.flake_id = system.flake_id
             AND selected_commit.git_commit_hash = request.context_revision
             AND selected_commit.source_archived = false
            LEFT JOIN flake_output_snapshots output_snapshot
              ON output_snapshot.commit_id = selected_commit.id
             AND output_snapshot.lifecycle = 'available'
             AND output_snapshot.schema_version = 1
            LEFT JOIN flake_output_contents output_content
              ON output_content.digest = output_snapshot.content_digest
             AND output_content.schema_version = 1
        ), input_sources AS (
            SELECT DISTINCT context.ordinal, input_name.name,
                   input.value->>'source' AS source_url,
                   input.value->>'locked_revision' AS locked_revision
            FROM context
            CROSS JOIN LATERAL jsonb_array_elements(
                CASE WHEN jsonb_typeof(context.output_payload->'inputs') = 'array'
                     THEN context.output_payload->'inputs' ELSE '[]'::jsonb END
            ) input(value)
            CROSS JOIN LATERAL (
                SELECT jsonb_array_elements_text(
                    CASE WHEN jsonb_typeof(input.value->'names') = 'array'
                         THEN input.value->'names' ELSE '[]'::jsonb END
                ) AS name
                UNION
                SELECT input.value->>'node' AS name
            ) input_name
            WHERE NULLIF(input_name.name, '') IS NOT NULL
              AND NULLIF(input.value->>'source', '') IS NOT NULL
              AND NULLIF(input.value->>'locked_revision', '') IS NOT NULL
        ), candidates AS (
            SELECT context.ordinal, flake.id AS flake_id, flake.name AS flake_name,
                   flake.repo_url, commit.git_commit_hash AS revision
            FROM context
            JOIN flakes flake ON flake.id = context.source_flake_id
                             AND flake.deleted_at IS NULL
            JOIN commits commit ON commit.flake_id = flake.id
                               AND commit.git_commit_hash = context.context_revision
                               AND commit.source_archived = false
            WHERE context.source_input = 'self'
              AND context.source_revision = context.context_revision
              AND ($3::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM systems visible_system
                  JOIN user_environment_memberships membership
                    ON membership.environment_id = visible_system.environment_id
                   AND membership.user_id = $3
                  WHERE visible_system.flake_id = flake.id
                    AND visible_system.is_active = true
              ))
            UNION ALL
            SELECT context.ordinal, flake.id, flake.name, flake.repo_url,
                   commit.git_commit_hash
            FROM context
            JOIN input_sources input
              ON input.ordinal = context.ordinal
             AND input.name = context.source_input
             AND input.locked_revision = context.source_revision
            JOIN flakes flake ON flake.repo_url = input.source_url
                             AND flake.deleted_at IS NULL
            JOIN commits commit ON commit.flake_id = flake.id
                               AND commit.git_commit_hash = context.source_revision
                               AND commit.source_archived = false
            WHERE context.source_input IS DISTINCT FROM 'self'
              AND ($3::uuid IS NULL OR EXISTS (
                  SELECT 1 FROM systems visible_system
                  JOIN user_environment_memberships membership
                    ON membership.environment_id = visible_system.environment_id
                   AND membership.user_id = $3
                  WHERE visible_system.flake_id = flake.id
                    AND visible_system.is_active = true
              ))
        ), resolved AS (
            SELECT ordinal, COUNT(DISTINCT (flake_id, revision)) AS identity_count,
                   MIN(flake_id) AS flake_id, MIN(flake_name) AS flake_name,
                   MIN(repo_url) AS repo_url, MIN(revision) AS revision
            FROM candidates
            GROUP BY ordinal
        )
        SELECT ordinal, flake_id, flake_name, repo_url, revision
        FROM resolved
        WHERE identity_count = 1
        "#,
    )
    .bind(system_id)
    .bind(encoded)
    .bind(visibility_user)
    .fetch_all(executor)
    .await?;

    let mut resolved = vec![None; requests.len()];
    for row in rows {
        let ordinal: i64 = row.try_get("ordinal")?;
        let ordinal = usize::try_from(ordinal).context("negative provenance request ordinal")?;
        let Some(slot) = resolved.get_mut(ordinal) else {
            anyhow::bail!("provenance resolver returned an invalid request ordinal");
        };
        let repo_url: String = row.try_get("repo_url")?;
        *slot = Some(TrackedFlakeIdentity {
            flake_id: row.try_get("flake_id")?,
            flake_name: row.try_get("flake_name")?,
            repo_url: crate::security::snapshot_redaction::redact_text(&repo_url),
            revision: row.try_get("revision")?,
        });
    }
    Ok(resolved)
}

/// Returns one bounded, deterministically ordered module-source page.
///
/// CONCURRENCY: The integrity check, version token, authoritative total, bounded
/// rows, and tracked provenance are read in one read-only repeatable-read
/// transaction. A response therefore represents one persisted snapshot version.
/// Continuations from a replaced version return
/// [`EvaluationModuleSourcesQuery::SnapshotChanged`] without returning rows.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot aggregate or resolve the page.
pub async fn get_evaluation_module_sources_page(
    pool: &PgPool,
    system_id: Uuid,
    selected: &SelectedEvaluationSnapshot,
    visibility_user: Option<Uuid>,
    requested_token: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<EvaluationModuleSourcesQuery> {
    let limit = limit.clamp(1, OPTIONS_PAGE_LIMIT);
    let offset = offset.clamp(0, OPTIONS_OFFSET_LIMIT);
    if selected.lifecycle != SnapshotLifecycle::Available {
        let lifecycle = if selected.lifecycle == SnapshotLifecycle::Available {
            SnapshotLifecycle::Unavailable
        } else {
            selected.lifecycle
        };
        return Ok(EvaluationModuleSourcesQuery::Page(
            EvaluationModuleSourcesPage {
                lifecycle,
                revision: selected.revision.clone(),
                generation: selected.generation,
                error: selected.error.clone().or_else(|| {
                    (lifecycle == SnapshotLifecycle::Unavailable)
                        .then(|| "Snapshot data is unavailable or corrupt".to_string())
                }),
                snapshot_token: None,
                total: 0,
                offset,
                limit,
                sources: Vec::new(),
            },
        ));
    }

    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    if !snapshot_is_usable(&mut *tx, selected.id).await? {
        tx.commit().await?;
        return Ok(EvaluationModuleSourcesQuery::Page(
            EvaluationModuleSourcesPage {
                lifecycle: SnapshotLifecycle::Unavailable,
                revision: selected.revision.clone(),
                generation: selected.generation,
                error: selected
                    .error
                    .clone()
                    .or_else(|| Some("Snapshot data is unavailable or corrupt".to_string())),
                snapshot_token: None,
                total: 0,
                offset,
                limit,
                sources: Vec::new(),
            },
        ));
    }
    let rows = sqlx::query(
        r#"
        WITH selected_snapshot AS (
            SELECT encode(digest(
                convert_to(id::text || ':' || snapshot_version::text, 'UTF8'),
                'sha256'
            ), 'hex') AS snapshot_token
            FROM evaluation_snapshots
            WHERE id = $1
        ), module_rows AS (
            SELECT definition.value->>'source_input' AS source_input,
                   definition.value->>'source_revision' AS source_revision,
                   definition.value->>'source_path' AS source_path,
                   COUNT(*)::bigint AS defined_count,
                   COUNT(*) FILTER (
                       WHERE COALESCE((definition.value->>'winning')::boolean, false)
                   )::bigint AS won_count
            FROM evaluation_snapshot_options item
            JOIN evaluation_option_contents content
              ON content.digest = item.content_digest
            CROSS JOIN LATERAL jsonb_array_elements(content.payload->'definitions')
                definition(value)
            WHERE item.snapshot_id = $1
            GROUP BY definition.value->>'source_input',
                     definition.value->>'source_revision',
                     definition.value->>'source_path'
        )
        SELECT selected_snapshot.snapshot_token,
               (SELECT COUNT(*)::bigint FROM module_rows) AS total,
               page.source_input, page.source_revision, page.source_path,
               page.defined_count, page.won_count
        FROM selected_snapshot
        LEFT JOIN LATERAL (
            SELECT * FROM module_rows
            ORDER BY won_count DESC, defined_count DESC,
                     source_input COLLATE "C" ASC NULLS LAST,
                     source_revision COLLATE "C" ASC NULLS LAST,
                     source_path COLLATE "C" ASC
            LIMIT $2 OFFSET $3
        ) page ON true
        "#,
    )
    .bind(selected.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await?;
    let snapshot_token: String = rows[0].try_get("snapshot_token")?;
    if requested_token.is_some_and(|token| token != snapshot_token.as_str()) {
        tx.commit().await?;
        return Ok(EvaluationModuleSourcesQuery::SnapshotChanged);
    }
    let total: i64 = rows[0].try_get("total")?;
    let mut sources = Vec::new();
    for row in rows {
        let Some(source_path) = row.try_get::<Option<String>, _>("source_path")? else {
            continue;
        };
        sources.push(EvaluationModuleSummary {
            source_input: row.try_get("source_input")?,
            source_revision: row.try_get("source_revision")?,
            source_path,
            defined_count: row.try_get("defined_count")?,
            won_count: row.try_get("won_count")?,
            tracked_flake: None,
        });
    }
    let identities = {
        let requests = sources
            .iter()
            .map(|source| ProvenanceResolutionRequest {
                context_revision: &selected.revision,
                source_input: source.source_input.as_deref(),
                source_revision: source.source_revision.as_deref(),
            })
            .collect::<Vec<_>>();
        resolve_tracked_provenance(&mut *tx, system_id, visibility_user, &requests).await?
    };
    for (source, identity) in sources.iter_mut().zip(identities) {
        source.tracked_flake = identity;
    }

    let page = EvaluationModuleSourcesPage {
        lifecycle: selected.lifecycle,
        revision: selected.revision.clone(),
        generation: selected.generation,
        error: selected.error.clone(),
        snapshot_token: Some(snapshot_token),
        total,
        offset,
        limit,
        sources,
    };
    tx.commit().await?;
    Ok(EvaluationModuleSourcesQuery::Page(page))
}

/// Returns a bounded, server-filtered page from an available snapshot.
///
/// The function decorates all returned selected and baseline definitions with
/// one batched provenance-resolution call. `system_id` supplies the registered
/// flake context, and `visibility_user` applies non-admin environment visibility.
/// The function never resolves identities in the browser or mutates snapshot data.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the page or persisted option
/// data has an invalid shape.
pub async fn query_options_page(
    pool: &PgPool,
    system_id: Uuid,
    selected: &SelectedEvaluationSnapshot,
    visibility_user: Option<Uuid>,
    search: &str,
    filter: EvaluatedOptionFilter,
    limit: i64,
    offset: i64,
) -> Result<EvaluatedOptionsPage> {
    let limit = limit.clamp(1, OPTIONS_PAGE_LIMIT);
    let offset = offset.clamp(0, OPTIONS_OFFSET_LIMIT);

    if selected.lifecycle != SnapshotLifecycle::Available {
        return Ok(EvaluatedOptionsPage {
            lifecycle: selected.lifecycle,
            revision: selected.revision.clone(),
            generation: selected.generation,
            generation_snapshot_id: selected.generation_snapshot_id,
            baseline_revision: None,
            comparison_available: false,
            error: selected.error.clone(),
            module_count: selected.module_count,
            evaluation_duration_ms: selected.evaluation_duration_ms,
            counts: EvaluatedOptionCounts::default(),
            total: 0,
            offset,
            limit,
            options: Vec::new(),
        });
    }

    // INTEGRITY: Selection and page loading are separate queries. Revalidate
    // the complete baseline here so later corruption cannot expose partial
    // before/diff data or make the valid selected snapshot unavailable.
    let baseline_id = match selected.baseline_id {
        Some(id) if snapshot_is_usable(pool, id).await? => Some(id),
        _ => None,
    };
    let comparison_available = baseline_id.is_some();
    let baseline_revision = comparison_available
        .then(|| selected.baseline_revision.clone())
        .flatten();

    let counts_row = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*)::bigint FROM evaluation_snapshot_options WHERE snapshot_id = $1)
                AS all_count,
            (SELECT COUNT(*)::bigint FROM evaluation_snapshot_options
             WHERE snapshot_id = $1 AND is_overridden) AS overridden_count,
            COUNT(*) FILTER (
                WHERE $2::uuid IS NOT NULL
                  AND selected.content_digest IS DISTINCT FROM baseline.content_digest
            )::bigint AS changed_count
        FROM (
            SELECT option_path FROM evaluation_snapshot_options WHERE snapshot_id = $1
            UNION
            SELECT option_path FROM evaluation_snapshot_options WHERE snapshot_id = $2
        ) paths
        LEFT JOIN evaluation_snapshot_options selected
          ON selected.snapshot_id = $1 AND selected.option_path = paths.option_path
        LEFT JOIN evaluation_snapshot_options baseline
          ON baseline.snapshot_id = $2 AND baseline.option_path = paths.option_path
        "#,
    )
    .bind(selected.id)
    .bind(baseline_id)
    .fetch_one(pool)
    .await?;
    let counts = EvaluatedOptionCounts {
        all: counts_row.try_get("all_count")?,
        overridden: counts_row.try_get("overridden_count")?,
        changed: comparison_available.then(|| counts_row.get("changed_count")),
    };

    let search = search
        .trim()
        .chars()
        .take(OPTIONS_SEARCH_LIMIT)
        .collect::<String>();
    let search_pattern = format!("%{}%", escape_like_literal(&search.to_ascii_lowercase()));
    let filter_name = match filter {
        EvaluatedOptionFilter::All => "all",
        EvaluatedOptionFilter::Overridden => "overridden",
        EvaluatedOptionFilter::Changed => "changed",
    };
    let total: i64 = sqlx::query_scalar(
        r#"
        WITH paths AS (
            SELECT option_path FROM evaluation_snapshot_options WHERE snapshot_id = $1
            UNION
            SELECT option_path FROM evaluation_snapshot_options
            WHERE snapshot_id = $2 AND $5 = 'changed'
        )
        SELECT COUNT(*)::bigint
        FROM paths
        LEFT JOIN evaluation_snapshot_options selected
          ON selected.snapshot_id = $1 AND selected.option_path = paths.option_path
        LEFT JOIN evaluation_option_contents content ON content.digest = selected.content_digest
        LEFT JOIN evaluation_snapshot_options baseline
          ON baseline.snapshot_id = $2 AND baseline.option_path = paths.option_path
        LEFT JOIN evaluation_option_contents baseline_content
          ON baseline_content.digest = baseline.content_digest
        WHERE ($3 = '' OR lower(paths.option_path || ' ' || COALESCE(
                     content.search_text, baseline_content.search_text, ''
               )) LIKE $4 ESCAPE '\')
          AND (
              ($5 = 'all' AND selected.snapshot_id IS NOT NULL)
              OR ($5 = 'overridden' AND selected.is_overridden)
              OR ($5 = 'changed' AND $2::uuid IS NOT NULL
                  AND baseline.content_digest IS DISTINCT FROM selected.content_digest)
          )
        "#,
    )
    .bind(selected.id)
    .bind(baseline_id)
    .bind(&search)
    .bind(&search_pattern)
    .bind(filter_name)
    .fetch_one(pool)
    .await?;
    let mut query = QueryBuilder::<Postgres>::new(
        "WITH paths AS (SELECT option_path FROM evaluation_snapshot_options WHERE snapshot_id = ",
    );
    query.push_bind(selected.id);
    if matches!(filter, EvaluatedOptionFilter::Changed) && comparison_available {
        query.push(
            " UNION SELECT option_path FROM evaluation_snapshot_options WHERE snapshot_id = ",
        );
        query.push_bind(baseline_id);
    }
    query.push(
        ") SELECT paths.option_path, content.payload, \
                baseline_content.payload AS before_payload, \
                baseline.content_digest IS DISTINCT FROM selected.content_digest AS changed \
                FROM paths LEFT JOIN evaluation_snapshot_options selected \
                ON selected.snapshot_id = ",
    );
    query.push_bind(selected.id);
    query.push(" AND selected.option_path = paths.option_path \
                LEFT JOIN evaluation_option_contents content ON content.digest = selected.content_digest \
                LEFT JOIN evaluation_snapshot_options baseline ON baseline.snapshot_id = ");
    query.push_bind(baseline_id);
    query.push(" AND baseline.option_path = paths.option_path \
                LEFT JOIN evaluation_option_contents baseline_content ON baseline_content.digest = baseline.content_digest \
                WHERE true");
    if !search.is_empty() {
        query.push(" AND lower(paths.option_path || ' ' || COALESCE(content.search_text, baseline_content.search_text, '')) LIKE ");
        query.push_bind(search_pattern);
        query.push(" ESCAPE '\\'");
    }
    match filter {
        EvaluatedOptionFilter::All => {}
        EvaluatedOptionFilter::Overridden => {
            query.push(" AND selected.is_overridden");
        }
        EvaluatedOptionFilter::Changed if comparison_available => {
            query.push(" AND baseline.content_digest IS DISTINCT FROM selected.content_digest");
        }
        EvaluatedOptionFilter::Changed => {
            query.push(" AND false");
        }
    };
    query.push(" ORDER BY paths.option_path LIMIT ");
    query.push_bind(limit);
    query.push(" OFFSET ");
    query.push_bind(offset);

    let rows = query.build().fetch_all(pool).await?;
    let decoded = rows
        .into_iter()
        .map(|row| {
            let path: String = row.try_get("option_path")?;
            let option = row
                .try_get::<Option<Value>, _>("payload")?
                .map(|payload| decode_option(path.clone(), payload))
                .transpose()?;
            let before = row
                .try_get::<Option<Value>, _>("before_payload")?
                .map(|payload| decode_option(path, payload))
                .transpose();
            let before = match before {
                Ok(before) => before,
                Err(_) => return Ok(None),
            };
            let changed = comparison_available.then(|| row.get("changed"));
            Ok(Some(EvaluatedOptionRow {
                diff: comparison_available
                    .then(|| typed_option_diff(before.as_ref(), option.as_ref())),
                option,
                before,
                changed,
            }))
        })
        .collect::<Result<Vec<_>>>();
    let mut options: Vec<EvaluatedOptionRow> = match decoded {
        Ok(options) if options.iter().all(Option::is_some) => {
            options.into_iter().flatten().collect()
        }
        Ok(_) => {
            let mut isolated = selected.clone();
            isolated.baseline_id = None;
            isolated.baseline_revision = None;
            return Box::pin(query_options_page(
                pool,
                system_id,
                &isolated,
                visibility_user,
                search.as_str(),
                filter,
                limit,
                offset,
            ))
            .await;
        }
        Err(_) => {
            return Ok(EvaluatedOptionsPage {
                lifecycle: SnapshotLifecycle::Unavailable,
                revision: selected.revision.clone(),
                generation: selected.generation,
                generation_snapshot_id: selected.generation_snapshot_id,
                baseline_revision: selected.baseline_revision.clone(),
                comparison_available: false,
                error: Some("Snapshot data is unavailable or corrupt".to_string()),
                module_count: selected.module_count,
                evaluation_duration_ms: selected.evaluation_duration_ms,
                counts: EvaluatedOptionCounts::default(),
                total: 0,
                offset,
                limit,
                options: Vec::new(),
            });
        }
    };

    // PERFORMANCE: Collect both selected and baseline definitions, then resolve
    // every returned provenance tuple in one set-based query. Page size bounds
    // the request independently of the complete snapshot size.
    let mut candidates = Vec::new();
    let mut locations = Vec::new();
    for (row_index, row) in options.iter().enumerate() {
        if let Some(option) = &row.option {
            for (definition_index, definition) in option.definitions.iter().enumerate() {
                candidates.push((
                    selected.revision.clone(),
                    definition.source_input.clone(),
                    definition.source_revision.clone(),
                ));
                locations.push((row_index, false, definition_index));
            }
        }
        if let (Some(before), Some(context_revision)) = (&row.before, &baseline_revision) {
            for (definition_index, definition) in before.definitions.iter().enumerate() {
                candidates.push((
                    context_revision.clone(),
                    definition.source_input.clone(),
                    definition.source_revision.clone(),
                ));
                locations.push((row_index, true, definition_index));
            }
        }
    }
    let requests = candidates
        .iter()
        .map(
            |(context_revision, source_input, source_revision)| ProvenanceResolutionRequest {
                context_revision,
                source_input: source_input.as_deref(),
                source_revision: source_revision.as_deref(),
            },
        )
        .collect::<Vec<_>>();
    let identities =
        resolve_tracked_provenance(pool, system_id, visibility_user, &requests).await?;
    for ((row_index, before, definition_index), identity) in locations.into_iter().zip(identities) {
        let option = if before {
            options[row_index].before.as_mut()
        } else {
            options[row_index].option.as_mut()
        };
        if let Some(definition) =
            option.and_then(|value| value.definitions.get_mut(definition_index))
        {
            definition.tracked_flake = identity;
        }
    }

    Ok(EvaluatedOptionsPage {
        lifecycle: selected.lifecycle,
        revision: selected.revision.clone(),
        generation: selected.generation,
        generation_snapshot_id: selected.generation_snapshot_id,
        baseline_revision,
        comparison_available,
        error: selected.error.clone(),
        module_count: selected.module_count,
        evaluation_duration_ms: selected.evaluation_duration_ms,
        counts,
        total,
        offset,
        limit,
        options,
    })
}

fn escape_like_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn paginate_flake_payload(payload: &mut Value, offset: usize, limit: usize) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };
    for key in ["declared_systems", "exported_modules", "inputs"] {
        if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
            *items = items.iter().skip(offset).take(limit).cloned().collect();
        }
    }
    if let Some(modules) = object
        .get_mut("exported_modules")
        .and_then(Value::as_array_mut)
    {
        for module in modules {
            if let Some(declarations) = module.get_mut("declarations").and_then(Value::as_array_mut)
            {
                let complete = declarations.is_empty();
                declarations.clear();
                if let Some(object) = module.as_object_mut() {
                    object.insert("declarations_complete".into(), Value::Bool(complete));
                }
            }
        }
    }
}

fn valid_flake_payload(payload: &Value) -> bool {
    let Some(object) = payload.as_object() else {
        return false;
    };
    ["declared_systems", "exported_modules", "inputs"]
        .iter()
        .all(|key| object.get(*key).is_some_and(Value::is_array))
}

fn add_read_time_input_age(payload: &mut Value) {
    let now = chrono::Utc::now().timestamp();
    let Some(inputs) = payload.get_mut("inputs").and_then(Value::as_array_mut) else {
        return;
    };
    for input in inputs {
        let Some(object) = input.as_object_mut() else {
            continue;
        };
        let age_days = object
            .get("last_modified")
            .and_then(Value::as_i64)
            .map(|timestamp| now.saturating_sub(timestamp).max(0) / 86_400);
        object.insert("age_days".into(), age_days.map_or(Value::Null, Value::from));
        object.insert(
            "stale".into(),
            Value::Bool(age_days.is_some_and(|age| age > 90)),
        );
    }
}

fn filter_payload_configurations(
    payload: &mut Value,
    visible: &std::collections::BTreeSet<String>,
) {
    let keep = |name: &str| visible.contains(name);
    if let Some(systems) = payload
        .get_mut("declared_systems")
        .and_then(Value::as_array_mut)
    {
        systems.retain(|system| system.as_str().is_some_and(keep));
    }
    if let Some(modules) = payload
        .get_mut("exported_modules")
        .and_then(Value::as_array_mut)
    {
        for module in modules {
            if let Some(consumers) = module.get_mut("consumers").and_then(Value::as_array_mut) {
                consumers.retain(|consumer| consumer.as_str().is_some_and(keep));
                let consumer_count = consumers.len();
                if let Some(object) = module.as_object_mut() {
                    object.insert("consumer_count".into(), Value::from(consumer_count));
                }
            }
        }
    }
}

fn decode_option(path: String, mut payload: Value) -> Result<EvaluatedOption> {
    payload
        .as_object_mut()
        .context("evaluation option payload is not an object")?
        .insert("path".into(), Value::String(path));
    serde_json::from_value(payload).context("evaluation option payload is corrupt")
}

fn parse_lifecycle(value: String) -> Result<SnapshotLifecycle> {
    match value.as_str() {
        "queued" => Ok(SnapshotLifecycle::Queued),
        "running" => Ok(SnapshotLifecycle::Running),
        "failed" => Ok(SnapshotLifecycle::Failed),
        "available" => Ok(SnapshotLifecycle::Available),
        "unavailable" => Ok(SnapshotLifecycle::Unavailable),
        _ => anyhow::bail!("unknown snapshot lifecycle {value}"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    use crate::models::commits::Commit;
    use crate::models::evaluation_snapshots::{
        OptionChangeKind, OptionDefinitionProvenance, SafeOptionValue,
    };
    use crate::models::public_key::PublicKey;
    use crate::models::systems::System;
    use crate::queries::commits::{
        get_commit_by_hash, insert_commit_with_metadata, set_commit_first_parent_by_repo_url,
    };
    use crate::queries::derivations::insert_derivation;
    use crate::queries::environments::create_environment;
    use crate::queries::flakes::{
        accept_history_rewrite_reset, count_systems_for_flake, insert_flake, reset_flake_source,
    };
    use crate::queries::systems::insert_system;
    use crate::queries::users::insert_user;
    use crate::security::snapshot_redaction::REDACTED_VALUE;

    use super::*;

    fn option(path: &str, value: Value) -> EvaluatedOption {
        EvaluatedOption {
            path: path.into(),
            declared_type: "string".into(),
            value: SafeOptionValue::Scalar(value),
            definitions: vec![OptionDefinitionProvenance {
                source_path: "/nix/store/source/module.nix".into(),
                source_input: Some("self".into()),
                source_revision: None,
                value: None,
                winning: true,
                priority: Some(100),
                status: Some("winning".into()),
                winner_note: None,
                tracked_flake: None,
            }],
            overridden: false,
        }
    }

    fn module_sources_page(query: EvaluationModuleSourcesQuery) -> EvaluationModuleSourcesPage {
        match query {
            EvaluationModuleSourcesQuery::Page(page) => page,
            EvaluationModuleSourcesQuery::SnapshotChanged => {
                panic!("fresh module-source query must not report replacement")
            }
        }
    }

    #[test]
    fn flake_output_token_binds_selected_and_first_parent_state() {
        let selected = [1_u8; 32];
        let previous = [2_u8; 32];
        let replacement = [3_u8; 32];
        let parent = "a".repeat(40);
        let token =
            flake_output_snapshot_token(Some(&selected), true, Some(&parent), Some(&previous))
                .expect("available selected content should produce a token");

        assert_eq!(token.len(), 64);
        assert_ne!(
            token,
            flake_output_snapshot_token(Some(&replacement), true, Some(&parent), Some(&previous))
                .expect("selected replacement should produce a token")
        );
        assert_ne!(
            token,
            flake_output_snapshot_token(Some(&selected), true, Some(&parent), Some(&replacement))
                .expect("parent replacement should produce a token")
        );
        assert_ne!(
            token,
            flake_output_snapshot_token(Some(&selected), true, None, None)
                .expect("root state should produce a token")
        );
        assert_ne!(
            flake_output_snapshot_token(Some(&selected), false, None, None),
            flake_output_snapshot_token(Some(&selected), true, None, None),
            "unresolved and resolved root states must not share a token"
        );
    }

    async fn insert_test_commit(pool: &PgPool, repo_url: &str, hash: &str) -> Commit {
        insert_commit_with_metadata(pool, hash, repo_url, Utc::now(), Some("test"), Some("test"))
            .await
            .expect("commit insert should succeed");
        get_commit_by_hash(pool, hash)
            .await
            .expect("commit should be readable")
    }

    async fn insert_test_system(pool: &PgPool, flake_id: i32, suffix: &str) -> System {
        let key = SigningKey::from_bytes(&[41; 32]);
        insert_system(
            pool,
            &System {
                id: Uuid::new_v4(),
                hostname: format!("host-{suffix}"),
                environment_id: None,
                is_active: true,
                public_key: PublicKey::from_verifying_key(key.verifying_key()),
                flake_id: Some(flake_id),
                derivation: String::new(),
                system_configuration_name: Some("host".into()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                desired_target: None,
                deployment_policy: "manual".into(),
            },
        )
        .await
        .expect("system insert should succeed")
    }

    async fn persist_test_snapshot(pool: &PgPool, commit: &Commit, value: &str) -> Uuid {
        let mut tx = pool.begin().await.expect("transaction should begin");
        let snapshot_id = persist_available_snapshot_tx(
            &mut tx,
            commit.id,
            "host",
            vec![option("services.example.value", json!(value))],
        )
        .await
        .expect("snapshot should persist");
        tx.commit().await.expect("transaction should commit");
        snapshot_id
    }

    async fn retain_test_generation(
        pool: &PgPool,
        system_id: Uuid,
        generation: i32,
        commit: &Commit,
        snapshot_id: Uuid,
    ) {
        let derivation = insert_derivation(pool, Some(commit), "host", "nixos")
            .await
            .expect("generation derivation should persist");
        sqlx::query(
            r#"
            INSERT INTO evaluation_generation_snapshots (
                system_id, generation, snapshot_id, derivation_id, commit_id
            ) VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(system_id)
        .bind(generation)
        .bind(snapshot_id)
        .bind(derivation.id)
        .bind(commit.id)
        .execute(pool)
        .await
        .expect("generation snapshot should be retained");
    }

    async fn assert_comparison_isolated(pool: &PgPool, selected: &SelectedEvaluationSnapshot) {
        let page = query_options_page(
            pool,
            Uuid::nil(),
            selected,
            None,
            "",
            EvaluatedOptionFilter::All,
            100,
            0,
        )
        .await
        .expect("selected snapshot should remain readable");
        assert_eq!(page.lifecycle, SnapshotLifecycle::Available);
        assert!(!page.comparison_available);
        assert!(page.baseline_revision.is_none());
        assert_eq!(page.counts.changed, None);
        assert_eq!(page.options.len(), 1);
        assert!(page.options[0].before.is_none());
        assert!(page.options[0].diff.is_none());
        assert!(page.options[0].changed.is_none());

        let changed = query_options_page(
            pool,
            Uuid::nil(),
            selected,
            None,
            "",
            EvaluatedOptionFilter::Changed,
            100,
            0,
        )
        .await
        .expect("Changed should remain a valid empty page without a baseline");
        assert!(!changed.comparison_available);
        assert_eq!(changed.counts.changed, None);
        assert_eq!(changed.total, 0);
        assert!(changed.options.is_empty());
    }

    #[test]
    fn request_bounds_are_finite() {
        assert_eq!(500_i64.clamp(1, OPTIONS_PAGE_LIMIT), 100);
        assert_eq!(i64::MAX.clamp(0, OPTIONS_OFFSET_LIMIT), 100_000);
    }

    #[test]
    fn lifecycle_parser_does_not_collapse_unavailable_and_failed() {
        assert_eq!(
            parse_lifecycle("unavailable".into()).unwrap(),
            SnapshotLifecycle::Unavailable
        );
        assert_eq!(
            parse_lifecycle("failed".into()).unwrap(),
            SnapshotLifecycle::Failed
        );
    }

    #[test]
    fn payload_validation_rejects_corrupt_or_incomplete_flake_snapshots() {
        assert!(!valid_flake_payload(&json!(null)));
        assert!(!valid_flake_payload(&json!({"declared_systems": []})));
        assert!(valid_flake_payload(&json!({
            "declared_systems": [],
            "exported_modules": [],
            "inputs": []
        })));
    }

    #[test]
    fn option_decoder_rejects_invalid_persisted_shapes() {
        let error = decode_option("services.example.enable".into(), json!({"value": true}))
            .expect_err("an incomplete option payload must be corrupt");
        assert!(error.to_string().contains("corrupt"));
    }

    #[test]
    fn oversized_individual_option_becomes_explicit_opaque_value() {
        let oversized = option(
            "services.example.large",
            json!("x".repeat(OPTION_CONTENT_BYTES_LIMIT + 1)),
        );

        let bounded = bounded_option_payload(oversized).expect("bounding should not fail");
        assert_eq!(
            bounded.declared_type,
            "unknown (oversized evaluator payload)"
        );
        assert_eq!(
            bounded.value,
            SafeOptionValue::Opaque {
                type_name: "oversized".into()
            }
        );
        assert!(bounded.definitions.is_empty());
    }

    #[test]
    fn hidden_configurations_are_removed_from_flake_output_payloads() {
        let mut payload = json!({
            "declared_systems": ["visible", "hidden", "shared"],
            "exported_modules": [{
                "name": "example",
                "consumer_count": 3,
                "consumers": ["visible", "hidden", "shared"]
            }]
        });
        let visible = ["visible".to_string(), "shared".to_string()]
            .into_iter()
            .collect();

        filter_payload_configurations(&mut payload, &visible);

        assert_eq!(payload["declared_systems"], json!(["visible", "shared"]));
        assert_eq!(
            payload["exported_modules"][0]["consumers"],
            json!(["visible", "shared"])
        );
        assert_eq!(payload["exported_modules"][0]["consumer_count"], 2);
    }

    #[test]
    fn option_search_escapes_sql_like_metacharacters() {
        assert_eq!(escape_like_literal(r"foo%_\bar"), r"foo\%\_\\bar");
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn failed_and_corrupt_snapshots_requeue_with_active_lifecycle(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/requeue-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("requeue-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let commit = insert_test_commit(&pool, &repo_url, &"e".repeat(40)).await;
        let system = insert_test_system(&pool, flake.id, &suffix).await;

        let mut tx = pool.begin().await.expect("transaction should begin");
        persist_failed_snapshot_tx(&mut tx, commit.id, "host", "safe failure")
            .await
            .expect("failed lifecycle should persist");
        tx.commit().await.expect("transaction should commit");
        sqlx::query(
            "UPDATE evaluation_attempts SET status = 'failed', completed_at = NOW() WHERE commit_id = $1 AND status = 'queued'",
        )
        .bind(commit.id)
        .execute(&pool)
        .await
        .expect("failed attempt lifecycle should persist");
        sqlx::query("UPDATE commits SET evaluation_status = 'failed' WHERE id = $1")
            .bind(commit.id)
            .execute(&pool)
            .await
            .expect("failed commit lifecycle should persist");

        let queued = queue_or_reuse_evaluation(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("failed snapshot should be queueable")
            .expect("revision should remain active");
        assert!(queued.queued);
        assert_eq!(queued.lifecycle, SnapshotLifecycle::Queued);
        let selected = select_commit_snapshot(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("queued snapshot should load")
            .expect("snapshot row should remain selected");
        assert_eq!(selected.lifecycle, SnapshotLifecycle::Queued);
        assert!(selected.error.is_none());

        sqlx::query("UPDATE commits SET evaluation_status = 'in_progress' WHERE id = $1")
            .bind(commit.id)
            .execute(&pool)
            .await
            .expect("active lifecycle should persist");
        sqlx::query(
            "UPDATE evaluation_attempts SET status = 'in_progress', started_at = NOW() WHERE commit_id = $1 AND status = 'queued'",
        )
        .bind(commit.id)
        .execute(&pool)
        .await
        .expect("active attempt lifecycle should persist");
        let selected = select_commit_snapshot(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("running snapshot should load")
            .expect("snapshot row should remain selected");
        assert_eq!(selected.lifecycle, SnapshotLifecycle::Running);

        let mut tx = pool.begin().await.expect("transaction should begin");
        persist_available_snapshot_tx(
            &mut tx,
            commit.id,
            "host",
            vec![option("services.example.enable", json!(true))],
        )
        .await
        .expect("available snapshot should persist");
        tx.commit().await.expect("transaction should commit");
        sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
            .bind(commit.id)
            .execute(&pool)
            .await
            .expect("terminal lifecycle should persist");
        sqlx::query(
            "UPDATE evaluation_attempts SET status = 'complete', completed_at = NOW() WHERE commit_id = $1 AND status = 'in_progress'",
        )
        .bind(commit.id)
        .execute(&pool)
        .await
        .expect("terminal attempt lifecycle should persist");
        sqlx::query(
            "UPDATE evaluation_snapshots SET option_count = option_count + 1 WHERE commit_id = $1 AND configuration_name = 'host'",
        )
        .bind(commit.id)
        .execute(&pool)
        .await
        .expect("test corruption should persist");

        let corrupt = select_commit_snapshot(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("corrupt snapshot should load")
            .expect("snapshot row should remain selected");
        assert_eq!(corrupt.lifecycle, SnapshotLifecycle::Unavailable);
        let queued = queue_or_reuse_evaluation(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("corrupt snapshot should be queueable")
            .expect("revision should remain active");
        assert!(queued.queued);
        assert_eq!(queued.lifecycle, SnapshotLifecycle::Queued);
        let selected = select_commit_snapshot(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("requeued corrupt snapshot should load")
            .expect("snapshot row should remain selected");
        assert_eq!(selected.lifecycle, SnapshotLifecycle::Queued);
        assert!(selected.error.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn snapshots_deduplicate_and_full_sha_prefixes_do_not_alias(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("flake-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let one_hash = format!("abcdef0{}", "1".repeat(33));
        let two_hash = format!("abcdef0{}", "2".repeat(33));
        let one = insert_test_commit(&pool, &repo_url, &one_hash).await;
        let two = insert_test_commit(&pool, &repo_url, &two_hash).await;
        let missing_parent = "f".repeat(40);
        set_commit_first_parent_by_repo_url(&pool, &repo_url, &two_hash, Some(&missing_parent))
            .await
            .expect("missing parent identity should persist");

        for commit in [&one, &two] {
            let mut tx = pool.begin().await.expect("transaction should begin");
            persist_available_snapshot_tx(
                &mut tx,
                commit.id,
                "host",
                vec![option("services.example.enable", json!(true))],
            )
            .await
            .expect("snapshot should persist");
            persist_flake_output_snapshot_tx(
                &mut tx,
                commit.id,
                &json!({"declared_systems": [], "exported_modules": [], "inputs": []}),
            )
            .await
            .expect("flake output snapshot should persist");
            tx.commit().await.expect("transaction should commit");
        }

        let content_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM evaluation_option_contents")
                .fetch_one(&pool)
                .await
                .expect("content count should load");
        let snapshot_hashes: Vec<String> = sqlx::query_scalar(
            "SELECT c.git_commit_hash FROM evaluation_snapshots es JOIN commits c ON c.id = es.commit_id ORDER BY c.git_commit_hash",
        )
        .fetch_all(&pool)
        .await
        .expect("snapshot identities should load");
        assert_eq!(
            content_count, 1,
            "identical redacted payloads must deduplicate"
        );
        assert_eq!(snapshot_hashes, [one_hash, two_hash]);
        let root = get_flake_output_snapshot(
            &pool,
            flake.id,
            &one.git_commit_hash,
            None,
            FlakeSystemFilter::All,
            50,
            0,
        )
        .await
        .expect("root snapshot query should succeed")
        .expect("root snapshot should exist");
        assert!(root.first_parent_revision.is_none());
        assert!(!root.comparison_available);
        assert!(root.delta.is_none());
        let missing = get_flake_output_snapshot(
            &pool,
            flake.id,
            &two.git_commit_hash,
            None,
            FlakeSystemFilter::All,
            50,
            0,
        )
        .await
        .expect("missing-parent query should succeed")
        .expect("selected snapshot should exist");
        assert_eq!(
            missing.first_parent_revision.as_deref(),
            Some(missing_parent.as_str())
        );
        assert!(!missing.comparison_available);
        assert!(missing.delta.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn secrets_are_absent_while_safe_values_remain_searchable_in_storage_and_api(
        pool: PgPool,
    ) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/value-redaction-{suffix}.git");
        insert_flake(
            &pool,
            &format!("value-redaction-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let commit = insert_test_commit(&pool, &repo_url, &"c".repeat(40)).await;
        let option = EvaluatedOption {
            path: "services.example.aliases".into(),
            declared_type: "attribute set".into(),
            value: SafeOptionValue::AttributeSet(
                serde_json::from_value(json!({
                    "GITHUB_PAT": "github-secret-value",
                    "nested": {"clientSecret": "nested-secret-value"},
                    "token": "token-key-secret",
                    "repository": "https://user:pass@example.test/repo?token=url-secret",
                    "authorizationValue": "Bearer bearer-secret-value",
                    "jwtValue": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlMTIzNDU2Nzg5MA",
                    "cloudValue": "AKIAIOSFODNN7EXAMPLE",
                    "displayName": "production package collection",
                    "enabled": true,
                    "attempts": 4
                }))
                .expect("attribute set shape"),
            ),
            definitions: vec![OptionDefinitionProvenance {
                source_path: "https://example.test/module.nix".into(),
                source_input: Some("self".into()),
                source_revision: Some("c".repeat(40)),
                value: Some(json!({"kind": "scalar", "value": "documented module default"})),
                winning: true,
                priority: Some(100),
                status: Some("winning".into()),
                winner_note: Some("A lower numeric module-system priority won.".into()),
                tracked_flake: None,
            }],
            overridden: false,
        };
        let mut tx = pool.begin().await.expect("transaction should begin");
        let snapshot_id = persist_available_snapshot_tx(&mut tx, commit.id, "host", vec![option])
            .await
            .expect("snapshot should persist");
        tx.commit().await.expect("transaction should commit");

        let persisted = sqlx::query_as::<_, (String, String)>(
            "SELECT payload::text, search_text FROM evaluation_option_contents LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("persisted payload should load");
        for secret in [
            "GITHUB_PAT",
            "github-secret-value",
            "clientSecret",
            "nested-secret-value",
            "token-key-secret",
            "user:pass",
            "url-secret",
            "bearer-secret-value",
            "eyJhbGci",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            assert!(!persisted.0.contains(secret));
            assert!(!persisted.1.contains(secret));
        }

        let selected = SelectedEvaluationSnapshot {
            id: snapshot_id,
            revision: commit.git_commit_hash.clone(),
            lifecycle: SnapshotLifecycle::Available,
            error: None,
            baseline_id: None,
            baseline_revision: None,
            generation: None,
            generation_snapshot_id: None,
            module_count: 1,
            evaluation_duration_ms: None,
        };
        for secret in [
            "github-secret-value",
            "nested-secret-value",
            "token-key-secret",
            "url-secret",
            "bearer-secret-value",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            let search = query_options_page(
                &pool,
                Uuid::nil(),
                &selected,
                None,
                secret,
                EvaluatedOptionFilter::All,
                100,
                0,
            )
            .await
            .expect("search should succeed");
            assert_eq!(search.total, 0, "redacted value must not be searchable");
        }
        let page = query_options_page(
            &pool,
            Uuid::nil(),
            &selected,
            None,
            "",
            EvaluatedOptionFilter::All,
            100,
            0,
        )
        .await
        .expect("API page should load");
        let api_json = serde_json::to_string(&page).expect("API page should serialize");
        for secret in [
            "GITHUB_PAT",
            "github-secret-value",
            "clientSecret",
            "nested-secret-value",
            "token-key-secret",
            "user:pass",
            "url-secret",
            "bearer-secret-value",
            "eyJhbGci",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            assert!(!api_json.contains(secret));
        }
        assert!(persisted.0.contains("production package collection"));
        assert!(persisted.0.contains("documented module default"));
        assert!(persisted.0.contains("lower numeric module-system priority"));
        let safe_search = query_options_page(
            &pool,
            Uuid::nil(),
            &selected,
            None,
            "production package collection",
            EvaluatedOptionFilter::All,
            100,
            0,
        )
        .await
        .expect("safe search should succeed");
        assert_eq!(safe_search.total, 1);
        assert!(api_json.contains("production package collection"));
        assert!(api_json.contains("true"));
        assert!(api_json.contains('4'));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn numeric_and_boolean_secrets_are_redacted_in_storage_search_and_api(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/scalar-redaction-{suffix}.git");
        insert_flake(
            &pool,
            &format!("scalar-redaction-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let commit = insert_test_commit(&pool, &repo_url, &"d".repeat(40)).await;
        let options = vec![
            EvaluatedOption {
                path: "services.example.apiTokenNumber".into(),
                declared_type: "integer".into(),
                value: SafeOptionValue::Scalar(json!(8675309)),
                definitions: Vec::new(),
                overridden: false,
            },
            EvaluatedOption {
                path: "services.example.secretEnabled".into(),
                declared_type: "boolean".into(),
                value: SafeOptionValue::Scalar(json!(true)),
                definitions: Vec::new(),
                overridden: false,
            },
        ];
        let mut tx = pool.begin().await.expect("transaction should begin");
        let snapshot_id = persist_available_snapshot_tx(&mut tx, commit.id, "host", options)
            .await
            .expect("snapshot should persist");
        tx.commit().await.expect("transaction should commit");

        let persisted = sqlx::query_as::<_, (String, Value, String)>(
            r#"
            SELECT paths.option_path, content.payload, content.search_text
            FROM evaluation_snapshot_options paths
            JOIN evaluation_option_contents content ON content.digest = paths.content_digest
            WHERE paths.snapshot_id = $1
            ORDER BY paths.option_path
            "#,
        )
        .bind(snapshot_id)
        .fetch_all(&pool)
        .await
        .expect("persisted scalar payloads should load");
        assert_eq!(persisted.len(), 2);
        for (_, payload, search_text) in &persisted {
            assert_eq!(
                payload["value"],
                json!({"kind": "scalar", "value": REDACTED_VALUE})
            );
            assert!(!search_text.contains("8675309"));
            assert!(!search_text.contains("true"));
        }

        let selected = SelectedEvaluationSnapshot {
            id: snapshot_id,
            revision: commit.git_commit_hash,
            lifecycle: SnapshotLifecycle::Available,
            error: None,
            baseline_id: None,
            baseline_revision: None,
            generation: None,
            generation_snapshot_id: None,
            module_count: 0,
            evaluation_duration_ms: None,
        };
        for secret in ["8675309", "true"] {
            let page = query_options_page(
                &pool,
                Uuid::nil(),
                &selected,
                None,
                secret,
                EvaluatedOptionFilter::All,
                100,
                0,
            )
            .await
            .expect("secret search should succeed");
            assert_eq!(page.total, 0, "scalar secret must not be searchable");
        }
        let page = query_options_page(
            &pool,
            Uuid::nil(),
            &selected,
            None,
            "",
            EvaluatedOptionFilter::All,
            100,
            0,
        )
        .await
        .expect("API page should load");
        assert_eq!(page.options.len(), 2);
        for row in page.options {
            let option = row.option.expect("current option should be present");
            assert_eq!(option.value, SafeOptionValue::Scalar(json!(REDACTED_VALUE)));
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn changed_query_is_symmetric_bounded_and_side_effect_free(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/diff-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("flake-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let before_hash = "a".repeat(40);
        let after_hash = "b".repeat(40);
        let before = insert_test_commit(&pool, &repo_url, &before_hash).await;
        let after = insert_test_commit(&pool, &repo_url, &after_hash).await;
        set_commit_first_parent_by_repo_url(&pool, &repo_url, &after_hash, Some(&before_hash))
            .await
            .expect("parent should persist");

        let key = SigningKey::from_bytes(&[42; 32]);
        let system = System {
            id: Uuid::new_v4(),
            hostname: format!("host-{suffix}"),
            environment_id: None,
            is_active: true,
            public_key: PublicKey::from_verifying_key(key.verifying_key()),
            flake_id: Some(flake.id),
            derivation: String::new(),
            system_configuration_name: Some("host".into()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".into(),
        };
        let system = insert_system(&pool, &system)
            .await
            .expect("system insert should succeed");

        let mut tx = pool.begin().await.expect("transaction should begin");
        persist_available_snapshot_tx(
            &mut tx,
            before.id,
            "host",
            vec![
                option("common", json!("before")),
                option("removed", json!(true)),
            ],
        )
        .await
        .expect("baseline should persist");
        persist_available_snapshot_tx(
            &mut tx,
            after.id,
            "host",
            vec![
                option("added", json!(true)),
                option("common", json!("after")),
            ],
        )
        .await
        .expect("selected snapshot should persist");
        tx.commit().await.expect("transaction should commit");

        let status_before: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(after.id)
                .fetch_one(&pool)
                .await
                .expect("status should load");
        let selected = select_commit_snapshot(&pool, system.id, &after_hash)
            .await
            .expect("selection should succeed")
            .expect("snapshot should exist");
        let first_page = query_options_page(
            &pool,
            system.id,
            &selected,
            None,
            "",
            EvaluatedOptionFilter::Changed,
            2,
            0,
        )
        .await
        .expect("first page should load");
        let second_page = query_options_page(
            &pool,
            system.id,
            &selected,
            None,
            "",
            EvaluatedOptionFilter::Changed,
            2,
            2,
        )
        .await
        .expect("second page should load");
        let status_after: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(after.id)
                .fetch_one(&pool)
                .await
                .expect("status should reload");

        assert_eq!(first_page.counts.all, 2);
        assert_eq!(first_page.counts.changed, Some(3));
        assert_eq!(first_page.total, 3);
        assert_eq!(first_page.options.len(), 2);
        assert_eq!(second_page.options.len(), 1);
        assert!(
            first_page
                .options
                .iter()
                .chain(&second_page.options)
                .any(|row| {
                    row.diff.as_ref().map(|diff| diff.kind) == Some(OptionChangeKind::Removed)
                        && row.option.is_none()
                })
        );
        assert_eq!(
            status_before, status_after,
            "reads must not queue evaluation"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn comparison_baseline_failures_are_isolated_for_commits_and_generations(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/baseline-isolation-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("baseline-isolation-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let system = insert_test_system(&pool, flake.id, &suffix).await;

        let mut commits = Vec::new();
        for index in 1_u8..=11 {
            commits.push(insert_test_commit(&pool, &repo_url, &format!("{index:040x}")).await);
        }
        let mut snapshots = Vec::new();
        for (index, commit) in commits.iter().enumerate() {
            snapshots.push(persist_test_snapshot(&pool, commit, &format!("value-{index}")).await);
        }

        for (selected, baseline) in [(1, 0), (3, 2), (5, 4), (7, 6)] {
            set_commit_first_parent_by_repo_url(
                &pool,
                &repo_url,
                &commits[selected].git_commit_hash,
                Some(&commits[baseline].git_commit_hash),
            )
            .await
            .expect("first-parent identity should persist");
        }

        retain_test_generation(&pool, system.id, 10, &commits[0], snapshots[0]).await;
        retain_test_generation(&pool, system.id, 11, &commits[1], snapshots[1]).await;
        retain_test_generation(&pool, system.id, 20, &commits[8], snapshots[8]).await;
        retain_test_generation(&pool, system.id, 21, &commits[2], snapshots[2]).await;
        retain_test_generation(&pool, system.id, 22, &commits[3], snapshots[3]).await;
        retain_test_generation(&pool, system.id, 30, &commits[9], snapshots[9]).await;
        retain_test_generation(&pool, system.id, 31, &commits[4], snapshots[4]).await;
        retain_test_generation(&pool, system.id, 32, &commits[5], snapshots[5]).await;
        retain_test_generation(&pool, system.id, 40, &commits[10], snapshots[10]).await;
        retain_test_generation(&pool, system.id, 41, &commits[6], snapshots[6]).await;
        retain_test_generation(&pool, system.id, 42, &commits[7], snapshots[7]).await;

        let valid_commit = select_commit_snapshot(&pool, system.id, &commits[1].git_commit_hash)
            .await
            .expect("valid commit selection should succeed")
            .expect("valid commit snapshot should exist");
        let valid_page = query_options_page(
            &pool,
            system.id,
            &valid_commit,
            None,
            "",
            EvaluatedOptionFilter::Changed,
            100,
            0,
        )
        .await
        .expect("valid commit comparison should load");
        assert!(valid_page.comparison_available);
        assert_eq!(valid_page.counts.changed, Some(1));
        assert!(valid_page.options[0].before.is_some());
        assert!(valid_page.options[0].diff.is_some());

        let valid_generation = select_generation_snapshot(&pool, system.id, 11)
            .await
            .expect("valid generation selection should succeed")
            .expect("valid generation snapshot should exist");
        assert_eq!(valid_generation.baseline_id, Some(snapshots[0]));

        // Capture selections before corruption to exercise the page-query
        // defense as well as normal post-corruption selection.
        let stale_commit_selections =
            futures::future::try_join_all([3_usize, 5, 7].into_iter().map(|index| {
                select_commit_snapshot(&pool, system.id, &commits[index].git_commit_hash)
            }))
            .await
            .expect("commit selections should succeed")
            .into_iter()
            .map(|selected| selected.expect("commit snapshot should exist"))
            .collect::<Vec<_>>();
        let stale_generation_selections = futures::future::try_join_all(
            [22, 32, 42]
                .into_iter()
                .map(|generation| select_generation_snapshot(&pool, system.id, generation)),
        )
        .await
        .expect("generation selections should succeed")
        .into_iter()
        .map(|selected| selected.expect("generation snapshot should exist"))
        .collect::<Vec<_>>();

        sqlx::query(
            r#"
            UPDATE evaluation_option_contents content
            SET payload = jsonb_set(content.payload, '{value}', '{"kind":"unknown"}'::jsonb)
            FROM evaluation_snapshot_options item
            WHERE item.snapshot_id = $1 AND item.content_digest = content.digest
            "#,
        )
        .bind(snapshots[2])
        .execute(&pool)
        .await
        .expect("undecodable payload corruption should persist");
        sqlx::query(
            "ALTER TABLE evaluation_snapshots DROP CONSTRAINT evaluation_snapshots_schema_version_check",
        )
        .execute(&pool)
        .await
        .expect("isolated test should permit an incompatible schema fixture");
        sqlx::query("UPDATE evaluation_snapshots SET schema_version = 2 WHERE id = $1")
            .bind(snapshots[4])
            .execute(&pool)
            .await
            .expect("incompatible schema should persist");
        sqlx::query(
            r#"
            UPDATE evaluation_option_contents content
            SET payload = content.payload - 'definitions'
            FROM evaluation_snapshot_options item
            WHERE item.snapshot_id = $1 AND item.content_digest = content.digest
            "#,
        )
        .bind(snapshots[6])
        .execute(&pool)
        .await
        .expect("missing required payload content should persist");

        for selected in stale_commit_selections
            .iter()
            .chain(&stale_generation_selections)
        {
            assert_comparison_isolated(&pool, selected).await;
        }

        for index in [3_usize, 5, 7] {
            let selected =
                select_commit_snapshot(&pool, system.id, &commits[index].git_commit_hash)
                    .await
                    .expect("commit selection should succeed")
                    .expect("selected commit should remain readable");
            assert!(selected.baseline_id.is_none());
            assert_comparison_isolated(&pool, &selected).await;
        }
        for (generation, expected_baseline) in
            [(22, snapshots[8]), (32, snapshots[9]), (42, snapshots[10])]
        {
            let selected = select_generation_snapshot(&pool, system.id, generation)
                .await
                .expect("generation selection should succeed")
                .expect("selected generation should remain readable");
            assert_eq!(selected.baseline_id, Some(expected_baseline));
            let page = query_options_page(
                &pool,
                system.id,
                &selected,
                None,
                "",
                EvaluatedOptionFilter::Changed,
                100,
                0,
            )
            .await
            .expect("generation fallback comparison should load");
            assert!(page.comparison_available);
            assert!(page.options[0].before.is_some());
            assert!(page.options[0].diff.is_some());
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn bounded_comparison_page_handles_multi_thousand_option_snapshots(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/bounded-options-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("bounded-options-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let system = insert_test_system(&pool, flake.id, &suffix).await;
        let baseline_commit = insert_test_commit(&pool, &repo_url, &"1".repeat(40)).await;
        let selected_commit = insert_test_commit(&pool, &repo_url, &"2".repeat(40)).await;
        set_commit_first_parent_by_repo_url(
            &pool,
            &repo_url,
            &selected_commit.git_commit_hash,
            Some(&baseline_commit.git_commit_hash),
        )
        .await
        .expect("first parent should persist");
        let baseline_id = persist_test_snapshot(&pool, &baseline_commit, "before").await;
        let selected_id = persist_test_snapshot(&pool, &selected_commit, "after").await;

        for snapshot_id in [baseline_id, selected_id] {
            sqlx::query(
                r#"
                INSERT INTO evaluation_snapshot_options (
                    snapshot_id, option_path, content_digest, is_overridden
                )
                SELECT $1,
                       'services.bulk.option' || lpad(value::text, 4, '0'),
                       seed.content_digest, false
                FROM generate_series(1, 5000) value
                CROSS JOIN LATERAL (
                    SELECT content_digest
                    FROM evaluation_snapshot_options
                    WHERE snapshot_id = $1
                    LIMIT 1
                ) seed
                "#,
            )
            .bind(snapshot_id)
            .execute(&pool)
            .await
            .expect("bulk option references should persist");
            sqlx::query("UPDATE evaluation_snapshots SET option_count = 5001 WHERE id = $1")
                .bind(snapshot_id)
                .execute(&pool)
                .await
                .expect("bulk option count should persist");
        }

        let selected = select_commit_snapshot(&pool, system.id, &selected_commit.git_commit_hash)
            .await
            .expect("large snapshot selection should succeed")
            .expect("large snapshot should exist");
        let page = query_options_page(
            &pool,
            system.id,
            &selected,
            None,
            "",
            EvaluatedOptionFilter::Changed,
            25,
            4_975,
        )
        .await
        .expect("bounded large-snapshot page should load");
        assert!(page.comparison_available);
        assert_eq!(page.counts.all, 5_001);
        assert_eq!(page.counts.changed, Some(5_001));
        assert_eq!(page.total, 5_001);
        assert_eq!(page.limit, 25);
        assert_eq!(page.options.len(), 25);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn evaluation_module_sources_page_is_complete_stable_bounded_and_read_only(pool: PgPool) {
        const SOURCE_TOTAL: i32 = 2_050;
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/module-sources-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("module-sources-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let system = insert_test_system(&pool, flake.id, &suffix).await;
        let commit = insert_test_commit(&pool, &repo_url, &"7".repeat(40)).await;
        let mut tx = pool
            .begin()
            .await
            .expect("snapshot transaction should begin");
        let snapshot_id = persist_available_snapshot_tx(&mut tx, commit.id, "host", Vec::new())
            .await
            .expect("empty snapshot envelope should persist");
        tx.commit().await.expect("snapshot should commit");

        sqlx::query(
            r#"
            WITH generated AS (
                SELECT value,
                       decode(md5('module-source-' || value::text) ||
                              md5('module-payload-' || value::text), 'hex') AS digest,
                       format('input-%04s', value) AS source_input,
                       lpad(to_hex(value), 40, '0') AS source_revision,
                       format('modules/%04s.nix', value) AS source_path,
                       value % 3 <> 0 AS winning
                FROM generate_series(1, $2) value
            ), inserted_contents AS (
                INSERT INTO evaluation_option_contents (digest, payload, search_text)
                SELECT digest,
                       jsonb_build_object(
                           'declared_type', 'boolean',
                           'value', jsonb_build_object('kind', 'scalar', 'value', true),
                           'definitions', jsonb_build_array(jsonb_build_object(
                               'source_path', source_path,
                               'source_input', source_input,
                               'source_revision', source_revision,
                               'value', NULL,
                               'winning', winning,
                               'priority', 100,
                               'status', CASE WHEN winning THEN 'winning' ELSE 'overridden' END,
                               'winner_note', NULL
                           )),
                           'overridden', NOT winning
                       ),
                       source_input || ' ' || source_path
                FROM generated
                RETURNING digest
            )
            INSERT INTO evaluation_snapshot_options (
                snapshot_id, option_path, content_digest, is_overridden
            )
            SELECT $1, format('services.generated.option%04s', generated.value),
                   generated.digest, NOT generated.winning
            FROM generated
            JOIN inserted_contents USING (digest)
            "#,
        )
        .bind(snapshot_id)
        .bind(SOURCE_TOTAL)
        .execute(&pool)
        .await
        .expect("large distinct source corpus should persist");
        sqlx::query(
            "UPDATE evaluation_snapshots SET option_count = $2, module_count = $2 WHERE id = $1",
        )
        .bind(snapshot_id)
        .bind(SOURCE_TOTAL)
        .execute(&pool)
        .await
        .expect("authoritative snapshot counts should persist");

        let selected = select_commit_snapshot(&pool, system.id, &commit.git_commit_hash)
            .await
            .expect("large source snapshot selection should succeed")
            .expect("large source snapshot should exist");
        let before: (String, i32, i32) = sqlx::query_as(
            "SELECT c.evaluation_status, s.option_count, s.module_count FROM commits c JOIN evaluation_snapshots s ON s.commit_id = c.id WHERE s.id = $1",
        )
        .bind(snapshot_id)
        .fetch_one(&pool)
        .await
        .expect("read-only state should load");

        let first = module_sources_page(
            get_evaluation_module_sources_page(&pool, system.id, &selected, None, None, 500, 0)
                .await
                .expect("first source page should load"),
        );
        assert_eq!(first.limit, 100);
        assert_eq!(first.total, i64::from(SOURCE_TOTAL));
        assert_eq!(first.sources.len(), 100);

        let mut all_sources = Vec::new();
        for offset in (0..SOURCE_TOTAL).step_by(100) {
            let page = module_sources_page(
                get_evaluation_module_sources_page(
                    &pool,
                    system.id,
                    &selected,
                    None,
                    first.snapshot_token.as_deref(),
                    100,
                    i64::from(offset),
                )
                .await
                .expect("source page should load"),
            );
            assert_eq!(page.total, i64::from(SOURCE_TOTAL));
            all_sources.extend(page.sources);
        }
        assert_eq!(all_sources.len(), SOURCE_TOTAL as usize);
        assert_eq!(
            all_sources
                .iter()
                .map(|source| (
                    source.source_input.as_deref(),
                    source.source_revision.as_deref(),
                    source.source_path.as_str()
                ))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            SOURCE_TOTAL as usize,
            "stable pages must not duplicate or skip source tuples"
        );
        assert_eq!(all_sources.last().map(|source| source.won_count), Some(0));
        let final_page = module_sources_page(
            get_evaluation_module_sources_page(
                &pool,
                system.id,
                &selected,
                None,
                first.snapshot_token.as_deref(),
                100,
                2_000,
            )
            .await
            .expect("final source page should load"),
        );
        assert_eq!(final_page.sources.len(), 50);
        let out_of_range = module_sources_page(
            get_evaluation_module_sources_page(
                &pool,
                system.id,
                &selected,
                None,
                first.snapshot_token.as_deref(),
                100,
                99_999,
            )
            .await
            .expect("out-of-range source page should load"),
        );
        assert_eq!(out_of_range.total, i64::from(SOURCE_TOTAL));
        assert!(out_of_range.sources.is_empty());

        for pair in all_sources.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            assert!(
                (left.won_count, left.defined_count) > (right.won_count, right.defined_count)
                    || ((left.won_count, left.defined_count)
                        == (right.won_count, right.defined_count)
                        && (
                            left.source_input.as_deref(),
                            left.source_revision.as_deref(),
                            left.source_path.as_str()
                        ) <= (
                            right.source_input.as_deref(),
                            right.source_revision.as_deref(),
                            right.source_path.as_str()
                        )),
                "source pages must preserve the documented deterministic order"
            );
        }

        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .execute(&pool)
            .await
            .expect("pg_stat_statements should be available");
        sqlx::query("SELECT pg_stat_statements_reset()")
            .execute(&pool)
            .await
            .expect("query statistics should reset");
        get_evaluation_module_sources_page(&pool, system.id, &selected, None, None, 100, 0)
            .await
            .expect("measured source page should load");
        let source_query_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(calls), 0)::bigint
            FROM pg_stat_statements
            WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
              AND query NOT ILIKE '%pg_stat_statements%'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("source query count should load");
        assert_eq!(
            source_query_count, 5,
            "source page query count must remain fixed with snapshot consistency"
        );

        sqlx::query("SELECT pg_stat_statements_reset()")
            .execute(&pool)
            .await
            .expect("query statistics should reset");
        query_options_page(
            &pool,
            system.id,
            &selected,
            None,
            "",
            EvaluatedOptionFilter::All,
            100,
            0,
        )
        .await
        .expect("measured options page should load");
        let option_query_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(calls), 0)::bigint
            FROM pg_stat_statements
            WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
              AND query NOT ILIKE '%pg_stat_statements%'
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("option query count should load");
        assert_eq!(
            option_query_count, 4,
            "option page query count must be fixed"
        );

        let after: (String, i32, i32) = sqlx::query_as(
            "SELECT c.evaluation_status, s.option_count, s.module_count FROM commits c JOIN evaluation_snapshots s ON s.commit_id = c.id WHERE s.id = $1",
        )
        .bind(snapshot_id)
        .fetch_one(&pool)
        .await
        .expect("read-only state should reload");
        assert_eq!(
            after, before,
            "source reads must not mutate evaluation state"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn evaluation_module_source_count_and_continuation_follow_bounded_replacements(
        pool: PgPool,
    ) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/module-replacement-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("module-replacement-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let system = insert_test_system(&pool, flake.id, &suffix).await;
        let revision = "6".repeat(40);
        let commit = insert_test_commit(&pool, &repo_url, &revision).await;

        let mut oversized = option("services.replacement.oversized", Value::Null);
        oversized.value = SafeOptionValue::List(vec![SafeOptionValue::Scalar(json!(true)); 20_000]);
        oversized.definitions[0].source_revision = Some(revision.clone());
        let mut tx = pool
            .begin()
            .await
            .expect("snapshot transaction should begin");
        let snapshot_id =
            persist_available_snapshot_tx(&mut tx, commit.id, "host", vec![oversized])
                .await
                .expect("oversized snapshot should persist as an opaque option");
        tx.commit().await.expect("snapshot should commit");

        let persisted_count: i32 =
            sqlx::query_scalar("SELECT module_count FROM evaluation_snapshots WHERE id = $1")
                .bind(snapshot_id)
                .fetch_one(&pool)
                .await
                .expect("module count should load");
        assert_eq!(
            persisted_count, 0,
            "definitions cleared by payload bounding must not contribute to module_count"
        );
        let selected = select_commit_snapshot(&pool, system.id, &revision)
            .await
            .expect("snapshot selection should succeed")
            .expect("snapshot should exist");
        let first = module_sources_page(
            get_evaluation_module_sources_page(&pool, system.id, &selected, None, None, 100, 0)
                .await
                .expect("first page should load"),
        );
        assert_eq!(first.total, 0);
        let old_token = first
            .snapshot_token
            .expect("available first page should return a snapshot token");

        let mut replacement = option("services.replacement.enable", json!(true));
        replacement.definitions[0].source_path = "modules/replacement.nix".to_string();
        replacement.definitions[0].source_revision = Some(revision.clone());
        let mut tx = pool
            .begin()
            .await
            .expect("replacement transaction should begin");
        let replacement_id =
            persist_available_snapshot_tx(&mut tx, commit.id, "host", vec![replacement])
                .await
                .expect("replacement snapshot should persist");
        tx.commit().await.expect("replacement should commit");
        assert_eq!(replacement_id, snapshot_id);

        let stale = get_evaluation_module_sources_page(
            &pool,
            system.id,
            &selected,
            None,
            Some(&old_token),
            100,
            1,
        )
        .await
        .expect("stale continuation should produce a typed result");
        assert_eq!(stale, EvaluationModuleSourcesQuery::SnapshotChanged);

        let replacement_selected = select_commit_snapshot(&pool, system.id, &revision)
            .await
            .expect("replacement selection should succeed")
            .expect("replacement snapshot should exist");
        assert_eq!(replacement_selected.module_count, 1);
        let replacement_page = module_sources_page(
            get_evaluation_module_sources_page(
                &pool,
                system.id,
                &replacement_selected,
                None,
                None,
                100,
                0,
            )
            .await
            .expect("replacement first page should load"),
        );
        assert_ne!(
            replacement_page.snapshot_token.as_deref(),
            Some(old_token.as_str())
        );
        assert_eq!(replacement_page.total, 1);
        assert_eq!(replacement_page.sources.len(), 1);
        assert_eq!(
            replacement_page.sources[0].source_path,
            "modules/replacement.nix"
        );
        assert!(replacement_page.sources[0].tracked_flake.is_some());
    }

    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    #[sqlx::test(migrations = "./migrations")]
    async fn retained_generation_survives_store_metadata_loss_and_blocks_commit_deletion(
        pool: PgPool,
    ) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/retention-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("flake-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let hash = "c".repeat(40);
        let commit = insert_test_commit(&pool, &repo_url, &hash).await;
        let hostname = format!("host-{suffix}");
        let key = SigningKey::from_bytes(&[43; 32]);
        let system = System {
            id: Uuid::new_v4(),
            hostname: hostname.clone(),
            environment_id: None,
            is_active: true,
            public_key: PublicKey::from_verifying_key(key.verifying_key()),
            flake_id: Some(flake.id),
            derivation: String::new(),
            system_configuration_name: Some("host".into()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".into(),
        };
        let system = insert_system(&pool, &system)
            .await
            .expect("system insert should succeed");
        let derivation = insert_derivation(&pool, Some(&commit), "host", "nixos")
            .await
            .expect("derivation insert should succeed");
        let store_path = format!("/nix/store/{suffix}-system");
        sqlx::query(
            "UPDATE derivations SET store_path = $1, expected_store_path = $1 WHERE id = $2",
        )
        .bind(&store_path)
        .bind(derivation.id)
        .execute(&pool)
        .await
        .expect("store path should persist");
        let same_closure_commit = insert_test_commit(&pool, &repo_url, &"d".repeat(40)).await;
        let same_closure_derivation =
            insert_derivation(&pool, Some(&same_closure_commit), "host", "nixos")
                .await
                .expect("same-closure derivation should insert");
        sqlx::query(
            "UPDATE derivations SET store_path = $1, expected_store_path = $1 WHERE id = $2",
        )
        .bind(&store_path)
        .bind(same_closure_derivation.id)
        .execute(&pool)
        .await
        .expect("same closure should persist for a distinct commit");
        sqlx::query(
            "UPDATE derivations
             SET cf_agent_enabled = true, policy_requirements_met = true
             WHERE id = ANY($1)",
        )
        .bind(vec![derivation.id, same_closure_derivation.id])
        .execute(&pool)
        .await
        .expect("deployment eligibility should persist");
        sqlx::query(
            "INSERT INTO cache_push_jobs
             (derivation_id, status, completed_at, cache_destination)
             VALUES ($1, 'completed', NOW(), 'task440-test'),
                    ($2, 'completed', NOW(), 'task440-test')",
        )
        .bind(derivation.id)
        .bind(same_closure_derivation.id)
        .execute(&pool)
        .await
        .expect("completed cache push should persist");
        let mut same_closure_tx = pool.begin().await.expect("transaction should begin");
        persist_available_snapshot_tx(
            &mut same_closure_tx,
            same_closure_commit.id,
            "host",
            vec![option("system.stateVersion", json!("different-commit"))],
        )
        .await
        .expect("same-closure snapshot should persist");
        same_closure_tx
            .commit()
            .await
            .expect("same-closure transaction should commit");
        sqlx::query("UPDATE systems SET deployment_policy = 'auto_latest' WHERE id = $1")
            .bind(system.id)
            .execute(&pool)
            .await
            .expect("auto_latest policy should persist");
        let request_id = Uuid::new_v4();
        let request_identity = format!("explicit:{request_id}");
        crate::queries::systems::reserve_explicit_deployment_request(
            &pool,
            system.id,
            request_id,
            &hash,
            "convert_to_manual",
        )
        .await
        .expect("explicit UI request should reserve before conversion");
        let conversion = crate::queries::systems::convert_auto_latest_system_to_manual_for_request(
            &pool,
            system.id,
            Some(request_id),
        )
        .await
        .expect("conversion should commit independently");
        let first_deployment = crate::queries::systems::queue_manual_deployment_atomic(
            &pool,
            system.id,
            &hash,
            "test",
            &request_identity,
            "convert_to_manual",
            "manual",
        )
        .await
        .expect("deployment queue should commit");
        assert_eq!(
            conversion,
            crate::queries::systems::ManualPolicyConversion::Converted
        );
        let commit_conflict = crate::queries::systems::queue_manual_deployment_atomic(
            &pool,
            system.id,
            &same_closure_commit.git_commit_hash,
            "test",
            &request_identity,
            "convert_to_manual",
            "manual",
        )
        .await
        .expect_err("one request identity must not select another commit");
        assert!(
            commit_conflict
                .downcast_ref::<crate::queries::systems::DeploymentRequestIdentityConflict>()
                .is_some()
        );
        let action_conflict = crate::queries::systems::queue_manual_deployment_atomic(
            &pool,
            system.id,
            &hash,
            "test",
            &request_identity,
            "deploy",
            "manual",
        )
        .await
        .expect_err("one request identity must not select another action");
        assert!(
            action_conflict
                .downcast_ref::<crate::queries::systems::DeploymentRequestIdentityConflict>()
                .is_some()
        );

        sqlx::query(
            "INSERT INTO system_states (hostname, change_reason, store_path, generation, timestamp)
             VALUES ($1, 'state_delta', $2, 8, NOW())",
        )
        .bind(&hostname)
        .bind(&store_path)
        .execute(&pool)
        .await
        .expect("pre-snapshot generation observation should persist");

        sqlx::query("UPDATE pending_system_deployments SET status = 'succeeded' WHERE id = $1")
            .bind(first_deployment.deployment_id)
            .execute(&pool)
            .await
            .expect("first deployment should become authoritative history");

        let overwritten = sqlx::query(
            "UPDATE pending_system_deployments SET requested_commit_id = $2 WHERE id = $1",
        )
        .bind(first_deployment.deployment_id)
        .bind(same_closure_commit.id)
        .execute(&pool)
        .await;
        assert!(
            overwritten.is_err(),
            "persisted deployment commit identity must be immutable"
        );
        let second_identity = format!("task440-second-{suffix}");
        let second_deployment = crate::queries::systems::queue_manual_deployment_atomic(
            &pool,
            system.id,
            &same_closure_commit.git_commit_hash,
            "test",
            &second_identity,
            "deploy",
            "manual",
        )
        .await
        .expect("a distinct commit with the same store path must queue independently");
        assert_ne!(
            first_deployment.deployment_id, second_deployment.deployment_id,
            "store-path equality must not collapse distinct commit requests"
        );

        let mut tx = pool.begin().await.expect("transaction should begin");
        persist_available_snapshot_tx(
            &mut tx,
            commit.id,
            "host",
            vec![option("system.stateVersion", json!("26.11"))],
        )
        .await
        .expect("snapshot should persist");
        persist_flake_output_snapshot_tx(
            &mut tx,
            commit.id,
            &json!({
                "declared_systems": ["host"],
                "exported_modules": [],
                "inputs": []
            }),
        )
        .await
        .expect("flake output snapshot should persist");
        tx.commit().await.expect("transaction should commit");

        assert!(
            select_generation_snapshot(&pool, system.id, 8)
                .await
                .expect("first reciprocal lookup should succeed")
                .is_none(),
            "an older request must not win reciprocal backfill"
        );
        let mut reciprocal_tx = pool
            .begin()
            .await
            .expect("reciprocal transaction should begin");
        persist_available_snapshot_tx(
            &mut reciprocal_tx,
            same_closure_commit.id,
            "host",
            vec![option("system.stateVersion", json!("different-commit"))],
        )
        .await
        .expect("authoritative snapshot should be repersisted");
        reciprocal_tx
            .commit()
            .await
            .expect("reciprocal transaction should commit");
        let reciprocally_retained = select_generation_snapshot(&pool, system.id, 8)
            .await
            .expect("reciprocal generation query should succeed")
            .expect("snapshot finalization should retain the observed generation");
        assert_eq!(
            reciprocally_retained.revision, same_closure_commit.git_commit_hash,
            "the latest authoritative request must own same-path reciprocal backfill"
        );
        sqlx::query("UPDATE pending_system_deployments SET status = 'failed' WHERE id = $1")
            .bind(second_deployment.deployment_id)
            .execute(&pool)
            .await
            .expect("second deployment should become terminal");
        let mut retain_tx = pool
            .begin()
            .await
            .expect("retention transaction should begin");
        assert!(
            retain_generation_snapshot_tx(&mut retain_tx, &hostname, Some(7), Some(&store_path))
                .await
                .expect("generation retention should succeed")
        );
        retain_tx.commit().await.expect("retention should commit");

        sqlx::query(
            "UPDATE derivations SET store_path = NULL, expected_store_path = NULL WHERE id = $1",
        )
        .bind(derivation.id)
        .execute(&pool)
        .await
        .expect("store metadata should clear");
        let selected = select_generation_snapshot(&pool, system.id, 7)
            .await
            .expect("generation query should succeed")
            .expect("retained snapshot should remain");
        assert_eq!(selected.revision, hash);
        assert_ne!(selected.revision, same_closure_commit.git_commit_hash);
        sqlx::query("UPDATE pending_system_deployments SET status = 'failed' WHERE id = $1")
            .bind(first_deployment.deployment_id)
            .execute(&pool)
            .await
            .expect("deployment should become terminal");
        let retry = crate::queries::systems::queue_manual_deployment_atomic(
            &pool,
            system.id,
            &hash,
            "test",
            &request_identity,
            "convert_to_manual",
            "manual",
        )
        .await
        .expect("terminal retry should reuse durable request identity");
        assert_eq!(retry.deployment_id, first_deployment.deployment_id);
        assert!(!retry.created);
        let output =
            get_flake_output_snapshot(&pool, flake.id, &hash, None, FlakeSystemFilter::All, 50, 0)
                .await
                .expect("flake output query should succeed")
                .expect("flake output should exist");
        let registry_count = count_systems_for_flake(&pool, flake.id)
            .await
            .expect("registry count should load");
        assert_eq!(output.managed_system_count, registry_count);
        assert_eq!(output.managed_system_count, 1);
        let visible_environment = Uuid::new_v4();
        let hidden_environment = Uuid::new_v4();
        let viewer = Uuid::new_v4();
        sqlx::query("INSERT INTO environments (id, name) VALUES ($1, $2), ($3, $4)")
            .bind(visible_environment)
            .bind(format!("visible-{suffix}"))
            .bind(hidden_environment)
            .bind(format!("hidden-{suffix}"))
            .execute(&pool)
            .await
            .expect("environments should insert");
        sqlx::query("UPDATE systems SET environment_id = $1 WHERE id = $2")
            .bind(visible_environment)
            .bind(system.id)
            .execute(&pool)
            .await
            .expect("visible environment should persist");
        sqlx::query(
            "INSERT INTO users
             (id, username, first_name, last_name, email, user_type)
             VALUES ($1, $2, 'Test', 'Viewer', $3, 'human')",
        )
        .bind(viewer)
        .bind(format!("viewer-{suffix}"))
        .bind(format!("viewer-{suffix}@example.test"))
        .execute(&pool)
        .await
        .expect("viewer should insert");
        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id) VALUES ($1, $2)",
        )
        .bind(viewer)
        .bind(visible_environment)
        .execute(&pool)
        .await
        .expect("viewer membership should insert");
        let hidden_key = SigningKey::from_bytes(&[44; 32]);
        insert_system(
            &pool,
            &System {
                id: Uuid::new_v4(),
                hostname: format!("hidden-{hostname}"),
                environment_id: Some(hidden_environment),
                is_active: true,
                public_key: PublicKey::from_verifying_key(hidden_key.verifying_key()),
                flake_id: Some(flake.id),
                derivation: String::new(),
                system_configuration_name: Some("host".into()),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                desired_target: None,
                deployment_policy: "manual".into(),
            },
        )
        .await
        .expect("hidden system should insert");
        let scoped_output = get_flake_output_snapshot(
            &pool,
            flake.id,
            &hash,
            Some(viewer),
            FlakeSystemFilter::All,
            1,
            0,
        )
        .await
        .expect("scoped output should load")
        .expect("scoped output should exist");
        assert_eq!(scoped_output.managed_system_count, 1);
        assert_eq!(scoped_output.systems.len(), 1);
        assert!(!scoped_output.systems[0].output_collapsed);
        let admin_output =
            get_flake_output_snapshot(&pool, flake.id, &hash, None, FlakeSystemFilter::All, 1, 0)
                .await
                .expect("admin output should load")
                .expect("admin output should exist");
        assert_eq!(admin_output.managed_system_count, 2);
        assert_eq!(
            admin_output.systems.len(),
            1,
            "visible systems are paginated"
        );
        assert!(admin_output.systems[0].output_collapsed);
        assert_eq!(
            admin_output.managed_system_count,
            count_systems_for_flake(&pool, flake.id)
                .await
                .expect("registry count should remain consistent")
        );
        let snapshot_only_hash = "e".repeat(40);
        let snapshot_only_commit = insert_test_commit(&pool, &repo_url, &snapshot_only_hash).await;
        let mut snapshot_only_tx = pool.begin().await.expect("transaction should begin");
        persist_available_snapshot_tx(
            &mut snapshot_only_tx,
            snapshot_only_commit.id,
            "host",
            vec![option("services.example.enable", json!(true))],
        )
        .await
        .expect("unretained snapshot should persist");
        snapshot_only_tx
            .commit()
            .await
            .expect("snapshot-only transaction should commit");
        let disposable_commit = insert_test_commit(&pool, &repo_url, &"f".repeat(40)).await;

        accept_history_rewrite_reset(&pool, flake.id)
            .await
            .expect("rewrite acceptance must preserve retained derivations");
        let retained_derivation_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM derivations WHERE id = $1)")
                .bind(derivation.id)
                .fetch_one(&pool)
                .await
                .expect("retained derivation existence should load");
        assert!(retained_derivation_exists);
        assert!(
            select_generation_snapshot(&pool, system.id, 7)
                .await
                .expect("generation should survive accepted rewrite")
                .is_some()
        );
        for old_lineage_commit_id in [commit.id, same_closure_commit.id, snapshot_only_commit.id] {
            let archived: bool =
                sqlx::query_scalar("SELECT source_archived FROM commits WHERE id = $1")
                    .bind(old_lineage_commit_id)
                    .fetch_one(&pool)
                    .await
                    .expect("preserved old-lineage commit should remain queryable by ID");
            assert!(
                archived,
                "every preserved old-lineage commit must be archived"
            );
        }
        assert!(
            select_commit_snapshot(&pool, system.id, &snapshot_only_hash)
                .await
                .expect("active snapshot lookup should succeed")
                .is_none(),
            "an unretained old-lineage snapshot must not remain an active revision"
        );
        assert!(
            get_flake_output_snapshot(&pool, flake.id, &hash, None, FlakeSystemFilter::All, 50, 0)
                .await
                .expect("active flake lookup should succeed")
                .is_none(),
            "an archived retained commit must not remain an active flake revision"
        );
        let disposable_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM commits WHERE id = $1)")
                .bind(disposable_commit.id)
                .fetch_one(&pool)
                .await
                .expect("disposable commit existence should load");
        assert!(
            !disposable_exists,
            "unreferenced old-lineage commits should be deleted"
        );

        let deletion = sqlx::query("DELETE FROM commits WHERE id = $1")
            .bind(commit.id)
            .execute(&pool)
            .await;
        assert!(
            deletion.is_err(),
            "retained generation must prevent snapshot cascade deletion"
        );
        let mut reset_tx = pool.begin().await.expect("reset transaction should begin");
        reset_flake_source(
            &mut reset_tx,
            flake.id,
            &flake.name,
            &format!("https://example.test/reset-{suffix}.git"),
            "release",
            &flake.build_scope,
        )
        .await
        .expect("source reset must remain operational with retained history");
        reset_tx.commit().await.expect("source reset should commit");
        let retained_commit_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM commits WHERE id = $1)")
                .bind(commit.id)
                .fetch_one(&pool)
                .await
                .expect("retained commit existence should load");
        let second_retained_commit_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM commits WHERE id = $1)")
                .bind(same_closure_commit.id)
                .fetch_one(&pool)
                .await
                .expect("unretained commit existence should load");
        assert!(retained_commit_exists);
        assert!(second_retained_commit_exists);
        let archived: bool =
            sqlx::query_scalar("SELECT source_archived FROM commits WHERE id = $1")
                .bind(commit.id)
                .fetch_one(&pool)
                .await
                .expect("archival lineage should load");
        assert!(archived);
        assert!(
            select_generation_snapshot(&pool, system.id, 7)
                .await
                .expect("generation history should remain queryable")
                .is_some()
        );
        assert!(
            select_commit_snapshot(&pool, system.id, &hash)
                .await
                .expect("active revision lookup should succeed")
                .is_none(),
            "an archived source SHA must not be authorized as an active revision"
        );
        assert!(
            get_flake_output_snapshot(&pool, flake.id, &hash, None, FlakeSystemFilter::All, 50, 0)
                .await
                .expect("active flake lookup should succeed")
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn selected_evaluation_summary_is_authoritative_and_visibility_filtered(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let visible_environment = create_environment(
            &pool,
            &format!("summary-visible-{suffix}"),
            None,
            "#112233",
            true,
            "manual",
            false,
            false,
            false,
        )
        .await
        .expect("visible environment should insert");
        let hidden_environment = create_environment(
            &pool,
            &format!("summary-hidden-{suffix}"),
            None,
            "#334455",
            true,
            "manual",
            false,
            false,
            false,
        )
        .await
        .expect("hidden environment should insert");
        let viewer = insert_user(
            &pool,
            &format!("summary-{suffix}@example.test"),
            Some("Summary Viewer"),
        )
        .await
        .expect("viewer should insert");
        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id) VALUES ($1, $2)",
        )
        .bind(viewer.id)
        .bind(visible_environment.id)
        .execute(&pool)
        .await
        .expect("visible membership should persist");

        let selected_revision = "a".repeat(40);
        let source_url = format!(
            "https://build-user:super-secret@example.test/source-{suffix}.git?token=leaked"
        );
        let source_flake = insert_flake(
            &pool,
            &format!("source-{suffix}"),
            &source_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("source flake should insert");
        let selected_commit = insert_test_commit(&pool, &source_url, &selected_revision).await;
        let source_system =
            insert_test_system(&pool, source_flake.id, &format!("source-{suffix}")).await;
        sqlx::query("UPDATE systems SET environment_id = $2 WHERE id = $1")
            .bind(source_system.id)
            .bind(visible_environment.id)
            .execute(&pool)
            .await
            .expect("source visibility should persist");

        let identities = [
            ("visible", "b".repeat(40), false, false, true),
            ("hidden", "c".repeat(40), false, false, false),
            ("deleted", "d".repeat(40), true, false, true),
            ("stale", "e".repeat(40), false, true, true),
            ("ambiguous-one", "9".repeat(40), false, false, true),
            ("ambiguous-two", "9".repeat(40), false, false, true),
        ];
        let mut input_rows = Vec::new();
        let mut provenance = Vec::new();
        for (name, revision, deleted, archived, visible) in identities {
            let repo_url = format!("https://{name}.summary.test/repo.git");
            let flake = insert_flake(
                &pool,
                &format!("{name}-{suffix}"),
                &repo_url,
                "main",
                "cf_systems_only",
            )
            .await
            .expect("external flake should insert");
            insert_commit_with_metadata(
                &pool,
                &revision,
                &repo_url,
                Utc::now(),
                Some("external"),
                Some("test"),
            )
            .await
            .expect("external commit should insert");
            if archived {
                sqlx::query(
                    "UPDATE commits SET source_archived = true WHERE flake_id = $1 AND git_commit_hash = $2",
                )
                .bind(flake.id)
                .bind(&revision)
                .execute(&pool)
                .await
                .expect("stale commit should archive");
            }
            let external_system =
                insert_test_system(&pool, flake.id, &format!("{name}-{suffix}")).await;
            sqlx::query("UPDATE systems SET environment_id = $2 WHERE id = $1")
                .bind(external_system.id)
                .bind(if visible {
                    visible_environment.id
                } else {
                    hidden_environment.id
                })
                .execute(&pool)
                .await
                .expect("external system visibility should persist");
            if deleted {
                sqlx::query("UPDATE flakes SET deleted_at = now() WHERE id = $1")
                    .bind(flake.id)
                    .execute(&pool)
                    .await
                    .expect("deleted flake fixture should persist");
            }
            let input_name = if name.starts_with("ambiguous") {
                "ambiguous"
            } else {
                name
            };
            input_rows.push(json!({
                "node": name,
                "names": [input_name],
                "source": repo_url,
                "locked_revision": revision
            }));
            if name != "ambiguous-two" {
                provenance.push((input_name.to_string(), revision));
            }
        }
        provenance.push(("untracked".into(), "f".repeat(40)));

        let mut options = vec![
            option("services.summary.selfOne", json!(true)),
            option("services.summary.selfTwo", json!(false)),
        ];
        for (index, option) in options.iter_mut().enumerate() {
            option.definitions[0].source_path = "modules/self.nix".into();
            option.definitions[0].source_revision = Some(selected_revision.clone());
            option.definitions[0].winning = index == 0;
            option.overridden = index != 0;
        }
        let mut mismatched_self = option("services.summary.mismatchedSelf", json!(true));
        mismatched_self.definitions[0].source_path = "modules/mismatched-self.nix".into();
        mismatched_self.definitions[0].source_revision = Some("0".repeat(40));
        options.push(mismatched_self);
        for (index, (input, revision)) in provenance.into_iter().enumerate() {
            let mut value = option(&format!("services.summary.external{index}"), json!(index));
            value.definitions[0].source_path = format!("modules/{input}.nix");
            value.definitions[0].source_input = Some(input);
            value.definitions[0].source_revision = Some(revision);
            options.push(value);
        }
        let mut tx = pool
            .begin()
            .await
            .expect("summary transaction should begin");
        let snapshot_id =
            persist_available_snapshot_tx(&mut tx, selected_commit.id, "host", options)
                .await
                .expect("summary snapshot should persist");
        persist_flake_output_snapshot_tx(
            &mut tx,
            selected_commit.id,
            &json!({
                "declared_systems": ["host"],
                "exported_modules": [],
                "inputs": input_rows
            }),
        )
        .await
        .expect("source lock snapshot should persist");
        tx.commit()
            .await
            .expect("summary transaction should commit");

        let baseline_revision = "8".repeat(40);
        let baseline_commit = insert_test_commit(&pool, &source_url, &baseline_revision).await;
        let mut baseline_option = option("services.summary.selfOne", json!(false));
        baseline_option.definitions[0].source_path = "modules/self.nix".into();
        baseline_option.definitions[0].source_revision = Some(baseline_revision.clone());
        let mut baseline_tx = pool
            .begin()
            .await
            .expect("baseline transaction should begin");
        persist_available_snapshot_tx(
            &mut baseline_tx,
            baseline_commit.id,
            "host",
            vec![baseline_option],
        )
        .await
        .expect("baseline snapshot should persist");
        persist_flake_output_snapshot_tx(
            &mut baseline_tx,
            baseline_commit.id,
            &json!({
                "declared_systems": ["host"],
                "exported_modules": [],
                "inputs": []
            }),
        )
        .await
        .expect("baseline output snapshot should persist");
        baseline_tx
            .commit()
            .await
            .expect("baseline transaction should commit");
        set_commit_first_parent_by_repo_url(
            &pool,
            &source_url,
            &selected_revision,
            Some(&baseline_revision),
        )
        .await
        .expect("selected first parent should persist");

        let derivation = insert_derivation(&pool, Some(&selected_commit), "host", "nixos")
            .await
            .expect("selected derivation should persist");
        sqlx::query(
            "UPDATE derivations SET store_path = $2, closure_total = 731, completed_at = now() WHERE id = $1",
        )
        .bind(derivation.id)
        .bind("/nix/store/selected-system")
        .execute(&pool)
        .await
        .expect("selected derivation metadata should persist");
        sqlx::query(
            "UPDATE evaluation_snapshots SET completed_at = now(), evaluation_duration_ms = 845 WHERE id = $1",
        )
        .bind(snapshot_id)
        .execute(&pool)
        .await
        .expect("evaluation facts should persist");
        sqlx::query(
            r#"
            INSERT INTO evaluation_generation_snapshots (
                system_id, generation, snapshot_id, derivation_id, commit_id,
                source_store_path
            ) VALUES ($1, 72, $2, $3, $4, '/nix/store/retained-generation')
            "#,
        )
        .bind(source_system.id)
        .bind(snapshot_id)
        .bind(derivation.id)
        .bind(selected_commit.id)
        .execute(&pool)
        .await
        .expect("retained generation fixture should persist");
        sqlx::query(
            "INSERT INTO system_states (hostname, change_reason, store_path, generation_matches_current_store_path, timestamp) VALUES ($1, 'startup', $2, true, now())",
        )
        .bind(&source_system.hostname)
        .bind("/nix/store/selected-system")
        .execute(&pool)
        .await
        .expect("running state should persist");

        let selected = select_commit_snapshot(&pool, source_system.id, &selected_revision)
            .await
            .expect("selected snapshot query should succeed")
            .expect("selected snapshot should exist");
        let summary = get_selected_evaluation_summary(&pool, source_system.id, &selected)
            .await
            .expect("visible summary should load");
        assert_eq!(summary.option_total, 9);
        assert_eq!(summary.module_source_total, 8);
        assert_eq!(summary.evaluation_duration_ms, Some(845));
        assert!(summary.completed_at.is_some());
        assert_eq!(summary.closure_package_count, Some(731));
        assert_eq!(summary.closure_size_bytes, None);
        assert_eq!(summary.host_delta_count, Some(0));
        assert_eq!(
            summary.selected_store_path.as_deref(),
            Some("/nix/store/selected-system")
        );
        assert_eq!(summary.selected_store_path, summary.running_store_path);
        assert_eq!(summary.running_profile_matches, Some(true));
        assert_eq!(summary.agent_fingerprint, AgentFingerprintStatus::Matches);
        assert_eq!(
            summary.seven_day_drift,
            SevenDayDriftStatus::InsufficientCoverage
        );
        assert_eq!(summary.drift, EvaluationDrift::Matches);
        let module_page = module_sources_page(
            get_evaluation_module_sources_page(
                &pool,
                source_system.id,
                &selected,
                Some(viewer.id),
                None,
                100,
                0,
            )
            .await
            .expect("visible module page should load"),
        );
        assert_eq!(module_page.total, summary.module_source_total);
        let self_module = module_page
            .sources
            .iter()
            .find(|module| {
                module.source_input.as_deref() == Some("self")
                    && module.source_revision.as_deref() == Some(selected_revision.as_str())
            })
            .expect("self module should aggregate");
        assert_eq!((self_module.defined_count, self_module.won_count), (2, 1));
        assert_eq!(
            self_module
                .tracked_flake
                .as_ref()
                .map(|identity| (&identity.flake_id, identity.revision.as_str())),
            Some((&source_flake.id, selected_revision.as_str()))
        );
        let self_identity = self_module
            .tracked_flake
            .as_ref()
            .expect("self identity should resolve by internal flake identity");
        assert!(!self_identity.repo_url.contains("super-secret"));
        assert!(!self_identity.repo_url.contains("token="));
        let mismatched_self = module_page
            .sources
            .iter()
            .find(|module| module.source_path == "modules/mismatched-self.nix")
            .expect("mismatched self provenance should remain visible but untracked");
        assert!(mismatched_self.tracked_flake.is_none());
        let visible_identity = module_page
            .sources
            .iter()
            .find(|module| module.source_input.as_deref() == Some("visible"))
            .and_then(|module| module.tracked_flake.as_ref())
            .expect("visible exact repository identity should resolve");
        assert_eq!(visible_identity.revision, "b".repeat(40));
        let tracked_inputs = module_page
            .sources
            .iter()
            .filter_map(|module| {
                module
                    .tracked_flake
                    .as_ref()
                    .map(|_| module.source_input.as_deref().unwrap_or_default())
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(tracked_inputs, ["self", "visible"].into_iter().collect());
        for input in ["hidden", "deleted", "stale", "untracked", "ambiguous"] {
            assert!(
                module_page
                    .sources
                    .iter()
                    .find(|module| module.source_input.as_deref() == Some(input))
                    .is_some_and(|module| module.tracked_flake.is_none()),
                "{input} must not disclose a navigation identity"
            );
        }

        let option_page = query_options_page(
            &pool,
            source_system.id,
            &selected,
            Some(viewer.id),
            "services.summary.selfOne",
            EvaluatedOptionFilter::All,
            10,
            0,
        )
        .await
        .expect("direct option provenance should load");
        assert!(option_page.comparison_available);
        let option_row = option_page
            .options
            .first()
            .expect("direct option should be returned");
        let selected_identity = option_row
            .option
            .as_ref()
            .and_then(|option| option.definitions[0].tracked_flake.as_ref())
            .expect("selected direct provenance should resolve");
        assert_eq!(selected_identity.revision, selected_revision);
        assert!(!selected_identity.repo_url.contains("super-secret"));
        let baseline_identity = option_row
            .before
            .as_ref()
            .and_then(|option| option.definitions[0].tracked_flake.as_ref())
            .expect("baseline direct provenance should use baseline context");
        assert_eq!(baseline_identity.revision, baseline_revision);

        sqlx::query(
            "INSERT INTO system_states (hostname, change_reason, store_path, generation_matches_current_store_path, timestamp) VALUES ($1, 'startup', $2, false, now() + interval '1 second')",
        )
        .bind(&source_system.hostname)
        .bind("/nix/store/out-of-band-system")
        .execute(&pool)
        .await
        .expect("drifted running state should persist");
        let drifted = get_selected_evaluation_summary(&pool, source_system.id, &selected)
            .await
            .expect("drifted summary should load");
        assert_eq!(drifted.drift, EvaluationDrift::Differs);
        assert_eq!(drifted.running_profile_matches, Some(false));

        let mut failed_selected = selected.clone();
        failed_selected.lifecycle = SnapshotLifecycle::Failed;
        failed_selected.error = Some("evaluation failed".into());
        let failed = get_selected_evaluation_summary(&pool, source_system.id, &failed_selected)
            .await
            .expect("failed summary should remain lifecycle-aware");
        assert_eq!(failed.lifecycle, SnapshotLifecycle::Failed);
        assert_eq!(failed.module_source_total, 0);
        assert_eq!(failed.option_total, 0);
        assert!(failed.selected_store_path.is_none());
        assert_eq!(failed.drift, EvaluationDrift::Unavailable);

        sqlx::query(
            r#"
            UPDATE evaluation_option_contents content
            SET payload = jsonb_set(content.payload, '{value,kind}', '"unknown"'::jsonb)
            FROM evaluation_snapshot_options item
            WHERE item.snapshot_id = $1 AND item.content_digest = content.digest
            "#,
        )
        .bind(snapshot_id)
        .execute(&pool)
        .await
        .expect("corrupt summary fixture should persist");
        let corrupt = get_selected_evaluation_summary(&pool, source_system.id, &selected)
            .await
            .expect("corrupt summary should degrade without an internal error");
        assert_eq!(corrupt.lifecycle, SnapshotLifecycle::Unavailable);
        assert_eq!(corrupt.module_source_total, 0);
        assert!(corrupt.completed_at.is_none());
        assert!(corrupt.evaluation_duration_ms.is_none());
        assert_eq!(corrupt.option_total, 0);
        assert!(corrupt.selected_store_path.is_none());
        assert!(corrupt.running_store_path.is_none());
        assert_eq!(corrupt.drift, EvaluationDrift::Unavailable);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn exported_module_declaration_pagination_is_stable_bounded_and_read_only(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/module-pages-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("module-pages-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake insert should succeed");
        let revision = "d".repeat(40);
        let commit = insert_test_commit(&pool, &repo_url, &revision).await;
        let declarations = (0..205)
            .rev()
            .map(|index| {
                json!({
                    "path": format!("services.fixture.option{index:03}"),
                    "declared_type": "string",
                    "has_default": true,
                    "default": format!("value-{index:03}"),
                    "source_paths": [format!("modules/{index:03}.nix")]
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "declared_systems": [],
            "exported_modules": [{
                "name": "large",
                "description": "Large deterministic module",
                "declarations": declarations,
                "consumers": [],
                "declaration_count": 205,
                "consumer_count": 0,
                "error": null
            }],
            "inputs": []
        });
        let mut tx = pool.begin().await.expect("transaction should begin");
        persist_flake_output_snapshot_tx(&mut tx, commit.id, &payload)
            .await
            .expect("flake output snapshot should persist");
        tx.commit().await.expect("transaction should commit");

        let rows_before: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM flake_output_snapshots), \
                    (SELECT COUNT(*) FROM flake_output_contents)",
        )
        .fetch_one(&pool)
        .await
        .expect("snapshot counts should load");
        let first = match get_flake_module_declarations(
            &pool, flake.id, &revision, "large", None, 1_000, 0,
        )
        .await
        .expect("first declaration page should load")
        {
            FlakeModuleDeclarationsQuery::Page(page) => page,
            other => panic!("expected first declaration page, got {other:?}"),
        };
        assert_eq!(first.lifecycle, SnapshotLifecycle::Available);
        assert_eq!(first.total, 205);
        assert_eq!(first.offset, 0);
        assert_eq!(first.limit, 100);
        assert_eq!(first.declarations.len(), 100);
        let token = first
            .snapshot_token
            .clone()
            .expect("available page must have a snapshot token");

        let second = match get_flake_module_declarations(
            &pool,
            flake.id,
            &revision,
            "large",
            Some(&token),
            100,
            100,
        )
        .await
        .expect("second declaration page should load")
        {
            FlakeModuleDeclarationsQuery::Page(page) => page,
            other => panic!("expected second declaration page, got {other:?}"),
        };
        let third = match get_flake_module_declarations(
            &pool,
            flake.id,
            &revision,
            "large",
            Some(&token),
            100,
            200,
        )
        .await
        .expect("third declaration page should load")
        {
            FlakeModuleDeclarationsQuery::Page(page) => page,
            other => panic!("expected third declaration page, got {other:?}"),
        };
        assert_eq!(second.declarations.len(), 100);
        assert_eq!(third.declarations.len(), 5);
        let paths = first
            .declarations
            .iter()
            .chain(&second.declarations)
            .chain(&third.declarations)
            .map(|declaration| declaration.path.clone())
            .collect::<Vec<_>>();
        let unique = paths.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(paths.len(), 205);
        assert_eq!(unique.len(), 205, "pages must not duplicate declarations");
        assert!(paths.windows(2).all(|pair| pair[0] < pair[1]));
        let repeated = match get_flake_module_declarations(
            &pool,
            flake.id,
            &revision,
            "large",
            Some(&token),
            100,
            100,
        )
        .await
        .expect("repeated page should load")
        {
            FlakeModuleDeclarationsQuery::Page(page) => page,
            other => panic!("expected repeated declaration page, got {other:?}"),
        };
        assert_eq!(second, repeated, "ordering must be deterministic");

        let summary = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::All,
            100,
            0,
        )
        .await
        .expect("summary should load")
        .expect("summary revision should exist");
        let module = &summary.outputs.expect("summary payload should exist")["exported_modules"][0];
        assert_eq!(module["declaration_count"], 205);
        assert_eq!(module["declarations"], json!([]));
        assert_eq!(module["declarations_complete"], false);

        let rows_after: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM flake_output_snapshots), \
                    (SELECT COUNT(*) FROM flake_output_contents)",
        )
        .fetch_one(&pool)
        .await
        .expect("snapshot counts should reload");
        assert_eq!(
            rows_before, rows_after,
            "read APIs must not mutate snapshots"
        );

        let mut replacement = payload;
        replacement["exported_modules"][0]["declarations"]
            .as_array_mut()
            .expect("replacement declarations should be an array")
            .push(json!({
                "path": "services.fixture.option999",
                "declared_type": "string",
                "has_default": false,
                "default": null,
                "source_paths": ["modules/999.nix"]
            }));
        replacement["exported_modules"][0]["declaration_count"] = json!(206);
        let mut replacement_tx = pool.begin().await.expect("transaction should begin");
        persist_flake_output_snapshot_tx(&mut replacement_tx, commit.id, &replacement)
            .await
            .expect("replacement snapshot should persist");
        replacement_tx
            .commit()
            .await
            .expect("replacement should commit");
        assert_eq!(
            get_flake_module_declarations(
                &pool,
                flake.id,
                &revision,
                "large",
                Some(&token),
                100,
                100,
            )
            .await
            .expect("stale-token query should complete"),
            FlakeModuleDeclarationsQuery::SnapshotChanged
        );

        replacement["exported_modules"][0]["declarations"][0]["has_default"] = json!("yes");
        let mut corrupt_tx = pool.begin().await.expect("transaction should begin");
        persist_flake_output_snapshot_tx(&mut corrupt_tx, commit.id, &replacement)
            .await
            .expect("malformed declaration snapshot should persist");
        corrupt_tx
            .commit()
            .await
            .expect("transaction should commit");
        let corrupt =
            match get_flake_module_declarations(&pool, flake.id, &revision, "large", None, 100, 0)
                .await
                .expect("malformed declarations must not propagate a query error")
            {
                FlakeModuleDeclarationsQuery::Page(page) => page,
                other => panic!("expected corrupt lifecycle page, got {other:?}"),
            };
        assert_eq!(corrupt.lifecycle, SnapshotLifecycle::Unavailable);
        assert!(corrupt.snapshot_token.is_none());
        assert_eq!(corrupt.total, 0);
        assert!(corrupt.declarations.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn ac24_metrics_and_filtered_reconciliation_are_authoritative(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/ac24-{suffix}.git");
        let flake = insert_flake(
            &pool,
            &format!("ac24-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake should insert");
        let baseline_revision = "6".repeat(40);
        let baseline_commit = insert_test_commit(&pool, &repo_url, &baseline_revision).await;
        let revision = "7".repeat(40);
        let commit = insert_test_commit(&pool, &repo_url, &revision).await;
        sqlx::query(
            "UPDATE commits SET first_parent_sha = $2, first_parent_resolved = true WHERE id = $1",
        )
        .bind(commit.id)
        .bind(&baseline_revision)
        .execute(&pool)
        .await
        .expect("selected first parent should persist");
        let system = insert_test_system(&pool, flake.id, &suffix).await;
        let environment = create_environment(
            &pool,
            &format!("ac24-environment-{suffix}"),
            None,
            "#123abc",
            true,
            "manual",
            false,
            false,
            false,
        )
        .await
        .expect("environment should insert");
        sqlx::query("UPDATE systems SET environment_id = $2 WHERE id = $1")
            .bind(system.id)
            .bind(environment.id)
            .execute(&pool)
            .await
            .expect("managed environment should persist");
        sqlx::query(
            r#"
            INSERT INTO systems (
                id, hostname, environment_id, is_active, public_key, flake_id,
                derivation, system_configuration_name
            )
            SELECT gen_random_uuid(), 'bulk-' || series.value || '-' || $3,
                   $2, true, source.public_key, $4, source.derivation, 'host'
            FROM systems source
            CROSS JOIN generate_series(1, 1000) series(value)
            WHERE source.id = $1
            "#,
        )
        .bind(system.id)
        .bind(environment.id)
        .bind(&suffix)
        .bind(flake.id)
        .execute(&pool)
        .await
        .expect("large visible fleet should insert set-wise");
        sqlx::query("ANALYZE systems")
            .execute(&pool)
            .await
            .expect("large fleet statistics should refresh");
        let undeclared_system =
            insert_test_system(&pool, flake.id, &format!("undeclared-{suffix}")).await;
        sqlx::query(
            "UPDATE systems SET environment_id = $2, system_configuration_name = 'undeclared' \
             WHERE id = $1",
        )
        .bind(undeclared_system.id)
        .bind(environment.id)
        .execute(&pool)
        .await
        .expect("undeclared managed system should persist");
        let undeclared_system_two =
            insert_test_system(&pool, flake.id, &format!("undeclared-two-{suffix}")).await;
        sqlx::query(
            "UPDATE systems SET environment_id = $2, system_configuration_name = 'undeclared-two' \
             WHERE id = $1",
        )
        .bind(undeclared_system_two.id)
        .bind(environment.id)
        .execute(&pool)
        .await
        .expect("second undeclared managed system should persist");

        let mut tx = pool
            .begin()
            .await
            .expect("snapshot transaction should begin");
        persist_flake_output_snapshot_tx(
            &mut tx,
            baseline_commit.id,
            &json!({
                "declared_systems": ["host"],
                "exported_modules": [],
                "inputs": []
            }),
        )
        .await
        .expect("baseline flake output should persist");
        let selected_id = persist_available_snapshot_tx(
            &mut tx,
            commit.id,
            "host",
            vec![
                option("services.shared", json!("same")),
                option("services.deviation", json!("selected")),
                option("services.addition", json!(true)),
            ],
        )
        .await
        .expect("selected snapshot should persist");
        for configuration in ["peer-one", "peer-two"] {
            persist_available_snapshot_tx(
                &mut tx,
                commit.id,
                configuration,
                vec![
                    option("services.shared", json!("same")),
                    option("services.deviation", json!("base")),
                ],
            )
            .await
            .expect("peer snapshot should persist");
        }
        persist_flake_output_snapshot_tx(
            &mut tx,
            commit.id,
            &json!({
                "declared_systems": ["host", "orphan", "orphan-two"],
                "exported_modules": [],
                "inputs": [{
                    "node": "nixpkgs",
                    "direct": true,
                    "last_modified": 0,
                    "transitive_descendant_count": 72,
                    "direct_descendant_count": 7
                }, {
                    "node": "transitive",
                    "direct": false,
                    "last_modified": 0,
                    "transitive_descendant_count": null,
                    "direct_descendant_count": null
                }]
            }),
        )
        .await
        .expect("flake output should persist");
        tx.commit()
            .await
            .expect("snapshot transaction should commit");

        let derivation = insert_derivation(&pool, Some(&commit), "host", "nixos")
            .await
            .expect("derivation should insert");
        sqlx::query(
            "UPDATE derivations SET store_path = $2, closure_total = 3, \
             closure_size_bytes = 4096 WHERE id = $1",
        )
        .bind(derivation.id)
        .bind("/nix/store/ac24-selected")
        .execute(&pool)
        .await
        .expect("closure facts should persist");
        crate::queries::derivations::set_closure_counts(&pool, derivation.id, 3, 0, None)
            .await
            .expect("an unavailable repeat must preserve measured closure bytes");
        let preserved_closure_size: Option<i64> =
            sqlx::query_scalar("SELECT closure_size_bytes FROM derivations WHERE id = $1")
                .bind(derivation.id)
                .fetch_one(&pool)
                .await
                .expect("preserved closure size should load");
        assert_eq!(preserved_closure_size, Some(4096));
        sqlx::query(
            "INSERT INTO system_states \
             (hostname, change_reason, store_path, timestamp) \
             VALUES ($1, 'startup', '/nix/store/pre-window-drift', \
                     now() - interval '7 days 5 minutes')",
        )
        .bind(&system.hostname)
        .execute(&pool)
        .await
        .expect("pre-window boundary state should persist");
        let state_id: i32 = sqlx::query_scalar(
            "INSERT INTO system_states \
             (hostname, change_reason, store_path, timestamp) \
             VALUES ($1, 'startup', $2, now() - interval '7 days') RETURNING id",
        )
        .bind(&system.hostname)
        .bind("/nix/store/ac24-selected")
        .fetch_one(&pool)
        .await
        .expect("initial running state should persist");
        assert_eq!(
            seven_day_drift_status(&pool, system.id, Some("/nix/store/ac24-selected"))
                .await
                .expect("incomplete history should classify"),
            SevenDayDriftStatus::InsufficientCoverage
        );
        sqlx::query(
            "INSERT INTO agent_heartbeats (system_state_id, timestamp) \
             SELECT $1, observed_at FROM generate_series( \
                 now() - interval '7 days', now(), interval '30 minutes' \
             ) observed_at",
        )
        .bind(state_id)
        .execute(&pool)
        .await
        .expect("continuous heartbeat observations should persist");

        assert_eq!(
            seven_day_drift_status(&pool, system.id, Some("/nix/store/ac24-selected"))
                .await
                .expect("pre-window drift must not affect the seven-day result"),
            SevenDayDriftStatus::NoObservedDrift
        );

        let selected = select_commit_snapshot(&pool, system.id, &revision)
            .await
            .expect("snapshot selection should succeed")
            .expect("selected snapshot should exist");
        assert_eq!(selected.id, selected_id);
        let summary = get_selected_evaluation_summary(&pool, system.id, &selected)
            .await
            .expect("summary should load");
        assert_eq!(summary.host_delta_count, Some(2));
        assert_eq!(summary.closure_size_bytes, Some(4096));
        assert_eq!(summary.agent_fingerprint, AgentFingerprintStatus::Matches);
        assert_eq!(
            summary.seven_day_drift,
            SevenDayDriftStatus::NoObservedDrift
        );

        sqlx::query(
            "INSERT INTO system_states (hostname, change_reason, store_path, timestamp) \
             VALUES ($1, 'state_delta', '/nix/store/ac24-drift', now() - interval '1 day')",
        )
        .bind(&system.hostname)
        .execute(&pool)
        .await
        .expect("drift observation should persist");
        assert_eq!(
            seven_day_drift_status(&pool, system.id, Some("/nix/store/ac24-selected"))
                .await
                .expect("drift should classify"),
            SevenDayDriftStatus::ObservedDrift
        );

        let baseline_derivation = insert_derivation(&pool, Some(&baseline_commit), "host", "nixos")
            .await
            .expect("baseline derivation should insert");
        sqlx::query("UPDATE derivations SET store_path = '/nix/store/ac24-pinned' WHERE id = $1")
            .bind(baseline_derivation.id)
            .execute(&pool)
            .await
            .expect("baseline path should persist");
        sqlx::query(
            "INSERT INTO system_states (hostname, change_reason, store_path, timestamp) \
             VALUES ($1, 'state_delta', '/nix/store/ac24-pinned', now())",
        )
        .bind(&system.hostname)
        .execute(&pool)
        .await
        .expect("pinned running revision should persist");

        let unmanaged = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            0,
        )
        .await
        .expect("filtered output should load")
        .expect("filtered output should exist");
        assert_eq!(unmanaged.declared_unmanaged_count, 2);
        assert_eq!(unmanaged.managed_undeclared_count, 2);
        assert_eq!(unmanaged.stale_direct_input_count, 1);
        assert_eq!(unmanaged.output_collapsed_count, 1);
        assert_eq!(unmanaged.pinned_revision_count, 1);
        assert_eq!(unmanaged.managed_system_count, 1003);
        assert_eq!(unmanaged.declared_system_count, 3);
        assert_eq!(unmanaged.previous_declared_system_count, Some(1));
        assert!(unmanaged.snapshot_token.is_some());
        assert_eq!(unmanaged.systems.len(), 1);
        assert_eq!(unmanaged.pagination.system_total, 2);
        assert!(unmanaged.pagination.systems_has_more);
        let delta = unmanaged
            .delta
            .as_ref()
            .expect("first page should include delta");
        assert_eq!(delta.systems_added_total, 2);
        assert_eq!(
            delta.systems_added.len(),
            1,
            "delta samples stay page-bounded"
        );
        assert_eq!(unmanaged.systems[0].configuration_name, "orphan");
        let stale = get_flake_output_snapshot_with_token(
            &pool,
            flake.id,
            &revision,
            Some("stale-token"),
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            1,
        )
        .await
        .expect_err("replacement token must reject continuation");
        assert_eq!(stale.to_string(), FLAKE_OUTPUT_SNAPSHOT_CHANGED);
        let unmanaged_continuation = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            1,
        )
        .await
        .expect("filtered continuation should load")
        .expect("filtered continuation should exist");
        assert_eq!(unmanaged_continuation.declared_unmanaged_count, 2);
        assert_eq!(unmanaged_continuation.managed_undeclared_count, 2);
        assert_eq!(unmanaged_continuation.systems.len(), 1);
        assert!(!unmanaged_continuation.pagination.systems_has_more);
        assert!(unmanaged_continuation.delta.is_none());
        assert_eq!(
            unmanaged_continuation.systems[0].configuration_name,
            "orphan-two"
        );

        let undeclared = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::ManagedUndeclared,
            1,
            0,
        )
        .await
        .expect("managed-undeclared output should load")
        .expect("managed-undeclared output should exist");
        assert_eq!(undeclared.declared_unmanaged_count, 2);
        assert_eq!(undeclared.managed_undeclared_count, 2);
        assert_eq!(undeclared.systems.len(), 1);
        assert_eq!(undeclared.systems[0].system_id, Some(undeclared_system.id));
        let undeclared_continuation = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::ManagedUndeclared,
            1,
            1,
        )
        .await
        .expect("managed-undeclared continuation should load")
        .expect("managed-undeclared continuation should exist");
        assert_eq!(undeclared_continuation.systems.len(), 1);
        assert_eq!(
            undeclared_continuation.systems[0].system_id,
            Some(undeclared_system_two.id)
        );

        let managed = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::All,
            100,
            1000,
        )
        .await
        .expect("managed output should load")
        .expect("managed output should exist");
        let managed_row = managed
            .systems
            .iter()
            .find(|row| row.system_id == Some(system.id))
            .expect("managed row should be visible");
        assert_eq!(
            managed_row.environment_name.as_deref(),
            Some(environment.name.as_str())
        );
        assert_eq!(managed_row.environment_color.as_deref(), Some("#123abc"));
        assert_eq!(
            unmanaged
                .outputs
                .as_ref()
                .expect("output payload should exist")["inputs"][0]["transitive_descendant_count"],
            72
        );
        assert_eq!(
            unmanaged
                .outputs
                .as_ref()
                .expect("output payload should exist")["inputs"][0]["direct_descendant_count"],
            7
        );

        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .execute(&pool)
            .await
            .expect("query-count extension should install");
        sqlx::query("SELECT pg_stat_statements_reset()")
            .execute(&pool)
            .await
            .expect("query counters should reset");
        let bounded = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::All,
            1,
            999,
        )
        .await
        .expect("large fleet page should load")
        .expect("large fleet snapshot should exist");
        assert_eq!(bounded.systems.len(), 1);
        assert_eq!(bounded.managed_system_count, 1003);
        let reconciliation_calls: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(calls), 0)::bigint FROM pg_stat_statements \
             WHERE query LIKE '%WITH parameters AS (%' \
               AND query LIKE '%visible_systems AS (%' \
               AND query LIKE '%AS systems,%'",
        )
        .fetch_one(&pool)
        .await
        .expect("reconciliation query count should load");
        assert_eq!(reconciliation_calls, 1, "one page must use one fleet query");

        let indexes: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT to_regclass('idx_system_states_hostname_timestamp_id')::text, \
                    to_regclass('idx_agent_heartbeats_system_state_timestamp')::text",
        )
        .fetch_one(&pool)
        .await
        .expect("observation indexes should resolve");
        assert!(indexes.0.is_some());
        assert!(indexes.1.is_some());

        let mut plan_tx = pool.begin().await.expect("plan transaction should begin");
        sqlx::query("SET LOCAL enable_seqscan = off")
            .execute(&mut *plan_tx)
            .await
            .expect("sequential scans should disable for plan assertion");
        let plan = sqlx::query(
            "EXPLAIN (FORMAT TEXT) \
             SELECT heartbeat.timestamp, state.store_path \
             FROM system_states state \
             JOIN agent_heartbeats heartbeat ON heartbeat.system_state_id = state.id \
             WHERE state.hostname = $1 \
               AND heartbeat.timestamp >= now() - interval '7 days 90 minutes' \
               AND heartbeat.timestamp <= now()",
        )
        .bind(&system.hostname)
        .fetch_all(&mut *plan_tx)
        .await
        .expect("observation query should explain")
        .into_iter()
        .map(|row| row.get::<String, _>("QUERY PLAN"))
        .collect::<Vec<_>>()
        .join("\n");
        assert!(plan.contains("idx_system_states_hostname_timestamp_id"));
        assert!(plan.contains("idx_agent_heartbeats_system_state_timestamp"));
        let fleet_plan = sqlx::query(
            "EXPLAIN (FORMAT TEXT) \
             SELECT id, hostname, \
                    COALESCE(NULLIF(btrim(system_configuration_name), ''), hostname) \
             FROM systems WHERE flake_id = $1 AND is_active = true \
             ORDER BY COALESCE(NULLIF(btrim(system_configuration_name), ''), hostname), hostname",
        )
        .bind(flake.id)
        .fetch_all(&mut *plan_tx)
        .await
        .expect("large fleet access should explain")
        .into_iter()
        .map(|row| row.get::<String, _>("QUERY PLAN"))
        .collect::<Vec<_>>()
        .join("\n");
        assert!(fleet_plan.contains("idx_systems_active_flake_effective_config"));
        plan_tx
            .rollback()
            .await
            .expect("plan transaction should roll back");

        let original_token = unmanaged
            .snapshot_token
            .as_deref()
            .expect("available output should issue a token")
            .to_string();
        let mut parent_replacement_tx = pool
            .begin()
            .await
            .expect("parent replacement transaction should begin");
        persist_flake_output_snapshot_tx(
            &mut parent_replacement_tx,
            baseline_commit.id,
            &json!({
                "declared_systems": ["host", "parent-added"],
                "exported_modules": [],
                "inputs": []
            }),
        )
        .await
        .expect("parent replacement should persist");
        parent_replacement_tx
            .commit()
            .await
            .expect("parent replacement should commit");
        let parent_replaced = get_flake_output_snapshot_with_token(
            &pool,
            flake.id,
            &revision,
            Some(&original_token),
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            1,
        )
        .await
        .expect_err("parent-only replacement must invalidate the token");
        assert_eq!(parent_replaced.to_string(), FLAKE_OUTPUT_SNAPSHOT_CHANGED);

        let parent_token = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            0,
        )
        .await
        .expect("replacement page should load")
        .expect("replacement page should exist")
        .snapshot_token
        .expect("replacement page should issue a token");
        sqlx::query(
            "UPDATE commits SET first_parent_sha = NULL, first_parent_resolved = true WHERE id = $1",
        )
        .bind(commit.id)
        .execute(&pool)
        .await
        .expect("root transition should persist");
        let root_transition = get_flake_output_snapshot_with_token(
            &pool,
            flake.id,
            &revision,
            Some(&parent_token),
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            1,
        )
        .await
        .expect_err("transition to no parent must invalidate the token");
        assert_eq!(root_transition.to_string(), FLAKE_OUTPUT_SNAPSHOT_CHANGED);

        let tokenless_root_continuation = get_flake_output_snapshot(
            &pool,
            flake.id,
            &revision,
            None,
            FlakeSystemFilter::DeclaredUnmanaged,
            1,
            1,
        )
        .await
        .expect("tokenless positive offset should retain compatibility")
        .expect("root snapshot should still exist");
        assert!(tokenless_root_continuation.snapshot_token.is_some());
        assert!(!tokenless_root_continuation.comparison_available);
        assert!(tokenless_root_continuation.previous_outputs.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn materialized_host_delta_scales_across_large_multi_configuration_corpus(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/host-delta-{suffix}.git");
        let _flake = insert_flake(
            &pool,
            &format!("host-delta-{suffix}"),
            &repo_url,
            "main",
            "cf_systems_only",
        )
        .await
        .expect("flake should insert");
        let commit = insert_test_commit(&pool, &repo_url, &"8".repeat(40)).await;
        let mut tx = pool
            .begin()
            .await
            .expect("snapshot transaction should begin");
        let selected_id = persist_available_snapshot_tx(
            &mut tx,
            commit.id,
            "host-000",
            vec![option("services.fixture.option0000", json!(true))],
        )
        .await
        .expect("seed snapshot should persist");
        let shared_digest: Vec<u8> = sqlx::query_scalar(
            "SELECT content_digest FROM evaluation_snapshot_options \
             WHERE snapshot_id = $1 LIMIT 1",
        )
        .bind(selected_id)
        .fetch_one(&mut *tx)
        .await
        .expect("shared digest should load");
        sqlx::query(
            "INSERT INTO evaluation_snapshot_options \
             (snapshot_id, option_path, content_digest, is_overridden) \
             SELECT $1, 'services.fixture.option' || lpad(value::text, 4, '0'), $2, false \
             FROM generate_series(1, 999) value",
        )
        .bind(selected_id)
        .bind(&shared_digest)
        .execute(&mut *tx)
        .await
        .expect("large selected option corpus should insert");
        sqlx::query("UPDATE evaluation_snapshots SET option_count = 1000 WHERE id = $1")
            .bind(selected_id)
            .execute(&mut *tx)
            .await
            .expect("selected option count should update");
        sqlx::query(
            "INSERT INTO evaluation_snapshots \
             (commit_id, configuration_name, lifecycle, option_count, module_count, \
              content_bytes, completed_at) \
             SELECT $1, 'host-' || lpad(value::text, 3, '0'), 'available', 1000, 1, 1, now() \
             FROM generate_series(1, 127) value",
        )
        .bind(commit.id)
        .execute(&mut *tx)
        .await
        .expect("large configuration corpus should insert");
        sqlx::query(
            "INSERT INTO evaluation_snapshot_options \
             (snapshot_id, option_path, content_digest, is_overridden) \
             SELECT snapshot.id, source.option_path, source.content_digest, source.is_overridden \
             FROM evaluation_snapshots snapshot \
             CROSS JOIN evaluation_snapshot_options source \
             WHERE snapshot.commit_id = $1 AND snapshot.id <> $2 \
               AND source.snapshot_id = $2",
        )
        .bind(commit.id)
        .bind(selected_id)
        .execute(&mut *tx)
        .await
        .expect("shared option references should insert set-wise");
        recompute_host_deltas_tx(&mut tx, commit.id)
            .await
            .expect("large modal corpus should materialize");
        tx.commit().await.expect("large corpus should commit");

        let counts: Vec<i64> = sqlx::query_scalar(
            "SELECT host_delta_count FROM evaluation_snapshots \
             WHERE commit_id = $1 ORDER BY configuration_name",
        )
        .bind(commit.id)
        .fetch_all(&pool)
        .await
        .expect("materialized counts should load");
        assert_eq!(counts.len(), 128);
        assert!(counts.iter().all(|count| *count == 0));

        let mut alternate = option("services.fixture.option0000", json!(true));
        alternate.definitions[0].source_path = "/nix/store/source/other-module.nix".into();
        let alternate_digest = alternate.content_digest();
        let alternate_search_text = alternate.search_text();
        let alternate_payload = json!({
            "declared_type": alternate.declared_type,
            "value": alternate.value,
            "definitions": alternate.definitions,
            "overridden": alternate.overridden,
        });
        let mut tx = pool
            .begin()
            .await
            .expect("replacement transaction should begin");
        sqlx::query(
            "INSERT INTO evaluation_option_contents (digest, payload, search_text) \
             VALUES ($1, $2, $3)",
        )
        .bind(alternate_digest.as_slice())
        .bind(alternate_payload)
        .bind(alternate_search_text)
        .execute(&mut *tx)
        .await
        .expect("provenance-only alternate content should insert");
        sqlx::query(
            "UPDATE evaluation_snapshot_options SET content_digest = $2 \
             WHERE snapshot_id = $1 AND option_path = 'services.fixture.option0000'",
        )
        .bind(selected_id)
        .bind(alternate_digest.as_slice())
        .execute(&mut *tx)
        .await
        .expect("selected provenance should change");
        recompute_host_deltas_tx(&mut tx, commit.id)
            .await
            .expect("replacement should recompute the same-commit modal base");
        tx.commit().await.expect("replacement should commit");

        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .execute(&pool)
            .await
            .expect("query-count extension should install");
        sqlx::query("SELECT pg_stat_statements_reset()")
            .execute(&pool)
            .await
            .expect("query counters should reset");
        let selected_delta: Option<i64> =
            sqlx::query_scalar("SELECT host_delta_count FROM evaluation_snapshots WHERE id = $1")
                .bind(selected_id)
                .fetch_one(&pool)
                .await
                .expect("selected scalar delta should load");
        assert_eq!(selected_delta, Some(1));
        let scalar_calls: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(calls), 0)::bigint FROM pg_stat_statements \
             WHERE query LIKE 'SELECT host_delta_count FROM evaluation_snapshots WHERE id = $1%'",
        )
        .fetch_one(&pool)
        .await
        .expect("scalar host-delta query count should load");
        assert_eq!(scalar_calls, 1, "host delta must remain one scalar read");
        let peer_deltas: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM evaluation_snapshots \
             WHERE commit_id = $1 AND id <> $2 AND host_delta_count <> 0",
        )
        .bind(commit.id)
        .bind(selected_id)
        .fetch_one(&pool)
        .await
        .expect("peer deltas should load");
        assert_eq!(peer_deltas, 0);

        let mut plan_tx = pool.begin().await.expect("plan transaction should begin");
        sqlx::query("SET LOCAL enable_seqscan = off")
            .execute(&mut *plan_tx)
            .await
            .expect("sequential scans should disable");
        let plan = sqlx::query(
            "EXPLAIN (FORMAT TEXT) \
             SELECT host_delta_count FROM evaluation_snapshots WHERE id = $1",
        )
        .bind(selected_id)
        .fetch_all(&mut *plan_tx)
        .await
        .expect("scalar host delta read should explain")
        .into_iter()
        .map(|row| row.get::<String, _>("QUERY PLAN"))
        .collect::<Vec<_>>()
        .join("\n");
        assert!(plan.contains("Index Scan"));
        assert!(plan.contains("evaluation_snapshots"));
        plan_tx
            .rollback()
            .await
            .expect("plan transaction should roll back");
    }
}
