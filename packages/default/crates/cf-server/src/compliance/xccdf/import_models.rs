//! Typed models for the XCCDF committed import flow.
//!
//! Separate from the export models and the parser models. These represent
//! the HTTP contract for the import endpoint and the intermediate state
//! used during plan validation and canonical record construction.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::compliance::xccdf::models::{CheckContent, FixContent, ParsedRule};

// Make serde derive available for all local structs
#[allow(unused_imports)]
use serde::{self};

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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ImportedPolicyCustomization {
    pub policy_name: Option<String>,
    pub policy_description: Option<String>,
    pub implementation_note: Option<String>,
    #[serde(default)]
    pub policy_severity: Option<String>,
    #[serde(default)]
    pub policy_rationale: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImportedCustomCheck {
    #[serde(default)]
    pub mode: String,
    pub rules: Vec<ImportedCustomCheckRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImportedCustomCheckRule {
    pub field_name: String,
    pub expression: String,
    pub description: String,
    #[serde(default = "default_true")]
    pub strict: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportedEvidenceRequirement {
    Command {
        command: String,
        expected_output: String,
    },
    File {
        path: String,
        expected_content: String,
    },
    UnitState {
        unit: String,
        state: String,
    },
    Log {
        source: String,
        unit: Option<String>,
        pattern: String,
    },
    Attestation {
        description: String,
    },
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

/// Proof/justification for reusing an existing policy via MapExisting.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MapExistingProof {
    /// The requirement unchanged from prior release; there is a trusted mapping.
    InheritedMapping,
    /// Exact normalized technical enforcement match discovered at preview time.
    ExactTechnicalMatch,
}

/// Action for one selected rule.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum XccdfRuleImportAction {
    CreateNativeCustom {
        rule_id: String,
        customization: ImportedPolicyCustomization,
        custom_check: ImportedCustomCheck,
        evidence_requirements: Vec<ImportedEvidenceRequirement>,
    },
    /// Import as a manual policy — the user must provide evidence of compliance.
    CreateManual {
        rule_id: String,
        #[serde(default)]
        customization: ImportedPolicyCustomization,
        #[serde(default)]
        evidence_requirements: Vec<ImportedEvidenceRequirement>,
    },
    /// Import as an unbound policy — exists in the bundle but has no implementation.
    CreateUnbound {
        rule_id: String,
        #[serde(default)]
        customization: ImportedPolicyCustomization,
    },
    /// Import as an opaque policy — check content is preserved but not executed.
    PreserveOpaque {
        rule_id: String,
        #[serde(default)]
        customization: ImportedPolicyCustomization,
    },
    /// Reuse an exact immutable local policy version while preserving the
    /// imported rule metadata and source mapping.
    MapExisting {
        rule_id: String,
        policy_version_id: Uuid,
        /// Explicit proof/justification for this reuse decision.
        #[serde(default)]
        proof: Option<MapExistingProof>,
    },
    /// Exclude the rule — create no policy or membership row.
    Exclude { rule_id: String },
}

impl XccdfRuleImportAction {
    pub fn rule_id(&self) -> &str {
        match self {
            Self::CreateNativeCustom { rule_id, .. } => rule_id,
            Self::CreateManual { rule_id, .. } => rule_id,
            Self::CreateUnbound { rule_id, .. } => rule_id,
            Self::PreserveOpaque { rule_id, .. } => rule_id,
            Self::MapExisting { rule_id, .. } => rule_id,
            Self::Exclude { rule_id } => rule_id,
        }
    }

    pub fn is_exclude(&self) -> bool {
        matches!(self, Self::Exclude { .. })
    }

    /// The `implementation_state` value to store for non-excluded rules.
    pub fn implementation_state(&self) -> Option<&'static str> {
        match self {
            Self::CreateNativeCustom { .. } => Some("native"),
            Self::CreateManual { .. } => Some("manual"),
            Self::CreateUnbound { .. } => Some("unbound"),
            Self::PreserveOpaque { .. } => Some("opaque"),
            Self::Exclude { .. } => None,
            Self::MapExisting { .. } => Some("mapped"),
        }
    }

    pub fn customization(&self) -> Option<&ImportedPolicyCustomization> {
        match self {
            Self::CreateNativeCustom { customization, .. }
            | Self::CreateManual { customization, .. }
            | Self::CreateUnbound { customization, .. }
            | Self::PreserveOpaque { customization, .. } => Some(customization),
            Self::MapExisting { .. } | Self::Exclude { .. } => None,
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
    /// Whether the foreign benchmark identifies DISA as its publisher. This is
    /// source evidence for the conservative DISA STIG classification projection.
    pub is_disa_stig: bool,
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

    pub fn native_check_invalid(rule_id: &str, message: impl Into<String>) -> Self {
        Self::new(
            "IMPORT_NATIVE_CHECK_INVALID",
            format!("rule {:?}: {}", rule_id, message.into()),
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
    pub created_policy_lineages: u32,
    pub created_policy_versions: u32,
    pub reused_policy_versions: u32,
    pub bundle_lineage_created: bool,
    pub bundle_version_created: bool,
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
    /// Set for MapExisting actions. No new local policy lineage is created.
    pub mapped_policy_version_id: Option<Uuid>,
    pub evidence_requirements: Vec<ImportedEvidenceRequirement>,
}

impl ImportedPolicyRecord {
    /// Build the canonical `compliance_metadata` JSONB value from parsed rule fields.
    pub fn build_compliance_metadata(rule: &ParsedRule, is_disa_stig: bool) -> serde_json::Value {
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

        // Derive curated normalised mapping arrays from structured identifiers.
        // These supplement (do not replace) the generic `identifiers` array.
        // Only values that begin with the canonical prefix are included; prose
        // text such as VulnDiscussion is never scanned here.
        let srg_ids: Vec<&str> = rule
            .identifiers
            .iter()
            .map(|id| id.value.as_str())
            .filter(|v| v.to_ascii_uppercase().starts_with("SRG-"))
            .collect();
        let cci_ids: Vec<&str> = rule
            .identifiers
            .iter()
            .map(|id| id.value.as_str())
            .filter(|v| v.to_ascii_uppercase().starts_with("CCI-"))
            .collect();

        let mut metadata = serde_json::json!({
            "source_rule_id": rule.id,
            "source_group_id": rule.group_id,
            "version": rule.version,
            "platforms": rule.platforms,
            "identifiers": identifiers,
            "references": references,
            "checks": checks,
            "fixes": fixes,
            "srg_ids": srg_ids,
            "cci_ids": cci_ids,
        });
        if let Some(object) = metadata.as_object_mut() {
            if is_disa_stig {
                object.insert("category".into(), serde_json::json!("security"));
                object.insert("framework".into(), serde_json::json!("DISA STIG"));
            }
            // `severity` and `rationale` are standard XCCDF fields. Do not add a
            // null placeholder when the source did not provide them, and do not
            // infer either field from discussion, check, or fix prose.
            if let Some(severity) = rule.severity.as_deref().filter(|value| !value.is_empty()) {
                object.insert("severity".into(), serde_json::json!(severity));
            }
            if let Some(rationale) = rule.rationale.as_deref().filter(|value| !value.is_empty()) {
                object.insert("rationale".into(), serde_json::json!(rationale));
            }
        }

        metadata
    }
}

fn check_content_to_json(c: &CheckContent) -> serde_json::Value {
    let body_parts: Vec<serde_json::Value> = c
        .body_parts
        .iter()
        .map(|part| match part {
            crate::compliance::xccdf::models::CheckBodyPart::Inline { content } => {
                serde_json::json!({
                    "type": "inline",
                    "content": content,
                })
            }
            crate::compliance::xccdf::models::CheckBodyPart::Reference { href, name } => {
                serde_json::json!({
                    "type": "reference",
                    "href": href,
                    "name": name,
                })
            }
        })
        .collect();
    serde_json::json!({
        "system": c.system,
        "selector": c.selector,
        "multi_check": c.multi_check,
        "negate": c.negate,
        "body_parts": body_parts,
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
