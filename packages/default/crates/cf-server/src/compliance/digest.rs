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
    /// Sorted list of added policy version IDs.
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
        let mut additions: Vec<String> = self.additions.iter().map(|id| id.to_string()).collect();
        additions.sort();
        let overrides: Vec<Value> = self
            .value_overrides
            .iter()
            .map(|(pid, path, val)| {
                json!({ "policy_version_id": pid.to_string(), "value_path": path, "value": val })
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

    let digest = canonical.compute_digest();
    let rows_affected = sqlx::query(
        r#"
        UPDATE deployment_policy_versions
        SET semantic_digest          = $1,
            digest_algorithm         = 'sha-256',
            canonicalization_version = 'cf-model-json-1',
            name                     = $3,
            description              = $4,
            policy_type              = $5,
            implementation_state     = $6,
            execution_phase          = $7,
            config                   = $8::jsonb,
            compliance_metadata      = $9::jsonb,
            dependencies             = $10::jsonb,
            enabled_by_default       = $11
        WHERE id = $2
          AND publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .bind(&digest)
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
    let digest = canonical.compute_digest();

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

    let rows_affected = sqlx::query(
        r#"
        UPDATE compliance_bundle_versions
        SET semantic_digest          = $1,
            digest_algorithm         = 'sha-256',
            canonicalization_version = 'cf-model-json-1',
            name                     = $3,
            framework                = $4,
            framework_version        = $5,
            description              = $6,
            layer                    = $7,
            owner                    = $8
        WHERE id = $2
          AND publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .bind(&digest)
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
        "SELECT policy_version_id FROM compliance_assignment_additions WHERE assignment_version_id = (SELECT current_version_id FROM compliance_bundle_assignments WHERE id = $1) ORDER BY policy_version_id",
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
    //   + additions (sorted by policy_version_id for stability)
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
        policy_id: Uuid,
        name: String,
        description: Option<String>,
        policy_type: String,
        implementation_state: String,
        execution_phase: String,
        config: Value,
        compliance_metadata: Value,
        dependencies: Value,
        opaque_xml: Option<String>,
        enabled: Option<bool>,
    }
    let pending_policies: Vec<PolicyVersionRow> = sqlx::query_as(
        r#"
        SELECT dpv.id, dpv.policy_id, dpv.name, dpv.description, dpv.policy_type,
               dpv.implementation_state, dpv.execution_phase, dpv.config,
               dpv.compliance_metadata, dpv.dependencies, dpv.opaque_xml,
               dpv.enabled_by_default AS enabled
        FROM deployment_policy_versions dpv
        WHERE dpv.semantic_digest = 'pending'
          AND dpv.publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in &pending_policies {
        let canonical = PolicyVersionCanonical {
            name: row.name.clone(),
            description: row.description.clone(),
            policy_type: row.policy_type.clone(),
            implementation_state: row.implementation_state.clone(),
            execution_phase: row.execution_phase.clone(),
            config: row.config.clone(),
            compliance_metadata: row.compliance_metadata.clone(),
            dependencies: row.dependencies.clone(),
            opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(row.opaque_xml.as_deref()),
            enabled_by_default: row.enabled,
        };
        let digest = canonical.compute_digest();
        sqlx::query(
            r#"
            UPDATE deployment_policy_versions
            SET semantic_digest = $1, digest_algorithm = 'sha-256',
                canonicalization_version = 'cf-model-json-1'
            WHERE id = $2
            "#,
        )
        .bind(&digest)
        .bind(row.id)
        .execute(pool)
        .await?;
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
          AND publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in &pending_bundles {
        #[derive(sqlx::FromRow)]
        struct MembershipRow {
            policy_version_id: Uuid,
            selected: bool,
        }
        let mem_rows: Vec<MembershipRow> = sqlx::query_as(
            r#"
            SELECT policy_version_id, selected
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = $1
            ORDER BY policy_order ASC
            "#,
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;

        let members: Vec<BundleMembershipEntry> = mem_rows
            .into_iter()
            .map(|r| BundleMembershipEntry {
                policy_version_id: r.policy_version_id,
                selected: r.selected,
            })
            .collect();

        let canonical = BundleVersionCanonical {
            name: row.name.clone(),
            framework: row.framework.clone(),
            framework_version: row.framework_version.clone(),
            description: row.description.clone(),
            layer: row.layer.clone(),
            owner: row.owner.clone(),
            members,
        };
        let digest = canonical.compute_digest();
        sqlx::query(
            r#"
            UPDATE compliance_bundle_versions
            SET semantic_digest = $1, digest_algorithm = 'sha-256',
                canonicalization_version = 'cf-model-json-1'
            WHERE id = $2
            "#,
        )
        .bind(&digest)
        .bind(row.id)
        .execute(pool)
        .await?;
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

    let remaining_assignment: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE assignment_overlay_digest = 'pending'",
    )
    .fetch_one(pool)
    .await?;

    if remaining_policy > 0 || remaining_bundle > 0 || remaining_assignment > 0 {
        anyhow::bail!(
            "compliance: {remaining_policy} policy version(s), {remaining_bundle} bundle version(s), \
             and {remaining_assignment} assignment(s) still have pending digests after backfill"
        );
    }

    tracing::info!(
        policies = pending_policies.len(),
        bundles = pending_bundles.len(),
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
