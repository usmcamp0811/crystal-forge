//! Defines the MCP tools and authorizes every call.
//!
//! Four tools read; three tools write (see [`is_write_tool`]). The write tools
//! live in `scan_tools` and `poam_write`.
//!
//! The transport layer authenticates the bearer token and builds a
//! [`ToolCaller`]. This module never trusts a cached tool list: [`call_tool`]
//! checks the tool's scopes again and computes the effective environments on
//! every call as the token grant intersected with the owner's current
//! environments (Admin owners skip only the membership side), restricted to
//! active environments. An empty effective set grants nothing.
//!
//! Results never include systems, findings, or POA&Ms outside the effective
//! environments. A hidden system or POA&M returns [`ToolError::NotFound`], the
//! same result as a nonexistent one. List results carry no totals or global
//! counts. Collections default to 25 items and accept at most 100.
//!
//! Text from CVE data and user-entered POA&M text is untrusted LLM input. CVE
//! descriptions have control characters removed and are truncated; other
//! fields are returned as stored and are JSON-escaped by the transport.
//! Responses never include flake repository URLs, which can embed credentials.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::SystemCveInventoryParams;
use crate::auth::authorization::{CredentialIdentity, RestrictedGrant};
use crate::models::poam::{CveRelationshipRowKey, PoamSummary};
use crate::queries::cves::{
    self, MAX_SYSTEM_CVE_INVENTORY_PAGE_SIZE, SystemCveInventoryPageError,
    SystemCveInventoryPageRequest, WorkItemRow, system_cve_inventory_page_error,
};
use crate::queries::systems::{SystemAccessRow, find_system_access_row};
use crate::services::poam::{self as poam_service, PoamActor, PoamError, SystemClock};

/// Default number of items in one collection page.
const DEFAULT_LIMIT: i64 = 25;
/// Largest accepted collection page (specification section 9.2).
const MAX_LIMIT: i64 = 100;
/// Longest returned CVE description in Unicode scalar values, before `…`.
const MAX_DESCRIPTION_CHARS: usize = 600;
/// Longest accepted `package_name` in Unicode scalar values.
const MAX_PACKAGE_NAME_CHARS: usize = 200;
/// Longest accepted cursor; a valid cursor is far shorter.
const MAX_CURSOR_BYTES: usize = 8_192;

const LIST_REMEDIATION_WORK: &str = "list_remediation_work";
const GET_SYSTEM_SECURITY_CONTEXT: &str = "get_system_security_context";
const GET_REMEDIATION_CONTEXT: &str = "get_remediation_context";
const GET_POAM_DETAIL: &str = "get_poam_detail";
const REQUEST_CVE_SCAN: &str = "request_cve_scan";
const GET_SCAN_RESULT: &str = "get_scan_result";
const UPDATE_POAM_PROGRESS: &str = "update_poam_progress";
const VERIFY_POAM: &str = "verify_poam";

/// Returns whether a tool changes state. The transport audits a denied call of
/// such a tool as `mcp_write_denied`.
pub fn is_write_tool(name: &str) -> bool {
    matches!(name, REQUEST_CVE_SCAN | UPDATE_POAM_PROGRESS | VERIFY_POAM)
}

const EVIDENCE_NOTE: &str = "Authority no_scan means that no usable completed \
scan exists for the running target. It does not mean that the system is clean. \
External and Reference systems have no managed running report, so they have no \
current findings. read_only=true means the evidence cannot authorize remediation.";

/// Identifies the authenticated caller of one MCP request.
///
/// The transport builds this value from a token admitted by
/// [`crate::services::access_tokens::validate_presented_token`] in the same
/// request. Owner fields describe the owner's state at admission time. The
/// grant is the token's immutable limit; nothing in this module widens it.
#[derive(Debug, Clone)]
pub struct ToolCaller {
    /// Identifies the token by its public ID, never by its secret.
    pub token_id: Uuid,
    /// Identifies the token owner.
    pub owner_user_id: Uuid,
    /// Reports whether the owner held Admin at admission. Admin removes only
    /// the owner-membership side of the intersection, never the grant side.
    pub owner_is_admin: bool,
    /// Lists the owner's environment memberships at admission.
    pub owner_environment_ids: Vec<Uuid>,
    /// Carries the token's immutable scopes, environments, and expiry.
    pub grant: RestrictedGrant,
}

/// Describes one MCP tool for `tools/list`.
///
/// Names, titles, descriptions, and schemas are static. They never contain
/// data from the database.
#[derive(Debug)]
pub struct ToolDef {
    /// Gives the stable MCP tool name.
    pub name: &'static str,
    /// Gives the human-readable tool title.
    pub title: &'static str,
    /// Describes the tool's inputs, outputs, and authorization rules.
    pub description: &'static str,
    /// Lists every scope that the token must hold to see and call the tool.
    pub required_scopes: &'static [&'static str],
    /// Returns the tool's JSON Schema (draft 2020-12) for its arguments.
    pub input_schema: fn() -> Value,
}

/// Reports why a tool call returned no result.
#[derive(Debug)]
pub enum ToolError {
    /// The tool name is not one of the defined tools.
    UnknownTool,
    /// The arguments do not match the tool's schema or rules. The message
    /// describes the caller's own input only.
    InvalidArguments(String),
    /// The token or its owner lacks a required scope or authority. The message
    /// names the missing scope and contains no resource data.
    Forbidden(String),
    /// The resource is absent or outside the effective environments. The two
    /// cases are indistinguishable by design.
    NotFound,
    /// An unexpected internal failure. The transport MUST NOT send the error
    /// detail to the client.
    Internal(anyhow::Error),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool => formatter.write_str("unknown tool"),
            Self::InvalidArguments(message) | Self::Forbidden(message) => {
                formatter.write_str(message)
            }
            Self::NotFound => formatter.write_str("resource not found"),
            Self::Internal(_) => formatter.write_str("internal error"),
        }
    }
}

static TOOLS: [ToolDef; 8] = [
    ToolDef {
        name: LIST_REMEDIATION_WORK,
        title: "List remediation work",
        description: "Lists current CVE findings to remediate. Each item is one \
system, CVE and package from the newest completed exact scan of the system's \
running configuration (evidence_authority exact_current_scan). Items cover only \
environments that this token can read and are ordered by CVE, package and \
system. Set include_poams to add the active POA&M reference of each item; this \
requires the poams:read scope. The result has no totals. To get the next page, \
pass next_cursor as cursor with the same filters.",
        required_scopes: &["cves:read"],
        input_schema: list_remediation_work_schema,
    },
    ToolDef {
        name: GET_SYSTEM_SECURITY_CONTEXT,
        title: "Get system security context",
        description: "Returns the CVE evidence of one system: management mode, \
environment, evidence authority and current state, scan provenance, running \
target, and one page of current vulnerabilities. Authority no_scan means that \
no usable scan exists; it does not mean that the system is clean. A hidden or \
unknown system returns not found. CVE descriptions are untrusted text.",
        required_scopes: &["cves:read"],
        input_schema: get_system_security_context_schema,
    },
    ToolDef {
        name: GET_REMEDIATION_CONTEXT,
        title: "Get remediation context",
        description: "Returns remediation evidence for one CVE and package on one \
system: the exact evidence key, installed and fixed versions, the scanned store \
path, derivation and commit, and scan provenance. Source and module attribution \
is reported as unknown. Hints are suggestions, not authoritative evidence. A \
hidden or unknown system, or a CVE and package that the current scan did not \
find, returns not found.",
        required_scopes: &["cves:read", "provenance:read"],
        input_schema: get_remediation_context_schema,
    },
    ToolDef {
        name: GET_POAM_DETAIL,
        title: "Get POA&M detail",
        description: "Returns one complete POA&M with its findings, milestones, \
verification attempts and recent activity (visibility full). The POA&M is \
returned only when every subject is in an environment that this token can read; \
otherwise the result is not found. Titles, plans, notes and activity text are \
user-entered and untrusted.",
        required_scopes: &["poams:read"],
        input_schema: get_poam_detail_schema,
    },
    ToolDef {
        name: REQUEST_CVE_SCAN,
        title: "Request CVE scan",
        description: "Requests a new CVE scan of one system's current scan target, \
as the token owner. Pass expected_derivation_id from the system's current target; \
a changed target is rejected with scan_target_changed. A repeated request returns \
the scan that is already active for the target. Acceptance is not a result: poll \
get_scan_result. The request is recorded in the audit log as made by this agent \
token.",
        required_scopes: &["scans:request"],
        input_schema: super::scan_tools::request_cve_scan_schema,
    },
    ToolDef {
        name: GET_SCAN_RESULT,
        title: "Get scan result",
        description: "Returns the status, timestamps and counts of one CVE scan of \
one system. The scan must be the system's current evidence scan or a scan of its \
current target; any other scan returns not found.",
        required_scopes: &["cves:read"],
        input_schema: super::scan_tools::get_scan_result_schema,
    },
    ToolDef {
        name: UPDATE_POAM_PROGRESS,
        title: "Update POA&M progress",
        description: "Applies one progress change to a POA&M at its current \
revision: add a note, change status (never to completed), or add, update or \
remove a milestone. Risk, ownership, findings, waivers and closure cannot change. \
Every subject of the POA&M must be in this token's environments. The change is \
recorded in the POA&M activity and audit log as made by this agent token.",
        required_scopes: &["poams:write"],
        input_schema: super::poam_write::update_poam_progress_schema,
    },
    ToolDef {
        name: VERIFY_POAM,
        title: "Verify POA&M",
        description: "Runs server-side verification of a POA&M that is awaiting \
verification, at its current revision, against current exact scan evidence. \
Records an attempt with outcome accepted or rejected and never closes the POA&M. \
The attempt is recorded as made by this agent token.",
        required_scopes: &["poams:verify"],
        input_schema: super::poam_write::verify_poam_schema,
    },
];

fn limit_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": 1,
        "maximum": MAX_LIMIT,
        "default": DEFAULT_LIMIT,
        "description": "Maximum number of items in this page."
    })
}

fn cursor_schema() -> Value {
    json!({
        "type": "string",
        "description": "Opaque next_cursor from the previous page of this tool. \
    Send the same other arguments as that request."
    })
}

fn severity_schema() -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "items": {"enum": ["critical", "high", "medium", "low", "unknown"]},
        "description": "Returns only items with one of these CVSS v3 severities."
    })
}

fn system_id_schema() -> Value {
    json!({"type": "string", "format": "uuid", "description": "System ID."})
}

fn list_remediation_work_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "environment_ids": {
                "type": "array",
                "minItems": 1,
                "items": {"type": "string", "format": "uuid"},
                "description": "Environment IDs to include. Every ID must be \
    readable by this token. Omit to include every environment that this token can read."
            },
            "severity": severity_schema(),
            "limit": limit_schema(),
            "cursor": cursor_schema(),
            "include_poams": {
                "type": "boolean",
                "default": false,
                "description": "Adds the active POA&M reference of each item. \
    Requires the poams:read scope."
            }
        },
        "required": [],
        "additionalProperties": false
    })
}

fn get_system_security_context_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "system_id": system_id_schema(),
            "severity": severity_schema(),
            "limit": limit_schema(),
            "cursor": cursor_schema()
        },
        "required": ["system_id"],
        "additionalProperties": false
    })
}

fn get_remediation_context_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "system_id": system_id_schema(),
            "cve_id": {
                "type": "string",
                "pattern": "^[Cc][Vv][Ee]-[0-9]{4}-[0-9]{4,}$",
                "description": "CVE identifier, for example CVE-2024-3094. \
    Matched case-insensitively."
            },
            "package_name": {
                "type": "string",
                "minLength": 1,
                "maxLength": MAX_PACKAGE_NAME_CHARS,
                "description": "Canonical or observed package name of the finding."
            }
        },
        "required": ["system_id", "cve_id", "package_name"],
        "additionalProperties": false
    })
}

fn get_poam_detail_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "poam_id": {"type": "string", "format": "uuid", "description": "POA&M ID."}
        },
        "required": ["poam_id"],
        "additionalProperties": false
    })
}

/// Returns the tools whose required scopes the token holds, in a stable order.
///
/// This filter is for discovery only. [`call_tool`] authorizes every call
/// again, so a client that cached an older list gains nothing.
pub fn visible_tools(caller: &ToolCaller) -> Vec<&'static ToolDef> {
    TOOLS
        .iter()
        .filter(|tool| caller.grant.has_scopes(tool.required_scopes))
        .collect()
}

/// Runs one tool for an authenticated caller and returns its JSON result.
///
/// Every call checks the tool's scopes against the grant and computes the
/// effective environments again before it reads any resource. Read tools never
/// mutate state. Write tools (`request_cve_scan`, `update_poam_progress`,
/// `verify_poam`) reauthorize against the token row, the owner's current role
/// and environments inside the transaction of their change, and record the
/// token as the acting credential. No tool runs Nix commands.
///
/// # Errors
///
/// - [`ToolError::UnknownTool`] when `name` is not a defined tool.
/// - [`ToolError::Forbidden`] when the grant lacks a required scope, has the
///   wrong audience or has expired, or the owner can no longer read.
/// - [`ToolError::InvalidArguments`] when `arguments` do not match the schema,
///   an environment selector is outside the effective set, or a cursor is
///   malformed or was issued for other filters.
/// - [`ToolError::NotFound`] when a requested system, finding, or POA&M is
///   absent or hidden.
/// - [`ToolError::Internal`] on a database or serialization failure.
pub async fn call_tool(
    pool: &PgPool,
    caller: &ToolCaller,
    name: &str,
    arguments: Value,
) -> Result<Value, ToolError> {
    let tool = TOOLS
        .iter()
        .find(|tool| tool.name == name)
        .ok_or(ToolError::UnknownTool)?;
    // SECURITY: Authorization precedes argument parsing and every resource
    // read, so a caller without the scopes learns nothing from the call.
    let effective = effective_environments(pool, caller, tool.required_scopes).await?;
    match tool.name {
        LIST_REMEDIATION_WORK => list_remediation_work(pool, caller, &effective, arguments).await,
        GET_SYSTEM_SECURITY_CONTEXT => {
            get_system_security_context(pool, &effective, arguments).await
        }
        GET_REMEDIATION_CONTEXT => get_remediation_context(pool, &effective, arguments).await,
        GET_POAM_DETAIL => get_poam_detail(pool, caller, &effective, arguments).await,
        REQUEST_CVE_SCAN => {
            super::scan_tools::request_cve_scan(pool, caller, &effective, arguments).await
        }
        GET_SCAN_RESULT => {
            super::scan_tools::get_scan_result(pool, caller, &effective, arguments).await
        }
        // SECURITY: The POA&M write services refresh the owner, token row,
        // roles and environments inside their own transaction; `effective`
        // above only rejects a call that cannot succeed.
        UPDATE_POAM_PROGRESS => {
            super::poam_write::update_poam_progress(pool, caller, arguments).await
        }
        VERIFY_POAM => super::poam_write::verify_poam(pool, caller, arguments).await,
        _ => Err(ToolError::UnknownTool),
    }
}

/// Returns the active environments that this call may read, sorted by ID.
async fn effective_environments(
    pool: &PgPool,
    caller: &ToolCaller,
    required_scopes: &[&str],
) -> Result<Vec<Uuid>, ToolError> {
    let granted = CredentialIdentity::Restricted(caller.grant.clone())
        .reading_environments(
            caller.owner_is_admin,
            caller.owner_environment_ids.clone(),
            required_scopes,
            Utc::now(),
        )
        .ok_or_else(|| {
            ToolError::Forbidden(format!(
                "This token does not grant {}",
                required_scopes.join(", ")
            ))
        })?;
    if granted.is_empty() {
        return Ok(Vec::new());
    }
    // SECURITY: A deactivated environment grants nothing even when the token
    // and the owner still name it. NULL `is_active` is treated as inactive,
    // as in the access-token service.
    sqlx::query_scalar(
        "SELECT id FROM environments WHERE id=ANY($1) AND COALESCE(is_active,false) ORDER BY id",
    )
    .bind(&granted)
    .fetch_all(pool)
    .await
    .map_err(|error| ToolError::Internal(error.into()))
}

pub(super) fn parse_arguments<T: DeserializeOwned>(arguments: Value) -> Result<T, ToolError> {
    // COMPATIBILITY: MCP clients may omit `arguments` for a tool without
    // required inputs; the transport then passes JSON null.
    let arguments = if arguments.is_null() {
        json!({})
    } else {
        arguments
    };
    serde_json::from_value(arguments)
        .map_err(|error| ToolError::InvalidArguments(error.to_string()))
}

fn page_limit(limit: Option<i64>) -> Result<i64, ToolError> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    if (1..=MAX_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(ToolError::InvalidArguments(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )))
    }
}

/// Selects one CVSS v3 severity band.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Unknown,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}

/// Returns sorted unique severity names; an empty result applies no filter.
fn severity_names(severity: Option<Vec<Severity>>) -> Result<Vec<String>, ToolError> {
    match severity {
        None => Ok(Vec::new()),
        Some(values) if values.is_empty() => Err(ToolError::InvalidArguments(
            "severity must not be empty".into(),
        )),
        Some(values) => Ok(values
            .into_iter()
            .map(Severity::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_owned)
            .collect()),
    }
}

/// Removes control characters except newline and truncates long text.
fn sanitize_description(text: &str) -> String {
    let mut cleaned = text
        .chars()
        .filter(|character| *character == '\n' || !character.is_control())
        .collect::<String>();
    if let Some((index, _)) = cleaned.char_indices().nth(MAX_DESCRIPTION_CHARS) {
        cleaned.truncate(index);
        cleaned.push('…');
    }
    cleaned
}

/// Returns the uppercase CVE ID when `value` matches `^CVE-\d{4}-\d{4,}$`
/// case-insensitively.
fn normalize_cve_id(value: &str) -> Option<String> {
    let id = value.trim().to_ascii_uppercase();
    let (year, number) = id.strip_prefix("CVE-")?.split_once('-')?;
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    (year.len() == 4 && number.len() >= 4 && digits(year) && digits(number)).then_some(id)
}

/// Returns a restricted POA&M actor. POA&M services refresh the owner's roles
/// and memberships from the database and intersect them with the grant, so
/// the placeholder authority fields below never grant anything.
pub(super) fn restricted_actor(caller: &ToolCaller) -> PoamActor {
    PoamActor {
        credential: CredentialIdentity::Restricted(caller.grant.clone()),
        user_id: caller.owner_user_id,
        identifier: String::new(),
        is_admin: false,
        can_mutate: false,
        environment_ids: Vec::new(),
        request_origin: None,
    }
}

/// Returns the system when its current environment is in `effective`.
///
/// SECURITY: An absent system, an unassigned system, and a system in another
/// environment all return [`ToolError::NotFound`]. Callers must call this
/// before they parse a cursor or read evidence.
pub(super) async fn authorized_system(
    pool: &PgPool,
    effective: &[Uuid],
    system_id: Uuid,
) -> Result<SystemAccessRow, ToolError> {
    match find_system_access_row(pool, system_id)
        .await
        .map_err(ToolError::Internal)?
    {
        Some(system)
            if system
                .environment_id
                .is_some_and(|environment_id| effective.contains(&environment_id)) =>
        {
            Ok(system)
        }
        _ => Err(ToolError::NotFound),
    }
}

fn inventory_request(
    limit: u16,
    after: Option<String>,
    search: Option<String>,
    severities: &[String],
) -> Result<SystemCveInventoryPageRequest, ToolError> {
    SystemCveInventoryPageRequest::from_params(SystemCveInventoryParams {
        limit: Some(limit),
        after,
        q: search,
        severity: (!severities.is_empty()).then(|| severities.join(",")),
        ..SystemCveInventoryParams::default()
    })
    .map_err(inventory_error)
}

fn inventory_error(error: anyhow::Error) -> ToolError {
    let mapped = match system_cve_inventory_page_error(&error) {
        Some(SystemCveInventoryPageError::InvalidRequest(message)) => {
            Some(ToolError::InvalidArguments((*message).to_owned()))
        }
        Some(
            SystemCveInventoryPageError::InvalidCursor
            | SystemCveInventoryPageError::InventoryChanged,
        ) => Some(ToolError::InvalidArguments(
            "cursor is invalid or the inventory changed; request the first page again".into(),
        )),
        Some(SystemCveInventoryPageError::TargetUnavailable) => Some(ToolError::NotFound),
        None => None,
    };
    mapped.unwrap_or_else(|| ToolError::Internal(error))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListWorkArgs {
    environment_ids: Option<Vec<Uuid>>,
    severity: Option<Vec<Severity>>,
    limit: Option<i64>,
    cursor: Option<String>,
    #[serde(default)]
    include_poams: bool,
}

/// Continues `list_remediation_work` after the last returned key.
///
/// SECURITY: The cursor is an unsigned position hint, not authority. Every
/// page is filtered by the effective environments of its own call. `filter`
/// binds the cursor to this tool and to the caller's filter arguments.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkCursor {
    tool: String,
    filter: String,
    cve_id: String,
    package_name: String,
    system_id: Uuid,
}

/// Fingerprints the caller's filter arguments, not the effective set. A
/// changed owner state therefore narrows later pages instead of failing them.
fn work_filter_fingerprint(environment_ids: Option<&[Uuid]>, severities: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(LIST_REMEDIATION_WORK);
    digest.update([0]);
    match environment_ids {
        None => digest.update("token"),
        Some(ids) => {
            for id in ids {
                digest.update(id.as_bytes());
            }
        }
    }
    digest.update([0]);
    digest.update(severities.join(","));
    hex::encode(digest.finalize())
}

fn decode_work_cursor(value: &str, fingerprint: &str) -> Result<WorkCursor, ToolError> {
    let invalid = || ToolError::InvalidArguments("cursor is invalid".into());
    if value.is_empty() || value.len() > MAX_CURSOR_BYTES {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| invalid())?;
    let cursor: WorkCursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.tool != LIST_REMEDIATION_WORK || cursor.filter != fingerprint {
        return Err(ToolError::InvalidArguments(
            "cursor was issued for a different tool or filter".into(),
        ));
    }
    Ok(cursor)
}

fn encode_work_cursor(cursor: &WorkCursor) -> Result<String, ToolError> {
    serde_json::to_vec(cursor)
        .map(|bytes| URL_SAFE_NO_PAD.encode(bytes))
        .map_err(|error| ToolError::Internal(error.into()))
}

async fn list_remediation_work(
    pool: &PgPool,
    caller: &ToolCaller,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: ListWorkArgs = parse_arguments(arguments)?;
    if args.include_poams && !caller.grant.has_scopes(&["poams:read"]) {
        return Err(ToolError::Forbidden(
            "This token does not grant poams:read".into(),
        ));
    }
    let limit = page_limit(args.limit)?;
    // SECURITY: An explicit selector can only narrow the effective set. An
    // unavailable ID is an error, never a request to read more environments.
    let requested = match args.environment_ids {
        None => None,
        Some(ids) if ids.is_empty() => {
            return Err(ToolError::InvalidArguments(
                "environment_ids must not be empty".into(),
            ));
        }
        Some(ids) => {
            let ids = ids.into_iter().collect::<BTreeSet<_>>();
            if let Some(id) = ids.iter().find(|id| !effective.contains(id)) {
                return Err(ToolError::InvalidArguments(format!(
                    "environment {id} is not available to this token"
                )));
            }
            Some(ids.into_iter().collect::<Vec<_>>())
        }
    };
    let severities = severity_names(args.severity)?;
    let fingerprint = work_filter_fingerprint(requested.as_deref(), &severities);
    let after = args
        .cursor
        .as_deref()
        .map(|cursor| decode_work_cursor(cursor, &fingerprint))
        .transpose()?;
    let mut rows = cves::list_current_exact_work_items(
        pool,
        requested.as_deref().unwrap_or(effective),
        &severities,
        after.as_ref().map(|cursor| {
            (
                cursor.cve_id.as_str(),
                cursor.package_name.as_str(),
                cursor.system_id,
            )
        }),
        limit + 1,
    )
    .await
    .map_err(ToolError::Internal)?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let next_cursor = match rows.last() {
        Some(row) if has_more => Some(encode_work_cursor(&WorkCursor {
            tool: LIST_REMEDIATION_WORK.into(),
            filter: fingerprint,
            cve_id: row.cve_id.clone(),
            package_name: row.package_name.clone(),
            system_id: row.system_id,
        })?),
        _ => None,
    };
    let poams = if args.include_poams {
        active_poams_for_rows(pool, caller, &rows).await?
    } else {
        HashMap::new()
    };
    let items = rows
        .iter()
        .map(|row| {
            let mut item = json!({
                "system_id": row.system_id,
                "hostname": row.hostname,
                "environment_id": row.environment_id,
                "environment_name": row.environment_name,
                "cve_id": row.cve_id,
                "package_name": row.package_name,
                "installed_version": row.installed_version,
                "fixed_version": row.fixed_version,
                "severity": row.severity,
                "evidence_authority": "exact_current_scan",
                "scan_id": row.scan_id,
                "scan_completed_at": row.scan_completed_at,
            });
            if args.include_poams {
                item["poam"] = poams
                    .get(&(row.system_id, row.cve_id.clone(), row.package_name.clone()))
                    .map_or(Value::Null, |poam| {
                        json!({
                            "id": poam.id,
                            "human_id": poam.human_id,
                            "title": poam.title,
                            "status": poam.status,
                        })
                    });
            }
            item
        })
        .collect::<Vec<_>>();
    Ok(json!({"items": items, "next_cursor": next_cursor}))
}

/// Returns the active POA&M of each page row, keyed by system, CVE, and
/// package.
///
/// PERFORMANCE: One read-only transaction runs per distinct system on the
/// page, so at most [`MAX_LIMIT`] transactions run per call.
async fn active_poams_for_rows(
    pool: &PgPool,
    caller: &ToolCaller,
    rows: &[WorkItemRow],
) -> Result<HashMap<(Uuid, String, String), PoamSummary>, ToolError> {
    let actor = restricted_actor(caller);
    let mut keys_by_system = BTreeMap::<Uuid, Vec<CveRelationshipRowKey>>::new();
    for row in rows {
        keys_by_system
            .entry(row.system_id)
            .or_default()
            .push(CveRelationshipRowKey {
                canonical_cve_id: row.cve_id.clone(),
                canonical_package_name: row.package_name.clone(),
                scan_id: row.scan_id,
                occurrence_derivation_path: row.observed_derivation_path.clone(),
            });
    }
    let mut poams = HashMap::new();
    for (system_id, keys) in keys_by_system {
        match poam_service::cve_relationships_for_rows(
            pool,
            &actor,
            system_id,
            &keys,
            Some(1),
            None,
            &SystemClock,
        )
        .await
        {
            Ok(relationships) => {
                for relationship in relationships {
                    if let Some(summary) = relationship.active_poam {
                        poams.insert(
                            (
                                system_id,
                                relationship.observation.canonical_cve_id,
                                relationship.observation.canonical_package_name,
                            ),
                            summary,
                        );
                    }
                }
            }
            // SECURITY: The service denies the whole system when the owner
            // state changed or a selected POA&M has a hidden subject. Those
            // rows report no POA&M rather than a partial plan reference.
            Err(PoamError::NotFound | PoamError::Forbidden) => {}
            Err(PoamError::Database(error)) => return Err(ToolError::Internal(error)),
            Err(other) => {
                return Err(ToolError::Internal(anyhow::anyhow!(
                    "POA&M relationship read failed: {other:?}"
                )));
            }
        }
    }
    Ok(poams)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemContextArgs {
    system_id: Uuid,
    severity: Option<Vec<Severity>>,
    limit: Option<i64>,
    cursor: Option<String>,
}

async fn get_system_security_context(
    pool: &PgPool,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: SystemContextArgs = parse_arguments(arguments)?;
    let limit = page_limit(args.limit)?;
    let severities = severity_names(args.severity)?;
    let system = authorized_system(pool, effective, args.system_id).await?;
    // SECURITY: The inventory cursor is parsed only after the system check,
    // and it is bound to this system, its evidence source, and its filters.
    let page = inventory_request(limit as u16, args.cursor, None, &severities)?;
    let inventory = cves::fetch_system_cve_inventory_page(pool, system.id, &page)
        .await
        .map_err(inventory_error)?;
    let vulnerabilities = inventory
        .rows
        .iter()
        .map(|row| {
            json!({
                "cve_id": row.cve_id,
                "package_name": row.package_name,
                "canonical_package_name": row.canonical_package_name,
                "installed_version": row.installed_version,
                "fixed_version": row.fixed_version,
                "severity": row.severity,
                "cvss_score": row.cvss_score,
                "status": row.status,
                "description": sanitize_description(&row.description),
                "first_seen": row.first_seen,
                "published_at": row.published_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "system": {
            "id": system.id,
            "hostname": system.hostname,
            "management_mode": system.management_mode,
            "environment": {"id": system.environment_id, "name": system.environment},
            "is_active": system.is_active,
        },
        "evidence": {
            "authority": inventory.authority,
            "current_state": inventory.current_state,
            "exact_authority_failure": inventory.exact_authority_failure,
            "source": inventory.source,
            "running_target": inventory.running_target,
            "read_only": inventory.read_only,
            "note": EVIDENCE_NOTE,
        },
        "vulnerabilities": vulnerabilities,
        "has_more": inventory.has_more,
        "next_cursor": inventory.next_cursor,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemediationArgs {
    system_id: Uuid,
    cve_id: String,
    package_name: String,
}

async fn get_remediation_context(
    pool: &PgPool,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: RemediationArgs = parse_arguments(arguments)?;
    let cve_id = normalize_cve_id(&args.cve_id)
        .ok_or_else(|| ToolError::InvalidArguments("cve_id must match CVE-YYYY-NNNN".into()))?;
    let package_chars = args.package_name.chars().count();
    if package_chars == 0 || package_chars > MAX_PACKAGE_NAME_CHARS {
        return Err(ToolError::InvalidArguments(format!(
            "package_name must be 1 to {MAX_PACKAGE_NAME_CHARS} characters"
        )));
    }
    let system = authorized_system(pool, effective, args.system_id).await?;
    let (inventory, row) =
        find_inventory_occurrence(pool, system.id, &cve_id, &args.package_name).await?;
    // SECURITY: The target query selects no flake repository URL. The target
    // is reported only when it names the same scan as the inventory row.
    let target = cves::fetch_current_cve_authority_target(pool, system.id)
        .await
        .map_err(ToolError::Internal)?
        .filter(|target| target.scan_id == row.scan_id)
        .map(|target| {
            json!({
                "generation": target.generation,
                "store_path": target.store_path,
                "derivation_path": target.derivation_path,
                "commit_hash": target.commit_hash,
            })
        });
    let hints = row
        .fixed_version
        .iter()
        .map(|fixed_version| {
            json!({
                "kind": "upgrade_package",
                "text": format!(
                    "The advisory data records version {fixed_version} of {} as fixed. \
            Updating the package to {fixed_version} or later can remove this finding. Confirm \
            with a new scan of the deployed system.",
                    row.canonical_package_name
                ),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "evidence_key": {
            "system_id": system.id,
            "scan_id": row.scan_id,
            "cve_id": row.cve_id,
            "package_name": row.canonical_package_name,
            "occurrence_derivation_path": row.occurrence_derivation_path,
        },
        "package": {
            "installed_version": row.installed_version,
            "fixed_version": row.fixed_version,
        },
        "target": target,
        "scan": inventory.source,
        "attribution": {
            "status": "unknown",
            "reason": "Source and module attribution is not implemented yet (TASK-271).",
        },
        "hints": hints,
        "hints_are_authoritative": false,
    }))
}

/// Finds the current inventory row for one exact CVE and package on a system.
///
/// The inventory search is a substring match on the CVE, so one search can
/// return more rows than one page holds. The lookup follows the inventory
/// cursor until it finds an exact CVE and package match or the inventory ends.
/// A missing finding therefore means the current inventory has no such row,
/// never that the row sits on a later page. Returns the page that holds the
/// row, because its `source` describes the scan for that row.
///
/// The loop ends: every page either finds the row, ends the inventory, or
/// advances the inventory cursor, and the inventory is finite. The MCP request
/// deadline bounds the total time.
async fn find_inventory_occurrence(
    pool: &PgPool,
    system_id: Uuid,
    cve_id: &str,
    package_name: &str,
) -> Result<
    (
        cves::SystemCveInventoryQuery,
        cves::ExactSystemVulnerabilityRow,
    ),
    ToolError,
> {
    let mut after = None;
    loop {
        let page = inventory_request(
            MAX_SYSTEM_CVE_INVENTORY_PAGE_SIZE,
            after,
            Some(cve_id.to_owned()),
            &[],
        )?;
        let mut inventory = cves::fetch_system_cve_inventory_page(pool, system_id, &page)
            .await
            .map_err(|error| match system_cve_inventory_page_error(&error) {
                // The caller sent no cursor; the cursor here is internal. An
                // inventory that changes between pages is a retryable failure.
                Some(
                    SystemCveInventoryPageError::InvalidCursor
                    | SystemCveInventoryPageError::InventoryChanged,
                ) => ToolError::Internal(anyhow::anyhow!(
                    "system CVE inventory changed during the remediation lookup"
                )),
                _ => inventory_error(error),
            })?;
        let found = inventory.rows.iter().position(|row| {
            row.cve_id == cve_id
                && (row.canonical_package_name == package_name || row.package_name == package_name)
        });
        if let Some(index) = found {
            let row = inventory.rows.swap_remove(index);
            return Ok((inventory, row));
        }
        match (inventory.has_more, inventory.next_cursor) {
            (true, Some(cursor)) => after = Some(cursor),
            _ => return Err(ToolError::NotFound),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PoamDetailArgs {
    poam_id: Uuid,
}

async fn get_poam_detail(
    pool: &PgPool,
    caller: &ToolCaller,
    effective: &[Uuid],
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: PoamDetailArgs = parse_arguments(arguments)?;
    if effective.is_empty() {
        return Err(ToolError::NotFound);
    }
    // SECURITY: The service repeats the owner refresh, grant intersection, and
    // active-environment filter in its own transaction, then requires every
    // POA&M subject to be visible. There is no scoped partial projection.
    let detail = poam_service::restricted_detail(
        pool,
        &restricted_actor(caller),
        args.poam_id,
        &SystemClock,
    )
    .await
    .map_err(|error| match error {
        PoamError::NotFound => ToolError::NotFound,
        PoamError::Forbidden => {
            ToolError::Forbidden("The token owner can no longer read POA&Ms".into())
        }
        PoamError::Database(error) => ToolError::Internal(error),
        other => ToolError::Internal(anyhow::anyhow!("POA&M detail read failed: {other:?}")),
    })?;
    Ok(json!({"visibility": "full", "poam": detail}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cve_ids_normalize_and_reject_malformed_values() {
        assert_eq!(
            normalize_cve_id(" cve-2024-3094 ").as_deref(),
            Some("CVE-2024-3094")
        );
        assert_eq!(
            normalize_cve_id("CVE-2024-123456").as_deref(),
            Some("CVE-2024-123456")
        );
        for invalid in [
            "CVE-24-1234",
            "CVE-2024-123",
            "CVE-2024-12a4",
            "GHSA-1234",
            "",
        ] {
            assert_eq!(normalize_cve_id(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn descriptions_lose_control_characters_and_are_truncated() {
        assert_eq!(sanitize_description("a\u{1b}[31mb\nc\t"), "a[31mb\nc");
        let long = "x".repeat(MAX_DESCRIPTION_CHARS + 5);
        let sanitized = sanitize_description(&long);
        assert_eq!(sanitized.chars().count(), MAX_DESCRIPTION_CHARS + 1);
        assert!(sanitized.ends_with('…'));
    }

    #[test]
    fn schemas_are_closed_objects() {
        for tool in &TOOLS {
            let schema = (tool.input_schema)();
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(schema["additionalProperties"], false, "{}", tool.name);
        }
    }
}
