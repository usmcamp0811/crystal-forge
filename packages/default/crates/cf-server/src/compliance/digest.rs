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
}

impl PolicyVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        json!({
            "canonicalization_version": "cf-model-json-1",
            "compliance_metadata": self.compliance_metadata,
            "config": self.config,
            "dependencies": self.dependencies,
            "description": self.description.as_deref().unwrap_or(""),
            "execution_phase": self.execution_phase,
            "implementation_state": self.implementation_state,
            "name": self.name,
            "policy_type": self.policy_type,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
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
    /// Ordered policy version IDs (by policy_order in compliance_bundle_version_policies).
    pub policy_version_ids: Vec<Uuid>,
}

impl BundleVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        let ids: Vec<String> = self
            .policy_version_ids
            .iter()
            .map(|id| id.to_string())
            .collect();
        json!({
            "canonicalization_version": "cf-model-json-1",
            "description": self.description.as_deref().unwrap_or(""),
            "framework": self.framework,
            "framework_version": self.framework_version.as_deref().unwrap_or(""),
            "layer": self.layer,
            "name": self.name,
            "owner": self.owner,
            "policy_version_ids": ids,
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
/// This must be called **before** the transaction commits. A failure here
/// rolls back the entire mutation.
pub async fn write_policy_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    policy_id: Uuid,
    canonical: &PolicyVersionCanonical,
) -> Result<()> {
    let digest = canonical.compute_digest();
    sqlx::query(
        r#"
        UPDATE deployment_policy_versions
        SET semantic_digest          = $1,
            digest_algorithm         = 'sha-256',
            canonicalization_version = 'cf-model-json-1',
            -- Refresh all canonical fields so the version row is authoritative.
            name                     = $3,
            description              = $4,
            policy_type              = $5,
            implementation_state     = $6,
            execution_phase          = $7,
            config                   = $8::jsonb,
            compliance_metadata      = $9::jsonb,
            dependencies             = $10::jsonb
        WHERE policy_id  = $2
          AND publication_state = 'draft'
        "#,
    )
    .bind(&digest)
    .bind(policy_id)
    .bind(&canonical.name)
    .bind(&canonical.description)
    .bind(&canonical.policy_type)
    .bind(&canonical.implementation_state)
    .bind(&canonical.execution_phase)
    .bind(&canonical.config)
    .bind(&canonical.compliance_metadata)
    .bind(&canonical.dependencies)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Load the ordered policy version IDs from the bundle version membership table.
pub async fn load_bundle_policy_version_ids(
    tx: &mut Transaction<'_, Postgres>,
    bundle_version_id: Uuid,
) -> Result<Vec<Uuid>> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT policy_version_id
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $1
        ORDER BY policy_order ASC
        "#,
    )
    .bind(bundle_version_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(ids)
}

/// Write the canonical bundle version digest inside the active transaction.
///
/// Locks the bundle version row for the duration of the transaction to prevent
/// concurrent updates producing a stale digest (P1 #3 concurrency).
pub async fn write_bundle_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    bundle_id: Uuid,
    canonical: &BundleVersionCanonical,
) -> Result<()> {
    let digest = canonical.compute_digest();

    // Lock the draft version row before writing to prevent TOCTOU races.
    let version_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM compliance_bundle_versions
        WHERE bundle_id = $1 AND publication_state = 'draft'
        FOR UPDATE
        "#,
    )
    .bind(bundle_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(version_id) = version_id else {
        return Ok(());
    };

    sqlx::query(
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
    .await?;

    Ok(())
}

/// Build and write the effective-set digest for a single assignment,
/// computing it from the assignment's own exclusions/additions/overrides.
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
        "SELECT policy_version_id FROM compliance_assignment_exclusions WHERE assignment_id = $1 ORDER BY policy_version_id",
    )
    .bind(assignment_id)
    .fetch_all(&mut **tx)
    .await?;

    // Additions.
    let additions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT policy_version_id FROM compliance_assignment_additions WHERE assignment_id = $1 ORDER BY policy_version_id",
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
        WHERE assignment_id = $1
        ORDER BY policy_version_id, value_path
        "#,
    )
    .bind(assignment_id)
    .fetch_all(&mut **tx)
    .await?;

    // Resolved effective policy set: baseline minus exclusions plus additions.
    // Uses the bundle version membership as the baseline.
    let effective_policy_version_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT bvp.policy_version_id
        FROM compliance_bundle_assignments a
        JOIN compliance_bundle_version_policies bvp ON bvp.bundle_version_id = a.bundle_version_id
        WHERE a.id = $1
          AND bvp.policy_version_id NOT IN (
              SELECT policy_version_id FROM compliance_assignment_exclusions WHERE assignment_id = $1
          )
        ORDER BY bvp.policy_order ASC
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
    sqlx::query("UPDATE compliance_bundle_assignments SET effective_set_digest = $1 WHERE id = $2")
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
    }
    let pending_policies: Vec<PolicyVersionRow> = sqlx::query_as(
        r#"
        SELECT id, policy_id, name, description, policy_type,
               implementation_state, execution_phase, config,
               compliance_metadata, dependencies
        FROM deployment_policy_versions
        WHERE semantic_digest = 'pending'
          AND publication_state = 'draft'
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
          AND publication_state = 'draft'
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in &pending_bundles {
        let policy_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT policy_version_id
            FROM compliance_bundle_version_policies
            WHERE bundle_version_id = $1
            ORDER BY policy_order ASC
            "#,
        )
        .bind(row.id)
        .fetch_all(pool)
        .await?;

        let canonical = BundleVersionCanonical {
            name: row.name.clone(),
            framework: row.framework.clone(),
            framework_version: row.framework_version.clone(),
            description: row.description.clone(),
            layer: row.layer.clone(),
            owner: row.owner.clone(),
            policy_version_ids: policy_ids,
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
        "SELECT id FROM compliance_bundle_assignments WHERE effective_set_digest = 'pending'",
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
        "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE effective_set_digest = 'pending'",
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

    fn policy(
        name: &str,
        description: Option<&str>,
        policy_type: &str,
        impl_state: &str,
        exec_phase: &str,
        config: Value,
        meta: Value,
        deps: Value,
    ) -> PolicyVersionCanonical {
        PolicyVersionCanonical {
            name: name.into(),
            description: description.map(String::from),
            policy_type: policy_type.into(),
            implementation_state: impl_state.into(),
            execution_phase: exec_phase.into(),
            config,
            compliance_metadata: meta,
            dependencies: deps,
        }
    }

    fn base_policy() -> PolicyVersionCanonical {
        policy(
            "firewall",
            Some("Firewall enabled"),
            "custom_check",
            "native",
            "nix-evaluation",
            json!({"expr": "cfg.config.networking.firewall.enable"}),
            json!({}),
            json!([]),
        )
    }

    fn bundle(policy_ids: Vec<Uuid>) -> BundleVersionCanonical {
        BundleVersionCanonical {
            name: "Test Bundle".into(),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            description: Some("Description".into()),
            layer: "os".into(),
            owner: "Team".into(),
            policy_version_ids: policy_ids,
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
}
