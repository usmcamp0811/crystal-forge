//! Policy authoring services shared by the Admin browser handlers and the MCP
//! policy authoring tools (TASK-534.7).
//!
//! This module owns the domain rules of policy draft creation (a new lineage
//! with an initial draft, or a draft derived from the published version),
//! MCP draft updates, policy definition validation, and the JSON/TOML policy
//! interchange preview and import. Browser handlers call it with
//! [`AuthoringActor::browser`]; MCP tools call it with
//! [`AuthoringActor::token`]. HTTP and MCP response shapes stay in the
//! callers.
//!
//! # Authorization
//!
//! Browser handlers check the session role before they call a service; the
//! services do not repeat that check and do not change it. For an
//! access-token actor, every write transaction first locks and checks the
//! owner row, the token row and the owner's roles (see `authorize_token_tx`):
//! the owner must be active and still an Admin, and the token must be live,
//! owned by the owner, of audience `mcp`, and hold the operation's scope
//! (`policies:draft` or `policies:import`). A failure returns
//! [`PolicyAuthoringError::Forbidden`] and writes nothing.
//!
//! An access-token actor may change only a mutable draft version whose
//! `created_by` is the token owner. Published (`accepted`) and `deprecated`
//! versions never change.
//!
//! # Concurrency
//!
//! CONCURRENCY: A token write locks in this order: owner row, token row and
//! role rows (`FOR SHARE`), then the domain locks of the operation. Draft
//! derivation and draft updates lock the lineage row `FOR UPDATE` and then the
//! version row, the order of `ensure_policy_draft` and
//! `write_policy_version_digest`. A new lineage takes the `policy-name:<name>`
//! advisory lock, and an import takes its sorted `policy-lineage:`,
//! `policy-version:` and `policy-name:` advisory locks, the keys that the
//! browser import uses. No writer of policy rows waits for an identity row,
//! and token revocation and role changes do not wait for policy locks, so no
//! wait-for cycle can form. A revocation or role change that committed first
//! is visible to the identity check; a later one waits for this transaction.
//!
//! # Negative guarantees
//!
//! No function here trusts, publishes, assigns, deletes, changes an
//! enforcement mode or enables a lineage. A token actor never changes the
//! lineage `enabled` flag; new lineages created through MCP start disabled.
//! Imported versions are untrusted drafts. No function evaluates Nix: the
//! config validators only parse expressions, with the 2-second deadline of
//! `cf_compliance::policy_config`.

use std::collections::{HashMap, HashSet};

use axum::http::StatusCode;
use cf_compliance::policy_document::{NormalizedPolicyImport, parse_policy_document};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::compliance::digest::{
    PolicyVersionCanonical, refresh_policy_mapping_digest, write_policy_version_digest,
};
use crate::compliance::mappings::decode_evidence_specs_strict;
use crate::compliance::xccdf::reconciliation::{
    ExistingPolicyIdentity, NativePolicyIdentity, PolicyReconciliationPlan, ReconcileConflict,
    ReconcileDecision, plan_policy_reconciliation,
};
use crate::models::auth_identity::AuthRole;
use crate::models::deployment_policies::{
    COMPOSITE_POLICY_TYPE, CreateDeploymentPolicyRequest, CreatePolicyRequirementMapping,
    validate_policy_type_config_async,
};
use crate::queries::compliance::{
    PolicyDraftDerivationError, PolicyDraftIntent, ensure_policy_draft,
};
use crate::queries::deployment_policies::create_deployment_policy_with_mappings_in_tx;
use crate::queries::framework_requirements::insert_policy_mapping_in_tx;

/// Scope that permits draft creation, draft updates and draft validation.
pub const POLICIES_DRAFT_SCOPE: &str = "policies:draft";
/// Scope that permits a policy interchange import.
pub const POLICIES_IMPORT_SCOPE: &str = "policies:import";

/// Policy types that a user may create or select.
///
/// `require_cf_agent` is a built-in invariant and is never accepted. The
/// order is part of the browser error message "Must be one of: ...".
pub const SUPPORTED_POLICY_TYPES: [&str; 8] = [
    "require_packages",
    "custom_check",
    "require_cve_check",
    "time_window",
    "require_approvals",
    "canary_rollout",
    "cve_threshold",
    "composite",
];

/// Execution phases that a policy version may declare.
///
/// The list is the `executionPhase` whitelist of `cf-xccdf-1.xsd`, which the
/// XCCDF writer enforces at export. A composite policy must use
/// `multi-phase`.
pub const EXECUTION_PHASES: [&str; 6] = [
    "nix-evaluation",
    "post-build",
    "pre-deployment",
    "deployment-orchestration",
    "continuous-assessment",
    "multi-phase",
];

/// Message of [`PolicyAuthoringError::Forbidden`] for a draft that another
/// user created. It reveals nothing else about the draft.
pub const DRAFT_OWNED_BY_OTHER_USER: &str = "draft is owned by another user";

const DRAFT_CREATE_FAILED: &str = "Failed to create policy draft";
const DRAFT_UPDATE_FAILED: &str = "Failed to update policy draft";
const IMPORT_FAILED: &str = "Failed to import policies";
const PREVIEW_FAILED: &str = "Failed to analyze policy compatibility";
const BUILTIN_POLICY_MESSAGE: &str = "The Crystal Forge agent requirement is a built-in \
invariant and cannot be created or edited as a policy.";

/// Requirement versions that a mapping may reference: the framework release
/// must have a computed digest and finished migration recovery. This is the
/// rule of `insert_policy_mapping_in_tx`.
const USABLE_REQUIREMENTS_SQL: &str = "SELECT rv.id FROM compliance_requirement_versions rv \
JOIN compliance_framework_versions fv ON fv.id = rv.framework_version_id \
WHERE rv.id = ANY($1) AND fv.semantic_digest <> 'pending' \
AND fv.migration_recovery_status = 'finalized'";

/// Identifies who performs a policy authoring operation.
///
/// `user_id` is recorded as `created_by` and as the audit actor. For a token
/// actor it is the token owner, and `access_token_id` names the token; the
/// token is recorded next to the owner, never instead of the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoringActor {
    /// Identifies the acting user (the owner for a token actor).
    pub user_id: Uuid,
    /// Identifies the owner access token, or `None` for a browser session.
    pub access_token_id: Option<Uuid>,
}

impl AuthoringActor {
    /// Returns a browser-session actor. The caller has already checked the
    /// session role.
    pub fn browser(user_id: Uuid) -> Self {
        Self {
            user_id,
            access_token_id: None,
        }
    }

    /// Returns an access-token actor. Every write reauthorizes the token and
    /// the owner inside its transaction.
    pub fn token(owner_user_id: Uuid, token_id: Uuid) -> Self {
        Self {
            user_id: owner_user_id,
            access_token_id: Some(token_id),
        }
    }
}

/// Reports why a policy authoring operation wrote nothing.
///
/// Every error leaves the database unchanged: the operation's transaction
/// rolls back when the error is returned.
#[derive(Debug)]
pub enum PolicyAuthoringError {
    /// The token, the owner or the draft owner check failed. The message
    /// names the missing authority and contains no resource data.
    Forbidden(String),
    /// The lineage or version does not exist.
    NotFound,
    /// The caller's input or the target state does not permit the operation.
    /// `code` is a stable machine-readable reason.
    Invalid {
        /// Stable reason code, for example `name_conflict`.
        code: &'static str,
        /// Describes the problem with the caller's own input.
        message: String,
    },
    /// The Nix parser gave no verdict within its deadline. The input was not
    /// checked; the caller should retry.
    ValidatorUnavailable(String),
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
    /// The interchange document digest differs from the expected digest.
    SourceDigestMismatch {
        /// Digest that the caller expected.
        expected: String,
        /// SHA-256 of the received document bytes.
        actual: String,
    },
    /// The interchange import has blocking identity conflicts or name
    /// collisions, in the browser response format.
    InterchangeConflicts(Vec<Value>),
    /// A database or invariant failure. `context` is the public message.
    Internal {
        /// Public description of the failed operation.
        context: &'static str,
        /// Detail for server logs only.
        error: anyhow::Error,
    },
}

fn internal<E>(context: &'static str) -> impl FnOnce(E) -> PolicyAuthoringError
where
    E: Into<anyhow::Error>,
{
    move |error| PolicyAuthoringError::Internal {
        context,
        error: error.into(),
    }
}

fn is_mutable_state(publication_state: &str) -> bool {
    matches!(publication_state, "incomplete" | "draft" | "interim")
}

/// Returns the lowercase hex SHA-256 of interchange document bytes.
pub fn source_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

// ── Authorization and attribution ───────────────────────────────────────────

/// Reauthorizes an access-token actor inside a write transaction.
///
/// A browser actor passes without a query. For a token actor the owner must
/// be active and hold Admin, and the token row must be live, owned by the
/// owner, of audience `mcp`, and hold `scope`. The bundle authoring service
/// (`services::bundle_authoring`) uses the same check with `bundles:draft`.
pub(crate) async fn authorize_token_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthoringActor,
    scope: &'static str,
) -> Result<(), PolicyAuthoringError> {
    let Some(token_id) = actor.access_token_id else {
        return Ok(());
    };
    let denied = || {
        PolicyAuthoringError::Forbidden(format!(
            "This token cannot use {scope}: it is revoked or expired, does not grant \
             {scope}, or its owner is no longer an active Admin"
        ))
    };
    // CONCURRENCY: Owner row, token row, roles, all `FOR SHARE`, before any
    // policy lock (see the module documentation). Token revoke and issue lock
    // the owner row and then the token row; role writers lock the owner row
    // `FOR UPDATE`.
    let owner_active =
        sqlx::query_scalar::<_, bool>("SELECT is_active FROM users WHERE id = $1 FOR SHARE")
            .bind(actor.user_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(internal("Failed to authorize the access token"))?;
    if owner_active != Some(true) {
        return Err(denied());
    }
    // SECURITY: The token row is the revocation authority. Admission at the
    // start of the request does not prove that the token is live now.
    let token_live = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM api_access_tokens
           WHERE id = $1 AND user_id = $2 AND audience = 'mcp' AND revoked_at IS NULL
             AND expires_at > clock_timestamp()
           FOR SHARE"#,
    )
    .bind(token_id)
    .bind(actor.user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal("Failed to authorize the access token"))?
    .is_some();
    let scope_granted: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM api_access_token_scopes WHERE token_id = $1 AND scope = $2)",
    )
    .bind(token_id)
    .bind(scope)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal("Failed to authorize the access token"))?;
    if !token_live || !scope_granted {
        return Err(denied());
    }
    // SECURITY: The authoring scopes are Admin-only at every use. A demotion
    // that committed before this statement denies the write.
    let roles = sqlx::query_scalar::<_, AuthRole>(
        "SELECT role FROM user_role_assignments WHERE user_id = $1 ORDER BY role FOR SHARE",
    )
    .bind(actor.user_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(internal("Failed to authorize the access token"))?;
    if !roles.contains(&AuthRole::Admin) {
        return Err(denied());
    }
    Ok(())
}

/// Inserts one `admin_audit_events` row for a policy authoring change.
///
/// The row uses the caller's transaction, so it commits or rolls back with
/// the change. The actor is `actor.user_id`. For a token actor the row also
/// stores `access_token_id`, and the metadata receives `via: "mcp"`,
/// `access_token_id` and an `access_token_name` snapshot, the TASK-534.5
/// attribution format. Callers MUST NOT put document text or other
/// user-supplied content into `metadata`; identifiers, digests and field
/// names only.
///
/// # Errors
///
/// Returns the database error when the row cannot be inserted.
pub async fn insert_policy_audit_event(
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthoringActor,
    action: &str,
    target: &str,
    metadata: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO admin_audit_events
               (actor_user_id, actor_identifier, action, target, metadata, access_token_id)
           VALUES ($1, (SELECT COALESCE(email, username) FROM users WHERE id = $1), $2, $3,
             CASE WHEN $5::uuid IS NULL THEN $4::jsonb
                  ELSE $4::jsonb || jsonb_build_object(
                    'via', 'mcp',
                    'access_token_id', $5::uuid,
                    'access_token_name', (SELECT name FROM api_access_tokens WHERE id = $5::uuid))
             END,
             $5::uuid)"#,
    )
    .bind(actor.user_id)
    .bind(action)
    .bind(target)
    .bind(metadata)
    .bind(actor.access_token_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Records on a version that an access token created it. A browser actor
/// records nothing; the columns keep their defaults.
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
        r#"UPDATE deployment_policy_versions
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

async fn draft_revision(
    tx: &mut Transaction<'_, Postgres>,
    version_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT draft_revision FROM deployment_policy_versions WHERE id = $1")
        .bind(version_id)
        .fetch_one(&mut **tx)
        .await
}

// ── Config and mapping validators ───────────────────────────────────────────

fn normalize_required_packages(packages: &[Value]) -> Result<Vec<String>, (StatusCode, String)> {
    let mut normalized = packages
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "config.packages must contain only non-empty strings".to_string(),
                ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(invalid) = normalized.iter().find(|pname| {
        !pname
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
            || !pname.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.' | b'_')
            })
    }) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "config.packages entry {invalid:?} is not a package pname; checks apply only to direct config.environment.systemPackages entries"
            ),
        ));
    }
    normalized.sort();
    normalized.dedup();
    if normalized.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "require_packages policy requires at least one package".to_string(),
        ));
    }
    Ok(normalized)
}

/// Checks the type-specific shape of a policy config and returns the
/// normalized config.
///
/// This check is synchronous and does not parse Nix; see
/// [`validate_policy_config_async`] for the complete validation.
///
/// # Errors
///
/// Returns `400 Bad Request` with a message when the config is null, not an
/// object, or does not match the policy type.
pub fn validate_policy_config_fields(
    policy_type: &str,
    config: &Value,
) -> Result<Value, (StatusCode, String)> {
    if config.is_null() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Policy config cannot be null".to_string(),
        ));
    }

    let obj = config.as_object().ok_or((
        StatusCode::BAD_REQUEST,
        "Policy config must be a JSON object".to_string(),
    ))?;

    if policy_type != "custom_check"
        && let Some(strict) = obj.get("strict")
    {
        if !strict.is_boolean() {
            return Err((
                StatusCode::BAD_REQUEST,
                "config.strict must be a boolean when provided".to_string(),
            ));
        }
    }

    let mut validated_config = config.clone();

    match policy_type {
        "require_cf_agent" => {
            if let Some(false) = obj.get("strict").and_then(|v| v.as_bool()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "require_cf_agent policy must enforce config.strict = true".to_string(),
                ));
            }
        }
        "require_packages" => {
            let packages = obj.get("packages").and_then(|v| v.as_array()).ok_or((
                StatusCode::BAD_REQUEST,
                "require_packages policy requires config.packages as a non-empty array".to_string(),
            ))?;

            if packages.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "require_packages policy requires at least one package".to_string(),
                ));
            }

            let normalized_packages = normalize_required_packages(packages)?;
            if let Some(config_obj) = validated_config.as_object_mut() {
                config_obj.insert(
                    "packages".to_string(),
                    Value::Array(normalized_packages.into_iter().map(Value::String).collect()),
                );
            }
        }
        "custom_check" => {
            validated_config = crate::models::custom_check::validate_and_normalize_config(
                config,
                crate::models::custom_check::ExpressionBinding::Compatible,
                false,
            )
            .map_err(|message| (StatusCode::BAD_REQUEST, message))?;
        }
        "require_cve_check" => {
            // Validate by attempting deserialization into CveCheckConfig
            serde_json::from_value::<crate::models::deployment_policies::CveCheckConfig>(
                config.clone(),
            )
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid require_cve_check config: {}", e),
                )
            })?;

            // Validate when_no_scan string value if present
            if let Some(wns) = obj.get("when_no_scan").and_then(|v| v.as_str()) {
                if wns != "block" && wns != "skip" {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "config.when_no_scan must be \"block\" or \"skip\"".to_string(),
                    ));
                }
            }
        }
        "time_window" => {
            // Validate by attempting deserialization
            let parsed = serde_json::from_value::<
                crate::models::deployment_policies::TimeWindowConfig,
            >(config.clone())
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid time_window config: {}", e),
                )
            })?;
            crate::services::time_window_policy::validate_config(&parsed).map_err(|message| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid time_window config: {message}"),
                )
            })?;

            // Validate action value
            if let Some(action) = obj.get("action").and_then(|v| v.as_str()) {
                if action != "block" && action != "warn" {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "config.action must be \"block\" or \"warn\"".to_string(),
                    ));
                }
            }
        }
        "require_approvals" => {
            // Validate by attempting deserialization
            serde_json::from_value::<crate::models::deployment_policies::ApprovalConfig>(
                config.clone(),
            )
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid require_approvals config: {}", e),
                )
            })?;

            // Validate count > 0
            if let Some(count) = obj.get("count").and_then(|v| v.as_u64()) {
                if count == 0 {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "config.count must be greater than 0".to_string(),
                    ));
                }
            }
        }
        "canary_rollout" => {
            // Validate by attempting deserialization
            serde_json::from_value::<crate::models::deployment_policies::CanaryConfig>(
                config.clone(),
            )
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid canary_rollout config: {}", e),
                )
            })?;

            // Validate percentage is between 1-100
            if let Some(pct) = obj.get("percentage").and_then(|v| v.as_u64()) {
                if pct == 0 || pct > 100 {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "config.percentage must be between 1 and 100".to_string(),
                    ));
                }
            }
        }
        "cve_threshold" => {
            // Validate by attempting deserialization
            serde_json::from_value::<crate::models::deployment_policies::CveThresholdConfig>(
                config.clone(),
            )
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid cve_threshold config: {}", e),
                )
            })?;

            // Validate no_scan_behavior
            if let Some(nsb) = obj.get("no_scan_behavior").and_then(|v| v.as_str()) {
                if nsb != "block" && nsb != "skip" && nsb != "warn" {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "config.no_scan_behavior must be \"block\", \"skip\", or \"warn\""
                            .to_string(),
                    ));
                }
            }
        }
        "composite" => {}
        _ => {}
    }

    Ok(validated_config)
}

/// Validates and normalizes a policy config, including the bounded Nix syntax
/// parse of `custom_check` and composite expressions.
///
/// The Nix parser runs on the blocking pool with a 2-second deadline. It
/// parses only; it never evaluates.
///
/// # Errors
///
/// Returns `(status, message)`: `400 Bad Request` for an invalid config, and
/// `503 Service Unavailable` when the parser gave no verdict (see
/// [`policy_validation_status`]).
pub async fn validate_policy_config_async(
    policy_type: &str,
    config: &Value,
) -> Result<Value, (StatusCode, String)> {
    let validated_config = validate_policy_config_fields(policy_type, config)?;
    validate_policy_type_config_async(policy_type, &validated_config)
        .await
        .map_err(|message| (policy_validation_status(&message), message))?;
    Ok(validated_config)
}

/// Returns 503 when the Nix parser gave no verdict and 400 otherwise.
///
/// An unavailable parser says nothing about the submitted policy, so the
/// client must retry instead of editing a valid expression.
pub fn policy_validation_status(message: &str) -> StatusCode {
    if crate::models::deployment_policies::is_syntax_validation_unavailable(message) {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    }
}

/// Checks manual requirement mappings of a policy create request.
///
/// Relationship, coverage and provenance are checked for every mapping first;
/// then each requirement version may appear once. The messages are the
/// browser create-policy messages.
///
/// # Errors
///
/// Returns the first violation as a message.
pub fn validate_manual_mappings(mappings: &[CreatePolicyRequirementMapping]) -> Result<(), String> {
    for mapping in mappings {
        if !matches!(
            mapping.relationship.as_str(),
            "implements" | "supports" | "provides_evidence_for"
        ) {
            return Err("Invalid mapping relationship".to_string());
        }
        if !matches!(mapping.coverage.as_str(), "full" | "partial") {
            return Err("Invalid mapping coverage".to_string());
        }
        if mapping.provenance != "manual" {
            return Err("Create-policy mappings must use manual provenance".to_string());
        }
    }
    let mut requirement_ids = HashSet::new();
    for mapping in mappings {
        if !requirement_ids.insert(mapping.requirement_version_id) {
            return Err("A requirement version may only be mapped once".to_string());
        }
    }
    Ok(())
}

// ── Definition validation ───────────────────────────────────────────────────

/// Describes one validation finding of a policy definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    /// Stable machine-readable reason, for example `invalid_config`.
    pub code: &'static str,
    /// Path of the input field, for example `requirement_mappings[0].coverage`.
    pub field: String,
    /// Human-readable description of the finding.
    pub message: String,
}

impl ValidationIssue {
    fn new(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            field: field.into(),
            message: message.into(),
        }
    }
}

/// Holds the content of a policy version that the validator checks.
#[derive(Debug, Clone)]
pub struct PolicyDefinition {
    /// Policy type; one of [`SUPPORTED_POLICY_TYPES`].
    pub policy_type: String,
    /// Type-specific config. It can contain Nix expressions; it is parsed,
    /// never evaluated.
    pub config: Value,
    /// Execution phase; `None` selects `multi-phase` for a composite policy
    /// and `nix-evaluation` otherwise.
    pub execution_phase: Option<String>,
    /// Version compliance metadata; must be a JSON object.
    pub compliance_metadata: Value,
    /// Manual requirement mappings to check.
    pub requirement_mappings: Vec<CreatePolicyRequirementMapping>,
}

/// Holds the normalized values of a valid [`PolicyDefinition`].
#[derive(Debug, Clone)]
pub struct ValidatedDefinition {
    /// Config after type-specific normalization.
    pub config: Value,
    /// Effective execution phase.
    pub execution_phase: String,
}

/// Reports why a [`PolicyDefinition`] is not valid or was not checked.
#[derive(Debug)]
pub enum DefinitionError {
    /// The definition has at least one issue.
    Invalid(Vec<ValidationIssue>),
    /// The Nix parser gave no verdict; the definition was not fully checked.
    ValidatorUnavailable(String),
    /// The requirement lookup failed.
    Database(sqlx::Error),
}

/// Validates a policy definition with the server validators.
///
/// Checks, in order: the policy type, the config (shape and bounded Nix
/// syntax parse), the execution phase (composite policies need
/// `multi-phase`), the compliance metadata (object; strict evidence
/// specifications), and the requirement mappings (relationship, coverage,
/// manual provenance, no duplicate, usable requirement version). All issues
/// are collected; the config is not checked when the type is invalid. The
/// only database access is one read of the requirement versions through
/// `executor`. Nothing is written and no Nix is evaluated. A valid result
/// does not prove that a check is effective.
///
/// # Errors
///
/// - [`DefinitionError::Invalid`] with every issue found.
/// - [`DefinitionError::ValidatorUnavailable`] when the Nix parser did not
///   finish within its deadline.
/// - [`DefinitionError::Database`] when the requirement lookup fails.
pub async fn validate_policy_definition<'e, E>(
    executor: E,
    definition: &PolicyDefinition,
) -> Result<ValidatedDefinition, DefinitionError>
where
    E: sqlx::PgExecutor<'e>,
{
    let mut issues = Vec::new();
    let policy_type = definition.policy_type.as_str();
    let composite = policy_type == COMPOSITE_POLICY_TYPE;
    let mut config = definition.config.clone();
    if policy_type == "require_cf_agent" {
        issues.push(ValidationIssue::new(
            "builtin_policy_type",
            "policy_type",
            BUILTIN_POLICY_MESSAGE,
        ));
    } else if !SUPPORTED_POLICY_TYPES.contains(&policy_type) {
        issues.push(ValidationIssue::new(
            "invalid_policy_type",
            "policy_type",
            format!(
                "Invalid policy_type '{policy_type}'. Must be one of: {}",
                SUPPORTED_POLICY_TYPES.join(", ")
            ),
        ));
    } else {
        match validate_policy_config_async(policy_type, &definition.config).await {
            Ok(normalized) => config = normalized,
            Err((status, message)) if status == StatusCode::SERVICE_UNAVAILABLE => {
                return Err(DefinitionError::ValidatorUnavailable(message));
            }
            Err((_, message)) => {
                issues.push(ValidationIssue::new("invalid_config", "config", message));
            }
        }
    }

    let execution_phase = definition.execution_phase.clone().unwrap_or_else(|| {
        if composite {
            "multi-phase".to_owned()
        } else {
            "nix-evaluation".to_owned()
        }
    });
    if !EXECUTION_PHASES.contains(&execution_phase.as_str()) {
        issues.push(ValidationIssue::new(
            "invalid_execution_phase",
            "execution_phase",
            format!(
                "execution_phase must be one of: {}",
                EXECUTION_PHASES.join(", ")
            ),
        ));
    } else if composite && execution_phase != "multi-phase" {
        issues.push(ValidationIssue::new(
            "invalid_execution_phase",
            "execution_phase",
            "composite policies must use execution_phase multi-phase",
        ));
    }

    if !definition.compliance_metadata.is_object() {
        issues.push(ValidationIssue::new(
            "invalid_compliance_metadata",
            "compliance_metadata",
            "compliance_metadata must be a JSON object",
        ));
    } else if let Err(error) = decode_evidence_specs_strict(&definition.compliance_metadata) {
        issues.push(ValidationIssue::new(
            "invalid_compliance_metadata",
            "compliance_metadata.evidence_specs",
            error.to_string(),
        ));
    }

    let mut seen = HashSet::new();
    for (index, mapping) in definition.requirement_mappings.iter().enumerate() {
        let field = format!("requirement_mappings[{index}]");
        if !matches!(
            mapping.relationship.as_str(),
            "implements" | "supports" | "provides_evidence_for"
        ) {
            issues.push(ValidationIssue::new(
                "invalid_mapping",
                format!("{field}.relationship"),
                "relationship must be implements, supports or provides_evidence_for",
            ));
        }
        if !matches!(mapping.coverage.as_str(), "full" | "partial") {
            issues.push(ValidationIssue::new(
                "invalid_mapping",
                format!("{field}.coverage"),
                "coverage must be full or partial",
            ));
        }
        if mapping.provenance != "manual" {
            issues.push(ValidationIssue::new(
                "invalid_mapping",
                format!("{field}.provenance"),
                "authored mappings must use manual provenance",
            ));
        }
        if !seen.insert(mapping.requirement_version_id) {
            issues.push(ValidationIssue::new(
                "duplicate_mapping",
                field,
                "A requirement version may only be mapped once",
            ));
        }
    }
    let requested: Vec<Uuid> = seen.into_iter().collect();
    if !requested.is_empty() {
        let usable: Vec<Uuid> = sqlx::query_scalar(USABLE_REQUIREMENTS_SQL)
            .bind(&requested)
            .fetch_all(executor)
            .await
            .map_err(DefinitionError::Database)?;
        for (index, mapping) in definition.requirement_mappings.iter().enumerate() {
            if !usable.contains(&mapping.requirement_version_id) {
                issues.push(ValidationIssue::new(
                    "unknown_requirement",
                    format!("requirement_mappings[{index}].requirement_version_id"),
                    "requirement version does not exist or its framework release is not finalized",
                ));
            }
        }
    }

    if issues.is_empty() {
        Ok(ValidatedDefinition {
            config,
            execution_phase,
        })
    } else {
        Err(DefinitionError::Invalid(issues))
    }
}

fn definition_error(error: DefinitionError) -> PolicyAuthoringError {
    match error {
        DefinitionError::Invalid(issues) => PolicyAuthoringError::Invalid {
            code: "invalid_definition",
            message: issues
                .iter()
                .map(|issue| format!("{} ({}): {}", issue.field, issue.code, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        },
        DefinitionError::ValidatorUnavailable(message) => {
            PolicyAuthoringError::ValidatorUnavailable(message)
        }
        DefinitionError::Database(error) => PolicyAuthoringError::Internal {
            context: "Failed to validate the policy definition",
            error: error.into(),
        },
    }
}

// ── Draft creation ──────────────────────────────────────────────────────────

/// Identifies a draft version that an operation created or changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyDraftRef {
    /// Lineage ID.
    pub policy_id: Uuid,
    /// Exact draft version ID.
    pub version_id: Uuid,
    /// Version label, unique within the lineage.
    pub version: String,
    /// Current draft revision.
    pub draft_revision: i64,
    /// Published version that the draft was derived from, if any.
    pub derived_from_version_id: Option<Uuid>,
}

/// Derives a new mutable draft from the lineage's current published version
/// inside the caller's transaction.
///
/// The browser route `POST /api/v1/policies/:policy_id/drafts` and the MCP
/// tool `create_policy_draft` (`mode: derive`) share this function. For a
/// token actor it first reauthorizes the token with `policies:draft`. It locks
/// the lineage row `FOR UPDATE`, derives the draft through
/// [`ensure_policy_draft`] with `created_by = actor.user_id`, records token
/// attribution on the version, and writes the `policy_draft_created` audit
/// event. `expected_source_version_id`, when given, must be the current
/// published version. `requested_version` is the new version label; `None`
/// or blank selects `<published version>-draft`. The caller commits.
///
/// # Errors
///
/// - [`PolicyAuthoringError::Forbidden`] when a token actor is no longer
///   authorized.
/// - [`PolicyAuthoringError::NotFound`] when the lineage does not exist.
/// - [`PolicyAuthoringError::Invalid`] (`source_version_changed`) when the
///   expected source is not the current published version.
/// - [`PolicyAuthoringError::NoPublishedSource`] and
///   [`PolicyAuthoringError::MutableDraftExists`] from the derivation rules.
/// - [`PolicyAuthoringError::Internal`] on a database failure.
pub async fn derive_policy_draft_tx(
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthoringActor,
    policy_id: Uuid,
    expected_source_version_id: Option<Uuid>,
    requested_version: Option<&str>,
) -> Result<PolicyDraftRef, PolicyAuthoringError> {
    authorize_token_tx(tx, actor, POLICIES_DRAFT_SCOPE).await?;
    // CONCURRENCY: The lineage row lock serializes derivations and draft
    // updates of this lineage; `ensure_policy_draft` locks the same row again.
    let published = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT current_published_version_id FROM deployment_policies WHERE id = $1 FOR UPDATE",
    )
    .bind(policy_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal(DRAFT_CREATE_FAILED))?
    .ok_or(PolicyAuthoringError::NotFound)?;
    if let (Some(expected), Some(current)) = (expected_source_version_id, published) {
        if expected != current {
            return Err(PolicyAuthoringError::Invalid {
                code: "source_version_changed",
                message: format!(
                    "source_version_id {expected} is not the current published version of \
                     the lineage"
                ),
            });
        }
    }
    let version_id = ensure_policy_draft(
        tx,
        policy_id,
        Some(actor.user_id),
        requested_version,
        PolicyDraftIntent::CreateExplicit,
    )
    .await
    .map_err(|error| {
        let derivation = error
            .downcast_ref::<PolicyDraftDerivationError>()
            .map(|derivation| match derivation {
                PolicyDraftDerivationError::NoPublishedSource => None,
                PolicyDraftDerivationError::MutableDraftExists(id) => Some(*id),
            });
        match derivation {
            Some(None) => PolicyAuthoringError::NoPublishedSource,
            Some(Some(id)) => PolicyAuthoringError::MutableDraftExists(id),
            None => PolicyAuthoringError::Internal {
                context: DRAFT_CREATE_FAILED,
                error,
            },
        }
    })?;
    let (version, derived_from, revision) = sqlx::query_as::<_, (String, Option<Uuid>, i64)>(
        r#"
        SELECT dpv.version,
               COALESCE(dpv.derived_from_version_id, dp.current_published_version_id),
               dpv.draft_revision
        FROM deployment_policy_versions dpv
        JOIN deployment_policies dp ON dp.id = dpv.policy_id
        WHERE dpv.id = $1
        "#,
    )
    .bind(version_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(internal(DRAFT_CREATE_FAILED))?;
    let Some(derived_from_version_id) = derived_from else {
        return Err(PolicyAuthoringError::Internal {
            context: DRAFT_CREATE_FAILED,
            error: anyhow::anyhow!("derived policy draft {version_id} has no published source"),
        });
    };
    record_agent_attribution(tx, version_id, actor)
        .await
        .map_err(internal(DRAFT_CREATE_FAILED))?;
    insert_policy_audit_event(
        tx,
        actor,
        "policy_draft_created",
        &version_id.to_string(),
        &json!({
            "operation": "derive",
            "lineage_id": policy_id,
            "version_id": version_id,
            "version": version,
            "derived_from_version_id": derived_from_version_id,
            "draft_revision": revision,
        }),
    )
    .await
    .map_err(internal(DRAFT_CREATE_FAILED))?;
    Ok(PolicyDraftRef {
        policy_id,
        version_id,
        version,
        draft_revision: revision,
        derived_from_version_id: Some(derived_from_version_id),
    })
}

/// Describes a new policy lineage with its initial draft.
#[derive(Debug, Clone)]
pub struct NewPolicyLineage {
    /// Lineage name; unique, 1 to 255 bytes after trimming.
    pub name: String,
    /// Lineage and draft description.
    pub description: Option<String>,
    /// Default enablement of the draft version. The lineage itself is always
    /// created disabled.
    pub enabled_by_default: bool,
    /// Content of the initial draft version.
    pub definition: PolicyDefinition,
}

/// Creates a new policy lineage with an initial draft for an actor.
///
/// The MCP tool `create_policy_draft` (`mode: new_lineage`) uses this
/// function. It validates the definition, then in one transaction
/// reauthorizes a token actor with `policies:draft`, takes the
/// `policy-name:<name>` advisory lock that the interchange import also takes,
/// rejects a used name, creates the lineage through
/// [`create_deployment_policy_with_mappings_in_tx`] with `enabled = false`,
/// writes the draft's execution phase, compliance metadata and default
/// enablement, records token attribution and writes the
/// `policy_draft_created` audit event. The draft is untrusted.
///
/// # Errors
///
/// - [`PolicyAuthoringError::Invalid`] for an invalid name
///   (`invalid_name`), a used name (`name_conflict`) or an invalid
///   definition (`invalid_definition`).
/// - [`PolicyAuthoringError::ValidatorUnavailable`] when the Nix parser gave
///   no verdict.
/// - [`PolicyAuthoringError::Forbidden`] when a token actor is no longer
///   authorized.
/// - [`PolicyAuthoringError::Internal`] on a database failure.
pub async fn create_policy_lineage_draft(
    pool: &PgPool,
    actor: &AuthoringActor,
    input: &NewPolicyLineage,
) -> Result<PolicyDraftRef, PolicyAuthoringError> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 255 {
        return Err(PolicyAuthoringError::Invalid {
            code: "invalid_name",
            message: "name must be 1 to 255 bytes after trimming".to_owned(),
        });
    }
    let validated = validate_policy_definition(pool, &input.definition)
        .await
        .map_err(definition_error)?;

    let mut tx = pool.begin().await.map_err(internal(DRAFT_CREATE_FAILED))?;
    authorize_token_tx(&mut tx, actor, POLICIES_DRAFT_SCOPE).await?;
    // CONCURRENCY: Same name key as the interchange import, so a concurrent
    // import or create of the same name waits and then sees this lineage.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("policy-name:{name}"))
        .execute(&mut *tx)
        .await
        .map_err(internal(DRAFT_CREATE_FAILED))?;
    let taken: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM deployment_policies WHERE name = $1)")
            .bind(name)
            .fetch_one(&mut *tx)
            .await
            .map_err(internal(DRAFT_CREATE_FAILED))?;
    if taken {
        return Err(PolicyAuthoringError::Invalid {
            code: "name_conflict",
            message: format!("a policy lineage named '{name}' already exists"),
        });
    }
    let request = CreateDeploymentPolicyRequest {
        name: name.to_owned(),
        description: input.description.clone(),
        policy_type: input.definition.policy_type.clone(),
        config: validated.config.clone(),
        // SECURITY: MCP never enables a lineage.
        enabled: Some(false),
        requirement_mappings: input.definition.requirement_mappings.clone(),
        ..CreateDeploymentPolicyRequest::default()
    };
    let policy =
        create_deployment_policy_with_mappings_in_tx(&mut tx, &request, Some(actor.user_id))
            .await
            .map_err(internal(DRAFT_CREATE_FAILED))?;
    let canonical = PolicyVersionCanonical {
        name: policy.name.clone(),
        description: policy.description.clone(),
        policy_type: policy.policy_type.clone(),
        implementation_state: "native".to_owned(),
        execution_phase: validated.execution_phase.clone(),
        config: policy.config.clone(),
        compliance_metadata: input.definition.compliance_metadata.clone(),
        dependencies: json!([]),
        opaque_xml_digest: None,
        enabled_by_default: Some(input.enabled_by_default),
    };
    write_policy_version_digest(&mut tx, policy.id, &canonical)
        .await
        .map_err(internal(DRAFT_CREATE_FAILED))?;
    let (version_id, version) = sqlx::query_as::<_, (Uuid, String)>(
        r#"SELECT dpv.id, dpv.version
           FROM deployment_policies dp
           JOIN deployment_policy_versions dpv ON dpv.id = dp.current_draft_version_id
           WHERE dp.id = $1"#,
    )
    .bind(policy.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal(DRAFT_CREATE_FAILED))?;
    record_agent_attribution(&mut tx, version_id, actor)
        .await
        .map_err(internal(DRAFT_CREATE_FAILED))?;
    let revision = draft_revision(&mut tx, version_id)
        .await
        .map_err(internal(DRAFT_CREATE_FAILED))?;
    insert_policy_audit_event(
        &mut tx,
        actor,
        "policy_draft_created",
        &version_id.to_string(),
        &json!({
            "operation": "new_lineage",
            "lineage_id": policy.id,
            "version_id": version_id,
            "version": version,
            "draft_revision": revision,
        }),
    )
    .await
    .map_err(internal(DRAFT_CREATE_FAILED))?;
    tx.commit().await.map_err(internal(DRAFT_CREATE_FAILED))?;
    Ok(PolicyDraftRef {
        policy_id: policy.id,
        version_id,
        version,
        draft_revision: revision,
        derived_from_version_id: None,
    })
}

// ── Draft update ────────────────────────────────────────────────────────────

/// Holds replacement values for a draft update. `None` keeps the stored
/// value.
#[derive(Debug, Clone, Default)]
pub struct DraftChanges {
    /// New description; a blank text clears it.
    pub description: Option<String>,
    /// New config; validated and normalized.
    pub config: Option<Value>,
    /// New execution phase.
    pub execution_phase: Option<String>,
    /// New default enablement of the version. The lineage `enabled` flag
    /// never changes.
    pub enabled_by_default: Option<bool>,
    /// New compliance metadata object; replaces the stored object.
    pub compliance_metadata: Option<Value>,
    /// New complete set of manual requirement mappings. Imported mappings
    /// stay unchanged.
    pub requirement_mappings: Option<Vec<CreatePolicyRequirementMapping>>,
}

/// Reports the result of [`update_policy_draft`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftUpdateOutcome {
    /// Lineage ID.
    pub policy_id: Uuid,
    /// Exact draft version ID.
    pub version_id: Uuid,
    /// Revision after the update; unchanged for a no-op.
    pub draft_revision: i64,
    /// `false` when the values equal the stored values and nothing was
    /// written.
    pub changed: bool,
    /// Names of the fields that changed.
    pub changed_fields: Vec<&'static str>,
}

/// One manual mapping in comparison order.
type MappingKey = (Uuid, String, String, Option<String>);

fn mapping_keys(mappings: &[CreatePolicyRequirementMapping]) -> Vec<MappingKey> {
    let mut keys = mappings
        .iter()
        .map(|mapping| {
            (
                mapping.requirement_version_id,
                mapping.relationship.clone(),
                mapping.coverage.clone(),
                mapping.rationale.clone(),
            )
        })
        .collect::<Vec<_>>();
    keys.sort();
    keys
}

#[derive(sqlx::FromRow)]
struct DraftRow {
    publication_state: String,
    created_by: Option<Uuid>,
    draft_revision: i64,
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
}

/// Replaces fields of a mutable draft version for an actor at an expected
/// revision.
///
/// The MCP tool `update_policy_draft` uses this function. In one transaction
/// it reauthorizes a token actor with `policies:draft`, locks the lineage row
/// `FOR UPDATE` and the version row `FOR UPDATE`, and then requires, in this
/// order: a mutable version, `created_by = actor.user_id`, the version being
/// the lineage's current draft, and `draft_revision = expected_revision`. It
/// validates the resulting definition. When every value equals the stored
/// value, it writes nothing (no row, no audit event) and returns
/// `changed: false`. Otherwise it writes the version fields and digests,
/// mirrors a changed description or config to the lineage row as the browser
/// editor does (never `name`, `policy_type` or `enabled`), replaces the
/// manual mappings when they changed, and writes the `policy_draft_updated`
/// audit event. The revision trigger increments `draft_revision`.
///
/// # Errors
///
/// - [`PolicyAuthoringError::Forbidden`] when a token actor is no longer
///   authorized, or with [`DRAFT_OWNED_BY_OTHER_USER`] for another user's
///   draft.
/// - [`PolicyAuthoringError::NotFound`] when the version does not exist.
/// - [`PolicyAuthoringError::Invalid`] for an immutable version
///   (`immutable_version`), a version that is not the current draft
///   (`not_current_draft`), an invalid definition (`invalid_definition`) or a
///   mapping that collides with an imported mapping (`mapping_conflict`).
/// - [`PolicyAuthoringError::StaleRevision`] when the revision changed.
/// - [`PolicyAuthoringError::ValidatorUnavailable`] when the Nix parser gave
///   no verdict.
/// - [`PolicyAuthoringError::Internal`] on a database failure.
pub async fn update_policy_draft(
    pool: &PgPool,
    actor: &AuthoringActor,
    version_id: Uuid,
    expected_revision: i64,
    changes: &DraftChanges,
) -> Result<DraftUpdateOutcome, PolicyAuthoringError> {
    let mut tx = pool.begin().await.map_err(internal(DRAFT_UPDATE_FAILED))?;
    authorize_token_tx(&mut tx, actor, POLICIES_DRAFT_SCOPE).await?;
    let policy_id: Uuid =
        sqlx::query_scalar("SELECT policy_id FROM deployment_policy_versions WHERE id = $1")
            .bind(version_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal(DRAFT_UPDATE_FAILED))?
            .ok_or(PolicyAuthoringError::NotFound)?;
    // CONCURRENCY: Lineage row, then version row, both `FOR UPDATE`: the
    // order of `ensure_policy_draft` and `write_policy_version_digest`. A
    // concurrent browser edit or derivation of this lineage waits, and its
    // change then fails the revision check of the next MCP update.
    let current_draft = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1 FOR UPDATE",
    )
    .bind(policy_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal(DRAFT_UPDATE_FAILED))?;
    let row = sqlx::query_as::<_, DraftRow>(
        r#"SELECT publication_state, created_by, draft_revision, name, description,
                  policy_type, implementation_state, execution_phase, config,
                  compliance_metadata, dependencies, opaque_xml, enabled_by_default
           FROM deployment_policy_versions WHERE id = $1 FOR UPDATE"#,
    )
    .bind(version_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal(DRAFT_UPDATE_FAILED))?;
    if !is_mutable_state(&row.publication_state) {
        return Err(PolicyAuthoringError::Invalid {
            code: "immutable_version",
            message: format!(
                "version {version_id} is {} and immutable; derive a new draft with \
                 create_policy_draft",
                row.publication_state
            ),
        });
    }
    if row.created_by != Some(actor.user_id) {
        return Err(PolicyAuthoringError::Forbidden(
            DRAFT_OWNED_BY_OTHER_USER.to_owned(),
        ));
    }
    if current_draft != Some(version_id) {
        return Err(PolicyAuthoringError::Invalid {
            code: "not_current_draft",
            message: format!("version {version_id} is not the current draft of its lineage"),
        });
    }
    if row.draft_revision != expected_revision {
        return Err(PolicyAuthoringError::StaleRevision {
            current: row.draft_revision,
        });
    }

    let description = match &changes.description {
        Some(text) if text.trim().is_empty() => None,
        Some(text) => Some(text.clone()),
        None => row.description.clone(),
    };
    let execution_phase = changes
        .execution_phase
        .clone()
        .unwrap_or_else(|| row.execution_phase.clone());
    let stored_enabled = row.enabled_by_default.unwrap_or(true);
    let enabled = changes.enabled_by_default.unwrap_or(stored_enabled);
    let metadata = changes
        .compliance_metadata
        .clone()
        .unwrap_or_else(|| row.compliance_metadata.clone());
    // The stored config and mappings are validated with the changes, so the
    // draft is valid after the update. The stored config keeps its stored
    // form unless the caller replaces it.
    let definition = PolicyDefinition {
        policy_type: row.policy_type.clone(),
        config: changes.config.clone().unwrap_or_else(|| row.config.clone()),
        execution_phase: Some(execution_phase.clone()),
        compliance_metadata: metadata.clone(),
        requirement_mappings: changes.requirement_mappings.clone().unwrap_or_default(),
    };
    let validated = validate_policy_definition(&mut *tx, &definition)
        .await
        .map_err(definition_error)?;
    let config = if changes.config.is_some() {
        validated.config
    } else {
        row.config.clone()
    };

    let mut current_mappings = sqlx::query_as::<_, MappingKey>(
        r#"SELECT requirement_version_id, relationship, coverage, rationale
           FROM policy_requirement_mappings
           WHERE policy_version_id = $1 AND provenance = 'manual'"#,
    )
    .bind(version_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal(DRAFT_UPDATE_FAILED))?;
    current_mappings.sort();
    let desired_mappings = changes.requirement_mappings.as_deref().map(mapping_keys);

    // INVARIANT: No-op detection compares every business field before any
    // UPDATE, so a no-op writes no row and keeps the revision.
    let mut changed_fields = Vec::new();
    if description != row.description {
        changed_fields.push("description");
    }
    if config != row.config {
        changed_fields.push("config");
    }
    if execution_phase != row.execution_phase {
        changed_fields.push("execution_phase");
    }
    if enabled != stored_enabled {
        changed_fields.push("enabled_by_default");
    }
    if metadata != row.compliance_metadata {
        changed_fields.push("compliance_metadata");
    }
    let mappings_changed = desired_mappings
        .as_ref()
        .is_some_and(|desired| *desired != current_mappings);
    if mappings_changed {
        changed_fields.push("requirement_mappings");
    }
    if changed_fields.is_empty() {
        return Ok(DraftUpdateOutcome {
            policy_id,
            version_id,
            draft_revision: row.draft_revision,
            changed: false,
            changed_fields,
        });
    }

    let fields_changed = changed_fields
        .iter()
        .any(|field| *field != "requirement_mappings");
    if changed_fields.contains(&"description") || changed_fields.contains(&"config") {
        // COMPATIBILITY: The browser editor reads the lineage row and writes
        // it back through `sync_policy_draft_version`. Mirroring keeps the
        // agent's description and config when an Admin later edits the draft
        // in the browser. `enabled` is not touched, so the lineage is never
        // enabled here; `write_policy_version_digest` below restores the
        // draft's own `enabled_by_default` after the sync trigger.
        sqlx::query("UPDATE deployment_policies SET description = $2, config = $3 WHERE id = $1")
            .bind(policy_id)
            .bind(&description)
            .bind(&config)
            .execute(&mut *tx)
            .await
            .map_err(internal(DRAFT_UPDATE_FAILED))?;
    }
    if fields_changed {
        let canonical = PolicyVersionCanonical {
            name: row.name.clone(),
            description: description.clone(),
            policy_type: row.policy_type.clone(),
            implementation_state: row.implementation_state.clone(),
            execution_phase: execution_phase.clone(),
            config: config.clone(),
            compliance_metadata: metadata.clone(),
            dependencies: row.dependencies.clone(),
            opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(row.opaque_xml.as_deref()),
            enabled_by_default: Some(enabled),
        };
        write_policy_version_digest(&mut tx, policy_id, &canonical)
            .await
            .map_err(internal(DRAFT_UPDATE_FAILED))?;
    }

    if let (true, Some(desired)) = (mappings_changed, changes.requirement_mappings.as_ref()) {
        let requested = desired
            .iter()
            .map(|mapping| mapping.requirement_version_id)
            .collect::<Vec<_>>();
        let imported: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT requirement_version_id FROM policy_requirement_mappings
               WHERE policy_version_id = $1 AND provenance <> 'manual'
                 AND requirement_version_id = ANY($2)
               ORDER BY requirement_version_id LIMIT 1"#,
        )
        .bind(version_id)
        .bind(&requested)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal(DRAFT_UPDATE_FAILED))?;
        if let Some(requirement_version_id) = imported {
            return Err(PolicyAuthoringError::Invalid {
                code: "mapping_conflict",
                message: format!(
                    "requirement version {requirement_version_id} already has an imported \
                     mapping on this version; imported mappings are read-only"
                ),
            });
        }
        sqlx::query(
            "DELETE FROM policy_requirement_mappings WHERE policy_version_id = $1 AND provenance = 'manual'",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .map_err(internal(DRAFT_UPDATE_FAILED))?;
        for mapping in desired {
            insert_policy_mapping_in_tx(
                &mut tx,
                version_id,
                mapping.requirement_version_id,
                &mapping.relationship,
                &mapping.coverage,
                mapping.rationale.as_deref(),
                "manual",
                None,
                actor.user_id,
            )
            .await
            .map_err(internal(DRAFT_UPDATE_FAILED))?;
        }
        refresh_policy_mapping_digest(&mut tx, version_id)
            .await
            .map_err(internal(DRAFT_UPDATE_FAILED))?;
    }

    let revision = draft_revision(&mut tx, version_id)
        .await
        .map_err(internal(DRAFT_UPDATE_FAILED))?;
    insert_policy_audit_event(
        &mut tx,
        actor,
        "policy_draft_updated",
        &version_id.to_string(),
        &json!({
            "operation": "update",
            "lineage_id": policy_id,
            "version_id": version_id,
            "previous_draft_revision": row.draft_revision,
            "draft_revision": revision,
            "changed_fields": changed_fields,
        }),
    )
    .await
    .map_err(internal(DRAFT_UPDATE_FAILED))?;
    tx.commit().await.map_err(internal(DRAFT_UPDATE_FAILED))?;
    Ok(DraftUpdateOutcome {
        policy_id,
        version_id,
        draft_revision: revision,
        changed: true,
        changed_fields,
    })
}

// ── Policy interchange preview and import ───────────────────────────────────

/// Describes the reconciliation of one imported policy version in a preview.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyReconciliationPreview {
    lineage_id: Uuid,
    version_id: Uuid,
    version: String,
    name: String,
    policy_type: String,
    semantic_digest: String,
    /// `exact_match`, `new_lineage`, `new_version` or `identity_conflict`.
    reconciliation_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_lineage_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_version_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    local_semantic_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name_collision: Option<NameCollision>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocking_conflicts: Vec<ConflictInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct NameCollision {
    local_policy_id: Uuid,
    local_policy_name: String,
}

#[derive(Debug, Clone, Serialize)]
struct ConflictInfo {
    code: String,
    #[serde(flatten)]
    details: Value,
}

/// Reports the read-only analysis of a policy interchange document.
///
/// The browser preview response and the MCP `preview_policy_import` result
/// serialize these fields unchanged.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyInterchangePreview {
    /// SHA-256 of the document bytes; the import must present it.
    pub source_sha256: String,
    /// Number of policy versions in the document.
    pub policy_count: usize,
    /// Reconciliation of every imported version, ordered by lineage and
    /// version ID.
    pub policies: Vec<PolicyReconciliationPreview>,
    /// Reports whether an identity conflict or a name collision blocks the
    /// import.
    pub has_blocking_conflicts: bool,
    /// Blocking identity conflicts and name collisions.
    pub blocking_conflicts: Vec<Value>,
}

/// Reports the result of one imported policy version.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyImportOutcome {
    /// Lineage ID of the version.
    pub lineage_id: Uuid,
    /// Exact version ID.
    pub version_id: Uuid,
    /// `exact_match`, `new_lineage` or `new_version`.
    pub reconciliation_action: &'static str,
    /// Reports whether the import created the version.
    pub created: bool,
    /// Publication state after the import.
    pub publication_state: String,
    /// Trust state after the import.
    pub trust_state: String,
    /// Lineage `enabled` flag after the import.
    pub enabled: bool,
    /// Revision of a created version; `None` for a reused version.
    pub draft_revision: Option<i64>,
}

/// Reports the result of a committed policy interchange import.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyImportResult {
    /// SHA-256 of the imported document bytes.
    pub source_sha256: String,
    /// Number of created versions.
    pub created: u32,
    /// Number of reused existing versions.
    pub reused: u32,
    /// One outcome per imported version, in reconciliation order.
    pub policies: Vec<PolicyImportOutcome>,
}

/// Returns whether a reconciliation conflict blocks an import.
fn reconciliation_conflict_is_blocking(conflict: &ReconcileConflict) -> bool {
    matches!(
        conflict,
        ReconcileConflict::VersionDigestMismatch { .. }
            | ReconcileConflict::VersionBelongsToDifferentLineage { .. }
            | ReconcileConflict::PolicyTypeMismatch { .. }
            | ReconcileConflict::LineageObjectTypeMismatch { .. }
            | ReconcileConflict::InvalidPortableIdentity { .. }
    )
}

/// Converts a reconciliation conflict to its response form.
fn conflict_to_info(conflict: &ReconcileConflict) -> ConflictInfo {
    let (code, details) = match conflict {
        ReconcileConflict::VersionDigestMismatch {
            lineage_id,
            version_id,
            local_digest,
            imported_digest,
            ..
        } => (
            "INTERCHANGE_VERSION_DIGEST_CONFLICT".to_string(),
            json!({
                "lineage_id": lineage_id,
                "version_id": version_id,
                "local_digest": local_digest,
                "imported_digest": imported_digest,
            }),
        ),
        ReconcileConflict::VersionBelongsToDifferentLineage {
            lineage_id,
            version_id,
            actual_lineage_id,
        } => (
            "INTERCHANGE_VERSION_LINEAGE_MISMATCH".to_string(),
            json!({
                "imported_lineage_id": lineage_id,
                "version_id": version_id,
                "actual_lineage_id": actual_lineage_id,
            }),
        ),
        ReconcileConflict::PolicyTypeMismatch {
            lineage_id,
            version_id,
        } => (
            "INTERCHANGE_POLICY_TYPE_MISMATCH".to_string(),
            json!({
                "lineage_id": lineage_id,
                "version_id": version_id,
            }),
        ),
        ReconcileConflict::LineageObjectTypeMismatch { lineage_id } => (
            "INTERCHANGE_LINEAGE_TYPE_MISMATCH".to_string(),
            json!({
                "lineage_id": lineage_id,
            }),
        ),
        ReconcileConflict::InvalidPortableIdentity { source_rule_id } => (
            "INTERCHANGE_INVALID_PORTABLE_ID".to_string(),
            json!({
                "source_rule_id": source_rule_id,
            }),
        ),
    };
    ConflictInfo { code, details }
}

fn conflict_value(conflict: &ReconcileConflict) -> Value {
    serde_json::to_value(conflict_to_info(conflict)).unwrap_or_else(|_| json!({}))
}

/// Validates normalized interchange policies with the server validators:
/// each config (including the bounded Nix syntax parse), the composite
/// execution phase, and unique version IDs.
///
/// # Errors
///
/// Returns the first problem as a message. A message for which
/// `is_syntax_validation_unavailable` is true means the parser gave no
/// verdict.
pub async fn validate_policy_interchange_document_async(
    policies: &[NormalizedPolicyImport],
) -> Result<(), String> {
    let mut seen_versions = HashSet::new();
    for policy in policies {
        // INVARIANT: `deployment_policy_versions.enabled_by_default` is NOT
        // NULL and part of the semantic digest. A default chosen here would
        // store a digest computed without the field, so a document must state
        // the value. Without this check the insert failed with a database
        // error and HTTP 500.
        if policy.enabled_by_default.is_none() {
            return Err(format!(
                "policy version {} must set enabled_by_default to true or false",
                policy.version_id
            ));
        }
        let composite =
            validate_policy_type_config_async(&policy.policy_type, &policy.config).await?;
        if composite.is_some() && policy.execution_phase != "multi-phase" {
            return Err(format!(
                "composite policy version {} must use execution_phase multi-phase",
                policy.version_id
            ));
        }
        if !seen_versions.insert(policy.version_id) {
            return Err(format!(
                "Duplicate version ID {} in import document",
                policy.version_id
            ));
        }
    }
    Ok(())
}

fn native_identities(imported: &[NormalizedPolicyImport]) -> Vec<NativePolicyIdentity> {
    imported
        .iter()
        .map(|policy| NativePolicyIdentity {
            lineage_id: policy.lineage_id,
            version_id: policy.version_id,
            policy_type: policy.policy_type.clone(),
            semantic_digest: policy.semantic_digest.clone(),
            // The name identifies the policy in conflict reports.
            source_rule_id: policy.name.clone(),
        })
        .collect()
}

fn existing_identities(rows: &[(Uuid, Uuid, String, String)]) -> Vec<ExistingPolicyIdentity> {
    rows.iter()
        .map(
            |(version_id, lineage_id, digest, policy_type)| ExistingPolicyIdentity {
                lineage_id: *lineage_id,
                version_id: *version_id,
                policy_type: policy_type.clone(),
                semantic_digest: digest.clone(),
            },
        )
        .collect()
}

const MATCHING_VERSIONS_SQL: &str = "SELECT id, policy_id, semantic_digest, policy_type \
FROM deployment_policy_versions WHERE id = ANY($1) OR policy_id = ANY($2)";

/// Loads the local identities that match imported IDs and runs the
/// authoritative planner inside the import transaction, after its locks.
async fn load_and_plan_policy_reconciliation_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    imported: &[NormalizedPolicyImport],
) -> Result<PolicyReconciliationPlan, sqlx::Error> {
    let lineage_ids: Vec<Uuid> = imported.iter().map(|policy| policy.lineage_id).collect();
    let version_ids: Vec<Uuid> = imported.iter().map(|policy| policy.version_id).collect();
    let matching: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(MATCHING_VERSIONS_SQL)
        .bind(&version_ids)
        .bind(&lineage_ids)
        .fetch_all(&mut **tx)
        .await?;
    Ok(plan_policy_reconciliation(
        &native_identities(imported),
        &existing_identities(&matching),
    ))
}

/// Loads local identities and policy names and builds the preview rows with
/// the authoritative planner. Performs no mutation.
async fn load_and_plan_policy_reconciliation(
    pool: &PgPool,
    imported: &[NormalizedPolicyImport],
) -> Result<(Vec<PolicyReconciliationPreview>, Vec<ReconcileConflict>), sqlx::Error> {
    let lineage_ids: Vec<Uuid> = imported.iter().map(|policy| policy.lineage_id).collect();
    let version_ids: Vec<Uuid> = imported.iter().map(|policy| policy.version_id).collect();
    let names: Vec<String> = imported.iter().map(|policy| policy.name.clone()).collect();

    // Versions that match imported IDs or belong to imported lineages; they
    // distinguish new_version from new_lineage.
    let matching: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(MATCHING_VERSIONS_SQL)
        .bind(&version_ids)
        .bind(&lineage_ids)
        .fetch_all(pool)
        .await?;
    let policies_by_name: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT name, id FROM deployment_policies WHERE name = ANY($1)")
            .bind(&names)
            .fetch_all(pool)
            .await?;

    let version_to_lineage: HashMap<Uuid, (Uuid, String)> = matching
        .iter()
        .map(|(version_id, lineage_id, digest, _)| (*version_id, (*lineage_id, digest.clone())))
        .collect();
    let name_to_lineage: HashMap<String, Uuid> = policies_by_name.into_iter().collect();
    let plan = plan_policy_reconciliation(
        &native_identities(imported),
        &existing_identities(&matching),
    );
    let name_collision = |name: &str, lineage_id: Uuid| {
        name_to_lineage
            .get(name)
            .filter(|local| **local != lineage_id)
            .map(|local| NameCollision {
                local_policy_id: *local,
                local_policy_name: name.to_owned(),
            })
    };

    let mut preview_map: HashMap<(Uuid, Uuid), PolicyReconciliationPreview> = HashMap::new();
    for (identity, decision) in &plan.decisions {
        let (state, local_lineage_id, local_version_id) = match decision {
            ReconcileDecision::ReuseExact {
                local_lineage_id,
                local_version_id,
            } => (
                "exact_match",
                Some(*local_lineage_id),
                Some(*local_version_id),
            ),
            ReconcileDecision::CreateLineageAndVersion { .. } => ("new_lineage", None, None),
            ReconcileDecision::CreateVersionInExistingLineage {
                local_lineage_id, ..
            } => ("new_version", Some(*local_lineage_id), None),
        };
        let local_semantic_digest = local_version_id.and_then(|local| {
            matching
                .iter()
                .find(|(version_id, _, _, _)| *version_id == local)
                .map(|(_, _, digest, _)| digest.clone())
        });
        let preview = PolicyReconciliationPreview {
            lineage_id: identity.lineage_id,
            version_id: identity.version_id,
            version: imported
                .iter()
                .find(|policy| policy.version_id == identity.version_id)
                .map(|policy| policy.version.clone())
                .unwrap_or_else(|| "0.1.0".to_string()),
            name: identity.source_rule_id.clone(),
            policy_type: identity.policy_type.clone(),
            semantic_digest: identity.semantic_digest.clone(),
            reconciliation_state: state.to_string(),
            local_lineage_id,
            local_version_id,
            local_semantic_digest,
            name_collision: name_collision(identity.source_rule_id.as_str(), identity.lineage_id),
            blocking_conflicts: Vec::new(),
        };
        preview_map.insert((preview.lineage_id, preview.version_id), preview);
    }

    // Identity conflicts have no decision; they map to a version preview when
    // the conflict names one.
    for conflict in &plan.conflicts {
        let (lineage_id, version_id) = match conflict {
            ReconcileConflict::VersionDigestMismatch {
                lineage_id,
                version_id,
                ..
            }
            | ReconcileConflict::VersionBelongsToDifferentLineage {
                lineage_id,
                version_id,
                ..
            }
            | ReconcileConflict::PolicyTypeMismatch {
                lineage_id,
                version_id,
            } => (*lineage_id, *version_id),
            ReconcileConflict::LineageObjectTypeMismatch { .. }
            | ReconcileConflict::InvalidPortableIdentity { .. } => continue,
        };
        let Some(policy) = imported
            .iter()
            .find(|policy| policy.lineage_id == lineage_id && policy.version_id == version_id)
        else {
            continue;
        };
        let local = version_to_lineage.get(&version_id);
        let preview = PolicyReconciliationPreview {
            lineage_id: policy.lineage_id,
            version_id: policy.version_id,
            version: policy.version.clone(),
            name: policy.name.clone(),
            policy_type: policy.policy_type.clone(),
            semantic_digest: policy.semantic_digest.clone(),
            reconciliation_state: "identity_conflict".to_string(),
            local_lineage_id: local.map(|(lineage, _)| *lineage),
            local_version_id: local.map(|_| version_id),
            local_semantic_digest: local.map(|(_, digest)| digest.clone()),
            name_collision: name_collision(policy.name.as_str(), policy.lineage_id),
            blocking_conflicts: vec![conflict_to_info(conflict)],
        };
        preview_map.insert((lineage_id, version_id), preview);
    }

    let mut previews: Vec<PolicyReconciliationPreview> = preview_map.into_values().collect();
    previews.sort_by(|a, b| {
        a.lineage_id
            .cmp(&b.lineage_id)
            .then(a.version_id.cmp(&b.version_id))
    });
    Ok((previews, plan.conflicts))
}

/// Parses, validates and reconciles a policy interchange document without
/// writing.
///
/// The browser route `POST /api/v1/policies/interchange/preview` and the MCP
/// tool `preview_policy_import` share this function. `filename` selects the
/// format by its extension (`.json` or `.toml`; JSON without a filename).
/// Policies without portable IDs receive the deterministic compatibility IDs
/// derived from the document digest, the same IDs that the import uses. The
/// function reads matching versions and lineage names and performs no write,
/// Nix evaluation, scan or deployment work.
///
/// # Errors
///
/// - [`PolicyAuthoringError::Invalid`] (`POLICY_INTERCHANGE_INVALID`) when the
///   document does not parse or validate.
/// - [`PolicyAuthoringError::Internal`] when the database read fails.
pub async fn preview_policy_interchange(
    pool: &PgPool,
    bytes: &[u8],
    filename: Option<&str>,
) -> Result<PolicyInterchangePreview, PolicyAuthoringError> {
    let source_sha256 = source_sha256(bytes);
    let invalid = |message: String| PolicyAuthoringError::Invalid {
        code: "POLICY_INTERCHANGE_INVALID",
        message,
    };
    let policies = parse_policy_document(bytes, filename, Some(&source_sha256)).map_err(invalid)?;
    validate_policy_interchange_document_async(&policies)
        .await
        .map_err(invalid)?;
    let (previews, conflicts) = load_and_plan_policy_reconciliation(pool, &policies)
        .await
        .map_err(internal(PREVIEW_FAILED))?;

    let mut blocking_conflicts: Vec<Value> = conflicts
        .iter()
        .filter(|conflict| reconciliation_conflict_is_blocking(conflict))
        .map(conflict_value)
        .collect();
    for preview in &previews {
        if let Some(collision) = &preview.name_collision {
            blocking_conflicts.push(json!({
                "code": "POLICY_INTERCHANGE_NAME_COLLISION",
                "imported_lineage_id": preview.lineage_id,
                "imported_name": preview.name,
                "local_lineage_id": collision.local_policy_id,
            }));
        }
    }
    let has_blocking_conflicts = conflicts.iter().any(reconciliation_conflict_is_blocking)
        || previews
            .iter()
            .any(|preview| preview.name_collision.is_some());
    Ok(PolicyInterchangePreview {
        source_sha256,
        policy_count: policies.len(),
        policies: previews,
        has_blocking_conflicts,
        blocking_conflicts,
    })
}

const VERSION_INSERT_SQL: &str = r#"
INSERT INTO deployment_policy_versions (
    id, policy_id, version, publication_state, name, description, policy_type,
    implementation_state, execution_phase, config, compliance_metadata, dependencies,
    opaque_xml, enabled_by_default, semantic_digest, created_by,
    created_via_agent, created_access_token_id, created_access_token_name
) VALUES (
    $1, $2, $3, 'draft', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
    $16::uuid IS NOT NULL, $16::uuid, (SELECT name FROM api_access_tokens WHERE id = $16::uuid)
)
RETURNING draft_revision
"#;

/// Inserts one imported version as an untrusted draft and returns its
/// revision. The trust state takes the column default `untrusted`.
async fn insert_imported_version(
    tx: &mut Transaction<'_, Postgres>,
    actor: &AuthoringActor,
    version_id: Uuid,
    lineage_id: Uuid,
    policy: &NormalizedPolicyImport,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(VERSION_INSERT_SQL)
        .bind(version_id)
        .bind(lineage_id)
        .bind(&policy.version)
        .bind(&policy.name)
        .bind(&policy.description)
        .bind(&policy.policy_type)
        .bind(&policy.implementation_state)
        .bind(&policy.execution_phase)
        .bind(&policy.config)
        .bind(&policy.compliance_metadata)
        .bind(&policy.dependencies)
        .bind(&policy.opaque_xml)
        .bind(policy.enabled_by_default)
        .bind(&policy.semantic_digest)
        .bind(actor.user_id)
        .bind(actor.access_token_id)
        .fetch_one(&mut **tx)
        .await
}

/// Imports a policy interchange document as untrusted drafts, all or nothing.
///
/// The browser route `POST /api/v1/policies/interchange/import` and the MCP
/// tool `import_policy_draft` share this function. The document digest must
/// equal `expected_source_sha256` (case-insensitive), the digest that the
/// preview returned. The function parses and validates the document again,
/// and then in one transaction: reauthorizes a token actor with
/// `policies:import`; takes the sorted `policy-lineage:`, `policy-version:`
/// and `policy-name:` advisory locks of every imported policy; plans the
/// reconciliation again under those locks; rejects any conflict and any name
/// collision of a new lineage; reuses exact matches without a change; creates
/// new lineages (disabled) and new versions as untrusted `draft` versions
/// with `created_by = actor.user_id` and token attribution; and writes one
/// `policy_interchange_imported` audit event per imported version with the
/// source digest, never the document. It never changes an existing version.
///
/// CONCURRENCY: Two imports of the same document serialize on the advisory
/// locks. The second one plans after the first commits, sees the created
/// versions, and reuses them, so one lineage results.
///
/// # Errors
///
/// - [`PolicyAuthoringError::SourceDigestMismatch`] when the digest differs.
/// - [`PolicyAuthoringError::Invalid`] (`POLICY_INTERCHANGE_INVALID`) when the
///   document does not parse or validate.
/// - [`PolicyAuthoringError::Forbidden`] when a token actor is no longer
///   authorized.
/// - [`PolicyAuthoringError::InterchangeConflicts`] for identity conflicts
///   and name collisions.
/// - [`PolicyAuthoringError::Internal`] on a database failure, with the
///   browser message of the failed step.
pub async fn import_policy_interchange(
    pool: &PgPool,
    actor: &AuthoringActor,
    bytes: &[u8],
    filename: Option<&str>,
    expected_source_sha256: &str,
) -> Result<PolicyImportResult, PolicyAuthoringError> {
    let actual_source_sha256 = source_sha256(bytes);
    if !expected_source_sha256.eq_ignore_ascii_case(&actual_source_sha256) {
        return Err(PolicyAuthoringError::SourceDigestMismatch {
            expected: expected_source_sha256.to_owned(),
            actual: actual_source_sha256,
        });
    }
    let invalid = |message: String| PolicyAuthoringError::Invalid {
        code: "POLICY_INTERCHANGE_INVALID",
        message,
    };
    // Source-aware deterministic parsing matches the preview identities.
    let policies =
        parse_policy_document(bytes, filename, Some(&actual_source_sha256)).map_err(invalid)?;
    validate_policy_interchange_document_async(&policies)
        .await
        .map_err(invalid)?;

    let mut tx = pool.begin().await.map_err(internal(IMPORT_FAILED))?;
    authorize_token_tx(&mut tx, actor, POLICIES_IMPORT_SCOPE).await?;
    // CONCURRENCY: Sorted advisory locks on every imported identity and on
    // the exact (case-sensitive) name, which protects the UNIQUE name
    // constraint. `create_policy_lineage_draft` takes the same name key.
    let mut lock_keys: Vec<String> = Vec::new();
    for policy in &policies {
        lock_keys.push(format!("policy-lineage:{}", policy.lineage_id));
        lock_keys.push(format!("policy-version:{}", policy.version_id));
        lock_keys.push(format!("policy-name:{}", policy.name));
    }
    lock_keys.sort();
    lock_keys.dedup();
    for lock_key in &lock_keys {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *tx)
            .await
            .map_err(internal("Failed to acquire import locks"))?;
    }

    let plan = load_and_plan_policy_reconciliation_in_tx(&mut tx, &policies)
        .await
        .map_err(internal("Failed to plan policy reconciliation"))?;
    if !plan.conflicts.is_empty() {
        return Err(PolicyAuthoringError::InterchangeConflicts(
            plan.conflicts
                .iter()
                .filter(|conflict| reconciliation_conflict_is_blocking(conflict))
                .map(conflict_value)
                .collect(),
        ));
    }

    let imported_policy = |version_id: Uuid| {
        policies
            .iter()
            .find(|policy| policy.version_id == version_id)
            .ok_or_else(|| PolicyAuthoringError::Internal {
                context: IMPORT_FAILED,
                error: anyhow::anyhow!(
                    "planned policy version {version_id} is not in the document"
                ),
            })
    };

    // A new lineage must not take a name that another lineage holds; the
    // UNIQUE constraint would otherwise fail the insert.
    for (identity, decision) in &plan.decisions {
        if let ReconcileDecision::CreateLineageAndVersion {
            portable_lineage_id,
            ..
        } = decision
        {
            let policy = imported_policy(identity.version_id)?;
            let collision: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM deployment_policies WHERE name = $1 AND id != $2",
            )
            .bind(&policy.name)
            .bind(portable_lineage_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal(IMPORT_FAILED))?;
            if let Some(local_lineage_id) = collision {
                return Err(PolicyAuthoringError::InterchangeConflicts(vec![json!({
                    "code": "POLICY_INTERCHANGE_NAME_COLLISION",
                    "imported_lineage_id": portable_lineage_id,
                    "imported_name": policy.name,
                    "local_lineage_id": local_lineage_id,
                })]));
            }
        }
    }

    let mut reused = 0u32;
    let mut created = 0u32;
    let mut outcomes: Vec<PolicyImportOutcome> = Vec::new();
    for (identity, decision) in &plan.decisions {
        match decision {
            ReconcileDecision::ReuseExact {
                local_lineage_id,
                local_version_id,
            } => {
                // An exact match is reported with its actual state and is
                // never changed.
                let (publication_state, trust_state, enabled): (String, String, bool) =
                    sqlx::query_as(
                        r#"SELECT dpv.publication_state, COALESCE(dpv.trust_state, 'untrusted'),
                                  dp.enabled
                           FROM deployment_policy_versions dpv
                           JOIN deployment_policies dp ON dpv.policy_id = dp.id
                           WHERE dpv.id = $1"#,
                    )
                    .bind(local_version_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(internal(IMPORT_FAILED))?;
                outcomes.push(PolicyImportOutcome {
                    lineage_id: *local_lineage_id,
                    version_id: *local_version_id,
                    reconciliation_action: "exact_match",
                    created: false,
                    publication_state,
                    trust_state,
                    enabled,
                    draft_revision: None,
                });
                reused += 1;
            }
            ReconcileDecision::CreateLineageAndVersion {
                portable_lineage_id,
                portable_version_id,
            } => {
                let policy = imported_policy(identity.version_id)?;
                // SECURITY: An imported lineage is always disabled.
                sqlx::query(
                    "INSERT INTO deployment_policies (id, name, description, policy_type, config, enabled) VALUES ($1, $2, $3, $4, $5, false)",
                )
                .bind(portable_lineage_id)
                .bind(&policy.name)
                .bind(&policy.description)
                .bind(&policy.policy_type)
                .bind(&policy.config)
                .execute(&mut *tx)
                .await
                .map_err(internal(IMPORT_FAILED))?;
                // The lineage trigger created a synthetic draft; the exact
                // portable version replaces it.
                let generated = sqlx::query_scalar::<_, Option<Uuid>>(
                    "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
                )
                .bind(portable_lineage_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(internal(IMPORT_FAILED))?;
                if let Some(generated) = generated {
                    sqlx::query(
                        "UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1",
                    )
                    .bind(portable_lineage_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(internal(IMPORT_FAILED))?;
                    sqlx::query("DELETE FROM deployment_policy_versions WHERE id = $1")
                        .bind(generated)
                        .execute(&mut *tx)
                        .await
                        .map_err(internal(IMPORT_FAILED))?;
                }
                let revision = insert_imported_version(
                    &mut tx,
                    actor,
                    *portable_version_id,
                    *portable_lineage_id,
                    policy,
                )
                .await
                .map_err(internal(IMPORT_FAILED))?;
                sqlx::query(
                    "UPDATE deployment_policies SET current_draft_version_id = $1 WHERE id = $2",
                )
                .bind(portable_version_id)
                .bind(portable_lineage_id)
                .execute(&mut *tx)
                .await
                .map_err(internal(IMPORT_FAILED))?;
                outcomes.push(PolicyImportOutcome {
                    lineage_id: *portable_lineage_id,
                    version_id: *portable_version_id,
                    reconciliation_action: "new_lineage",
                    created: true,
                    publication_state: "draft".to_string(),
                    trust_state: "untrusted".to_string(),
                    enabled: false,
                    draft_revision: Some(revision),
                });
                created += 1;
            }
            ReconcileDecision::CreateVersionInExistingLineage {
                local_lineage_id,
                portable_version_id,
            } => {
                let policy = imported_policy(identity.version_id)?;
                let enabled: bool =
                    sqlx::query_scalar("SELECT enabled FROM deployment_policies WHERE id = $1")
                        .bind(local_lineage_id)
                        .fetch_one(&mut *tx)
                        .await
                        .map_err(internal(IMPORT_FAILED))?;
                let revision = insert_imported_version(
                    &mut tx,
                    actor,
                    *portable_version_id,
                    *local_lineage_id,
                    policy,
                )
                .await
                .map_err(internal(IMPORT_FAILED))?;
                outcomes.push(PolicyImportOutcome {
                    lineage_id: *local_lineage_id,
                    version_id: *portable_version_id,
                    reconciliation_action: "new_version",
                    created: true,
                    publication_state: "draft".to_string(),
                    trust_state: "untrusted".to_string(),
                    enabled,
                    draft_revision: Some(revision),
                });
                created += 1;
            }
        }
    }

    for outcome in &outcomes {
        insert_policy_audit_event(
            &mut tx,
            actor,
            "policy_interchange_imported",
            &outcome.version_id.to_string(),
            &json!({
                "lineage_id": outcome.lineage_id,
                "version_id": outcome.version_id,
                "source_digest": actual_source_sha256,
                "reconciliation_action": outcome.reconciliation_action,
                "created": outcome.created,
                "final_publication_state": outcome.publication_state,
                "final_trust_state": outcome.trust_state,
                "final_enabled": outcome.enabled,
                "draft_revision": outcome.draft_revision,
            }),
        )
        .await
        .map_err(internal("Failed to write import audit events"))?;
    }
    tx.commit().await.map_err(internal(IMPORT_FAILED))?;
    Ok(PolicyImportResult {
        source_sha256: actual_source_sha256,
        created,
        reused,
        policies: outcomes,
    })
}
