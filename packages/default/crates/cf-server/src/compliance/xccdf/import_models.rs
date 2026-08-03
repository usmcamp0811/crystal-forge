//! Typed models for the XCCDF committed import flow.
//!
//! Separate from the export models and the parser models. These represent
//! the HTTP contract for the import endpoint and the intermediate state
//! used during plan validation and canonical record construction.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::compliance::xccdf::models::{CheckBody, CheckContent, FixContent, ParsedRule};

// ── Import plan (inbound from caller) ─────────────────────────────────────────

/// The JSON import plan submitted alongside the XCCDF file.
///
/// The plan is always validated against the reparsed document, never against
/// metadata copied from a preview response.
#[derive(Debug, Deserialize)]
pub struct XccdfImportPlan {
    /// SHA-256 hex of the original uploaded package (ZIP or XML), as returned
    /// by the preview endpoint. Used to detect TOCTOU between preview and import.
    pub expected_sha256: String,
    /// If supplied, only rules selected by this profile are eligible for import
    /// unless the rule action explicitly overrides the constraint.
    pub selected_profile_id: Option<String>,
    /// The rule IDs the caller has chosen to act on.  Every listed ID must exist
    /// in the reparsed document and must have exactly one action.  Rules not
    /// listed here are implicitly excluded.
    pub selected_rule_ids: Vec<String>,
    /// One action per selected rule.
    pub rule_actions: Vec<XccdfRuleImportAction>,
    /// Metadata for the draft bundle to create.
    pub bundle: ImportedBundlePlan,
}

/// Metadata for the draft bundle created during import.
#[derive(Debug, Deserialize)]
pub struct ImportedBundlePlan {
    pub name: String,
    pub framework: String,
    pub version: String,
    pub layer: Option<String>,
    pub owner: Option<String>,
    pub description: Option<String>,
}

/// Action for one selected rule.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum XccdfRuleImportAction {
    /// Import as a manual policy — the user must provide evidence of compliance.
    CreateManual { rule_id: String },
    /// Import as an unbound policy — exists in the bundle but has no implementation.
    CreateUnbound { rule_id: String },
    /// Import as an opaque policy — check content is preserved but not executed.
    PreserveOpaque { rule_id: String },
    /// Exclude the rule — create no policy or membership row.
    Exclude { rule_id: String },
}

impl XccdfRuleImportAction {
    pub fn rule_id(&self) -> &str {
        match self {
            Self::CreateManual { rule_id } => rule_id,
            Self::CreateUnbound { rule_id } => rule_id,
            Self::PreserveOpaque { rule_id } => rule_id,
            Self::Exclude { rule_id } => rule_id,
        }
    }

    pub fn is_exclude(&self) -> bool {
        matches!(self, Self::Exclude { .. })
    }

    /// The `implementation_state` value to store for non-excluded rules.
    pub fn implementation_state(&self) -> Option<&'static str> {
        match self {
            Self::CreateManual { .. } => Some("manual"),
            Self::CreateUnbound { .. } => Some("unbound"),
            Self::PreserveOpaque { .. } => Some("opaque"),
            Self::Exclude { .. } => None,
        }
    }
}

// ── Validated import plan ─────────────────────────────────────────────────────

/// The import plan after validation against the reparsed document.
///
/// Only non-excluded rules appear here, in their original document order.
pub struct ValidatedImportPlan {
    pub expected_sha256: String,
    pub bundle: ImportedBundlePlan,
    /// Non-excluded rules in document order, each paired with its action.
    pub rules_to_import: Vec<(ParsedRule, XccdfRuleImportAction)>,
}

// ── Validation errors ─────────────────────────────────────────────────────────

/// Structured validation failure for import plan checking.
///
/// All variants carry a machine-readable error code.
#[derive(Debug)]
pub struct ImportPlanError {
    pub code: &'static str,
    pub message: String,
}

impl ImportPlanError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn source_digest_invalid(value: &str) -> Self {
        Self::new(
            "SOURCE_DIGEST_INVALID",
            format!(
                "expected_sha256 {:?} is not a valid SHA-256 hex digest",
                value
            ),
        )
    }

    pub fn source_digest_mismatch(expected: &str, actual: &str) -> Self {
        Self::new(
            "SOURCE_DIGEST_MISMATCH",
            format!(
                "uploaded package digest {} does not match expected {}",
                actual, expected
            ),
        )
    }

    pub fn bundle_name_empty() -> Self {
        Self::new(
            "IMPORT_PLAN_INVALID",
            "bundle name must not be empty after trimming",
        )
    }

    pub fn bundle_version_empty() -> Self {
        Self::new(
            "IMPORT_PLAN_INVALID",
            "bundle version must not be empty after trimming",
        )
    }

    pub fn bundle_framework_empty() -> Self {
        Self::new(
            "IMPORT_PLAN_INVALID",
            "bundle framework must not be empty after trimming",
        )
    }

    pub fn rule_not_found(rule_id: &str) -> Self {
        Self::new(
            "IMPORT_RULE_NOT_FOUND",
            format!(
                "selected_rule_id {:?} does not exist in the document",
                rule_id
            ),
        )
    }

    pub fn rule_duplicate(rule_id: &str) -> Self {
        Self::new(
            "IMPORT_RULE_DUPLICATE",
            format!(
                "rule_id {:?} appears more than once in selected_rule_ids",
                rule_id
            ),
        )
    }

    pub fn action_missing(rule_id: &str) -> Self {
        Self::new(
            "IMPORT_RULE_ACTION_MISSING",
            format!("selected rule {:?} has no action in rule_actions", rule_id),
        )
    }

    pub fn action_duplicate(rule_id: &str) -> Self {
        Self::new(
            "IMPORT_RULE_ACTION_DUPLICATE",
            format!(
                "rule_id {:?} appears more than once in rule_actions",
                rule_id
            ),
        )
    }

    pub fn action_for_unselected(rule_id: &str) -> Self {
        Self::new(
            "IMPORT_RULE_ACTION_DUPLICATE",
            format!(
                "action for rule {:?} references a rule not in selected_rule_ids",
                rule_id
            ),
        )
    }

    pub fn profile_not_found(profile_id: &str) -> Self {
        Self::new(
            "IMPORT_PROFILE_NOT_FOUND",
            format!(
                "selected_profile_id {:?} does not exist in the document",
                profile_id
            ),
        )
    }

    pub fn rule_not_in_profile(rule_id: &str, profile_id: &str) -> Self {
        Self::new(
            "IMPORT_RULE_NOT_IN_PROFILE",
            format!(
                "rule {:?} is not selected by profile {:?}",
                rule_id, profile_id
            ),
        )
    }

    pub fn document_class_unsupported(class: &str) -> Self {
        Self::new(
            "IMPORT_DOCUMENT_CLASS_UNSUPPORTED",
            format!(
                "document class {:?} is not supported by this import slice; \
                 only foreign_xccdf and invalid_xccdf (with warnings) are accepted",
                class
            ),
        )
    }

    pub fn cf_native_invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(code, message)
    }
}

// ── Import result (outbound to caller) ────────────────────────────────────────

/// HTTP 201 response body for a successful committed import.
#[derive(Debug, Serialize)]
pub struct XccdfCommittedImportResult {
    pub source_artifact_id: Uuid,
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub created_policy_count: u32,
    pub excluded_rule_count: u32,
    pub created_policy_version_ids: Vec<Uuid>,
    pub source_sha256: String,
    pub bundle_semantic_digest: String,
    pub warnings: Vec<ImportWarning>,
}

/// A non-blocking warning emitted during import.
#[derive(Debug, Serialize)]
pub struct ImportWarning {
    pub code: String,
    pub message: String,
}

// ── Canonical imported policy record ─────────────────────────────────────────

/// The compact data needed to insert one imported policy and its first
/// draft version record.
///
/// All metadata preserved from the foreign source document.
#[derive(Debug, Clone)]
pub struct ImportedPolicyRecord {
    /// New portable lineage UUID.
    pub policy_id: Uuid,
    /// New portable version UUID.
    pub policy_version_id: Uuid,
    /// Rule ID from the foreign source document.
    pub source_rule_id: String,
    /// Rule order in the source document (0-based).
    pub source_rule_order: usize,
    /// `native`, `manual`, `unbound`, or `opaque`.
    pub implementation_state: String,
    /// Native policy type from the typed CF payload.
    pub policy_type: String,
    pub version: Option<String>,
    pub execution_phase: String,
    pub config: serde_json::Value,
    pub dependencies: serde_json::Value,
    pub enabled_by_default: bool,
    pub portable: bool,
    pub semantic_digest: Option<String>,
    pub selected: bool,
    pub policy_order: i32,
    /// Human-readable policy name from the rule title, falling back to the rule ID.
    pub name: String,
    pub description: Option<String>,
    /// Compliance metadata serialised for the `compliance_metadata` JSONB column.
    pub compliance_metadata: serde_json::Value,
    /// The full original opaque XML for `preserve_opaque` rules, when present.
    pub opaque_xml: Option<String>,
}

impl ImportedPolicyRecord {
    /// Build the canonical `compliance_metadata` JSONB value from parsed rule fields.
    pub fn build_compliance_metadata(rule: &ParsedRule) -> serde_json::Value {
        let identifiers: Vec<serde_json::Value> = rule
            .identifiers
            .iter()
            .map(|id| serde_json::json!({ "system": id.system, "value": id.value }))
            .collect();

        let references: Vec<serde_json::Value> = rule
            .references
            .iter()
            .map(|r| serde_json::json!({ "href": r.href, "title": r.title }))
            .collect();

        let checks: Vec<serde_json::Value> = rule
            .checks
            .iter()
            .map(|c| check_content_to_json(c))
            .collect();

        let fixes: Vec<serde_json::Value> =
            rule.fix.iter().map(|f| fix_content_to_json(f)).collect();

        serde_json::json!({
            "source_rule_id": rule.id,
            "source_group_id": rule.group_id,
            "severity": rule.severity,
            "rationale": rule.rationale,
            "version": rule.version,
            "platforms": rule.platforms,
            "identifiers": identifiers,
            "references": references,
            "checks": checks,
            "fixes": fixes,
        })
    }
}

fn check_content_to_json(c: &CheckContent) -> serde_json::Value {
    let body = match &c.body {
        CheckBody::Inline { content } => serde_json::json!({
            "kind": "inline",
            "content": content,
        }),
        CheckBody::Reference { href, name } => serde_json::json!({
            "kind": "reference",
            "href": href,
            "name": name,
        }),
    };
    serde_json::json!({
        "system": c.system,
        "selector": c.selector,
        "multi_check": c.multi_check,
        "negate": c.negate,
        "body": body,
    })
}

fn fix_content_to_json(f: &FixContent) -> serde_json::Value {
    serde_json::json!({
        "id": f.id,
        "system": f.system,
        "complexity": f.complexity,
        "disruption": f.disruption,
        "content": f.content,
    })
}
