use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::{
    BundleVersionRequirementMembership, ComplianceGroupingScheme, DeletionEligibility,
};
use crate::compliance::digest::{
    BundleVersionCanonical, PolicyVersionCanonical, load_bundle_membership,
    refresh_bundle_requirement_digest, write_assignment_effective_set_digest,
    write_bundle_version_digest, write_policy_version_digest,
};
use crate::compliance::resolver::{
    EffectivePolicySource, ResolutionOutcome, resolve_system_effective_policies,
    resolve_systems_effective_policies_for_bundle_version_batch,
    resolve_systems_effective_policies_for_bundle_versions_batch,
};
use crate::queries::deletion::{blocker, eligibility};

pub async fn list_grouping_schemes(pool: &PgPool) -> Result<Vec<ComplianceGroupingScheme>> {
    let rows: Vec<(Uuid, String, Option<String>, Value, DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, name, description, groups, created_at, updated_at FROM compliance_grouping_schemes ORDER BY name, id",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id, name, description, groups, created_at, updated_at)| {
            let groups = serde_json::from_value(groups)
                .context("stored compliance grouping scheme has invalid groups")?;
            Ok(ComplianceGroupingScheme {
                id,
                name,
                description,
                groups,
                created_at,
                updated_at,
            })
        })
        .collect()
}

pub async fn create_grouping_scheme(
    pool: &PgPool,
    scheme: ComplianceGroupingScheme,
    actor_id: Uuid,
) -> Result<ComplianceGroupingScheme> {
    let groups = serde_json::to_value(&scheme.groups)?;
    let (created_at, updated_at): (DateTime<Utc>, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO compliance_grouping_schemes (id, name, description, groups, created_by) VALUES ($1, $2, $3, $4, $5) RETURNING created_at, updated_at",
    )
    .bind(scheme.id)
    .bind(&scheme.name)
    .bind(&scheme.description)
    .bind(groups)
    .bind(actor_id)
    .fetch_one(pool)
    .await?;
    Ok(ComplianceGroupingScheme {
        created_at,
        updated_at,
        ..scheme
    })
}

pub async fn update_grouping_scheme(
    pool: &PgPool,
    scheme: ComplianceGroupingScheme,
) -> Result<Option<ComplianceGroupingScheme>> {
    let groups = serde_json::to_value(&scheme.groups)?;
    let timestamps: Option<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
        "UPDATE compliance_grouping_schemes SET name = $2, description = $3, groups = $4, updated_at = CURRENT_TIMESTAMP WHERE id = $1 RETURNING created_at, updated_at",
    )
    .bind(scheme.id)
    .bind(&scheme.name)
    .bind(&scheme.description)
    .bind(groups)
    .fetch_optional(pool)
    .await?;
    Ok(
        timestamps.map(|(created_at, updated_at)| ComplianceGroupingScheme {
            created_at,
            updated_at,
            ..scheme
        }),
    )
}

pub async fn delete_grouping_scheme(pool: &PgPool, id: Uuid) -> Result<bool> {
    Ok(
        sqlx::query("DELETE FROM compliance_grouping_schemes WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected()
            == 1,
    )
}

// ─── Draft-lifecycle helpers ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDraftIntent {
    EnsureMutable,
    CreateExplicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleDraftIntent {
    EnsureMutable,
    CreateExplicit,
}

#[derive(Debug)]
pub enum BundleDraftDerivationError {
    NoPublishedSource,
    MutableDraftExists(Uuid),
}

impl std::fmt::Display for BundleDraftDerivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPublishedSource => {
                f.write_str("bundle has no published version to derive from")
            }
            Self::MutableDraftExists(id) => write!(f, "bundle already has mutable draft {id}"),
        }
    }
}

impl std::error::Error for BundleDraftDerivationError {}

#[derive(Debug)]
pub enum PolicyDraftDerivationError {
    NoPublishedSource,
    MutableDraftExists(Uuid),
}

impl std::fmt::Display for PolicyDraftDerivationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPublishedSource => {
                f.write_str("policy has no published version to derive from")
            }
            Self::MutableDraftExists(id) => write!(f, "policy already has mutable draft {id}"),
        }
    }
}

impl std::error::Error for PolicyDraftDerivationError {}

/// Ensure the bundle lineage has a mutable draft version.
///
/// If `current_draft_version_id` is already set and mutable, returns it.
/// If the lineage has only a published version, creates a new draft derived from
/// it (copies metadata and exact membership), sets
/// `derived_from_version_id`, records `actor_id` as the creating user,
/// updates the pointer, and returns the new version id.
/// Returns an error if neither pointer exists.
pub async fn ensure_bundle_draft(
    tx: &mut Transaction<'_, Postgres>,
    bundle_id: Uuid,
    actor_id: Option<Uuid>,
    requested_version: Option<&str>,
    intent: BundleDraftIntent,
) -> Result<Uuid> {
    #[derive(sqlx::FromRow)]
    struct BundlePointers {
        current_draft_version_id: Option<Uuid>,
        current_published_version_id: Option<Uuid>,
        draft_publication_state: Option<String>,
    }
    let pointers: BundlePointers = sqlx::query_as(
        r#"
        SELECT
            b.current_draft_version_id,
            b.current_published_version_id,
            bv.publication_state AS draft_publication_state
        FROM compliance_bundles b
        LEFT JOIN compliance_bundle_versions bv ON bv.id = b.current_draft_version_id
        WHERE b.id = $1
        FOR UPDATE OF b
        "#,
    )
    .bind(bundle_id)
    .fetch_one(&mut **tx)
    .await?;

    // EnsureMutable may reuse a mutable draft. CreateExplicit must first prove
    // that a published source exists, so draft-only lineages return 422.
    if intent == BundleDraftIntent::EnsureMutable {
        if let Some(draft_id) = pointers.current_draft_version_id {
            let state = pointers.draft_publication_state.as_deref().unwrap_or("");
            if matches!(state, "incomplete" | "draft" | "interim") {
                return Ok(draft_id);
            }
        }
    }

    // Load the published version to derive from.
    let published_id = pointers
        .current_published_version_id
        .ok_or(BundleDraftDerivationError::NoPublishedSource)?;

    if let Some(draft_id) = pointers.current_draft_version_id {
        let state = pointers.draft_publication_state.as_deref().unwrap_or("");
        if matches!(state, "incomplete" | "draft" | "interim") {
            return Err(BundleDraftDerivationError::MutableDraftExists(draft_id).into());
        }
    }

    #[derive(sqlx::FromRow)]
    struct PublishedBundleVersion {
        name: String,
        framework: String,
        framework_version: Option<String>,
        framework_version_id: Option<Uuid>,
        description: Option<String>,
        layer: String,
        owner: String,
        version: String,
    }
    let pub_ver: PublishedBundleVersion = sqlx::query_as(
        r#"
        SELECT name, framework, framework_version, framework_version_id, description, layer, owner, version
        FROM compliance_bundle_versions
        WHERE id = $1
        "#,
    )
    .bind(published_id)
    .fetch_one(&mut **tx)
    .await?;

    // Choose the next draft version string.
    let new_version = requested_version
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-draft", pub_ver.version));

    let new_draft_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_bundle_versions (
            bundle_id, version, name, framework, framework_version, framework_version_id,
            description, layer, owner, semantic_digest, derived_from_version_id,
            created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'pending', $10, $11)
        RETURNING id
        "#,
    )
    .bind(bundle_id)
    .bind(&new_version)
    .bind(&pub_ver.name)
    .bind(&pub_ver.framework)
    .bind(&pub_ver.framework_version)
    .bind(pub_ver.framework_version_id)
    .bind(&pub_ver.description)
    .bind(&pub_ver.layer)
    .bind(&pub_ver.owner)
    .bind(published_id)
    .bind(actor_id) // created_by
    .fetch_one(&mut **tx)
    .await?;

    // Copy exact membership from the published version to the new draft.
    sqlx::query(
        r#"
        INSERT INTO compliance_bundle_version_policies
            (bundle_version_id, policy_version_id, policy_order, selected)
        SELECT $1, policy_version_id, policy_order, selected
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $2
        "#,
    )
    .bind(new_draft_id)
    .bind(published_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO compliance_bundle_version_requirements
            (bundle_version_id, requirement_version_id, selected, requirement_order)
        SELECT $1, requirement_version_id, selected, requirement_order
        FROM compliance_bundle_version_requirements
        WHERE bundle_version_id = $2
        "#,
    )
    .bind(new_draft_id)
    .bind(published_id)
    .execute(&mut **tx)
    .await?;

    refresh_bundle_requirement_digest(tx, new_draft_id).await?;

    // Assignments are independent lineages scoped to a bundle lineage and target.
    // A draft bundle version must not duplicate or reactivate an assignment lineage;
    // active assignments remain bound to their accepted bundle version until an
    // explicit assignment-version transition rebinds them.

    // Update the lineage pointer (integrity trigger fires and validates).
    sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = $1 WHERE id = $2")
        .bind(new_draft_id)
        .bind(bundle_id)
        .execute(&mut **tx)
        .await?;

    let canonical = BundleVersionCanonical {
        name: pub_ver.name,
        framework: pub_ver.framework,
        framework_version: pub_ver.framework_version,
        description: pub_ver.description,
        layer: pub_ver.layer,
        owner: pub_ver.owner,
        members: load_bundle_membership(tx, new_draft_id).await?,
    };
    write_bundle_version_digest(tx, bundle_id, &canonical).await?;

    Ok(new_draft_id)
}

/// Ensure the policy lineage has a mutable draft version.
///
/// Same logic as `ensure_bundle_draft` but for deployment policies.
pub async fn ensure_policy_draft(
    tx: &mut Transaction<'_, Postgres>,
    policy_id: Uuid,
    actor_id: Option<Uuid>,
    requested_version: Option<&str>,
    intent: PolicyDraftIntent,
) -> Result<Uuid> {
    #[derive(sqlx::FromRow)]
    struct PolicyPointers {
        current_draft_version_id: Option<Uuid>,
        current_published_version_id: Option<Uuid>,
        draft_publication_state: Option<String>,
    }
    let pointers: PolicyPointers = sqlx::query_as(
        r#"
        SELECT
            dp.current_draft_version_id,
            dp.current_published_version_id,
            dpv.publication_state AS draft_publication_state
        FROM deployment_policies dp
        LEFT JOIN deployment_policy_versions dpv ON dpv.id = dp.current_draft_version_id
        WHERE dp.id = $1
        FOR UPDATE OF dp
        "#,
    )
    .bind(policy_id)
    .fetch_one(&mut **tx)
    .await?;

    if intent == PolicyDraftIntent::EnsureMutable {
        if let Some(draft_id) = pointers.current_draft_version_id {
            let state = pointers.draft_publication_state.as_deref().unwrap_or("");
            if matches!(state, "incomplete" | "draft" | "interim") {
                return Ok(draft_id);
            }
        }
    }

    let published_id = pointers
        .current_published_version_id
        .ok_or(PolicyDraftDerivationError::NoPublishedSource)?;

    if let Some(draft_id) = pointers.current_draft_version_id {
        let state = pointers.draft_publication_state.as_deref().unwrap_or("");
        if matches!(state, "incomplete" | "draft" | "interim") {
            return Err(PolicyDraftDerivationError::MutableDraftExists(draft_id).into());
        }
    }

    #[derive(sqlx::FromRow)]
    struct PubPolicyVersion {
        name: String,
        description: Option<String>,
        policy_type: String,
        implementation_state: String,
        execution_phase: String,
        config: Value,
        compliance_metadata: Value,
        dependencies: Value,
        opaque_xml: Option<String>,
        version: String,
        enabled_by_default: Option<bool>,
    }
    let pub_ver: PubPolicyVersion = sqlx::query_as(
        r#"
        SELECT name, description, policy_type, implementation_state, execution_phase,
               config, compliance_metadata, dependencies, opaque_xml, version,
               enabled_by_default
        FROM deployment_policy_versions
        WHERE id = $1
        "#,
    )
    .bind(published_id)
    .fetch_one(&mut **tx)
    .await?;

    let new_version = requested_version
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-draft", pub_ver.version));

    let new_draft_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO deployment_policy_versions (
            policy_id, version, name, description, policy_type,
            implementation_state, execution_phase, config,
            compliance_metadata, dependencies, opaque_xml,
            semantic_digest, derived_from_version_id, created_by, enabled_by_default
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'pending', $12, $13, $14)
        RETURNING id
        "#,
    )
    .bind(policy_id)
    .bind(&new_version)
    .bind(&pub_ver.name)
    .bind(&pub_ver.description)
    .bind(&pub_ver.policy_type)
    .bind(&pub_ver.implementation_state)
    .bind(&pub_ver.execution_phase)
    .bind(&pub_ver.config)
    .bind(&pub_ver.compliance_metadata)
    .bind(&pub_ver.dependencies)
    .bind(&pub_ver.opaque_xml)
    .bind(published_id)
    .bind(actor_id)
    .bind(&pub_ver.enabled_by_default)
    .fetch_one(&mut **tx)
    .await?;

    // Derived drafts inherit mapping semantics while receiving fresh row IDs.
    sqlx::query(
        r#"
        INSERT INTO policy_requirement_mappings (
            policy_version_id, requirement_version_id, relationship, coverage,
            rationale, provenance, source_artifact_id, trust_state, created_by
        )
        SELECT $1, requirement_version_id, relationship, coverage,
               rationale, provenance, source_artifact_id, trust_state, created_by
        FROM policy_requirement_mappings
        WHERE policy_version_id = $2
        "#,
    )
    .bind(new_draft_id)
    .bind(published_id)
    .execute(&mut **tx)
    .await?;

    sqlx::query("UPDATE deployment_policies SET current_draft_version_id = $1 WHERE id = $2")
        .bind(new_draft_id)
        .bind(policy_id)
        .execute(&mut **tx)
        .await?;

    let canonical = PolicyVersionCanonical {
        name: pub_ver.name,
        description: pub_ver.description,
        policy_type: pub_ver.policy_type,
        implementation_state: pub_ver.implementation_state,
        execution_phase: pub_ver.execution_phase,
        config: pub_ver.config,
        compliance_metadata: pub_ver.compliance_metadata,
        dependencies: pub_ver.dependencies,
        opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(pub_ver.opaque_xml.as_deref()),
        enabled_by_default: pub_ver.enabled_by_default,
    };
    write_policy_version_digest(tx, policy_id, &canonical).await?;

    Ok(new_draft_id)
}

// ─── Typed validation error ───────────────────────────────────────────────────

/// Validation failures for bundle create/update operations.
/// Using a typed error instead of string matching lets handlers return
/// predictable 400 responses regardless of future message changes.
#[derive(Debug)]
pub enum BundleValidationError {
    NameRequired,
    FrameworkRequired,
    EmptyBaseline,
    DuplicateRequirement(Uuid),
    RequirementNotFound(Uuid),
}

impl std::fmt::Display for BundleValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameRequired => f.write_str("Bundle name is required"),
            Self::FrameworkRequired => f.write_str("Framework is required"),
            Self::EmptyBaseline => f.write_str("At least one policy or requirement is required"),
            Self::DuplicateRequirement(id) => {
                write!(f, "Duplicate requirement version {id} in request")
            }
            Self::RequirementNotFound(id) => write!(f, "Requirement version {id} was not found"),
        }
    }
}

impl std::error::Error for BundleValidationError {}

/// Validate fields common to both create and update.
/// Returns an `anyhow::Error` wrapping a [`BundleValidationError`] so callers
/// can downcast to distinguish validation failures from infrastructure errors.
fn validate_bundle_request(
    name: &str,
    framework: &str,
    policy_ids: &[Uuid],
    requirement_version_ids: &[Uuid],
) -> Result<()> {
    if name.is_empty() {
        return Err(BundleValidationError::NameRequired.into());
    }
    if framework.is_empty() {
        return Err(BundleValidationError::FrameworkRequired.into());
    }
    if policy_ids.is_empty() && requirement_version_ids.is_empty() {
        return Err(BundleValidationError::EmptyBaseline.into());
    }
    let mut seen = std::collections::HashSet::new();
    for requirement_version_id in requirement_version_ids {
        if !seen.insert(requirement_version_id) {
            return Err(
                BundleValidationError::DuplicateRequirement(*requirement_version_id).into(),
            );
        }
    }
    Ok(())
}

async fn validate_requirement_versions(
    tx: &mut Transaction<'_, Postgres>,
    requirement_version_ids: &[Uuid],
) -> Result<()> {
    if requirement_version_ids.is_empty() {
        return Ok(());
    }
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_requirement_versions WHERE id = ANY($1)",
    )
    .bind(requirement_version_ids)
    .fetch_one(&mut **tx)
    .await?;
    if found != requirement_version_ids.len() as i64 {
        for requirement_version_id in requirement_version_ids {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM compliance_requirement_versions WHERE id = $1)",
            )
            .bind(requirement_version_id)
            .fetch_one(&mut **tx)
            .await?;
            if !exists {
                return Err(
                    BundleValidationError::RequirementNotFound(*requirement_version_id).into(),
                );
            }
        }
    }
    Ok(())
}

use crate::api::models::{
    BundleVersionPolicyMembership, ComplianceBundleSummary, ComplianceBundleSystemsResponse,
    ComplianceBundleVersionSummary, ComplianceControlEvidence, ComplianceControlStatus,
    ComplianceEnvironmentRef, ComplianceEvidenceArtifact, ComplianceEvidenceItem,
    ComplianceEvidenceResponse, ComplianceRollupTotals, ComplianceSystemRollup,
    CreateComplianceBundleRequest, PolicyVersionBundleUsage, PolicyVersionSystemUsage,
    PolicyVersionUsageResponse, UpdateComplianceBundleRequest,
};

/// Load the exact immutable policy versions selected by one bundle version.
pub async fn list_bundle_version_policy_membership(
    pool: &PgPool,
    bundle_version_id: Uuid,
) -> Result<Option<Vec<BundleVersionPolicyMembership>>> {
    let bundle_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM compliance_bundle_versions WHERE id = $1)",
    )
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await?;

    if !bundle_exists {
        return Ok(None);
    }

    let members = sqlx::query_as::<_, BundleVersionPolicyMembership>(
        r#"
        SELECT cbvp.policy_version_id,
               pv.policy_id AS policy_lineage_id,
               cbvp.policy_order,
               pv.name,
               pv.description,
               pv.policy_type,
               COALESCE(pv.enabled_by_default, true) AS enabled
        FROM compliance_bundle_version_policies cbvp
        JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
        WHERE cbvp.bundle_version_id = $1
          AND cbvp.selected = true
        ORDER BY cbvp.policy_order
        "#,
    )
    .bind(bundle_version_id)
    .fetch_all(pool)
    .await?;

    Ok(Some(members))
}

/// Return immutable bundle membership and assignment-resolved system usage for
/// one exact policy version. Bundle membership and effective usage are kept
/// separate because assignment overlays can exclude or add policy versions.
pub async fn load_policy_version_usage(
    pool: &PgPool,
    policy_version_id: Uuid,
) -> Result<Option<PolicyVersionUsageResponse>> {
    let policy_version_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM deployment_policy_versions WHERE id = $1)",
    )
    .bind(policy_version_id)
    .fetch_one(pool)
    .await?;
    if !policy_version_exists {
        return Ok(None);
    }

    let bundle_versions = sqlx::query_as::<_, PolicyVersionBundleUsage>(
        r#"
        SELECT b.id AS bundle_id,
               b.name AS bundle_name,
               bv.id AS bundle_version_id,
               bv.version AS bundle_version,
               bv.publication_state,
               bvp.policy_order,
               COALESCE(b.current_published_version_id = bv.id, false) AS is_current_published,
               COALESCE(b.current_draft_version_id = bv.id, false) AS is_current_draft
        FROM compliance_bundle_version_policies bvp
        JOIN compliance_bundle_versions bv ON bv.id = bvp.bundle_version_id
        JOIN compliance_bundles b ON b.id = bv.bundle_id
        WHERE bvp.policy_version_id = $1
          AND bvp.selected = true
        ORDER BY b.name, bv.created_at DESC, bv.id
        "#,
    )
    .bind(policy_version_id)
    .fetch_all(pool)
    .await
    .context("load exact policy-version bundle usage")?;

    #[derive(Debug, Clone, FromRow)]
    struct CandidateSystemUsage {
        system_id: Uuid,
        hostname: String,
        environment: Option<String>,
        bundle_id: Uuid,
        bundle_name: String,
        bundle_version_id: Uuid,
        bundle_version: String,
    }

    let candidates = sqlx::query_as::<_, CandidateSystemUsage>(
        r#"
        WITH candidate_assignments AS (
            SELECT a.bundle_id,
                   a.bundle_version_id,
                   a.scope_type,
                   a.environment_id,
                   a.system_id
            FROM compliance_bundle_assignments a
            WHERE a.active
              AND a.current_version_id IS NOT NULL
              AND (
                  EXISTS (
                      SELECT 1
                      FROM compliance_bundle_version_policies bvp
                      WHERE bvp.bundle_version_id = a.bundle_version_id
                        AND bvp.policy_version_id = $1
                        AND bvp.selected = true
                  )
                  OR EXISTS (
                      SELECT 1
                      FROM compliance_assignment_additions addition
                      WHERE addition.assignment_version_id = a.current_version_id
                        AND addition.policy_version_id = $1
                  )
              )
        )
        SELECT DISTINCT
               s.id AS system_id,
               s.hostname,
               e.name AS environment,
               b.id AS bundle_id,
               b.name AS bundle_name,
               bv.id AS bundle_version_id,
               bv.version AS bundle_version
        FROM candidate_assignments a
        JOIN compliance_bundles b ON b.id = a.bundle_id
        JOIN compliance_bundle_versions bv ON bv.id = a.bundle_version_id
        JOIN systems s ON
             (a.scope_type = 'system' AND a.system_id = s.id)
             OR (a.scope_type = 'environment' AND a.environment_id = s.environment_id)
        LEFT JOIN environments e ON e.id = s.environment_id
        ORDER BY b.name, bv.version, s.hostname, s.id
        "#,
    )
    .bind(policy_version_id)
    .fetch_all(pool)
    .await
    .context("load exact policy-version system candidates")?;

    let mut requests = std::collections::BTreeMap::<Uuid, Vec<Uuid>>::new();
    for candidate in &candidates {
        requests
            .entry(candidate.bundle_version_id)
            .or_default()
            .push(candidate.system_id);
    }
    let requests: Vec<_> = requests.into_iter().collect();
    let resolved = resolve_systems_effective_policies_for_bundle_versions_batch(pool, &requests)
        .await
        .context("resolve exact policy-version system usage")?;

    let mut systems = Vec::new();
    for candidate in candidates {
        let Some(ResolutionOutcome::Resolved(set)) =
            resolved.get(&(candidate.bundle_version_id, candidate.system_id))
        else {
            continue;
        };
        let Some(bundle_provenance) = set.policies.iter().find_map(|policy| {
            (policy.policy_version_id == policy_version_id)
                .then(|| {
                    policy.provenance.iter().find(|entry| {
                        entry.authoritative
                            && entry.bundle_id == Some(candidate.bundle_id)
                            && entry.bundle_version_id == Some(candidate.bundle_version_id)
                    })
                })
                .flatten()
        }) else {
            continue;
        };
        let source = match &bundle_provenance.source {
            EffectivePolicySource::Baseline => "baseline",
            EffectivePolicySource::Addition => "addition",
            EffectivePolicySource::LegacyDirect => "legacy_direct",
        };
        systems.push(PolicyVersionSystemUsage {
            system_id: candidate.system_id,
            hostname: candidate.hostname,
            environment: candidate.environment,
            bundle_id: candidate.bundle_id,
            bundle_name: candidate.bundle_name,
            bundle_version_id: candidate.bundle_version_id,
            bundle_version: candidate.bundle_version,
            source: source.to_string(),
            enforcement_mode: bundle_provenance.enforcement_mode.clone(),
        });
    }

    Ok(Some(PolicyVersionUsageResponse {
        policy_version_id,
        bundle_versions,
        systems,
    }))
}

pub async fn list_bundle_version_requirement_membership(
    pool: &PgPool,
    bundle_version_id: Uuid,
) -> Result<Option<Vec<BundleVersionRequirementMembership>>> {
    let bundle_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM compliance_bundle_versions WHERE id = $1)",
    )
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await?;
    if !bundle_exists {
        return Ok(None);
    }
    let members = sqlx::query_as::<_, BundleVersionRequirementMembership>(
        r#"
        SELECT bvr.requirement_version_id,
               rv.requirement_id,
               fv.framework_id,
               rv.framework_version_id,
               f.name AS framework_name,
               fv.version AS framework_version,
               rv.external_id,
               rv.title,
               rv.kind,
               bvr.selected,
               bvr.requirement_order
        FROM compliance_bundle_version_requirements bvr
        JOIN compliance_requirement_versions rv ON rv.id = bvr.requirement_version_id
        JOIN compliance_framework_versions fv ON fv.id = rv.framework_version_id
        JOIN compliance_frameworks f ON f.id = fv.framework_id
        WHERE bvr.bundle_version_id = $1
        ORDER BY bvr.requirement_order, bvr.requirement_version_id
        "#,
    )
    .bind(bundle_version_id)
    .fetch_all(pool)
    .await?;
    Ok(Some(members))
}

#[derive(Debug, FromRow)]
struct BundleRow {
    id: Uuid,
    name: String,
    framework: String,
    version: String,
    description: Option<String>,
    layer: String,
    owner: String,
    last_review: Option<DateTime<Utc>>,
    policy_ids: Vec<Uuid>,
    env_ids: Vec<Uuid>,
    env_names: Vec<String>,
    env_colors: Vec<String>,
    policy_count: i64,
    requirement_count: i64,
    control_count: i64,
    environment_count: i64,
    active_assignment_count: i64,
    current_draft_version_id: Option<Uuid>,
    current_published_version_id: Option<Uuid>,
    current_draft_version: Option<String>,
    current_published_version: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct SystemRow {
    pub id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub health_status: String,
    pub critical_cve_count: i32,
    pub high_cve_count: i32,
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct PolicyRow {
    pub id: Uuid,
    #[sqlx(default)]
    pub bundle_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub config: Value,
    pub enabled: bool,
    #[sqlx(default)]
    pub compliance_metadata: Value,
}

fn bundle_from_row(row: BundleRow) -> ComplianceBundleSummary {
    let required_envs = row
        .env_ids
        .into_iter()
        .zip(row.env_names)
        .zip(row.env_colors)
        .map(|((id, name), color_hex)| ComplianceEnvironmentRef {
            id,
            name,
            color_hex,
        })
        .collect();

    ComplianceBundleSummary {
        id: row.id,
        name: row.name,
        framework: row.framework,
        version: row.version,
        description: row.description,
        layer: row.layer,
        owner: row.owner,
        last_review: row.last_review,
        policy_ids: row.policy_ids,
        required_envs,
        policy_count: row.policy_count,
        requirement_count: row.requirement_count,
        control_count: row.control_count,
        environment_count: row.environment_count,
        active_assignment_count: row.active_assignment_count,
        current_draft_version_id: row.current_draft_version_id,
        current_published_version_id: row.current_published_version_id,
        current_draft_version: row.current_draft_version,
        current_published_version: row.current_published_version,
        versions: Vec::new(),
        applicable_system_count: 0,
        aggregate_score: None,
    }
}

fn aggregate_score(pass: i64, evaluated_controls: i64) -> Option<i64> {
    if evaluated_controls > 0 {
        Some((pass * 100) / evaluated_controls)
    } else {
        None
    }
}

async fn list_bundle_summary_aggregates(
    pool: &PgPool,
    bundles: &[ComplianceBundleSummary],
) -> Result<std::collections::HashMap<Uuid, (i64, Option<i64>)>> {
    let pairs: Vec<(Uuid, Uuid)> = bundles
        .iter()
        .filter_map(|bundle| {
            bundle
                .current_published_version_id
                .or(bundle.current_draft_version_id)
                .map(|version_id| (bundle.id, version_id))
        })
        .collect();
    if pairs.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // Load every bundle/version's applicable systems in one set-based query.
    // Environment membership is only an eligibility boundary; active
    // assignments are the sole source of applicability for every revision.
    let bundle_ids: Vec<Uuid> = pairs.iter().map(|(bundle_id, _)| *bundle_id).collect();
    let version_ids: Vec<Uuid> = pairs.iter().map(|(_, version_id)| *version_id).collect();
    let rows: Vec<(Uuid, Uuid, Uuid, String, Option<String>, String, i32, i32)> = sqlx::query_as(
        r#"
        WITH requested(bundle_id, bundle_version_id) AS (
            SELECT * FROM unnest($1::uuid[], $2::uuid[])
        )
        SELECT DISTINCT requested.bundle_id, requested.bundle_version_id,
               v.id, v.hostname, v.environment, v.health_status,
               v.critical_cve_count, v.high_cve_count
        FROM requested
        JOIN compliance_bundles b ON b.id = requested.bundle_id
         JOIN view_system_list v ON EXISTS (
             SELECT 1
             FROM compliance_bundle_assignments a
             LEFT JOIN environments system_env ON system_env.name = v.environment
             WHERE a.bundle_id = b.id
               AND a.bundle_version_id = requested.bundle_version_id
               AND a.active
               AND (
                   (a.scope_type = 'system' AND a.system_id = v.id)
                   OR (a.scope_type = 'environment' AND a.environment_id = system_env.id)
               )
         )
        ORDER BY requested.bundle_id, requested.bundle_version_id, v.hostname
        "#,
    )
    .bind(&bundle_ids)
    .bind(&version_ids)
    .fetch_all(pool)
    .await?;

    let mut systems_by_pair = std::collections::HashMap::<(Uuid, Uuid), Vec<SystemRow>>::new();
    for (
        bundle_id,
        version_id,
        id,
        hostname,
        environment,
        health_status,
        critical_cve_count,
        high_cve_count,
    ) in rows
    {
        let system = SystemRow {
            id,
            hostname,
            environment,
            health_status,
            critical_cve_count,
            high_cve_count,
        };
        systems_by_pair
            .entry((bundle_id, version_id))
            .or_default()
            .push(system);
    }

    // Resolve each distinct version once for all systems that need it. This is
    // the bounded batch path; no bundle calls the detailed systems endpoint.
    let mut system_ids_by_version = std::collections::HashMap::<Uuid, Vec<Uuid>>::new();
    for ((_, version_id), systems) in &systems_by_pair {
        let ids = system_ids_by_version.entry(*version_id).or_default();
        ids.extend(systems.iter().map(|system| system.id));
    }
    for ids in system_ids_by_version.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }
    let resolver_requests: Vec<(Uuid, Vec<Uuid>)> = system_ids_by_version.into_iter().collect();
    let effective_by_version_system =
        resolve_systems_effective_policies_for_bundle_versions_batch(pool, &resolver_requests)
            .await?;

    let mut evidence_work = Vec::new();
    let mut unresolved_by_pair =
        std::collections::HashMap::<(Uuid, Uuid), Vec<ComplianceSystemRollup>>::new();
    for (bundle_id, version_id) in pairs {
        let Some(systems) = systems_by_pair.get(&(bundle_id, version_id)) else {
            unresolved_by_pair.insert((bundle_id, version_id), Vec::new());
            continue;
        };
        for system in systems {
            // Determine assignment status for this specific system and bundle
            let assignment_status =
                determine_assignment_status_for_system(pool, bundle_id, system.id)
                    .await
                    .ok()
                    .flatten();
            let rollup = match effective_by_version_system.get(&(version_id, system.id)) {
                Some(ResolutionOutcome::Resolved(set)) if set.bundle_version_id == version_id => {
                    evidence_work.push((
                        (bundle_id, version_id),
                        system.clone(),
                        set.policies.clone(),
                    ));
                    None
                }
                Some(ResolutionOutcome::Conflict(conflicts)) => Some(unresolved_system_rollup(
                    system.clone(),
                    0,
                    conflicts
                        .first()
                        .map(|conflict| conflict.code.as_str())
                        .unwrap_or("conflict"),
                    assignment_status,
                )),
                _ => Some(unresolved_system_rollup(
                    system.clone(),
                    0,
                    "not_applicable",
                    assignment_status,
                )),
            };
            if let Some(rollup) = rollup {
                unresolved_by_pair
                    .entry((bundle_id, version_id))
                    .or_default()
                    .push(rollup);
            }
        }
    }

    // Note: assignment_status is now determined per-system inside the batch function
    let dummy_assignment_status = std::collections::HashMap::new();
    let batched_rollups = effective_policy_rollups_with_evidence_batch(
        pool,
        &evidence_work,
        &dummy_assignment_status,
    )
    .await?;
    let mut rollups_by_pair = unresolved_by_pair;
    for (pair, rollup) in batched_rollups {
        rollups_by_pair.entry(pair).or_default().push(rollup);
    }

    let mut aggregates = std::collections::HashMap::new();
    for ((bundle_id, version_id), rollups) in rollups_by_pair {
        let totals = totals_for_rollups(&rollups);
        aggregates.insert(
            bundle_id,
            (
                totals.system_count,
                aggregate_score(totals.pass, totals.evaluated_controls),
            ),
        );
    }
    Ok(aggregates)
}

pub async fn list_bundles(pool: &PgPool) -> Result<Vec<ComplianceBundleSummary>> {
    let rows = sqlx::query_as::<_, BundleRow>(
        r#"
        SELECT
            b.id,
            b.name,
            b.framework,
            b.version,
            b.description,
            b.layer,
            b.owner,
            b.last_review,
            COALESCE(p.policy_ids, ARRAY[]::uuid[]) AS policy_ids,
            COALESCE(e.env_ids, ARRAY[]::uuid[]) AS env_ids,
            COALESCE(e.env_names, ARRAY[]::text[]) AS env_names,
            COALESCE(e.env_colors, ARRAY[]::text[]) AS env_colors,
            COALESCE(p.policy_count, 0)::bigint AS policy_count,
            COALESCE(r.requirement_count, 0)::bigint AS requirement_count,
            COALESCE(p.policy_count, 0)::bigint AS control_count,
            COALESCE(e.environment_count, 0)::bigint AS environment_count,
            COALESCE(a.active_assignment_count, 0)::bigint AS active_assignment_count,
            b.current_draft_version_id,
            b.current_published_version_id,
            dv.version AS current_draft_version,
            pv.version AS current_published_version
        FROM compliance_bundles b
        LEFT JOIN LATERAL (
            SELECT
                array_agg(pv.policy_id ORDER BY pv.name) AS policy_ids,
                count(*)::bigint AS policy_count
            FROM compliance_bundle_version_policies cbvp
            JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
            WHERE cbvp.bundle_version_id = COALESCE(
                b.current_published_version_id,
                b.current_draft_version_id
            )
              AND cbvp.selected = true
        ) p ON TRUE
        LEFT JOIN LATERAL (
            SELECT count(*)::bigint AS requirement_count
            FROM compliance_bundle_version_requirements bvr
            WHERE bvr.bundle_version_id = COALESCE(
                b.current_published_version_id,
                b.current_draft_version_id
            )
              AND bvr.selected = true
        ) r ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                array_agg(e.id ORDER BY e.name) AS env_ids,
                array_agg(e.name ORDER BY e.name) AS env_names,
                array_agg(COALESCE(e.color_hex, '#6B7280') ORDER BY e.name) AS env_colors,
                count(*)::bigint AS environment_count
            FROM compliance_bundle_environments cbe
            JOIN environments e ON e.id = cbe.environment_id
            WHERE cbe.bundle_id = b.id
        ) e ON TRUE
        LEFT JOIN LATERAL (
            SELECT count(*)::bigint AS active_assignment_count
            FROM compliance_bundle_assignments assignment
            JOIN compliance_bundle_versions assignment_version
              ON assignment_version.id = assignment.bundle_version_id
            WHERE assignment_version.bundle_id = b.id
              AND COALESCE(assignment.active, true)
        ) a ON TRUE
        LEFT JOIN compliance_bundle_versions dv ON dv.id = b.current_draft_version_id
        LEFT JOIN compliance_bundle_versions pv ON pv.id = b.current_published_version_id
        ORDER BY b.name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut bundles: Vec<ComplianceBundleSummary> = rows.into_iter().map(bundle_from_row).collect();
    let bundle_ids: Vec<Uuid> = bundles.iter().map(|bundle| bundle.id).collect();
    if bundle_ids.is_empty() {
        return Ok(bundles);
    }

    let version_rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            String,
            String,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            Option<Uuid>,
            i64,
            i64,
            i64,
        ),
    >(
        r#"
        SELECT v.id, v.bundle_id, v.version, v.publication_state, v.trust_state,
               v.semantic_digest, v.created_at, v.published_at, v.derived_from_version_id,
                COUNT(DISTINCT cbvp.policy_version_id)::bigint AS policy_count,
                COUNT(DISTINCT bvr.requirement_version_id)::bigint AS requirement_count,
                COUNT(DISTINCT cbvp.policy_version_id)::bigint AS control_count
        FROM compliance_bundle_versions v
        LEFT JOIN compliance_bundle_version_policies cbvp ON cbvp.bundle_version_id = v.id
        LEFT JOIN compliance_bundle_version_requirements bvr ON bvr.bundle_version_id = v.id
        WHERE v.bundle_id = ANY($1)
        GROUP BY v.id
        ORDER BY v.bundle_id, v.created_at DESC, v.id DESC
        "#,
    )
    .bind(&bundle_ids)
    .fetch_all(pool)
    .await?;

    for bundle in &mut bundles {
        bundle.versions = version_rows
            .iter()
            .filter(|row| row.1 == bundle.id)
            .map(|row| ComplianceBundleVersionSummary {
                id: row.0,
                bundle_id: row.1,
                version: row.2.clone(),
                publication_state: row.3.clone(),
                trust_state: row.4.clone(),
                semantic_digest: row.5.clone(),
                created_at: row.6,
                published_at: row.7,
                derived_from_version_id: row.8,
                policy_count: row.9,
                requirement_count: row.10,
                control_count: row.11,
                is_current_published: bundle.current_published_version_id == Some(row.0),
                is_current_draft: bundle.current_draft_version_id == Some(row.0),
            })
            .collect();
    }

    let aggregates = list_bundle_summary_aggregates(pool, &bundles).await?;
    for bundle in &mut bundles {
        if let Some((system_count, score)) = aggregates.get(&bundle.id) {
            bundle.applicable_system_count = *system_count;
            bundle.aggregate_score = *score;
        }
    }

    Ok(bundles)
}

pub async fn create_bundle(
    pool: &PgPool,
    request: CreateComplianceBundleRequest,
) -> Result<ComplianceBundleSummary> {
    let name = request.name.trim();
    let framework = request.framework.trim();
    let version = request.version.as_deref().unwrap_or("").trim();
    let layer = request.layer.as_deref().unwrap_or("fleet").trim();
    let description = request
        .description
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    validate_bundle_request(
        name,
        framework,
        &request.policy_ids,
        &request.requirement_version_ids,
    )?;

    let mut tx = pool.begin().await?;
    validate_requirement_versions(&mut tx, &request.requirement_version_ids).await?;

    // 1. Insert the lineage row. The sync trigger fires AFTER INSERT and creates
    //    the initial draft version row + sets current_draft_version_id.
    let bundle_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO compliance_bundles (name, framework, version, description, layer)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(framework)
    .bind(version)
    .bind(description)
    .bind(if layer.is_empty() { "fleet" } else { layer })
    .fetch_one(&mut *tx)
    .await?;

    // 2. Insert policy and environment membership. The membership trigger
    //    (trigger_sync_bundle_version_membership) maintains bundle_version_policies
    //    automatically.
    for policy_id in request.policy_ids {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_policies (bundle_id, policy_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await?;
    }

    for environment_id in request.required_envs {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_environments (bundle_id, environment_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(environment_id)
        .execute(&mut *tx)
        .await?;
    }

    // Compute and persist canonical bundle and assignment digests inside the
    // transaction so any failure rolls back the entire mutation.
    let (draft_version_id, stored_layer, stored_owner): (Uuid, String, String) = sqlx::query_as(
        r#"
            SELECT b.current_draft_version_id, b.layer, b.owner
            FROM compliance_bundles b
            WHERE b.id = $1
            "#,
    )
    .bind(bundle_id)
    .fetch_one(&mut *tx)
    .await?;

    let members = load_bundle_membership(&mut tx, draft_version_id).await?;

    for (requirement_order, requirement_version_id) in
        request.requirement_version_ids.iter().enumerate()
    {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_version_requirements
                (bundle_version_id, requirement_version_id, selected, requirement_order)
            VALUES ($1, $2, true, $3)
            "#,
        )
        .bind(draft_version_id)
        .bind(requirement_version_id)
        .bind(requirement_order as i32)
        .execute(&mut *tx)
        .await?;
    }

    let req_layer = request.layer.as_deref().unwrap_or("").trim();
    let canonical = BundleVersionCanonical {
        name: request.name.trim().to_string(),
        framework: request.framework.trim().to_string(),
        framework_version: request.version.as_deref().map(str::trim).map(String::from),
        description: request
            .description
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        layer: if req_layer.is_empty() {
            stored_layer
        } else {
            req_layer.to_string()
        },
        owner: stored_owner,
        members,
    };
    write_bundle_version_digest(&mut tx, bundle_id, &canonical).await?;
    refresh_bundle_requirement_digest(&mut tx, draft_version_id).await?;

    // Write assignment overlay digests for all new environment assignments
    // (created by trigger; still have assignment_overlay_digest = 'pending').
    let assignment_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM compliance_bundle_assignments
        WHERE bundle_version_id = $1
        ORDER BY id
        FOR UPDATE
        "#,
    )
    .bind(draft_version_id)
    .fetch_all(&mut *tx)
    .await?;

    for assignment_id in assignment_ids {
        write_assignment_effective_set_digest(&mut tx, assignment_id).await?;
    }

    tx.commit().await?;

    find_bundle(pool, bundle_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created bundle was not found"))
}

pub async fn find_bundle(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Option<ComplianceBundleSummary>> {
    let row = sqlx::query_as::<_, BundleRow>(
        r#"
        SELECT
            b.id,
            b.name,
            b.framework,
            b.version,
            b.description,
            b.layer,
            b.owner,
            b.last_review,
            COALESCE(p.policy_ids, ARRAY[]::uuid[]) AS policy_ids,
            COALESCE(e.env_ids, ARRAY[]::uuid[]) AS env_ids,
            COALESCE(e.env_names, ARRAY[]::text[]) AS env_names,
            COALESCE(e.env_colors, ARRAY[]::text[]) AS env_colors,
            COALESCE(p.policy_count, 0)::bigint AS policy_count,
            COALESCE(r.requirement_count, 0)::bigint AS requirement_count,
            COALESCE(p.policy_count, 0)::bigint AS control_count,
            COALESCE(e.environment_count, 0)::bigint AS environment_count,
            COALESCE(a.active_assignment_count, 0)::bigint AS active_assignment_count,
            b.current_draft_version_id,
            b.current_published_version_id,
            dv.version AS current_draft_version,
            pv.version AS current_published_version
        FROM compliance_bundles b
        LEFT JOIN LATERAL (
             SELECT array_agg(policy_id ORDER BY policy_id) AS policy_ids, count(*)::bigint AS policy_count
            FROM compliance_bundle_policies
            WHERE bundle_id = b.id
        ) p ON TRUE
        LEFT JOIN LATERAL (
            SELECT count(*)::bigint AS requirement_count
            FROM compliance_bundle_version_requirements bvr
            WHERE bvr.bundle_version_id = COALESCE(
                b.current_draft_version_id,
                b.current_published_version_id
            )
        ) r ON TRUE
        LEFT JOIN LATERAL (
            SELECT
                array_agg(e.id ORDER BY e.name) AS env_ids,
                array_agg(e.name ORDER BY e.name) AS env_names,
                array_agg(COALESCE(e.color_hex, '#6B7280') ORDER BY e.name) AS env_colors,
                count(*)::bigint AS environment_count
            FROM compliance_bundle_environments cbe
            JOIN environments e ON e.id = cbe.environment_id
            WHERE cbe.bundle_id = b.id
        ) e ON TRUE
        LEFT JOIN LATERAL (
            SELECT count(*)::bigint AS active_assignment_count
            FROM compliance_bundle_assignments assignment
            JOIN compliance_bundle_versions assignment_version
              ON assignment_version.id = assignment.bundle_version_id
            WHERE assignment_version.bundle_id = b.id
              AND COALESCE(assignment.active, true)
        ) a ON TRUE
        LEFT JOIN compliance_bundle_versions dv ON dv.id = b.current_draft_version_id
        LEFT JOIN compliance_bundle_versions pv ON pv.id = b.current_published_version_id
        WHERE b.id = $1
        "#,
    )
    .bind(bundle_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(bundle_from_row))
}

pub async fn update_bundle(
    pool: &PgPool,
    bundle_id: Uuid,
    request: UpdateComplianceBundleRequest,
    actor_id: Option<Uuid>,
) -> Result<Option<ComplianceBundleSummary>> {
    let name = request.name.trim();
    let framework = request.framework.trim();
    let version = request.version.as_deref().unwrap_or("").trim();

    validate_bundle_request(
        name,
        framework,
        &request.policy_ids,
        &request.requirement_version_ids,
    )?;

    let mut tx = pool.begin().await?;

    // Verify the bundle exists and that the *current draft* is mutable.
    // A bundle may have published history yet still have a separate mutable draft;
    // only editing an already-immutable draft is rejected (P1 #6).
    // Verify the bundle exists first.
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM compliance_bundles WHERE id = $1)")
            .bind(bundle_id)
            .fetch_one(&mut *tx)
            .await?;

    if !exists {
        return Ok(None);
    }

    // Ensure a mutable draft exists (creates a derived draft from the published
    // version when needed). (P1 #2)
    let draft_version_id = ensure_bundle_draft(
        &mut tx,
        bundle_id,
        actor_id,
        None,
        BundleDraftIntent::EnsureMutable,
    )
    .await?;
    validate_requirement_versions(&mut tx, &request.requirement_version_ids).await?;

    // Load layer and owner from the current draft version (not from constants).
    let (stored_layer, stored_owner): (String, String) =
        sqlx::query_as("SELECT layer, owner FROM compliance_bundle_versions WHERE id = $1")
            .bind(draft_version_id)
            .fetch_one(&mut *tx)
            .await?;

    // Update the lineage row. The sync trigger fires AFTER UPDATE and updates
    // the draft version row automatically.
    let updated = sqlx::query_scalar::<_, i64>(
        r#"
        UPDATE compliance_bundles
        SET name = $1, framework = $2, version = $3, description = $4,
            last_review = now()
        WHERE id = $5
        RETURNING 1::bigint
        "#,
    )
    .bind(name)
    .bind(framework)
    .bind(version)
    .bind(
        request
            .description
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty()),
    )
    .bind(bundle_id)
    .fetch_optional(&mut *tx)
    .await?;

    if updated.is_none() {
        return Ok(None);
    }

    // Diff-based policy membership update: preserve unchanged rows (exact
    // version IDs, order, selected) and only remove/add lineages. (P1 #1)
    let new_policy_set: std::collections::HashSet<Uuid> =
        request.policy_ids.iter().copied().collect();

    let existing_policies: Vec<Uuid> =
        sqlx::query_scalar("SELECT policy_id FROM compliance_bundle_policies WHERE bundle_id = $1")
            .bind(bundle_id)
            .fetch_all(&mut *tx)
            .await?;

    let existing_policy_set: std::collections::HashSet<Uuid> =
        existing_policies.iter().copied().collect();

    // Remove lineages no longer in the request.
    for removed in existing_policy_set.difference(&new_policy_set) {
        sqlx::query(
            "DELETE FROM compliance_bundle_policies WHERE bundle_id = $1 AND policy_id = $2",
        )
        .bind(bundle_id)
        .bind(removed)
        .execute(&mut *tx)
        .await?;
    }

    // Add newly requested lineages (in request order — trigger picks up the
    // correct version pointer). ON CONFLICT ensures idempotency.
    for policy_id in &request.policy_ids {
        if !existing_policy_set.contains(policy_id) {
            sqlx::query(
                r#"
                INSERT INTO compliance_bundle_policies (bundle_id, policy_id)
                VALUES ($1, $2) ON CONFLICT DO NOTHING
                "#,
            )
            .bind(bundle_id)
            .bind(policy_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Reject duplicate policy IDs in the request (P1 #2).
    {
        let mut seen = std::collections::HashSet::new();
        for pid in &request.policy_ids {
            if !seen.insert(pid) {
                bail!("Duplicate policy lineage {pid} in request");
            }
        }
    }

    // Update policy_order in compliance_bundle_version_policies to match the
    // request vector. Use a temporary +100000 offset to avoid the unique
    // (bundle_version_id, policy_order) constraint during reordering.
    //
    // Join via the stored version row's policy_id so any exact version
    // (draft, published, imported) is matched correctly (P1 #2 fix).
    //
    // Step 1: Offset all existing orders far above the final range.
    sqlx::query(
        r#"
        UPDATE compliance_bundle_version_policies
        SET policy_order = policy_order + 100000
        WHERE bundle_version_id = $1
        "#,
    )
    .bind(draft_version_id)
    .execute(&mut *tx)
    .await?;

    // Step 2: Assign each requested lineage its exact requested position.
    for (pos, policy_lineage_id) in request.policy_ids.iter().enumerate() {
        let rows_affected = sqlx::query(
            r#"
            UPDATE compliance_bundle_version_policies bvp
            SET policy_order = $1
            FROM deployment_policy_versions pv
            WHERE bvp.bundle_version_id = $2
              AND pv.id = bvp.policy_version_id
              AND pv.policy_id = $3
            "#,
        )
        .bind(pos as i32)
        .bind(draft_version_id)
        .bind(policy_lineage_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected != 1 {
            bail!(
                "Policy lineage {} is not a member of bundle version {} \
                 (no version row found). Add the policy before reordering.",
                policy_lineage_id,
                draft_version_id
            );
        }
    }

    // Step 3: Assert that no temporarily-offset rows remain.  They indicate
    // an internal synchronisation error (a lineage in the request vector did
    // not match any membership row).  The composite key (bundle_version_id,
    // policy_version_id) is used because the table has no surrogate id column.
    let orphaned: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM compliance_bundle_version_policies
        WHERE bundle_version_id = $1
          AND policy_order >= 100000
        "#,
    )
    .bind(draft_version_id)
    .fetch_one(&mut *tx)
    .await?;

    if orphaned > 0 {
        bail!(
            "{} bundle membership row(s) still have a temporary +100000 offset in bundle version {}. \
             This indicates a policy lineage in the request vector was not found in the stored \
             membership. The bundle may be in an inconsistent state.",
            orphaned,
            draft_version_id
        );
    }

    sqlx::query("DELETE FROM compliance_bundle_version_requirements WHERE bundle_version_id = $1")
        .bind(draft_version_id)
        .execute(&mut *tx)
        .await?;
    for (requirement_order, requirement_version_id) in
        request.requirement_version_ids.iter().enumerate()
    {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_version_requirements
                (bundle_version_id, requirement_version_id, selected, requirement_order)
            VALUES ($1, $2, true, $3)
            "#,
        )
        .bind(draft_version_id)
        .bind(requirement_version_id)
        .bind(requirement_order as i32)
        .execute(&mut *tx)
        .await?;
    }

    // Diff-based environment update: preserve unchanged assignments.
    let new_env_set: std::collections::HashSet<Uuid> =
        request.required_envs.iter().copied().collect();

    let existing_envs: Vec<Uuid> = sqlx::query_scalar(
        "SELECT environment_id FROM compliance_bundle_environments WHERE bundle_id = $1",
    )
    .bind(bundle_id)
    .fetch_all(&mut *tx)
    .await?;

    let existing_env_set: std::collections::HashSet<Uuid> = existing_envs.iter().copied().collect();

    for removed in existing_env_set.difference(&new_env_set) {
        sqlx::query(
            "DELETE FROM compliance_bundle_environments WHERE bundle_id = $1 AND environment_id = $2",
        )
        .bind(bundle_id)
        .bind(removed)
        .execute(&mut *tx)
        .await?;
    }

    for added in new_env_set.difference(&existing_env_set) {
        sqlx::query(
            r#"
            INSERT INTO compliance_bundle_environments (bundle_id, environment_id)
            VALUES ($1, $2) ON CONFLICT DO NOTHING
            "#,
        )
        .bind(bundle_id)
        .bind(added)
        .execute(&mut *tx)
        .await?;
    }

    let members = load_bundle_membership(&mut tx, draft_version_id).await?;

    let canonical = BundleVersionCanonical {
        name: name.to_string(),
        framework: framework.to_string(),
        framework_version: if version.is_empty() {
            None
        } else {
            Some(version.to_string())
        },
        description: request
            .description
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        layer: stored_layer,
        owner: stored_owner,
        members,
    };
    write_bundle_version_digest(&mut tx, bundle_id, &canonical).await?;
    refresh_bundle_requirement_digest(&mut tx, draft_version_id).await?;

    // Write assignment effective-set digests for ALL assignments on this draft
    // version (both pre-existing and newly created by the trigger). (P1 #1)
    let assignment_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM compliance_bundle_assignments
        WHERE bundle_version_id = $1
        ORDER BY id
        FOR UPDATE
        "#,
    )
    .bind(draft_version_id)
    .fetch_all(&mut *tx)
    .await?;

    for assignment_id in assignment_ids {
        write_assignment_effective_set_digest(&mut tx, assignment_id).await?;
    }

    tx.commit().await?;

    find_bundle(pool, bundle_id).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleDeleteOutcome {
    Deleted,
    NotFound,
    Blocked(DeletionEligibility),
}

async fn bundle_deletion_eligibility_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    bundle_id: Uuid,
) -> Result<Option<DeletionEligibility>> {
    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM compliance_bundles WHERE id = $1 FOR UPDATE")
            .bind(bundle_id)
            .fetch_optional(&mut **tx)
            .await
            .context("Failed to lock compliance bundle")?;
    if exists.is_none() {
        return Ok(None);
    }
    let immutable_versions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_versions WHERE bundle_id = $1 AND publication_state IN ('accepted', 'deprecated') ORDER BY created_at, id",
    )
    .bind(bundle_id)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to check compliance bundle immutable history")?;
    let immutable_assignment_history: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM compliance_bundle_assignment_versions av
         JOIN compliance_bundle_assignments a ON a.id = av.assignment_id
         JOIN compliance_bundle_versions bv ON bv.id = av.bundle_version_id
         WHERE a.bundle_id = $1
           AND bv.publication_state IN ('accepted', 'deprecated')",
    )
    .bind(bundle_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check immutable bundle assignment history")?;
    let immutable_membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_version_policies bvp JOIN compliance_bundle_versions bv ON bv.id = bvp.bundle_version_id WHERE bv.bundle_id = $1 AND bv.publication_state IN ('accepted', 'deprecated')",
    )
    .bind(bundle_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check immutable bundle memberships")?;
    let mutable_membership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_version_policies bvp JOIN compliance_bundle_versions bv ON bv.id = bvp.bundle_version_id WHERE bv.bundle_id = $1 AND bv.publication_state IN ('incomplete', 'draft', 'interim')",
    )
    .bind(bundle_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check mutable draft bundle memberships")?;
    let immutable_source_mapping_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_source_object_mappings m JOIN compliance_bundle_versions bv ON bv.id = m.bundle_version_id LEFT JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id WHERE bv.bundle_id = $1 AND (bv.publication_state IN ('accepted', 'deprecated') OR pv.publication_state IN ('accepted', 'deprecated'))",
    )
    .bind(bundle_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check immutable bundle source mappings")?;
    let disposable_source_mapping_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_source_object_mappings m JOIN compliance_bundle_versions bv ON bv.id = m.bundle_version_id LEFT JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id WHERE bv.bundle_id = $1 AND bv.publication_state IN ('incomplete', 'draft', 'interim') AND (pv.id IS NULL OR pv.publication_state IN ('incomplete', 'draft', 'interim'))",
    )
    .bind(bundle_id)
    .fetch_one(&mut **tx)
    .await
    .context("Failed to check disposable bundle source mappings")?;
    let mut blockers = Vec::new();
    if !immutable_versions.is_empty() {
        blockers.push(blocker("bundle_immutable_history", "This compliance bundle has accepted or deprecated history and cannot be permanently deleted.", false, None, immutable_versions));
    }
    if immutable_assignment_history > 0 {
        blockers.push(blocker("immutable_assignment_history", "This compliance bundle has immutable assignment history and cannot be permanently deleted.", false, Some(immutable_assignment_history), Vec::new()));
    }
    if immutable_membership_count > 0 {
        blockers.push(blocker("immutable_bundle_membership", "This compliance bundle has immutable policy membership and cannot be permanently deleted.", false, Some(immutable_membership_count), Vec::new()));
    }
    if mutable_membership_count > 0 {
        blockers.push(blocker(
            "mutable_draft_membership",
            "Draft bundle membership will be removed with this bundle.",
            true,
            Some(mutable_membership_count),
            Vec::new(),
        ));
    }
    if disposable_source_mapping_count > 0 {
        blockers.push(blocker(
            "disposable_source_mapping",
            "Draft-only source mappings will be removed with this bundle.",
            true,
            Some(disposable_source_mapping_count),
            Vec::new(),
        ));
    }
    if immutable_source_mapping_count > 0 {
        blockers.push(blocker("immutable_source_mapping", "This compliance bundle has retained source mappings and cannot be permanently deleted.", false, Some(immutable_source_mapping_count), Vec::new()));
    }
    Ok(Some(eligibility(blockers)))
}

pub async fn bundle_deletion_eligibility(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Option<DeletionEligibility>> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin compliance bundle deletion preflight")?;
    let result = bundle_deletion_eligibility_in_transaction(&mut tx, bundle_id).await;
    tx.rollback().await.ok();
    result
}

/// Delete a bundle lineage only when it is disposable.
///
/// The lineage row is locked before eligibility is checked and deleted. The
/// existing database trigger remains as a final race-safety guard for accepted
/// and deprecated history; it is not used as the normal API control flow.
pub async fn delete_bundle(pool: &PgPool, bundle_id: Uuid) -> Result<BundleDeleteOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin bundle deletion")?;

    let Some(eligibility) = bundle_deletion_eligibility_in_transaction(&mut tx, bundle_id).await?
    else {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::NotFound);
    };
    if !eligibility.eligible {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::Blocked(eligibility));
    }

    // Draft-only assignment lineages are disposable with their draft bundle.
    // Remove them before deleting bundle versions because assignment rows hold
    // RESTRICT references to those versions. Immutable assignment history was
    // already checked above and blocks this path when it references published
    // or deprecated bundle versions.
    sqlx::query("DELETE FROM compliance_bundle_assignments WHERE bundle_id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .context("Failed to remove disposable bundle assignments")?;

    sqlx::query("DELETE FROM compliance_source_object_mappings m USING compliance_bundle_versions bv WHERE m.bundle_version_id = bv.id AND bv.bundle_id = $1 AND bv.publication_state IN ('incomplete', 'draft', 'interim') AND NOT EXISTS (SELECT 1 FROM deployment_policy_versions pv WHERE pv.id = m.policy_version_id AND pv.publication_state IN ('accepted', 'deprecated'))")
        .bind(bundle_id).execute(&mut *tx).await.context("Failed to remove disposable bundle source mappings")?;
    sqlx::query("DELETE FROM compliance_bundle_version_policies bvp USING compliance_bundle_versions bv WHERE bvp.bundle_version_id = bv.id AND bv.bundle_id = $1 AND bv.publication_state IN ('incomplete', 'draft', 'interim')")
        .bind(bundle_id).execute(&mut *tx).await.context("Failed to remove mutable draft bundle memberships")?;

    // Keep the trigger in place as defense in depth against publication races.
    let deleted = sqlx::query("DELETE FROM compliance_bundles WHERE id = $1")
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete compliance bundle")?
        .rows_affected();

    if deleted != 1 {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::NotFound);
    }

    tx.commit()
        .await
        .context("Failed to commit compliance bundle deletion")?;
    Ok(BundleDeleteOutcome::Deleted)
}

pub async fn list_bundle_systems(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Option<ComplianceBundleSystemsResponse>> {
    let Some(bundle) = find_bundle(pool, bundle_id).await? else {
        return Ok(None);
    };
    // The unversioned endpoint is a convenience alias for the bundle's current
    // immutable revision. It must never fall back to mutable lineage membership
    // because that would bypass assignment overlays and diverge from the UI.
    if let Some(version_id) = bundle
        .current_published_version_id
        .or(bundle.current_draft_version_id)
    {
        return list_bundle_systems_for_version(pool, bundle_id, version_id).await;
    }
    let policies = list_bundle_policies(pool, bundle_id).await?;
    let systems = list_applicable_system_rows(pool, bundle_id).await?;

    let mut rollups = Vec::with_capacity(systems.len());
    for system in systems {
        rollups.push(system_rollup_with_evidence(pool, system, &policies).await?);
    }
    let totals = totals_for_rollups(&rollups);

    Ok(Some(ComplianceBundleSystemsResponse {
        bundle_id: bundle.id,
        bundle_version_id: None,
        systems: rollups,
        totals,
    }))
}

async fn list_explicit_bundle_version_system_rows(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
) -> Result<Vec<SystemRow>> {
    Ok(sqlx::query_as::<_, SystemRow>(
        r#"
        SELECT DISTINCT v.id, v.hostname, v.environment, v.health_status,
               v.critical_cve_count, v.high_cve_count
        FROM view_system_list v
        LEFT JOIN environments e ON e.name = v.environment
        JOIN compliance_bundle_assignments a
          ON a.bundle_version_id = $2 AND a.active
         AND (
             (a.scope_type = 'system' AND a.system_id = v.id)
             OR (a.scope_type = 'environment' AND a.environment_id = e.id)
         )
        JOIN compliance_bundles b ON b.id = a.bundle_id AND b.id = $1
        ORDER BY v.hostname ASC
        "#,
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .fetch_all(pool)
    .await?)
}

/// Resolve the systems rollup against one exact immutable bundle revision.
/// This deliberately does not substitute the lineage's current membership.
pub async fn list_bundle_systems_for_version(
    pool: &PgPool,
    bundle_id: Uuid,
    bundle_version_id: Uuid,
) -> Result<Option<ComplianceBundleSystemsResponse>> {
    let Some(_bundle) = find_bundle(pool, bundle_id).await? else {
        return Ok(None);
    };
    let version_belongs: Option<Uuid> =
        sqlx::query_scalar("SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1")
            .bind(bundle_version_id)
            .fetch_optional(pool)
            .await?;
    if version_belongs != Some(bundle_id) {
        return Ok(None);
    }

    let policies = sqlx::query_as::<_, PolicyRow>(
        r#"SELECT pv.policy_id AS id, $2 AS bundle_id, pv.name, pv.description,
                  pv.policy_type, pv.config, (dp.enabled AND pv.publication_state IN ('accepted', 'deprecated')) AS enabled
           FROM compliance_bundle_version_policies cbvp
           JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
           JOIN deployment_policies dp ON dp.id = pv.policy_id
           WHERE cbvp.bundle_version_id = $1
           ORDER BY cbvp.policy_order"#,
    )
    .bind(bundle_version_id)
    .bind(bundle_id)
    .fetch_all(pool)
    .await?;
    let systems =
        list_explicit_bundle_version_system_rows(pool, bundle_id, bundle_version_id).await?;
    let system_ids: Vec<Uuid> = systems.iter().map(|system| system.id).collect();
    let effective = resolve_systems_effective_policies_for_bundle_version_batch(
        pool,
        &system_ids,
        bundle_version_id,
    )
    .await?;

    let mut rollups = Vec::with_capacity(systems.len());
    for system in systems {
        // Determine assignment status for this specific system
        let assignment_status = determine_assignment_status_for_system(pool, bundle_id, system.id)
            .await
            .ok()
            .flatten();

        let rollup = match effective.get(&system.id) {
            Some(ResolutionOutcome::Resolved(set))
                if set.bundle_version_id == bundle_version_id =>
            {
                effective_policy_rollup_with_evidence(
                    pool,
                    &system,
                    &set.policies,
                    assignment_status,
                )
                .await?
            }
            Some(ResolutionOutcome::Conflict(conflicts)) => unresolved_system_rollup(
                system,
                policies.len() as i64,
                conflicts
                    .first()
                    .map(|c| c.code.as_str())
                    .unwrap_or("conflict"),
                assignment_status,
            ),
            // Missing or mismatched resolution has no authoritative effective
            // set. Never substitute lineage/current membership for this view.
            _ => unresolved_system_rollup(
                system,
                policies.len() as i64,
                "not_applicable",
                assignment_status,
            ),
        };
        rollups.push(rollup);
    }
    let totals = totals_for_rollups(&rollups);
    Ok(Some(ComplianceBundleSystemsResponse {
        bundle_id,
        bundle_version_id: Some(bundle_version_id),
        systems: rollups,
        totals,
    }))
}

/// Get all compliance bundles applicable to a specific system with their rollups.
/// Returns only bundles where the system is in scope (matches environment filter).
///
/// This function uses set-based queries to avoid N+1 patterns:
/// 1. Fetch system once
/// 2. Fetch all bundles once
/// 3. Fetch all applicable bundle IDs in one query (using environment filter)
/// 4. Fetch policies for all applicable bundles in one query
/// 5. Compute rollups in memory using deterministic logic
///
/// All-or-nothing behavior: Database or infrastructure failures fail the entire
/// request. Individual bundle rollup computation uses pure deterministic logic
/// with no fallible operations.
///
/// Returns None if the system does not exist (caller should return 404).
pub struct SystemBundleRollups {
    pub bundles: Vec<(ComplianceBundleSummary, ComplianceSystemRollup)>,
    pub direct_rollup: ComplianceSystemRollup,
    pub overall_rollup: ComplianceSystemRollup,
}

fn partition_effective_policies_by_bundle(
    policies: &[crate::compliance::resolver::EffectivePolicy],
) -> (
    std::collections::HashMap<Uuid, Vec<crate::compliance::resolver::EffectivePolicy>>,
    Vec<crate::compliance::resolver::EffectivePolicy>,
) {
    let mut by_bundle = std::collections::HashMap::new();
    let mut direct = Vec::new();
    for policy in policies {
        if matches!(
            policy.source,
            crate::compliance::resolver::EffectivePolicySource::LegacyDirect
        ) {
            direct.push(policy.clone());
            continue;
        }
        let mut attributed_bundles = std::collections::HashSet::new();
        for provenance in &policy.provenance {
            if provenance.authoritative {
                let Some(bundle_id) = provenance.bundle_id else {
                    continue;
                };
                if attributed_bundles.insert(bundle_id) {
                    by_bundle
                        .entry(bundle_id)
                        .or_insert_with(Vec::new)
                        .push(policy.clone());
                }
            }
        }
    }
    (by_bundle, direct)
}

pub async fn list_system_bundles(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemBundleRollups>> {
    // First verify the system exists - return None for 404 behavior
    let system_row = sqlx::query_as::<_, SystemRow>(
        r#"
        SELECT
            id,
            hostname,
            environment,
            health_status,
            critical_cve_count,
            high_cve_count
        FROM view_system_list
        WHERE id = $1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    let Some(system) = system_row else {
        return Ok(None);
    };

    // Get all bundles (one query)
    let all_bundles = list_bundles(pool).await?;

    // Determine which bundles apply to this system using set-based query
    // This replaces N individual applicability checks
    let applicable_bundle_ids_vec: Vec<Uuid> = if all_bundles.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar::<_, Uuid>(
            r#"
        SELECT DISTINCT b.id
        FROM compliance_bundles b
        LEFT JOIN environments e ON e.name = $2
        WHERE b.id = ANY($1)
          AND EXISTS (
              SELECT 1
              FROM compliance_bundle_assignments a
              WHERE a.bundle_id = b.id
                AND a.bundle_version_id = COALESCE(b.current_published_version_id, b.current_draft_version_id)
                AND a.active
                AND (
                    (a.scope_type = 'system' AND a.system_id = $3)
                    OR (a.scope_type = 'environment' AND a.environment_id = e.id)
                )
          )
        "#,
        )
        .bind(all_bundles.iter().map(|b| b.id).collect::<Vec<_>>())
        .bind(&system.environment)
        .bind(system.id)
        .fetch_all(pool)
        .await?
    };

    // Convert to HashSet for O(1) membership checks
    let applicable_bundle_ids: std::collections::HashSet<Uuid> =
        applicable_bundle_ids_vec.into_iter().collect();

    // Fetch all policies for all applicable bundles in one query
    // This replaces N individual policy fetches
    let all_policies = if applicable_bundle_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as::<_, PolicyRow>(
            r#"
        SELECT dp.id, cbp.bundle_id, dp.name, dp.description, dp.policy_type, dp.config, dp.enabled
        FROM compliance_bundle_policies cbp
        JOIN deployment_policies dp ON dp.id = cbp.policy_id
        WHERE cbp.bundle_id = ANY($1)
        ORDER BY cbp.bundle_id, dp.name ASC
        "#,
        )
        .bind(applicable_bundle_ids.iter().copied().collect::<Vec<_>>())
        .fetch_all(pool)
        .await?
    };

    // Group policies by bundle_id for O(1) lookup
    let mut policies_by_bundle: std::collections::HashMap<Uuid, Vec<PolicyRow>> =
        std::collections::HashMap::new();
    for policy in all_policies {
        policies_by_bundle
            .entry(policy.bundle_id)
            .or_insert_with(Vec::new)
            .push(policy);
    }

    let visible_bundles: Vec<ComplianceBundleSummary> = all_bundles
        .into_iter()
        .filter(|bundle| applicable_bundle_ids.contains(&bundle.id))
        .collect();

    let outcome = resolve_system_effective_policies(pool, system_id).await?;
    let ResolutionOutcome::Resolved(effective) = outcome else {
        let state = match outcome {
            ResolutionOutcome::Conflict(conflicts) => conflicts
                .first()
                .map(|conflict| conflict.code.clone())
                .unwrap_or_else(|| "conflict".to_string()),
            ResolutionOutcome::Resolved(_) => unreachable!(),
        };
        let bundles = visible_bundles
            .into_iter()
            .map(|bundle| {
                let total = policies_by_bundle
                    .get(&bundle.id)
                    .map_or(0, |policies| policies.len() as i64);
                (
                    bundle,
                    unresolved_system_rollup(system.clone(), total, &state, None),
                )
            })
            .collect();
        return Ok(Some(SystemBundleRollups {
            bundles,
            direct_rollup: unresolved_system_rollup(system.clone(), 0, &state, None),
            overall_rollup: unresolved_system_rollup(system, 0, &state, None),
        }));
    };

    let (mut policies_by_bundle, direct_policies) =
        partition_effective_policies_by_bundle(&effective.policies);

    // The effective policy set's bundle_id is from the effective resolution,
    // which may not match all visible bundles. Determine assignment per bundle.
    let mut bundles = Vec::with_capacity(visible_bundles.len());
    for bundle in visible_bundles {
        // Determine assignment status for this specific bundle and system
        let assignment_status = determine_assignment_status_for_system(pool, bundle.id, system.id)
            .await
            .ok()
            .flatten();
        let policies = policies_by_bundle.remove(&bundle.id).unwrap_or_default();
        bundles.push((
            bundle,
            effective_policy_rollup_with_evidence(pool, &system, &policies, assignment_status)
                .await?,
        ));
    }
    let direct_rollup =
        effective_policy_rollup_with_evidence(pool, &system, &direct_policies, None).await?;
    // For overall rollup, determine assignment from the effective policy set's bundle version
    let bundle_for_effective: Option<Uuid> =
        sqlx::query_scalar("SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1")
            .bind(effective.bundle_version_id)
            .fetch_optional(pool)
            .await?;
    let overall_assignment_status = match bundle_for_effective {
        Some(bundle_id) => determine_assignment_status_for_system(pool, bundle_id, system.id)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let overall_rollup = effective_policy_rollup_with_evidence(
        pool,
        &system,
        &effective.policies,
        overall_assignment_status,
    )
    .await?;

    Ok(Some(SystemBundleRollups {
        bundles,
        direct_rollup,
        overall_rollup,
    }))
}

/// Pure in-memory assembly of compliance bundles for a system.
/// Exported as pub(crate) for unit testing without database fixtures.
///
/// Given:
/// - A system row
/// - All bundles
/// - The set of applicable bundle IDs
/// - Policies grouped by bundle_id
/// - Optional assignment status for the bundle
///
/// Returns bundles with computed rollups, filtered to only applicable bundles.
pub(crate) fn assemble_system_compliance_bundles(
    system: &SystemRow,
    bundles: Vec<ComplianceBundleSummary>,
    applicable_bundle_ids: &std::collections::HashSet<Uuid>,
    policies_by_bundle: &std::collections::HashMap<Uuid, Vec<PolicyRow>>,
) -> Vec<(ComplianceBundleSummary, ComplianceSystemRollup)> {
    let mut result = Vec::new();

    for bundle in bundles {
        if !applicable_bundle_ids.contains(&bundle.id) {
            continue;
        }

        let policies = policies_by_bundle
            .get(&bundle.id)
            .cloned()
            .unwrap_or_default();

        // system_rollup is pure deterministic computation with no fallible operations
        let rollup = system_rollup(system.clone(), &policies, None);
        result.push((bundle, rollup));
    }

    result
}

pub async fn get_system_evidence(
    pool: &PgPool,
    bundle_id: Uuid,
    system_id: Uuid,
    bundle_version_id: Option<Uuid>,
) -> Result<Option<ComplianceEvidenceResponse>> {
    let Some(bundle) = find_bundle(pool, bundle_id).await? else {
        return Ok(None);
    };

    // Use the same environment predicate as list_applicable_system_rows so that
    // requesting evidence for a system outside the bundle's environment scope
    // returns None (→ 404) rather than fabricated out-of-scope compliance data.
    let system = match bundle_version_id {
        Some(version_id) => list_explicit_bundle_version_system_rows(pool, bundle_id, version_id)
            .await?
            .into_iter()
            .find(|row| row.id == system_id),
        None => find_applicable_system_row(pool, bundle_id, system_id).await?,
    };

    let Some(system) = system else {
        return Ok(None);
    };

    let mut policies = match bundle_version_id {
        Some(version_id) => {
            let version: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT bundle_id, framework FROM compliance_bundle_versions WHERE id = $1",
            )
            .bind(version_id)
            .fetch_optional(pool)
            .await?;
            if version.as_ref().map(|(id, _)| *id) != Some(bundle_id) {
                return Ok(None);
            }
            sqlx::query_as::<_, PolicyRow>(
                r#"SELECT pv.policy_id AS id, $2 AS bundle_id, pv.name, pv.description,
                           pv.policy_type, pv.config, pv.compliance_metadata,
                          (dp.enabled AND pv.publication_state IN ('accepted', 'deprecated')) AS enabled
                   FROM compliance_bundle_version_policies cbvp
                   JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
                   JOIN deployment_policies dp ON dp.id = pv.policy_id
                   WHERE cbvp.bundle_version_id = $1
                   ORDER BY cbvp.policy_order"#,
            )
            .bind(version_id)
            .bind(bundle_id)
            .fetch_all(pool)
            .await?
        }
        None => list_bundle_policies(pool, bundle_id).await?,
    };
    let mut resolution_state: Option<String> = None;
    if let ResolutionOutcome::Resolved(effective) =
        resolve_system_effective_policies(pool, system_id).await?
    {
        let resolved_bundle_id: Option<Uuid> =
            sqlx::query_scalar("SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1")
                .bind(effective.bundle_version_id)
                .fetch_optional(pool)
                .await?;
        let exact_requested = bundle_version_id
            .map(|requested| effective.bundle_version_id == requested)
            .unwrap_or(resolved_bundle_id == Some(bundle_id));
        if exact_requested && resolved_bundle_id == Some(bundle_id) {
            policies = materialize_effective_policies(pool, &effective.policies).await?;
        } else if bundle_version_id.is_some() {
            resolution_state = Some("not_applicable".to_string());
        }
    } else if bundle_version_id.is_some() {
        resolution_state = Some("conflict".to_string());
    }
    let mut controls = Vec::with_capacity(policies.len());
    for policy in policies {
        let evidence = resolve_control_evidence(pool, &system, policy).await?;
        controls.push(evidence);
    }

    Ok(Some(ComplianceEvidenceResponse {
        bundle_id,
        bundle_version_id,
        framework: match bundle_version_id {
            Some(version_id) => {
                sqlx::query_scalar("SELECT framework FROM compliance_bundle_versions WHERE id = $1")
                    .bind(version_id)
                    .fetch_optional(pool)
                    .await?
            }
            None => Some(bundle.framework),
        },
        system_id,
        hostname: system.hostname,
        controls,
        resolution_state,
    }))
}

/// Fetch a single system row only if it is within the bundle's environment scope.
/// Uses the identical predicate as [`list_applicable_system_rows`] so the two
/// functions can never diverge in which systems they consider applicable.
async fn find_applicable_system_row(
    pool: &PgPool,
    bundle_id: Uuid,
    system_id: Uuid,
) -> Result<Option<SystemRow>> {
    Ok(sqlx::query_as::<_, SystemRow>(
        r#"
        SELECT
            v.id,
            v.hostname,
            v.environment,
            v.health_status,
            v.critical_cve_count,
            v.high_cve_count
        FROM view_system_list v
        JOIN compliance_bundle_versions bv
          ON bv.id = COALESCE(
              (SELECT current_published_version_id FROM compliance_bundles WHERE id = $1),
              (SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1)
          )
        LEFT JOIN environments e ON e.name = v.environment
        WHERE v.id = $2
          AND EXISTS (
              SELECT 1
              FROM compliance_bundle_assignments a
              WHERE a.bundle_id = $1
                AND a.bundle_version_id = bv.id
                AND a.active
                AND (
                    (a.scope_type = 'system' AND a.system_id = v.id)
                    OR (a.scope_type = 'environment' AND a.environment_id = e.id)
                )
          )
        "#,
    )
    .bind(bundle_id)
    .bind(system_id)
    .fetch_optional(pool)
    .await?)
}

async fn list_bundle_policies(pool: &PgPool, bundle_id: Uuid) -> Result<Vec<PolicyRow>> {
    Ok(sqlx::query_as::<_, PolicyRow>(
        r#"
        SELECT dp.id, dp.name, dp.description, dp.policy_type, dp.config, dp.enabled
        FROM compliance_bundle_policies cbp
        JOIN deployment_policies dp ON dp.id = cbp.policy_id
        WHERE cbp.bundle_id = $1
        ORDER BY dp.name ASC
        "#,
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await?)
}

async fn list_applicable_system_rows(pool: &PgPool, bundle_id: Uuid) -> Result<Vec<SystemRow>> {
    Ok(sqlx::query_as::<_, SystemRow>(
        r#"
        SELECT
            v.id,
            v.hostname,
            v.environment,
            v.health_status,
            v.critical_cve_count,
            v.high_cve_count
        FROM view_system_list v
        JOIN compliance_bundles b ON b.id = $1
        JOIN compliance_bundle_versions bv
          ON bv.id = COALESCE(b.current_published_version_id, b.current_draft_version_id)
        LEFT JOIN environments e ON e.name = v.environment
        WHERE EXISTS (
            SELECT 1
            FROM compliance_bundle_assignments a
            WHERE a.bundle_id = b.id
              AND a.bundle_version_id = bv.id
              AND a.active
              AND (
                  (a.scope_type = 'system' AND a.system_id = v.id)
                  OR (a.scope_type = 'environment' AND a.environment_id = e.id)
              )
        )
        ORDER BY v.hostname ASC
        "#,
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await?)
}

/// Determine assignment status for a system assigned to a bundle version.
///
/// Determine assignment status for a specific system and bundle.
/// Queries the compliance_bundle_assignments table to find if the system has an
/// active assignment and determines if it targets the current published version.
///
/// Returns:
/// - "current" if the system has an active assignment to the bundle's current published version
/// - "pinned" if the system has an active assignment to an older accepted version  
/// - None if no active assignment exists for the system
pub(crate) async fn determine_assignment_status_for_system(
    pool: &PgPool,
    bundle_id: Uuid,
    system_id: Uuid,
) -> Result<Option<String>> {
    // Get the bundle's current published version
    let current_published_version: Option<Uuid> = sqlx::query_scalar(
        "SELECT current_published_version_id FROM compliance_bundles WHERE id = $1",
    )
    .bind(bundle_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    // Check for active assignment targeting this system (system scope takes precedence)
    // Note: compliance_bundle_assignments table doesn't have bundle_id directly,
    // only bundle_version_id, so we need to join to get bundles
    let assigned_version: Option<Uuid> = sqlx::query_scalar(
        r#"SELECT cba.bundle_version_id FROM compliance_bundle_assignments cba
           JOIN compliance_bundle_versions cbv ON cbv.id = cba.bundle_version_id
           WHERE cbv.bundle_id = $1 AND cba.system_id = $2 AND cba.active = true
           LIMIT 1"#,
    )
    .bind(bundle_id)
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    // If system has no direct assignment, check for environment scope assignment
    let assigned_version = match assigned_version {
        Some(v) => Some(v),
        None => {
            // Check for environment-scoped assignment
            sqlx::query_scalar(
                r#"SELECT cba.bundle_version_id FROM compliance_bundle_assignments cba
                   JOIN compliance_bundle_versions cbv ON cbv.id = cba.bundle_version_id
                   WHERE cbv.bundle_id = $1 AND cba.scope_type = 'environment' AND cba.active = true
                   AND EXISTS (
                       SELECT 1 FROM systems s 
                       WHERE s.id = $2 AND s.environment_id = cba.environment_id
                    )
                    LIMIT 1"#,
            )
            .bind(bundle_id)
            .bind(system_id)
            .fetch_optional(pool)
            .await?
        }
    };

    // Determine status based on which version we're assigned to
    match assigned_version {
        Some(v) => {
            // System has an assignment; determine if it's current or pinned
            match current_published_version {
                Some(current) if v == current => Ok(Some("current".to_string())),
                Some(_) => Ok(Some("pinned".to_string())),
                None => Ok(Some("pinned".to_string())), // No current published, but assigned to something
            }
        }
        None => Ok(None), // No assignment for this system
    }
}

/// Legacy function that determined status per bundle version (kept for backwards compatibility).
/// Deprecated: Use determine_assignment_status_for_system() instead.
pub(crate) async fn determine_assignment_status_for_bundle_version(
    pool: &PgPool,
    bundle_version_id: Uuid,
) -> Result<Option<String>> {
    // Get the current published version of the bundle
    let current_published_version: Option<Uuid> = sqlx::query_scalar(
        "SELECT current_published_version_id FROM compliance_bundles WHERE id = (SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1)",
    )
    .bind(bundle_version_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let current_published = match current_published_version {
        Some(v) => v,
        None => {
            // Bundle has no published version yet; assignment cannot be "current"
            return Ok(Some("pinned".to_string()));
        }
    };

    // Determine the status based on which version we're assigned to
    if bundle_version_id == current_published {
        Ok(Some("current".to_string()))
    } else {
        // Assignment targets an older/other accepted version
        Ok(Some("pinned".to_string()))
    }
}

pub(crate) fn system_rollup(
    system: SystemRow,
    policies: &[PolicyRow],
    assignment_status: Option<String>,
) -> ComplianceSystemRollup {
    let statuses = policies
        .iter()
        .map(|policy| match evaluate_policy(&system, policy) {
            PolicyEval::Evaluated(status) => status,
            PolicyEval::Disabled | PolicyEval::Unsupported => ComplianceControlStatus::NotChecked,
        })
        .collect::<Vec<_>>();
    rollup_from_statuses(system, &statuses, 0, assignment_status)
}

async fn system_rollup_with_evidence(
    pool: &PgPool,
    system: SystemRow,
    policies: &[PolicyRow],
) -> Result<ComplianceSystemRollup> {
    let mut statuses = Vec::with_capacity(policies.len());
    for policy in policies.iter().cloned() {
        statuses.push(
            resolve_control_evidence(pool, &system, policy)
                .await?
                .status,
        );
    }
    Ok(rollup_from_statuses(system, &statuses, 0, None))
}

fn rollup_from_statuses(
    system: SystemRow,
    statuses: &[ComplianceControlStatus],
    report_only: i64,
    assignment_status: Option<String>,
) -> ComplianceSystemRollup {
    let mut pass = 0i64;
    let mut warn = 0i64;
    let mut fail = 0i64;
    let mut waiver = 0i64;
    let mut not_checked = 0i64;
    let mut not_applicable = 0i64;
    let mut error_count = 0i64;
    // Only policies that were actually evaluated count toward total and score.
    // Disabled and unsupported policies are surfaced as warn but excluded from
    // the denominator so they don't silently deflate the score.
    let mut evaluated_total = 0i64;

    for status in statuses {
        match status {
            ComplianceControlStatus::Pass => {
                pass += 1;
                evaluated_total += 1;
            }
            ComplianceControlStatus::Warn => {
                warn += 1;
                evaluated_total += 1;
            }
            ComplianceControlStatus::Fail => {
                fail += 1;
                evaluated_total += 1;
            }
            ComplianceControlStatus::Waiver => {
                waiver += 1;
                evaluated_total += 1;
            }
            // Canonical evidence states: each control maps to exactly one
            // bucket. warn + not_checked + not_applicable + error + pass +
            // fail + waiver == total. No double-counting.
            ComplianceControlStatus::NotChecked => {
                not_checked += 1;
            }
            ComplianceControlStatus::NotApplicable => {
                not_applicable += 1;
            }
            ComplianceControlStatus::Error => {
                error_count += 1;
            }
        }
    }

    // total = full bundle policy count (for UI display: "N of M controls evaluated").
    // evaluated_total = only the policies that were actually assessed; this is the
    // correct denominator for the score.
    let total = statuses.len() as i64;
    let score = if evaluated_total == 0 {
        0
    } else {
        (pass * 100) / evaluated_total
    };

    ComplianceSystemRollup {
        system_id: system.id,
        hostname: system.hostname,
        environment: system.environment,
        applies: true,
        total,
        evaluated_total,
        pass,
        warn,
        fail,
        waiver,
        not_checked,
        not_applicable,
        error: error_count,
        report_only,
        score,
        resolution_state: None,
        assignment_status,
        assignment_reason: None,
        assignment_approved_by: None,
        assignment_deadline: None,
        assignment_poam: None,
    }
}

fn unresolved_system_rollup(
    system: SystemRow,
    selected_controls: i64,
    state: &str,
    assignment_status: Option<String>,
) -> ComplianceSystemRollup {
    ComplianceSystemRollup {
        system_id: system.id,
        hostname: system.hostname,
        environment: system.environment,
        applies: true,
        total: selected_controls,
        evaluated_total: 0,
        pass: 0,
        warn: 0,
        fail: 0,
        waiver: 0,
        not_checked: selected_controls,
        not_applicable: 0,
        error: 0,
        report_only: 0,
        score: 0,
        resolution_state: Some(state.to_string()),
        assignment_status,
        assignment_reason: None,
        assignment_approved_by: None,
        assignment_deadline: None,
        assignment_poam: None,
    }
}

/// Compute a rollup from the resolver's effective policy set.
///
/// This resolver-aware rollup correctly accounts for exclusions, additions,
/// overrides, and specificity precedence. Legacy direct membership rollups
/// should be replaced with this function when assignments exist.
pub(crate) fn effective_policy_rollup(
    system: &SystemRow,
    effective_policies: &[crate::compliance::resolver::EffectivePolicy],
    assignment_status: Option<String>,
) -> ComplianceSystemRollup {
    let mut pass = 0i64;
    let mut warn = 0i64;
    let mut fail = 0i64;
    let mut waiver = 0i64;
    let mut not_checked = 0i64;
    let mut not_applicable = 0i64;
    let mut error_count = 0i64;
    let mut report_only = 0i64;
    let mut evaluated_total = 0i64;

    let total = effective_policies.len() as i64;

    for ep in effective_policies {
        let is_report_only = matches!(
            ep.effective_mode,
            crate::compliance::resolver::AssignmentMode::ReportOnly
        );

        // Evaluate the policy based on its type and the system health.
        // For effective policies we use the resolved effective_config.
        let policy_type = &ep.policy_type;
        let config = &ep.effective_config;

        match policy_type.as_str() {
            "require_cf_agent" => {
                let status = match system.health_status.as_str() {
                    "healthy" | "online" => ComplianceControlStatus::Pass,
                    "offline" => ComplianceControlStatus::Fail,
                    _ => ComplianceControlStatus::Warn,
                };
                evaluated_total += 1;
                match status {
                    ComplianceControlStatus::Pass => pass += 1,
                    ComplianceControlStatus::Warn => warn += 1,
                    ComplianceControlStatus::Fail => fail += 1,
                    ComplianceControlStatus::Waiver => waiver += 1,
                    _ => {}
                }
            }
            "require_cve_check" => {
                let max_critical = config
                    .get("max_critical")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(i64::MAX);
                let require_high_justification = config
                    .get("require_high_justification")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);

                evaluated_total += 1;
                if i64::from(system.critical_cve_count) > max_critical {
                    fail += 1;
                } else if require_high_justification && system.high_cve_count > 0 {
                    warn += 1;
                } else {
                    pass += 1;
                }
            }
            // These policies require their real evaluation pipeline evidence;
            // system health alone is not evidence that they passed.
            "require_packages" | "custom_check" | "time_window" | "require_approvals"
            | "canary_rollout" | "cve_threshold" => {
                not_checked += 1;
            }
            // Manual policies: counted but not evaluated.
            "manual" | "external" => {
                not_checked += 1;
            }
            // Unsupported / opaque: counted but not evaluated.
            _ => {
                not_checked += 1;
            }
        }

        if is_report_only {
            report_only += 1;
        }
    }

    let score = if evaluated_total == 0 {
        0
    } else {
        (pass * 100) / evaluated_total
    };

    ComplianceSystemRollup {
        system_id: system.id,
        hostname: system.hostname.clone(),
        environment: system.environment.clone(),
        applies: true,
        total,
        evaluated_total,
        pass,
        warn,
        fail,
        waiver,
        not_checked,
        not_applicable,
        error: error_count,
        report_only,
        score,
        resolution_state: None,
        assignment_status,
        assignment_reason: None,
        assignment_approved_by: None,
        assignment_deadline: None,
        assignment_poam: None,
    }
}

/// Resolve evidence for the exact version/configuration selected by the
/// assignment resolver. This keeps assignment overlays and evidence views on
/// the same policy set while avoiding heartbeat-derived results.
pub(crate) async fn effective_policy_rollup_with_evidence(
    pool: &PgPool,
    system: &SystemRow,
    effective_policies: &[crate::compliance::resolver::EffectivePolicy],
    assignment_status: Option<String>,
) -> Result<ComplianceSystemRollup> {
    let policies = materialize_effective_policies(pool, effective_policies).await?;
    let report_only = effective_policies
        .iter()
        .filter(|policy| {
            matches!(
                policy.effective_mode,
                crate::compliance::resolver::AssignmentMode::ReportOnly
            )
        })
        .count() as i64;

    let mut statuses = Vec::with_capacity(policies.len());
    for policy in policies {
        statuses.push(resolve_control_evidence(pool, system, policy).await?.status);
    }
    Ok(rollup_from_statuses(
        system.clone(),
        &statuses,
        report_only,
        assignment_status,
    ))
}

/// Batch the evidence inputs needed by catalog aggregates. The detail path is
/// intentionally unchanged, while this path loads policy metadata, deployed
/// assessment contexts, and latest completed CVE scans once for the complete
/// set of `(bundle version, system)` work.
async fn effective_policy_rollups_with_evidence_batch(
    pool: &PgPool,
    work: &[(
        (Uuid, Uuid),
        SystemRow,
        Vec<crate::compliance::resolver::EffectivePolicy>,
    )],
    _assignment_status_by_version: &std::collections::HashMap<Uuid, Option<String>>,
) -> Result<Vec<((Uuid, Uuid), ComplianceSystemRollup)>> {
    if work.is_empty() {
        return Ok(Vec::new());
    }

    let effective_policies: Vec<_> = work
        .iter()
        .flat_map(|(_, _, policies)| policies.iter().cloned())
        .collect();
    let materialized = materialize_effective_policies(pool, &effective_policies).await?;
    let policies_by_version = materialized
        .into_iter()
        .zip(
            effective_policies
                .iter()
                .map(|policy| policy.policy_version_id),
        )
        .map(|(policy, version_id)| (version_id, policy))
        .collect::<std::collections::HashMap<_, _>>();

    let system_ids: Vec<Uuid> = work
        .iter()
        .map(|(_, system, _)| system.id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let context_rows: Vec<(Uuid, i32, Value)> = sqlx::query_as(
        r#"
        SELECT s.id, d.id AS derivation_id, d.policy_results
        FROM systems s
        JOIN LATERAL (
            SELECT ss.store_path
            FROM system_states ss
            WHERE ss.hostname = s.hostname
            ORDER BY ss.timestamp DESC, ss.id DESC
            LIMIT 1
        ) deployed ON true
        JOIN derivations d ON COALESCE(d.store_path, d.expected_store_path) = deployed.store_path
        WHERE s.id = ANY($1)
          AND d.derivation_type = 'nixos'
        ORDER BY s.id, d.completed_at DESC NULLS LAST, d.id DESC
        "#,
    )
    .bind(&system_ids)
    .fetch_all(pool)
    .await?;
    let contexts = context_rows.into_iter().fold(
        std::collections::HashMap::<Uuid, AssessmentContext>::new(),
        |mut contexts, (system_id, derivation_id, policy_results)| {
            contexts.entry(system_id).or_insert(AssessmentContext {
                derivation_id,
                policy_results,
            });
            contexts
        },
    );
    let derivation_ids: Vec<i32> = contexts
        .values()
        .map(|context| context.derivation_id)
        .collect();
    let scans: std::collections::HashMap<i32, (Uuid, i32, i32)> = sqlx::query_as(
        r#"
        SELECT DISTINCT ON (derivation_id) id, derivation_id, critical_count, high_count
        FROM cve_scans
        WHERE derivation_id = ANY($1) AND status = 'completed'
        ORDER BY derivation_id, completed_at DESC NULLS LAST, id DESC
        "#,
    )
    .bind(&derivation_ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id, derivation_id, critical, high)| (derivation_id, (id, critical, high)))
    .collect();

    let mut result = Vec::with_capacity(work.len());
    for (pair, system, policies) in work {
        let (bundle_id, _version_id) = pair;
        // Determine assignment status for this specific system and bundle
        let assignment_status = determine_assignment_status_for_system(pool, *bundle_id, system.id)
            .await
            .ok()
            .flatten();
        let context = contexts.get(&system.id);
        let mut statuses = Vec::with_capacity(policies.len());
        let report_only = policies
            .iter()
            .filter(|policy| {
                matches!(
                    policy.effective_mode,
                    crate::compliance::resolver::AssignmentMode::ReportOnly
                )
            })
            .count() as i64;
        for effective in policies {
            let mut policy = policies_by_version
                .get(&effective.policy_version_id)
                .context("missing materialized effective policy in batch evidence")?
                .clone();
            // Policy metadata is immutable and may be shared by many systems,
            // but effective_config is runtime state after assignment overlays.
            // Never let one system's override overwrite another's evaluation.
            policy.config = effective.effective_config.clone();
            statuses.push(batch_evidence_status(
                &policy,
                context,
                context.and_then(|context| scans.get(&context.derivation_id)),
            ));
        }
        result.push((
            *pair,
            rollup_from_statuses(system.clone(), &statuses, report_only, assignment_status),
        ));
    }
    Ok(result)
}

fn batch_evidence_status(
    policy: &PolicyRow,
    context: Option<&AssessmentContext>,
    scan: Option<&(Uuid, i32, i32)>,
) -> ComplianceControlStatus {
    if !policy.enabled {
        return ComplianceControlStatus::NotChecked;
    }
    match policy.policy_type.as_str() {
        "require_cf_agent" | "require_packages" | "custom_check" => {
            let Some(context) = context else {
                return ComplianceControlStatus::NotChecked;
            };
            match nix_policy_result(&context.policy_results, policy.id) {
                Ok(None) => ComplianceControlStatus::NotChecked,
                Ok(Some((true, _))) => ComplianceControlStatus::Pass,
                Ok(Some((false, _))) => ComplianceControlStatus::Fail,
                Err(_) => ComplianceControlStatus::Error,
            }
        }
        "require_cve_check" => {
            let Some((_, critical_count, high_count)) = scan else {
                return ComplianceControlStatus::NotChecked;
            };
            let max_critical = policy
                .config
                .get("max_critical")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            let max_high = policy.config.get("max_high").and_then(Value::as_i64);
            if i64::from(*critical_count) > max_critical
                || max_high.is_some_and(|max| i64::from(*high_count) > max)
            {
                ComplianceControlStatus::Fail
            } else {
                ComplianceControlStatus::Pass
            }
        }
        _ => ComplianceControlStatus::NotChecked,
    }
}

/// Materialize an assignment resolver output for evidence. `PolicyRow::id` is
/// deliberately the stable lineage identity because persisted Nix results are
/// keyed by lineage; the version is used only to load exact metadata and the
/// effective enabled state. Every consumer of effective evidence must use this
/// helper rather than constructing a `PolicyRow` from a version UUID.
async fn materialize_effective_policies(
    pool: &PgPool,
    effective_policies: &[crate::compliance::resolver::EffectivePolicy],
) -> Result<Vec<PolicyRow>> {
    let version_ids: Vec<Uuid> = effective_policies
        .iter()
        .map(|policy| policy.policy_version_id)
        .collect();
    let rows: Vec<(Uuid, String, Option<String>, bool, Value)> = sqlx::query_as(
        r#"
        SELECT pv.id, pv.name, pv.description,
               (dp.enabled AND pv.publication_state IN ('accepted', 'deprecated')) AS enabled,
               pv.compliance_metadata
        FROM deployment_policy_versions pv
        JOIN deployment_policies dp ON dp.id = pv.policy_id
        WHERE pv.id = ANY($1)
        "#,
    )
    .bind(&version_ids)
    .fetch_all(pool)
    .await?;
    let rows = rows
        .into_iter()
        .map(|(id, name, description, enabled, compliance_metadata)| {
            (id, (name, description, enabled, compliance_metadata))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut policies = Vec::with_capacity(effective_policies.len());
    for effective in effective_policies {
        let Some((name, description, enabled, compliance_metadata)) =
            rows.get(&effective.policy_version_id).cloned()
        else {
            bail!(
                "effective policy version {} no longer exists",
                effective.policy_version_id
            );
        };
        policies.push(PolicyRow {
            id: effective.policy_lineage_id,
            bundle_id: Uuid::nil(),
            name,
            description,
            policy_type: effective.policy_type.clone(),
            config: effective.effective_config.clone(),
            enabled,
            compliance_metadata,
        });
    }
    Ok(policies)
}

pub(crate) fn totals_for_rollups(rollups: &[ComplianceSystemRollup]) -> ComplianceRollupTotals {
    let mut totals = ComplianceRollupTotals {
        system_count: rollups.len() as i64,
        ..ComplianceRollupTotals::default()
    };

    for rollup in rollups {
        // A host is "fully compliant" when every evaluated control passed —
        // i.e. no failures and no genuine evaluation warnings.
        // not_checked/not_applicable are unevaluated and must not block compliance.
        if rollup.fail == 0 && rollup.warn == 0 && rollup.error == 0 && rollup.evaluated_total > 0 {
            totals.fully_compliant_count += 1;
        }
        totals.pass += rollup.pass;
        totals.warn += rollup.warn;
        totals.fail += rollup.fail;
        totals.waiver += rollup.waiver;
        totals.total_controls += rollup.total;
        totals.evaluated_controls += rollup.evaluated_total;
    }

    // overall_score uses evaluated_controls (not total_controls) as the
    // denominator so disabled/unsupported policies cannot deflate the headline score.
    totals.overall_score = if totals.evaluated_controls == 0 {
        0
    } else {
        (totals.pass * 100) / totals.evaluated_controls
    };
    totals
}

/// Three-way status result so callers can distinguish evaluatable outcomes
/// from controls that were not evaluated (disabled or unsupported type).
enum PolicyEval {
    /// Control was evaluated and produced a compliance outcome.
    Evaluated(ComplianceControlStatus),
    /// Control is disabled — excluded from scores, surfaced as not-evaluated.
    Disabled,
    /// Policy type is not yet supported — not a pass, but not a scored failure.
    Unsupported,
}

fn evaluate_policy(system: &SystemRow, policy: &PolicyRow) -> PolicyEval {
    if !policy.enabled {
        return PolicyEval::Disabled;
    }

    match policy.policy_type.as_str() {
        "require_cf_agent" => {
            // Fail-closed: only known-healthy statuses pass.
            // Unknown / stale / error values → Warn (indeterminate, not a scored pass).
            let status = match system.health_status.as_str() {
                "healthy" | "online" => ComplianceControlStatus::Pass,
                "offline" => ComplianceControlStatus::Fail,
                _ => ComplianceControlStatus::Warn,
            };
            PolicyEval::Evaluated(status)
        }
        "require_cve_check" => {
            let max_critical = policy
                .config
                .get("max_critical")
                .and_then(Value::as_i64)
                .unwrap_or(i64::MAX);
            let require_high_justification = policy
                .config
                .get("require_high_justification")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let status = if i64::from(system.critical_cve_count) > max_critical {
                ComplianceControlStatus::Fail
            } else if require_high_justification && system.high_cve_count > 0 {
                ComplianceControlStatus::Warn
            } else {
                ComplianceControlStatus::Pass
            };
            PolicyEval::Evaluated(status)
        }
        // Unknown policy type: not a fabricated pass, but also not a scored
        // failure — return Warn so the reviewer knows it needs attention.
        _ => PolicyEval::Unsupported,
    }
}

/// Translate a PolicyEval into the ComplianceControlStatus used by rollups and
/// evidence. Disabled and unsupported controls map to `NotChecked`.
/// Use `attention_count()` on the rollup for legacy UI summary badges.
fn policy_status(system: &SystemRow, policy: &PolicyRow) -> ComplianceControlStatus {
    match evaluate_policy(system, policy) {
        PolicyEval::Evaluated(s) => s,
        PolicyEval::Disabled | PolicyEval::Unsupported => ComplianceControlStatus::NotChecked,
    }
}

#[derive(Debug, Clone, FromRow)]
struct AssessmentContext {
    derivation_id: i32,
    policy_results: Value,
}

async fn assessment_context(pool: &PgPool, system_id: Uuid) -> Result<Option<AssessmentContext>> {
    sqlx::query_as(
        r#"
        SELECT d.id AS derivation_id, d.policy_results
        FROM systems s
        JOIN LATERAL (
            SELECT ss.store_path
            FROM system_states ss
            WHERE ss.hostname = s.hostname
            ORDER BY ss.timestamp DESC, ss.id DESC
            LIMIT 1
        ) deployed ON true
        JOIN derivations d ON COALESCE(d.store_path, d.expected_store_path) = deployed.store_path
        WHERE s.id = $1
          AND d.derivation_type = 'nixos'
        ORDER BY d.completed_at DESC NULLS LAST, d.id DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await
    .context("load deployed assessment context")
}

fn nix_policy_result(
    policy_results: &Value,
    policy_id: Uuid,
) -> Result<Option<(bool, Option<String>)>> {
    let Some(assigned) = policy_results.get("assigned") else {
        return Ok(None);
    };
    let assigned = assigned
        .as_object()
        .context("persisted policy results have a non-object assigned map")?;
    let Some(result) = assigned.get(&policy_id.to_string()) else {
        return Ok(None);
    };
    let result = result
        .as_object()
        .context("persisted policy result is not an object")?;
    let passed = result
        .get("passed")
        .and_then(Value::as_bool)
        .context("persisted policy result has no boolean passed value")?;
    let details = result
        .get("details")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(Some((passed, details)))
}

async fn resolve_control_evidence(
    pool: &PgPool,
    system: &SystemRow,
    policy: PolicyRow,
) -> Result<ComplianceControlEvidence> {
    if !policy.enabled {
        return Ok(control_evidence_with_resolved_status(
            system,
            policy,
            ComplianceControlStatus::NotChecked,
            "Policy is disabled and was not evaluated.".to_string(),
            "No evidence is collected for disabled policies.".to_string(),
            "policy_eval",
            "Disabled policy",
        ));
    }
    let context = assessment_context(pool, system.id).await?;
    let (status, summary, body, artifact_type, artifact_title) = match policy.policy_type.as_str() {
        // All Nix-evaluated policy types write an assigned per-lineage result
        // during evaluation. Heartbeat health is not policy evidence.
        "require_cf_agent" | "require_packages" | "custom_check" => match context {
            None => (
                ComplianceControlStatus::NotChecked,
                format!(
                    "No deployed evaluation is available for '{}' on {}.",
                    policy.name, system.hostname
                ),
                "No deployed system state has persisted policy results.".to_string(),
                "policy_eval",
                "No applicable Nix evaluation",
            ),
            Some(context) => match nix_policy_result(&context.policy_results, policy.id) {
                Ok(None) => (
                    ComplianceControlStatus::NotChecked,
                    format!(
                        "The deployed evaluation contains no result for '{}' on {}.",
                        policy.name, system.hostname
                    ),
                    format!(
                        "derivation_id={} policy_lineage_id={}",
                        context.derivation_id, policy.id
                    ),
                    "policy_eval",
                    "No applicable Nix policy result",
                ),
                Ok(Some((true, details))) => (
                    ComplianceControlStatus::Pass,
                    format!(
                        "The deployed Nix evaluation passed '{}' on {}.",
                        policy.name, system.hostname
                    ),
                    details.unwrap_or_else(|| "Persisted Nix policy result passed.".to_string()),
                    "policy_eval",
                    "Persisted Nix policy result",
                ),
                Ok(Some((false, details))) => (
                    ComplianceControlStatus::Fail,
                    format!(
                        "The deployed Nix evaluation failed '{}' on {}.",
                        policy.name, system.hostname
                    ),
                    details.unwrap_or_else(|| "Persisted Nix policy result failed.".to_string()),
                    "policy_eval",
                    "Persisted Nix policy result",
                ),
                Err(error) => (
                    ComplianceControlStatus::Error,
                    format!(
                        "The persisted evaluation result for '{}' is invalid.",
                        policy.name
                    ),
                    error.to_string(),
                    "policy_eval",
                    "Invalid persisted Nix policy result",
                ),
            },
        },
        "require_cve_check" => match context {
            None => (
                ComplianceControlStatus::NotChecked,
                format!(
                    "No deployed derivation is available to assess '{}' on {}.",
                    policy.name, system.hostname
                ),
                "No deployed system state has a derivation.".to_string(),
                "cve_scan",
                "No applicable CVE scan",
            ),
            Some(context) => {
                let scan: Option<(Uuid, i32, i32)> = sqlx::query_as(
                    r#"SELECT id, critical_count, high_count
                       FROM cve_scans
                       WHERE derivation_id = $1 AND status = 'completed'
                       ORDER BY completed_at DESC NULLS LAST, id DESC
                       LIMIT 1"#,
                )
                .bind(context.derivation_id)
                .fetch_optional(pool)
                .await?;
                match scan {
                    None => (
                        ComplianceControlStatus::NotChecked,
                        format!(
                            "No completed CVE scan is available for '{}' on {}.",
                            policy.name, system.hostname
                        ),
                        format!("derivation_id={}", context.derivation_id),
                        "cve_scan",
                        "No applicable CVE scan",
                    ),
                    Some((scan_id, critical_count, high_count)) => {
                        let max_critical = policy
                            .config
                            .get("max_critical")
                            .and_then(Value::as_i64)
                            .unwrap_or(i64::MAX);
                        let max_high = policy.config.get("max_high").and_then(Value::as_i64);
                        let failed = i64::from(critical_count) > max_critical
                            || max_high.is_some_and(|max| i64::from(high_count) > max);
                        let status = if failed {
                            ComplianceControlStatus::Fail
                        } else {
                            ComplianceControlStatus::Pass
                        };
                        (
                            status,
                            format!(
                                "Completed CVE scan assessed '{}' on {}.",
                                policy.name, system.hostname
                            ),
                            format!(
                                "scan_id={scan_id} critical_count={critical_count} high_count={high_count}"
                            ),
                            "cve_scan",
                            "Completed CVE scan",
                        )
                    }
                }
            }
        },
        _ => (
            ComplianceControlStatus::NotChecked,
            format!(
                "No applicable evidence found for '{}' on {}; control is not checked.",
                policy.name, system.hostname
            ),
            format!(
                "policy_type={} enabled={}",
                policy.policy_type, policy.enabled
            ),
            "policy_eval",
            "No applicable evidence",
        ),
    };

    Ok(control_evidence_with_resolved_status(
        system,
        policy,
        status,
        summary,
        body,
        artifact_type,
        artifact_title,
    ))
}

fn control_evidence_with_resolved_status(
    system: &SystemRow,
    policy: PolicyRow,
    status: ComplianceControlStatus,
    summary: String,
    body: String,
    artifact_type: &str,
    artifact_title: &str,
) -> ComplianceControlEvidence {
    let severity = policy
        .compliance_metadata
        .get("severity")
        .and_then(Value::as_str)
        .unwrap_or("info")
        .to_string();

    ComplianceControlEvidence {
        policy_id: policy.id,
        policy_name: policy.name.clone(),
        status,
        severity,
        summary,
        evidence_items: vec![ComplianceEvidenceItem {
            kind: artifact_type.to_string(),
            label: policy
                .description
                .clone()
                .unwrap_or_else(|| policy.name.clone()),
            body: body.clone(),
            artifact: Some(ComplianceEvidenceArtifact {
                artifact_type: artifact_type.to_string(),
                title: artifact_title.to_string(),
                body,
            }),
        }],
        framework_mapping: String::new(),
        control_family: policy
            .compliance_metadata
            .get("control_family")
            .and_then(Value::as_str)
            .map(str::to_string),
        cmmc_level: policy
            .compliance_metadata
            .get("cmmc_level")
            .and_then(Value::as_i64)
            .and_then(|level| i32::try_from(level).ok()),
        cis_section: policy
            .compliance_metadata
            .get("cis_section")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn control_evidence(system: &SystemRow, policy: PolicyRow) -> ComplianceControlEvidence {
    let eval = evaluate_policy(system, &policy);
    let status = match &eval {
        PolicyEval::Evaluated(s) => s.clone(),
        PolicyEval::Disabled | PolicyEval::Unsupported => ComplianceControlStatus::NotChecked,
    };

    let severity = policy
        .compliance_metadata
        .get("severity")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            match status {
                ComplianceControlStatus::Fail => "high",
                ComplianceControlStatus::Warn | ComplianceControlStatus::Error => "medium",
                ComplianceControlStatus::Pass | ComplianceControlStatus::Waiver => "low",
                ComplianceControlStatus::NotChecked | ComplianceControlStatus::NotApplicable => {
                    "info"
                }
            }
            .to_string()
        });

    let summary = match &eval {
        PolicyEval::Evaluated(ComplianceControlStatus::Pass) => format!(
            "{} satisfies {} based on current Crystal Forge evaluation data.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Warn) => format!(
            "{} requires reviewer attention for {}; the evaluation produced an indeterminate result.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Fail) => format!(
            "{} does not satisfy {} based on current Crystal Forge evaluation data.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Waiver) => format!(
            "{} has a waiver recorded for {}.",
            system.hostname, policy.name
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::NotChecked) => format!(
            "No applicable evidence found for '{}' on {}; control is not checked.",
            policy.name, system.hostname
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::NotApplicable) => format!(
            "Control '{}' does not apply to {}.",
            policy.name, system.hostname
        ),
        PolicyEval::Evaluated(ComplianceControlStatus::Error) => format!(
            "Evaluator error when assessing '{}' on {}.",
            policy.name, system.hostname
        ),
        PolicyEval::Disabled => format!(
            "Policy '{}' is disabled and was not evaluated on {}.",
            policy.name, system.hostname
        ),
        PolicyEval::Unsupported => format!(
            "Policy type '{}' is not yet supported by the Crystal Forge evaluator; '{}' was not evaluated on {}.",
            policy.policy_type, policy.name, system.hostname
        ),
    };

    // The evidence body contains the raw inputs Crystal Forge used.
    // This is evaluation input data, not auditor-collected evidence.
    let body = format!(
        "policy_type={} enabled={} health_status={} critical_cves={} high_cves={}",
        policy.policy_type,
        policy.enabled,
        system.health_status,
        system.critical_cve_count,
        system.high_cve_count
    );

    // Framework mapping: only emit when a real framework control identifier
    // exists on the policy. The policy_type → name string is an internal label,
    // not a framework mapping, so we omit it rather than fabricate one.
    let framework_mapping = String::new();

    ComplianceControlEvidence {
        policy_id: policy.id,
        policy_name: policy.name.clone(),
        status,
        severity,
        summary,
        evidence_items: vec![ComplianceEvidenceItem {
            kind: "policy_eval".to_string(),
            label: policy
                .description
                .clone()
                .unwrap_or_else(|| policy.name.clone()),
            body: body.clone(),
            artifact: Some(ComplianceEvidenceArtifact {
                artifact_type: if policy.policy_type == "require_cve_check" {
                    "cve_scan".to_string()
                } else {
                    "policy_eval".to_string()
                },
                // Accurate label: these are Crystal Forge evaluation inputs,
                // not authoritative auditor-collected evidence artifacts.
                title: "Crystal Forge evaluation inputs".to_string(),
                body,
            }),
        }],
        framework_mapping,
        control_family: policy
            .compliance_metadata
            .get("control_family")
            .and_then(Value::as_str)
            .map(str::to_string),
        cmmc_level: policy
            .compliance_metadata
            .get("cmmc_level")
            .and_then(Value::as_i64)
            .and_then(|level| i32::try_from(level).ok()),
        cis_section: policy
            .compliance_metadata
            .get("cis_section")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::resolver::{
        AssignmentMode, EffectivePolicy, EffectivePolicySource, PolicySpecificity, ProvenanceEntry,
    };
    use sqlx::PgPool;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn requirement_validation_allows_requirement_only_baselines() {
        let requirement_id = Uuid::new_v4();
        assert!(validate_bundle_request("bundle", "framework", &[], &[requirement_id]).is_ok());
        let error = validate_bundle_request("bundle", "framework", &[], &[]).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<BundleValidationError>(),
            Some(BundleValidationError::EmptyBaseline)
        ));
        assert_eq!(
            error.to_string(),
            "At least one policy or requirement is required"
        );
        assert!(
            validate_bundle_request(
                "bundle",
                "framework",
                &[],
                &[requirement_id, requirement_id]
            )
            .is_err()
        );
    }

    #[tokio::test]
    #[ignore = "requires an isolated migrated database"]
    async fn requirement_baseline_lifecycle_is_ordered_atomic_and_digest_independent() {
        let pool = PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .unwrap();
        let suffix = Uuid::new_v4().to_string();
        let mut tx = pool.begin().await.unwrap();
        let framework_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_frameworks (name, canonical_source_key) VALUES ($1, $2) RETURNING id",
        )
        .bind(format!("Baseline framework {suffix}"))
        .bind(format!("baseline-{suffix}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let framework_version_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_framework_versions (framework_id, version, canonical_release_key) VALUES ($1, '1', $2) RETURNING id",
        )
        .bind(framework_id)
        .bind(format!("release-{suffix}"))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let mut requirement_ids = Vec::new();
        for key in ["REQ-A", "REQ-B", "REQ-C"] {
            let requirement_id: Uuid = sqlx::query_scalar(
                "INSERT INTO compliance_requirements (framework_id, canonical_requirement_key) VALUES ($1, $2) RETURNING id",
            )
            .bind(framework_id)
            .bind(format!("{key}-{suffix}"))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            let version_id: Uuid = sqlx::query_scalar(
                "INSERT INTO compliance_requirement_versions (requirement_id, framework_version_id, external_id, kind) VALUES ($1, $2, $3, 'rule') RETURNING id",
            )
            .bind(requirement_id)
            .bind(framework_version_id)
            .bind(key)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            requirement_ids.push(version_id);
        }
        tx.commit().await.unwrap();

        let created = create_bundle(
            &pool,
            CreateComplianceBundleRequest {
                name: format!("Baseline bundle {suffix}"),
                framework: "Test".to_string(),
                version: Some("1".to_string()),
                description: None,
                layer: None,
                required_envs: vec![],
                policy_ids: vec![],
                requirement_version_ids: requirement_ids.clone(),
            },
        )
        .await
        .unwrap();
        let draft_id = created.current_draft_version_id.unwrap();
        let members = list_bundle_version_requirement_membership(&pool, draft_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            members
                .iter()
                .map(|member| member.requirement_version_id)
                .collect::<Vec<_>>(),
            requirement_ids
        );
        assert!(
            members
                .iter()
                .all(|member| member.framework_id == framework_id)
        );
        assert!(
            members
                .iter()
                .all(|member| member.framework_name.starts_with("Baseline framework"))
        );
        assert!(members.iter().all(|member| member.framework_version == "1"));
        let before: (String, String) = sqlx::query_as(
            "SELECT semantic_digest, requirement_digest FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(draft_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        let updated = update_bundle(
            &pool,
            created.id,
            UpdateComplianceBundleRequest {
                name: created.name.clone(),
                framework: created.framework.clone(),
                version: Some(created.version.clone()),
                description: created.description.clone(),
                required_envs: vec![],
                policy_ids: vec![],
                requirement_version_ids: requirement_ids.iter().rev().copied().collect(),
            },
            None,
        )
        .await
        .unwrap()
        .unwrap();
        let after_id = updated.current_draft_version_id.unwrap();
        let after: (String, String) = sqlx::query_as(
            "SELECT semantic_digest, requirement_digest FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(after_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(before.0, after.0);
        assert_eq!(before.1, after.1);
        assert_ne!(after.1, "pending");

        let mutated = update_bundle(
            &pool,
            created.id,
            UpdateComplianceBundleRequest {
                name: created.name.clone(),
                framework: created.framework.clone(),
                version: Some(created.version.clone()),
                description: created.description.clone(),
                required_envs: vec![],
                policy_ids: vec![],
                requirement_version_ids: vec![requirement_ids[0], requirement_ids[2]],
            },
            None,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(mutated.requirement_count, 2);
        let mutated_digest: String = sqlx::query_scalar(
            "SELECT requirement_digest FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(mutated.current_draft_version_id.unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(after.1, mutated_digest);

        let accepted_id = mutated.current_draft_version_id.unwrap();
        let mut publish_tx = pool.begin().await.unwrap();
        sqlx::query(
            "UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE current_draft_version_id = $1",
        )
        .bind(accepted_id)
        .execute(&mut *publish_tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', published_at = now() WHERE id = $1",
        )
        .bind(accepted_id)
        .execute(&mut *publish_tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(accepted_id)
        .bind(created.id)
        .execute(&mut *publish_tx)
        .await
        .unwrap();
        publish_tx.commit().await.unwrap();

        let published_summary = find_bundle(&pool, created.id).await.unwrap().unwrap();
        assert_eq!(published_summary.requirement_count, 2);

        let derived = update_bundle(
            &pool,
            created.id,
            UpdateComplianceBundleRequest {
                name: created.name.clone(),
                framework: created.framework.clone(),
                version: Some(created.version.clone()),
                description: created.description.clone(),
                required_envs: vec![],
                policy_ids: vec![],
                requirement_version_ids: vec![requirement_ids[1]],
            },
            None,
        )
        .await
        .unwrap()
        .unwrap();
        let derived_id = derived.current_draft_version_id.unwrap();
        assert_ne!(derived_id, accepted_id);
        let derived_from: Option<Uuid> = sqlx::query_scalar(
            "SELECT derived_from_version_id FROM compliance_bundle_versions WHERE id = $1",
        )
        .bind(derived_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(derived_from, Some(accepted_id));
        assert_eq!(derived.requirement_count, 1);

        let duplicate = create_bundle(
            &pool,
            CreateComplianceBundleRequest {
                name: format!("Duplicate baseline {suffix}"),
                framework: "Test".to_string(),
                version: None,
                description: None,
                layer: None,
                required_envs: vec![],
                policy_ids: vec![],
                requirement_version_ids: vec![requirement_ids[0], requirement_ids[0]],
            },
        )
        .await;
        assert!(duplicate.is_err());
        let missing = create_bundle(
            &pool,
            CreateComplianceBundleRequest {
                name: format!("Missing baseline {suffix}"),
                framework: "Test".to_string(),
                version: None,
                description: None,
                layer: None,
                required_envs: vec![],
                policy_ids: vec![],
                requirement_version_ids: vec![Uuid::new_v4()],
            },
        )
        .await;
        assert!(missing.is_err());
    }

    #[tokio::test]
    #[ignore = "requires live database with pg_stat_statements"]
    async fn bundle_summary_aggregate_query_count_is_bounded_across_versions() {
        let pool = PgPool::connect(
            &std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"),
        )
        .await
        .unwrap();
        sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
            .execute(&pool)
            .await
            .expect("pg_stat_statements extension must be available");

        let suffix = Uuid::new_v4().simple().to_string()[..8].to_string();
        let env_id = create_query_count_environment(&pool, &suffix).await;
        let system_ids = create_query_count_systems(&pool, env_id, &suffix).await;
        let custom_policy_id = create_published_query_count_policy(
            &pool,
            &format!("aggregate-query-custom-{suffix}"),
            "custom_check",
            serde_json::json!({ "expression": "config.security.auditd.enable", "strict": true }),
            &format!("aggregate-query-custom-digest-{suffix}"),
        )
        .await;
        let cve_policy_id = create_published_query_count_policy(
            &pool,
            &format!("aggregate-query-cve-{suffix}"),
            "require_cve_check",
            serde_json::json!({ "max_critical": 0, "max_high": 0 }),
            &format!("aggregate-query-cve-digest-{suffix}"),
        )
        .await;
        set_query_count_policy_results(&pool, custom_policy_id, &suffix).await;

        let mut summaries = Vec::new();
        for index in 0..20 {
            summaries.push(
                create_query_count_bundle(
                    &pool,
                    env_id,
                    &[custom_policy_id, cve_policy_id],
                    system_ids[index % system_ids.len()],
                    index,
                    &suffix,
                )
                .await,
            );
        }

        let one_version_calls =
            measured_bundle_summary_aggregate_calls(&pool, &summaries[..1]).await;
        let twenty_version_calls = measured_bundle_summary_aggregate_calls(&pool, &summaries).await;
        println!(
            "aggregate query count regression: one={one_version_calls}, twenty={twenty_version_calls}"
        );

        assert!(
            twenty_version_calls <= one_version_calls + 8,
            "aggregate query count should stay bounded across versions: one={one_version_calls}, twenty={twenty_version_calls}"
        );
    }

    async fn create_query_count_environment(pool: &PgPool, suffix: &str) -> Uuid {
        sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
            .bind(format!("aggregate-query-env-{suffix}"))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn create_query_count_systems(pool: &PgPool, env_id: Uuid, suffix: &str) -> Vec<Uuid> {
        let flake_id: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("aggregate-query-flake-{suffix}"))
                .bind(format!("https://example.invalid/{suffix}.git"))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("aggregate-query-{suffix}"))
        .fetch_one(pool)
        .await
        .unwrap();

        let mut system_ids = Vec::new();
        for index in 0..10 {
            let system_id = Uuid::new_v4();
            let hostname = format!("aggregate-query-system-{index}-{suffix}");
            let derivation_path = format!("/nix/store/{suffix}-{index}-system.drv");
            sqlx::query("INSERT INTO systems (id, hostname, environment_id, public_key, flake_id, derivation, is_active) VALUES ($1, $2, $3, $4, $5, $6, TRUE)")
                .bind(system_id)
                .bind(&hostname)
                .bind(env_id)
                .bind(format!("ssh-ed25519 aggregate-query-{index}"))
                .bind(flake_id)
                .bind(&derivation_path)
                .execute(pool)
                .await
                .unwrap();
            let derivation_id: i32 = sqlx::query_scalar(
                "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, store_path, expected_store_path, status_id, attempt_count, completed_at, policy_results) VALUES ($1, 'nixos', $2, $3, $3, $3, 10, 0, now(), '{}'::jsonb) RETURNING id",
            )
            .bind(commit_id)
            .bind(&hostname)
            .bind(&derivation_path)
            .fetch_one(pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO system_states (hostname, store_path, change_reason) VALUES ($1, $2, 'startup')")
                .bind(&hostname)
                .bind(&derivation_path)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO cve_scans (derivation_id, status, scanner_name, critical_count, high_count, completed_at) VALUES ($1, 'completed', 'query-count-test', 0, 0, now())")
                .bind(derivation_id)
                .execute(pool)
                .await
                .unwrap();
            system_ids.push(system_id);
        }
        system_ids
    }

    async fn create_published_query_count_policy(
        pool: &PgPool,
        name: &str,
        policy_type: &str,
        config: Value,
        digest: &str,
    ) -> Uuid {
        let policy_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(name)
        .bind(policy_type)
        .bind(config)
        .fetch_one(pool)
        .await
        .unwrap();
        let version_id: Uuid =
            sqlx::query_scalar("SELECT id FROM deployment_policy_versions WHERE policy_id = $1")
                .bind(policy_id)
                .fetch_one(pool)
                .await
                .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1 AND current_draft_version_id = $2")
            .bind(policy_id)
            .bind(version_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'accepted', semantic_digest = $2, trust_state = 'trusted', implementation_state = 'native' WHERE id = $1",
        )
        .bind(version_id)
        .bind(digest)
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
        version_id
    }

    async fn set_query_count_policy_results(pool: &PgPool, policy_version_id: Uuid, suffix: &str) {
        let (policy_id,): (Uuid,) =
            sqlx::query_as("SELECT policy_id FROM deployment_policy_versions WHERE id = $1")
                .bind(policy_version_id)
                .fetch_one(pool)
                .await
                .unwrap();
        let policy_results = serde_json::json!({
            "assigned": {
                policy_id.to_string(): { "passed": true, "details": "ok" }
            }
        });
        sqlx::query("UPDATE derivations SET policy_results = $1 WHERE derivation_path LIKE $2")
            .bind(policy_results)
            .bind(format!("/nix/store/{suffix}-%-system.drv"))
            .execute(pool)
            .await
            .unwrap();
    }

    async fn create_query_count_bundle(
        pool: &PgPool,
        env_id: Uuid,
        policy_version_ids: &[Uuid],
        system_id: Uuid,
        index: usize,
        suffix: &str,
    ) -> ComplianceBundleSummary {
        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, version, layer, owner) VALUES ($1, 'QUERY', '1.0', 'os', 'Tests') RETURNING id",
        )
        .bind(format!("aggregate-query-bundle-{index}-{suffix}"))
        .fetch_one(pool)
        .await
        .unwrap();
        let bundle_version_id: Uuid =
            sqlx::query_scalar("SELECT id FROM compliance_bundle_versions WHERE bundle_id = $1")
                .bind(bundle_id)
                .fetch_one(pool)
                .await
                .unwrap();
        for (order, policy_version_id) in policy_version_ids.iter().enumerate() {
            sqlx::query("INSERT INTO compliance_bundle_version_policies (bundle_version_id, policy_version_id, policy_order, selected) VALUES ($1, $2, $3, TRUE)")
                .bind(bundle_version_id)
                .bind(policy_version_id)
                .bind(order as i32)
                .execute(pool)
                .await
                .unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1 AND current_draft_version_id = $2")
            .bind(bundle_id)
            .bind(bundle_version_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', semantic_digest = $2, trust_state = 'trusted' WHERE id = $1",
        )
        .bind(bundle_version_id)
        .bind(format!("aggregate-query-bundle-digest-{index}-{suffix}"))
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        sqlx::query("INSERT INTO compliance_bundle_environments (bundle_id, environment_id) VALUES ($1, $2)")
            .bind(bundle_id)
            .bind(env_id)
            .execute(pool)
            .await
            .unwrap();
        let assignment_id = Uuid::new_v4();
        let assignment_version_id = Uuid::new_v4();
        sqlx::query("INSERT INTO compliance_bundle_assignments (id, bundle_id, bundle_version_id, scope_type, system_id, enforcement_mode, assignment_overlay_digest, active) VALUES ($1, $2, $3, 'system', $4, 'report_only', $5, TRUE)")
            .bind(assignment_id)
            .bind(bundle_id)
            .bind(bundle_version_id)
            .bind(system_id)
            .bind(format!("aggregate-query-assignment-digest-{index}-{suffix}"))
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO compliance_bundle_assignment_versions (id, assignment_id, version_number, bundle_version_id, enforcement_mode, assignment_overlay_digest) VALUES ($1, $2, 1, $3, 'report_only', $4)")
            .bind(assignment_version_id)
            .bind(assignment_id)
            .bind(bundle_version_id)
            .bind(format!("aggregate-query-assignment-version-digest-{index}-{suffix}"))
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2",
        )
        .bind(assignment_version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO compliance_assignment_value_overrides (assignment_id, assignment_version_id, policy_version_id, value_path, value) VALUES ($1, $2, $3, 'max_high', '0'::jsonb)")
            .bind(assignment_id)
            .bind(assignment_version_id)
            .bind(policy_version_ids[1])
            .execute(pool)
            .await
            .unwrap();

        let mut summary = bundle(
            bundle_id,
            &format!("aggregate-query-bundle-{index}-{suffix}"),
        );
        summary.current_published_version_id = Some(bundle_version_id);
        summary
    }

    async fn measured_bundle_summary_aggregate_calls(
        pool: &PgPool,
        summaries: &[ComplianceBundleSummary],
    ) -> i64 {
        sqlx::query("SELECT pg_stat_statements_reset()")
            .execute(pool)
            .await
            .unwrap();
        let aggregates = list_bundle_summary_aggregates(pool, summaries)
            .await
            .unwrap();
        assert_eq!(aggregates.len(), summaries.len());
        sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(calls), 0)::bigint
            FROM pg_stat_statements
            WHERE dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
              AND query NOT ILIKE '%pg_stat_statements%'
            "#,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn named_policy(policy_type: &str, name: &str, config: Value, enabled: bool) -> PolicyRow {
        PolicyRow {
            id: Uuid::nil(),
            bundle_id: Uuid::nil(),
            name: name.to_string(),
            description: None,
            policy_type: policy_type.to_string(),
            config,
            enabled,
            compliance_metadata: Value::Null,
        }
    }

    fn policy(policy_type: &str, config: Value, enabled: bool) -> PolicyRow {
        named_policy(policy_type, policy_type, config, enabled)
    }

    fn system(health_status: &str, critical: i32, high: i32) -> SystemRow {
        SystemRow {
            id: Uuid::nil(),
            hostname: "host-01".to_string(),
            environment: Some("prod".to_string()),
            health_status: health_status.to_string(),
            critical_cve_count: critical,
            high_cve_count: high,
        }
    }

    fn system_with_hostname(hostname: &str, environment: Option<&str>) -> SystemRow {
        SystemRow {
            id: Uuid::new_v4(),
            hostname: hostname.to_string(),
            environment: environment.map(str::to_string),
            health_status: "healthy".to_string(),
            critical_cve_count: 0,
            high_cve_count: 0,
        }
    }

    #[test]
    fn partitions_bundle_and_direct_effective_policies_without_duplication() {
        let bundle_a = Uuid::from_u128(101);
        let bundle_b = Uuid::from_u128(102);
        let bundle_policy = |version_id, bundle_id| EffectivePolicy {
            policy_version_id: version_id,
            policy_lineage_id: version_id,
            policy_type: "require_cf_agent".to_string(),
            source: EffectivePolicySource::Baseline,
            specificity: PolicySpecificity::Environment,
            baseline_order: Some(0),
            addition_order: None,
            overrides: Vec::new(),
            effective_config: Value::Null,
            assignment_mode: AssignmentMode::ReportOnly,
            effective_mode: AssignmentMode::ReportOnly,
            provenance: vec![ProvenanceEntry {
                source: EffectivePolicySource::Baseline,
                specificity: PolicySpecificity::Environment,
                assignment_id: Some(Uuid::from_u128(201)),
                bundle_id: Some(bundle_id),
                bundle_version_id: Some(Uuid::from_u128(301)),
                scope_type: Some("environment".to_string()),
                enforcement_mode: "report_only".to_string(),
                authoritative: true,
            }],
        };
        let mut direct_policy = bundle_policy(Uuid::from_u128(3), bundle_a);
        direct_policy.source = EffectivePolicySource::LegacyDirect;
        direct_policy.effective_mode = AssignmentMode::Enforce;

        let mut shared = bundle_policy(Uuid::from_u128(4), bundle_a);
        shared.provenance.push(ProvenanceEntry {
            source: EffectivePolicySource::Baseline,
            specificity: PolicySpecificity::Environment,
            assignment_id: Some(Uuid::from_u128(202)),
            bundle_id: Some(bundle_b),
            bundle_version_id: Some(Uuid::from_u128(302)),
            scope_type: Some("environment".to_string()),
            enforcement_mode: "report_only".to_string(),
            authoritative: false,
        });

        let (by_bundle, direct) = partition_effective_policies_by_bundle(&[
            bundle_policy(Uuid::from_u128(1), bundle_a),
            bundle_policy(Uuid::from_u128(2), bundle_b),
            shared,
            direct_policy,
        ]);

        assert_eq!(by_bundle[&bundle_a].len(), 2);
        assert_eq!(by_bundle[&bundle_b].len(), 1);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].policy_version_id, Uuid::from_u128(3));
        assert_eq!(
            by_bundle[&bundle_a][0].effective_mode,
            AssignmentMode::ReportOnly
        );
    }

    #[test]
    fn persisted_nix_result_uses_policy_lineage_identity() {
        let policy_id = Uuid::new_v4();
        let results = serde_json::json!({
            "assigned": {
                policy_id.to_string(): {
                    "passed": false,
                    "details": "firewall is disabled"
                }
            }
        });

        let result = nix_policy_result(&results, policy_id).unwrap();
        assert_eq!(
            result,
            Some((false, Some("firewall is disabled".to_string())))
        );
    }

    #[test]
    fn missing_persisted_nix_result_is_not_an_evaluation() {
        let result =
            nix_policy_result(&serde_json::json!({"assigned": {}}), Uuid::new_v4()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn malformed_persisted_nix_result_is_an_error() {
        let policy_id = Uuid::new_v4();
        let results = serde_json::json!({
            "assigned": { policy_id.to_string(): { "passed": "yes" } }
        });

        assert!(nix_policy_result(&results, policy_id).is_err());
    }

    fn bundle(id: Uuid, name: &str) -> ComplianceBundleSummary {
        ComplianceBundleSummary {
            id,
            name: name.to_string(),
            framework: "Test Framework".to_string(),
            version: "1.0".to_string(),
            description: None,
            layer: "infrastructure".to_string(),
            owner: "test-owner".to_string(),
            last_review: None,
            policy_ids: vec![],
            required_envs: vec![],
            policy_count: 0,
            requirement_count: 0,
            control_count: 0,
            environment_count: 0,
            active_assignment_count: 0,
            current_draft_version_id: None,
            current_published_version_id: None,
            current_draft_version: None,
            current_published_version: None,
            versions: vec![],
            applicable_system_count: 0,
            aggregate_score: None,
        }
    }

    fn bundled_policy(bundle_id: Uuid, name: &str, enabled: bool) -> PolicyRow {
        PolicyRow {
            id: Uuid::new_v4(),
            bundle_id,
            name: name.to_string(),
            description: None,
            policy_type: "require_cf_agent".to_string(),
            config: serde_json::json!({}),
            enabled,
            compliance_metadata: Value::Null,
        }
    }

    // ── require_cve_check ─────────────────────────────────────────────────────

    #[test]
    fn cve_policy_fails_when_critical_exceeds_threshold() {
        let status = policy_status(
            &system("healthy", 1, 0),
            &policy(
                "require_cve_check",
                serde_json::json!({ "max_critical": 0 }),
                true,
            ),
        );
        assert!(matches!(status, ComplianceControlStatus::Fail));
    }

    #[test]
    fn cve_policy_passes_when_within_threshold() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy(
                "require_cve_check",
                serde_json::json!({ "max_critical": 0 }),
                true,
            ),
        );
        assert!(matches!(status, ComplianceControlStatus::Pass));
    }

    #[test]
    fn cve_policy_warns_when_high_justification_required_and_high_cves_present() {
        let status = policy_status(
            &system("healthy", 0, 3),
            &policy(
                "require_cve_check",
                serde_json::json!({ "max_critical": 0, "require_high_justification": true }),
                true,
            ),
        );
        assert!(matches!(status, ComplianceControlStatus::Warn));
    }

    // ── require_cf_agent ──────────────────────────────────────────────────────

    #[test]
    fn agent_policy_passes_for_healthy_status() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Pass));
    }

    #[test]
    fn agent_policy_passes_for_online_status() {
        let status = policy_status(
            &system("online", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Pass));
    }

    #[test]
    fn agent_policy_fails_for_offline_status() {
        let status = policy_status(
            &system("offline", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), true),
        );
        assert!(matches!(status, ComplianceControlStatus::Fail));
    }

    #[test]
    fn agent_policy_warns_for_unknown_status_not_fabricate_pass() {
        // Any unrecognised status must not pass — it is indeterminate.
        for unknown in &["stale", "error", "unhealthy", "degraded", "unknown", ""] {
            let status = policy_status(
                &system(unknown, 0, 0),
                &policy("require_cf_agent", serde_json::json!({}), true),
            );
            assert!(
                matches!(status, ComplianceControlStatus::Warn),
                "expected Warn for health_status={unknown:?}, got {status:?}"
            );
        }
    }

    // ── disabled policies ─────────────────────────────────────────────────────

    // ── disabled policies ─────────────────────────────────────────────────────
    // Disabled and unsupported controls are `not_checked`, not `warn`.
    // Canonical categories are mutually exclusive:
    //   pass + warn + fail + waiver + not_checked + not_applicable + error == total

    #[test]
    fn disabled_policy_returns_not_checked_not_warn() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("require_cf_agent", serde_json::json!({}), false),
        );
        assert!(
            matches!(status, ComplianceControlStatus::NotChecked),
            "disabled policy must be NotChecked, not Warn"
        );
    }

    #[test]
    fn disabled_policy_is_excluded_from_score_denominator() {
        let sys = system("healthy", 0, 0);
        let rollup = system_rollup(
            sys,
            &[policy("require_cf_agent", serde_json::json!({}), false)],
            None,
        );
        assert_eq!(
            rollup.not_checked, 1,
            "disabled policy increments not_checked"
        );
        assert_eq!(rollup.warn, 0, "disabled policy must not increment warn");
        assert_eq!(
            rollup.score, 0,
            "score with only disabled policies should be 0"
        );
        assert_eq!(rollup.total, 1);
        assert_eq!(
            rollup.evaluated_total, 0,
            "disabled policy must not count as evaluated"
        );
        // Attention count = warn + not_checked + error — same total as before.
        assert_eq!(
            rollup.warn + rollup.not_checked + rollup.error,
            1,
            "attention_count must still be 1"
        );
    }

    // ── unsupported policy types ──────────────────────────────────────────────

    #[test]
    fn unsupported_policy_returns_not_checked_not_warn() {
        let status = policy_status(
            &system("healthy", 0, 0),
            &policy("custom_check", serde_json::json!({}), true),
        );
        assert!(
            matches!(status, ComplianceControlStatus::NotChecked),
            "unsupported policy must be NotChecked, not Warn"
        );
    }

    #[test]
    fn unsupported_policy_excluded_from_score_denominator() {
        let sys = system("healthy", 0, 0);
        let rollup = system_rollup(
            sys,
            &[policy(
                "custom_unsupported_type",
                serde_json::json!({}),
                true,
            )],
            None,
        );
        assert_eq!(
            rollup.not_checked, 1,
            "unsupported policy increments not_checked"
        );
        assert_eq!(rollup.warn, 0, "unsupported policy must not increment warn");
        assert_eq!(
            rollup.score, 0,
            "score with only unsupported policies should be 0"
        );
        assert_eq!(rollup.evaluated_total, 0);
    }

    // ── mixed: evaluated pass + disabled ─────────────────────────────────────
    // Canonical categories are mutually exclusive; disabled maps to not_checked.

    #[test]
    fn mixed_passing_and_disabled_scores_100_percent() {
        let sys = system("healthy", 0, 0);
        let policies = vec![
            // evaluated → pass
            policy("require_cf_agent", serde_json::json!({}), true),
            // not evaluated → not_checked (excluded from denominator)
            policy("require_cf_agent", serde_json::json!({}), false),
        ];
        let rollup = system_rollup(sys, &policies, None);

        assert_eq!(rollup.pass, 1, "one passing control");
        assert_eq!(rollup.not_checked, 1, "one disabled increments not_checked");
        assert_eq!(rollup.warn, 0, "no warn for disabled controls");
        assert_eq!(rollup.fail, 0);
        assert_eq!(rollup.evaluated_total, 1, "only one control was evaluated");
        assert_eq!(
            rollup.score, 100,
            "host score must be 100 with one pass and one disabled"
        );
    }

    #[test]
    fn canonical_categories_are_mutually_exclusive() {
        let sys = system("healthy", 0, 0);
        let policies = vec![
            policy("require_cf_agent", serde_json::json!({}), true), // pass
            policy("require_cf_agent", serde_json::json!({}), false), // not_checked
            policy("custom_check", serde_json::json!({}), true),     // not_checked (unsupported)
        ];
        let rollup = system_rollup(sys, &policies, None);
        let canonical_total = rollup.pass
            + rollup.warn
            + rollup.fail
            + rollup.waiver
            + rollup.not_checked
            + rollup.not_applicable
            + rollup.error;
        assert_eq!(
            canonical_total, rollup.total,
            "canonical categories must exactly sum to total"
        );
    }

    #[test]
    fn aggregate_score_uses_evaluated_controls_not_total() {
        let sys = system("healthy", 0, 0);
        let policies = vec![
            policy("require_cf_agent", serde_json::json!({}), true), // pass
            policy("require_cf_agent", serde_json::json!({}), false), // disabled → not_checked
        ];
        let rollup = system_rollup(sys, &policies, None);
        let totals = totals_for_rollups(&[rollup]);

        // Overall score must use evaluated_controls (1) not total_controls (2).
        assert_eq!(
            totals.overall_score, 100,
            "bundle overall score must be 100 when the only evaluated control passes"
        );
        assert_eq!(totals.evaluated_controls, 1);
        assert_eq!(totals.total_controls, 2);
    }

    #[test]
    fn aggregate_score_returns_none_without_evaluated_controls() {
        assert_eq!(aggregate_score(0, 0), None);
    }

    #[test]
    fn aggregate_score_returns_zero_for_one_failed_control() {
        assert_eq!(aggregate_score(0, 1), Some(0));
    }

    #[test]
    fn aggregate_score_returns_100_for_one_passing_control() {
        assert_eq!(aggregate_score(1, 1), Some(100));
    }

    #[test]
    fn fully_compliant_count_ignores_not_evaluated_controls() {
        let sys = system("healthy", 0, 0);
        let policies = vec![
            policy("require_cf_agent", serde_json::json!({}), true), // pass
            policy("require_cf_agent", serde_json::json!({}), false), // disabled → not_checked
        ];
        let rollup = system_rollup(sys, &policies, None);
        // not_checked = 1 (from disabled), warn = 0.
        // All *evaluated* controls pass, so the host must count as fully compliant.
        assert_eq!(rollup.not_checked, 1, "disabled maps to not_checked");
        assert_eq!(rollup.warn, 0, "no warn for disabled");
        assert_eq!(rollup.pass, 1);
        let totals = totals_for_rollups(&[rollup]);
        assert_eq!(
            totals.fully_compliant_count, 1,
            "host with all evaluated controls passing must count as fully compliant \
             even when disabled policies are not_checked"
        );
    }

    // ── evidence labels ───────────────────────────────────────────────────────

    #[test]
    fn evidence_artifact_title_is_not_authoritative() {
        let sys = system("healthy", 0, 0);
        let pol = policy("require_cf_agent", serde_json::json!({}), true);
        let ev = control_evidence(&sys, pol);
        let artifact = ev.evidence_items[0].artifact.as_ref().unwrap();
        assert_ne!(
            artifact.title, "Authoritative Crystal Forge signal",
            "artifact title must not claim to be authoritative auditor evidence"
        );
        assert!(
            artifact.title.contains("evaluation inputs")
                || artifact.title.contains("Crystal Forge"),
            "artifact title should describe evaluation inputs, got: {:?}",
            artifact.title
        );
    }

    #[test]
    fn framework_mapping_is_empty_when_no_real_mapping_exists() {
        let sys = system("healthy", 0, 0);
        let pol = named_policy(
            "require_cf_agent",
            "CF Agent Check",
            serde_json::json!({}),
            true,
        );
        let ev = control_evidence(&sys, pol);
        assert!(
            ev.framework_mapping.is_empty(),
            "framework_mapping must be empty when no real framework control ID exists, \
             got: {:?}",
            ev.framework_mapping
        );
    }

    #[test]
    fn system_bundle_assembly_filters_non_applicable_bundles() {
        let sys = system_with_hostname("test-host", Some("prod"));
        let bundle1_id = Uuid::new_v4();
        let bundle2_id = Uuid::new_v4();
        let bundle3_id = Uuid::new_v4();

        let mut applicable_ids = HashSet::new();
        applicable_ids.insert(bundle1_id);
        applicable_ids.insert(bundle3_id);

        let result = assemble_system_compliance_bundles(
            &sys,
            vec![
                bundle(bundle1_id, "Applicable Bundle 1"),
                bundle(bundle2_id, "Non-Applicable Bundle"),
                bundle(bundle3_id, "Applicable Bundle 2"),
            ],
            &applicable_ids,
            &HashMap::new(),
        );

        assert_eq!(result.len(), 2, "should only include applicable bundles");
        assert!(result.iter().any(|(bundle, _)| bundle.id == bundle1_id));
        assert!(result.iter().any(|(bundle, _)| bundle.id == bundle3_id));
        assert!(!result.iter().any(|(bundle, _)| bundle.id == bundle2_id));
    }

    #[test]
    fn system_bundle_assembly_handles_bundle_with_no_policies() {
        let sys = system_with_hostname("test-host", Some("prod"));
        let bundle_id = Uuid::new_v4();

        let mut applicable_ids = HashSet::new();
        applicable_ids.insert(bundle_id);

        let result = assemble_system_compliance_bundles(
            &sys,
            vec![bundle(bundle_id, "Empty Bundle")],
            &applicable_ids,
            &HashMap::new(),
        );

        assert_eq!(result.len(), 1);
        let (_, rollup) = &result[0];
        assert_eq!(rollup.hostname, "test-host");
        assert_eq!(rollup.total, 0);
        assert_eq!(rollup.pass, 0);
        assert_eq!(rollup.warn, 0);
        assert_eq!(rollup.fail, 0);
        assert_eq!(rollup.score, 0);
    }

    #[test]
    fn system_bundle_assembly_uses_policies_grouped_by_bundle() {
        let sys = system_with_hostname("test-host", Some("prod"));
        let bundle1_id = Uuid::new_v4();
        let bundle2_id = Uuid::new_v4();

        let mut applicable_ids = HashSet::new();
        applicable_ids.insert(bundle1_id);
        applicable_ids.insert(bundle2_id);

        let mut policies_by_bundle = HashMap::new();
        policies_by_bundle.insert(
            bundle1_id,
            vec![
                bundled_policy(bundle1_id, "policy1", true),
                bundled_policy(bundle1_id, "policy2", true),
            ],
        );
        policies_by_bundle.insert(
            bundle2_id,
            vec![bundled_policy(bundle2_id, "policy3", true)],
        );

        let result = assemble_system_compliance_bundles(
            &sys,
            vec![
                bundle(bundle1_id, "Bundle 1"),
                bundle(bundle2_id, "Bundle 2"),
            ],
            &applicable_ids,
            &policies_by_bundle,
        );

        assert_eq!(result.len(), 2);
        let bundle1_rollup = result
            .iter()
            .find(|(bundle, _)| bundle.id == bundle1_id)
            .expect("bundle 1 rollup should exist");
        let bundle2_rollup = result
            .iter()
            .find(|(bundle, _)| bundle.id == bundle2_id)
            .expect("bundle 2 rollup should exist");

        assert_eq!(bundle1_rollup.1.total, 2);
        assert_eq!(bundle2_rollup.1.total, 1);
    }

    #[test]
    fn system_bundle_assembly_returns_empty_for_empty_bundle_list() {
        let sys = system_with_hostname("test-host", None);

        let result =
            assemble_system_compliance_bundles(&sys, vec![], &HashSet::new(), &HashMap::new());

        assert!(result.is_empty());
    }

    #[test]
    fn system_rollup_preserves_system_info() {
        let sys = system_with_hostname("test-host-123", Some("staging"));
        let policies = vec![bundled_policy(Uuid::new_v4(), "test-policy", true)];

        let rollup = system_rollup(sys.clone(), &policies, None);

        assert_eq!(rollup.hostname, "test-host-123");
        assert_eq!(rollup.environment, Some("staging".to_string()));
        assert_eq!(rollup.system_id, sys.id);
    }

    #[test]
    fn effective_rollup_uses_selected_version_and_overlay_membership() {
        use crate::compliance::resolver::{
            AssignmentMode, EffectivePolicy, EffectivePolicySource, PolicySpecificity,
        };

        let sys = system("healthy", 0, 0);
        let baseline_version = Uuid::new_v4();
        let added_version = Uuid::new_v4();
        let excluded_version = Uuid::new_v4();
        let effective = vec![
            EffectivePolicy {
                policy_version_id: baseline_version,
                policy_lineage_id: Uuid::new_v4(),
                policy_type: "require_cf_agent".to_string(),
                source: EffectivePolicySource::Baseline,
                specificity: PolicySpecificity::BundleBaseline,
                baseline_order: Some(0),
                addition_order: None,
                overrides: vec![],
                effective_config: serde_json::json!({}),
                assignment_mode: AssignmentMode::Enforce,
                effective_mode: AssignmentMode::Enforce,
                provenance: vec![],
            },
            EffectivePolicy {
                policy_version_id: added_version,
                policy_lineage_id: Uuid::new_v4(),
                policy_type: "require_cve_check".to_string(),
                source: EffectivePolicySource::Addition,
                specificity: PolicySpecificity::System,
                baseline_order: None,
                addition_order: Some(0),
                overrides: vec![],
                effective_config: serde_json::json!({"max_critical": 0}),
                assignment_mode: AssignmentMode::Enforce,
                effective_mode: AssignmentMode::Enforce,
                provenance: vec![],
            },
        ];

        let rollup = effective_policy_rollup(&sys, &effective, None);

        assert_eq!(rollup.total, 2);
        assert_eq!(rollup.pass, 2);
        assert_eq!(rollup.evaluated_total, 2);
        assert!(
            effective
                .iter()
                .all(|policy| policy.policy_version_id != excluded_version)
        );
        assert!(
            effective
                .iter()
                .any(|policy| policy.policy_version_id == added_version)
        );
    }

    #[test]
    fn effective_rollup_counts_only_exact_selected_membership() {
        use crate::compliance::resolver::{
            AssignmentMode, EffectivePolicy, EffectivePolicySource, PolicySpecificity,
        };

        let sys = system("healthy", 1, 0);
        let selected_baseline = Uuid::from_u128(101);
        let selected_addition = Uuid::from_u128(102);
        let excluded_baseline = Uuid::from_u128(103);

        let effective = vec![
            EffectivePolicy {
                policy_version_id: selected_baseline,
                policy_lineage_id: Uuid::from_u128(201),
                policy_type: "require_cf_agent".to_string(),
                source: EffectivePolicySource::Baseline,
                specificity: PolicySpecificity::BundleBaseline,
                baseline_order: Some(0),
                addition_order: None,
                overrides: vec![],
                effective_config: serde_json::json!({}),
                assignment_mode: AssignmentMode::Enforce,
                effective_mode: AssignmentMode::Enforce,
                provenance: vec![],
            },
            EffectivePolicy {
                policy_version_id: selected_addition,
                policy_lineage_id: Uuid::from_u128(202),
                policy_type: "require_cve_check".to_string(),
                source: EffectivePolicySource::Addition,
                specificity: PolicySpecificity::System,
                baseline_order: None,
                addition_order: Some(0),
                overrides: vec![],
                effective_config: serde_json::json!({"max_critical": 0}),
                assignment_mode: AssignmentMode::ReportOnly,
                effective_mode: AssignmentMode::ReportOnly,
                provenance: vec![],
            },
        ];

        let rollup = effective_policy_rollup(&sys, &effective, None);

        assert_eq!(rollup.total, 2);
        assert_eq!(rollup.pass, 1);
        assert_eq!(rollup.fail, 1);
        assert_eq!(rollup.report_only, 1);
        assert_eq!(rollup.evaluated_total, 2);
        assert!(
            !effective
                .iter()
                .any(|policy| { policy.policy_version_id == excluded_baseline })
        );
    }

    #[test]
    fn effective_rollup_uses_overlay_value_override() {
        use crate::compliance::resolver::{
            AssignmentMode, EffectivePolicy, EffectivePolicySource, PolicyOverride,
            PolicySpecificity,
        };

        let sys = system("healthy", 1, 0);
        let policy_version_id = Uuid::from_u128(301);
        let effective = EffectivePolicy {
            policy_version_id,
            policy_lineage_id: Uuid::from_u128(302),
            policy_type: "require_cve_check".to_string(),
            source: EffectivePolicySource::Addition,
            specificity: PolicySpecificity::System,
            baseline_order: None,
            addition_order: Some(0),
            overrides: vec![PolicyOverride {
                policy_version_id,
                value_path: "max_critical".to_string(),
                value: serde_json::json!(2),
            }],
            effective_config: serde_json::json!({"max_critical": 2}),
            assignment_mode: AssignmentMode::Enforce,
            effective_mode: AssignmentMode::Enforce,
            provenance: vec![],
        };

        let rollup = effective_policy_rollup(&sys, &[effective], None);

        assert_eq!(rollup.total, 1);
        assert_eq!(rollup.pass, 1);
        assert_eq!(rollup.fail, 0);
        assert_eq!(rollup.evaluated_total, 1);
        assert_eq!(rollup.score, 100);
    }

    #[test]
    fn assignment_status_passed_through_rollup() {
        let sys = system("healthy", 0, 0);
        let statuses = vec![ComplianceControlStatus::Pass];

        // Test that assignment_status is correctly passed through
        let rollup = rollup_from_statuses(sys.clone(), &statuses, 0, Some("current".to_string()));
        assert_eq!(rollup.assignment_status, Some("current".to_string()));
        assert_eq!(rollup.pass, 1);

        // Test pinned status
        let rollup2 = rollup_from_statuses(sys.clone(), &statuses, 0, Some("pinned".to_string()));
        assert_eq!(rollup2.assignment_status, Some("pinned".to_string()));

        // Test no assignment
        let rollup3 = rollup_from_statuses(sys, &statuses, 0, None);
        assert_eq!(rollup3.assignment_status, None);
    }
}

// Integration test cases for list_system_bundles (database-dependent)
//
// Pure assembly and rollup tests live in the unit-test module above so they can
// exercise internal row types without widening the production API surface.
//
// The following integration test scenarios require database fixtures:
//
// 1. Unknown system returns None → 404
// 2. System with no applicable bundles → 200 + empty array
// 3. Environment-based applicability filtering (prod/dev/unscoped)
// 4. Rollup parity with existing bundle-systems endpoint
// 5. Query count is constant (4 queries) regardless of bundle count
// 6. Handler auth enforcement (403 without credentials)
//
// These should be implemented as #[sqlx::test] or VM integration tests.
