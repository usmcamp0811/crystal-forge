//! Database queries for compliance frameworks, framework versions,
//! requirement lineages, requirement versions, policy-requirement mappings,
//! and bundle requirement membership.
//!
//! # Design notes
//!
//! - All semantic digests follow the `'pending'` sentinel + Rust-computes
//!   pattern from [`crate::compliance::digest`].  Every INSERT of a versioned
//!   entity immediately calls `write_*_digest()` within the same transaction.
//!
//! - Advisory locks use the sorted-and-deduped
//!   `pg_advisory_xact_lock(hashtextextended($1, 0))` pattern from the
//!   compliance interchange module to prevent concurrent identity races.
//!
//! - Mapping mutations are permitted only for draft (non-accepted/deprecated)
//!   policy versions; the trigger in migration 0213 enforces this at the DB
//!   layer, but we also check at the application layer for clear error messages.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::compliance::digest::refresh_policy_version_digest;
use crate::compliance::framework_model::{
    FrameworkVersionCanonical, write_framework_version_digest,
};
use crate::compliance::requirement_model::{
    FrameworkReconciliation, FrameworkReconciliationState, PolicyCandidate,
    PolicyCandidateMatchType, RelatedRequirementIdentifiers, RequirementReconciliation,
    RequirementReconciliationPreview, RequirementReconciliationState, RequirementVersionCanonical,
    write_requirement_version_digest,
};
use crate::compliance::xccdf::disa_stig_adapter::DisaStigFrameworkIdentity;

// ── API-facing DTOs ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkSummary {
    pub id: Uuid,
    pub name: String,
    pub publisher: Option<String>,
    pub canonical_source_key: String,
    pub description: Option<String>,
    pub version_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkVersionSummary {
    pub id: Uuid,
    pub framework_id: Uuid,
    pub version: String,
    pub canonical_release_key: String,
    pub title: Option<String>,
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub semantic_digest: String,
    pub requirement_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementVersionSummary {
    pub id: Uuid,
    pub requirement_id: Uuid,
    pub framework_version_id: Uuid,
    pub external_id: String,
    pub title: Option<String>,
    pub kind: String,
    pub severity: Option<String>,
    pub parent_requirement_version_id: Option<Uuid>,
    pub semantic_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMappingRow {
    pub id: Uuid,
    pub policy_version_id: Uuid,
    pub requirement_version_id: Uuid,
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
    pub provenance: String,
    pub trust_state: String,
    pub framework_id: Uuid,
    pub framework_name: String,
    pub framework_version_id: Uuid,
    pub framework_version: String,
    pub requirement_external_id: String,
    pub requirement_title: Option<String>,
}

/// Coverage result for a single requirement within a bundle version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementCoverage {
    Full,
    Partial,
    Unmapped,
}

/// Aggregated coverage counts for a bundle version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleCoverageReport {
    pub bundle_version_id: Uuid,
    pub total_requirements: i64,
    pub full: i64,
    pub partial: i64,
    pub unmapped: i64,
    pub rows: Vec<BundleCoverageRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleCoverageRow {
    pub requirement_version_id: Uuid,
    pub external_id: String,
    pub title: Option<String>,
    pub kind: String,
    pub parent_requirement_version_id: Option<Uuid>,
    pub coverage: RequirementCoverage,
    /// Policy version IDs mapped to this requirement that are also in the bundle.
    pub mapped_policy_version_ids: Vec<Uuid>,
}

// ── Framework CRUD ────────────────────────────────────────────────────────────

/// List all framework lineages with version counts.
pub async fn list_frameworks(pool: &PgPool) -> Result<Vec<FrameworkSummary>> {
    sqlx::query_as!(
        FrameworkSummary,
        r#"
        SELECT
            f.id,
            f.name,
            f.publisher,
            f.canonical_source_key,
            f.description,
            COUNT(fv.id) AS "version_count!"
        FROM compliance_frameworks f
        LEFT JOIN compliance_framework_versions fv ON fv.framework_id = f.id
        GROUP BY f.id
        ORDER BY f.name
        "#
    )
    .fetch_all(pool)
    .await
    .context("failed to list compliance frameworks")
}

/// List versions for a specific framework.
pub async fn list_framework_versions(
    pool: &PgPool,
    framework_id: Uuid,
) -> Result<Vec<FrameworkVersionSummary>> {
    sqlx::query_as!(
        FrameworkVersionSummary,
        r#"
        SELECT
            fv.id,
            fv.framework_id,
            fv.version,
            fv.canonical_release_key,
            fv.title,
            fv.published_at,
            fv.semantic_digest,
            COUNT(rv.id) AS "requirement_count!"
        FROM compliance_framework_versions fv
        LEFT JOIN compliance_requirement_versions rv ON rv.framework_version_id = fv.id
        WHERE fv.framework_id = $1
        GROUP BY fv.id
        ORDER BY fv.created_at DESC
        "#,
        framework_id
    )
    .fetch_all(pool)
    .await
    .context("failed to list framework versions")
}

/// Return the exact policy-version IDs with at least one normalized mapping to
/// any release of `framework_id`.  Bundle editors use this compact projection
/// to split a policy picker into framework-mapped and custom additions without
/// issuing one mapping request per policy.
pub async fn list_framework_mapped_policy_versions(
    pool: &PgPool,
    framework_id: Uuid,
) -> Result<Vec<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT DISTINCT m.policy_version_id
        FROM policy_requirement_mappings m
        JOIN compliance_requirement_versions rv ON rv.id = m.requirement_version_id
        JOIN compliance_framework_versions fv ON fv.id = rv.framework_version_id
        WHERE fv.framework_id = $1
        ORDER BY m.policy_version_id
        "#,
    )
    .bind(framework_id)
    .fetch_all(pool)
    .await
    .context("failed to list mapped policy versions for framework")
}

// ── Requirement search ────────────────────────────────────────────────────────

/// Server-side paginated requirement search within a framework version.
///
/// `q` is matched against `external_id`, `title`, CCI IDs, and SRG IDs via
/// full-text search (pg_tsvector) and substring matching.
///
/// Returns at most `limit` rows (capped at 50).  The cursor is an opaque
/// base64-encoded offset for stability.
pub async fn search_requirements(
    pool: &PgPool,
    framework_version_id: Uuid,
    q: Option<&str>,
    kind_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<RequirementVersionSummary>> {
    let limit = limit.min(50).max(1);
    let q_param = q.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    if let Some(query) = q_param {
        sqlx::query_as!(
            RequirementVersionSummary,
            r#"
            SELECT
                rv.id,
                rv.requirement_id,
                rv.framework_version_id,
                rv.external_id,
                rv.title,
                rv.kind,
                rv.severity,
                rv.parent_requirement_version_id,
                rv.semantic_digest
            FROM compliance_requirement_versions rv
            WHERE rv.framework_version_id = $1
              AND ($2::TEXT IS NULL OR rv.kind = $2)
              AND (
                  to_tsvector('english',
                      COALESCE(rv.external_id, '') || ' ' || COALESCE(rv.title, ''))
                      @@ plainto_tsquery('english', $3)
                  OR rv.external_id ILIKE '%' || $3 || '%'
                  OR rv.title ILIKE '%' || $3 || '%'
                  OR rv.metadata->>'cci_ids' ILIKE '%' || $3 || '%'
                  OR rv.metadata->>'srg_ids' ILIKE '%' || $3 || '%'
              )
            ORDER BY rv.external_id
            LIMIT $4 OFFSET $5
            "#,
            framework_version_id,
            kind_filter,
            query,
            limit,
            offset
        )
        .fetch_all(pool)
        .await
        .context("failed to search requirements")
    } else {
        sqlx::query_as!(
            RequirementVersionSummary,
            r#"
            SELECT
                rv.id,
                rv.requirement_id,
                rv.framework_version_id,
                rv.external_id,
                rv.title,
                rv.kind,
                rv.severity,
                rv.parent_requirement_version_id,
                rv.semantic_digest
            FROM compliance_requirement_versions rv
            WHERE rv.framework_version_id = $1
              AND ($2::TEXT IS NULL OR rv.kind = $2)
            ORDER BY rv.external_id
            LIMIT $3 OFFSET $4
            "#,
            framework_version_id,
            kind_filter,
            limit,
            offset
        )
        .fetch_all(pool)
        .await
        .context("failed to list requirements")
    }
}

/// Fetch the children of a requirement version (direct descendants in hierarchy).
pub async fn list_requirement_children(
    pool: &PgPool,
    parent_id: Uuid,
) -> Result<Vec<RequirementVersionSummary>> {
    sqlx::query_as!(
        RequirementVersionSummary,
        r#"
        SELECT
            rv.id,
            rv.requirement_id,
            rv.framework_version_id,
            rv.external_id,
            rv.title,
            rv.kind,
            rv.severity,
            rv.parent_requirement_version_id,
            rv.semantic_digest
        FROM compliance_requirement_versions rv
        WHERE rv.parent_requirement_version_id = $1
        ORDER BY rv.external_id
        "#,
        parent_id
    )
    .fetch_all(pool)
    .await
    .context("failed to list requirement children")
}

// ── Policy-requirement mapping CRUD ──────────────────────────────────────────

/// List all requirement mappings for a policy version, joined with framework
/// and requirement version data for the UI.
pub async fn list_policy_mappings(
    pool: &PgPool,
    policy_version_id: Uuid,
) -> Result<Vec<PolicyMappingRow>> {
    sqlx::query_as!(
        PolicyMappingRow,
        r#"
        SELECT
            m.id,
            m.policy_version_id,
            m.requirement_version_id,
            m.relationship,
            m.coverage,
            m.rationale,
            m.provenance,
            m.trust_state,
            f.id   AS "framework_id!",
            f.name AS "framework_name!",
            fv.id  AS "framework_version_id!",
            fv.version AS "framework_version!",
            rv.external_id AS "requirement_external_id!",
            rv.title AS requirement_title
        FROM policy_requirement_mappings m
        JOIN compliance_requirement_versions rv ON rv.id = m.requirement_version_id
        JOIN compliance_framework_versions fv ON fv.id = rv.framework_version_id
        JOIN compliance_frameworks f ON f.id = fv.framework_id
        WHERE m.policy_version_id = $1
        ORDER BY f.name, rv.external_id
        "#,
        policy_version_id
    )
    .fetch_all(pool)
    .await
    .context("failed to list policy requirement mappings")
}

/// Create a requirement mapping on a mutable (draft) policy version.
///
/// Returns the new mapping ID on success.
/// Fails with a descriptive error if the policy version is accepted/deprecated.
pub async fn create_policy_mapping(
    pool: &PgPool,
    policy_version_id: Uuid,
    requirement_version_id: Uuid,
    relationship: &str,
    coverage: &str,
    rationale: Option<&str>,
    provenance: &str,
    created_by: Uuid,
) -> Result<Uuid> {
    // Check mutability at the application layer for a clear error message.
    let pub_state: Option<String> = sqlx::query_scalar(
        "SELECT publication_state FROM deployment_policy_versions WHERE id = $1",
    )
    .bind(policy_version_id)
    .fetch_optional(pool)
    .await
    .context("failed to check policy version state")?;

    match pub_state.as_deref() {
        None => {
            bail!("POLICY_VERSION_NOT_FOUND: policy version {policy_version_id} does not exist")
        }
        Some("accepted") | Some("deprecated") => bail!(
            "POLICY_MAPPING_IMMUTABLE: cannot modify mappings on policy version {} \
             because it is in an immutable state. Create a derived draft first.",
            policy_version_id
        ),
        _ => {}
    }

    let mut tx = pool
        .begin()
        .await
        .context("failed to begin mapping transaction")?;
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO policy_requirement_mappings
            (policy_version_id, requirement_version_id, relationship, coverage,
             rationale, provenance, trust_state, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, 'trusted', $7)
        RETURNING id
        "#,
    )
    .bind(policy_version_id)
    .bind(requirement_version_id)
    .bind(relationship)
    .bind(coverage)
    .bind(rationale)
    .bind(provenance)
    .bind(created_by)
    .fetch_one(&mut *tx)
    .await
    .context("failed to insert policy requirement mapping")?;

    refresh_policy_version_digest(&mut tx, policy_version_id).await?;
    tx.commit()
        .await
        .context("failed to commit mapping transaction")?;

    Ok(id)
}

/// Update relationship/coverage/rationale on an existing mapping.
/// Fails if the policy version is accepted/deprecated.
pub async fn update_policy_mapping(
    pool: &PgPool,
    mapping_id: Uuid,
    relationship: &str,
    coverage: &str,
    rationale: Option<&str>,
) -> Result<()> {
    let policy_version_id: Uuid = sqlx::query_scalar(
        "SELECT policy_version_id FROM policy_requirement_mappings WHERE id = $1",
    )
    .bind(mapping_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        anyhow::anyhow!("POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND: mapping {mapping_id} was not found")
    })?;
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin mapping transaction")?;
    let affected = sqlx::query(
        r#"
        UPDATE policy_requirement_mappings m
        SET relationship = $2, coverage = $3, rationale = $4
        FROM deployment_policy_versions pv
        WHERE m.id = $1
          AND pv.id = m.policy_version_id
          AND pv.publication_state NOT IN ('accepted', 'deprecated')
        "#,
    )
    .bind(mapping_id)
    .bind(relationship)
    .bind(coverage)
    .bind(rationale)
    .execute(&mut *tx)
    .await
    .context("failed to update policy requirement mapping")?;

    if affected.rows_affected() == 0 {
        bail!(
            "POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND: mapping {mapping_id} was not found \
             or belongs to an immutable policy version"
        );
    }
    refresh_policy_version_digest(&mut tx, policy_version_id).await?;
    tx.commit()
        .await
        .context("failed to commit mapping transaction")?;
    Ok(())
}

/// Delete a requirement mapping.
/// Fails if the policy version is accepted/deprecated.
pub async fn delete_policy_mapping(pool: &PgPool, mapping_id: Uuid) -> Result<()> {
    let policy_version_id: Uuid = sqlx::query_scalar(
        "SELECT policy_version_id FROM policy_requirement_mappings WHERE id = $1",
    )
    .bind(mapping_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        anyhow::anyhow!("POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND: mapping {mapping_id} was not found")
    })?;
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin mapping transaction")?;
    let affected = sqlx::query(
        r#"
        DELETE FROM policy_requirement_mappings m
        USING deployment_policy_versions pv
        WHERE m.id = $1
          AND pv.id = m.policy_version_id
          AND pv.publication_state NOT IN ('accepted', 'deprecated')
        "#,
    )
    .bind(mapping_id)
    .execute(&mut *tx)
    .await
    .context("failed to delete policy requirement mapping")?;

    if affected.rows_affected() == 0 {
        bail!(
            "POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND: mapping {mapping_id} was not found \
             or belongs to an immutable policy version"
        );
    }
    refresh_policy_version_digest(&mut tx, policy_version_id).await?;
    tx.commit()
        .await
        .context("failed to commit mapping transaction")?;
    Ok(())
}

// ── Bundle requirement coverage ───────────────────────────────────────────────

/// Compute authoritative requirement coverage for a bundle version.
///
/// Coverage rules:
/// - `full`:    at least one mapping with `relationship='implements'` AND
///              `coverage='full'` AND `trust_state='trusted'`.
/// - `partial`: at least one trusted mapping without `full`+`implements`.
/// - `unmapped`: no trusted mapping.
///
/// Only requirements with `selected=true` in the bundle's requirement baseline
/// are included.  Coverage is computed from:
///   bundle requirement membership × bundle policy membership × policy mappings.
pub async fn compute_bundle_requirement_coverage(
    pool: &PgPool,
    bundle_version_id: Uuid,
) -> Result<BundleCoverageReport> {
    // Single query: join bundle requirements → bundle policies → mappings.
    struct RawRow {
        requirement_version_id: Uuid,
        external_id: String,
        title: Option<String>,
        kind: String,
        parent_requirement_version_id: Option<Uuid>,
        has_full_coverage: bool,
        has_partial_coverage: bool,
        mapped_policy_version_ids: Vec<Uuid>,
    }

    // We need a macro-compatible query.  Use query! with anonymous struct.
    let rows = sqlx::query!(
        r#"
        WITH bundle_reqs AS (
            -- All selected requirements in the bundle version
            SELECT bvr.requirement_version_id
            FROM compliance_bundle_version_requirements bvr
            WHERE bvr.bundle_version_id = $1
              AND bvr.selected = true
        ),
        bundle_policies AS (
            -- All policy version IDs in the bundle
            SELECT bvp.policy_version_id
            FROM compliance_bundle_version_policies bvp
            WHERE bvp.bundle_version_id = $1
              AND bvp.selected = true
        ),
        applicable_mappings AS (
            -- Trusted mappings where both the policy and requirement are in this bundle
            SELECT
                m.requirement_version_id,
                m.policy_version_id,
                m.relationship,
                m.coverage
            FROM policy_requirement_mappings m
            WHERE m.trust_state = 'trusted'
              AND m.requirement_version_id IN (SELECT requirement_version_id FROM bundle_reqs)
              AND m.policy_version_id      IN (SELECT policy_version_id       FROM bundle_policies)
        )
        SELECT
            rv.id              AS "requirement_version_id!",
            rv.external_id     AS "external_id!",
            rv.title,
            rv.kind            AS "kind!",
            rv.parent_requirement_version_id,
            COALESCE(
                BOOL_OR(
                    am.relationship = 'implements' AND am.coverage = 'full'
                ), false
            )                  AS "has_full_coverage!: bool",
            COALESCE(
                BOOL_OR(am.requirement_version_id IS NOT NULL),
                false
            )                  AS "has_partial_coverage!: bool",
            COALESCE(
                ARRAY_AGG(am.policy_version_id) FILTER (WHERE am.policy_version_id IS NOT NULL),
                ARRAY[]::uuid[]
            )                  AS "mapped_policy_version_ids!: Vec<Uuid>"
        FROM bundle_reqs br
        JOIN compliance_requirement_versions rv ON rv.id = br.requirement_version_id
        LEFT JOIN applicable_mappings am ON am.requirement_version_id = br.requirement_version_id
        GROUP BY rv.id, rv.external_id, rv.title, rv.kind, rv.parent_requirement_version_id
        ORDER BY rv.external_id
        "#,
        bundle_version_id
    )
    .fetch_all(pool)
    .await
    .context("failed to compute bundle requirement coverage")?;

    let mut full_count = 0i64;
    let mut partial_count = 0i64;
    let mut unmapped_count = 0i64;
    let total = rows.len() as i64;

    let coverage_rows: Vec<BundleCoverageRow> = rows
        .into_iter()
        .map(|r| {
            let coverage = if r.has_full_coverage {
                full_count += 1;
                RequirementCoverage::Full
            } else if r.has_partial_coverage {
                partial_count += 1;
                RequirementCoverage::Partial
            } else {
                unmapped_count += 1;
                RequirementCoverage::Unmapped
            };
            BundleCoverageRow {
                requirement_version_id: r.requirement_version_id,
                external_id: r.external_id,
                title: r.title,
                kind: r.kind,
                parent_requirement_version_id: r.parent_requirement_version_id,
                coverage,
                mapped_policy_version_ids: r.mapped_policy_version_ids,
            }
        })
        .collect();

    Ok(BundleCoverageReport {
        bundle_version_id,
        total_requirements: total,
        full: full_count,
        partial: partial_count,
        unmapped: unmapped_count,
        rows: coverage_rows,
    })
}

// ── Reconciliation (preview, mutation-free) ───────────────────────────────────

/// Reconcile an imported STIG against existing framework/requirement state.
///
/// This is a **read-only** operation — no rows are written.  It returns the
/// classification for the framework release and each rule's requirement.
///
/// Called by the XCCDF preview handler to populate the new reconciliation
/// summary step in the import UI.
pub async fn preview_framework_reconciliation(
    pool: &PgPool,
    identity: &DisaStigFrameworkIdentity,
    source_sha256: &str,
) -> Result<FrameworkReconciliation> {
    // 1. Check for exact artifact reuse.
    let artifact_exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM compliance_source_artifacts WHERE sha256 = $1")
            .bind(source_sha256)
            .fetch_optional(pool)
            .await
            .context("failed to check source artifact")?;

    if artifact_exists.is_some() {
        // Check if this exact artifact was already used for a framework version.
        let fv_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM compliance_framework_versions WHERE source_artifact_id = \
             (SELECT id FROM compliance_source_artifacts WHERE sha256 = $1 LIMIT 1)",
        )
        .bind(source_sha256)
        .fetch_optional(pool)
        .await
        .context("failed to check framework version for artifact")?;

        if let Some(existing_fv_id) = fv_id {
            let fw_id: Uuid = sqlx::query_scalar(
                "SELECT framework_id FROM compliance_framework_versions WHERE id = $1",
            )
            .bind(existing_fv_id)
            .fetch_one(pool)
            .await
            .context("failed to get framework_id for version")?;

            return Ok(FrameworkReconciliation {
                state: FrameworkReconciliationState::ExactArtifact,
                canonical_source_key: identity.canonical_source_key.clone(),
                canonical_release_key: identity.canonical_release_key.clone(),
                existing_framework_id: Some(fw_id),
                existing_framework_version_id: Some(existing_fv_id),
            });
        }
    }

    // 2. Look up the framework lineage.
    let existing_framework_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM compliance_frameworks WHERE canonical_source_key = $1")
            .bind(&identity.canonical_source_key)
            .fetch_optional(pool)
            .await
            .context("failed to look up framework by canonical key")?;

    let Some(fw_id) = existing_framework_id else {
        return Ok(FrameworkReconciliation {
            state: FrameworkReconciliationState::NewFramework,
            canonical_source_key: identity.canonical_source_key.clone(),
            canonical_release_key: identity.canonical_release_key.clone(),
            existing_framework_id: None,
            existing_framework_version_id: None,
        });
    };

    // 3. Look up the framework version by release key.
    let existing_fv: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, semantic_digest \
         FROM compliance_framework_versions \
         WHERE framework_id = $1 AND canonical_release_key = $2",
    )
    .bind(fw_id)
    .bind(&identity.canonical_release_key)
    .fetch_optional(pool)
    .await
    .context("failed to look up framework version by release key")?;

    match existing_fv {
        None => Ok(FrameworkReconciliation {
            state: FrameworkReconciliationState::NewRelease,
            canonical_source_key: identity.canonical_source_key.clone(),
            canonical_release_key: identity.canonical_release_key.clone(),
            existing_framework_id: Some(fw_id),
            existing_framework_version_id: None,
        }),
        Some((fv_id, _existing_digest)) => {
            // Release key exists.  Check semantic content.
            // For now: treat any existing release with the same key as
            // ExistingRelease.  A future enhancement computes the proposed
            // digest and compares to detect ReleaseConflict.
            Ok(FrameworkReconciliation {
                state: FrameworkReconciliationState::ExistingRelease,
                canonical_source_key: identity.canonical_source_key.clone(),
                canonical_release_key: identity.canonical_release_key.clone(),
                existing_framework_id: Some(fw_id),
                existing_framework_version_id: Some(fv_id),
            })
        }
    }
}

/// Classify each rule's requirement against existing DB state (batch query).
///
/// Returns one `RequirementReconciliation` per rule, in input order.
/// This is mutation-free — no rows are written.
pub async fn preview_requirement_reconciliation(
    pool: &PgPool,
    framework_id: Uuid,
    framework_version_id: Option<Uuid>,
    proposed: &[RequirementVersionCanonical],
) -> Result<RequirementReconciliationPreview> {
    if proposed.is_empty() {
        return Ok(RequirementReconciliationPreview {
            requirements: vec![],
            removed_requirements: vec![],
        });
    }

    let canonical_keys: Vec<&str> = proposed
        .iter()
        .map(|requirement| requirement.canonical_requirement_key.as_str())
        .collect();

    // Batch: fetch existing requirement lineages for this framework.
    let existing_lineages: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT canonical_requirement_key, id \
         FROM compliance_requirements \
         WHERE framework_id = $1 \
           AND canonical_requirement_key = ANY($2)",
    )
    .bind(framework_id)
    .bind(&canonical_keys)
    .fetch_all(pool)
    .await
    .context("failed to batch-query existing requirement lineages")?;

    use std::collections::HashMap;
    let lineage_map: HashMap<String, Uuid> = existing_lineages.into_iter().collect();

    // Batch: fetch existing requirement versions for the previous framework version
    // (to detect changes).
    let comparison_fv_id = match framework_version_id {
        Some(existing_fv_id) => Some(existing_fv_id),
        None => sqlx::query_scalar(
            "SELECT id FROM compliance_framework_versions \
              WHERE framework_id = $1 \
              ORDER BY created_at DESC LIMIT 1",
        )
        .bind(framework_id)
        .fetch_optional(pool)
        .await
        .context("failed to find previous framework version")?,
    };

    let prev_versions: HashMap<Uuid, (Uuid, String, String)> =
        if let Some(comparison_fv_id) = comparison_fv_id {
            let rows: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
                "SELECT rv.requirement_id, rv.id, rv.semantic_digest \
                       , rv.external_id \
                 FROM compliance_requirement_versions rv \
                 WHERE rv.framework_version_id = $1",
            )
            .bind(comparison_fv_id)
            .fetch_all(pool)
            .await
            .context("failed to fetch previous requirement versions")?;
            rows.into_iter()
                .map(|(req_id, rv_id, digest, external_id)| (req_id, (rv_id, digest, external_id)))
                .collect()
        } else {
            HashMap::new()
        };

    // Build per-rule reconciliation.
    let mut results = Vec::with_capacity(proposed.len());
    for canonical in proposed {
        let key = &canonical.canonical_requirement_key;
        let rec = if let Some(&req_id) = lineage_map.get(key) {
            // Check if this requirement has a version in the previous release.
            match prev_versions.get(&req_id) {
                Some((prev_rv_id, prev_digest, _)) => RequirementReconciliation {
                    canonical_requirement_key: key.clone(),
                    external_id: canonical.external_id.clone(),
                    state: if canonical.compute_digest() == *prev_digest {
                        RequirementReconciliationState::ExistingUnchanged
                    } else {
                        RequirementReconciliationState::ExistingChanged
                    },
                    existing_requirement_id: Some(req_id),
                    existing_requirement_version_id: Some(*prev_rv_id),
                    existing_digest: Some(prev_digest.clone()),
                },
                None => RequirementReconciliation {
                    canonical_requirement_key: key.clone(),
                    external_id: canonical.external_id.clone(),
                    state: RequirementReconciliationState::NewRequirement,
                    existing_requirement_id: Some(req_id),
                    existing_requirement_version_id: None,
                    existing_digest: None,
                },
            }
        } else {
            RequirementReconciliation {
                canonical_requirement_key: key.clone(),
                external_id: canonical.external_id.clone(),
                state: RequirementReconciliationState::NewRequirement,
                existing_requirement_id: None,
                existing_requirement_version_id: None,
                existing_digest: None,
            }
        };
        results.push(rec);
    }

    let proposed_requirement_ids: std::collections::HashSet<Uuid> = results
        .iter()
        .filter_map(|requirement| requirement.existing_requirement_id)
        .collect();
    let removed_requirements = prev_versions
        .into_iter()
        .filter_map(
            |(requirement_id, (requirement_version_id, digest, external_id))| {
                (!proposed_requirement_ids.contains(&requirement_id)).then_some(
                    RequirementReconciliation {
                        canonical_requirement_key: external_id.clone(),
                        external_id,
                        state: RequirementReconciliationState::RemovedFromRelease,
                        existing_requirement_id: Some(requirement_id),
                        existing_requirement_version_id: Some(requirement_version_id),
                        existing_digest: Some(digest),
                    },
                )
            },
        )
        .collect();
    Ok(RequirementReconciliationPreview {
        requirements: results,
        removed_requirements,
    })
}

/// Find policy candidates for a requirement (for the reconciliation preview).
///
/// Searches in priority order:
/// 1. Authoritative mapping already exists for this requirement.
/// 2. Inherited mapping from an unchanged previous requirement version.
/// 3. Exact normalized technical enforcement match against policy config.
///
/// Returns candidates with `match_type` and `confidence` for UI display,
/// deduplicated by policy version (highest confidence retained).
pub async fn find_policy_candidates(
    pool: &PgPool,
    authoritative_requirement_version_id: Option<Uuid>,
    inherited_requirement_version_id: Option<Uuid>,
    fix_text: Option<&str>,
    related_identifiers: &RelatedRequirementIdentifiers,
    incoming_framework_id: Option<Uuid>,
) -> Result<Vec<PolicyCandidate>> {
    use crate::compliance::xccdf::exact_technical_match::{
        RequirementTechnicalIdentity, find_exact_technical_match_candidates,
    };
    use std::collections::HashMap;

    let mut candidates: HashMap<Uuid, PolicyCandidate> = HashMap::new();

    // 1. Authoritative mappings: highest confidence.
    if let Some(requirement_version_id) = authoritative_requirement_version_id {
        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            r#"
            SELECT DISTINCT dp.id, pv.id, pv.name
            FROM policy_requirement_mappings m
            JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id
            JOIN deployment_policies dp ON dp.id = pv.policy_id
            WHERE m.requirement_version_id = $1
              AND m.trust_state = 'trusted'
              AND pv.publication_state = 'accepted'
              AND dp.current_published_version_id = pv.id
            ORDER BY pv.name
            "#,
        )
        .bind(requirement_version_id)
        .fetch_all(pool)
        .await
        .context("failed to find authoritative policy candidates")?;

        for (policy_id, policy_version_id, policy_name) in rows {
            candidates.insert(
                policy_version_id,
                PolicyCandidate {
                    policy_id,
                    policy_version_id,
                    policy_name,
                    match_type: PolicyCandidateMatchType::AuthoritativeMapping,
                    confidence: 100,
                    match_reasons: vec![
                        "Authoritative policy-requirement mapping exists.".to_string(),
                    ],
                    related_evidence: None,
                },
            );
        }
    }

    // 2. Inherited mappings: second-highest confidence.
    // Only add if not already present from authoritative search.
    if let Some(requirement_version_id) = inherited_requirement_version_id {
        let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
            r#"
            SELECT DISTINCT dp.id, pv.id, pv.name
            FROM policy_requirement_mappings m
            JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id
            JOIN deployment_policies dp ON dp.id = pv.policy_id
            WHERE m.requirement_version_id = $1
              AND m.trust_state = 'trusted'
              AND pv.publication_state = 'accepted'
              AND dp.current_published_version_id = pv.id
            ORDER BY pv.name
            "#,
        )
        .bind(requirement_version_id)
        .fetch_all(pool)
        .await
        .context("failed to find inherited policy candidates")?;

        for (policy_id, policy_version_id, policy_name) in rows {
            candidates
                .entry(policy_version_id)
                .or_insert_with(|| PolicyCandidate {
                    policy_id,
                    policy_version_id,
                    policy_name,
                    match_type: PolicyCandidateMatchType::InheritedMapping,
                    confidence: 95,
                    match_reasons: vec![
                        "Trusted mapping on an unchanged requirement in the prior release."
                            .to_string(),
                    ],
                    related_evidence: None,
                });
        }
    }

    // 3. Exact technical enforcement match: lowest of these three priorities.
    // Only add if not already present from mapping searches.
    if let Some(fix_text_content) = fix_text {
        let technical_identity = RequirementTechnicalIdentity::from_fix_text(fix_text_content);

        if !technical_identity.enforced_options.is_empty() {
            let tech_matches = find_exact_technical_match_candidates(pool, &technical_identity)
                .await
                .context("failed to find exact technical match candidates")?;

            for tech_match in tech_matches {
                // Only add if this policy version wasn't already found via mapping.
                if !candidates.contains_key(&tech_match.policy_version_id) {
                    let description = technical_identity.description();
                    candidates.insert(
                        tech_match.policy_version_id,
                        PolicyCandidate {
                            policy_id: tech_match.policy_id,
                            policy_version_id: tech_match.policy_version_id,
                            policy_name: tech_match.policy_name,
                            match_type: PolicyCandidateMatchType::ExactTechnicalMatch,
                            confidence: 90,
                            match_reasons: vec![format!(
                                "Exact normalized enforcement match: {}",
                                description
                            )],
                            related_evidence: None,
                        },
                    );
                }
            }
        }
    }

    // 4. Related mappings: review-only evidence from exact shared CCI/SRG
    // identifiers. Restrict this to trusted mappings on the current accepted
    // policy version; stale or suggested evidence must never be promoted.
    if !related_identifiers.is_empty() {
        let rows: Vec<(
            Uuid,
            Uuid,
            String,
            Uuid,
            Uuid,
            String,
            String,
            String,
            Value,
        )> = sqlx::query_as(
            r#"
                SELECT DISTINCT
                    dp.id,
                    pv.id,
                    pv.name,
                    rv.id,
                    f.id,
                    COALESCE(f.name, 'Unknown framework'),
                    rv.external_id,
                    COALESCE(rv.title, rv.external_id),
                    rv.metadata
                FROM policy_requirement_mappings m
                JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id
                JOIN deployment_policies dp ON dp.id = pv.policy_id
                JOIN compliance_requirement_versions rv ON rv.id = m.requirement_version_id
                JOIN compliance_requirements r ON r.id = rv.requirement_id
                JOIN compliance_frameworks f ON f.id = r.framework_id
                WHERE m.trust_state = 'trusted'
                  AND pv.publication_state = 'accepted'
                  AND dp.current_published_version_id = pv.id
                  AND ($1::uuid IS NULL OR f.id <> $1)

                ORDER BY pv.name, rv.external_id
                "#,
        )
        .bind(incoming_framework_id)
        .fetch_all(pool)
        .await
        .context("failed to find related policy candidates")?;

        for (
            policy_id,
            policy_version_id,
            policy_name,
            related_requirement_version_id,
            framework_id,
            framework_name,
            external_id,
            title,
            metadata,
        ) in rows
        {
            if candidates.contains_key(&policy_version_id) {
                continue;
            }
            let candidate_ids = RelatedRequirementIdentifiers::from_metadata(&metadata);
            let shared_cci = related_identifiers
                .cci_ids
                .intersection(&candidate_ids.cci_ids)
                .cloned()
                .collect::<Vec<_>>();
            let shared_srg = related_identifiers
                .srg_ids
                .intersection(&candidate_ids.srg_ids)
                .cloned()
                .collect::<Vec<_>>();
            if shared_cci.is_empty() && shared_srg.is_empty() {
                continue;
            }
            let mut reasons = shared_cci
                .into_iter()
                .map(|id| format!("Shared {id}"))
                .collect::<Vec<_>>();
            reasons.extend(shared_srg.into_iter().map(|id| format!("Shared {id}")));
            reasons.push(format!(
                "Existing mapping: {framework_name} {external_id} ({title})"
            ));
            candidates.insert(
                policy_version_id,
                PolicyCandidate {
                    policy_id,
                    policy_version_id,
                    policy_name,
                    match_type: PolicyCandidateMatchType::RelatedMapping,
                    confidence: 70,
                    match_reasons: reasons,
                    related_evidence: Some(
                        crate::compliance::requirement_model::RelatedCandidateEvidence {
                            shared_cci_ids: related_identifiers
                                .cci_ids
                                .intersection(&candidate_ids.cci_ids)
                                .cloned()
                                .collect(),
                            shared_srg_ids: related_identifiers
                                .srg_ids
                                .intersection(&candidate_ids.srg_ids)
                                .cloned()
                                .collect(),
                            related_requirement_version_id,
                            related_framework_id: framework_id,
                            related_framework_name: framework_name,
                            related_external_id: external_id,
                        },
                    ),
                },
            );
        }
    }

    // Sort candidates by confidence descending, then by name for stable ordering.
    let mut result: Vec<PolicyCandidate> = candidates.into_values().collect();
    result.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| a.policy_name.cmp(&b.policy_name))
    });

    Ok(result)
}

// ── Atomic commit helpers ─────────────────────────────────────────────────────

/// Upsert a framework lineage within an open transaction.
///
/// If the `canonical_source_key` already exists, returns the existing ID.
/// Otherwise inserts a new row.
pub async fn upsert_framework_lineage(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
    publisher: Option<&str>,
    canonical_source_key: &str,
    description: Option<&str>,
) -> Result<Uuid> {
    // Advisory lock on the canonical key to prevent concurrent race.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("compliance-framework:{canonical_source_key}"))
        .execute(&mut **tx)
        .await
        .context("failed to acquire framework lineage lock")?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_frameworks
            (name, publisher, canonical_source_key, description)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (canonical_source_key) DO UPDATE
            SET name = EXCLUDED.name
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(publisher)
    .bind(canonical_source_key)
    .bind(description)
    .fetch_one(&mut **tx)
    .await
    .context("failed to upsert framework lineage")?;

    Ok(id)
}

/// Insert a new framework version within an open transaction.
///
/// Returns the new version ID and writes the semantic digest.
/// Fails with `FRAMEWORK_RELEASE_CONFLICT` if the release key already exists.
pub async fn insert_framework_version(
    tx: &mut Transaction<'_, Postgres>,
    framework_id: Uuid,
    canonical: &FrameworkVersionCanonical,
    source_artifact_id: Option<Uuid>,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Uuid> {
    // Check for conflict.
    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_framework_versions \
         WHERE framework_id = $1 AND canonical_release_key = $2",
    )
    .bind(framework_id)
    .bind(&canonical.canonical_release_key)
    .fetch_optional(&mut **tx)
    .await
    .context("failed to check for existing framework version")?;

    if let Some(existing_id) = existing {
        bail!(
            "FRAMEWORK_RELEASE_CONFLICT: framework version with release key '{}' already exists \
             for framework {} (existing ID: {}). \
             Importing a different artifact for the same release key is not permitted.",
            canonical.canonical_release_key,
            framework_id,
            existing_id
        );
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_framework_versions
            (framework_id, version, canonical_release_key, title,
             published_at, source_artifact_id, semantic_digest)
        VALUES ($1, $2, $3, $4, $5, $6, 'pending')
        RETURNING id
        "#,
    )
    .bind(framework_id)
    .bind(&canonical.version)
    .bind(&canonical.canonical_release_key)
    .bind(canonical.title.as_deref())
    .bind(published_at)
    .bind(source_artifact_id)
    .fetch_one(&mut **tx)
    .await
    .context("failed to insert framework version")?;

    write_framework_version_digest(tx, id, canonical)
        .await
        .context("failed to write framework version digest")?;

    Ok(id)
}

/// Upsert a requirement lineage within an open transaction.
pub async fn upsert_requirement_lineage(
    tx: &mut Transaction<'_, Postgres>,
    framework_id: Uuid,
    canonical_requirement_key: &str,
) -> Result<Uuid> {
    // Advisory lock on (framework_id, canonical_requirement_key).
    let lock_key = format!("compliance-requirement:{framework_id}:{canonical_requirement_key}");
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut **tx)
        .await
        .context("failed to acquire requirement lineage lock")?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_requirements (framework_id, canonical_requirement_key)
        VALUES ($1, $2)
        ON CONFLICT (framework_id, canonical_requirement_key) DO UPDATE
            SET framework_id = EXCLUDED.framework_id
        RETURNING id
        "#,
    )
    .bind(framework_id)
    .bind(canonical_requirement_key)
    .fetch_one(&mut **tx)
    .await
    .context("failed to upsert requirement lineage")?;

    Ok(id)
}

/// Insert a requirement version within an open transaction.
///
/// Returns the new version ID and writes the semantic digest.
pub async fn insert_requirement_version(
    tx: &mut Transaction<'_, Postgres>,
    requirement_id: Uuid,
    framework_version_id: Uuid,
    canonical: &RequirementVersionCanonical,
    parent_requirement_version_id: Option<Uuid>,
) -> Result<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_requirement_versions
            (requirement_id, framework_version_id, external_id, title, description,
             kind, parent_requirement_version_id, severity, check_text, fix_text,
             metadata, semantic_digest)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending')
        ON CONFLICT (requirement_id, framework_version_id) DO UPDATE
            SET external_id  = EXCLUDED.external_id,
                title        = EXCLUDED.title,
                description  = EXCLUDED.description,
                kind         = EXCLUDED.kind,
                severity     = EXCLUDED.severity,
                check_text   = EXCLUDED.check_text,
                fix_text     = EXCLUDED.fix_text,
                metadata     = EXCLUDED.metadata
        RETURNING id
        "#,
    )
    .bind(requirement_id)
    .bind(framework_version_id)
    .bind(&canonical.external_id)
    .bind(canonical.title.as_deref())
    .bind(canonical.description.as_deref())
    .bind(&canonical.kind)
    .bind(parent_requirement_version_id)
    .bind(canonical.severity.as_deref())
    .bind(canonical.check_text.as_deref())
    .bind(canonical.fix_text.as_deref())
    .bind(&canonical.metadata)
    .fetch_one(&mut **tx)
    .await
    .context("failed to upsert requirement version")?;

    write_requirement_version_digest(tx, id, canonical)
        .await
        .context("failed to write requirement version digest")?;

    Ok(id)
}

/// Insert a requirement into a bundle version's requirement baseline.
pub async fn insert_bundle_version_requirement(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
    requirement_version_id: Uuid,
    requirement_order: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO compliance_bundle_version_requirements
            (bundle_version_id, requirement_version_id, selected, requirement_order)
        VALUES ($1, $2, true, $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(bundle_version_id)
    .bind(requirement_version_id)
    .bind(requirement_order)
    .execute(&mut **tx)
    .await
    .context("failed to insert bundle version requirement")?;

    Ok(())
}

/// Insert a policy-requirement mapping within an open transaction.
///
/// Uses ON CONFLICT DO NOTHING so re-importing does not fail on existing mappings.
///
/// # ON CONFLICT semantics (TASK-418 review)
///
/// The unique target is `(policy_version_id, requirement_version_id)`. In the
/// current commit path (`commit_foreign_import`) every mapping target is a
/// freshly created policy version or a freshly derived mutable draft, so the
/// conflict cannot fire within a single import: each rule appears once per
/// import and each draft UUID is unique to its transaction. Across imports the
/// draft is always newly derived, so no existing pair is ever revisited either.
/// DO NOTHING is therefore inert today; it is kept as a defensive guard for
/// future paths that might map onto stable version IDs.  Mappings on an
/// accepted/deprecated policy version are additionally write-protected by the
/// `guard_policy_mapping_immutability` trigger, so a DO UPDATE counterpart
/// would fail loudly there instead of silently overwriting authoritative
/// semantics.
pub async fn insert_policy_mapping_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    policy_version_id: Uuid,
    requirement_version_id: Uuid,
    relationship: &str,
    coverage: &str,
    rationale: Option<&str>,
    provenance: &str,
    source_artifact_id: Option<Uuid>,
    created_by: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO policy_requirement_mappings
            (policy_version_id, requirement_version_id, relationship, coverage,
             rationale, provenance, trust_state, source_artifact_id, created_by)
        VALUES ($1, $2, $3, $4, $5, $6, 'trusted', $7, $8)
        ON CONFLICT (policy_version_id, requirement_version_id) DO NOTHING
        "#,
    )
    .bind(policy_version_id)
    .bind(requirement_version_id)
    .bind(relationship)
    .bind(coverage)
    .bind(rationale)
    .bind(provenance)
    .bind(source_artifact_id)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .context("failed to insert policy requirement mapping in transaction")?;

    refresh_policy_version_digest(tx, policy_version_id).await?;

    Ok(())
}

// ── DB-gated tests ────────────────────────────────────────────────────────────

#[cfg(test)]
pub mod tests {
    use super::*;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn test_pool() -> PgPool {
        PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB-gated tests"),
        )
        .await
        .expect("connect to test DB")
    }

    // ── Framework uniqueness ──────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn framework_lineage_is_idempotent() {
        let pool = test_pool().await;
        let key = format!("test-framework-{}", Uuid::new_v4());

        let mut tx1 = pool.begin().await.unwrap();
        let id1 = upsert_framework_lineage(&mut tx1, "Test FW", None, &key, None)
            .await
            .unwrap();
        tx1.commit().await.unwrap();

        let mut tx2 = pool.begin().await.unwrap();
        let id2 = upsert_framework_lineage(&mut tx2, "Test FW Updated", None, &key, None)
            .await
            .unwrap();
        tx2.commit().await.unwrap();

        assert_eq!(id1, id2, "same canonical key must return same lineage id");
    }

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn framework_version_release_conflict() {
        let pool = test_pool().await;
        let key = format!("test-fw-conflict-{}", Uuid::new_v4());

        let mut tx = pool.begin().await.unwrap();
        let fw_id = upsert_framework_lineage(&mut tx, "Conflict FW", None, &key, None)
            .await
            .unwrap();
        let canonical = FrameworkVersionCanonical {
            canonical_source_key: key.clone(),
            canonical_release_key: "V1R1".to_string(),
            version: "V1R1".to_string(),
            publisher: None,
            title: None,
        };
        insert_framework_version(&mut tx, fw_id, &canonical, None, None)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // Second insert of same release key should fail.
        let mut tx2 = pool.begin().await.unwrap();
        let err = insert_framework_version(&mut tx2, fw_id, &canonical, None, None).await;
        tx2.rollback().await.unwrap();

        assert!(err.is_err(), "duplicate release key must be rejected");
        let msg = err.unwrap_err().to_string();
        assert!(
            msg.contains("FRAMEWORK_RELEASE_CONFLICT"),
            "expected FRAMEWORK_RELEASE_CONFLICT in: {msg}"
        );
    }

    // ── Requirement lineage ───────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn requirement_lineage_is_idempotent() {
        let pool = test_pool().await;
        let fw_key = format!("test-fw-req-{}", Uuid::new_v4());

        let mut tx = pool.begin().await.unwrap();
        let fw_id = upsert_framework_lineage(&mut tx, "FW", None, &fw_key, None)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let req_key = format!("V-{}", Uuid::new_v4().simple());

        let mut tx1 = pool.begin().await.unwrap();
        let req_id1 = upsert_requirement_lineage(&mut tx1, fw_id, &req_key)
            .await
            .unwrap();
        tx1.commit().await.unwrap();

        let mut tx2 = pool.begin().await.unwrap();
        let req_id2 = upsert_requirement_lineage(&mut tx2, fw_id, &req_key)
            .await
            .unwrap();
        tx2.commit().await.unwrap();

        assert_eq!(req_id1, req_id2, "same key must return same lineage id");
    }

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn requirement_reconciliation_classifies_changed_and_removed_requirements() {
        let pool = test_pool().await;
        let framework_key = format!("test-fw-release-diff-{}", Uuid::new_v4());
        let unchanged = RequirementVersionCanonical {
            canonical_requirement_key: "V-unchanged".to_string(),
            external_id: "V-unchanged".to_string(),
            title: Some("Unchanged requirement".to_string()),
            description: None,
            kind: "rule".to_string(),
            severity: Some("medium".to_string()),
            check_text: Some("check".to_string()),
            fix_text: Some("fix".to_string()),
            metadata: serde_json::json!({}),
        };
        let changed_v1 = RequirementVersionCanonical {
            canonical_requirement_key: "V-changed".to_string(),
            external_id: "V-changed".to_string(),
            title: Some("Original title".to_string()),
            description: None,
            kind: "rule".to_string(),
            severity: Some("medium".to_string()),
            check_text: Some("check".to_string()),
            fix_text: Some("fix".to_string()),
            metadata: serde_json::json!({}),
        };
        let removed = RequirementVersionCanonical {
            canonical_requirement_key: "V-removed".to_string(),
            external_id: "V-removed".to_string(),
            title: None,
            description: None,
            kind: "rule".to_string(),
            severity: None,
            check_text: None,
            fix_text: None,
            metadata: serde_json::json!({}),
        };

        let mut tx = pool.begin().await.unwrap();
        let framework_id = upsert_framework_lineage(
            &mut tx,
            "Release diff framework",
            None,
            &framework_key,
            None,
        )
        .await
        .unwrap();
        let framework_version_id = insert_framework_version(
            &mut tx,
            framework_id,
            &FrameworkVersionCanonical {
                canonical_source_key: framework_key,
                canonical_release_key: "V1R1".to_string(),
                version: "V1R1".to_string(),
                publisher: None,
                title: None,
            },
            None,
            None,
        )
        .await
        .unwrap();
        for canonical in [&unchanged, &changed_v1, &removed] {
            let requirement_id = upsert_requirement_lineage(
                &mut tx,
                framework_id,
                &canonical.canonical_requirement_key,
            )
            .await
            .unwrap();
            insert_requirement_version(
                &mut tx,
                requirement_id,
                framework_version_id,
                canonical,
                None,
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        let changed_v2 = RequirementVersionCanonical {
            title: Some("Changed title".to_string()),
            ..changed_v1.clone()
        };
        let preview =
            preview_requirement_reconciliation(&pool, framework_id, None, &[unchanged, changed_v2])
                .await
                .unwrap();

        assert_eq!(
            preview.requirements[0].state,
            RequirementReconciliationState::ExistingUnchanged
        );
        assert_eq!(
            preview.requirements[1].state,
            RequirementReconciliationState::ExistingChanged
        );
        assert_eq!(preview.removed_requirements.len(), 1);
        assert_eq!(preview.removed_requirements[0].external_id, "V-removed");
        assert_eq!(
            preview.removed_requirements[0].state,
            RequirementReconciliationState::RemovedFromRelease
        );
    }

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn related_candidates_use_trusted_current_normalized_mappings() {
        use crate::compliance::requirement_model::RelatedRequirementIdentifiers;

        let pool = test_pool().await;
        let actor: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let source_key = format!("related-candidate-{}", Uuid::new_v4());
        let related_key = format!("related-source-{}", Uuid::new_v4());
        let shared_cci = format!("CCI-TEST-{}", Uuid::new_v4().simple());
        let mut tx = pool.begin().await.unwrap();
        let incoming_framework =
            upsert_framework_lineage(&mut tx, "Incoming framework", None, &source_key, None)
                .await
                .unwrap();
        let related_framework =
            upsert_framework_lineage(&mut tx, "Related framework", None, &related_key, None)
                .await
                .unwrap();
        let version = FrameworkVersionCanonical {
            canonical_source_key: source_key.clone(),
            canonical_release_key: "V1R1".to_string(),
            version: "V1R1".to_string(),
            publisher: None,
            title: None,
        };
        let incoming_version =
            insert_framework_version(&mut tx, incoming_framework, &version, None, None)
                .await
                .unwrap();
        let related_version = insert_framework_version(
            &mut tx,
            related_framework,
            &FrameworkVersionCanonical {
                canonical_source_key: related_key.clone(),
                ..version.clone()
            },
            None,
            None,
        )
        .await
        .unwrap();
        let incoming_req = upsert_requirement_lineage(&mut tx, incoming_framework, "V-INCOMING")
            .await
            .unwrap();
        let related_req = upsert_requirement_lineage(&mut tx, related_framework, "NIST-AC-X")
            .await
            .unwrap();
        let _incoming_rv = insert_requirement_version(
            &mut tx,
            incoming_req,
            incoming_version,
            &RequirementVersionCanonical {
                canonical_requirement_key: "V-INCOMING".to_string(),
                external_id: "V-INCOMING".to_string(),
                title: Some("Incoming".to_string()),
                kind: "rule".to_string(),
                metadata: serde_json::json!({"cci_ids": [&shared_cci]}),
                description: None,
                severity: None,
                check_text: None,
                fix_text: None,
            },
            None,
        )
        .await
        .unwrap();
        let authoritative_req = upsert_requirement_lineage(&mut tx, incoming_framework, "V-AUTH")
            .await
            .unwrap();
        let inherited_req = upsert_requirement_lineage(&mut tx, incoming_framework, "V-INHERITED")
            .await
            .unwrap();
        let authoritative_rv = insert_requirement_version(
            &mut tx,
            authoritative_req,
            incoming_version,
            &RequirementVersionCanonical {
                canonical_requirement_key: "V-AUTH".to_string(),
                external_id: "V-AUTH".to_string(),
                title: Some("Authoritative".to_string()),
                kind: "rule".to_string(),
                metadata: serde_json::json!({}),
                description: None,
                severity: None,
                check_text: None,
                fix_text: Some("services.openssh.enable = true;".to_string()),
            },
            None,
        )
        .await
        .unwrap();
        let inherited_rv = insert_requirement_version(
            &mut tx,
            inherited_req,
            incoming_version,
            &RequirementVersionCanonical {
                canonical_requirement_key: "V-INHERITED".to_string(),
                external_id: "V-INHERITED".to_string(),
                title: Some("Inherited".to_string()),
                kind: "rule".to_string(),
                metadata: serde_json::json!({}),
                description: None,
                severity: None,
                check_text: None,
                fix_text: Some("services.openssh.enable = true;".to_string()),
            },
            None,
        )
        .await
        .unwrap();
        let related_rv = insert_requirement_version(
            &mut tx,
            related_req,
            related_version,
            &RequirementVersionCanonical {
                canonical_requirement_key: "NIST-AC-X".to_string(),
                external_id: "NIST-AC-X".to_string(),
                title: Some("NIST access control".to_string()),
                kind: "control".to_string(),
                metadata: serde_json::json!({"cci_ids": [shared_cci.to_ascii_lowercase()], "srg_ids": ["SRG-OS-000109-GPOS-00051"]}),
                description: None,
                severity: None,
                check_text: None,
                fix_text: None,
            },
            None,
        )
        .await
        .unwrap();
        let policy_id = Uuid::new_v4();
        let policy_version_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO deployment_policies (id, name, description, policy_type) VALUES ($1, $2, $3, 'native')",
        )
        .bind(policy_id)
        .bind(format!("Related candidate policy {}", Uuid::new_v4()))
        .bind("related candidate test")
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO deployment_policy_versions (id, policy_id, version, publication_state, name, policy_type, implementation_state, execution_phase, config, compliance_metadata, dependencies, semantic_digest, digest_algorithm) VALUES ($1, $2, '1.0', 'draft', 'related candidate policy', 'native', 'native', 'deploy', '{\"services.openssh.enable\": true}', '{}', '[]', 'related-candidate', 'sha-256')",
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        insert_policy_mapping_in_tx(
            &mut tx,
            policy_version_id,
            related_rv,
            "implements",
            "full",
            Some("shared CCI evidence"),
            "manual",
            None,
            actor,
        )
        .await
        .unwrap();
        insert_policy_mapping_in_tx(
            &mut tx,
            policy_version_id,
            authoritative_rv,
            "implements",
            "full",
            Some("authoritative precedence"),
            "manual",
            None,
            actor,
        )
        .await
        .unwrap();
        insert_policy_mapping_in_tx(
            &mut tx,
            policy_version_id,
            inherited_rv,
            "implements",
            "full",
            Some("inherited precedence"),
            "manual",
            None,
            actor,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE deployment_policy_versions SET trust_state = 'trusted', trusted_by = $2, trusted_at = NOW(), publication_state = 'accepted', published_at = NOW() WHERE id = $1")
            .bind(policy_version_id)
            .bind(actor)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let candidates = find_policy_candidates(
            &pool,
            None,
            None,
            None,
            &RelatedRequirementIdentifiers {
                cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                srg_ids: Default::default(),
            },
            Some(incoming_framework),
        )
        .await
        .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].match_type,
            PolicyCandidateMatchType::RelatedMapping
        );
        assert_eq!(candidates[0].confidence, 70);
        assert!(
            candidates[0]
                .match_reasons
                .iter()
                .any(|reason| { reason == &format!("Shared {}", shared_cci.to_ascii_uppercase()) }),
            "reasons: {:?}",
            candidates[0].match_reasons
        );

        let precedence = find_policy_candidates(
            &pool,
            Some(authoritative_rv),
            Some(inherited_rv),
            Some("services.openssh.enable = true;"),
            &RelatedRequirementIdentifiers {
                cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                srg_ids: Default::default(),
            },
            Some(incoming_framework),
        )
        .await
        .unwrap();
        let selected = precedence
            .iter()
            .find(|candidate| candidate.policy_version_id == policy_version_id)
            .expect("precedence fixture policy candidate");
        assert_eq!(
            selected.match_type,
            PolicyCandidateMatchType::AuthoritativeMapping
        );
        assert_eq!(selected.confidence, 100);

        let inherited_precedence = find_policy_candidates(
            &pool,
            None,
            Some(inherited_rv),
            Some("services.openssh.enable = true;"),
            &RelatedRequirementIdentifiers {
                cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                srg_ids: Default::default(),
            },
            Some(incoming_framework),
        )
        .await
        .unwrap();
        let selected = inherited_precedence
            .iter()
            .find(|candidate| candidate.policy_version_id == policy_version_id)
            .expect("inherited precedence fixture policy candidate");
        assert_eq!(
            selected.match_type,
            PolicyCandidateMatchType::InheritedMapping
        );
        assert_eq!(selected.confidence, 95);

        let exact_precedence = find_policy_candidates(
            &pool,
            None,
            None,
            Some("services.openssh.enable = true;"),
            &RelatedRequirementIdentifiers {
                cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                srg_ids: Default::default(),
            },
            Some(incoming_framework),
        )
        .await
        .unwrap();
        let selected = exact_precedence
            .iter()
            .find(|candidate| candidate.policy_version_id == policy_version_id)
            .expect("exact precedence fixture policy candidate");
        assert_eq!(
            selected.match_type,
            PolicyCandidateMatchType::ExactTechnicalMatch
        );
        assert_eq!(selected.confidence, 90);
        assert!(
            candidates[0]
                .match_reasons
                .iter()
                .any(|reason| reason.contains("NIST-AC-X"))
        );

        // Exact matching is required: a different identifier, a substring, or
        // no identifiers must not produce a related candidate. Same-framework
        // evidence is also deliberately excluded by the query boundary.
        for identifiers in [
            RelatedRequirementIdentifiers {
                cci_ids: [format!("{shared_cci}-EXTRA")].into_iter().collect(),
                srg_ids: Default::default(),
            },
            RelatedRequirementIdentifiers {
                cci_ids: [format!("{shared_cci}-OTHER")].into_iter().collect(),
                srg_ids: Default::default(),
            },
            RelatedRequirementIdentifiers::default(),
        ] {
            assert!(
                find_policy_candidates(
                    &pool,
                    None,
                    None,
                    None,
                    &identifiers,
                    Some(incoming_framework),
                )
                .await
                .unwrap()
                .is_empty()
            );
        }

        assert!(
            find_policy_candidates(
                &pool,
                None,
                None,
                None,
                &RelatedRequirementIdentifiers {
                    cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                    srg_ids: Default::default(),
                },
                Some(related_framework),
            )
            .await
            .unwrap()
            .is_empty(),
            "same-framework mappings must not be offered as related candidates"
        );

        // Trust is an eligibility boundary, independent of identifier overlap.
        // Build a separate accepted/current policy whose mapping remains
        // suggested; accepted mappings are immutable after publication.
        let untrusted_policy_id = Uuid::new_v4();
        let untrusted_version_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO deployment_policies (id, name, description, policy_type) VALUES ($1, $2, $3, 'native')",
        )
        .bind(untrusted_policy_id)
        .bind(format!("Untrusted related policy {}", Uuid::new_v4()))
        .bind("untrusted related candidate test")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO deployment_policy_versions (id, policy_id, version, publication_state, name, policy_type, implementation_state, execution_phase, config, compliance_metadata, dependencies, semantic_digest, digest_algorithm) VALUES ($1, $2, '1.0', 'draft', 'untrusted related policy', 'native', 'native', 'deploy', '{}', '{}', '[]', 'untrusted-related', 'sha-256')",
        )
        .bind(untrusted_version_id)
        .bind(untrusted_policy_id)
        .execute(&pool)
        .await
        .unwrap();
        let mut untrusted_tx = pool.begin().await.unwrap();
        insert_policy_mapping_in_tx(
            &mut untrusted_tx,
            untrusted_version_id,
            related_rv,
            "implements",
            "full",
            Some("untrusted shared evidence"),
            "manual",
            None,
            actor,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE policy_requirement_mappings SET trust_state = 'suggested' WHERE policy_version_id = $1")
            .bind(untrusted_version_id)
            .execute(&mut *untrusted_tx)
            .await
            .unwrap();
        sqlx::query("UPDATE deployment_policy_versions SET trust_state = 'trusted', trusted_by = $2, trusted_at = NOW(), publication_state = 'accepted', published_at = NOW() WHERE id = $1")
            .bind(untrusted_version_id)
            .bind(actor)
            .execute(&mut *untrusted_tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(untrusted_version_id)
        .bind(untrusted_policy_id)
        .execute(&mut *untrusted_tx)
        .await
        .unwrap();
        untrusted_tx.commit().await.unwrap();
        let candidates = find_policy_candidates(
            &pool,
            None,
            None,
            None,
            &RelatedRequirementIdentifiers {
                cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                srg_ids: Default::default(),
            },
            Some(incoming_framework),
        )
        .await
        .unwrap();
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.policy_version_id != untrusted_version_id),
            "untrusted mappings must not produce candidates"
        );

        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = NULL WHERE id = $1",
        )
        .bind(policy_id)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            find_policy_candidates(
                &pool,
                None,
                None,
                None,
                &RelatedRequirementIdentifiers {
                    cci_ids: [shared_cci.to_ascii_uppercase()].into_iter().collect(),
                    srg_ids: Default::default(),
                },
                Some(incoming_framework),
            )
            .await
            .unwrap()
            .is_empty()
        );
    }

    // ── Mapping immutability ──────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn mapping_blocked_on_accepted_policy_version() {
        let pool = test_pool().await;

        // Create a minimal policy + accepted version.
        let policy_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ($1, 'custom_check', '{\"expression\":\"true\"}', false) RETURNING id",
        )
        .bind(format!("mapping-immutability-test-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();

        let version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Accept the policy version.
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
            .bind(policy_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE deployment_policy_versions \
             SET publication_state = 'accepted', trust_state = 'trusted', \
                 trusted_at = NOW(), published_at = NOW() \
             WHERE id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Create a minimal requirement version to map to.
        let fw_key = format!("test-fw-mapping-{}", Uuid::new_v4());
        let mut tx = pool.begin().await.unwrap();
        let fw_id = upsert_framework_lineage(&mut tx, "FW", None, &fw_key, None)
            .await
            .unwrap();
        let fv_canonical = FrameworkVersionCanonical {
            canonical_source_key: fw_key.clone(),
            canonical_release_key: "V1R1".to_string(),
            version: "V1R1".to_string(),
            publisher: None,
            title: None,
        };
        let fv_id = insert_framework_version(&mut tx, fw_id, &fv_canonical, None, None)
            .await
            .unwrap();
        let req_key = format!("V-{}", Uuid::new_v4().simple());
        let req_id = upsert_requirement_lineage(&mut tx, fw_id, &req_key)
            .await
            .unwrap();
        let rv_canonical = RequirementVersionCanonical {
            canonical_requirement_key: req_key.clone(),
            external_id: req_key.clone(),
            title: None,
            description: None,
            kind: "rule".to_string(),
            severity: None,
            check_text: None,
            fix_text: None,
            metadata: serde_json::json!({}),
        };
        let rv_id = insert_requirement_version(&mut tx, req_id, fv_id, &rv_canonical, None)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // Attempt to create a mapping on the accepted version — must fail.
        // Use a real user for the FK.
        let actor: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let result = create_policy_mapping(
            &pool,
            version_id,
            rv_id,
            "implements",
            "full",
            None,
            "manual",
            actor,
        )
        .await;

        assert!(
            result.is_err(),
            "mapping on accepted version must be rejected"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("POLICY_MAPPING_IMMUTABLE"),
            "expected POLICY_MAPPING_IMMUTABLE in: {msg}"
        );
    }

    // ── Bundle coverage ───────────────────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires a database"]
    async fn bundle_coverage_full_partial_unmapped() {
        let pool = test_pool().await;

        // Create framework + version + 3 requirements.
        let fw_key = format!("test-fw-coverage-{}", Uuid::new_v4());
        let mut tx = pool.begin().await.unwrap();

        let fw_id = upsert_framework_lineage(&mut tx, "Coverage FW", None, &fw_key, None)
            .await
            .unwrap();
        let fv_canonical = FrameworkVersionCanonical {
            canonical_source_key: fw_key.clone(),
            canonical_release_key: "V1R1".to_string(),
            version: "V1R1".to_string(),
            publisher: None,
            title: None,
        };
        let fv_id = insert_framework_version(&mut tx, fw_id, &fv_canonical, None, None)
            .await
            .unwrap();

        let mut make_req = |key: &str| {
            let canonical = RequirementVersionCanonical {
                canonical_requirement_key: key.to_string(),
                external_id: key.to_string(),
                title: None,
                description: None,
                kind: "rule".to_string(),
                severity: None,
                check_text: None,
                fix_text: None,
                metadata: serde_json::json!({}),
            };
            (key.to_string(), canonical, fv_id)
        };

        let (k1, c1, fv) = make_req("V-001");
        let (k2, c2, _) = make_req("V-002");
        let (k3, c3, _) = make_req("V-003");

        let req_id1 = upsert_requirement_lineage(&mut tx, fw_id, &k1)
            .await
            .unwrap();
        let req_id2 = upsert_requirement_lineage(&mut tx, fw_id, &k2)
            .await
            .unwrap();
        let req_id3 = upsert_requirement_lineage(&mut tx, fw_id, &k3)
            .await
            .unwrap();

        let rv_id1 = insert_requirement_version(&mut tx, req_id1, fv, &c1, None)
            .await
            .unwrap();
        let rv_id2 = insert_requirement_version(&mut tx, req_id2, fv, &c2, None)
            .await
            .unwrap();
        let rv_id3 = insert_requirement_version(&mut tx, req_id3, fv, &c3, None)
            .await
            .unwrap();

        tx.commit().await.unwrap();

        // Create a bundle + version with all 3 requirements.
        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, version, layer, owner) \
             VALUES ($1, 'test', '1.0', 'fleet', 'test') RETURNING id",
        )
        .bind(format!("coverage-test-bundle-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();

        let bv_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        insert_bundle_version_requirement(&mut tx, bv_id, rv_id1, 0)
            .await
            .unwrap();
        insert_bundle_version_requirement(&mut tx, bv_id, rv_id2, 1)
            .await
            .unwrap();
        insert_bundle_version_requirement(&mut tx, bv_id, rv_id3, 2)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // Create a policy + draft version, add it to the bundle.
        let pol_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ($1, 'custom_check', '{\"expression\":\"true\"}', false) RETURNING id",
        )
        .bind(format!("coverage-policy-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .unwrap();

        let pv_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(pol_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Add the policy to the bundle.
        sqlx::query(
            "INSERT INTO compliance_bundle_version_policies \
             (bundle_version_id, policy_version_id, policy_order, selected) \
             VALUES ($1, $2, 0, true)",
        )
        .bind(bv_id)
        .bind(pv_id)
        .execute(&pool)
        .await
        .unwrap();

        // Map the policy to V-001 (full/implements) → full coverage.
        // Map the policy to V-002 (partial/supports) → partial coverage.
        // V-003 has no mapping → unmapped.
        //
        // Use a real user ID to satisfy the FK on policy_requirement_mappings.
        let actor: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        create_policy_mapping(
            &pool,
            pv_id,
            rv_id1,
            "implements",
            "full",
            None,
            "manual",
            actor,
        )
        .await
        .unwrap();
        create_policy_mapping(
            &pool, pv_id, rv_id2, "supports", "partial", None, "manual", actor,
        )
        .await
        .unwrap();

        // Compute coverage.
        let report = compute_bundle_requirement_coverage(&pool, bv_id)
            .await
            .unwrap();

        assert_eq!(report.total_requirements, 3);
        assert_eq!(report.full, 1);
        assert_eq!(report.partial, 1);
        assert_eq!(report.unmapped, 1);
    }
}
