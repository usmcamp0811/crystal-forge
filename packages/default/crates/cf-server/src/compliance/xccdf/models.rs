//! Typed XCCDF 1.2 and CF-XCCDF extension data structures.
//!
//! These are the canonical server representations used by the parser, export
//! writer, preview API, and import API. They intentionally do NOT mirror XML
//! element layout — they represent the compliance domain model.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Document-level types ─────────────────────────────────────────────────────

/// Classification of an uploaded XCCDF document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentClass {
    /// Crystal Forge authored, exact match to known extension version.
    CfNativeExact,
    /// Crystal Forge authored with unsupported extension content.
    CfNativeUnsupportedExtension,
    /// Standard XCCDF or STIG from a third party.
    ForeignXccdf,
    /// Not valid XCCDF.
    InvalidXccdf,
    /// Not an XCCDF file (e.g., a ZIP without a recognised XCCDF inside).
    UnsupportedPackage,
}

/// Fidelity of a parsed document relative to Crystal Forge's model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    NativeExact,
    NormalizedComplete,
    PreservedOpaque,
    Degraded,
}

/// Top-level parsed XCCDF document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedXccdf {
    pub class: DocumentClass,
    pub fidelity: Fidelity,
    pub fidelity_losses: Vec<String>,
    pub source_filename: Option<String>,
    pub source_bytes: Vec<u8>,
    pub source_sha256: String,
    /// XCCDF namespace version detected in the source document: `"1.1"` or `"1.2"`.
    pub xccdf_namespace_version: Option<&'static str>,
    pub xccdf_version: Option<String>,
    pub benchmark: Option<BenchmarkMeta>,
    pub profiles: Vec<ParsedProfile>,
    pub rules: Vec<ParsedRule>,
    pub groups: Vec<ParsedGroup>,
    pub values: Vec<ParsedValue>,
    pub cf_bundle_meta: Option<CfBundleMeta>,
    pub signature_info: Option<SignatureInfo>,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

// ── Benchmark ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMeta {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub status: Option<String>,
    pub status_date: Option<String>,
    pub platforms: Vec<String>,
    pub publisher: Option<String>,
    pub references: Vec<Reference>,
}

// ── Profile ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedProfile {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub select_ids: Vec<String>,
    pub extends_id: Option<String>,
    pub is_abstract: bool,
    pub is_baseline: bool,
}

// ── Rule ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedRule {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub rationale: Option<String>,
    pub severity: Option<String>,
    pub weight: Option<f64>,
    pub version: Option<String>,
    pub checks: Vec<CheckContent>,
    pub fix: Option<FixContent>,
    pub identifiers: Vec<StandardIdentifier>,
    pub references: Vec<Reference>,
    pub platforms: Vec<String>,
    pub group_id: Option<String>,
    pub rule_order: Option<usize>,
    /// CF-native policy metadata, when detected.
    pub cf_policy_meta: Option<CfPolicyMeta>,
    /// Preserved unknown XML for fidelity.
    pub preserved_xml: Option<String>,
}

// ── Check / Fix ───────────────────────────────────────────────────────────────

/// The body of an XCCDF check — exactly one form is valid.
///
/// XCCDF 1.2 defines `<check-content-ref>` and `<check-content>` as exclusive
/// alternatives within a `<check>`. Both cannot coexist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckBody {
    /// Inline check content. Contains the check text directly.
    Inline { content: String },
    /// External reference. `href` is required; `name` is optional.
    Reference { href: String, name: Option<String> },
}

/// A validated XCCDF check element.
///
/// Preserves every XCCDF 1.2 `<check>` attribute that affects evaluation
/// semantics: `system`, `selector`, `multi-check`, and `negate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckContent {
    pub system: String,
    pub body: CheckBody,
    pub selector: Option<String>,
    /// XCCDF 1.2 `multi-check` attribute: when true, the check may
    /// produce multiple results (one per selector or target).
    pub multi_check: Option<bool>,
    /// XCCDF 1.2 `negate` attribute: when true, the check result is
    /// inverted (pass becomes fail and vice versa).
    pub negate: Option<bool>,
}

/// A validated XCCDF fix element.
///
/// Preserves `id`, `system`, `complexity`, `disruption`, and body content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixContent {
    /// XCCDF fix identifier (NCName).
    pub id: Option<String>,
    pub system: Option<String>,
    pub content: String,
    pub complexity: Option<String>,
    pub disruption: Option<String>,
}

// ── Identifiers and references ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardIdentifier {
    pub system: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub href: Option<String>,
    pub title: Option<String>,
}

// ── Group ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedGroup {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub rule_ids: Vec<String>,
}

// ── Value ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedValue {
    pub id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub value_type: String,
    pub default_value: Option<String>,
    pub allowed_values: Vec<String>,
}

// ── CF-native metadata ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfBundleMeta {
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub schema_version: Option<String>,
    pub publication_state: String,
    pub framework: Option<String>,
    pub framework_version: Option<String>,
    pub layer: Option<String>,
    pub owner: Option<String>,
    pub digest: Option<String>,
    pub digest_algorithm: Option<String>,
    pub canonicalization_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfPolicyMeta {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub publication_state: String,
    pub enabled_default: Option<bool>,
    pub selected: Option<bool>,
    pub policy_order: Option<i32>,
    pub implementation_state: Option<String>,
    pub version: Option<String>,
    pub execution_phase: Option<String>,
    pub strict: Option<bool>,
    pub policy_type: Option<String>,
    pub config: Option<serde_json::Value>,
    pub compliance_metadata: Option<serde_json::Value>,
    pub dependencies: Option<serde_json::Value>,
    pub digest: Option<String>,
    pub digest_algorithm: Option<String>,
    pub canonicalization_version: Option<String>,
}

// ── Signature ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub present: bool,
    pub algorithm: Option<String>,
    pub verified: Option<bool>,
    pub signer: Option<String>,
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub summary: String,
    pub field: Option<String>,
    pub xml_line: Option<u64>,
    pub xml_column: Option<u64>,
    pub object_identity: Option<String>,
    pub blocking: bool,
    pub remediation: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, summary: &str) -> Self {
        Self {
            code: code.to_string(),
            summary: summary.to_string(),
            field: None,
            xml_line: None,
            xml_column: None,
            object_identity: None,
            blocking: true,
            remediation: None,
        }
    }

    pub fn warning(code: &str, summary: &str) -> Self {
        Self {
            code: code.to_string(),
            summary: summary.to_string(),
            field: None,
            xml_line: None,
            xml_column: None,
            object_identity: None,
            blocking: false,
            remediation: None,
        }
    }
}

// ── Implementation-state classification ───────────────────────────────────────

/// Predicted implementation state for an imported rule before user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredictedState {
    Native,
    Manual,
    External,
    Unbound,
    Opaque,
}
