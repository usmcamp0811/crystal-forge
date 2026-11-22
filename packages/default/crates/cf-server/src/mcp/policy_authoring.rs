//! Defines the MCP policy authoring tools (TASK-534.7):
//! `create_policy_draft`, `update_policy_draft` and `validate_policy_draft`
//! (scope `policies:draft`), `preview_policy_import` (scope
//! `compliance:preview`) and `import_policy_draft` (scope `policies:import`).
//!
//! The functions here parse and bound the tool arguments and map service
//! errors to [`ToolError`]. The domain rules live in
//! [`crate::services::policy_authoring`], which the Admin browser handlers
//! share. The dispatcher in [`super::tools`] checks the scope, and for the
//! Admin-only scopes the owner's Admin role at admission, before it calls a
//! function here. Every write then reauthorizes the token row, the scope and
//! the owner's Admin role inside its own transaction.
//!
//! The write set is closed. No tool trusts, publishes, assigns, enables,
//! changes an enforcement mode or deletes. Created and imported versions are
//! untrusted drafts; new lineages start disabled. An agent edits only a
//! mutable draft whose `created_by` is the token owner.
//!
//! SECURITY: Policy documents and configs are untrusted text. They can
//! contain Nix expressions; the validators only parse them with a 2-second
//! deadline and never evaluate them. Audit events record identifiers,
//! revisions, field names and the source SHA-256, never document text.

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use super::tools::{ToolCaller, ToolError, parse_arguments};
use crate::models::deployment_policies::{
    CreatePolicyRequirementMapping, is_syntax_validation_unavailable,
};
use crate::services::policy_authoring::{
    self as authoring, AuthoringActor, DefinitionError, DraftChanges, EXECUTION_PHASES,
    NewPolicyLineage, PolicyAuthoringError, PolicyDefinition, ValidationIssue,
};

/// Tool name of draft creation (new lineage or derived draft).
pub(super) const CREATE_POLICY_DRAFT: &str = "create_policy_draft";
/// Tool name of the draft update at an expected revision.
pub(super) const UPDATE_POLICY_DRAFT: &str = "update_policy_draft";
/// Tool name of the definition validation.
pub(super) const VALIDATE_POLICY_DRAFT: &str = "validate_policy_draft";
/// Tool name of the read-only interchange preview.
pub(super) const PREVIEW_POLICY_IMPORT: &str = "preview_policy_import";
/// Tool name of the interchange import.
pub(super) const IMPORT_POLICY_DRAFT: &str = "import_policy_draft";

const VALIDATION_NOTE: &str = "Syntax validation does not prove that the check is effective.";

const DRAFT_NOTE: &str = "The draft is untrusted and unpublished. Only an Admin in the \
browser can trust, publish or assign it. Send draft_revision with every update_policy_draft \
call.";

const IMPORT_NOTE: &str = "Imported versions are untrusted drafts and new lineages are \
disabled. Only an Admin in the browser can trust, publish or assign them.";

/// Message of a validator that gave no verdict. The input was not checked.
const VALIDATOR_UNAVAILABLE: &str = "The Nix syntax validator did not finish within its \
2-second deadline, so the policy was not checked and nothing was written. Retry the request.";

// ── Schemas ──────────────────────────────────────────────────────────────────

fn uuid_schema(description: &str) -> Value {
    json!({"type": "string", "format": "uuid", "description": description})
}

fn mappings_schema() -> Value {
    json!({
        "type": "array",
        "description": "Complete set of manual requirement mappings. Each requirement \
    version may appear once.",
        "items": {
            "type": "object",
            "properties": {
                "requirement_version_id": uuid_schema("Requirement version ID from \
    list_compliance_requirements."),
                "relationship": {"enum": ["implements", "supports", "provides_evidence_for"]},
                "coverage": {"enum": ["full", "partial"]},
                "rationale": {"type": "string"}
            },
            "required": ["requirement_version_id", "relationship", "coverage"],
            "additionalProperties": false
        }
    })
}

fn execution_phase_schema() -> Value {
    json!({
        "enum": EXECUTION_PHASES,
        "description": "Execution phase. Composite policies must use multi-phase. Default: \
    multi-phase for composite, else nix-evaluation."
    })
}

fn config_schema() -> Value {
    json!({
        "type": "object",
        "description": "Type-specific policy config, for example custom_check \
    {\"mode\": \"all\", \"rules\": [{\"expression\": \"...\"}]} or composite rules. Nix \
    expressions are parsed, never evaluated."
    })
}

fn metadata_schema() -> Value {
    json!({
        "type": "object",
        "description": "Version compliance metadata object (for example srg_ids, cci_ids, \
    category, rationale, evidence_specs). Replaces the stored object."
    })
}

fn document_schema() -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "description": "Complete policy interchange document text \
    (urn:crystal-forge:policy-set:1 or one exported policy). The request body limit of 1 MiB \
    bounds its size."
    })
}

fn format_schema() -> Value {
    json!({"enum": ["json", "toml"], "description": "Document format."})
}

/// Returns the JSON Schema (draft 2020-12) of `create_policy_draft`.
///
/// `mode` selects `new_lineage` (requires `name`, `policy_type`, `config`) or
/// `derive` (requires `policy_id`). The server rejects properties of the
/// other mode.
pub fn create_policy_draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "mode": {"enum": ["new_lineage", "derive"]},
            "name": {"type": "string", "minLength": 1, "maxLength": 255,
                "description": "new_lineage: unique lineage name."},
            "description": {"type": "string", "description": "new_lineage: description."},
            "policy_type": {"type": "string", "description": "new_lineage: policy type, \
    for example custom_check or composite."},
            "config": config_schema(),
            "execution_phase": execution_phase_schema(),
            "enabled_by_default": {"type": "boolean", "default": true,
                "description": "new_lineage: default enablement of the version. The new \
    lineage itself is always disabled."},
            "compliance_metadata": metadata_schema(),
            "requirement_mappings": mappings_schema(),
            "policy_id": uuid_schema("derive: policy lineage ID with a published version and \
    no open draft."),
            "source_version_id": uuid_schema("derive: optional; must be the current published \
    version."),
            "version": {"type": "string", "description": "derive: optional new version label. \
    Default: <published version>-draft."}
        },
        "required": ["mode"],
        "oneOf": [
            {"properties": {"mode": {"const": "new_lineage"}},
             "required": ["name", "policy_type", "config"]},
            {"properties": {"mode": {"const": "derive"}}, "required": ["policy_id"]}
        ],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `update_policy_draft`.
pub fn update_policy_draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "version_id": uuid_schema("Draft version ID created by this token's owner."),
            "draft_revision": {"type": "integer", "description": "Current draft_revision from \
    get_compliance_policy or the last draft tool result. A stale revision is rejected with \
    stale_revision and changes nothing."},
            "description": {"type": "string", "description": "New description; an empty \
    string clears it."},
            "config": config_schema(),
            "execution_phase": execution_phase_schema(),
            "enabled_by_default": {"type": "boolean", "description": "New default \
    enablement of the version. The lineage enablement never changes."},
            "compliance_metadata": metadata_schema(),
            "requirement_mappings": mappings_schema()
        },
        "required": ["version_id", "draft_revision"],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `validate_policy_draft`.
/// Exactly one of `version_id` and `definition` is required.
pub fn validate_policy_draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "version_id": uuid_schema("Existing policy version to validate. Send exactly one \
    of version_id and definition."),
            "definition": {
                "type": "object",
                "description": "Proposed policy to validate.",
                "properties": {
                    "name": {"type": "string", "description": "Proposed lineage name; only \
    used for a name-in-use warning."},
                    "policy_type": {"type": "string"},
                    "config": config_schema(),
                    "execution_phase": execution_phase_schema(),
                    "compliance_metadata": metadata_schema(),
                    "requirement_mappings": mappings_schema()
                },
                "required": ["policy_type", "config"],
                "additionalProperties": false
            }
        },
        "required": [],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `preview_policy_import`.
pub fn preview_policy_import_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": document_schema(),
            "format": format_schema()
        },
        "required": ["document", "format"],
        "additionalProperties": false
    })
}

/// Returns the JSON Schema (draft 2020-12) of `import_policy_draft`.
pub fn import_policy_draft_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "document": document_schema(),
            "format": format_schema(),
            "expected_source_sha256": {
                "type": "string",
                "pattern": "^[0-9a-fA-F]{64}$",
                "description": "source_sha256 from preview_policy_import for the same document."
            }
        },
        "required": ["document", "format", "expected_source_sha256"],
        "additionalProperties": false
    })
}

// ── Arguments ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingArg {
    requirement_version_id: Uuid,
    relationship: String,
    coverage: String,
    rationale: Option<String>,
}

fn manual_mappings(mappings: Vec<MappingArg>) -> Vec<CreatePolicyRequirementMapping> {
    mappings
        .into_iter()
        .map(|mapping| CreatePolicyRequirementMapping {
            requirement_version_id: mapping.requirement_version_id,
            relationship: mapping.relationship,
            coverage: mapping.coverage,
            rationale: mapping.rationale,
            // SECURITY: Agent mappings are always manual; imported and
            // inferred provenance comes only from the import paths.
            provenance: "manual".to_owned(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum CreateArgs {
    NewLineage {
        name: String,
        description: Option<String>,
        policy_type: String,
        config: Value,
        execution_phase: Option<String>,
        enabled_by_default: Option<bool>,
        compliance_metadata: Option<Value>,
        #[serde(default)]
        requirement_mappings: Vec<MappingArg>,
    },
    Derive {
        policy_id: Uuid,
        source_version_id: Option<Uuid>,
        version: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    version_id: Uuid,
    draft_revision: i64,
    description: Option<String>,
    config: Option<Value>,
    execution_phase: Option<String>,
    enabled_by_default: Option<bool>,
    compliance_metadata: Option<Value>,
    requirement_mappings: Option<Vec<MappingArg>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DefinitionArgs {
    name: Option<String>,
    policy_type: String,
    config: Value,
    execution_phase: Option<String>,
    compliance_metadata: Option<Value>,
    #[serde(default)]
    requirement_mappings: Vec<MappingArg>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateArgs {
    version_id: Option<Uuid>,
    definition: Option<DefinitionArgs>,
}

/// Selects the interchange document format. XCCDF is not accepted through
/// MCP.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DocumentFormat {
    Json,
    Toml,
}

impl DocumentFormat {
    /// Returns a filename whose extension selects the parser format.
    fn filename(self) -> &'static str {
        match self {
            Self::Json => "document.json",
            Self::Toml => "document.toml",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewArgs {
    document: String,
    format: DocumentFormat,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportArgs {
    document: String,
    format: DocumentFormat,
    expected_source_sha256: String,
}

fn non_empty_document(document: &str) -> Result<(), ToolError> {
    if document.trim().is_empty() {
        return Err(ToolError::InvalidArguments(
            "document must not be empty".into(),
        ));
    }
    Ok(())
}

fn actor(caller: &ToolCaller) -> AuthoringActor {
    AuthoringActor::token(caller.owner_user_id, caller.token_id)
}

/// Maps a service error to the tool error that the agent sees.
fn tool_error(error: PolicyAuthoringError) -> ToolError {
    match error {
        PolicyAuthoringError::Forbidden(message) => ToolError::Forbidden(message),
        PolicyAuthoringError::NotFound => ToolError::NotFound,
        PolicyAuthoringError::ValidatorUnavailable(_) => {
            ToolError::Unavailable(VALIDATOR_UNAVAILABLE.into())
        }
        PolicyAuthoringError::Invalid { message, .. }
            if is_syntax_validation_unavailable(&message) =>
        {
            ToolError::Unavailable(VALIDATOR_UNAVAILABLE.into())
        }
        PolicyAuthoringError::Invalid { code, message } => {
            ToolError::InvalidArguments(format!("{code}: {message}"))
        }
        PolicyAuthoringError::StaleRevision { current } => ToolError::InvalidArguments(format!(
            "stale_revision: the draft revision is now {current}. Read the version again with \
             get_compliance_policy and retry with its current draft_revision."
        )),
        PolicyAuthoringError::NoPublishedSource => ToolError::InvalidArguments(
            "no_published_version: the lineage has no published version to derive a draft from"
                .into(),
        ),
        PolicyAuthoringError::MutableDraftExists(version_id) => ToolError::InvalidArguments(
            format!("draft_exists: the lineage already has the open draft version {version_id}"),
        ),
        PolicyAuthoringError::SourceDigestMismatch { expected, actual } => {
            ToolError::InvalidArguments(format!(
                "source_digest_mismatch: expected_source_sha256 {expected} does not match the \
                 document SHA-256 {actual}; preview the document again"
            ))
        }
        PolicyAuthoringError::InterchangeConflicts(conflicts) => {
            ToolError::InvalidArguments(format!("import_conflicts: {}", Value::Array(conflicts)))
        }
        PolicyAuthoringError::Internal { context, error } => {
            ToolError::Internal(error.context(context))
        }
    }
}

fn draft_result(mode: &str, draft: &authoring::PolicyDraftRef) -> Value {
    json!({
        "mode": mode,
        "policy_id": draft.policy_id,
        "version_id": draft.version_id,
        "version": draft.version,
        "draft_revision": draft.draft_revision,
        "derived_from_version_id": draft.derived_from_version_id,
        "publication_state": "draft",
        "trust_state": "untrusted",
        "note": DRAFT_NOTE,
    })
}

// ── Tools ────────────────────────────────────────────────────────────────────

/// Runs `create_policy_draft`: creates a new lineage with an initial draft
/// (`mode: new_lineage`) or derives a draft from the published version of a
/// lineage (`mode: derive`).
pub(super) async fn create_policy_draft(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let actor = actor(caller);
    match parse_arguments::<CreateArgs>(arguments)? {
        CreateArgs::NewLineage {
            name,
            description,
            policy_type,
            config,
            execution_phase,
            enabled_by_default,
            compliance_metadata,
            requirement_mappings,
        } => {
            let input = NewPolicyLineage {
                name,
                description: description.filter(|text| !text.trim().is_empty()),
                enabled_by_default: enabled_by_default.unwrap_or(true),
                definition: PolicyDefinition {
                    policy_type,
                    config,
                    execution_phase,
                    compliance_metadata: compliance_metadata.unwrap_or_else(|| json!({})),
                    requirement_mappings: manual_mappings(requirement_mappings),
                },
            };
            let draft = authoring::create_policy_lineage_draft(pool, &actor, &input)
                .await
                .map_err(tool_error)?;
            Ok(draft_result("new_lineage", &draft))
        }
        CreateArgs::Derive {
            policy_id,
            source_version_id,
            version,
        } => {
            let mut tx = pool
                .begin()
                .await
                .map_err(|error| ToolError::Internal(error.into()))?;
            let draft = authoring::derive_policy_draft_tx(
                &mut tx,
                &actor,
                policy_id,
                source_version_id,
                version.as_deref(),
            )
            .await
            .map_err(tool_error)?;
            tx.commit()
                .await
                .map_err(|error| ToolError::Internal(error.into()))?;
            Ok(draft_result("derive", &draft))
        }
    }
}

/// Runs `update_policy_draft`: replaces fields of the owner's mutable draft
/// at the current `draft_revision`. A no-op returns `changed: false` and
/// writes nothing.
pub(super) async fn update_policy_draft(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: UpdateArgs = parse_arguments(arguments)?;
    let changes = DraftChanges {
        description: args.description,
        config: args.config,
        execution_phase: args.execution_phase,
        enabled_by_default: args.enabled_by_default,
        compliance_metadata: args.compliance_metadata,
        requirement_mappings: args.requirement_mappings.map(manual_mappings),
    };
    let outcome = authoring::update_policy_draft(
        pool,
        &actor(caller),
        args.version_id,
        args.draft_revision,
        &changes,
    )
    .await
    .map_err(tool_error)?;
    serde_json::to_value(outcome).map_err(|error| ToolError::Internal(error.into()))
}

#[derive(sqlx::FromRow)]
struct StoredDefinition {
    publication_state: String,
    policy_type: String,
    config: Value,
    execution_phase: String,
    compliance_metadata: Value,
}

fn warning(code: &'static str, field: &str, message: &str) -> ValidationIssue {
    ValidationIssue {
        code,
        field: field.to_owned(),
        message: message.to_owned(),
    }
}

/// Runs `validate_policy_draft` for an inline definition or a stored version.
///
/// Returns `{valid, errors, warnings, note}`; each error and warning is
/// `{code, field, message}`. Nothing is written and no Nix is evaluated. A
/// parser that gives no verdict returns an `internal_error` with a retry
/// hint, never `valid: true`.
pub(super) async fn validate_policy_draft(
    pool: &PgPool,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: ValidateArgs = parse_arguments(arguments)?;
    let mut warnings = Vec::new();
    let definition = match (args.version_id, args.definition) {
        (Some(version_id), None) => {
            let stored = sqlx::query_as::<_, StoredDefinition>(
                r#"SELECT publication_state, policy_type, config, execution_phase,
                          compliance_metadata
                   FROM deployment_policy_versions WHERE id = $1"#,
            )
            .bind(version_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| ToolError::Internal(error.into()))?
            .ok_or(ToolError::NotFound)?;
            let mappings = sqlx::query_as::<_, (Uuid, String, String, Option<String>)>(
                r#"SELECT requirement_version_id, relationship, coverage, rationale
                   FROM policy_requirement_mappings
                   WHERE policy_version_id = $1 AND provenance = 'manual'"#,
            )
            .bind(version_id)
            .fetch_all(pool)
            .await
            .map_err(|error| ToolError::Internal(error.into()))?;
            if !matches!(
                stored.publication_state.as_str(),
                "incomplete" | "draft" | "interim"
            ) {
                warnings.push(warning(
                    "immutable_version",
                    "version_id",
                    "The version is published or deprecated and cannot change; the result is \
                     informational.",
                ));
            }
            PolicyDefinition {
                policy_type: stored.policy_type,
                config: stored.config,
                execution_phase: Some(stored.execution_phase),
                compliance_metadata: stored.compliance_metadata,
                requirement_mappings: mappings
                    .into_iter()
                    .map(
                        |(requirement_version_id, relationship, coverage, rationale)| {
                            CreatePolicyRequirementMapping {
                                requirement_version_id,
                                relationship,
                                coverage,
                                rationale,
                                provenance: "manual".to_owned(),
                            }
                        },
                    )
                    .collect(),
            }
        }
        (None, Some(definition)) => {
            if let Some(name) = definition
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
            {
                let taken: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM deployment_policies WHERE name = $1)",
                )
                .bind(name)
                .fetch_one(pool)
                .await
                .map_err(|error| ToolError::Internal(error.into()))?;
                if taken {
                    warnings.push(warning(
                        "name_in_use",
                        "definition.name",
                        "A policy lineage with this name exists; create_policy_draft with \
                         mode new_lineage would be rejected.",
                    ));
                }
            }
            PolicyDefinition {
                policy_type: definition.policy_type,
                config: definition.config,
                execution_phase: definition.execution_phase,
                compliance_metadata: definition.compliance_metadata.unwrap_or_else(|| json!({})),
                requirement_mappings: manual_mappings(definition.requirement_mappings),
            }
        }
        _ => {
            return Err(ToolError::InvalidArguments(
                "send exactly one of version_id and definition".into(),
            ));
        }
    };
    if definition.requirement_mappings.is_empty() {
        warnings.push(warning(
            "unmapped",
            "requirement_mappings",
            "The policy maps to no framework requirement.",
        ));
    }
    let errors = match authoring::validate_policy_definition(pool, &definition).await {
        Ok(_) => Vec::new(),
        Err(DefinitionError::Invalid(issues)) => issues,
        Err(DefinitionError::ValidatorUnavailable(_)) => {
            return Err(ToolError::Unavailable(VALIDATOR_UNAVAILABLE.into()));
        }
        Err(DefinitionError::Database(error)) => return Err(ToolError::Internal(error.into())),
    };
    Ok(json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "warnings": warnings,
        "note": VALIDATION_NOTE,
    }))
}

/// Runs `preview_policy_import`: parses, validates and reconciles a JSON or
/// TOML interchange document without writing.
pub(super) async fn preview_policy_import(
    pool: &PgPool,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: PreviewArgs = parse_arguments(arguments)?;
    non_empty_document(&args.document)?;
    let preview = authoring::preview_policy_interchange(
        pool,
        args.document.as_bytes(),
        Some(args.format.filename()),
    )
    .await
    .map_err(tool_error)?;
    let mut result =
        serde_json::to_value(preview).map_err(|error| ToolError::Internal(error.into()))?;
    result["note"] = json!(
        "Preview only: nothing was written. Pass source_sha256 as expected_source_sha256 to \
         import_policy_draft with the same document and format."
    );
    Ok(result)
}

/// Runs `import_policy_draft`: imports a JSON or TOML interchange document
/// as untrusted drafts, all or nothing, when its SHA-256 equals
/// `expected_source_sha256`.
pub(super) async fn import_policy_draft(
    pool: &PgPool,
    caller: &ToolCaller,
    arguments: Value,
) -> Result<Value, ToolError> {
    let args: ImportArgs = parse_arguments(arguments)?;
    non_empty_document(&args.document)?;
    let imported = authoring::import_policy_interchange(
        pool,
        &actor(caller),
        args.document.as_bytes(),
        Some(args.format.filename()),
        &args.expected_source_sha256,
    )
    .await
    .map_err(tool_error)?;
    Ok(json!({
        "source_sha256": imported.source_sha256,
        "created_policy_count": imported.created,
        "reused_policy_count": imported.reused,
        "policies": imported.policies,
        "publication_state": "draft",
        "trusted": false,
        "enabled": false,
        "note": IMPORT_NOTE,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_closed_objects() {
        for schema in [
            create_policy_draft_schema(),
            update_policy_draft_schema(),
            validate_policy_draft_schema(),
            preview_policy_import_schema(),
            import_policy_draft_schema(),
        ] {
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
        }
    }

    #[test]
    fn create_arguments_are_tagged_by_mode_and_closed() {
        let derive: CreateArgs =
            parse_arguments(json!({"mode": "derive", "policy_id": Uuid::nil()})).expect("derive");
        assert!(matches!(derive, CreateArgs::Derive { .. }));
        let new: CreateArgs = parse_arguments(json!({
            "mode": "new_lineage",
            "name": "p",
            "policy_type": "custom_check",
            "config": {"mode": "all", "rules": []}
        }))
        .expect("new lineage");
        assert!(matches!(new, CreateArgs::NewLineage { .. }));
        for invalid in [
            json!({"mode": "publish", "policy_id": Uuid::nil()}),
            json!({"mode": "derive", "policy_id": Uuid::nil(), "name": "x"}),
            json!({"policy_id": Uuid::nil()}),
            json!({"mode": "derive", "policy_id": Uuid::nil(), "trust": true}),
        ] {
            assert!(
                parse_arguments::<CreateArgs>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn documents_accept_only_json_and_toml() {
        for format in ["json", "toml"] {
            assert!(
                parse_arguments::<PreviewArgs>(json!({"document": "x", "format": format})).is_ok()
            );
        }
        for format in ["xml", "xccdf", "yaml", "JSON"] {
            assert!(
                parse_arguments::<PreviewArgs>(json!({"document": "x", "format": format})).is_err(),
                "{format}"
            );
        }
    }

    #[test]
    fn unavailable_validator_is_never_reported_as_invalid_input() {
        let message = format!(
            "{}: the Nix parser did not finish within 2 seconds.",
            crate::models::deployment_policies::NIX_SYNTAX_VALIDATION_UNAVAILABLE
        );
        assert!(matches!(
            tool_error(PolicyAuthoringError::Invalid {
                code: "POLICY_INTERCHANGE_INVALID",
                message,
            }),
            ToolError::Unavailable(_)
        ));
        assert!(matches!(
            tool_error(PolicyAuthoringError::StaleRevision { current: 4 }),
            ToolError::InvalidArguments(message) if message.starts_with("stale_revision:")
        ));
        assert!(matches!(
            tool_error(PolicyAuthoringError::Forbidden("draft is owned by another user".into())),
            ToolError::Forbidden(message) if message == "draft is owned by another user"
        ));
    }
}
