use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::ComplianceGroupingScheme;
use crate::compliance::digest::{
    BundleVersionCanonical, PolicyVersionCanonical, load_bundle_membership,
    write_assignment_effective_set_digest, write_bundle_version_digest,
    write_policy_version_digest,
};
use crate::compliance::resolver::{
    ResolutionOutcome, resolve_system_effective_policies,
    resolve_systems_effective_policies_for_bundle_version_batch,
};

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
        description: Option<String>,
        layer: String,
        owner: String,
        version: String,
    }
    let pub_ver: PublishedBundleVersion = sqlx::query_as(
        r#"
        SELECT name, framework, framework_version, description, layer, owner, version
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
            bundle_id, version, name, framework, framework_version,
            description, layer, owner, semantic_digest, derived_from_version_id,
            created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9, $10)
        RETURNING id
        "#,
    )
    .bind(bundle_id)
    .bind(&new_version)
    .bind(&pub_ver.name)
    .bind(&pub_ver.framework)
    .bind(&pub_ver.framework_version)
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
    PolicyRequired,
}

impl std::fmt::Display for BundleValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NameRequired => f.write_str("Bundle name is required"),
            Self::FrameworkRequired => f.write_str("Framework is required"),
            Self::PolicyRequired => f.write_str("At least one policy is required"),
        }
    }
}

impl std::error::Error for BundleValidationError {}

/// Validate fields common to both create and update.
/// Returns an `anyhow::Error` wrapping a [`BundleValidationError`] so callers
/// can downcast to distinguish validation failures from infrastructure errors.
fn validate_bundle_request(name: &str, framework: &str, policy_ids: &[Uuid]) -> Result<()> {
    if name.is_empty() {
        return Err(BundleValidationError::NameRequired.into());
    }
    if framework.is_empty() {
        return Err(BundleValidationError::FrameworkRequired.into());
    }
    if policy_ids.is_empty() {
        return Err(BundleValidationError::PolicyRequired.into());
    }
    Ok(())
}

use crate::api::models::{
    BundleVersionPolicyMembership, ComplianceBundleSummary, ComplianceBundleSystemsResponse,
    ComplianceBundleVersionSummary, ComplianceControlEvidence, ComplianceControlStatus,
    ComplianceEnvironmentRef, ComplianceEvidenceArtifact, ComplianceEvidenceItem,
    ComplianceEvidenceResponse, ComplianceRollupTotals, ComplianceSystemRollup,
    CreateComplianceBundleRequest, UpdateComplianceBundleRequest,
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
        control_count: row.control_count,
        environment_count: row.environment_count,
        active_assignment_count: row.active_assignment_count,
        current_draft_version_id: row.current_draft_version_id,
        current_published_version_id: row.current_published_version_id,
        current_draft_version: row.current_draft_version,
        current_published_version: row.current_published_version,
        versions: Vec::new(),
    }
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
            COALESCE(p.control_count, 0)::bigint AS control_count,
            COALESCE(e.environment_count, 0)::bigint AS environment_count,
            COALESCE(a.active_assignment_count, 0)::bigint AS active_assignment_count,
            b.current_draft_version_id,
            b.current_published_version_id,
            dv.version AS current_draft_version,
            pv.version AS current_published_version
        FROM compliance_bundles b
        LEFT JOIN LATERAL (
            SELECT
                array_agg(cbp.policy_id ORDER BY dp.name) AS policy_ids,
                count(*)::bigint AS control_count
            FROM compliance_bundle_policies cbp
            JOIN deployment_policies dp ON dp.id = cbp.policy_id
            WHERE cbp.bundle_id = b.id
        ) p ON TRUE
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
        ),
    >(
        r#"
        SELECT v.id, v.bundle_id, v.version, v.publication_state, v.trust_state,
               v.semantic_digest, v.created_at, v.published_at, v.derived_from_version_id,
               COUNT(cbvp.policy_version_id)::bigint AS control_count
        FROM compliance_bundle_versions v
        LEFT JOIN compliance_bundle_version_policies cbvp ON cbvp.bundle_version_id = v.id
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
                control_count: row.9,
                is_current_published: bundle.current_published_version_id == Some(row.0),
                is_current_draft: bundle.current_draft_version_id == Some(row.0),
            })
            .collect();
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

    validate_bundle_request(name, framework, &request.policy_ids)?;

    let mut tx = pool.begin().await?;

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
            COALESCE(p.control_count, 0)::bigint AS control_count,
            COALESCE(e.environment_count, 0)::bigint AS environment_count,
            COALESCE(a.active_assignment_count, 0)::bigint AS active_assignment_count,
            b.current_draft_version_id,
            b.current_published_version_id,
            dv.version AS current_draft_version,
            pv.version AS current_published_version
        FROM compliance_bundles b
        LEFT JOIN LATERAL (
            SELECT array_agg(policy_id ORDER BY policy_id) AS policy_ids, count(*)::bigint AS control_count
            FROM compliance_bundle_policies
            WHERE bundle_id = b.id
        ) p ON TRUE
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

    validate_bundle_request(name, framework, &request.policy_ids)?;

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
        RETURNING 1
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
    BlockedByImmutableHistory { version_ids: Vec<Uuid> },
    BlockedBySourceMappings { mapping_count: i64 },
    BlockedByAssignments { assignment_count: i64 },
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

    let exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM compliance_bundles WHERE id = $1 FOR UPDATE")
            .bind(bundle_id)
            .fetch_optional(&mut *tx)
            .await
            .context("Failed to lock compliance bundle")?;

    if exists.is_none() {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::NotFound);
    }

    let immutable_versions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM compliance_bundle_versions WHERE bundle_id = $1 AND publication_state IN ('accepted', 'deprecated') ORDER BY created_at, id",
    )
    .bind(bundle_id)
    .fetch_all(&mut *tx)
    .await
    .context("Failed to check compliance bundle immutable history")?;

    if !immutable_versions.is_empty() {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::BlockedByImmutableHistory {
            version_ids: immutable_versions,
        });
    }

    // Assignment lineage and version rows are immutable. Even inactive
    // assignments retain a RESTRICT reference to the bundle lineage, so they
    // must be reported as blockers rather than reaching the final DELETE and
    // surfacing as an FK error.
    let assignment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE bundle_id = $1",
    )
    .bind(bundle_id)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to check compliance bundle assignments")?;

    if assignment_count > 0 {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::BlockedByAssignments { assignment_count });
    }

    let mapping_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM compliance_source_object_mappings m
        JOIN compliance_bundle_versions v ON v.id = m.bundle_version_id
        WHERE v.bundle_id = $1
        "#,
    )
    .bind(bundle_id)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to check compliance bundle source mappings")?;

    if mapping_count > 0 {
        tx.rollback().await.ok();
        return Ok(BundleDeleteOutcome::BlockedBySourceMappings { mapping_count });
    }

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
    let policies = list_bundle_policies(pool, bundle_id).await?;
    let systems = list_applicable_system_rows(pool, bundle_id).await?;

    let rollups: Vec<_> = systems
        .into_iter()
        .map(|system| system_rollup(system, &policies))
        .collect();
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
    let is_current_published: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM compliance_bundles WHERE id = $1 AND current_published_version_id = $2)",
    )
    .bind(bundle_id)
    .bind(bundle_version_id)
    .fetch_one(pool)
    .await?;
    let systems = if is_current_published {
        list_applicable_system_rows(pool, bundle_id).await?
    } else {
        list_explicit_bundle_version_system_rows(pool, bundle_id, bundle_version_id).await?
    };
    let system_ids: Vec<Uuid> = systems.iter().map(|system| system.id).collect();
    let effective = resolve_systems_effective_policies_for_bundle_version_batch(
        pool,
        &system_ids,
        bundle_version_id,
    )
    .await?;
    let rollups: Vec<_> = systems
        .into_iter()
        .map(|system| match effective.get(&system.id) {
            Some(ResolutionOutcome::Resolved(set))
                if set.bundle_version_id == bundle_version_id =>
            {
                effective_policy_rollup(&system, &set.policies)
            }
            Some(ResolutionOutcome::Conflict(conflicts)) => unresolved_system_rollup(
                system,
                policies.len() as i64,
                conflicts
                    .first()
                    .map(|c| c.code.as_str())
                    .unwrap_or("conflict"),
            ),
            // Missing or mismatched resolution has no authoritative effective
            // set. Never substitute lineage/current membership for this view.
            _ => unresolved_system_rollup(system, policies.len() as i64, "not_applicable"),
        })
        .collect();
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
pub async fn list_system_bundles(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<Vec<(ComplianceBundleSummary, ComplianceSystemRollup)>>> {
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

    if all_bundles.is_empty() {
        return Ok(Some(Vec::new()));
    }

    // Determine which bundles apply to this system using set-based query
    // This replaces N individual applicability checks
    let applicable_bundle_ids_vec: Vec<Uuid> = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT DISTINCT b.id
        FROM compliance_bundles b
        LEFT JOIN environments e ON e.name = $2
        WHERE b.id = ANY($1)
          AND (
            NOT EXISTS (
                SELECT 1 FROM compliance_bundle_environments cbe
                WHERE cbe.bundle_id = b.id
            )
            OR EXISTS (
                SELECT 1 FROM compliance_bundle_environments cbe
                WHERE cbe.bundle_id = b.id AND cbe.environment_id = e.id
            )
          )
        "#,
    )
    .bind(all_bundles.iter().map(|b| b.id).collect::<Vec<_>>())
    .bind(&system.environment)
    .fetch_all(pool)
    .await?;

    // Convert to HashSet for O(1) membership checks
    let applicable_bundle_ids: std::collections::HashSet<Uuid> =
        applicable_bundle_ids_vec.into_iter().collect();

    if applicable_bundle_ids.is_empty() {
        return Ok(Some(Vec::new()));
    }

    // Fetch all policies for all applicable bundles in one query
    // This replaces N individual policy fetches
    let all_policies = sqlx::query_as::<_, PolicyRow>(
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
    .await?;

    // Group policies by bundle_id for O(1) lookup
    let mut policies_by_bundle: std::collections::HashMap<Uuid, Vec<PolicyRow>> =
        std::collections::HashMap::new();
    for policy in all_policies {
        policies_by_bundle
            .entry(policy.bundle_id)
            .or_insert_with(Vec::new)
            .push(policy);
    }

    // Compute legacy rollups first, then replace the assigned bundle's rollup
    // with the authoritative resolver result. This preserves visibility of
    // other applicable bundles while ensuring exclusions, additions, direct
    // policies, and overrides are reflected for the effective assignment.
    let mut result = assemble_system_compliance_bundles(
        &system,
        all_bundles,
        &applicable_bundle_ids,
        &policies_by_bundle,
    );

    if let ResolutionOutcome::Resolved(effective) =
        resolve_system_effective_policies(pool, system_id).await?
    {
        let resolved_bundle_id: Option<Uuid> =
            sqlx::query_scalar("SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1")
                .bind(effective.bundle_version_id)
                .fetch_optional(pool)
                .await?;
        if let Some(resolved_bundle_id) = resolved_bundle_id {
            if let Some((_, rollup)) = result
                .iter_mut()
                .find(|(bundle, _)| bundle.id == resolved_bundle_id)
            {
                *rollup = effective_policy_rollup(&system, &effective.policies);
            }
        }
    }

    Ok(Some(result))
}

/// Pure in-memory assembly of compliance bundles for a system.
/// Exported as pub(crate) for unit testing without database fixtures.
///
/// Given:
/// - A system row
/// - All bundles
/// - The set of applicable bundle IDs
/// - Policies grouped by bundle_id
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
        let rollup = system_rollup(system.clone(), &policies);
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
            let ids: Vec<Uuid> = effective
                .policies
                .iter()
                .map(|policy| policy.policy_version_id)
                .collect();
            let names = sqlx::query_as::<_, (Uuid, String, Option<String>, bool, Value)>(
                "SELECT id, name, description, enabled, compliance_metadata FROM deployment_policy_versions WHERE id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(pool)
            .await?;
            let names: std::collections::HashMap<Uuid, (String, Option<String>, bool, Value)> =
                names
                    .into_iter()
                    .map(|(id, name, description, enabled, metadata)| {
                        (id, (name, description, enabled, metadata))
                    })
                    .collect();
            policies = effective
                .policies
                .into_iter()
                .filter_map(|policy| {
                    let (name, description, enabled, compliance_metadata) =
                        names.get(&policy.policy_version_id)?.clone();
                    Some(PolicyRow {
                        id: policy.policy_version_id,
                        bundle_id,
                        name,
                        description,
                        policy_type: policy.policy_type,
                        config: policy.effective_config,
                        enabled,
                        compliance_metadata,
                    })
                })
                .collect();
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
        LEFT JOIN environments e ON e.name = v.environment
        WHERE v.id = $2
          AND (
            NOT EXISTS (
                SELECT 1 FROM compliance_bundle_environments cbe
                WHERE cbe.bundle_id = $1
            )
            OR EXISTS (
                SELECT 1 FROM compliance_bundle_environments cbe
                WHERE cbe.bundle_id = $1 AND cbe.environment_id = e.id
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
        LEFT JOIN environments e ON e.name = v.environment
        WHERE NOT EXISTS (
            SELECT 1 FROM compliance_bundle_environments cbe
            WHERE cbe.bundle_id = $1
        )
        OR EXISTS (
            SELECT 1 FROM compliance_bundle_environments cbe
            WHERE cbe.bundle_id = $1 AND cbe.environment_id = e.id
        )
        ORDER BY v.hostname ASC
        "#,
    )
    .bind(bundle_id)
    .fetch_all(pool)
    .await?)
}

pub(crate) fn system_rollup(system: SystemRow, policies: &[PolicyRow]) -> ComplianceSystemRollup {
    let mut pass = 0i64;
    let mut warn = 0i64;
    let mut fail = 0i64;
    let waiver = 0i64;
    let mut not_checked = 0i64;
    let mut not_applicable = 0i64;
    let mut error_count = 0i64;
    // Only policies that were actually evaluated count toward total and score.
    // Disabled and unsupported policies are surfaced as warn but excluded from
    // the denominator so they don't silently deflate the score.
    let mut evaluated_total = 0i64;

    for policy in policies {
        match evaluate_policy(&system, policy) {
            PolicyEval::Evaluated(ComplianceControlStatus::Pass) => {
                pass += 1;
                evaluated_total += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Warn) => {
                warn += 1;
                evaluated_total += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Fail) => {
                fail += 1;
                evaluated_total += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Waiver) => {
                evaluated_total += 1;
            }
            // Canonical evidence states: each control maps to exactly one
            // bucket. warn + not_checked + not_applicable + error + pass +
            // fail + waiver == total. No double-counting.
            PolicyEval::Evaluated(ComplianceControlStatus::NotChecked) => {
                not_checked += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::NotApplicable) => {
                not_applicable += 1;
            }
            PolicyEval::Evaluated(ComplianceControlStatus::Error) => {
                error_count += 1;
            }
            // Disabled or unsupported controls are selected but not evaluated.
            // They surface as not_checked, not warn, preserving mutual exclusivity.
            PolicyEval::Disabled | PolicyEval::Unsupported => {
                not_checked += 1;
            }
        }
    }

    // total = full bundle policy count (for UI display: "N of M controls evaluated").
    // evaluated_total = only the policies that were actually assessed; this is the
    // correct denominator for the score.
    let total = policies.len() as i64;
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
        report_only: 0,
        score,
        resolution_state: None,
    }
}

fn unresolved_system_rollup(
    system: SystemRow,
    selected_controls: i64,
    state: &str,
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
    }
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
            SELECT ss.derivation_path
            FROM system_states ss
            WHERE ss.hostname = s.hostname
            ORDER BY ss.timestamp DESC, ss.id DESC
            LIMIT 1
        ) deployed ON true
        JOIN derivations d ON d.derivation_path = deployed.derivation_path
        WHERE s.id = $1
          AND d.derivation_type = 'nixos'
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
    use std::collections::{HashMap, HashSet};

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
            control_count: 0,
            environment_count: 0,
            active_assignment_count: 0,
            current_draft_version_id: None,
            current_published_version_id: None,
            current_draft_version: None,
            current_published_version: None,
            versions: vec![],
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
        let rollup = system_rollup(sys, &policies);

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
        let rollup = system_rollup(sys, &policies);
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
        let rollup = system_rollup(sys, &policies);
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
    fn fully_compliant_count_ignores_not_evaluated_controls() {
        let sys = system("healthy", 0, 0);
        let policies = vec![
            policy("require_cf_agent", serde_json::json!({}), true), // pass
            policy("require_cf_agent", serde_json::json!({}), false), // disabled → not_checked
        ];
        let rollup = system_rollup(sys, &policies);
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

        let rollup = system_rollup(sys.clone(), &policies);

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

        let rollup = effective_policy_rollup(&sys, &effective);

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

        let rollup = effective_policy_rollup(&sys, &effective);

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

        let rollup = effective_policy_rollup(&sys, &[effective]);

        assert_eq!(rollup.total, 1);
        assert_eq!(rollup.pass, 1);
        assert_eq!(rollup.fail, 0);
        assert_eq!(rollup.evaluated_total, 1);
        assert_eq!(rollup.score, 100);
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
