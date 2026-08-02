//! Database queries for running-state attestations, trust classification,
//! investigations, and resolution actions.

use crate::models::running_state_attestations::{
    AttestationAssessment, AttestationInvestigation, AttestationResolutionAction,
    AttestationSummary, AttestationTrustSummary, SystemTrustState,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Attestation Insert (immutable)
// ---------------------------------------------------------------------------

/// Insert a new attestation record. Returns the server-assigned row ID.
///
/// This is the only write path for attestation data. No update path exists
/// for signed fields.
pub async fn insert_attestation(
    tx: &mut Transaction<'_, Postgres>,
    attestation_id: Uuid,
    system_id: Uuid,
    agent_key_id: &str,
    agent_session_id: Option<Uuid>,
    protocol_version: i32,
    boot_id: &str,
    boot_timestamp: Option<DateTime<Utc>>,
    observed_at: DateTime<Utc>,
    monotonic_counter: i64,
    current_system_store_path: &str,
    current_system_nar_hash: Option<&str>,
    system_profile_store_path: Option<&str>,
    booted_generation: Option<i64>,
    kernel_version: Option<&str>,
    nix_version: Option<&str>,
    agent_version: &str,
    agent_build_hash: Option<&str>,
    reported_authorization_id: Option<Uuid>,
    reported_execution_id: Option<Uuid>,
    activation_source: Option<&str>,
    canonical_payload: &[u8],
    payload_digest: &[u8],
    signature: &[u8],
    verification_status: &str,
    verification_reason_code: Option<&str>,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO running_state_attestations (
            attestation_id, system_id, agent_key_id, agent_session_id,
            protocol_version, boot_id, boot_timestamp, observed_at,
            monotonic_counter, current_system_store_path, current_system_nar_hash,
            system_profile_store_path, booted_generation, kernel_version, nix_version,
            agent_version, agent_build_hash, reported_authorization_id, reported_execution_id,
            activation_source, canonical_payload, payload_digest, signature,
            verification_status, verification_reason_code
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
            $21, $22, $23, $24, $25
        )
        RETURNING id
        "#,
    )
    .bind(attestation_id)
    .bind(system_id)
    .bind(agent_key_id)
    .bind(agent_session_id)
    .bind(protocol_version)
    .bind(boot_id)
    .bind(boot_timestamp)
    .bind(observed_at)
    .bind(monotonic_counter)
    .bind(current_system_store_path)
    .bind(current_system_nar_hash)
    .bind(system_profile_store_path)
    .bind(booted_generation)
    .bind(kernel_version)
    .bind(nix_version)
    .bind(agent_version)
    .bind(agent_build_hash)
    .bind(reported_authorization_id)
    .bind(reported_execution_id)
    .bind(activation_source)
    .bind(canonical_payload)
    .bind(payload_digest)
    .bind(signature)
    .bind(verification_status)
    .bind(verification_reason_code)
    .fetch_one(&mut **tx)
    .await
    .context("insert attestation")?;

    Ok(row.get::<Uuid, _>("id"))
}

// ---------------------------------------------------------------------------
// Replay Detection
// ---------------------------------------------------------------------------

/// Check if an attestation ID already exists.
pub async fn attestation_id_exists(
    tx: &mut Transaction<'_, Postgres>,
    attestation_id: Uuid,
) -> Result<bool> {
    let row = sqlx::query(
        "SELECT EXISTS(SELECT 1 FROM running_state_attestations WHERE attestation_id = $1) AS exists",
    )
    .bind(attestation_id)
    .fetch_one(&mut **tx)
    .await
    .context("check attestation id exists")?;
    Ok(row.get::<bool, _>("exists"))
}

/// Get the latest accepted monotonic counter for an agent key + boot session.
pub async fn latest_counter_for_boot(
    tx: &mut Transaction<'_, Postgres>,
    agent_key_id: &str,
    boot_id: &str,
) -> Result<Option<i64>> {
    let row = sqlx::query(
        r#"
        SELECT MAX(monotonic_counter) AS max_counter
        FROM running_state_attestations
        WHERE agent_key_id = $1 AND boot_id = $2
          AND verification_status = 'verified'
        "#,
    )
    .bind(agent_key_id)
    .bind(boot_id)
    .fetch_one(&mut **tx)
    .await
    .context("latest counter for boot")?;
    Ok(row.get("max_counter"))
}

// ---------------------------------------------------------------------------
// Assessment
// ---------------------------------------------------------------------------

/// Insert a trust classification assessment for an attestation.
pub async fn insert_assessment(
    tx: &mut Transaction<'_, Postgres>,
    attestation_id: Uuid,
    system_id: Uuid,
    classification: &str,
    reason_code: &str,
    matched_authorization_id: Option<Uuid>,
    matched_deployment_execution_id: Option<Uuid>,
    matched_artifact_id: Option<Uuid>,
    classifier_version: i32,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO running_state_attestation_assessments (
            attestation_id, system_id, classification, reason_code,
            matched_authorization_id, matched_deployment_execution_id,
            matched_artifact_id, classifier_version
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (attestation_id) DO UPDATE SET attestation_id = $1
        RETURNING id
        "#,
    )
    .bind(attestation_id)
    .bind(system_id)
    .bind(classification)
    .bind(reason_code)
    .bind(matched_authorization_id)
    .bind(matched_deployment_execution_id)
    .bind(matched_artifact_id)
    .bind(classifier_version)
    .fetch_one(&mut **tx)
    .await
    .context("insert assessment")?;
    Ok(row.get::<Uuid, _>("id"))
}

// ---------------------------------------------------------------------------
// System Trust State
// ---------------------------------------------------------------------------

/// Upsert the current projected trust state for a system.
pub async fn upsert_system_trust_state(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    classification: &str,
    reason_code: &str,
    latest_attestation_id: Option<Uuid>,
    latest_authorization_id: Option<Uuid>,
    observed_store_path: Option<&str>,
    expected_store_path: Option<&str>,
    evidence_age_seconds: Option<i64>,
    investigation_id: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO system_trust_states (
            system_id, current_classification, reason_code,
            latest_attestation_id, latest_authorization_id,
            observed_store_path, expected_store_path,
            evidence_age_seconds, investigation_id, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
        ON CONFLICT (system_id) DO UPDATE SET
            current_classification = EXCLUDED.current_classification,
            reason_code = EXCLUDED.reason_code,
            latest_attestation_id = EXCLUDED.latest_attestation_id,
            latest_authorization_id = EXCLUDED.latest_authorization_id,
            observed_store_path = EXCLUDED.observed_store_path,
            expected_store_path = EXCLUDED.expected_store_path,
            evidence_age_seconds = EXCLUDED.evidence_age_seconds,
            investigation_id = EXCLUDED.investigation_id,
            updated_at = NOW()
        "#,
    )
    .bind(system_id)
    .bind(classification)
    .bind(reason_code)
    .bind(latest_attestation_id)
    .bind(latest_authorization_id)
    .bind(observed_store_path)
    .bind(expected_store_path)
    .bind(evidence_age_seconds)
    .bind(investigation_id)
    .execute(&mut **tx)
    .await
    .context("upsert system trust state")?;
    Ok(())
}

/// Get the current trust state for a system.
pub async fn get_system_trust_state(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemTrustState>> {
    sqlx::query_as::<_, SystemTrustState>(
        "SELECT * FROM system_trust_states WHERE system_id = $1",
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await
    .context("get system trust state")
}

/// Get the latest verified attestation for a system.
pub async fn get_latest_verified_attestation(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<AttestationSummary>> {
    sqlx::query_as::<_, AttestationSummary>(
        r#"
        SELECT
            id, attestation_id, system_id, agent_key_id,
            observed_at, received_at, current_system_store_path,
            verification_status, boot_id, monotonic_counter, agent_version
        FROM running_state_attestations
        WHERE system_id = $1 AND verification_status = 'verified'
        ORDER BY observed_at DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await
    .context("get latest verified attestation")
}

/// List attestation history for a system (paginated, no payload bytes).
pub async fn list_attestation_history(
    pool: &PgPool,
    system_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<AttestationSummary>> {
    sqlx::query_as::<_, AttestationSummary>(
        r#"
        SELECT
            id, attestation_id, system_id, agent_key_id,
            observed_at, received_at, current_system_store_path,
            verification_status, boot_id, monotonic_counter, agent_version
        FROM running_state_attestations
        WHERE system_id = $1
        ORDER BY observed_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(system_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("list attestation history")
}

// ---------------------------------------------------------------------------
// Trust Summary (Dashboard Widget)
// ---------------------------------------------------------------------------

/// Get attestation trust summary counts.
pub async fn get_trust_summary(pool: &PgPool) -> Result<AttestationTrustSummary> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (
                WHERE current_classification IN ('unauthorized_artifact', 'unknown_artifact', 'agent_identity_invalid')
            ) AS flagged_unresolved,
            COUNT(*) FILTER (
                WHERE current_classification = 'authorized_current'
            ) AS authorized_current,
            COUNT(*) FILTER (
                WHERE current_classification IN ('authorized_but_evidence_stale', 'agent_attestation_stale')
            ) AS stale_evidence
        FROM system_trust_states
        "#,
    )
    .fetch_one(pool)
    .await
    .context("get trust summary")?;

    Ok(AttestationTrustSummary {
        flagged_unresolved: row.get::<i64, _>("flagged_unresolved"),
        authorized_current: row.get::<i64, _>("authorized_current"),
        stale_evidence: row.get::<i64, _>("stale_evidence"),
    })
}

// ---------------------------------------------------------------------------
// Investigations
// ---------------------------------------------------------------------------

/// Open an investigation.
pub async fn open_investigation(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    source_attestation_id: Uuid,
    opened_by_user_id: Uuid,
    opening_note: &str,
    owner_user_id: Option<Uuid>,
) -> Result<AttestationInvestigation> {
    sqlx::query_as::<_, AttestationInvestigation>(
        r#"
        INSERT INTO attestation_investigations (
            system_id, source_attestation_id, status,
            opened_by_user_id, owner_user_id, opening_note
        ) VALUES ($1, $2, 'open', $3, $4, $5)
        RETURNING *
        "#,
    )
    .bind(system_id)
    .bind(source_attestation_id)
    .bind(opened_by_user_id)
    .bind(owner_user_id)
    .bind(opening_note)
    .fetch_one(&mut **tx)
    .await
    .context("open investigation")
}

/// Close an investigation.
pub async fn close_investigation(
    tx: &mut Transaction<'_, Postgres>,
    investigation_id: Uuid,
    resolved_by_user_id: Uuid,
    resolution_reason: &str,
    resolution_note: &str,
) -> Result<AttestationInvestigation> {
    sqlx::query_as::<_, AttestationInvestigation>(
        r#"
        UPDATE attestation_investigations
        SET status = 'resolved',
            resolved_by_user_id = $2,
            resolution_reason = $3,
            resolution_note = $4,
            resolved_at = NOW(),
            updated_at = NOW()
        WHERE id = $1 AND status = 'open'
        RETURNING *
        "#,
    )
    .bind(investigation_id)
    .bind(resolved_by_user_id)
    .bind(resolution_reason)
    .bind(resolution_note)
    .fetch_one(&mut **tx)
    .await
    .context("close investigation")
}

/// Get open investigation for a system.
pub async fn get_open_investigation(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<AttestationInvestigation>> {
    sqlx::query_as::<_, AttestationInvestigation>(
        "SELECT * FROM attestation_investigations WHERE system_id = $1 AND status = 'open'",
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await
    .context("get open investigation")
}

// ---------------------------------------------------------------------------
// Resolution Actions
// ---------------------------------------------------------------------------

/// Record a resolution action.
pub async fn insert_resolution_action(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    attestation_id: Uuid,
    actor_user_id: Uuid,
    action: &str,
    note: &str,
    created_authorization_id: Option<Uuid>,
    created_deployment_request_id: Option<Uuid>,
    investigation_id: Option<Uuid>,
) -> Result<AttestationResolutionAction> {
    sqlx::query_as::<_, AttestationResolutionAction>(
        r#"
        INSERT INTO attestation_resolution_actions (
            system_id, attestation_id, actor_user_id, action, note,
            created_authorization_id, created_deployment_request_id, investigation_id
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(system_id)
    .bind(attestation_id)
    .bind(actor_user_id)
    .bind(action)
    .bind(note)
    .bind(created_authorization_id)
    .bind(created_deployment_request_id)
    .bind(investigation_id)
    .fetch_one(&mut **tx)
    .await
    .context("insert resolution action")
}

// ---------------------------------------------------------------------------
// Systems Attention Counts
// ---------------------------------------------------------------------------

/// Get attention counts for the sidebar Systems badge.
pub async fn get_systems_attention_counts(pool: &PgPool) -> Result<(i64, i64)> {
    // Returns (pending_approvals, flagged_attestations)
    let row = sqlx::query(
        r#"
        SELECT
            (SELECT COUNT(*) FROM deployment_approval_requests WHERE status = 'pending') AS pending_approvals,
            (SELECT COUNT(*) FROM system_trust_states
             WHERE current_classification IN ('unauthorized_artifact', 'unknown_artifact', 'agent_identity_invalid')
            ) AS flagged_attestations
        "#,
    )
    .fetch_one(pool)
    .await
    .context("get systems attention counts")?;

    Ok((
        row.get::<i64, _>("pending_approvals"),
        row.get::<i64, _>("flagged_attestations"),
    ))
}

/// Get all system trust states that need staleness update.
pub async fn get_systems_needing_staleness_update(
    pool: &PgPool,
    freshness_threshold_secs: i64,
) -> Result<Vec<SystemTrustState>> {
    sqlx::query_as::<_, SystemTrustState>(
        r#"
        SELECT sts.* FROM system_trust_states sts
        WHERE sts.current_classification IN ('authorized_current', 'authorized_but_evidence_stale')
          AND sts.latest_attestation_id IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM running_state_attestations rsa
              WHERE rsa.id = sts.latest_attestation_id
                AND rsa.observed_at < NOW() - ($1 || ' seconds')::INTERVAL
          )
        "#,
    )
    .bind(freshness_threshold_secs.to_string())
    .fetch_all(pool)
    .await
    .context("get systems needing staleness update")
}
