//! Read-only queries over the global compliance catalog for the MCP
//! compliance read tools (TASK-534.6).
//!
//! The catalog is security policies (`deployment_policies` lineages and
//! `deployment_policy_versions`), compliance bundles (`compliance_bundles`
//! lineages and `compliance_bundle_versions`) and framework requirements.
//! Catalog rows are global: they belong to no environment, and every
//! authenticated user can read them in the browser. These queries therefore
//! take no environment filter.
//!
//! These queries never select assignments, usage counts, systems or evidence.
//! They never write.
//!
//! Keyset pages order lineages by `(name COLLATE "C", id)`. `COLLATE "C"`
//! makes the order bytewise, so a cursor that stores the last name compares
//! the same way on every page, independent of the database collation.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Identifies one policy lineage and its current version pointers.
///
/// `current_draft_version_id` names the one shared draft of the lineage, if
/// any. A draft is mutable only while its `publication_state` is
/// `incomplete`, `draft` or `interim`.
#[derive(Debug, Clone, FromRow)]
pub struct CatalogPolicyLineage {
    /// Lineage ID (`deployment_policies.id`).
    pub id: Uuid,
    /// Lineage name. Unique among lineages.
    pub name: String,
    /// Lineage description, user-entered text.
    pub description: Option<String>,
    /// Current published (`accepted`) version, if any.
    pub current_published_version_id: Option<Uuid>,
    /// Current draft version, if any.
    pub current_draft_version_id: Option<Uuid>,
}

/// Holds one exact policy version. Values are returned as stored.
///
/// `config` may hold Nix expressions and other user-entered text. It is
/// untrusted data and is never evaluated by these queries.
#[derive(Debug, Clone, FromRow)]
pub struct CatalogPolicyVersion {
    /// Version ID (`deployment_policy_versions.id`).
    pub id: Uuid,
    /// Lineage ID.
    pub policy_id: Uuid,
    /// Version label, unique within the lineage.
    pub version: String,
    /// Version name.
    pub name: String,
    /// Version description.
    pub description: Option<String>,
    /// Policy type, for example `custom_check`.
    pub policy_type: String,
    /// `incomplete`, `draft`, `interim`, `accepted` or `deprecated`.
    pub publication_state: String,
    /// `untrusted`, `trusted` or `rejected`.
    pub trust_state: String,
    /// `native`, `manual`, `external`, `unbound` or `opaque`.
    pub implementation_state: String,
    /// Execution phase, for example `nix-evaluation`.
    pub execution_phase: String,
    /// Default enablement; `None` means enabled.
    pub enabled_by_default: Option<bool>,
    /// Type-specific configuration as stored.
    pub config: Value,
    /// Semantic digest of the version model.
    pub semantic_digest: String,
    /// Digest of the version's requirement mappings.
    pub mapping_digest: String,
    /// Source artifact that the version was imported from, if any.
    pub source_artifact_id: Option<Uuid>,
    /// Version that this version was derived from, if any.
    pub derived_from_version_id: Option<Uuid>,
    /// User who created the version, if recorded.
    pub created_by: Option<Uuid>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Publication time, if published.
    pub published_at: Option<DateTime<Utc>>,
    /// Optimistic-concurrency counter of a mutable version (migration 0344).
    /// It never changes after publication.
    pub draft_revision: i64,
    /// Reports that an owner access token created the version through MCP.
    pub created_via_agent: bool,
    /// Token name recorded at creation through MCP, if any.
    pub created_access_token_name: Option<String>,
}

const POLICY_VERSION_COLUMNS: &str = "v.id, v.policy_id, v.version, v.name, v.description, \
     v.policy_type, v.publication_state, v.trust_state, v.implementation_state, \
     v.execution_phase, v.enabled_by_default, v.config, v.semantic_digest, v.mapping_digest, \
     v.source_artifact_id, v.derived_from_version_id, v.created_by, v.created_at, \
     v.published_at, v.draft_revision, v.created_via_agent, v.created_access_token_name";

/// Filters of a policy lineage search. `None` applies no filter.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyCatalogFilter<'a> {
    /// Case-insensitive substring of the lineage name or description, or of
    /// the name or description of a current version.
    pub query: Option<&'a str>,
    /// Exact policy type of a current version.
    pub policy_type: Option<&'a str>,
    /// Exact publication state of a current version.
    pub publication_state: Option<&'a str>,
    /// Exact trust state of a current version.
    pub trust_state: Option<&'a str>,
}

/// Returns up to `limit` policy lineages after `after`, in
/// `(name COLLATE "C", id)` order.
///
/// The type and state filters match when one current version (the current
/// published or the current draft version) matches all of them. Historical
/// versions never match.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn search_policy_lineages(
    pool: &PgPool,
    filter: PolicyCatalogFilter<'_>,
    after: Option<(&str, Uuid)>,
    limit: i64,
) -> Result<Vec<CatalogPolicyLineage>> {
    sqlx::query_as::<_, CatalogPolicyLineage>(
        r#"
        SELECT dp.id, dp.name, dp.description,
               dp.current_published_version_id, dp.current_draft_version_id
          FROM deployment_policies dp
         WHERE ($1::text IS NULL
                OR dp.name COLLATE "C" > $1::text COLLATE "C"
                OR (dp.name = $1::text AND dp.id > $2::uuid))
           AND ($3::text IS NULL
                OR strpos(lower(dp.name), lower($3::text)) > 0
                OR strpos(lower(COALESCE(dp.description, '')), lower($3::text)) > 0
                OR EXISTS (
                    SELECT 1 FROM deployment_policy_versions v
                     WHERE v.id IN (dp.current_published_version_id, dp.current_draft_version_id)
                       AND (strpos(lower(v.name), lower($3::text)) > 0
                            OR strpos(lower(COALESCE(v.description, '')), lower($3::text)) > 0)))
           AND (($4::text IS NULL AND $5::text IS NULL AND $6::text IS NULL)
                OR EXISTS (
                    SELECT 1 FROM deployment_policy_versions v
                     WHERE v.id IN (dp.current_published_version_id, dp.current_draft_version_id)
                       AND ($4::text IS NULL OR v.policy_type = $4::text)
                       AND ($5::text IS NULL OR v.publication_state = $5::text)
                       AND ($6::text IS NULL OR v.trust_state = $6::text)))
         ORDER BY dp.name COLLATE "C", dp.id
         LIMIT $7
        "#,
    )
    .bind(after.map(|(name, _)| name))
    .bind(after.map(|(_, id)| id))
    .bind(filter.query)
    .bind(filter.policy_type)
    .bind(filter.publication_state)
    .bind(filter.trust_state)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to search policy lineages")
}

/// Returns one policy lineage, or `None` when it does not exist.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn find_policy_lineage(
    pool: &PgPool,
    policy_id: Uuid,
) -> Result<Option<CatalogPolicyLineage>> {
    sqlx::query_as::<_, CatalogPolicyLineage>(
        "SELECT id, name, description, current_published_version_id, current_draft_version_id \
           FROM deployment_policies WHERE id = $1",
    )
    .bind(policy_id)
    .fetch_optional(pool)
    .await
    .context("failed to load policy lineage")
}

/// Returns the policy versions with the given IDs, in no particular order.
/// Unknown IDs are skipped.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn fetch_policy_versions(
    pool: &PgPool,
    version_ids: &[Uuid],
) -> Result<Vec<CatalogPolicyVersion>> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, CatalogPolicyVersion>(&format!(
        "SELECT {POLICY_VERSION_COLUMNS} FROM deployment_policy_versions v WHERE v.id = ANY($1)"
    ))
    .bind(version_ids)
    .fetch_all(pool)
    .await
    .context("failed to load policy versions")
}

/// Returns up to `limit` versions of one policy lineage, newest first.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn list_policy_lineage_versions(
    pool: &PgPool,
    policy_id: Uuid,
    limit: i64,
) -> Result<Vec<CatalogPolicyVersion>> {
    sqlx::query_as::<_, CatalogPolicyVersion>(&format!(
        "SELECT {POLICY_VERSION_COLUMNS} FROM deployment_policy_versions v \
          WHERE v.policy_id = $1 ORDER BY v.created_at DESC, v.id DESC LIMIT $2"
    ))
    .bind(policy_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list policy lineage versions")
}

/// Identifies one compliance bundle lineage and its current version pointers.
#[derive(Debug, Clone, FromRow)]
pub struct CatalogBundleLineage {
    /// Lineage ID (`compliance_bundles.id`).
    pub id: Uuid,
    /// Lineage name. Unique among lineages.
    pub name: String,
    /// Current published (`accepted`) version, if any.
    pub current_published_version_id: Option<Uuid>,
    /// Current draft version, if any.
    pub current_draft_version_id: Option<Uuid>,
}

/// Holds one exact compliance bundle version. Values are returned as stored.
#[derive(Debug, Clone, FromRow)]
pub struct CatalogBundleVersion {
    /// Version ID (`compliance_bundle_versions.id`).
    pub id: Uuid,
    /// Lineage ID.
    pub bundle_id: Uuid,
    /// Version label, unique within the lineage.
    pub version: String,
    /// Version name.
    pub name: String,
    /// Framework label as entered.
    pub framework: String,
    /// Framework version label as entered.
    pub framework_version: Option<String>,
    /// Normalized source framework release, if any.
    pub framework_version_id: Option<Uuid>,
    /// Version description.
    pub description: Option<String>,
    /// Layer, for example `fleet`.
    pub layer: String,
    /// Owner label.
    pub owner: String,
    /// `incomplete`, `draft`, `interim`, `accepted` or `deprecated`.
    pub publication_state: String,
    /// `untrusted`, `trusted` or `rejected`.
    pub trust_state: String,
    /// Semantic digest of the version model and policy membership.
    pub semantic_digest: String,
    /// Digest of the requirement membership.
    pub requirement_digest: String,
    /// Source artifact that the version was imported from, if any.
    pub source_artifact_id: Option<Uuid>,
    /// Version that this version was derived from, if any.
    pub derived_from_version_id: Option<Uuid>,
    /// User who created the version, if recorded.
    pub created_by: Option<Uuid>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Publication time, if published.
    pub published_at: Option<DateTime<Utc>>,
}

const BUNDLE_VERSION_COLUMNS: &str = "v.id, v.bundle_id, v.version, v.name, v.framework, \
     v.framework_version, v.framework_version_id, v.description, v.layer, v.owner, \
     v.publication_state, v.trust_state, v.semantic_digest, v.requirement_digest, \
     v.source_artifact_id, v.derived_from_version_id, v.created_by, v.created_at, \
     v.published_at";

/// Returns up to `limit` bundle lineages after `after`, in
/// `(name COLLATE "C", id)` order. `query` is a case-insensitive substring of
/// the lineage name.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn search_bundle_lineages(
    pool: &PgPool,
    query: Option<&str>,
    after: Option<(&str, Uuid)>,
    limit: i64,
) -> Result<Vec<CatalogBundleLineage>> {
    sqlx::query_as::<_, CatalogBundleLineage>(
        r#"
        SELECT b.id, b.name, b.current_published_version_id, b.current_draft_version_id
          FROM compliance_bundles b
         WHERE ($1::text IS NULL
                OR b.name COLLATE "C" > $1::text COLLATE "C"
                OR (b.name = $1::text AND b.id > $2::uuid))
           AND ($3::text IS NULL OR strpos(lower(b.name), lower($3::text)) > 0)
         ORDER BY b.name COLLATE "C", b.id
         LIMIT $4
        "#,
    )
    .bind(after.map(|(name, _)| name))
    .bind(after.map(|(_, id)| id))
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to search bundle lineages")
}

/// Returns one bundle lineage, or `None` when it does not exist.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn find_bundle_lineage(
    pool: &PgPool,
    bundle_id: Uuid,
) -> Result<Option<CatalogBundleLineage>> {
    sqlx::query_as::<_, CatalogBundleLineage>(
        "SELECT id, name, current_published_version_id, current_draft_version_id \
           FROM compliance_bundles WHERE id = $1",
    )
    .bind(bundle_id)
    .fetch_optional(pool)
    .await
    .context("failed to load bundle lineage")
}

/// Returns the bundle versions with the given IDs, in no particular order.
/// Unknown IDs are skipped.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn fetch_bundle_versions(
    pool: &PgPool,
    version_ids: &[Uuid],
) -> Result<Vec<CatalogBundleVersion>> {
    if version_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, CatalogBundleVersion>(&format!(
        "SELECT {BUNDLE_VERSION_COLUMNS} FROM compliance_bundle_versions v WHERE v.id = ANY($1)"
    ))
    .bind(version_ids)
    .fetch_all(pool)
    .await
    .context("failed to load bundle versions")
}

/// Returns up to `limit` versions of one bundle lineage, newest first.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn list_bundle_lineage_versions(
    pool: &PgPool,
    bundle_id: Uuid,
    limit: i64,
) -> Result<Vec<CatalogBundleVersion>> {
    sqlx::query_as::<_, CatalogBundleVersion>(&format!(
        "SELECT {BUNDLE_VERSION_COLUMNS} FROM compliance_bundle_versions v \
          WHERE v.bundle_id = $1 ORDER BY v.created_at DESC, v.id DESC LIMIT $2"
    ))
    .bind(bundle_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list bundle lineage versions")
}

/// Reports whether a framework version (release) exists.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run the query.
pub async fn framework_version_exists(pool: &PgPool, framework_version_id: Uuid) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM compliance_framework_versions WHERE id = $1)")
        .bind(framework_version_id)
        .fetch_one(pool)
        .await
        .context("failed to check framework version")
}

/// Identifies one requirement version and its framework release.
#[derive(Debug, Clone, FromRow)]
pub struct CatalogRequirementVersion {
    /// Requirement version ID.
    pub id: Uuid,
    /// Requirement lineage ID.
    pub requirement_id: Uuid,
    /// Framework lineage ID.
    pub framework_id: Uuid,
    /// Framework lineage name.
    pub framework_name: String,
    /// Framework release ID.
    pub framework_version_id: Uuid,
    /// Framework release label.
    pub framework_version: String,
    /// Identifier in the source framework, for example a STIG rule ID.
    pub external_id: String,
    /// Requirement title.
    pub title: Option<String>,
    /// Node type, for example `rule` or `control`.
    pub kind: String,
    /// Severity, if the framework assigns one.
    pub severity: Option<String>,
}

/// Returns one requirement version with its framework release, or `None`
/// when it does not exist.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn find_requirement_version(
    pool: &PgPool,
    requirement_version_id: Uuid,
) -> Result<Option<CatalogRequirementVersion>> {
    sqlx::query_as::<_, CatalogRequirementVersion>(
        r#"
        SELECT rv.id, rv.requirement_id, f.id AS framework_id, f.name AS framework_name,
               fv.id AS framework_version_id, fv.version AS framework_version,
               rv.external_id, rv.title, rv.kind, rv.severity
          FROM compliance_requirement_versions rv
          JOIN compliance_framework_versions fv ON fv.id = rv.framework_version_id
          JOIN compliance_frameworks f ON f.id = fv.framework_id
         WHERE rv.id = $1
        "#,
    )
    .bind(requirement_version_id)
    .fetch_optional(pool)
    .await
    .context("failed to load requirement version")
}

/// Describes one mapping from an exact policy version to a requirement
/// version.
#[derive(Debug, Clone, FromRow)]
pub struct CatalogRequirementMapping {
    /// Mapping ID.
    pub id: Uuid,
    /// Exact mapped policy version.
    pub policy_version_id: Uuid,
    /// Lineage of the mapped policy version.
    pub policy_id: Uuid,
    /// Name of the mapped policy version.
    pub policy_name: String,
    /// Version label of the mapped policy version.
    pub policy_version: String,
    /// Publication state of the mapped policy version.
    pub policy_publication_state: String,
    /// `implements`, `supports` or `provides_evidence_for`.
    pub relationship: String,
    /// `full` or `partial`.
    pub coverage: String,
    /// `manual`, `imported`, `inherited`, `inferred` or `suggested`.
    pub provenance: String,
    /// Mapping trust state, for example `trusted` or `suggested`.
    pub trust_state: String,
    /// Rationale, user-entered text.
    pub rationale: Option<String>,
}

/// Returns up to `limit` mappings of one requirement version, ordered by
/// policy name, policy version label and mapping ID.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot run or decode the query.
pub async fn list_requirement_mappings(
    pool: &PgPool,
    requirement_version_id: Uuid,
    limit: i64,
) -> Result<Vec<CatalogRequirementMapping>> {
    sqlx::query_as::<_, CatalogRequirementMapping>(
        r#"
        SELECT m.id, m.policy_version_id, pv.policy_id, pv.name AS policy_name,
               pv.version AS policy_version, pv.publication_state AS policy_publication_state,
               m.relationship, m.coverage, m.provenance, m.trust_state, m.rationale
          FROM policy_requirement_mappings m
          JOIN deployment_policy_versions pv ON pv.id = m.policy_version_id
         WHERE m.requirement_version_id = $1
         ORDER BY pv.name COLLATE "C", pv.version COLLATE "C", m.id
         LIMIT $2
        "#,
    )
    .bind(requirement_version_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("failed to list requirement mappings")
}
