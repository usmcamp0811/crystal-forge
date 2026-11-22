//! Defines the MCP bundle tools (TASK-534.8): `create_bundle_draft` and
//! `update_bundle_draft` (scope `bundles:draft`, Admin-only) and
//! `preview_bundle_impact` (scope `compliance:preview`).
//!
//! The functions here parse the tool arguments and map service errors to
//! [`ToolError`]. The domain rules live in
//! [`crate::services::bundle_authoring`]. The dispatcher in [`super::tools`]
//! checks the scope, and for `bundles:draft` the owner's Admin role at
//! admission, before it calls a function here. Every write then
//! reauthorizes the token row, the scope and the owner's Admin role inside
//! its own transaction.
//!
//! The write set is closed. No tool trusts, publishes, assigns, changes an
//! enforcement mode, deletes or schedules work. Created versions are
//! untrusted drafts. An agent edits only a mutable bundle draft whose
//! `created_by` is the token owner, and sets its membership only from exact
//! policy version IDs.
//!
//! SECURITY: `preview_bundle_impact` reads only a system or environment in
//! the call's effective environments; any other target returns not found,
//! the same result as an unknown ID. It writes nothing and never reports a
//! pass or fail outcome.

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::tools::{ToolCaller, ToolError, parse_arguments};
use crate::compliance::resolver::AssignmentMode;
use crate::services::bundle_authoring::{
    self as authoring, BundleAuthoringError, BundleDraftChanges, BundleDraftOutcome,
    NewBundleLineage, PreviewTarget,
};
use crate::services::policy_authoring::AuthoringActor;

/// Tool name of bundle draft creation (new lineage or derived draft).
pub(super) const CREATE_BUNDLE_DRAFT: &str = "create_bundle_draft";
/// Tool name of the bundle draft update at an expected revision.
pub(super) const UPDATE_BUNDLE_DRAFT: &str = "update_bundle_draft";
/// Tool name of the read-only candidate impact preview.
pub(super) const PREVIEW_BUNDLE_IMPACT: &str = "preview_bundle_impact";

const DRAFT_NOTE: &str = "The bundle draft is untrusted and unpublished. Only an Admin in \
the browser (Compliance view) can trust, publish or assign it. Send draft_revision with every \
update_bundle_draft call. Warnings list members that publication does not accept yet.";

const PREVIEW_NOTE: &str = "Preview only: nothing was written, assigned or scheduled. facts \
are deterministic resolver results; requires_evaluation lists outcomes that only an \
evaluation after a real assignment can show. This preview never reports pass or fail.";

// ── Schemas ──────────────────────────────────────────────────────────────────

fn uuid_schema(description: &str) -> Value {
    json!({"type": "string", "format": "uuid", "description": description})
}

fn members_schema() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string", "format": "uuid"},
        "description": "Complete ordered membership as exact policy version IDs (from \
    get_compliance_policy or the policy draft tools). At most one version per policy lineage; \
    no duplicates; never a lineage ID. Draft and untrusted members are accepted with warnings."
    })
}

/// Returns the JSON Schema (draft 2020-12) of `create_bundle_draft`.
///
/// `mode` selects `new_lineage` (requires `name` and `framework`) or
/// `derive` (requires `bundle_id`). The server rejects properties of the
/// other mode.
pub fn create_bundle_draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "mode": {"enum": ["new_lineage", "derive"]},
            "name": {"type": "string", "minLength": 1, "maxLength": 255,
                "description": "new_lineage: unique bundle name."},
            "description": {"type": "string", "description": "new_lineage: description."},
            "framework": {"type": "string", "minLength": 1,
                "description": "new_lineage: framework label, for example NIST 800-53."},
            "framework_version": {"type": "string",
                "description": "new_lineage: framework version label, for example rev5."},
            "policy_version_ids": members_schema(),
            "bundle_id": uuid_schema("derive: bundle lineage ID with a published version and no \
    open draft. The draft keeps the published version's exact membership.")
        },
        "required": ["mode"],
        "oneOf": [
            {"properties": {"mode": {"const": "new_lineage"}},
             "required": ["name", "framework"]},
            {"properties": {"mode": {"const": "derive"}}, "required": ["bundle_id"]}
        ],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `update_bundle_draft`.
pub fn update_bundle_draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "version_id": uuid_schema("Bundle draft version ID created by this token's owner."),
            "draft_revision": {"type": "integer", "description": "Current draft_revision from \
    get_compliance_bundle or the last bundle draft tool result. A stale revision is rejected \
    with stale_revision and changes nothing."},
            "description": {"type": "string", "description": "New description; an empty \
    string clears it."},
            "framework": {"type": "string", "minLength": 1, "description": "New framework label."},
            "framework_version": {"type": "string", "description": "New framework version \
    label; an empty string clears it."},
            "policy_version_ids": members_schema()
        },
        "required": ["version_id", "draft_revision"],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `preview_bundle_impact`.
/// Exactly one of `system_id` and `environment_id` is required.
pub fn preview_bundle_impact_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "bundle_version_id": uuid_schema("Candidate bundle version; it may be a draft."),
            "system_id": uuid_schema("Target system in this token's environments. Send \
    exactly one of system_id and environment_id."),
            "environment_id": uuid_schema("Target environment of this token. At most 100 \
    systems are previewed."),
            "enforcement_mode": {"enum": ["enforce", "report_only"],
                "description": "Hypothetical enforcement mode of the assignment."}
        },
        "required": ["bundle_version_id", "enforcement_mode"],
        "additionalProperties": false
    })
}

// ── Arguments ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum CreateArgs {
    NewLineage {
        name: String,
        description: Option<String>,
        framework: String,
        framework_version: Option<String>,
        #[serde(default)]
        policy_version_ids: Vec<Uuid>,
    },
    Derive {
        bundle_id: Uuid,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    version_id: Uuid,
    draft_revision: i64,
    description: Option<String>,
    framework: Option<String>,
    framework_version: Option<String>,
    policy_version_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ModeArg {
    Enforce,
    ReportOnly,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewArgs {
    bundle_version_id: Uuid,
    system_id: Option<Uuid>,
    environment_id: Option<Uuid>,
    enforcement_mode: ModeArg,
}

fn actor(caller: &ToolCaller) -> AuthoringActor {
    AuthoringActor::token(caller.owner_user_id, caller.token_id)
}

/// Maps a service error to the tool error that the agent sees.
fn tool_error(error: BundleAuthoringError) -> ToolError {
    match error {
        BundleAuthoringError::Forbidden(message) => ToolError::Forbidden(message),
        BundleAuthoringError::NotFound => ToolError::NotFound,
        BundleAuthoringError::Invalid { code, message } => {
            ToolError::InvalidArguments(format!("{code}: {message}"))
        }
        BundleAuthoringError::StaleRevision { current } => ToolError::InvalidArguments(format!(
            "stale_revision: the draft revision is now {current}. Read the version again with \
             get_compliance_bundle and retry with its current draft_revision."
        )),
        BundleAuthoringError::NoPublishedSource => ToolError::InvalidArguments(
            "no_published_version: the bundle has no published version to derive a draft from"
                .into(),
        ),
        BundleAuthoringError::MutableDraftExists(version_id) => ToolError::InvalidArguments(
            format!("draft_exists: the bundle already has the open draft version {version_id}"),
        ),
        BundleAuthoringError::Internal { context, error } => {
            ToolError::Internal(error.context(context))
        }
    }
}

fn draft_result(mode: &str, outcome: BundleDraftOutcome) -> Result<Value, ToolError> {
    let mut result =
        serde_json::to_value(outcome).map_err(|error| ToolError::Internal(error.into()))?;
    result["mode"] = json!(mode);
    result["note"] = json!(DRAFT_NOTE);
    Ok(result)
}

// ── Tools ────────────────────────────────────────────────────────────────────

/// Runs `create_bundle_draft`: creates a new bundle lineage with an initial
/// draft (`mode: new_lineage`) or derives a draft from the published version
/// of a bundle (`mode: derive`).
pub(super) async fn create_bundle_draft(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let actor = actor(caller);
    match parse_arguments::<CreateArgs>(arguments)? {
        CreateArgs::NewLineage {
            name,
            description,
            framework,
            framework_version,
            policy_version_ids,
        } => {
            let input = NewBundleLineage {
                name,
                description,
                framework,
                framework_version,
                policy_version_ids,
            };
            let outcome = authoring::create_bundle_lineage_draft(pool, &actor, &input)
                .await
                .map_err(tool_error)?;
            draft_result("new_lineage", outcome)
        }
        CreateArgs::Derive { bundle_id } => {
            let outcome = authoring::derive_bundle_draft(pool, &actor, bundle_id)
                .await
                .map_err(tool_error)?;
            draft_result("derive", outcome)
        }
    }
}

/// Runs `update_bundle_draft`: replaces metadata and, optionally, the exact
/// ordered membership of the owner's bundle draft at the current
/// `draft_revision`. A no-op returns `changed: false` and writes nothing.
pub(super) async fn update_bundle_draft(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: UpdateArgs = parse_arguments(arguments)?;
    let changes = BundleDraftChanges {
        description: args.description,
        framework: args.framework,
        framework_version: args.framework_version,
        policy_version_ids: args.policy_version_ids,
    };
    let outcome = authoring::update_bundle_draft(
        pool,
        &actor(caller),
        args.version_id,
        args.draft_revision,
        &changes,
    )
    .await
    .map_err(tool_error)?;
    draft_result("update", outcome)
}

/// Runs `preview_bundle_impact` for a target in `effective`.
pub(super) async fn preview_bundle_impact(
    pool: &PgPool,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: PreviewArgs = parse_arguments(arguments)?;
    let target = match (args.system_id, args.environment_id) {
        (Some(system_id), None) => PreviewTarget::System(system_id),
        (None, Some(environment_id)) => PreviewTarget::Environment(environment_id),
        _ => {
            return Err(ToolError::InvalidArguments(
                "send exactly one of system_id and environment_id".into(),
            ));
        }
    };
    let mode = match args.enforcement_mode {
        ModeArg::Enforce => AssignmentMode::Enforce,
        ModeArg::ReportOnly => AssignmentMode::ReportOnly,
    };
    let preview =
        authoring::preview_bundle_impact(pool, effective, args.bundle_version_id, target, mode)
            .await
            .map_err(tool_error)?;
    let mut result =
        serde_json::to_value(preview).map_err(|error| ToolError::Internal(error.into()))?;
    result["note"] = json!(PREVIEW_NOTE);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_closed_objects() {
        for schema in [
            create_bundle_draft_schema(),
            update_bundle_draft_schema(),
            preview_bundle_impact_schema(),
        ] {
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
        }
    }

    #[test]
    fn create_arguments_are_tagged_by_mode_and_closed() {
        let derive: CreateArgs =
            parse_arguments(json!({"mode": "derive", "bundle_id": Uuid::nil()})).expect("derive");
        assert!(matches!(derive, CreateArgs::Derive { .. }));
        let new: CreateArgs = parse_arguments(json!({
            "mode": "new_lineage",
            "name": "b",
            "framework": "NIST",
            "policy_version_ids": [Uuid::nil()]
        }))
        .expect("new lineage");
        assert!(matches!(new, CreateArgs::NewLineage { .. }));
        for invalid in [
            json!({"mode": "publish", "bundle_id": Uuid::nil()}),
            json!({"mode": "derive", "bundle_id": Uuid::nil(), "name": "x"}),
            json!({"mode": "new_lineage", "name": "b", "framework": "f", "policy_ids": []}),
            json!({"mode": "derive", "bundle_id": Uuid::nil(), "trust": true}),
        ] {
            assert!(
                parse_arguments::<CreateArgs>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn preview_accepts_only_known_modes() {
        for mode in ["enforce", "report_only"] {
            let args = json!({"bundle_version_id": Uuid::nil(), "system_id": Uuid::nil(),
                "enforcement_mode": mode});
            assert!(parse_arguments::<PreviewArgs>(args).is_ok(), "{mode}");
        }
        for mode in ["audit", "Enforce", ""] {
            let args = json!({"bundle_version_id": Uuid::nil(), "system_id": Uuid::nil(),
                "enforcement_mode": mode});
            assert!(parse_arguments::<PreviewArgs>(args).is_err(), "{mode}");
        }
    }

    #[test]
    fn stale_revision_and_ownership_errors_are_stable() {
        assert!(matches!(
            tool_error(BundleAuthoringError::StaleRevision { current: 7 }),
            ToolError::InvalidArguments(message) if message.starts_with("stale_revision:")
        ));
        assert!(matches!(
            tool_error(BundleAuthoringError::Forbidden(
                authoring::BUNDLE_DRAFT_OWNED_BY_OTHER_USER.into()
            )),
            ToolError::Forbidden(message) if message == authoring::BUNDLE_DRAFT_OWNED_BY_OTHER_USER
        ));
        assert!(matches!(
            tool_error(BundleAuthoringError::NotFound),
            ToolError::NotFound
        ));
    }
}
