//! Defines the MCP tools for POA&M creation, progress, and verification.
//!
//! The functions here parse and bound the tool arguments and map service
//! errors to [`ToolError`]. They do not authorize: the POA&M service
//! reauthorizes the token row, the required write scope,
//! the owner role (Operator or Admin) and the effective environments inside
//! the mutation transaction, and requires every POA&M subject in scope. A
//! hidden POA&M returns [`ToolError::NotFound`], the same result as an absent
//! one.
//!
//! `create_cve_poam` uses runtime evidence when eligible or records a separate
//! HEAD planning subject. It hands off an existing planning POA&M in place
//! when eligible exact runtime evidence becomes available.
//! `update_poam_progress` cannot change risk, title,
//! plan, owner, assignee, target date, finding links or waivers, and cannot
//! close or reopen a POA&M. `verify_poam` records a verification attempt and
//! never closes. Mutations record the access token ID in the applicable
//! activity, audit, and verification rows; the owner stays the recorded actor.

use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::tools::{
    ToolCaller, ToolError, normalize_cve_id, parse_arguments, restricted_actor, system_id_schema,
};
use crate::models::poam::{CreateScannedCvePoamRequest, PoamStatus};
use crate::services::poam::{
    self as poam_service, PoamError, RestrictedProgressAction, SystemClock,
};

fn poam_id_schema() -> Value {
    json!({"type": "string", "format": "uuid", "description": "POA&M ID."})
}

/// Returns the closed create-or-reuse argument schema.
pub(super) fn create_cve_poam_schema() -> Value {
    json!({
        "type": "object", "additionalProperties": false,
        "required": ["system_id", "cve_id", "canonical_package_name", "expected_scan_id", "title"],
        "properties": {
            "system_id": system_id_schema(),
            "cve_id": {"type": "string", "minLength": 13, "maxLength": 20},
            "canonical_package_name": {"type": "string", "minLength": 1, "maxLength": 4096,
                "description": "Exact, case-sensitive canonical package name."},
            "expected_scan_id": {"type": "string", "format": "uuid",
                "description": "Expected sealed HEAD scan ID. Used only for a new planning POA&M."},
            "title": {"type": "string", "minLength": 1, "maxLength": 256},
            "plan": {"type": "string", "maxLength": 16384},
        }
    })
}

/// Creates or reuses a POA&M via the environment-scoped owner credential.
///
/// # Errors
/// Returns argument, authorization, evidence, conflict, or persistence errors.
pub(super) async fn create_cve_poam(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let mut request: CreateScannedCvePoamRequest = parse_arguments(arguments)?;
    request.cve_id = normalize_cve_id(&request.cve_id)
        .filter(|id| id.len() <= 20)
        .ok_or_else(|| ToolError::InvalidArguments("cve_id must be a canonical CVE ID".into()))?;
    let package = &request.canonical_package_name;
    if package.trim() != package
        || package.is_empty()
        || package.len() > 4096
        || package.contains('\0')
    {
        return Err(ToolError::InvalidArguments(
            "canonical_package_name must be an exact non-empty package name".into(),
        ));
    }
    poam_service::restricted_create_cve_poam(pool, &restricted_actor(caller), request)
        .await
        .map_err(|error| tool_error(error, "poams:create"))
}

fn revision_schema() -> Value {
    json!({
        "type": "integer",
        "description": "Current POA&M revision from get_poam_detail. A stale \
    revision is rejected with stale_revision and changes nothing."
    })
}

fn milestone_id_schema() -> Value {
    json!({
        "type": "string",
        "format": "uuid",
        "description": "Milestone ID from get_poam_detail."
    })
}

fn date_schema(description: &str) -> Value {
    json!({"type": "string", "format": "date", "description": description})
}

/// Returns the JSON Schema (draft 2020-12) of `update_poam_progress`.
///
/// `action` is one object tagged by `type`: `add_note`, `set_status`,
/// `add_milestone`, `update_milestone` or `remove_milestone`. Every object
/// is closed to other properties.
pub fn update_poam_progress_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "poam_id": poam_id_schema(),
            "revision": revision_schema(),
            "action": {
                "description": "Exactly one progress change, selected by type.",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "type": {"const": "add_note"},
                            "text": {
                                "type": "string",
                                "description": "Note text. Required after trimming; at most 4096 bytes."
                            }
                        },
                        "required": ["type", "text"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": {"const": "set_status"},
                            "status": {
                                "enum": ["open", "in_progress", "blocked", "awaiting_verification"],
                                "description": "Destination status. completed is not allowed; \
    only verification and closure by a person complete a POA&M. awaiting_verification \
    can return only to in_progress or blocked."
                            },
                            "note": {
                                "type": "string",
                                "description": "Optional context, at most 4096 bytes."
                            }
                        },
                        "required": ["type", "status"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": {"const": "add_milestone"},
                            "title": {
                                "type": "string",
                                "description": "Milestone title. Required after trimming; at most 256 bytes."
                            },
                            "target_date": date_schema("Planned completion date, YYYY-MM-DD.")
                        },
                        "required": ["type", "title", "target_date"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": {"const": "update_milestone"},
                            "milestone_id": milestone_id_schema(),
                            "title": {
                                "type": "string",
                                "description": "New title, not blank, at most 256 bytes."
                            },
                            "target_date": date_schema("New planned completion date, YYYY-MM-DD."),
                            "completed": {
                                "type": "boolean",
                                "description": "Marks the milestone complete or incomplete."
                            }
                        },
                        "required": ["type", "milestone_id"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": {
                            "type": {"const": "remove_milestone"},
                            "milestone_id": milestone_id_schema()
                        },
                        "required": ["type", "milestone_id"],
                        "additionalProperties": false
                    }
                ]
            }
        },
        "required": ["poam_id", "revision", "action"],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `verify_poam`.
pub fn verify_poam_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "poam_id": poam_id_schema(),
            "revision": revision_schema()
        },
        "required": ["poam_id", "revision"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgressArgs {
    poam_id: Uuid,
    revision: i64,
    action: ProgressActionArgs,
}

/// Mirrors [`RestrictedProgressAction`] in the MCP wire format. The service
/// validates the values; this type only fixes the shape.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ProgressActionArgs {
    AddNote {
        text: String,
    },
    SetStatus {
        status: PoamStatus,
        note: Option<String>,
    },
    AddMilestone {
        title: String,
        target_date: NaiveDate,
    },
    UpdateMilestone {
        milestone_id: Uuid,
        title: Option<String>,
        target_date: Option<NaiveDate>,
        completed: Option<bool>,
    },
    RemoveMilestone {
        milestone_id: Uuid,
    },
}

impl From<ProgressActionArgs> for RestrictedProgressAction {
    fn from(value: ProgressActionArgs) -> Self {
        match value {
            ProgressActionArgs::AddNote { text } => Self::AddNote { text },
            ProgressActionArgs::SetStatus { status, note } => Self::SetStatus { status, note },
            ProgressActionArgs::AddMilestone { title, target_date } => {
                Self::AddMilestone { title, target_date }
            }
            ProgressActionArgs::UpdateMilestone {
                milestone_id,
                title,
                target_date,
                completed,
            } => Self::UpdateMilestone {
                milestone_id,
                title,
                target_date,
                completed,
            },
            ProgressActionArgs::RemoveMilestone { milestone_id } => {
                Self::RemoveMilestone { milestone_id }
            }
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyArgs {
    poam_id: Uuid,
    revision: i64,
}

/// Applies one progress action to a POA&M for an access token with
/// `poams:write`.
///
/// Returns `{"poam_id", "revision", "status", "changed"}`. `revision` is the
/// new revision. `changed` is the recorded activity payload, or `null` when
/// the action changed nothing (for example an `update_milestone` with the
/// current values); the revision is then unchanged.
///
/// # Errors
///
/// - [`ToolError::InvalidArguments`] for arguments that do not match the
///   schema, and for service validation and conflict results, as
///   `<code>: <message>`. `stale_revision` tells the agent to read the POA&M
///   again with `get_poam_detail`.
/// - [`ToolError::Forbidden`] when the token is revoked or expired, lacks
///   `poams:write`, or its owner is no longer an active Operator or Admin.
/// - [`ToolError::NotFound`] when the POA&M or milestone is absent or any
///   POA&M subject is outside the effective environments.
/// - [`ToolError::Internal`] on a database failure.
pub async fn update_poam_progress(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: ProgressArgs = parse_arguments(arguments)?;
    let outcome = poam_service::restricted_progress(
        pool,
        &restricted_actor(caller),
        args.poam_id,
        args.revision,
        args.action.into(),
    )
    .await
    .map_err(|error| tool_error(error, "poams:write"))?;
    serde_json::to_value(outcome).map_err(|error| ToolError::Internal(error.into()))
}

/// Records a sealed verification attempt for an access token with
/// `poams:verify`.
///
/// The POA&M must be `awaiting_verification`. The attempt checks the current
/// exact evidence of every finding, as the browser verification does.
/// Returns `{"attempt_id", "outcome", "revision", "items", "cve_items"}`,
/// where `outcome` is `accepted` or `rejected`. The tool never closes the
/// POA&M; a person closes it after an accepted attempt.
///
/// # Errors
///
/// As for [`update_poam_progress`], with `poams:verify` as the scope. A POA&M
/// that is not awaiting verification returns
/// [`ToolError::InvalidArguments`] with `invalid_transition`.
pub async fn verify_poam(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: VerifyArgs = parse_arguments(arguments)?;
    poam_service::restricted_verify(
        pool,
        &restricted_actor(caller),
        args.poam_id,
        args.revision,
        &SystemClock,
    )
    .await
    .map_err(|error| tool_error(error, "poams:verify"))
}

fn tool_error(error: PoamError, scope: &str) -> ToolError {
    match error {
        PoamError::NotFound => ToolError::NotFound,
        PoamError::Forbidden => ToolError::Forbidden(format!(
            "This token cannot use {scope}: it is revoked or expired, does not \
             grant {scope}, or its owner is no longer an active Operator or Admin"
        )),
        PoamError::Conflict("stale_revision", message) => ToolError::InvalidArguments(format!(
            "stale_revision: {message}. Read the POA&M again with get_poam_detail \
             and retry with its current revision."
        )),
        PoamError::Validation(code, message)
        | PoamError::Conflict(code, message)
        | PoamError::ConflictDetails(code, message, _)
        | PoamError::Precondition(code, message, _) => {
            ToolError::InvalidArguments(format!("{code}: {message}"))
        }
        PoamError::Database(error) => ToolError::Internal(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_actions_are_tagged_and_closed() {
        let args: ProgressArgs = parse_arguments(json!({
            "poam_id": Uuid::nil(),
            "revision": 3,
            "action": {"type": "set_status", "status": "blocked"}
        }))
        .expect("set_status parses");
        assert!(matches!(
            args.action,
            ProgressActionArgs::SetStatus {
                status: PoamStatus::Blocked,
                note: None
            }
        ));
        for invalid in [
            json!({"type": "add_note", "text": "x", "extra": 1}),
            json!({"type": "close"}),
            json!({"text": "x"}),
            json!({"type": "update_poam", "risk": "low"}),
        ] {
            assert!(
                parse_arguments::<ProgressArgs>(
                    json!({"poam_id": Uuid::nil(), "revision": 1, "action": invalid})
                )
                .is_err(),
                "{invalid}"
            );
        }
        assert!(
            parse_arguments::<ProgressArgs>(json!({
                "poam_id": Uuid::nil(),
                "revision": 1,
                "risk": "low",
                "action": {"type": "add_note", "text": "x"}
            }))
            .is_err()
        );
    }

    #[test]
    fn stale_revision_points_to_get_poam_detail() {
        let error = tool_error(
            PoamError::Conflict("stale_revision", "POA&M revision is stale".into()),
            "poams:write",
        );
        assert!(matches!(
            error,
            ToolError::InvalidArguments(message)
                if message.starts_with("stale_revision:") && message.contains("get_poam_detail")
        ));
    }

    #[test]
    fn schemas_are_closed_objects() {
        for schema in [update_poam_progress_schema(), verify_poam_schema()] {
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
        }
        for action in update_poam_progress_schema()["properties"]["action"]["oneOf"]
            .as_array()
            .expect("action variants")
        {
            assert_eq!(action["additionalProperties"], false);
        }
    }
}
