//! Rust-authoritative canonical digest helpers.
//!
//! This module is the single canonical implementation of `cf-model-json-1`
//! digests for policies, bundles, and assignment effective-sets. SQL triggers
//! set `semantic_digest = 'pending'` as a sentinel; every Rust write path
//! must call the appropriate helper *within the same transaction* and fail if
//! the digest cannot be computed or stored.
//!
//! # Canonical field sets
//!
//! ## Policy version
//!
//! ```text
//! canonicalization_version, compliance_metadata, config, dependencies,
//! description, execution_phase, implementation_state, name, policy_type
//! ```
//!
//! ## Bundle version
//!
//! ```text
//! canonicalization_version, description, framework, framework_version,
//! layer, name, owner, policy_version_ids (ordered by policy_order)
//! ```
//!
//! ## Assignment effective-set
//!
//! ```text
//! canonicalization_version, additions (sorted UUIDs),
//! effective_policy_version_ids (resolved ordered set),
//! enforcement_mode, exclusions (sorted UUIDs),
//! value_overrides (sorted by policy_version_id, then value_path)
//! ```
//!
//! The typed canonical DTOs and their pure digest computation live in the
//! database-free `cf-compliance` crate and are re-exported here unchanged. Only
//! the transactional persist helpers below are server-local.

use anyhow::Result;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub use cf_compliance::digest::*;

// ── Transactional persist helpers ─────────────────────────────────────────────

/// Write the canonical policy version digest inside the active transaction.
///
/// Targets exactly the current draft version via `current_draft_version_id`
/// with `FOR UPDATE` to prevent TOCTOU races. Fails if no draft version exists
/// so a digest failure rolls back the entire mutation (P1 #5).
pub async fn write_policy_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    policy_id: Uuid,
    canonical: &PolicyVersionCanonical,
) -> Result<()> {
    // Resolve and lock the current draft version pointer.
    let version_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT current_draft_version_id
        FROM deployment_policies
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(policy_id)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();

    let version_id = version_id
        .ok_or_else(|| anyhow::anyhow!("Policy {policy_id} has no current draft version"))?;

    let digest = policy_version_digest(tx, version_id, canonical).await?;
    let mapping_digest = policy_mapping_digest(tx, version_id).await?;
    let rows_affected = sqlx::query(
        r#"
        UPDATE deployment_policy_versions
        SET semantic_digest          = $1,
            mapping_digest            = $2,
            digest_algorithm         = 'sha-256',
            canonicalization_version = 'cf-model-json-1',
            name                     = $4,
            description              = $5,
            policy_type              = $6,
            implementation_state     = $7,
            execution_phase          = $8,
            config                   = $9::jsonb,
            compliance_metadata      = $10::jsonb,
            dependencies             = $11::jsonb,
            enabled_by_default       = $12
        WHERE id = $3
          AND publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .bind(&digest)
    .bind(&mapping_digest)
    .bind(version_id)
    .bind(&canonical.name)
    .bind(&canonical.description)
    .bind(&canonical.policy_type)
    .bind(&canonical.implementation_state)
    .bind(&canonical.execution_phase)
    .bind(&canonical.config)
    .bind(&canonical.compliance_metadata)
    .bind(&canonical.dependencies)
    .bind(&canonical.enabled_by_default)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    if rows_affected != 1 {
        anyhow::bail!("Policy version {version_id} is not in draft state and cannot be updated");
    }
    Ok(())
}

/// Recompute a policy version's modeled semantic digest in the same transaction.
/// Mapping mutations refresh the separate mapping component; this helper keeps
/// the legacy semantic digest contract independent of normalized mappings.
pub async fn refresh_policy_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    policy_version_id: Uuid,
) -> Result<()> {
    let row = sqlx::query_as::<_, PolicyDigestRow>(
        r#"
        SELECT name, description, policy_type, implementation_state,
               execution_phase, config, compliance_metadata, dependencies,
               opaque_xml, enabled_by_default, publication_state
        FROM deployment_policy_versions
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(policy_version_id)
    .fetch_one(&mut **tx)
    .await?;

    if matches!(row.publication_state.as_str(), "accepted" | "deprecated") {
        anyhow::bail!("POLICY_MAPPING_IMMUTABLE: policy version {policy_version_id} is immutable");
    }

    let canonical = PolicyVersionCanonical {
        name: row.name,
        description: row.description,
        policy_type: row.policy_type,
        implementation_state: row.implementation_state,
        execution_phase: row.execution_phase,
        config: row.config,
        compliance_metadata: row.compliance_metadata,
        dependencies: row.dependencies,
        opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(row.opaque_xml.as_deref()),
        enabled_by_default: row.enabled_by_default,
    };
    let digest = policy_version_digest(tx, policy_version_id, &canonical).await?;
    sqlx::query(
        "UPDATE deployment_policy_versions SET semantic_digest = $1, digest_algorithm = 'sha-256', canonicalization_version = 'cf-model-json-1' WHERE id = $2",
    )
    .bind(digest)
    .bind(policy_version_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn policy_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    policy_version_id: Uuid,
    canonical: &PolicyVersionCanonical,
) -> Result<String> {
    let _ = (tx, policy_version_id);
    Ok(canonical.compute_digest())
}

async fn policy_mapping_digest(
    tx: &mut Transaction<'_, Postgres>,
    policy_version_id: Uuid,
) -> Result<String> {
    let rows = sqlx::query_as::<_, PolicyMappingCanonicalRow>(
        "SELECT requirement_version_id, relationship, coverage, rationale, provenance, trust_state FROM policy_requirement_mappings WHERE policy_version_id = $1",
    )
    .bind(policy_version_id)
    .fetch_all(&mut **tx)
    .await?;
    let mut mappings = rows
        .into_iter()
        .map(|row| PolicyMappingCanonical {
            requirement_version_id: row.requirement_version_id,
            relationship: row.relationship,
            coverage: row.coverage,
            rationale: row.rationale,
            provenance: row.provenance,
            trust_state: row.trust_state,
        })
        .collect::<Vec<_>>();
    Ok(compute_mapping_digest(&mut mappings))
}

/// Refresh only the normalized mapping component for a mutable policy version.
pub async fn refresh_policy_mapping_digest(
    tx: &mut Transaction<'_, Postgres>,
    policy_version_id: Uuid,
) -> Result<()> {
    let state: String = sqlx::query_scalar(
        "SELECT publication_state FROM deployment_policy_versions WHERE id = $1 FOR UPDATE",
    )
    .bind(policy_version_id)
    .fetch_one(&mut **tx)
    .await?;
    if matches!(state.as_str(), "accepted" | "deprecated") {
        anyhow::bail!("POLICY_MAPPING_IMMUTABLE: policy version {policy_version_id} is immutable");
    }
    let digest = policy_mapping_digest(tx, policy_version_id).await?;
    sqlx::query("UPDATE deployment_policy_versions SET mapping_digest = $1 WHERE id = $2")
        .bind(digest)
        .bind(policy_version_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct PolicyDigestRow {
    name: String,
    description: Option<String>,
    policy_type: String,
    implementation_state: String,
    execution_phase: String,
    config: Value,
    compliance_metadata: Value,
    dependencies: Value,
    opaque_xml: Option<String>,
    enabled_by_default: Option<bool>,
    publication_state: String,
}

#[derive(sqlx::FromRow)]
struct PolicyMappingCanonicalRow {
    requirement_version_id: Uuid,
    relationship: String,
    coverage: String,
    rationale: Option<String>,
    provenance: String,
    trust_state: String,
}

/// Load the ordered membership entries including `selected`.
pub async fn load_bundle_membership(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
) -> Result<Vec<BundleMembershipEntry>> {
    #[derive(sqlx::FromRow)]
    struct MembershipRow {
        policy_version_id: Uuid,
        selected: bool,
    }
    let rows: Vec<MembershipRow> = sqlx::query_as(
        r#"
        SELECT policy_version_id, selected
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $1
        ORDER BY policy_order ASC
        "#,
    )
    .bind(bundle_version_id)
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| BundleMembershipEntry {
            policy_version_id: r.policy_version_id,
            selected: r.selected,
        })
        .collect())
}

/// Write the canonical bundle version digest inside the active transaction.
///
/// Resolves the current draft version via `current_draft_version_id FOR UPDATE`
/// (P1 #5 and concurrency safety). Fails if no draft version exists.
pub async fn write_bundle_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    bundle_id: Uuid,
    canonical: &BundleVersionCanonical,
) -> Result<()> {
    // Resolve and lock the current draft version pointer.
    let version_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT current_draft_version_id
        FROM compliance_bundles
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(bundle_id)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();

    let version_id = version_id
        .ok_or_else(|| anyhow::anyhow!("Bundle {bundle_id} has no current draft version"))?;
    let digest = bundle_version_digest(tx, version_id, canonical).await?;
    let requirement_digest = bundle_requirement_digest(tx, version_id).await?;

    let rows_affected = sqlx::query(
        r#"
        UPDATE compliance_bundle_versions
        SET semantic_digest          = $1,
            requirement_digest       = $2,
            digest_algorithm         = 'sha-256',
            canonicalization_version = 'cf-model-json-1',
            name                     = $4,
            framework                = $5,
            framework_version        = $6,
            description              = $7,
            layer                    = $8,
            owner                    = $9
        WHERE id = $3
          AND publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .bind(&digest)
    .bind(&requirement_digest)
    .bind(version_id)
    .bind(&canonical.name)
    .bind(&canonical.framework)
    .bind(&canonical.framework_version)
    .bind(&canonical.description)
    .bind(&canonical.layer)
    .bind(&canonical.owner)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    if rows_affected != 1 {
        anyhow::bail!("Bundle version {version_id} is not in draft state and cannot be updated");
    }
    Ok(())
}

pub async fn refresh_bundle_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
) -> Result<()> {
    #[derive(sqlx::FromRow)]
    struct BundleRow {
        name: String,
        framework: String,
        framework_version: Option<String>,
        description: Option<String>,
        layer: String,
        owner: String,
        publication_state: String,
    }
    let row: BundleRow = sqlx::query_as(
        "SELECT bundle_id, name, framework, framework_version, description, layer, owner, publication_state FROM compliance_bundle_versions WHERE id = $1 FOR UPDATE",
    )
    .bind(bundle_version_id)
    .fetch_one(&mut **tx)
    .await?;
    if matches!(row.publication_state.as_str(), "accepted" | "deprecated") {
        anyhow::bail!("BUNDLE_VERSION_IMMUTABLE: bundle version {bundle_version_id} is immutable");
    }
    let canonical = BundleVersionCanonical {
        name: row.name,
        framework: row.framework,
        framework_version: row.framework_version,
        description: row.description,
        layer: row.layer,
        owner: row.owner,
        members: load_bundle_membership(tx, bundle_version_id).await?,
    };
    let digest = bundle_version_digest(tx, bundle_version_id, &canonical).await?;
    sqlx::query("UPDATE compliance_bundle_versions SET semantic_digest = $1, digest_algorithm = 'sha-256', canonicalization_version = 'cf-model-json-1' WHERE id = $2")
        .bind(digest)
        .bind(bundle_version_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn bundle_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
    canonical: &BundleVersionCanonical,
) -> Result<String> {
    let _ = (tx, bundle_version_id);
    Ok(canonical.compute_digest())
}

async fn bundle_requirement_digest(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
) -> Result<String> {
    let mut memberships: Vec<BundleRequirementCanonical> = sqlx::query_as::<_, BundleRequirementRow>(
        "SELECT requirement_version_id, selected FROM compliance_bundle_version_requirements WHERE bundle_version_id = $1",
    )
    .bind(bundle_version_id)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(|row| BundleRequirementCanonical {
        requirement_version_id: row.requirement_version_id,
        selected: row.selected,
    })
    .collect();
    Ok(compute_requirement_membership_digest(&mut memberships))
}

/// Refresh only the normalized requirement-baseline component for a mutable bundle version.
pub async fn refresh_bundle_requirement_digest(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
) -> Result<()> {
    let state: String = sqlx::query_scalar(
        "SELECT publication_state FROM compliance_bundle_versions WHERE id = $1 FOR UPDATE",
    )
    .bind(bundle_version_id)
    .fetch_one(&mut **tx)
    .await?;
    if matches!(state.as_str(), "accepted" | "deprecated") {
        anyhow::bail!("BUNDLE_VERSION_IMMUTABLE: bundle version {bundle_version_id} is immutable");
    }
    let digest = bundle_requirement_digest(tx, bundle_version_id).await?;
    sqlx::query("UPDATE compliance_bundle_versions SET requirement_digest = $1 WHERE id = $2")
        .bind(digest)
        .bind(bundle_version_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct BundleRequirementRow {
    requirement_version_id: Uuid,
    selected: bool,
}

/// Build and write the assignment overlay digest for a single assignment.
///
/// Covers: selected baseline - exclusions + additions.
/// Does NOT include direct environment/system policies (resolved at evaluation).
/// Stored as `assignment_overlay_digest` to accurately reflect the scope (P1 #4).
///
/// The effective-policy resolver that includes direct policies will compute and
/// persist the full effective-set at evaluation time.
pub async fn write_assignment_effective_set_digest(
    tx: &mut Transaction<'_, Postgres>,
    assignment_id: Uuid,
) -> Result<()> {
    // Fetch enforcement mode.
    let enforcement_mode: String = sqlx::query_scalar(
        "SELECT enforcement_mode FROM compliance_bundle_assignments WHERE id = $1",
    )
    .bind(assignment_id)
    .fetch_one(&mut **tx)
    .await?;

    // Exclusions.
    let exclusions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT policy_version_id FROM compliance_assignment_exclusions WHERE assignment_version_id = (SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1) ORDER BY policy_version_id",
    )
    .bind(assignment_id)
    .fetch_all(&mut **tx)
    .await?;

    // Additions.
    let additions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT policy_version_id FROM compliance_assignment_additions WHERE assignment_version_id = (SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1) ORDER BY addition_order",
    )
    .bind(assignment_id)
    .fetch_all(&mut **tx)
    .await?;

    // Value overrides.
    #[derive(sqlx::FromRow)]
    struct Override {
        policy_version_id: Uuid,
        value_path: String,
        value: Value,
    }
    let overrides: Vec<Override> = sqlx::query_as(
        r#"
        SELECT policy_version_id, value_path, value
        FROM compliance_assignment_value_overrides
        WHERE assignment_version_id = (SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1)
        ORDER BY policy_version_id, value_path
        "#,
    )
    .bind(assignment_id)
    .fetch_all(&mut **tx)
    .await?;

    // Ordered assignment overlay policy set:
    //   selected baseline - exclusions (in policy_order)
    //   + additions (in declared assignment order)
    //
    // Uses a CTE with explicit source_rank so the UNION ALL can be
    // ORDER-ed correctly (bare ORDER BY inside UNION ALL operands is
    // invalid PostgreSQL). (P1 #1)
    let effective_policy_version_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        WITH overlay AS (
            SELECT
                bvp.policy_version_id,
                0            AS source_rank,
                bvp.policy_order AS source_order
            FROM compliance_bundle_assignments a
            JOIN compliance_bundle_version_policies bvp
              ON bvp.bundle_version_id = a.bundle_version_id
            WHERE a.id = $1
              AND bvp.selected = TRUE
              AND NOT EXISTS (
                  SELECT 1
                  FROM compliance_assignment_exclusions ex
                   WHERE ex.assignment_version_id = a.current_version_id
                    AND ex.policy_version_id = bvp.policy_version_id
              )

            UNION ALL

            SELECT
                aa.policy_version_id,
                1   AS source_rank,
                ROW_NUMBER() OVER (
                    ORDER BY aa.policy_version_id
                )::integer AS source_order
            FROM compliance_assignment_additions aa
            WHERE aa.assignment_version_id = (SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1)
        )
        SELECT policy_version_id
        FROM overlay
        ORDER BY source_rank, source_order, policy_version_id
        "#,
    )
    .bind(assignment_id)
    .fetch_all(&mut **tx)
    .await?;

    let canonical = AssignmentEffectiveSetCanonical {
        enforcement_mode,
        exclusions,
        additions,
        value_overrides: overrides
            .into_iter()
            .map(|o| (o.policy_version_id, o.value_path, o.value))
            .collect(),
        effective_policy_version_ids,
    };

    let digest = canonical.compute_digest();
    // Write to assignment_overlay_digest (renamed from effective_set_digest in 0201).
    sqlx::query(
        "UPDATE compliance_bundle_assignments SET assignment_overlay_digest = $1 WHERE id = $2",
    )
    .bind(&digest)
    .bind(assignment_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

// ── Startup backfill ──────────────────────────────────────────────────────────

/// Recompute all `pending` semantic and effective-set digests at startup.
///
/// Called once during server initialisation after migrations. Returns an error
/// if any row remains `pending` after the recomputation pass, which prevents
/// the server from starting with invalid identity data.
pub async fn backfill_pending_digests(pool: &PgPool) -> Result<()> {
    tracing::info!("compliance: backfilling pending semantic digests");

    // ── Policy versions ───────────────────────────────────────────────────────
    #[derive(sqlx::FromRow)]
    struct PolicyVersionRow {
        id: Uuid,
    }
    let pending_policies: Vec<PolicyVersionRow> = sqlx::query_as(
        "SELECT id FROM deployment_policy_versions WHERE semantic_digest = 'pending'",
    )
    .fetch_all(pool)
    .await?;

    for row in &pending_policies {
        let mut tx = pool.begin().await?;
        refresh_policy_version_digest(&mut tx, row.id).await?;
        tx.commit().await?;
    }

    // ── Bundle versions ───────────────────────────────────────────────────────
    #[derive(sqlx::FromRow)]
    struct BundleVersionRow {
        id: Uuid,
        bundle_id: Uuid,
        name: String,
        framework: String,
        framework_version: Option<String>,
        description: Option<String>,
        layer: String,
        owner: String,
    }
    let pending_bundles: Vec<BundleVersionRow> = sqlx::query_as(
        r#"
        SELECT id, bundle_id, name, framework, framework_version,
               description, layer, owner
        FROM compliance_bundle_versions
        WHERE semantic_digest = 'pending'
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in &pending_bundles {
        let mut tx = pool.begin().await?;
        refresh_bundle_version_digest(&mut tx, row.id).await?;
        tx.commit().await?;
    }

    // Component backfills intentionally bypass the mutable-version guards: the
    // migration adds these columns to already accepted/deprecated rows. They
    // update only a pending component column and never change semantic content.
    let pending_mapping_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM deployment_policy_versions WHERE mapping_digest = 'pending'",
    )
    .fetch_all(pool)
    .await?;
    for version_id in &pending_mapping_ids {
        let mut tx = pool.begin().await?;
        let digest = policy_mapping_digest(&mut tx, *version_id).await?;
        sqlx::query(
            "UPDATE deployment_policy_versions SET mapping_digest = $1 WHERE id = $2 AND mapping_digest = 'pending'",
        )
        .bind(digest)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }

    let pending_requirement_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_versions WHERE requirement_digest = 'pending'",
    )
    .fetch_all(pool)
    .await?;
    for version_id in &pending_requirement_ids {
        let mut tx = pool.begin().await?;
        let digest = bundle_requirement_digest(&mut tx, *version_id).await?;
        sqlx::query(
            "UPDATE compliance_bundle_versions SET requirement_digest = $1 WHERE id = $2 AND requirement_digest = 'pending'",
        )
        .bind(digest)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }

    // ── Assignment effective-set digests ──────────────────────────────────────
    let pending_assignments: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_assignments WHERE assignment_overlay_digest = 'pending'",
    )
    .fetch_all(pool)
    .await?;

    for assignment_id in &pending_assignments {
        let mut tx = pool.begin().await?;
        write_assignment_effective_set_digest(&mut tx, *assignment_id).await?;
        tx.commit().await?;
    }

    // ── Verify no pending rows remain ─────────────────────────────────────────
    let remaining_policy: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deployment_policy_versions WHERE semantic_digest = 'pending'",
    )
    .fetch_one(pool)
    .await?;

    let remaining_bundle: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_versions WHERE semantic_digest = 'pending'",
    )
    .fetch_one(pool)
    .await?;

    let remaining_mapping: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deployment_policy_versions WHERE mapping_digest = 'pending'",
    )
    .fetch_one(pool)
    .await?;
    let remaining_requirement: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_versions WHERE requirement_digest = 'pending'",
    )
    .fetch_one(pool)
    .await?;

    let remaining_assignment: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE assignment_overlay_digest = 'pending'",
    )
    .fetch_one(pool)
    .await?;

    if remaining_policy > 0
        || remaining_bundle > 0
        || remaining_assignment > 0
        || remaining_mapping > 0
        || remaining_requirement > 0
    {
        anyhow::bail!(
            "compliance: {remaining_policy} policy semantic, {remaining_mapping} policy mapping, \
             {remaining_bundle} bundle semantic, {remaining_requirement} bundle requirement, and \
             {remaining_assignment} assignment digest(s) still pending after backfill"
        );
    }

    tracing::info!(
        policies = pending_policies.len(),
        bundles = pending_bundles.len(),
        mapping_components = pending_mapping_ids.len(),
        requirement_components = pending_requirement_ids.len(),
        assignments = pending_assignments.len(),
        "compliance: digest backfill complete"
    );
    Ok(())
}
