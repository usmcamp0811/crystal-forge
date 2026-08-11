//! Database queries for deployment policy management.

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::api::models::DeletionEligibility;
use crate::compliance::digest::{PolicyVersionCanonical, write_policy_version_digest};
use crate::compliance::mappings::{
    initial_policy_metadata, merge_classification_into_metadata, merge_policy_mappings,
};
use crate::models::deployment_policies::{
    CreateDeploymentPolicyRequest, DeploymentPolicyRecord, UpdateDeploymentPolicyRequest,
};
use crate::queries::compliance::ensure_policy_draft;
use crate::queries::deletion::{blocker, eligibility};

/// List deployment policies with pagination
pub async fn list_deployment_policies(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<Vec<DeploymentPolicyRecord>> {
    let policies = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT id, name, description, policy_type, config, enabled, created_at, updated_at
        FROM deployment_policies
        ORDER BY name ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to list deployment policies")?;

    Ok(policies)
}

/// List all enabled deployment policies for evaluator execution.
/// Deprecated in favour of [`list_enabled_policies_for_flake`] which scopes
/// results to the environments that actually contain systems from the flake
/// being evaluated. Kept for CVE-policy loading (no flake context needed).
pub async fn list_enabled_deployment_policies(
    pool: &PgPool,
) -> Result<Vec<DeploymentPolicyRecord>> {
    let policies = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT id, name, description, policy_type, config, enabled, created_at, updated_at
        FROM deployment_policies
        WHERE enabled = true
        ORDER BY name ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to list enabled deployment policies")?;

    Ok(policies)
}

/// List enabled deployment policies that are assigned to at least one
/// environment which contains an active system belonging to `flake_id`.
///
/// This is the correct scope for Nix-eval policy checks: a policy that is
/// only assigned to environments unrelated to this flake should not block
/// builds for systems in other environments.
pub async fn list_enabled_policies_for_flake(
    pool: &PgPool,
    flake_id: i32,
) -> Result<Vec<DeploymentPolicyRecord>> {
    let policies = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT DISTINCT dp.id, dp.name, dp.description, dp.policy_type,
                        dp.config, dp.enabled, dp.created_at, dp.updated_at
        FROM deployment_policies dp
        WHERE dp.enabled = true
          AND (
              -- Policy is attached to an environment that has at least one
              -- active system from this flake.
              EXISTS (
                  SELECT 1
                  FROM environment_policies ep
                  JOIN systems s ON s.environment_id = ep.environment_id
                  WHERE ep.policy_id = dp.id
                    AND s.flake_id   = $1
                    AND s.is_active  = TRUE
              )
              OR
              -- Policy is attached directly to an active system from this flake.
              EXISTS (
                  SELECT 1
                  FROM system_policies sp
                  JOIN systems s ON s.id = sp.system_id
                  WHERE sp.policy_id = dp.id
                    AND s.flake_id   = $1
                    AND s.is_active  = TRUE
              )
          )
        ORDER BY dp.name ASC
        "#,
    )
    .bind(flake_id)
    .fetch_all(pool)
    .await
    .context("Failed to list enabled deployment policies for flake")?;

    Ok(policies)
}

/// A raw row returned by [`list_policy_rows_by_configuration_for_flake`].
/// Each row associates one enabled policy record with the NixOS configuration
/// name of the system that holds the assignment (via environment or directly).
#[derive(Debug, sqlx::FromRow)]
pub struct ConfigPolicyRow {
    /// NixOS configuration name: `COALESCE(NULLIF(BTRIM(system_configuration_name), ''), hostname)`.
    pub configuration_name: String,
    /// Stable policy UUID — used as the sort/deduplicate key.
    pub policy_id: Uuid,
    /// Policy name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Policy type string (e.g. `"require_cf_agent"`, `"require_packages"`).
    pub policy_type: String,
    /// Policy configuration JSON.
    pub config: serde_json::Value,
    /// Whether the policy is enabled (always `true` here due to the WHERE clause).
    pub enabled: bool,
    /// Creation timestamp.
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    /// Last-updated timestamp.
    pub updated_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
}

impl ConfigPolicyRow {
    /// Convert this row into a `DeploymentPolicyRecord` for use with the
    /// existing `parse_deployment_policy_record` helper.
    pub fn as_policy_record(&self) -> DeploymentPolicyRecord {
        DeploymentPolicyRecord {
            id: self.policy_id,
            name: self.name.clone(),
            description: self.description.clone(),
            policy_type: self.policy_type.clone(),
            config: self.config.clone(),
            enabled: self.enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Load enabled deployment policies scoped to the active systems in `flake_id`,
/// returning one row per (configuration_name, policy) pair.
///
/// This is the data source for building a [`PoliciesByConfiguration`] map.
/// Converting the rows to parsed `DeploymentPolicy` values and handling
/// conflict detection is done in the calling layer (`server/mod.rs`) which
/// has access to `parse_deployment_policy_record`.
///
/// The UNION of `environment_policies` and `system_policies` is intentional:
/// assigning the same policy through both sources must not produce two rows.
/// Rows are ordered by `(configuration_name, policy_id)` for deterministic
/// processing.
///
/// Inactive systems (`is_active = FALSE`) are excluded so that stale
/// registrations cannot pollute the policy set for current evaluations.
pub async fn list_policy_rows_by_configuration_for_flake(
    pool: &PgPool,
    flake_id: i32,
) -> Result<Vec<ConfigPolicyRow>> {
    let rows = sqlx::query_as::<_, ConfigPolicyRow>(
        r#"
        WITH scoped_systems AS (
            SELECT
                s.id AS system_id,
                s.environment_id,
                COALESCE(
                    NULLIF(BTRIM(s.system_configuration_name), ''),
                    s.hostname
                ) AS configuration_name
            FROM systems s
            WHERE s.flake_id = $1
              AND s.is_active = TRUE
        ),
        assigned_policy_ids AS (
            -- Via environment assignment
            SELECT ss.configuration_name, ep.policy_id
            FROM scoped_systems ss
            JOIN environment_policies ep ON ep.environment_id = ss.environment_id

            UNION

            -- Via direct system assignment
            SELECT ss.configuration_name, sp.policy_id
            FROM scoped_systems ss
            JOIN system_policies sp ON sp.system_id = ss.system_id
        )
        SELECT
            api.configuration_name,
            dp.id          AS policy_id,
            dp.name,
            dp.description,
            dp.policy_type,
            dp.config,
            dp.enabled,
            dp.created_at,
            dp.updated_at
        FROM assigned_policy_ids api
        JOIN deployment_policies dp ON dp.id = api.policy_id
        WHERE dp.enabled = TRUE
        ORDER BY api.configuration_name, dp.id
        "#,
    )
    .bind(flake_id)
    .fetch_all(pool)
    .await
    .context("Failed to list per-configuration policy rows for flake")?;

    Ok(rows)
}

/// Load the active configuration names for a flake (whether or not they have
/// assigned policies).  Used alongside `list_policy_rows_by_configuration_for_flake`
/// so we can detect registered configurations with zero policies.
pub async fn list_registered_configuration_names_for_flake(
    pool: &PgPool,
    flake_id: i32,
) -> Result<Vec<String>> {
    let names = sqlx::query_scalar::<_, String>(
        r#"
        SELECT COALESCE(NULLIF(BTRIM(system_configuration_name), ''), hostname)
        FROM systems
        WHERE flake_id = $1 AND is_active = TRUE
        ORDER BY 1
        "#,
    )
    .bind(flake_id)
    .fetch_all(pool)
    .await
    .context("Failed to list registered configuration names for flake")?;

    Ok(names)
}

/// Count total deployment policies (for pagination metadata)
pub async fn count_deployment_policies(pool: &PgPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deployment_policies")
        .fetch_one(pool)
        .await
        .context("Failed to count deployment policies")?;

    Ok(count)
}

/// Get a deployment policy by ID
pub async fn get_deployment_policy_by_id(
    pool: &PgPool,
    policy_id: &Uuid,
) -> Result<Option<DeploymentPolicyRecord>> {
    let policy = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT id, name, description, policy_type, config, enabled, created_at, updated_at
        FROM deployment_policies
        WHERE id = $1
        "#,
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch deployment policy by ID")?;

    Ok(policy)
}

/// Fetch a deployment policy record resolved to an exact version.
pub async fn get_deployment_policy_by_version(
    pool: &PgPool,
    policy_version_id: &Uuid,
) -> Result<Option<DeploymentPolicyRecord>> {
    let row = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        SELECT dp.id, dp.name, dp.description, dp.policy_type,
               COALESCE(pv.config, dp.config) AS config,
               dp.enabled, dp.created_at, dp.updated_at
          FROM deployment_policy_versions pv
          JOIN deployment_policies dp ON dp.id = pv.policy_id
         WHERE pv.id = $1
        "#,
    )
    .bind(policy_version_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch deployment policy by version ID")?;

    Ok(row)
}

/// Fetch deployment policy records for exact version IDs in one query.
///
/// Evaluation and deployment resolve policies per system, but many systems
/// share the same policy versions. Loading those versions as a batch avoids a
/// policy-version query for every system/policy pair.
pub async fn get_deployment_policies_by_versions(
    pool: &PgPool,
    policy_version_ids: &[Uuid],
) -> Result<HashMap<Uuid, DeploymentPolicyRecord>> {
    if policy_version_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            Option<String>,
            String,
            serde_json::Value,
            bool,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        r#"
        SELECT pv.id, dp.id, dp.name, dp.description, dp.policy_type,
               COALESCE(pv.config, dp.config) AS config,
               dp.enabled, dp.created_at, dp.updated_at
          FROM deployment_policy_versions pv
          JOIN deployment_policies dp ON dp.id = pv.policy_id
         WHERE pv.id = ANY($1)
        "#,
    )
    .bind(policy_version_ids)
    .fetch_all(pool)
    .await
    .context("Failed to fetch deployment policies by version IDs")?;

    Ok(rows
        .into_iter()
        .map(
            |(
                version_id,
                id,
                name,
                description,
                policy_type,
                config,
                enabled,
                created_at,
                updated_at,
            )| {
                (
                    version_id,
                    DeploymentPolicyRecord {
                        id,
                        name,
                        description,
                        policy_type,
                        config,
                        enabled,
                        created_at,
                        updated_at,
                    },
                )
            },
        )
        .collect())
}

/// Create a new deployment policy.
///
/// Runs entirely within a transaction. The SQL trigger creates the draft
/// version row with `semantic_digest = 'pending'`; within the same transaction
/// we compute the real Rust-canonical digest and persist it. A digest failure
/// rolls back the entire insert.
pub async fn create_deployment_policy(
    pool: &PgPool,
    request: &CreateDeploymentPolicyRequest,
) -> Result<DeploymentPolicyRecord> {
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    let policy = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        INSERT INTO deployment_policies (name, description, policy_type, config, enabled)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, description, policy_type, config, enabled, created_at, updated_at
        "#,
    )
    .bind(&request.name)
    .bind(&request.description)
    .bind(&request.policy_type)
    .bind(&request.config)
    .bind(request.enabled.unwrap_or(true))
    .fetch_one(&mut *tx)
    .await
    .context("Failed to create deployment policy")?;

    // Compute and persist the canonical digest before committing.
    // New policies created via the legacy API have no opaque XML.
    // Build compliance_metadata containing any supplied SRG/CCI mappings.
    let srg_ids_opt: Option<&[String]> = if request.srg_ids.is_empty() {
        None
    } else {
        Some(&request.srg_ids)
    };
    let cci_ids_opt: Option<&[String]> = if request.cci_ids.is_empty() {
        None
    } else {
        Some(&request.cci_ids)
    };
    let base_metadata = initial_policy_metadata(srg_ids_opt, cci_ids_opt)
        .context("Failed to build compliance metadata for new policy")?;
    // Merge classification fields into the initial metadata.
    let compliance_metadata = merge_classification_into_metadata(
        &base_metadata,
        request.category.as_deref(),
        request.framework.as_deref(),
        request.severity.as_deref(),
        request.control_family.as_deref(),
        request.cmmc_level,
        request.cis_section.as_deref(),
        request.rationale.as_deref(),
    );

    let canonical = PolicyVersionCanonical {
        name: policy.name.clone(),
        description: policy.description.clone(),
        policy_type: policy.policy_type.clone(),
        implementation_state: "native".to_string(),
        execution_phase: "nix-evaluation".to_string(),
        config: policy.config.clone(),
        compliance_metadata,
        dependencies: serde_json::json!([]),
        opaque_xml_digest: None,
        enabled_by_default: Some(policy.enabled),
    };
    write_policy_version_digest(&mut tx, policy.id, &canonical)
        .await
        .context("Failed to write policy version digest")?;

    tx.commit()
        .await
        .context("Failed to commit policy creation")?;
    Ok(policy)
}

/// Update an existing deployment policy.
///
/// Loads the current draft version's rich semantic fields (implementation_state,
/// execution_phase, compliance_metadata, dependencies) before updating, so that
/// a plain name/config/enabled edit does not overwrite imported metadata (P1 #4).
///
/// Runs entirely within a transaction; a digest failure rolls back the update.
pub async fn update_deployment_policy(
    pool: &PgPool,
    policy_id: &Uuid,
    request: &UpdateDeploymentPolicyRequest,
    actor_id: Option<Uuid>,
) -> Result<Option<DeploymentPolicyRecord>> {
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    // Load the current draft version's rich fields before the lineage update,
    // so that updating the lineage cannot erase imported semantics (P1 #4).
    // Ensure a mutable draft exists (creates a derived draft from the published
    // version when needed). (P1 #2)
    let _draft_version_id = ensure_policy_draft(
        &mut tx,
        *policy_id,
        actor_id,
        None,
        crate::queries::compliance::PolicyDraftIntent::EnsureMutable,
    )
    .await
    .context("Failed to ensure policy draft version exists")?;

    // Load rich semantic fields from the current draft version row.
    // This is done AFTER ensure_policy_draft so the pointer is guaranteed valid.
    #[derive(sqlx::FromRow)]
    struct DraftVersionFields {
        implementation_state: String,
        execution_phase: String,
        compliance_metadata: serde_json::Value,
        dependencies: serde_json::Value,
        opaque_xml: Option<String>,
    }
    let draft_fields: Option<DraftVersionFields> = sqlx::query_as(
        r#"
        SELECT dpv.implementation_state, dpv.execution_phase,
               dpv.compliance_metadata, dpv.dependencies, dpv.opaque_xml
        FROM deployment_policies dp
        JOIN deployment_policy_versions dpv ON dpv.id = dp.current_draft_version_id
        WHERE dp.id = $1
          AND dpv.publication_state IN ('incomplete', 'draft', 'interim')
        "#,
    )
    .bind(policy_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to load policy draft version fields")?;

    let policy = sqlx::query_as::<_, DeploymentPolicyRecord>(
        r#"
        UPDATE deployment_policies
        SET
            name        = COALESCE($2, name),
            description = COALESCE($3, description),
            policy_type = COALESCE($4, policy_type),
            config      = COALESCE($5, config),
            enabled     = COALESCE($6, enabled)
        WHERE id = $1
        RETURNING id, name, description, policy_type, config, enabled, created_at, updated_at
        "#,
    )
    .bind(policy_id)
    .bind(&request.name)
    .bind(&request.description)
    .bind(&request.policy_type)
    .bind(&request.config)
    .bind(request.enabled)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to update deployment policy")?;

    if let Some(ref p) = policy {
        // Merge: preserve existing rich fields; update only what the legacy
        // request supports. Callers using the full version-aware API can update
        // implementation_state, execution_phase, etc. separately.
        let (impl_state, exec_phase, existing_meta, deps, opaque_xml) =
            if let Some(df) = draft_fields {
                (
                    df.implementation_state,
                    df.execution_phase,
                    df.compliance_metadata,
                    df.dependencies,
                    df.opaque_xml,
                )
            } else {
                // No prior draft version — new policy path, use defaults.
                (
                    "native".to_string(),
                    "nix-evaluation".to_string(),
                    serde_json::json!({}),
                    serde_json::json!([]),
                    None,
                )
            };

        // Merge SRG/CCI mappings into existing compliance_metadata, preserving
        // all other metadata keys (source fidelity, rationale, checks, etc.).
        let srg_opt = request.srg_ids.as_deref();
        let cci_opt = request.cci_ids.as_deref();
        let srg_cci_merged = merge_policy_mappings(&existing_meta, srg_opt, cci_opt)
            .context("Failed to merge SRG/CCI mappings")?;
        // Merge classification fields into the already-merged metadata.
        let merged_meta = merge_classification_into_metadata(
            &srg_cci_merged,
            request.category.as_deref(),
            request.framework.as_deref(),
            request.severity.as_deref(),
            request.control_family.as_deref(),
            request.cmmc_level,
            request.cis_section.as_deref(),
            request.rationale.as_deref(),
        );

        let canonical = PolicyVersionCanonical {
            name: p.name.clone(),
            description: p.description.clone(),
            policy_type: p.policy_type.clone(),
            implementation_state: impl_state,
            execution_phase: exec_phase,
            config: p.config.clone(),
            compliance_metadata: merged_meta,
            dependencies: deps,
            opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(opaque_xml.as_deref()),
            enabled_by_default: Some(p.enabled),
        };
        write_policy_version_digest(&mut tx, p.id, &canonical)
            .await
            .context("Failed to write policy version digest")?;
    }

    tx.commit()
        .await
        .context("Failed to commit policy update")?;
    Ok(policy)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDeleteOutcome {
    Deleted,
    NotFound,
    Blocked(DeletionEligibility),
}

/// Return every retained record that prevents deleting this policy lineage.
///
/// The caller owns the transaction so DELETE can hold the lineage lock from
/// preflight through removal; the public status endpoint uses the same helper
/// in a short-lived transaction.
async fn policy_deletion_eligibility_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    policy_id: &Uuid,
) -> Result<Option<DeletionEligibility>> {
    let policy: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, policy_type FROM deployment_policies WHERE id = $1 FOR UPDATE")
            .bind(policy_id)
            .fetch_optional(&mut **tx)
            .await
            .context("Failed to lock deployment policy")?;
    let Some((_id, policy_type)) = policy else {
        return Ok(None);
    };

    let immutable_versions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM deployment_policy_versions WHERE policy_id = $1 AND publication_state IN ('accepted', 'deprecated') ORDER BY created_at, id",
    )
    .bind(policy_id)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to check policy immutable history")?;

    let immutable_assignment_history: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM (
            SELECT e.assignment_version_id FROM compliance_assignment_exclusions e JOIN deployment_policy_versions pv ON pv.id = e.policy_version_id WHERE pv.policy_id = $1
            UNION SELECT a.assignment_version_id FROM compliance_assignment_additions a JOIN deployment_policy_versions pv ON pv.id = a.policy_version_id WHERE pv.policy_id = $1
            UNION SELECT o.assignment_version_id FROM compliance_assignment_value_overrides o JOIN deployment_policy_versions pv ON pv.id = o.policy_version_id WHERE pv.policy_id = $1
        ) AS assignment_history
        "#,
    )
    .bind(policy_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check immutable policy assignment history")?;
    let immutable_memberships: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT bvp.bundle_version_id FROM compliance_bundle_version_policies bvp JOIN deployment_policy_versions pv ON pv.id = bvp.policy_version_id JOIN compliance_bundle_versions bv ON bv.id = bvp.bundle_version_id WHERE pv.policy_id = $1 AND bv.publication_state IN ('accepted', 'deprecated') ORDER BY bvp.bundle_version_id",
    )
    .bind(policy_id)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to check immutable bundle memberships")?;
    let mutable_membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_version_policies bvp JOIN deployment_policy_versions pv ON pv.id = bvp.policy_version_id JOIN compliance_bundle_versions bv ON bv.id = bvp.bundle_version_id WHERE pv.policy_id = $1 AND bv.publication_state IN ('incomplete', 'draft', 'interim')",
    )
    .bind(policy_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check mutable draft memberships")?;
    let environment_assignment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM environment_policies WHERE policy_id = $1")
            .bind(policy_id)
            .fetch_one(&mut **tx)
            .await
            .context("Failed to check environment policy assignments")?;
    let system_assignment_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM system_policies WHERE policy_id = $1")
            .bind(policy_id)
            .fetch_one(&mut **tx)
            .await
            .context("Failed to check system policy assignments")?;

    let immutable_source_mapping_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_source_object_mappings m JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id LEFT JOIN compliance_bundle_versions bv ON bv.id = m.bundle_version_id WHERE pv.policy_id = $1 AND (pv.publication_state IN ('accepted', 'deprecated') OR bv.publication_state IN ('accepted', 'deprecated'))",
    )
    .bind(policy_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check immutable policy source mappings")?;
    let disposable_source_mapping_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_source_object_mappings m JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id LEFT JOIN compliance_bundle_versions bv ON bv.id = m.bundle_version_id WHERE pv.policy_id = $1 AND pv.publication_state IN ('incomplete', 'draft', 'interim') AND (bv.id IS NULL OR bv.publication_state IN ('incomplete', 'draft', 'interim'))",
    )
    .bind(policy_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check disposable policy source mappings")?;

    let mut blockers = Vec::new();
    if policy_type == "require_cf_agent" {
        blockers.push(blocker(
            "policy_core",
            "The core require_cf_agent policy cannot be permanently deleted.",
            false,
            None,
            Vec::new(),
        ));
    }
    if !immutable_versions.is_empty() {
        blockers.push(blocker(
            "policy_immutable_history",
            "This policy has accepted or deprecated history and cannot be permanently deleted.",
            false,
            None,
            immutable_versions,
        ));
    }
    if immutable_assignment_history > 0 {
        blockers.push(blocker("immutable_assignment_history", "This policy is referenced by immutable assignment history and cannot be permanently deleted.", false, Some(immutable_assignment_history), Vec::new()));
    }
    if !immutable_memberships.is_empty() {
        blockers.push(blocker(
            "immutable_bundle_membership",
            "This policy belongs to immutable bundle membership and cannot be permanently deleted.",
            false,
            None,
            immutable_memberships,
        ));
    }
    if mutable_membership_count > 0 {
        blockers.push(blocker(
            "mutable_draft_membership",
            "Draft bundle membership will be removed with this policy.",
            true,
            Some(mutable_membership_count),
            Vec::new(),
        ));
    }
    let assignment_count = environment_assignment_count + system_assignment_count;
    if assignment_count > 0 {
        blockers.push(blocker(
            "mutable_direct_assignment",
            "Direct environment or system assignments will be removed with this policy.",
            true,
            Some(assignment_count),
            Vec::new(),
        ));
    }
    if disposable_source_mapping_count > 0 {
        blockers.push(blocker(
            "disposable_source_mapping",
            "Draft-only source mappings will be removed with this policy.",
            true,
            Some(disposable_source_mapping_count),
            Vec::new(),
        ));
    }
    if immutable_source_mapping_count > 0 {
        blockers.push(blocker(
            "immutable_source_mapping",
            "This policy has retained source mappings and cannot be permanently deleted.",
            false,
            Some(immutable_source_mapping_count),
            Vec::new(),
        ));
    }
    Ok(Some(eligibility(blockers)))
}

pub async fn policy_deletion_eligibility(
    pool: &PgPool,
    policy_id: &Uuid,
) -> Result<Option<DeletionEligibility>> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin policy deletion preflight")?;
    let result = policy_deletion_eligibility_in_transaction(&mut tx, policy_id).await;
    tx.rollback().await.ok();
    result
}

/// Delete a policy lineage only when no immutable history or reference would
/// be destroyed. The lineage row is locked for the full eligibility check and
/// delete; the FK guards remain defense in depth.
pub async fn delete_deployment_policy(
    pool: &PgPool,
    policy_id: &Uuid,
) -> Result<PolicyDeleteOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin policy deletion")?;

    let Some(eligibility) = policy_deletion_eligibility_in_transaction(&mut tx, policy_id).await?
    else {
        tx.rollback().await.ok();
        return Ok(PolicyDeleteOutcome::NotFound);
    };
    if !eligibility.eligible {
        tx.rollback().await.ok();
        return Ok(PolicyDeleteOutcome::Blocked(eligibility));
    }

    sqlx::query(
        "DELETE FROM compliance_source_object_mappings m \
         WHERE m.policy_version_id IN ( \
             SELECT pv.id FROM deployment_policy_versions pv \
             WHERE pv.policy_id = $1 \
               AND pv.publication_state IN ('incomplete', 'draft', 'interim') \
         ) \
         AND ( \
             m.bundle_version_id IS NULL \
             OR NOT EXISTS ( \
                 SELECT 1 FROM compliance_bundle_versions bv \
                 WHERE bv.id = m.bundle_version_id \
                   AND bv.publication_state IN ('accepted', 'deprecated') \
             ) \
         )",
    )
    .bind(policy_id)
    .execute(&mut *tx)
    .await
    .context("Failed to remove disposable policy source mappings")?;
    sqlx::query("DELETE FROM compliance_bundle_version_policies bvp USING deployment_policy_versions pv, compliance_bundle_versions bv WHERE bvp.policy_version_id = pv.id AND bvp.bundle_version_id = bv.id AND pv.policy_id = $1 AND bv.publication_state IN ('incomplete', 'draft', 'interim')")
        .bind(policy_id).execute(&mut *tx).await.context("Failed to remove mutable draft bundle memberships")?;
    sqlx::query("DELETE FROM environment_policies WHERE policy_id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .context("Failed to remove mutable environment policy assignments")?;
    sqlx::query("DELETE FROM system_policies WHERE policy_id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .context("Failed to remove mutable system policy assignments")?;

    let deleted = sqlx::query("DELETE FROM deployment_policies WHERE id = $1")
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete deployment policy")?
        .rows_affected();

    if deleted != 1 {
        tx.rollback().await.ok();
        return Ok(PolicyDeleteOutcome::NotFound);
    }

    tx.commit()
        .await
        .context("Failed to commit deployment policy deletion")?;
    Ok(PolicyDeleteOutcome::Deleted)
}

/// Check if a policy name already exists (case-insensitive)
/// exclude_id: Optional policy ID to exclude from the check (for updates)
pub async fn check_policy_name_exists(
    pool: &PgPool,
    name: &str,
    exclude_id: Option<&Uuid>,
) -> Result<bool> {
    let count: i64 = match exclude_id {
        Some(id) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policies WHERE LOWER(name) = LOWER($1) AND id != $2",
        )
        .bind(name)
        .bind(id)
        .fetch_one(pool)
        .await
        .context("Failed to check policy name existence")?,
        None => sqlx::query_scalar(
            "SELECT COUNT(*) FROM deployment_policies WHERE LOWER(name) = LOWER($1)",
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .context("Failed to check policy name existence")?,
    };

    Ok(count > 0)
}

/// Check if a policy with equivalent semantic content already exists.
///
/// A duplicate is defined as same policy_type and same config JSON payload.
/// exclude_id: Optional policy ID to exclude from the check (for updates).
pub async fn check_policy_content_exists(
    pool: &PgPool,
    policy_type: &str,
    config: &serde_json::Value,
    exclude_id: Option<&Uuid>,
) -> Result<bool> {
    let count: i64 = match exclude_id {
        Some(id) => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM deployment_policies
                WHERE policy_type = $1
                  AND config = $2
                  AND id != $3
                "#,
        )
        .bind(policy_type)
        .bind(config)
        .bind(id)
        .fetch_one(pool)
        .await
        .context("Failed to check policy content existence")?,
        None => sqlx::query_scalar(
            r#"
                SELECT COUNT(*)
                FROM deployment_policies
                WHERE policy_type = $1
                  AND config = $2
                "#,
        )
        .bind(policy_type)
        .bind(config)
        .fetch_one(pool)
        .await
        .context("Failed to check policy content existence")?,
    };

    Ok(count > 0)
}

/// Count distinct active systems that inherit each deployment policy through
/// either their environment (environment_policies) or direct system assignment (system_policies).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PolicySystemCount {
    pub policy_id: Uuid,
    pub system_count: i64,
}

pub async fn count_systems_for_all_policies(pool: &PgPool) -> Result<Vec<PolicySystemCount>> {
    let rows = sqlx::query_as::<_, PolicySystemCount>(
        r#"
        WITH policy_systems AS (
            SELECT ep.policy_id, s.id AS system_id
            FROM environment_policies ep
            JOIN systems s ON s.environment_id = ep.environment_id AND s.is_active = TRUE
            UNION
            SELECT sp.policy_id, s.id AS system_id
            FROM system_policies sp
            JOIN systems s ON s.id = sp.system_id AND s.is_active = TRUE
        )
        SELECT dp.id AS policy_id, COUNT(DISTINCT ps.system_id) AS system_count
        FROM deployment_policies dp
        LEFT JOIN policy_systems ps ON ps.policy_id = dp.id
        GROUP BY dp.id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to count systems per policy")?;
    Ok(rows)
}

/// Count total NixOS derivations (systems) in the fleet.
pub async fn count_nixos_derivations(pool: &PgPool) -> Result<i64> {
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM derivations WHERE derivation_type = 'nixos'"#)
            .fetch_one(pool)
            .await
            .context("Failed to count NixOS derivations")?;
    Ok(count)
}

/// Check if a policy is in use by any environments or systems
pub async fn check_policy_in_use(pool: &PgPool, policy_id: &Uuid) -> Result<bool> {
    let env_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM environment_policies WHERE policy_id = $1")
            .bind(policy_id)
            .fetch_one(pool)
            .await
            .context("Failed to check environment_policies")?;

    if env_count > 0 {
        return Ok(true);
    }

    let system_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM system_policies WHERE policy_id = $1")
            .bind(policy_id)
            .fetch_one(pool)
            .await
            .context("Failed to check system_policies")?;

    Ok(system_count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;
    use sqlx::migrate::Migrator;
    use std::fs;
    use std::path::PathBuf;

    static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

    // Note: These tests would require a test database setup
    // For now, they serve as documentation of expected behavior

    #[test]
    fn test_query_compilation() {
        // This test ensures the SQL queries compile correctly
        // Actual database tests would require sqlx test fixtures
    }

    // ── DB-level tests for list_policy_rows_by_configuration_for_flake ────
    //
    // These tests require a live database and are marked #[ignore].
    // Run with:
    //   CRYSTAL_FORGE_TEST_DATABASE_URL=... cargo test -p cf-server --lib \
    //     policies_by_configuration -- --ignored --test-threads=1

    async fn get_test_pool() -> sqlx::PgPool {
        let url = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .expect("CRYSTAL_FORGE_TEST_DATABASE_URL must be set");
        sqlx::PgPool::connect(&url)
            .await
            .expect("connect to test DB")
    }

    fn admin_database_url() -> String {
        let url = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect("CRYSTAL_FORGE_TEST_DATABASE_URL or DATABASE_URL must be set");
        let slash = url
            .rfind('/')
            .expect("database URL must contain a final /db segment");
        format!("{}postgres", &url[..slash + 1])
    }

    async fn create_temp_db() -> (sqlx::PgPool, sqlx::PgPool, String) {
        let admin_url = admin_database_url();
        let admin_pool = sqlx::PgPool::connect(&admin_url)
            .await
            .expect("connect to admin database");
        let db_name = format!("cf_m187_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
            .execute(&admin_pool)
            .await
            .expect("create temp database");

        let slash = admin_url
            .rfind('/')
            .expect("admin URL must contain final /db segment");
        let db_url = format!("{}{}", &admin_url[..slash + 1], db_name);
        let db_pool = sqlx::PgPool::connect(&db_url)
            .await
            .expect("connect to temp database");
        (admin_pool, db_pool, db_name)
    }

    async fn drop_temp_db(admin_pool: &sqlx::PgPool, db_name: &str) {
        let _ = sqlx::query(
            r#"
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE datname = $1
              AND pid <> pg_backend_pid()
            "#,
        )
        .bind(db_name)
        .execute(admin_pool)
        .await;

        let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name))
            .execute(admin_pool)
            .await;
    }

    fn migrations_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")
    }

    async fn apply_migrations_through(pool: &sqlx::PgPool, max_version: i64) {
        let dir = migrations_dir();
        let mut entries = fs::read_dir(&dir)
            .expect("read migrations dir")
            .map(|entry| entry.expect("dir entry"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".sql") {
                continue;
            }
            let version: i64 = name[..4].parse().expect("4-digit migration prefix");
            if version > max_version {
                continue;
            }
            let sql = fs::read_to_string(entry.path()).expect("read migration file");
            pool.execute(sql.as_str())
                .await
                .unwrap_or_else(|e| panic!("failed to apply migration {}: {e}", name));
        }
    }

    /// Insert a minimal flake, environment, system, policy, and assignment row,
    /// returning (flake_id, system_id, environment_id, policy_id).
    async fn insert_test_fixture(
        pool: &sqlx::PgPool,
        flake_name: &str,
        env_name: &str,
        system_hostname: &str,
        system_config_name: Option<&str>,
        policy_name: &str,
        policy_via_env: bool, // true = env assignment, false = direct system assignment
    ) -> (i32, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
        // Flake
        let flake_id: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(flake_name)
                .bind(format!("https://example.com/{}.git", flake_name))
                .fetch_one(pool)
                .await
                .expect("insert flake");

        // Environment
        let env_id: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(env_name)
                .fetch_one(pool)
                .await
                .expect("insert environment");

        // System
        let sys_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, system_configuration_name, flake_id, environment_id, is_active) \
             VALUES ($1, $2, $3, $4, TRUE) RETURNING id",
        )
        .bind(system_hostname)
        .bind(system_config_name)
        .bind(flake_id)
        .bind(env_id)
        .fetch_one(pool)
        .await
        .expect("insert system");

        // Policy
        let pol_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ($1, 'require_cf_agent', '{}', TRUE) RETURNING id",
        )
        .bind(policy_name)
        .fetch_one(pool)
        .await
        .expect("insert policy");

        if policy_via_env {
            sqlx::query(
                "INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)",
            )
            .bind(env_id)
            .bind(pol_id)
            .execute(pool)
            .await
            .expect("insert env policy");
        } else {
            sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
                .bind(sys_id)
                .bind(pol_id)
                .execute(pool)
                .await
                .expect("insert system policy");
        }

        (flake_id, sys_id, env_id, pol_id)
    }

    /// Publish a policy version in the trigger-safe order: clear the draft
    /// pointer, flip the version state, then set the published pointer, all in
    /// one transaction so the deferred lineage-pointer trigger passes at COMMIT.
    async fn publish_policy_version_row(
        pool: &sqlx::PgPool,
        version_id: &Uuid,
        state: &str,
    ) -> sqlx::Result<()> {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "UPDATE deployment_policies SET current_draft_version_id = NULL \
             WHERE current_draft_version_id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE deployment_policy_versions \
             SET publication_state = $1, published_at = CURRENT_TIMESTAMP \
             WHERE id = $2",
        )
        .bind(state)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 \
             WHERE id = (SELECT policy_id FROM deployment_policy_versions WHERE id = $1)",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    /// Publish a bundle version in the trigger-safe order, mirroring
    /// `publish_policy_version_row` for compliance_bundle_versions.
    async fn publish_bundle_version_row(
        pool: &sqlx::PgPool,
        version_id: &Uuid,
        state: &str,
    ) -> sqlx::Result<()> {
        let mut tx = pool.begin().await?;
        sqlx::query(
            "UPDATE compliance_bundles SET current_draft_version_id = NULL \
             WHERE current_draft_version_id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE compliance_bundle_versions \
             SET publication_state = $1, published_at = CURRENT_TIMESTAMP \
             WHERE id = $2",
        )
        .bind(state)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 \
             WHERE id = (SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1)",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn policies_by_configuration_different_flakes_do_not_leak() {
        let pool = get_test_pool().await;
        let mut tx = pool.begin().await.unwrap();

        // Flake A: policy "require-grafana"
        let fid_a: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('flake-a', 'https://a.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        let env_a: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('env-a') RETURNING id")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let sys_a: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, flake_id, environment_id, is_active) \
             VALUES ('alpha', $1, $2, TRUE) RETURNING id",
        )
        .bind(fid_a)
        .bind(env_a)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let pol_a: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('require-grafana', 'require_packages', '{\"packages\":[\"grafana\"]}', TRUE) RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env_a)
            .bind(pol_a)
            .execute(&mut *tx)
            .await
            .unwrap();

        // Flake B: policy "require-neovim"
        let fid_b: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('flake-b', 'https://b.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        let env_b: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('env-b') RETURNING id")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let sys_b: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, flake_id, environment_id, is_active) \
             VALUES ('beta', $1, $2, TRUE) RETURNING id",
        )
        .bind(fid_b)
        .bind(env_b)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let pol_b: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('require-neovim', 'require_packages', '{\"packages\":[\"neovim\"]}', TRUE) RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env_b)
            .bind(pol_b)
            .execute(&mut *tx)
            .await
            .unwrap();

        let _ = (sys_a, sys_b); // suppress unused warnings

        // Flake A's rows must contain only pol_a
        let rows_a = list_policy_rows_by_configuration_for_flake(&pool, fid_a)
            .await
            .unwrap();
        assert_eq!(rows_a.len(), 1, "flake A must have exactly 1 policy row");
        assert_eq!(
            rows_a[0].policy_id, pol_a,
            "flake A row must be grafana policy"
        );

        // Flake B's rows must contain only pol_b
        let rows_b = list_policy_rows_by_configuration_for_flake(&pool, fid_b)
            .await
            .unwrap();
        assert_eq!(rows_b.len(), 1, "flake B must have exactly 1 policy row");
        assert_eq!(
            rows_b[0].policy_id, pol_b,
            "flake B row must be neovim policy"
        );

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn policies_by_configuration_two_environments_same_flake() {
        let pool = get_test_pool().await;
        let mut tx = pool.begin().await.unwrap();

        let fid: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('multi-env-flake', 'https://multi.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();

        // Environment A: alpha, policy grafana
        let env_a: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('env-alpha') RETURNING id")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO systems (hostname, system_configuration_name, flake_id, environment_id, is_active) \
             VALUES ('alpha-host', 'alpha', $1, $2, TRUE)",
        ).bind(fid).bind(env_a).execute(&mut *tx).await.unwrap();
        let pol_grafana: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('grafana-policy', 'require_packages', '{\"packages\":[\"grafana\"]}', TRUE) RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env_a)
            .bind(pol_grafana)
            .execute(&mut *tx)
            .await
            .unwrap();

        // Environment B: beta, policy neovim
        let env_b: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('env-beta') RETURNING id")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO systems (hostname, system_configuration_name, flake_id, environment_id, is_active) \
             VALUES ('beta-host', 'beta', $1, $2, TRUE)",
        ).bind(fid).bind(env_b).execute(&mut *tx).await.unwrap();
        let pol_neovim: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('neovim-policy', 'require_packages', '{\"packages\":[\"neovim\"]}', TRUE) RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env_b)
            .bind(pol_neovim)
            .execute(&mut *tx)
            .await
            .unwrap();

        let rows = list_policy_rows_by_configuration_for_flake(&pool, fid)
            .await
            .unwrap();

        let alpha_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.configuration_name == "alpha")
            .collect();
        let beta_rows: Vec<_> = rows
            .iter()
            .filter(|r| r.configuration_name == "beta")
            .collect();

        assert_eq!(alpha_rows.len(), 1);
        assert_eq!(
            alpha_rows[0].policy_id, pol_grafana,
            "alpha must have grafana policy"
        );

        assert_eq!(beta_rows.len(), 1);
        assert_eq!(
            beta_rows[0].policy_id, pol_neovim,
            "beta must have neovim policy"
        );

        // Cross-check: alpha must NOT have neovim, beta must NOT have grafana
        assert!(
            !alpha_rows.iter().any(|r| r.policy_id == pol_neovim),
            "alpha must not see beta's policy"
        );
        assert!(
            !beta_rows.iter().any(|r| r.policy_id == pol_grafana),
            "beta must not see alpha's policy"
        );

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn policies_by_configuration_duplicate_assignment_deduplicated() {
        let pool = get_test_pool().await;
        let mut tx = pool.begin().await.unwrap();

        let fid: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('dedup-flake', 'https://dedup.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        let env: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('dedup-env') RETURNING id")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let sys: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, flake_id, environment_id, is_active) \
             VALUES ('dedup-host', $1, $2, TRUE) RETURNING id",
        )
        .bind(fid)
        .bind(env)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let pol: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('shared-policy', 'require_cf_agent', '{}', TRUE) RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();

        // Assign same policy BOTH via environment AND directly to system
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env)
            .bind(pol)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
            .bind(sys)
            .bind(pol)
            .execute(&mut *tx)
            .await
            .unwrap();

        let rows = list_policy_rows_by_configuration_for_flake(&pool, fid)
            .await
            .unwrap();

        // UNION in SQL must produce exactly 1 row (not 2)
        assert_eq!(
            rows.len(),
            1,
            "duplicate assignment must produce exactly one row (UNION deduplication)"
        );
        assert_eq!(rows[0].policy_id, pol);

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn policies_by_configuration_disabled_policy_excluded() {
        let pool = get_test_pool().await;
        let mut tx = pool.begin().await.unwrap();

        let fid: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('disabled-flake', 'https://disabled.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        let env: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO environments (name) VALUES ('disabled-env') RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO systems (hostname, flake_id, environment_id, is_active) VALUES ('host', $1, $2, TRUE)",
        ).bind(fid).bind(env).execute(&mut *tx).await.unwrap();
        let pol: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('disabled-policy', 'require_cf_agent', '{}', FALSE) RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env)
            .bind(pol)
            .execute(&mut *tx)
            .await
            .unwrap();

        let rows = list_policy_rows_by_configuration_for_flake(&pool, fid)
            .await
            .unwrap();
        assert!(
            rows.is_empty(),
            "disabled policy must not appear in results"
        );

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn policies_by_configuration_inactive_system_excluded() {
        let pool = get_test_pool().await;
        let mut tx = pool.begin().await.unwrap();

        let fid: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('inactive-flake', 'https://inactive.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        let env: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO environments (name) VALUES ('inactive-env') RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        // is_active = FALSE
        sqlx::query(
            "INSERT INTO systems (hostname, flake_id, environment_id, is_active) VALUES ('inactive-host', $1, $2, FALSE)",
        ).bind(fid).bind(env).execute(&mut *tx).await.unwrap();
        let pol: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('inactive-policy', 'require_cf_agent', '{}', TRUE) RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env)
            .bind(pol)
            .execute(&mut *tx)
            .await
            .unwrap();

        let rows = list_policy_rows_by_configuration_for_flake(&pool, fid)
            .await
            .unwrap();
        assert!(
            rows.is_empty(),
            "inactive system's policies must not appear"
        );

        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn policies_by_configuration_hostname_fallback() {
        let pool = get_test_pool().await;
        let mut tx = pool.begin().await.unwrap();

        let fid: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('hostname-flake', 'https://hostname.example') RETURNING id",
        ).fetch_one(&mut *tx).await.unwrap();
        let env: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO environments (name) VALUES ('hostname-env') RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        // system_configuration_name is NULL → key should be hostname
        let sys: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, system_configuration_name, flake_id, environment_id, is_active) \
             VALUES ('my-hostname', NULL, $1, $2, TRUE) RETURNING id",
        ).bind(fid).bind(env).fetch_one(&mut *tx).await.unwrap();
        let pol: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('hostname-policy', 'require_cf_agent', '{}', TRUE) RETURNING id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
            .bind(sys)
            .bind(pol)
            .execute(&mut *tx)
            .await
            .unwrap();

        let rows = list_policy_rows_by_configuration_for_flake(&pool, fid)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].configuration_name, "my-hostname",
            "key must fall back to hostname"
        );

        tx.rollback().await.unwrap();
    }

    // ── Migration 0187 upgrade-path regression tests ──────────────────────

    #[tokio::test]
    #[ignore = "requires a test database role with CREATE DATABASE privileges"]
    async fn migration_0187_transitions_legacy_require_cf_agent_to_disabled_historical_record() {
        let (admin_pool, pool, db_name) = create_temp_db().await;

        // Bring schema to the state immediately before 0187.
        apply_migrations_through(&pool, 186).await;

        // 1. A legacy require_cf_agent record exists and is enabled.
        let cf_policy_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM deployment_policies WHERE policy_type = 'require_cf_agent' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("canonical require_cf_agent policy should exist after 0089");

        let enabled_before: bool =
            sqlx::query_scalar("SELECT enabled FROM deployment_policies WHERE id = $1")
                .bind(cf_policy_id)
                .fetch_one(&pool)
                .await
                .expect("fetch enabled before migration 0187");
        assert!(
            enabled_before,
            "legacy row should still be enabled before 0187"
        );

        // Seed one environment assignment and one direct system assignment.
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url) VALUES ('m187-upgrade-flake', 'https://m187.example') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert flake");
        let env_id: uuid::Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ('m187-env') RETURNING id")
                .fetch_one(&pool)
                .await
                .expect("insert environment");
        let sys_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, flake_id, environment_id, is_active, derivation) \
             VALUES ('m187-host', 'test-public-key', $1, $2, TRUE, 'direct') RETURNING id",
        )
        .bind(flake_id)
        .bind(env_id)
        .fetch_one(&pool)
        .await
        .expect("insert system");

        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env_id)
            .bind(cf_policy_id)
            .execute(&pool)
            .await
            .expect("insert legacy env assignment");
        sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
            .bind(sys_id)
            .bind(cf_policy_id)
            .execute(&pool)
            .await
            .expect("insert legacy system assignment");

        // Seed an ordinary policy and assignments so we can prove they survive unchanged.
        let normal_policy_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ('m187-normal', 'require_packages', '{\"packages\":[\"grafana\"]}', TRUE) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("insert ordinary policy");
        sqlx::query("INSERT INTO environment_policies (environment_id, policy_id) VALUES ($1, $2)")
            .bind(env_id)
            .bind(normal_policy_id)
            .execute(&pool)
            .await
            .expect("insert ordinary env assignment");

        // 2. Migration 0187 applies successfully.
        let sql_0187 = fs::read_to_string(
            migrations_dir().join("0187_deduplicate_legacy_cf_agent_policy.sql"),
        )
        .expect("read migration 0187");
        pool.execute(sql_0187.as_str())
            .await
            .expect("migration 0187 should apply cleanly");

        // 3. Legacy policy record remains present but is disabled.
        let enabled_after: bool =
            sqlx::query_scalar("SELECT enabled FROM deployment_policies WHERE id = $1")
                .bind(cf_policy_id)
                .fetch_one(&pool)
                .await
                .expect("fetch enabled after migration 0187");
        assert!(
            !enabled_after,
            "legacy require_cf_agent row must be disabled"
        );

        // 4. Legacy environment/system assignments are removed.
        let env_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM environment_policies WHERE policy_id = $1")
                .bind(cf_policy_id)
                .fetch_one(&pool)
                .await
                .expect("count legacy env assignments");
        assert_eq!(env_count, 0, "legacy env assignments must be removed");
        let sys_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM system_policies WHERE policy_id = $1")
                .bind(cf_policy_id)
                .fetch_one(&pool)
                .await
                .expect("count legacy system assignments");
        assert_eq!(sys_count, 0, "legacy system assignments must be removed");

        // 5. Ordinary policy and assignments remain intact.
        let normal_enabled: bool =
            sqlx::query_scalar("SELECT enabled FROM deployment_policies WHERE id = $1")
                .bind(normal_policy_id)
                .fetch_one(&pool)
                .await
                .expect("fetch normal policy enabled");
        assert!(normal_enabled, "ordinary policy must remain enabled");
        let normal_env_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM environment_policies WHERE policy_id = $1")
                .bind(normal_policy_id)
                .fetch_one(&pool)
                .await
                .expect("count normal env assignment");
        assert_eq!(normal_env_count, 1, "ordinary env assignment must survive");

        // 6. Obsolete enabled constraint is gone; replacement disabled constraint exists.
        let old_constraint_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'deployment_policies_require_cf_agent_enabled')",
        )
        .fetch_one(&pool)
        .await
        .expect("check old constraint");
        assert!(
            !old_constraint_exists,
            "obsolete enabled constraint must be dropped"
        );
        let new_constraint_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'deployment_policies_require_cf_agent_disabled')",
        )
        .fetch_one(&pool)
        .await
        .expect("check new disabled constraint");
        assert!(
            new_constraint_exists,
            "replacement disabled constraint must exist"
        );

        // 7. Direct SQL re-enabling the legacy row must now fail.
        let reenable = sqlx::query("UPDATE deployment_policies SET enabled = TRUE WHERE id = $1")
            .bind(cf_policy_id)
            .execute(&pool)
            .await;
        assert!(
            reenable.is_err(),
            "direct SQL re-enable must be rejected by replacement constraint"
        );

        // 8. Ordinary non-CF-agent policies can still toggle normally.
        sqlx::query("UPDATE deployment_policies SET enabled = FALSE WHERE id = $1")
            .bind(normal_policy_id)
            .execute(&pool)
            .await
            .expect("disable ordinary policy");
        sqlx::query("UPDATE deployment_policies SET enabled = TRUE WHERE id = $1")
            .bind(normal_policy_id)
            .execute(&pool)
            .await
            .expect("re-enable ordinary policy");

        drop(pool);
        drop_temp_db(&admin_pool, &db_name).await;
    }

    #[tokio::test]
    #[ignore = "requires a test database role with CREATE DATABASE privileges"]
    async fn full_migration_chain_applies_cleanly_on_fresh_database_including_0210() {
        let (admin_pool, pool, db_name) = create_temp_db().await;

        MIGRATOR
            .run(&pool)
            .await
            .expect("full migration chain should apply successfully on a fresh database");

        // The deletion mapping guard migration must be recorded as applied.
        let success_210: Option<bool> =
            sqlx::query_scalar("SELECT success FROM _sqlx_migrations WHERE version = 210")
                .fetch_optional(&pool)
                .await
                .expect("query _sqlx_migrations for version 210");
        assert_eq!(
            success_210,
            Some(true),
            "migration 0210 must be recorded as successful after a fresh-chain apply"
        );

        drop(pool);
        drop_temp_db(&admin_pool, &db_name).await;
    }

    #[tokio::test]
    #[ignore = "requires a test database role with CREATE DATABASE privileges"]
    async fn source_mapping_guard_uses_all_referenced_version_states() {
        let (admin_pool, pool, db_name) = create_temp_db().await;
        MIGRATOR
            .run(&pool)
            .await
            .expect("apply migrations to isolated database");

        let artifact_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_source_artifacts (content, filename, media_type, sha256, parser_version) VALUES ($1, 'fixture.xml', 'application/xml', encode(digest($1, 'sha256'), 'hex'), 'test') RETURNING id",
        )
        .bind(b"<Benchmark/>".as_slice())
        .fetch_one(&pool)
        .await
        .expect("insert source artifact");

        // A-G: draft or immutable single references, then every two-reference
        // combination. A mapping is disposable only in A, C, and E.
        let cases = [
            ("A", Some("draft"), None, true),
            ("B", Some("accepted"), None, false),
            ("C", None, Some("draft"), true),
            ("D", None, Some("accepted"), false),
            ("E", Some("draft"), Some("draft"), true),
            ("F", Some("accepted"), Some("draft"), false),
            ("G", Some("draft"), Some("accepted"), false),
        ];

        for (case, policy_state, bundle_state, disposable) in cases {
            let policy_version_id = if let Some(policy_state) = policy_state {
                let policy_id: Uuid = sqlx::query_scalar(
                    "INSERT INTO deployment_policies (name, policy_type, config, enabled) VALUES ($1, 'custom_check', '{\"expression\": \"true\"}', false) RETURNING id",
                )
                .bind(format!("source-mapping-{case}-policy"))
                .fetch_one(&pool)
                .await
                .expect("insert policy");
                let version_id: Uuid = sqlx::query_scalar(
                    "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
                )
                .bind(policy_id)
                .fetch_one(&pool)
                .await
                .expect("load policy version");
                if policy_state != "draft" {
                    publish_policy_version_row(&pool, &version_id, policy_state)
                        .await
                        .expect("publish policy version");
                }
                Some(version_id)
            } else {
                None
            };

            let bundle_version_id = if let Some(bundle_state) = bundle_state {
                let bundle_id: Uuid = sqlx::query_scalar(
                    "INSERT INTO compliance_bundles (name, framework, layer, owner) VALUES ($1, 'test', 'fleet', 'test') RETURNING id",
                )
                .bind(format!("source-mapping-{case}-bundle"))
                .fetch_one(&pool)
                .await
                .expect("insert bundle");
                let version_id: Uuid = sqlx::query_scalar(
                    "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1",
                )
                .bind(bundle_id)
                .fetch_one(&pool)
                .await
                .expect("load bundle version");
                if bundle_state != "draft" {
                    publish_bundle_version_row(&pool, &version_id, bundle_state)
                        .await
                        .expect("publish bundle version");
                }
                Some(version_id)
            } else {
                None
            };

            let mapping_id: Uuid = sqlx::query_scalar(
                "INSERT INTO compliance_source_object_mappings (source_artifact_id, object_kind, source_identity, policy_version_id, bundle_version_id, fidelity) VALUES ($1, 'rule', $2, $3, $4, 'native_exact') RETURNING id",
            )
            .bind(artifact_id)
            .bind(format!("source-mapping-{case}"))
            .bind(policy_version_id)
            .bind(bundle_version_id)
            .fetch_one(&pool)
            .await
            .expect("insert source mapping");
            let result = sqlx::query("DELETE FROM compliance_source_object_mappings WHERE id = $1")
                .bind(mapping_id)
                .execute(&pool)
                .await;
            assert_eq!(
                result.is_ok(),
                disposable,
                "case {case} must classify mappings from version state, not a non-null UUID"
            );
        }

        drop(pool);
        drop_temp_db(&admin_pool, &db_name).await;
    }

    #[tokio::test]
    #[ignore = "requires CRYSTAL_FORGE_TEST_DATABASE_URL with migration 0210 applied"]
    async fn hard_delete_removes_disposable_draft_source_mapping() {
        let pool = get_test_pool().await;
        let suffix = uuid::Uuid::new_v4();
        let policy_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) VALUES ($1, 'custom_check', '{\"expression\": \"true\"}', false) RETURNING id",
        )
        .bind(format!("deletion-disposable-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("insert draft policy");
        let policy_version_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM deployment_policy_versions WHERE policy_id = $1 AND publication_state = 'draft'",
        )
        .bind(policy_id)
        .fetch_one(&pool)
        .await
        .expect("find draft policy version");
        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, layer, owner) VALUES ($1, 'test', 'fleet', 'test') RETURNING id",
        )
        .bind(format!("deletion-disposable-bundle-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("insert draft bundle");
        let bundle_version_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM compliance_bundle_versions WHERE bundle_id = $1 AND publication_state = 'draft'",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .expect("find draft bundle version");
        let source_artifact_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_source_artifacts (content, filename, media_type, sha256, parser_version) VALUES ($1, 'fixture.xml', 'application/xml', encode(digest($1, 'sha256'), 'hex'), 'test') RETURNING id",
        )
        .bind(format!("<Benchmark><!-- {suffix} --></Benchmark>").into_bytes())
        .fetch_one(&pool)
        .await
        .expect("insert source artifact");
        let mapping_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_source_object_mappings (source_artifact_id, object_kind, source_identity, policy_version_id, bundle_version_id, fidelity) VALUES ($1, 'rule', $2, $3, $4, 'native_exact') RETURNING id",
        )
        .bind(source_artifact_id)
        .bind(format!("rule-{suffix}"))
        .bind(policy_version_id)
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .expect("insert disposable mapping");

        let eligibility = policy_deletion_eligibility(&pool, &policy_id)
            .await
            .expect("load eligibility")
            .expect("policy exists");
        assert!(eligibility.eligible);
        assert!(
            eligibility
                .blockers
                .iter()
                .any(|blocker| blocker.kind == "disposable_source_mapping" && blocker.removable)
        );
        assert_eq!(
            delete_deployment_policy(&pool, &policy_id)
                .await
                .expect("delete policy"),
            PolicyDeleteOutcome::Deleted
        );
        let mapping_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM compliance_source_object_mappings WHERE id = $1)",
        )
        .bind(mapping_id)
        .fetch_one(&pool)
        .await
        .expect("check mapping cleanup");
        assert!(!mapping_exists);

        assert_eq!(
            crate::queries::compliance::delete_bundle(&pool, bundle_id)
                .await
                .expect("delete draft bundle"),
            crate::queries::compliance::BundleDeleteOutcome::Deleted
        );

        let bundle_policy_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) VALUES ($1, 'custom_check', '{\"expression\": \"true\"}', false) RETURNING id",
        )
        .bind(format!("deletion-disposable-bundle-policy-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("insert second draft policy");
        let bundle_policy_version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(bundle_policy_id)
        .fetch_one(&pool)
        .await
        .expect("find second draft policy version");
        let bundle_only_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, layer, owner) VALUES ($1, 'test', 'fleet', 'test') RETURNING id",
        )
        .bind(format!("deletion-disposable-bundle-only-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("insert second draft bundle");
        let bundle_only_version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1",
        )
        .bind(bundle_only_id)
        .fetch_one(&pool)
        .await
        .expect("find second draft bundle version");
        let bundle_mapping_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_source_object_mappings (source_artifact_id, object_kind, source_identity, policy_version_id, bundle_version_id, fidelity) VALUES ($1, 'rule', $2, $3, $4, 'native_exact') RETURNING id",
        )
        .bind(source_artifact_id)
        .bind(format!("bundle-rule-{suffix}"))
        .bind(bundle_policy_version_id)
        .bind(bundle_only_version_id)
        .fetch_one(&pool)
        .await
        .expect("insert disposable bundle mapping");

        let bundle_eligibility =
            crate::queries::compliance::bundle_deletion_eligibility(&pool, bundle_only_id)
                .await
                .expect("load bundle eligibility")
                .expect("bundle exists");
        assert!(bundle_eligibility.eligible);
        assert!(
            bundle_eligibility.blockers.iter().any(|blocker| {
                blocker.kind == "disposable_source_mapping" && blocker.removable
            })
        );
        assert_eq!(
            crate::queries::compliance::delete_bundle(&pool, bundle_only_id)
                .await
                .expect("delete second draft bundle"),
            crate::queries::compliance::BundleDeleteOutcome::Deleted
        );
        let bundle_mapping_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM compliance_source_object_mappings WHERE id = $1)",
        )
        .bind(bundle_mapping_id)
        .fetch_one(&pool)
        .await
        .expect("check bundle mapping cleanup");
        assert!(!bundle_mapping_exists);
        assert_eq!(
            delete_deployment_policy(&pool, &bundle_policy_id)
                .await
                .expect("delete second draft policy"),
            PolicyDeleteOutcome::Deleted
        );

        sqlx::query("DELETE FROM compliance_source_artifacts WHERE id = $1")
            .bind(source_artifact_id)
            .execute(&pool)
            .await
            .expect("clean up source artifact");
    }
}
