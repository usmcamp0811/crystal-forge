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

use crate::compliance::framework_model::{FrameworkVersionCanonical, write_framework_version_digest};
use crate::compliance::requirement_model::{
    FrameworkReconciliation, FrameworkReconciliationState, PolicyCandidate,
    PolicyCandidateMatchType, PolicyReconciliation, RequirementReconciliation,
    RequirementReconciliationState, RequirementVersionCanonical, write_requirement_version_digest,
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
        None => bail!("POLICY_VERSION_NOT_FOUND: policy version {policy_version_id} does not exist"),
        Some("accepted") | Some("deprecated") => bail!(
            "POLICY_MAPPING_IMMUTABLE: cannot modify mappings on policy version {} \
             because it is in an immutable state. Create a derived draft first.",
            policy_version_id
        ),
        _ => {}
    }

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
    .fetch_one(pool)
    .await
    .context("failed to insert policy requirement mapping")?;

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
    .execute(pool)
    .await
    .context("failed to update policy requirement mapping")?;

    if affected.rows_affected() == 0 {
        bail!(
            "POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND: mapping {mapping_id} was not found \
             or belongs to an immutable policy version"
        );
    }
    Ok(())
}

/// Delete a requirement mapping.
/// Fails if the policy version is accepted/deprecated.
pub async fn delete_policy_mapping(pool: &PgPool, mapping_id: Uuid) -> Result<()> {
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
    .execute(pool)
    .await
    .context("failed to delete policy requirement mapping")?;

    if affected.rows_affected() == 0 {
        bail!(
            "POLICY_MAPPING_IMMUTABLE_OR_NOT_FOUND: mapping {mapping_id} was not found \
             or belongs to an immutable policy version"
        );
    }
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
    let artifact_exists: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_source_artifacts WHERE sha256 = $1",
    )
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
    let existing_framework_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_frameworks WHERE canonical_source_key = $1",
    )
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
    canonical_keys: &[String],
) -> Result<Vec<RequirementReconciliation>> {
    if canonical_keys.is_empty() {
        return Ok(vec![]);
    }

    // Batch: fetch existing requirement lineages for this framework.
    let existing_lineages: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT canonical_requirement_key, id \
         FROM compliance_requirements \
         WHERE framework_id = $1 \
           AND canonical_requirement_key = ANY($2)",
    )
    .bind(framework_id)
    .bind(canonical_keys)
    .fetch_all(pool)
    .await
    .context("failed to batch-query existing requirement lineages")?;

    use std::collections::HashMap;
    let lineage_map: HashMap<String, Uuid> = existing_lineages.into_iter().collect();

    // Batch: fetch existing requirement versions for the previous framework version
    // (to detect changes).
    let prev_versions: HashMap<Uuid, (Uuid, String)> = if let Some(fv_id) = framework_version_id {
        // Find the most recent earlier version for the same framework.
        let prev_fv_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM compliance_framework_versions \
             WHERE framework_id = $1 AND id != $2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(framework_id)
        .bind(fv_id)
        .fetch_optional(pool)
        .await
        .context("failed to find previous framework version")?;

        if let Some(prev_id) = prev_fv_id {
            let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
                "SELECT rv.requirement_id, rv.id, rv.semantic_digest \
                 FROM compliance_requirement_versions rv \
                 WHERE rv.framework_version_id = $1",
            )
            .bind(prev_id)
            .fetch_all(pool)
            .await
            .context("failed to fetch previous requirement versions")?;
            rows.into_iter()
                .map(|(req_id, rv_id, digest)| (req_id, (rv_id, digest)))
                .collect()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    // Build per-rule reconciliation.
    let mut results = Vec::with_capacity(canonical_keys.len());
    for key in canonical_keys {
        let rec = if let Some(&req_id) = lineage_map.get(key) {
            // Check if this requirement has a version in the previous release.
            match prev_versions.get(&req_id) {
                Some((_prev_rv_id, _prev_digest)) => {
                    // Requirement existed in a previous release.
                    // (Full digest comparison would happen in commit path.)
                    RequirementReconciliation {
                        canonical_requirement_key: key.clone(),
                        external_id: key.clone(),
                        state: RequirementReconciliationState::ExistingUnchanged,
                        existing_requirement_id: Some(req_id),
                        existing_requirement_version_id: None,
                        existing_digest: None,
                    }
                }
                None => RequirementReconciliation {
                    canonical_requirement_key: key.clone(),
                    external_id: key.clone(),
                    state: RequirementReconciliationState::ExistingUnchanged,
                    existing_requirement_id: Some(req_id),
                    existing_requirement_version_id: None,
                    existing_digest: None,
                },
            }
        } else {
            RequirementReconciliation {
                canonical_requirement_key: key.clone(),
                external_id: key.clone(),
                state: RequirementReconciliationState::NewRequirement,
                existing_requirement_id: None,
                existing_requirement_version_id: None,
                existing_digest: None,
            }
        };
        results.push(rec);
    }
    Ok(results)
}

/// Find policy candidates for a requirement (for the reconciliation preview).
///
/// Searches in priority order:
/// 1. Authoritative mapping already exists for this requirement.
/// 2. Inherited mapping from an unchanged previous requirement version.
/// 3. (Future: exact technical match, related mapping, fuzzy similarity.)
///
/// Returns candidates with `match_type` and `confidence` for UI display.
pub async fn find_policy_candidates(
    pool: &PgPool,
    requirement_id: Uuid,
) -> Result<Vec<PolicyCandidate>> {
    // Check for authoritative mappings on accepted policy versions.
    struct CandidateRow {
        policy_id: Uuid,
        policy_version_id: Uuid,
        policy_name: String,
    }

    let rows: Vec<CandidateRow> = sqlx::query_as!(
        CandidateRow,
        r#"
        SELECT DISTINCT
            dp.id  AS "policy_id!",
            pv.id  AS "policy_version_id!",
            pv.name AS "policy_name!"
        FROM policy_requirement_mappings m
        JOIN compliance_requirement_versions rv ON rv.id = m.requirement_version_id
        JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id
        JOIN deployment_policies dp ON dp.id = pv.policy_id
        WHERE rv.requirement_id = $1
          AND m.trust_state = 'trusted'
          AND pv.publication_state IN ('accepted', 'deprecated')
        ORDER BY pv.name
        "#,
        requirement_id
    )
    .fetch_all(pool)
    .await
    .context("failed to find authoritative policy candidates")?;

    let candidates = rows
        .into_iter()
        .map(|r| PolicyCandidate {
            policy_id: r.policy_id,
            policy_version_id: r.policy_version_id,
            policy_name: r.policy_name,
            match_type: PolicyCandidateMatchType::AuthoritativeMapping,
            confidence: 100,
            match_reasons: vec!["Authoritative policy-requirement mapping exists.".to_string()],
        })
        .collect();

    Ok(candidates)
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
            &std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set for DB-gated tests"),
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
        let err = insert_framework_version(&mut tx2, fw_id, &canonical, None, None)
            .await;
        tx2.rollback().await.unwrap();

        assert!(
            err.is_err(),
            "duplicate release key must be rejected"
        );
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

        let version_id: Uuid =
            sqlx::query_scalar("SELECT current_draft_version_id FROM deployment_policies WHERE id = $1")
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

        assert!(result.is_err(), "mapping on accepted version must be rejected");
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

        let req_id1 = upsert_requirement_lineage(&mut tx, fw_id, &k1).await.unwrap();
        let req_id2 = upsert_requirement_lineage(&mut tx, fw_id, &k2).await.unwrap();
        let req_id3 = upsert_requirement_lineage(&mut tx, fw_id, &k3).await.unwrap();

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

        let bv_id: Uuid =
            sqlx::query_scalar("SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1")
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

        let pv_id: Uuid =
            sqlx::query_scalar("SELECT current_draft_version_id FROM deployment_policies WHERE id = $1")
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
            &pool, pv_id, rv_id1, "implements", "full", None, "manual", actor,
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
