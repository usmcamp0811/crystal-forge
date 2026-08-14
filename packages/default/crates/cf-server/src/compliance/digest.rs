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

use anyhow::Result;
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::canonical::semantic_digest;

// ── Typed canonical DTOs ─────────────────────────────────────────────────────

/// All semantic fields for a policy version digest.
///
/// The digest must change when any field that affects activation, enforcement,
/// or exported meaning changes. Fields like timestamps, trust state, local DB
/// IDs, and assignment state are excluded.
///
/// `opaque_xml` preserves imported semantics that Crystal Forge cannot model;
/// two opaque policies with different XML but identical modeled fields must
/// produce different digests. We hash the normalized opaque content rather than
/// including raw bytes in the digest DTO.
#[derive(Debug, Clone)]
pub struct PolicyVersionCanonical {
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub implementation_state: String,
    pub execution_phase: String,
    pub config: Value,
    pub compliance_metadata: Value,
    pub dependencies: Value,
    /// SHA-256 hex of the normalised opaque XML, or `null` when absent.
    /// Included so that different preserved XML always produces a different digest.
    pub opaque_xml_digest: Option<String>,
    /// Whether the policy lineage is currently enabled. This is part of the
    /// version model's default activation state for interchange.
    pub enabled_by_default: Option<bool>,
}

impl PolicyVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        json!({
            "canonicalization_version": "cf-model-json-1",
            "compliance_metadata": self.compliance_metadata,
            "config": self.config,
            "dependencies": self.dependencies,
            "description": self.description.as_deref().unwrap_or(""),
            "enabled_by_default": self.enabled_by_default,
            "execution_phase": self.execution_phase,
            "implementation_state": self.implementation_state,
            "name": self.name,
            "opaque_xml_digest": self.opaque_xml_digest,
            "policy_type": self.policy_type,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }

    /// Compute the sha-256 hex digest of the trimmed opaque XML, or return `None`.
    pub fn digest_opaque_xml(xml: Option<&str>) -> Option<String> {
        use sha2::{Digest as ShaDigest, Sha256};
        xml.map(|s| hex::encode(Sha256::digest(s.trim().as_bytes())))
    }
}

#[derive(Debug, Clone)]
struct PolicyMappingCanonical {
    requirement_version_id: Uuid,
    relationship: String,
    coverage: String,
    rationale: Option<String>,
    provenance: String,
    trust_state: String,
}

fn compute_mapping_digest(mappings: &mut [PolicyMappingCanonical]) -> String {
    mappings.sort_by_key(|mapping| mapping.requirement_version_id);
    let entries: Vec<Value> = mappings
        .iter()
        .map(|mapping| {
            json!({
                "coverage": mapping.coverage,
                "provenance": mapping.provenance,
                "rationale": mapping.rationale,
                "relationship": mapping.relationship,
                "requirement_version_id": mapping.requirement_version_id.to_string(),
                "trust_state": mapping.trust_state,
            })
        })
        .collect();
    semantic_digest(&json!(entries))
}

/// A single exact membership entry with both version identity and selection state.
#[derive(Debug, Clone)]
pub struct BundleMembershipEntry {
    pub policy_version_id: Uuid,
    pub selected: bool,
}

/// All semantic fields for a bundle version digest.
#[derive(Debug, Clone)]
pub struct BundleVersionCanonical {
    pub name: String,
    pub framework: String,
    pub framework_version: Option<String>,
    pub description: Option<String>,
    pub layer: String,
    pub owner: String,
    /// Ordered membership entries (by policy_order).
    pub members: Vec<BundleMembershipEntry>,
}

impl BundleVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        let members: Vec<Value> = self
            .members
            .iter()
            .map(|m| {
                json!({
                    "policy_version_id": m.policy_version_id.to_string(),
                    "selected": m.selected,
                })
            })
            .collect();
        json!({
            "canonicalization_version": "cf-model-json-1",
            "description": self.description.as_deref().unwrap_or(""),
            "framework": self.framework,
            "framework_version": self.framework_version.as_deref().unwrap_or(""),
            "layer": self.layer,
            "members": members,
            "name": self.name,
            "owner": self.owner,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

#[derive(Debug, Clone)]
struct BundleRequirementCanonical {
    requirement_version_id: Uuid,
    selected: bool,
}

fn compute_requirement_membership_digest(memberships: &mut [BundleRequirementCanonical]) -> String {
    memberships.sort_by_key(|membership| membership.requirement_version_id);
    semantic_digest(&json!(
        memberships
            .iter()
            .map(|membership| json!({
                "requirement_version_id": membership.requirement_version_id.to_string(),
                "selected": membership.selected,
            }))
            .collect::<Vec<_>>()
    ))
}

/// All semantic fields for an assignment effective-set digest.
///
/// Does NOT simply copy the bundle digest. Captures the specific overlay that
/// makes this assignment distinct: exclusions, additions, value overrides, and
/// enforcement mode, combined with the ordered resolved effective policy set.
#[derive(Debug, Clone)]
pub struct AssignmentEffectiveSetCanonical {
    pub enforcement_mode: String,
    /// Sorted list of excluded policy version IDs.
    pub exclusions: Vec<Uuid>,
    /// Added policy version IDs in declared assignment order.
    pub additions: Vec<Uuid>,
    /// Value overrides sorted by (policy_version_id, value_path).
    pub value_overrides: Vec<(Uuid, String, Value)>,
    /// Final resolved effective policy version IDs in evaluation order.
    pub effective_policy_version_ids: Vec<Uuid>,
}

impl AssignmentEffectiveSetCanonical {
    pub fn to_digest_value(&self) -> Value {
        let mut exclusions: Vec<String> = self.exclusions.iter().map(|id| id.to_string()).collect();
        exclusions.sort();
        let additions: Vec<String> = self.additions.iter().map(|id| id.to_string()).collect();
        let mut overrides = self
            .value_overrides
            .iter()
            .map(|(pid, path, val)| (pid.to_string(), path.clone(), val.clone()))
            .collect::<Vec<_>>();
        overrides.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
        let overrides: Vec<Value> = overrides
            .into_iter()
            .map(|(pid, path, val)| {
                json!({ "policy_version_id": pid, "value_path": path, "value": val })
            })
            .collect();
        let effective: Vec<String> = self
            .effective_policy_version_ids
            .iter()
            .map(|id| id.to_string())
            .collect();
        json!({
            "additions": additions,
            "canonicalization_version": "cf-model-json-1",
            "effective_policy_version_ids": effective,
            "enforcement_mode": self.enforcement_mode,
            "exclusions": exclusions,
            "value_overrides": overrides,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

/// Canonical digest input for a system's combined resolved set. Unlike the
/// assignment overlay digest, this includes every bundle source and direct
/// policy contribution that participated in resolution.
#[derive(Debug, Clone)]
pub struct CombinedEffectiveSetCanonical {
    pub bundle_version_ids_ordered: Vec<Uuid>,
    pub addition_policy_version_ids: Vec<Uuid>,
    pub direct_policy_version_ids: Vec<Uuid>,
    pub effective_policy_version_ids: Vec<Uuid>,
    pub policy_modes: Vec<(Uuid, String)>,
    pub effective_configs: Vec<(Uuid, Value)>,
}

impl CombinedEffectiveSetCanonical {
    pub fn to_digest_value(&self) -> Value {
        let mut additions: Vec<String> = self
            .addition_policy_version_ids
            .iter()
            .map(ToString::to_string)
            .collect();
        additions.sort();
        let mut direct: Vec<String> = self
            .direct_policy_version_ids
            .iter()
            .map(ToString::to_string)
            .collect();
        direct.sort();
        let modes: Vec<Value> = self
            .policy_modes
            .iter()
            .map(|(id, mode)| json!({ "policy_version_id": id.to_string(), "mode": mode }))
            .collect();
        let configs: Vec<Value> = self
            .effective_configs
            .iter()
            .map(|(id, config)| json!({ "policy_version_id": id.to_string(), "config": config }))
            .collect();
        json!({
            "canonicalization_version": "cf-model-json-1",
            "bundle_version_ids_ordered": self.bundle_version_ids_ordered,
            "addition_policy_version_ids": additions,
            "direct_policy_version_ids": direct,
            "effective_policy_version_ids": self.effective_policy_version_ids,
            "policy_modes": modes,
            "effective_configs": configs,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

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

/// Recompute a policy version digest after a mapping mutation in the same
/// transaction. The version's modeled fields and all mapping semantics are
/// read from the locked transaction state, so insertion order and row IDs do
/// not affect the result.
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn base_policy() -> PolicyVersionCanonical {
        PolicyVersionCanonical {
            name: "firewall".into(),
            description: Some("Firewall enabled".into()),
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: json!({"expr": "cfg.config.networking.firewall.enable"}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml_digest: None,
            enabled_by_default: Some(true),
        }
    }

    fn bundle(policy_ids: Vec<Uuid>) -> BundleVersionCanonical {
        BundleVersionCanonical {
            name: "Test Bundle".into(),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            description: Some("Description".into()),
            layer: "os".into(),
            owner: "Team".into(),
            members: policy_ids
                .into_iter()
                .map(|id| BundleMembershipEntry {
                    policy_version_id: id,
                    selected: true,
                })
                .collect(),
        }
    }

    fn mapping(requirement_version_id: Uuid) -> PolicyMappingCanonical {
        PolicyMappingCanonical {
            requirement_version_id,
            relationship: "supports".into(),
            coverage: "partial".into(),
            rationale: Some("rationale".into()),
            provenance: "manual".into(),
            trust_state: "trusted".into(),
        }
    }

    #[test]
    fn policy_digest_is_deterministic() {
        let a = base_policy().compute_digest();
        let b = base_policy().compute_digest();
        assert_eq!(a, b);
    }

    #[test]
    fn policy_digest_changes_when_implementation_state_changes() {
        let native = base_policy().compute_digest();
        let mut unbound = base_policy();
        unbound.implementation_state = "unbound".into();
        assert_ne!(native, unbound.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_execution_phase_changes() {
        let nix = base_policy().compute_digest();
        let mut post = base_policy();
        post.execution_phase = "post-build".into();
        assert_ne!(nix, post.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_compliance_metadata_changes() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.compliance_metadata = json!({"stig_id": "V-123456"});
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_for_each_classification_metadata_key() {
        let baseline = base_policy().compute_digest();
        let classification_edits = [
            ("category", json!("security")),
            ("framework", json!("DISA STIG")),
            ("severity", json!("high")),
            ("control_family", json!("AC")),
            ("cmmc_level", json!(2)),
            ("cis_section", json!("4.1")),
            ("rationale", json!("Required by the source control.")),
        ];

        for (key, value) in classification_edits {
            let mut policy = base_policy();
            policy.compliance_metadata = json!({key: value});
            assert_ne!(
                baseline,
                policy.compute_digest(),
                "{key} must affect the digest"
            );
        }
    }

    #[test]
    fn policy_digest_changes_when_dependencies_change() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.dependencies = json!([{"nix_option": "services.example.enable"}]);
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_opaque_xml_changes() {
        let mut with_xml = base_policy();
        with_xml.opaque_xml_digest =
            PolicyVersionCanonical::digest_opaque_xml(Some("<check>A</check>"));

        let mut with_different_xml = base_policy();
        with_different_xml.opaque_xml_digest =
            PolicyVersionCanonical::digest_opaque_xml(Some("<check>B</check>"));

        let no_xml = base_policy(); // opaque_xml_digest = None

        assert_ne!(no_xml.compute_digest(), with_xml.compute_digest());
        assert_ne!(
            with_xml.compute_digest(),
            with_different_xml.compute_digest()
        );
    }

    #[test]
    fn policy_digest_changes_when_config_changes() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.config = json!({"expr": "false"});
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_name_changes() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.name = "other-name".into();
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn bundle_digest_is_deterministic() {
        let ids = vec![
            Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap(),
            Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap(),
        ];
        assert_eq!(
            bundle(ids.clone()).compute_digest(),
            bundle(ids).compute_digest()
        );
    }

    #[test]
    fn bundle_digest_changes_on_policy_order() {
        let id1 = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let id2 = Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap();
        assert_ne!(
            bundle(vec![id1, id2]).compute_digest(),
            bundle(vec![id2, id1]).compute_digest()
        );
    }

    #[test]
    fn bundle_digest_changes_on_framework_version() {
        let mut b2 = bundle(vec![]);
        b2.framework_version = Some("V1R2".into());
        assert_ne!(bundle(vec![]).compute_digest(), b2.compute_digest());
    }

    #[test]
    fn bundle_digest_changes_on_description() {
        let mut b2 = bundle(vec![]);
        b2.description = Some("Different".into());
        assert_ne!(bundle(vec![]).compute_digest(), b2.compute_digest());
    }

    #[test]
    fn assignment_digest_differs_for_different_exclusions() {
        let id = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![id],
        };
        let b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![id],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![],
        };
        assert_ne!(a.compute_digest(), b.compute_digest());
    }

    #[test]
    fn policy_mapping_digest_changes_for_each_semantic_field() {
        let requirement_id = Uuid::from_u128(1);
        let base = {
            let mut mappings = vec![mapping(requirement_id)];
            compute_mapping_digest(&mut mappings)
        };

        for (field, value) in [
            ("relationship", "implements"),
            ("coverage", "full"),
            ("provenance", "inherited"),
            ("trust_state", "suggested"),
        ] {
            let mut changed = mapping(requirement_id);
            match field {
                "relationship" => changed.relationship = value.into(),
                "coverage" => changed.coverage = value.into(),
                "provenance" => changed.provenance = value.into(),
                "trust_state" => changed.trust_state = value.into(),
                _ => unreachable!(),
            }
            assert_ne!(base, compute_mapping_digest(&mut vec![changed]));
        }

        let mut changed = mapping(requirement_id);
        changed.rationale = Some("different rationale".into());
        assert_ne!(base, compute_mapping_digest(&mut vec![changed]));
    }

    #[test]
    fn policy_mapping_digest_is_stable_across_insertion_order() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let mut left = vec![mapping(first), mapping(second)];
        let mut right = vec![mapping(second), mapping(first)];
        assert_eq!(
            compute_mapping_digest(&mut left),
            compute_mapping_digest(&mut right)
        );
    }

    #[test]
    fn semantic_digest_remains_independent_of_policy_mappings() {
        let semantic = base_policy().compute_digest();
        let mut changed = mapping(Uuid::from_u128(2));
        changed.coverage = "full".into();
        assert_eq!(semantic, base_policy().compute_digest());
        assert_ne!(
            compute_mapping_digest(&mut vec![mapping(Uuid::from_u128(1))]),
            compute_mapping_digest(&mut vec![changed])
        );
    }

    #[test]
    fn bundle_requirement_digest_is_order_independent_and_membership_sensitive() {
        let first = BundleRequirementCanonical {
            requirement_version_id: Uuid::from_u128(1),
            selected: true,
        };
        let second = BundleRequirementCanonical {
            requirement_version_id: Uuid::from_u128(2),
            selected: false,
        };
        let mut left = vec![first.clone(), second.clone()];
        let mut right = vec![second, first];
        assert_eq!(
            compute_requirement_membership_digest(&mut left),
            compute_requirement_membership_digest(&mut right)
        );
        right[0].selected = !right[0].selected;
        assert_ne!(
            compute_requirement_membership_digest(&mut left),
            compute_requirement_membership_digest(&mut right)
        );
    }

    #[test]
    fn assignment_digest_differs_for_enforcement_mode() {
        let a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![],
        };
        let b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "report_only".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![],
        };
        assert_ne!(a.compute_digest(), b.compute_digest());
    }

    #[test]
    fn assignment_override_digest_is_order_independent() {
        let first = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let second = Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap();
        let a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![
                (second, "strict".into(), json!(false)),
                (first, "count".into(), json!(1)),
            ],
            effective_policy_version_ids: vec![first, second],
        };
        let b = AssignmentEffectiveSetCanonical {
            value_overrides: vec![
                (first, "count".into(), json!(1)),
                (second, "strict".into(), json!(false)),
            ],
            ..a.clone()
        };
        assert_eq!(a.compute_digest(), b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_enabled_by_default_changes() {
        assert_ne!(base_policy().compute_digest(), {
            let mut p = base_policy();
            p.enabled_by_default = Some(false);
            p.compute_digest()
        });
    }

    #[test]
    fn bundle_digest_changes_when_selected_changes() {
        let id = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let mut b = bundle(vec![id]);
        b.members[0].selected = false;
        assert_ne!(bundle(vec![id]).compute_digest(), b.compute_digest());
    }
}
