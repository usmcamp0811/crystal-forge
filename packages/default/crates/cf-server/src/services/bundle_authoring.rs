//! Compliance bundle draft authoring and candidate impact preview for the MCP
//! bundle tools (TASK-534.8).
//!
//! This module owns three operations:
//!
//! - draft creation: a new bundle lineage with an initial draft
//!   ([`create_bundle_lineage_draft`]) or a draft derived from the published
//!   version ([`derive_bundle_draft`]);
//! - draft updates at an expected revision ([`update_bundle_draft`]), which
//!   can set the exact ordered policy-version membership of the draft;
//! - the read-only impact preview of a candidate bundle version on a system
//!   or an environment ([`preview_bundle_impact`]).
//!
//! The browser bundle routes keep their own lineage-based logic in
//! `queries::compliance` (`create_bundle`, `update_bundle`); this module
//! reuses `create_bundle_in_tx` and `ensure_bundle_draft` from there.
//!
//! # Authorization
//!
//! Every write first reauthorizes an access-token actor inside its
//! transaction with `bundles:draft` (see `authorize_token_tx` in
//! `services::policy_authoring`): the owner must be active and still an
//! Admin, and the token must be live and hold the scope. An access-token
//! actor changes only a mutable draft whose `created_by` is the token owner.
//! Published (`accepted`) and `deprecated` versions never change.
//!
//! The preview reads only systems in the caller's effective environments. A
//! system or environment outside them returns [`BundleAuthoringError::NotFound`],
//! the same result as an unknown ID.
//!
//! # Exact membership and the lineage membership trigger
//!
//! `compliance_bundle_policies` is the lineage-level membership that the
//! browser editor reads and writes. Its trigger
//! `sync_bundle_version_membership` (migration 0202) changes the current
//! draft's `compliance_bundle_version_policies` rows: an inserted lineage
//! adds that lineage's current draft or else published version at the end,
//! and a deleted lineage removes every version of that lineage and compacts
//! the order. The trigger therefore never selects an exact version.
//!
//! [`update_bundle_draft`] and [`create_bundle_lineage_draft`] write exact
//! membership in this order inside one transaction (see
//! `write_exact_membership_tx`):
//!
//! 1. Delete the lineage rows of lineages that leave the bundle, then insert
//!    the lineage rows of lineages that join. The trigger runs and changes the
//!    version rows.
//! 2. Delete every version row of the draft and insert the requested exact
//!    versions with `policy_order` 0, 1, ... and `selected = true`.
//!
//! INVARIANT: After step 2 the lineage set equals the set of lineages of the
//! version rows, and the version rows equal the request exactly. Step 2 runs
//! last, so no trigger can replace a requested version with a lineage's
//! newest version. The digest is then recomputed from the stored rows, as
//! after a browser edit.
//!
//! # Concurrency
//!
//! CONCURRENCY: A write locks in this order: owner, token and role rows
//! (`FOR SHARE`, in `authorize_token_tx`), the bundle lineage row
//! `FOR UPDATE`, the bundle version row `FOR UPDATE`, then the referenced
//! policy versions `FOR SHARE` in ID order. `ensure_bundle_draft`,
//! `write_bundle_version_digest` and the browser `update_bundle` lock the
//! lineage row before the version row, and bundle publication locks the bundle
//! version before its policy versions, so no wait-for cycle can form. Two
//! updates of one draft serialize on the lineage row; the later one then sees
//! the new `draft_revision` and fails with
//! [`BundleAuthoringError::StaleRevision`].
//!
//! # Negative guarantees
//!
//! No function here trusts, publishes, assigns, deletes, changes an
//! enforcement mode or schedules work. [`preview_bundle_impact`] runs in a
//! `READ ONLY` transaction that is always rolled back.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::CreateComplianceBundleRequest;
use crate::compliance::digest::{
    refresh_bundle_version_digest, write_assignment_effective_set_digest,
};
use crate::compliance::resolver::{
    AssignmentMode, AssignmentTarget, CandidateAssignment, EffectivePolicySource,
    ResolutionConflict, ResolutionOutcome, resolve_system_effective_policies_in_tx,
    resolve_system_effective_policies_with_candidate_in_tx,
    resolve_systems_effective_policies_in_tx,
};
use crate::queries::compliance::{
    BundleDraftDerivationError, BundleDraftIntent, create_bundle_in_tx, ensure_bundle_draft,
};
use crate::queries::framework_requirements::{
    RequirementCoverage, compute_bundle_requirement_coverage,
};
use crate::services::policy_authoring::{
    AuthoringActor, PolicyAuthoringError, authorize_token_tx, insert_policy_audit_event,
};

/// Scope that permits bundle draft creation and updates.
pub const BUNDLES_DRAFT_SCOPE: &str = "bundles:draft";

/// Message of [`BundleAuthoringError::Forbidden`] for a draft that another
/// user created. It reveals nothing else about the draft.
pub const BUNDLE_DRAFT_OWNED_BY_OTHER_USER: &str = "bundle draft is owned by another user";

/// Most systems that one environment preview resolves. An environment with
/// more systems returns the first systems by hostname and `truncated: true`.
/// The bound keeps one preview within the 30-second MCP request deadline:
/// each system costs two resolver passes in one transaction.
pub const MAX_PREVIEW_SYSTEMS: usize = 100;

/// Most requirement coverage gaps listed in one preview. The counts always
/// cover every requirement.
const MAX_COVERAGE_GAPS: usize = 100;

const CREATE_FAILED: &str = "Failed to create bundle draft";
const UPDATE_FAILED: &str = "Failed to update bundle draft";
const PREVIEW_FAILED: &str = "Failed to preview bundle impact";

/// Reports why a bundle authoring operation wrote nothing or a preview
/// returned no result.
///
/// Every error leaves the database unchanged: the operation's transaction
/// rolls back when the error is returned.
#[derive(Debug)]
pub enum BundleAuthoringError {
    /// The token, the owner or the draft owner check failed. The message
    /// names the missing authority and contains no resource data.
    Forbidden(String),
    /// The lineage, version, system or environment does not exist or is
    /// outside the caller's effective environments.
    NotFound,
    /// The caller's input or the target state does not permit the operation.
    Invalid {
        /// Stable reason code, for example `duplicate_policy_lineage`.
        code: &'static str,
        /// Describes the problem with the caller's own input.
        message: String,
    },
    /// The draft changed since the caller read it.
    StaleRevision {
        /// The draft's current revision.
        current: i64,
    },
    /// The lineage has no published version to derive a draft from.
    NoPublishedSource,
    /// The lineage already has this mutable draft. The ID is the only detail
    /// revealed; its creator is not.
    MutableDraftExists(Uuid),
    /// A database or invariant failure. `context` is the public message.
    Internal {
        /// Public description of the failed operation.
        context: &'static str,
        /// Detail for server logs only.
        error: anyhow::Error,
    },
}

fn internal<E>(context: &'static str) -> impl FnOnce(E) -> BundleAuthoringError
where
    E: Into<anyhow::Error>,
{
    move |error| BundleAuthoringError::Internal {
        context,
        error: error.into(),
    }
}

fn invalid(code: &'static str, message: String) -> BundleAuthoringError {
    BundleAuthoringError::Invalid { code, message }
}

async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthoringActor,
) -> Result<(), BundleAuthoringError> {
    authorize_token_tx(tx, actor, BUNDLES_DRAFT_SCOPE)
        .await
        .map_err(|error| match error {
            PolicyAuthoringError::Forbidden(message) => BundleAuthoringError::Forbidden(message),
            PolicyAuthoringError::Internal { context, error } => {
                BundleAuthoringError::Internal { context, error }
            }
            other => BundleAuthoringError::Internal {
                context: "Failed to authorize the access token",
                error: anyhow::anyhow!("unexpected authorization result: {other:?}"),
            },
        })
}

fn is_mutable_state(publication_state: &str) -> bool {
    matches!(publication_state, "incomplete" | "draft" | "interim")
}

/// Implementation states that publication requires to be trusted (the rule
/// of `publish_bundle_version` and of the resolver).
fn is_executable(implementation_state: &str) -> bool {
    matches!(implementation_state, "native" | "external" | "manual")
}

// ── Exact membership validation ─────────────────────────────────────────────

/// Reports a requested member that publication does not accept yet. The
/// draft can still be saved; an Admin must publish or trust the member in
/// the browser before the bundle can be published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MembershipWarning {
    /// `draft_member` or `untrusted_member`.
    pub code: &'static str,
    /// Exact policy version that the warning is about.
    pub policy_version_id: Uuid,
    /// Explains the publication step that the member still needs.
    pub message: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct MemberVersionRow {
    id: Uuid,
    policy_id: Uuid,
    publication_state: String,
    trust_state: String,
    implementation_state: String,
}

/// Checks a requested ordered membership against the stored policy versions
/// and returns `(policy_version_id, policy_lineage_id)` in request order with
/// the publication warnings.
///
/// Errors (nothing is written): a repeated version (`duplicate_policy_version`),
/// an unknown version (`unknown_policy_version`), two versions of one lineage
/// (`duplicate_policy_lineage`), and a member that `publish_bundle_version`
/// can never accept (`member_never_publishable`): a `deprecated` version, or
/// an `accepted` executable version that is not trusted. Both are immutable,
/// so no later review can make them publishable.
///
/// Warnings: a member that is not `accepted` yet (`draft_member`) and an
/// executable member that is not trusted yet (`untrusted_member`).
fn classify_members(
    requested: &[Uuid],
    rows: &[MemberVersionRow],
) -> Result<(Vec<(Uuid, Uuid)>, Vec<MembershipWarning>), BundleAuthoringError> {
    let mut seen = HashSet::new();
    if let Some(duplicate) = requested.iter().find(|id| !seen.insert(**id)) {
        return Err(invalid(
            "duplicate_policy_version",
            format!("policy version {duplicate} appears more than once"),
        ));
    }
    let by_id: HashMap<Uuid, &MemberVersionRow> = rows.iter().map(|row| (row.id, row)).collect();
    let mut lineages = HashMap::new();
    let mut members = Vec::with_capacity(requested.len());
    let mut warnings = Vec::new();
    for id in requested {
        let Some(row) = by_id.get(id) else {
            return Err(invalid(
                "unknown_policy_version",
                format!("policy version {id} does not exist"),
            ));
        };
        if let Some(other) = lineages.insert(row.policy_id, *id) {
            return Err(invalid(
                "duplicate_policy_lineage",
                format!(
                    "policy versions {other} and {id} belong to the same policy lineage {}; a \
                     bundle holds at most one version per lineage",
                    row.policy_id
                ),
            ));
        }
        let executable_untrusted =
            is_executable(&row.implementation_state) && row.trust_state != "trusted";
        let never_publishable = row.publication_state == "deprecated"
            || (row.publication_state == "accepted" && executable_untrusted);
        if never_publishable {
            return Err(invalid(
                "member_never_publishable",
                format!(
                    "policy version {id} is {} and {}; bundle publication can never accept it",
                    row.publication_state, row.trust_state
                ),
            ));
        }
        if row.publication_state != "accepted" {
            warnings.push(MembershipWarning {
                code: "draft_member",
                policy_version_id: *id,
                message: format!(
                    "The member is {}. An Admin must publish it before the bundle can be \
                     published.",
                    row.publication_state
                ),
            });
        }
        if executable_untrusted {
            warnings.push(MembershipWarning {
                code: "untrusted_member",
                policy_version_id: *id,
                message: format!(
                    "The member is {}. An Admin must trust it before the bundle can be published.",
                    row.trust_state
                ),
            });
        }
        members.push((*id, row.policy_id));
    }
    Ok((members, warnings))
}

/// Locks the requested policy versions `FOR SHARE` in ID order and checks
/// them with [`classify_members`].
async fn validate_members_tx(
    tx: &mut Transaction<'_, Postgres>,
    requested: &[Uuid],
    context: &'static str,
) -> Result<(Vec<(Uuid, Uuid)>, Vec<MembershipWarning>), BundleAuthoringError> {
    let mut sorted = requested.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    // CONCURRENCY: `FOR SHARE` keeps the members' state (publication, trust)
    // fixed until commit; ID order gives every writer the same lock order.
    let rows = sqlx::query_as::<_, MemberVersionRow>(
        r#"SELECT id, policy_id, publication_state, trust_state, implementation_state
           FROM deployment_policy_versions
           WHERE id = ANY($1)
           ORDER BY id
           FOR SHARE"#,
    )
    .bind(&sorted)
    .fetch_all(&mut **tx)
    .await
    .map_err(internal(context))?;
    classify_members(requested, &rows)
}

/// Replaces the membership of the lineage's current draft with exact policy
/// versions. See the module documentation for the order of the writes.
///
/// Preconditions: the caller holds the lineage and version row locks,
/// `version_id` is the lineage's current mutable draft, and `members` passed
/// [`classify_members`].
async fn write_exact_membership_tx(
    tx: &mut Transaction<'_, Postgres>,
    bundle_id: Uuid,
    version_id: Uuid,
    members: &[(Uuid, Uuid)],
) -> anyhow::Result<()> {
    let version_ids = members.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let lineage_ids = members
        .iter()
        .map(|(_, lineage)| *lineage)
        .collect::<Vec<_>>();
    // Step 1: lineage rows. The membership trigger changes the draft's
    // version rows; step 2 replaces them.
    sqlx::query(
        "DELETE FROM compliance_bundle_policies WHERE bundle_id = $1 AND NOT (policy_id = ANY($2))",
    )
    .bind(bundle_id)
    .bind(&lineage_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"INSERT INTO compliance_bundle_policies (bundle_id, policy_id)
           SELECT $1, lineage FROM unnest($2::uuid[]) AS lineage
           ON CONFLICT DO NOTHING"#,
    )
    .bind(bundle_id)
    .bind(&lineage_ids)
    .execute(&mut **tx)
    .await?;
    // Step 2: exact version rows in request order.
    sqlx::query("DELETE FROM compliance_bundle_version_policies WHERE bundle_version_id = $1")
        .bind(version_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO compliance_bundle_version_policies
               (bundle_version_id, policy_version_id, policy_order, selected)
           SELECT $1, member.id, (member.ordinal - 1)::integer, true
           FROM unnest($2::uuid[]) WITH ORDINALITY AS member(id, ordinal)"#,
    )
    .bind(version_id)
    .bind(&version_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Recomputes the draft's semantic digest from its stored rows and the
/// overlay digests of assignments that target the draft, as the browser
/// `update_bundle` does after a change.
async fn refresh_draft_digests_tx(
    tx: &mut Transaction<'_, Postgres>,
    version_id: Uuid,
) -> anyhow::Result<()> {
    refresh_bundle_version_digest(tx, version_id).await?;
    let assignment_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM compliance_bundle_assignments
           WHERE bundle_version_id = $1
           ORDER BY id
           FOR UPDATE"#,
    )
    .bind(version_id)
    .fetch_all(&mut **tx)
    .await?;
    for assignment_id in assignment_ids {
        write_assignment_effective_set_digest(tx, assignment_id).await?;
    }
    Ok(())
}

/// Records on a bundle version that an access token created it. A browser
/// actor records nothing; the columns keep their defaults.
async fn record_agent_attribution(
    tx: &mut Transaction<'_, Postgres>,
    version_id: Uuid,
    actor: &AuthoringActor,
) -> Result<(), sqlx::Error> {
    let Some(token_id) = actor.access_token_id else {
        return Ok(());
    };
    // INVARIANT: The revision trigger ignores the attribution columns, so
    // this UPDATE never changes `draft_revision`.
    sqlx::query(
        r#"UPDATE compliance_bundle_versions
           SET created_via_agent = true,
               created_access_token_id = $2,
               created_access_token_name = (SELECT name FROM api_access_tokens WHERE id = $2)
           WHERE id = $1"#,
    )
    .bind(version_id)
    .bind(token_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ── Draft creation and update ───────────────────────────────────────────────

/// Describes the bundle draft that an operation created or changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BundleDraftOutcome {
    /// Bundle lineage ID.
    pub bundle_id: Uuid,
    /// Exact draft version ID.
    pub version_id: Uuid,
    /// Version label, unique within the lineage.
    pub version: String,
    /// Current draft revision; unchanged for a no-op update.
    pub draft_revision: i64,
    /// Published version that the draft was derived from, if any.
    pub derived_from_version_id: Option<Uuid>,
    /// `false` when an update found nothing to change and wrote nothing.
    pub changed: bool,
    /// Names of the changed fields; empty for creation.
    pub changed_fields: Vec<&'static str>,
    /// Members that publication does not accept yet.
    pub warnings: Vec<MembershipWarning>,
}

/// Describes a new bundle lineage with its initial draft.
#[derive(Debug, Clone)]
pub struct NewBundleLineage {
    /// Lineage name; unique, 1 to 255 bytes after trimming.
    pub name: String,
    /// Description; blank means none.
    pub description: Option<String>,
    /// Framework label, for example `NIST 800-53`; not blank.
    pub framework: String,
    /// Framework version label, for example `rev5`.
    pub framework_version: Option<String>,
    /// Initial exact ordered membership; may be empty.
    pub policy_version_ids: Vec<Uuid>,
}

async fn draft_summary(
    tx: &mut Transaction<'_, Postgres>,
    version_id: Uuid,
) -> Result<(String, Option<Uuid>, i64), sqlx::Error> {
    sqlx::query_as(
        "SELECT version, derived_from_version_id, draft_revision \
         FROM compliance_bundle_versions WHERE id = $1",
    )
    .bind(version_id)
    .fetch_one(&mut **tx)
    .await
}

/// Creates a bundle lineage with an initial untrusted draft for an actor.
///
/// The MCP tool `create_bundle_draft` (`mode: new_lineage`) uses this
/// function. In one transaction it reauthorizes a token actor with
/// `bundles:draft`, takes the `bundle-name:<name>` advisory lock, rejects a
/// used name, validates and locks the initial members, creates the lineage
/// through [`create_bundle_in_tx`] with `created_by = actor.user_id`, writes
/// the exact membership, records token attribution and writes the
/// `bundle_draft_created` audit event.
///
/// # Errors
///
/// - [`BundleAuthoringError::Invalid`] for a blank or long name
///   (`invalid_name`), a blank framework (`invalid_framework`), a used name
///   (`name_conflict`) or invalid members (see [`classify_members`]).
/// - [`BundleAuthoringError::Forbidden`] when a token actor is no longer
///   authorized.
/// - [`BundleAuthoringError::Internal`] on a database failure.
pub async fn create_bundle_lineage_draft(
    pool: &PgPool,
    actor: &AuthoringActor,
    input: &NewBundleLineage,
) -> Result<BundleDraftOutcome, BundleAuthoringError> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(invalid(
            "invalid_name",
            "name must be 1 to 255 bytes after trimming".to_owned(),
        ));
    }
    let framework = input.framework.trim();
    if framework.is_empty() {
        return Err(invalid(
            "invalid_framework",
            "framework must not be blank".to_owned(),
        ));
    }

    let mut tx = pool.begin().await.map_err(internal(CREATE_FAILED))?;
    authorize(&mut tx, actor).await?;
    // CONCURRENCY: Two creates of one name serialize here; the later one
    // then sees the first lineage and fails with `name_conflict` instead of
    // a unique-constraint error.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("bundle-name:{name}"))
        .execute(&mut *tx)
        .await
        .map_err(internal(CREATE_FAILED))?;
    let taken: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM compliance_bundles WHERE name = $1)")
            .bind(name)
            .fetch_one(&mut *tx)
            .await
            .map_err(internal(CREATE_FAILED))?;
    if taken {
        return Err(invalid(
            "name_conflict",
            format!("a compliance bundle named '{name}' already exists"),
        ));
    }
    let (members, warnings) =
        validate_members_tx(&mut tx, &input.policy_version_ids, CREATE_FAILED).await?;
    let request = CreateComplianceBundleRequest {
        name: name.to_owned(),
        framework: framework.to_owned(),
        version: input
            .framework_version
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        description: input
            .description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        layer: None,
        required_envs: Vec::new(),
        policy_ids: Vec::new(),
        requirement_version_ids: Vec::new(),
    };
    let (bundle_id, version_id) = create_bundle_in_tx(&mut tx, &request, Some(actor.user_id))
        .await
        .map_err(internal(CREATE_FAILED))?;
    if !members.is_empty() {
        write_exact_membership_tx(&mut tx, bundle_id, version_id, &members)
            .await
            .map_err(internal(CREATE_FAILED))?;
        refresh_draft_digests_tx(&mut tx, version_id)
            .await
            .map_err(internal(CREATE_FAILED))?;
    }
    record_agent_attribution(&mut tx, version_id, actor)
        .await
        .map_err(internal(CREATE_FAILED))?;
    let (version, _, revision) = draft_summary(&mut tx, version_id)
        .await
        .map_err(internal(CREATE_FAILED))?;
    insert_policy_audit_event(
        &mut tx,
        actor,
        "bundle_draft_created",
        &version_id.to_string(),
        &json!({
            "operation": "new_lineage",
            "bundle_id": bundle_id,
            "version_id": version_id,
            "version": version,
            "draft_revision": revision,
            "member_count": members.len(),
        }),
    )
    .await
    .map_err(internal(CREATE_FAILED))?;
    tx.commit().await.map_err(internal(CREATE_FAILED))?;
    Ok(BundleDraftOutcome {
        bundle_id,
        version_id,
        version,
        draft_revision: revision,
        derived_from_version_id: None,
        changed: true,
        changed_fields: Vec::new(),
        warnings,
    })
}

/// Derives a new mutable draft from the bundle lineage's current published
/// version for an actor.
///
/// The MCP tool `create_bundle_draft` (`mode: derive`) uses this function.
/// In one transaction it reauthorizes a token actor with `bundles:draft`,
/// locks the lineage row `FOR UPDATE`, derives the draft through
/// [`ensure_bundle_draft`] with `created_by = actor.user_id` (the draft keeps
/// the published version's exact membership and requirement membership),
/// records token attribution and writes the `bundle_draft_created` audit
/// event. The draft is untrusted.
///
/// # Errors
///
/// - [`BundleAuthoringError::Forbidden`] when a token actor is no longer
///   authorized.
/// - [`BundleAuthoringError::NotFound`] when the lineage does not exist.
/// - [`BundleAuthoringError::NoPublishedSource`] and
///   [`BundleAuthoringError::MutableDraftExists`] from the derivation rules.
/// - [`BundleAuthoringError::Internal`] on a database failure.
pub async fn derive_bundle_draft(
    pool: &PgPool,
    actor: &AuthoringActor,
    bundle_id: Uuid,
) -> Result<BundleDraftOutcome, BundleAuthoringError> {
    let mut tx = pool.begin().await.map_err(internal(CREATE_FAILED))?;
    authorize(&mut tx, actor).await?;
    // CONCURRENCY: `ensure_bundle_draft` locks the same row again.
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM compliance_bundles WHERE id = $1 FOR UPDATE")
        .bind(bundle_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal(CREATE_FAILED))?
        .ok_or(BundleAuthoringError::NotFound)?;
    let version_id = ensure_bundle_draft(
        &mut tx,
        bundle_id,
        Some(actor.user_id),
        None,
        BundleDraftIntent::CreateExplicit,
    )
    .await
    .map_err(|error| {
        let derivation = error
            .downcast_ref::<BundleDraftDerivationError>()
            .map(|derivation| match derivation {
                BundleDraftDerivationError::NoPublishedSource => None,
                BundleDraftDerivationError::MutableDraftExists(id) => Some(*id),
            });
        match derivation {
            Some(None) => BundleAuthoringError::NoPublishedSource,
            Some(Some(id)) => BundleAuthoringError::MutableDraftExists(id),
            None => BundleAuthoringError::Internal {
                context: CREATE_FAILED,
                error,
            },
        }
    })?;
    record_agent_attribution(&mut tx, version_id, actor)
        .await
        .map_err(internal(CREATE_FAILED))?;
    let (version, derived_from, revision) = draft_summary(&mut tx, version_id)
        .await
        .map_err(internal(CREATE_FAILED))?;
    insert_policy_audit_event(
        &mut tx,
        actor,
        "bundle_draft_created",
        &version_id.to_string(),
        &json!({
            "operation": "derive",
            "bundle_id": bundle_id,
            "version_id": version_id,
            "version": version,
            "derived_from_version_id": derived_from,
            "draft_revision": revision,
        }),
    )
    .await
    .map_err(internal(CREATE_FAILED))?;
    tx.commit().await.map_err(internal(CREATE_FAILED))?;
    Ok(BundleDraftOutcome {
        bundle_id,
        version_id,
        version,
        draft_revision: revision,
        derived_from_version_id: derived_from,
        changed: true,
        changed_fields: Vec::new(),
        warnings: Vec::new(),
    })
}

/// Holds replacement values for a bundle draft update. `None` keeps the
/// stored value.
#[derive(Debug, Clone, Default)]
pub struct BundleDraftChanges {
    /// New description; a blank text clears it.
    pub description: Option<String>,
    /// New framework label; must not be blank.
    pub framework: Option<String>,
    /// New framework version label; a blank text clears it.
    pub framework_version: Option<String>,
    /// New complete ordered membership of exact policy version IDs.
    pub policy_version_ids: Option<Vec<Uuid>>,
}

#[derive(sqlx::FromRow)]
struct BundleDraftRow {
    publication_state: String,
    created_by: Option<Uuid>,
    draft_revision: i64,
    version: String,
    derived_from_version_id: Option<Uuid>,
    framework: String,
    framework_version: Option<String>,
    description: Option<String>,
}

fn trimmed(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Replaces metadata and, optionally, the exact ordered membership of a
/// mutable bundle draft at an expected revision.
///
/// The MCP tool `update_bundle_draft` uses this function. In one transaction
/// it reauthorizes a token actor with `bundles:draft`, locks the lineage row
/// and the version row `FOR UPDATE`, and then requires, in this order: a
/// mutable version, `created_by = actor.user_id`, the version being the
/// lineage's current draft, and `draft_revision = expected_revision`. It
/// validates and locks the requested members. When every value equals the
/// stored value, it writes nothing (no row, no audit event) and returns
/// `changed: false`. Otherwise it writes metadata through the lineage row
/// (the browser editor's path; the sync trigger copies it to the draft),
/// writes the exact membership, recomputes the digests and writes the
/// `bundle_draft_updated` audit event. The revision triggers of migration
/// 0345 increment `draft_revision`.
///
/// # Errors
///
/// - [`BundleAuthoringError::Forbidden`] when a token actor is no longer
///   authorized, or with [`BUNDLE_DRAFT_OWNED_BY_OTHER_USER`] for another
///   user's draft.
/// - [`BundleAuthoringError::NotFound`] when the version does not exist.
/// - [`BundleAuthoringError::Invalid`] for an immutable version
///   (`immutable_version`), a version that is not the current draft
///   (`not_current_draft`), a blank framework (`invalid_framework`) or
///   invalid members (see [`classify_members`]).
/// - [`BundleAuthoringError::StaleRevision`] when the revision changed.
/// - [`BundleAuthoringError::Internal`] on a database failure.
pub async fn update_bundle_draft(
    pool: &PgPool,
    actor: &AuthoringActor,
    version_id: Uuid,
    expected_revision: i64,
    changes: &BundleDraftChanges,
) -> Result<BundleDraftOutcome, BundleAuthoringError> {
    let mut tx = pool.begin().await.map_err(internal(UPDATE_FAILED))?;
    authorize(&mut tx, actor).await?;
    let bundle_id: Uuid =
        sqlx::query_scalar("SELECT bundle_id FROM compliance_bundle_versions WHERE id = $1")
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal(UPDATE_FAILED))?
            .ok_or(BundleAuthoringError::NotFound)?;
    // CONCURRENCY: Lineage row, then version row, both `FOR UPDATE` (see the
    // module documentation). A concurrent update of this draft waits here
    // and then fails the revision check below.
    let current_draft = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1 FOR UPDATE",
    )
    .bind(bundle_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal(UPDATE_FAILED))?;
    let row = sqlx::query_as::<_, BundleDraftRow>(
        r#"SELECT publication_state, created_by, draft_revision, version,
                  derived_from_version_id, framework, framework_version, description
           FROM compliance_bundle_versions WHERE id = $1 FOR UPDATE"#,
    )
    .bind(version_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal(UPDATE_FAILED))?;
    if !is_mutable_state(&row.publication_state) {
        return Err(invalid(
            "immutable_version",
            format!(
                "bundle version {version_id} is {} and immutable; derive a new draft with \
                 create_bundle_draft",
                row.publication_state
            ),
        ));
    }
    if row.created_by != Some(actor.user_id) {
        return Err(BundleAuthoringError::Forbidden(
            BUNDLE_DRAFT_OWNED_BY_OTHER_USER.to_owned(),
        ));
    }
    if current_draft != Some(version_id) {
        return Err(invalid(
            "not_current_draft",
            format!("bundle version {version_id} is not the current draft of its lineage"),
        ));
    }
    if row.draft_revision != expected_revision {
        return Err(BundleAuthoringError::StaleRevision {
            current: row.draft_revision,
        });
    }

    let description = match &changes.description {
        Some(text) => trimmed(text),
        None => row.description.clone(),
    };
    let framework = match &changes.framework {
        Some(text) => trimmed(text).ok_or_else(|| {
            invalid(
                "invalid_framework",
                "framework must not be blank".to_owned(),
            )
        })?,
        None => row.framework.clone(),
    };
    let framework_version = match &changes.framework_version {
        Some(text) => trimmed(text),
        None => row.framework_version.clone(),
    };
    let (members, warnings) = match &changes.policy_version_ids {
        Some(requested) => {
            let (members, warnings) =
                validate_members_tx(&mut tx, requested, UPDATE_FAILED).await?;
            (Some(members), warnings)
        }
        None => (None, Vec::new()),
    };
    let stored_members: Vec<(Uuid, bool)> = sqlx::query_as(
        r#"SELECT policy_version_id, selected FROM compliance_bundle_version_policies
           WHERE bundle_version_id = $1 ORDER BY policy_order"#,
    )
    .bind(version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal(UPDATE_FAILED))?;

    // INVARIANT: No-op detection compares every field before any write, so a
    // no-op writes no row and keeps the revision.
    let mut changed_fields = Vec::new();
    if description != row.description {
        changed_fields.push("description");
    }
    if framework != row.framework {
        changed_fields.push("framework");
    }
    if framework_version != row.framework_version {
        changed_fields.push("framework_version");
    }
    let membership_changed = members.as_ref().is_some_and(|members| {
        members
            .iter()
            .map(|(id, _)| (*id, true))
            .ne(stored_members.iter().copied())
    });
    if membership_changed {
        changed_fields.push("policy_version_ids");
    }
    if changed_fields.is_empty() {
        return Ok(BundleDraftOutcome {
            bundle_id,
            version_id,
            version: row.version,
            draft_revision: row.draft_revision,
            derived_from_version_id: row.derived_from_version_id,
            changed: false,
            changed_fields,
            warnings,
        });
    }

    if changed_fields
        .iter()
        .any(|field| *field != "policy_version_ids")
    {
        // COMPATIBILITY: The browser editor reads and writes the lineage row;
        // `sync_bundle_draft_version` copies it to the current draft (this
        // version). The lineage stores a missing framework version as ''.
        sqlx::query(
            "UPDATE compliance_bundles SET framework = $2, version = $3, description = $4 \
             WHERE id = $1",
        )
        .bind(bundle_id)
        .bind(&framework)
        .bind(framework_version.as_deref().unwrap_or(""))
        .bind(&description)
        .execute(&mut *tx)
        .await
        .map_err(internal(UPDATE_FAILED))?;
    }
    if let (true, Some(members)) = (membership_changed, members.as_ref()) {
        write_exact_membership_tx(&mut tx, bundle_id, version_id, members)
            .await
            .map_err(internal(UPDATE_FAILED))?;
    }
    refresh_draft_digests_tx(&mut tx, version_id)
        .await
        .map_err(internal(UPDATE_FAILED))?;

    let (version, derived_from, revision) = draft_summary(&mut tx, version_id)
        .await
        .map_err(internal(UPDATE_FAILED))?;
    insert_policy_audit_event(
        &mut tx,
        actor,
        "bundle_draft_updated",
        &version_id.to_string(),
        &json!({
            "operation": "update",
            "bundle_id": bundle_id,
            "version_id": version_id,
            "previous_draft_revision": row.draft_revision,
            "draft_revision": revision,
            "changed_fields": changed_fields,
            "member_count": members.as_ref().map(Vec::len),
        }),
    )
    .await
    .map_err(internal(UPDATE_FAILED))?;
    tx.commit().await.map_err(internal(UPDATE_FAILED))?;
    Ok(BundleDraftOutcome {
        bundle_id,
        version_id,
        version,
        draft_revision: revision,
        derived_from_version_id: derived_from,
        changed: true,
        changed_fields,
        warnings,
    })
}

// ── Candidate impact preview ────────────────────────────────────────────────

/// Selects the scope of an impact preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTarget {
    /// One system; its environment must be effective for the caller.
    System(Uuid),
    /// Every system of one effective environment, at most
    /// [`MAX_PREVIEW_SYSTEMS`].
    Environment(Uuid),
}

/// One effective policy, reduced to the facts that a preview compares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectivePolicyFact {
    /// Policy lineage ID.
    pub policy_lineage_id: Uuid,
    /// Exact effective policy version ID.
    pub policy_version_id: Uuid,
    /// Policy type.
    pub policy_type: String,
    /// `enforce` or `report_only`.
    pub enforcement_mode: &'static str,
    /// How the policy entered the set.
    pub source: EffectivePolicySource,
}

/// One resolver result in a preview.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResolvedPolicies {
    /// The resolver produced a complete set.
    Resolved {
        /// Effective policies in resolver order.
        policies: Vec<EffectivePolicyFact>,
        /// Resolver warnings.
        warnings: Vec<String>,
    },
    /// The resolver rejected the inputs; no set exists.
    Conflict {
        /// Resolver conflicts.
        conflicts: Vec<ResolutionConflict>,
    },
}

impl ResolvedPolicies {
    fn from_outcome(outcome: Option<ResolutionOutcome>) -> Self {
        match outcome {
            Some(ResolutionOutcome::Resolved(set)) => Self::Resolved {
                policies: set
                    .policies
                    .into_iter()
                    .map(|policy| EffectivePolicyFact {
                        policy_lineage_id: policy.policy_lineage_id,
                        policy_version_id: policy.policy_version_id,
                        policy_type: policy.policy_type,
                        enforcement_mode: policy.effective_mode.as_str(),
                        source: policy.source,
                    })
                    .collect(),
                warnings: set.warnings,
            },
            Some(ResolutionOutcome::Conflict(conflicts)) => Self::Conflict { conflicts },
            // The batch resolver omits a system that disappeared during the
            // preview; it has no assignments and no effective policy.
            None => Self::Resolved {
                policies: Vec::new(),
                warnings: Vec::new(),
            },
        }
    }

    fn policies(&self) -> Option<&[EffectivePolicyFact]> {
        match self {
            Self::Resolved { policies, .. } => Some(policies.as_slice()),
            Self::Conflict { .. } => None,
        }
    }
}

/// A policy lineage whose effective version or enforcement mode differs
/// between the current and the proposed set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyChange {
    /// Policy lineage ID.
    pub policy_lineage_id: Uuid,
    /// Effective version now.
    pub current_version_id: Uuid,
    /// Effective version with the candidate.
    pub proposed_version_id: Uuid,
    /// Enforcement mode now.
    pub current_enforcement_mode: &'static str,
    /// Enforcement mode with the candidate.
    pub proposed_enforcement_mode: &'static str,
}

/// The preview result of one system.
#[derive(Debug, Clone, Serialize)]
pub struct SystemImpact {
    /// System ID.
    pub system_id: Uuid,
    /// System hostname.
    pub hostname: String,
    /// The effective policies now (the real resolver).
    pub current: ResolvedPolicies,
    /// The effective policies with the candidate assigned.
    pub proposed: ResolvedPolicies,
    /// Lineages effective only with the candidate. Empty when either side
    /// has a conflict.
    pub added: Vec<EffectivePolicyFact>,
    /// Lineages effective only now. Empty when either side has a conflict.
    pub removed: Vec<EffectivePolicyFact>,
    /// Lineages whose version or mode changes. Empty when either side has a
    /// conflict.
    pub changed: Vec<PolicyChange>,
}

/// Compares two effective sets by policy lineage.
///
/// `added` and `changed` follow the proposed order, `removed` the current
/// order. A lineage is changed when its exact version or its enforcement
/// mode differs.
fn diff_policies(
    current: &[EffectivePolicyFact],
    proposed: &[EffectivePolicyFact],
) -> (
    Vec<EffectivePolicyFact>,
    Vec<EffectivePolicyFact>,
    Vec<PolicyChange>,
) {
    let now: HashMap<Uuid, &EffectivePolicyFact> = current
        .iter()
        .map(|policy| (policy.policy_lineage_id, policy))
        .collect();
    let next: HashMap<Uuid, &EffectivePolicyFact> = proposed
        .iter()
        .map(|policy| (policy.policy_lineage_id, policy))
        .collect();
    let added = proposed
        .iter()
        .filter(|policy| !now.contains_key(&policy.policy_lineage_id))
        .cloned()
        .collect();
    let removed = current
        .iter()
        .filter(|policy| !next.contains_key(&policy.policy_lineage_id))
        .cloned()
        .collect();
    let changed = proposed
        .iter()
        .filter_map(|policy| {
            let before = now.get(&policy.policy_lineage_id)?;
            (before.policy_version_id != policy.policy_version_id
                || before.enforcement_mode != policy.enforcement_mode)
                .then(|| PolicyChange {
                    policy_lineage_id: policy.policy_lineage_id,
                    current_version_id: before.policy_version_id,
                    proposed_version_id: policy.policy_version_id,
                    current_enforcement_mode: before.enforcement_mode,
                    proposed_enforcement_mode: policy.enforcement_mode,
                })
        })
        .collect();
    (added, removed, changed)
}

/// Identifies the candidate bundle version of a preview.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CandidateVersion {
    /// Bundle lineage ID.
    pub bundle_id: Uuid,
    /// Exact candidate version ID.
    pub bundle_version_id: Uuid,
    /// Version label.
    pub version: String,
    /// Version name.
    pub name: String,
    /// Publication state.
    pub publication_state: String,
    /// Trust state.
    pub trust_state: String,
    /// Semantic digest, or `pending`.
    pub semantic_digest: String,
}

/// One member of the candidate version with its review state.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CandidateMember {
    /// Position in the bundle.
    pub policy_order: i32,
    /// Exact policy version ID.
    pub policy_version_id: Uuid,
    /// Policy lineage ID.
    pub policy_id: Uuid,
    /// Version name.
    pub name: String,
    /// Selected members are part of the baseline.
    pub selected: bool,
    /// Publication state.
    pub publication_state: String,
    /// Trust state.
    pub trust_state: String,
    /// Implementation state.
    pub implementation_state: String,
}

impl CandidateMember {
    /// Reports a member that publication does not accept yet.
    fn is_draft(&self) -> bool {
        self.publication_state != "accepted" && self.publication_state != "deprecated"
    }

    /// Reports an executable member that is not trusted.
    fn is_untrusted(&self) -> bool {
        is_executable(&self.implementation_state) && self.trust_state != "trusted"
    }
}

/// One requirement that the candidate does not cover fully.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageGap {
    /// Requirement version ID.
    pub requirement_version_id: Uuid,
    /// External identifier, for example a STIG rule ID.
    pub external_id: String,
    /// `partial`, `unmapped` or `recovery_required`.
    pub coverage: RequirementCoverage,
}

/// Requirement coverage of the candidate from
/// [`compute_bundle_requirement_coverage`].
#[derive(Debug, Clone, Serialize)]
pub struct CoverageSummary {
    /// Selected requirements of the candidate.
    pub total_requirements: i64,
    /// Requirements with full coverage.
    pub full: i64,
    /// Requirements with partial coverage.
    pub partial: i64,
    /// Requirements with no trusted mapping.
    pub unmapped: i64,
    /// Requirements whose framework release is pending recovery.
    pub recovery_required: i64,
    /// First [`MAX_COVERAGE_GAPS`] requirements without full coverage.
    pub gaps: Vec<CoverageGap>,
    /// Reports that more gaps exist than `gaps` lists.
    pub gaps_truncated: bool,
}

/// Counts derived deterministically from the resolver results.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PreviewFacts {
    /// Systems resolved in this preview.
    pub systems_previewed: usize,
    /// Systems whose effective set changes.
    pub systems_with_changes: usize,
    /// Systems whose current resolution has a conflict.
    pub systems_with_current_conflicts: usize,
    /// Systems whose proposed resolution has a conflict.
    pub systems_with_proposed_conflicts: usize,
    /// Added lineages over all systems.
    pub added_policies: usize,
    /// Removed lineages over all systems.
    pub removed_policies: usize,
    /// Changed lineages over all systems.
    pub changed_policies: usize,
    /// Candidate members that are not published yet.
    pub draft_members: usize,
    /// Executable candidate members that are not trusted.
    pub untrusted_members: usize,
}

/// Outcomes that this preview cannot determine. They need evaluation of the
/// systems, which only a real assignment followed by evaluation produces.
pub const REQUIRES_EVALUATION: [&str; 3] = [
    "Whether each system passes or fails the added or changed policies.",
    "Whether an enforce-mode assignment would block a deployment of a system.",
    "Whether evidence satisfies the requirements that the coverage counts as mapped.",
];

/// The result of [`preview_bundle_impact`].
#[derive(Debug, Clone, Serialize)]
pub struct BundleImpactPreview {
    /// The candidate bundle version.
    pub candidate: CandidateVersion,
    /// Reports whether the candidate can be assigned now (accepted, trusted
    /// and with a computed digest). A preview never assigns.
    pub assignable_now: bool,
    /// `system` or `environment`.
    pub scope_type: &'static str,
    /// System or environment ID.
    pub scope_id: Uuid,
    /// Hypothetical enforcement mode.
    pub enforcement_mode: &'static str,
    /// Per-system results, ordered by hostname.
    pub systems: Vec<SystemImpact>,
    /// Reports that the environment has more than [`MAX_PREVIEW_SYSTEMS`]
    /// systems and only the first ones were previewed.
    pub truncated: bool,
    /// Candidate members in bundle order.
    pub members: Vec<CandidateMember>,
    /// Requirement coverage of the candidate.
    pub coverage: CoverageSummary,
    /// Deterministic results.
    pub facts: PreviewFacts,
    /// Outcomes that need evaluation; the preview never claims them.
    pub requires_evaluation: Vec<&'static str>,
    /// Review findings about the candidate.
    pub warnings: Vec<String>,
}

/// Previews the effect of assigning a candidate bundle version at a target
/// in an enforcement mode, without writing.
///
/// For every system of the target (one system, or up to
/// [`MAX_PREVIEW_SYSTEMS`] systems of an environment ordered by hostname) the
/// function resolves the effective policies now with the real resolver
/// ([`resolve_system_effective_policies_in_tx`] for a system,
/// [`resolve_systems_effective_policies_in_tx`] for an environment) and with
/// the candidate through
/// [`resolve_system_effective_policies_with_candidate_in_tx`], and reports
/// the difference by policy lineage. The candidate may be a draft or
/// untrusted; draft and untrusted members are listed. Requirement coverage
/// comes from [`compute_bundle_requirement_coverage`].
///
/// SECURITY: The target must be in `effective` (the caller's effective
/// environments); otherwise the result is [`BundleAuthoringError::NotFound`],
/// the same as for an unknown ID. Only systems of that environment appear in
/// the result.
///
/// The resolution runs in one `REPEATABLE READ, READ ONLY` transaction that
/// is always rolled back: the preview writes nothing, assigns nothing and
/// schedules nothing. It never reports a pass or fail outcome; see
/// [`REQUIRES_EVALUATION`].
///
/// # Errors
///
/// - [`BundleAuthoringError::NotFound`] for an unknown bundle version or a
///   hidden or unknown target.
/// - [`BundleAuthoringError::Internal`] on a database failure.
pub async fn preview_bundle_impact(
    pool: &PgPool,
    effective: &[Uuid],
    bundle_version_id: Uuid,
    target: PreviewTarget,
    mode: AssignmentMode,
) -> Result<BundleImpactPreview, BundleAuthoringError> {
    let mut tx = pool.begin().await.map_err(internal(PREVIEW_FAILED))?;
    // SECURITY: READ ONLY makes PostgreSQL reject any write in this
    // transaction, and the transaction is rolled back below.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(internal(PREVIEW_FAILED))?;

    // SECURITY: The target check runs before any candidate or policy read.
    let (systems, truncated, assignment_target) = match target {
        PreviewTarget::System(system_id) => {
            let row = sqlx::query_as::<_, (String, Option<Uuid>)>(
                "SELECT hostname, environment_id FROM systems WHERE id = $1",
            )
            .bind(system_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal(PREVIEW_FAILED))?;
            match row {
                Some((hostname, Some(environment_id))) if effective.contains(&environment_id) => (
                    vec![(system_id, hostname)],
                    false,
                    AssignmentTarget::System { system_id },
                ),
                _ => return Err(BundleAuthoringError::NotFound),
            }
        }
        PreviewTarget::Environment(environment_id) => {
            if !effective.contains(&environment_id) {
                return Err(BundleAuthoringError::NotFound);
            }
            let mut systems = sqlx::query_as::<_, (Uuid, String)>(
                r#"SELECT id, hostname FROM systems
                   WHERE environment_id = $1
                   ORDER BY hostname, id
                   LIMIT $2"#,
            )
            .bind(environment_id)
            .bind(MAX_PREVIEW_SYSTEMS as i64 + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(internal(PREVIEW_FAILED))?;
            let truncated = systems.len() > MAX_PREVIEW_SYSTEMS;
            systems.truncate(MAX_PREVIEW_SYSTEMS);
            (
                systems,
                truncated,
                AssignmentTarget::Environment { environment_id },
            )
        }
    };

    let candidate = sqlx::query_as::<_, CandidateVersion>(
        r#"SELECT bundle_id, id AS bundle_version_id, version, name, publication_state,
                  trust_state, semantic_digest
           FROM compliance_bundle_versions WHERE id = $1"#,
    )
    .bind(bundle_version_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal(PREVIEW_FAILED))?
    .ok_or(BundleAuthoringError::NotFound)?;
    let members = sqlx::query_as::<_, CandidateMember>(
        r#"SELECT cbvp.policy_order, cbvp.policy_version_id, pv.policy_id, pv.name,
                  cbvp.selected, pv.publication_state, pv.trust_state, pv.implementation_state
           FROM compliance_bundle_version_policies cbvp
           JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
           WHERE cbvp.bundle_version_id = $1
           ORDER BY cbvp.policy_order"#,
    )
    .bind(bundle_version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal(PREVIEW_FAILED))?;

    let system_ids = systems.iter().map(|(id, _)| *id).collect::<Vec<_>>();
    let mut current_sets = match target {
        PreviewTarget::System(system_id) => {
            let outcome = resolve_system_effective_policies_in_tx(&mut tx, system_id)
                .await
                .map_err(internal(PREVIEW_FAILED))?;
            HashMap::from([(system_id, outcome)])
        }
        PreviewTarget::Environment(_) => {
            resolve_systems_effective_policies_in_tx(&mut tx, &system_ids)
                .await
                .map_err(internal(PREVIEW_FAILED))?
        }
    };
    let hypothetical = CandidateAssignment {
        bundle_id: candidate.bundle_id,
        bundle_version_id,
        target: assignment_target,
        assignment_mode: mode.clone(),
    };
    let mut facts = PreviewFacts {
        systems_previewed: systems.len(),
        ..PreviewFacts::default()
    };
    let mut impacts = Vec::with_capacity(systems.len());
    for (system_id, hostname) in systems {
        let proposed = resolve_system_effective_policies_with_candidate_in_tx(
            &mut tx,
            system_id,
            &hypothetical,
        )
        .await
        .map_err(internal(PREVIEW_FAILED))?;
        let current = ResolvedPolicies::from_outcome(current_sets.remove(&system_id));
        let proposed = ResolvedPolicies::from_outcome(Some(proposed));
        let (added, removed, changed) = match (current.policies(), proposed.policies()) {
            (Some(now), Some(next)) => diff_policies(now, next),
            _ => (Vec::new(), Vec::new(), Vec::new()),
        };
        facts.systems_with_current_conflicts += usize::from(current.policies().is_none());
        facts.systems_with_proposed_conflicts += usize::from(proposed.policies().is_none());
        facts.systems_with_changes +=
            usize::from(!added.is_empty() || !removed.is_empty() || !changed.is_empty());
        facts.added_policies += added.len();
        facts.removed_policies += removed.len();
        facts.changed_policies += changed.len();
        impacts.push(SystemImpact {
            system_id,
            hostname,
            current,
            proposed,
            added,
            removed,
            changed,
        });
    }
    // The transaction only read; the rollback releases its snapshot.
    tx.rollback().await.map_err(internal(PREVIEW_FAILED))?;

    let report = compute_bundle_requirement_coverage(pool, bundle_version_id)
        .await
        .map_err(internal(PREVIEW_FAILED))?;
    let mut gaps = report
        .rows
        .into_iter()
        .filter(|row| row.coverage != RequirementCoverage::Full)
        .map(|row| CoverageGap {
            requirement_version_id: row.requirement_version_id,
            external_id: row.external_id,
            coverage: row.coverage,
        })
        .collect::<Vec<_>>();
    let gaps_truncated = gaps.len() > MAX_COVERAGE_GAPS;
    gaps.truncate(MAX_COVERAGE_GAPS);
    let coverage = CoverageSummary {
        total_requirements: report.total_requirements,
        full: report.full,
        partial: report.partial,
        unmapped: report.unmapped,
        recovery_required: report.recovery_required,
        gaps,
        gaps_truncated,
    };

    facts.draft_members = members.iter().filter(|member| member.is_draft()).count();
    facts.untrusted_members = members
        .iter()
        .filter(|member| member.is_untrusted())
        .count();
    let assignable_now = candidate.publication_state == "accepted"
        && candidate.trust_state == "trusted"
        && candidate.semantic_digest != "pending";
    let warnings = preview_warnings(&candidate, &facts, truncated);
    Ok(BundleImpactPreview {
        assignable_now,
        scope_type: hypothetical.target.scope_type(),
        scope_id: hypothetical.target.scope_id(),
        enforcement_mode: mode.as_str(),
        candidate,
        systems: impacts,
        truncated,
        members,
        coverage,
        facts,
        requires_evaluation: REQUIRES_EVALUATION.to_vec(),
        warnings,
    })
}

fn preview_warnings(
    candidate: &CandidateVersion,
    facts: &PreviewFacts,
    truncated: bool,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if candidate.publication_state != "accepted" {
        warnings.push(format!(
            "The candidate is {} and cannot be assigned. An Admin must trust and publish it in \
             the browser first.",
            candidate.publication_state
        ));
    } else if candidate.trust_state != "trusted" {
        warnings.push(format!(
            "The candidate is {} and cannot be assigned until an Admin trusts it.",
            candidate.trust_state
        ));
    }
    if candidate.semantic_digest == "pending" {
        warnings.push("The candidate digest is pending.".to_owned());
    }
    if facts.draft_members > 0 {
        warnings.push(format!(
            "{} member(s) are not published. Bundle publication requires published members.",
            facts.draft_members
        ));
    }
    if facts.untrusted_members > 0 {
        warnings.push(format!(
            "{} executable member(s) are not trusted. Bundle publication requires trusted \
             members.",
            facts.untrusted_members
        ));
    }
    if truncated {
        warnings.push(format!(
            "The environment has more than {MAX_PREVIEW_SYSTEMS} systems; only the first \
             {MAX_PREVIEW_SYSTEMS} by hostname were previewed."
        ));
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(policy_id: Uuid, state: &str, trust: &str) -> MemberVersionRow {
        MemberVersionRow {
            id: Uuid::new_v4(),
            policy_id,
            publication_state: state.to_owned(),
            trust_state: trust.to_owned(),
            implementation_state: "native".to_owned(),
        }
    }

    fn code(
        result: Result<(Vec<(Uuid, Uuid)>, Vec<MembershipWarning>), BundleAuthoringError>,
    ) -> &'static str {
        match result {
            Err(BundleAuthoringError::Invalid { code, .. }) => code,
            other => panic!("expected an invalid membership, got {other:?}"),
        }
    }

    #[test]
    fn members_keep_request_order_and_report_review_warnings() {
        let published = member(Uuid::new_v4(), "accepted", "trusted");
        let draft = member(Uuid::new_v4(), "draft", "untrusted");
        let rows = vec![published.clone(), draft.clone()];
        let (members, warnings) =
            classify_members(&[draft.id, published.id], &rows).expect("valid members");
        assert_eq!(
            members,
            vec![
                (draft.id, draft.policy_id),
                (published.id, published.policy_id)
            ]
        );
        let codes = warnings.iter().map(|w| w.code).collect::<Vec<_>>();
        assert_eq!(codes, ["draft_member", "untrusted_member"]);
        assert!(warnings.iter().all(|w| w.policy_version_id == draft.id));
    }

    #[test]
    fn invalid_members_are_rejected() {
        let lineage = Uuid::new_v4();
        let first = member(lineage, "accepted", "trusted");
        let second = member(lineage, "draft", "untrusted");
        let rows = vec![first.clone(), second.clone()];
        assert_eq!(
            code(classify_members(&[first.id, first.id], &rows)),
            "duplicate_policy_version"
        );
        assert_eq!(
            code(classify_members(&[first.id, Uuid::new_v4()], &rows)),
            "unknown_policy_version"
        );
        assert_eq!(
            code(classify_members(&[first.id, second.id], &rows)),
            "duplicate_policy_lineage"
        );
        let deprecated = member(Uuid::new_v4(), "deprecated", "trusted");
        let rejected = member(Uuid::new_v4(), "accepted", "rejected");
        let rows = vec![deprecated.clone(), rejected.clone()];
        for id in [deprecated.id, rejected.id] {
            assert_eq!(
                code(classify_members(&[id], &rows)),
                "member_never_publishable"
            );
        }
    }

    fn fact(lineage: Uuid, version: Uuid, mode: &'static str) -> EffectivePolicyFact {
        EffectivePolicyFact {
            policy_lineage_id: lineage,
            policy_version_id: version,
            policy_type: "custom_check".to_owned(),
            enforcement_mode: mode,
            source: EffectivePolicySource::Baseline,
        }
    }

    #[test]
    fn diff_reports_added_removed_and_changed_lineages() {
        let (kept, bumped, moded, gone, new) = (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let (old_version, new_version, same) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let current = vec![
            fact(kept, same, "enforce"),
            fact(bumped, old_version, "enforce"),
            fact(moded, same, "enforce"),
            fact(gone, same, "enforce"),
        ];
        let proposed = vec![
            fact(kept, same, "enforce"),
            fact(bumped, new_version, "enforce"),
            fact(moded, same, "report_only"),
            fact(new, same, "enforce"),
        ];
        let (added, removed, changed) = diff_policies(&current, &proposed);
        assert_eq!(added, vec![fact(new, same, "enforce")]);
        assert_eq!(removed, vec![fact(gone, same, "enforce")]);
        assert_eq!(
            changed
                .iter()
                .map(|c| c.policy_lineage_id)
                .collect::<Vec<_>>(),
            vec![bumped, moded]
        );
        assert_eq!(changed[0].proposed_version_id, new_version);
        assert_eq!(changed[1].proposed_enforcement_mode, "report_only");
        let (added, removed, changed) = diff_policies(&current, &current);
        assert!(added.is_empty() && removed.is_empty() && changed.is_empty());
    }
}
