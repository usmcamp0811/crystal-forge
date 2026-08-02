//! Typed export models for XCCDF 1.2 bundle export.
//!
//! These models are the interface between the database snapshot loader and the
//! XML writer. They represent the complete set of fields needed to produce a
//! standards-valid CF-XCCDF document.

use serde_json::Value;
use uuid::Uuid;

use super::super::canonical::{ImplementationState, PublicationState};

/// Valid XCCDF 1.2 `<fix>` complexity enumeration values.
pub(crate) const VALID_FIX_COMPLEXITY: &[&str] = &["unknown", "low", "medium", "high"];
/// Valid XCCDF 1.2 `<fix>` disruption enumeration values.
pub(crate) const VALID_FIX_DISRUPTION: &[&str] = &["unknown", "low", "medium", "high"];

/// Validate a string as a XCCDF NCName: `[A-Za-z_][\w.-]*`.
fn is_xccdf_ncname(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

/// Complete data for a single source-object mapping (standard identifiers).
#[derive(Debug, Clone)]
pub struct XccdfSourceMapping {
    pub object_kind: String,
    pub source_identity: String,
    pub fidelity: String,
}

/// The body of an imported XCCDF standard check — exactly one form is valid.
///
/// XCCDF 1.2 defines `<check-content-ref>` and `<check-content>` as exclusive
/// alternatives within a `<check>`. Both cannot coexist, and a ref name
/// without an href is also invalid.
#[derive(Debug, Clone)]
pub enum XccdfCheckBody {
    /// Inline check content. Contains the check text directly.
    Inline { content: String },
    /// External reference. `href` is required; `name` is optional.
    Reference { href: String, name: Option<String> },
}

/// A validated imported standard XCCDF check element.
///
/// Preserves every XCCDF 1.2 `<check>` attribute that affects evaluation
/// semantics. Unsupported attributes are captured in `unsupported_attrs`
/// rather than silently dropped, so the writer can emit a fidelity warning
/// or preserve them in opaque source content.
#[derive(Debug, Clone)]
pub struct XccdfStandardCheck {
    pub system: String,
    pub body: XccdfCheckBody,
    pub selector: Option<String>,
    /// XCCDF 1.2 `multi-check` attribute: when `"true"`, the check may
    /// produce multiple results (one per selector or target).
    pub multi_check: Option<bool>,
    /// XCCDF 1.2 `negate` attribute: when `"true"`, the check result is
    /// inverted (pass becomes fail and vice versa).
    pub negate: Option<bool>,
    /// Any standard check attributes not represented in the typed fields.
    /// Preserved for fidelity; the writer may emit a warning or embed them
    /// in opaque source content.
    pub unsupported_attrs: Vec<(String, String)>,
}

/// A validated imported standard XCCDF fix element.
#[derive(Debug, Clone)]
pub struct XccdfStandardFix {
    pub id: Option<String>,
    pub system: Option<String>,
    pub content: String,
    pub complexity: Option<String>,
    pub disruption: Option<String>,
}

/// Complete data for a single policy version in an export.
#[derive(Debug, Clone)]
pub struct XccdfPolicyExport {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub version: String,
    pub publication_state: PublicationState,
    pub semantic_digest: String,
    pub digest_algorithm: String,
    pub canonicalization_version: String,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub execution_phase: String,
    pub implementation_state: ImplementationState,
    pub enabled_default: bool,
    pub selected: bool,
    pub policy_order: i32,
    pub config: Value,
    pub compliance_metadata: Value,
    pub dependencies: Value,
    pub opaque_xml: Option<String>,
    pub source_mappings: Vec<XccdfSourceMapping>,
}

/// Errors that can arise while parsing imported check metadata.
#[derive(Debug)]
pub enum ImportedCheckError {
    /// The `check` object exists but `system` is missing or not a string.
    MissingSystem,
    /// Both inline content and a reference are present.
    AmbiguousBody,
    /// A content-ref name was given without an href.
    RefNameWithoutHref,
    /// Neither inline content nor a reference is present.
    EmptyBody,
    /// The check body is a reference-only form and no opaque_xml fallback is
    /// available. Standalone export requires self-contained check content.
    ReferenceOnlyWithoutFallback,
    /// The check body is a reference-only form and opaque_xml is present but
    /// contains only whitespace or a trivial placeholder. A self-contained
    /// fallback for the referenced check content is required.
    ReferenceFallbackInsufficient,
}

impl std::fmt::Display for ImportedCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSystem => write!(
                f,
                "compliance_metadata.check.system is missing or not a string"
            ),
            Self::AmbiguousBody => write!(
                f,
                "compliance_metadata.check has both inline content and a content-ref; \
                 XCCDF requires exactly one"
            ),
            Self::RefNameWithoutHref => write!(
                f,
                "compliance_metadata.check has content_ref_name without content_ref_href; \
                 XCCDF requires an href when a name is given"
            ),
            Self::EmptyBody => write!(
                f,
                "compliance_metadata.check has neither inline content nor a content-ref"
            ),
            Self::ReferenceOnlyWithoutFallback => write!(
                f,
                "compliance_metadata.check is a reference-only form \
                 (href without inline content) and no opaque_xml fallback is available; \
                 standalone XCCDF export requires self-contained check content"
            ),
            Self::ReferenceFallbackInsufficient => write!(
                f,
                "compliance_metadata.check is a reference-only form and opaque_xml is present \
                 but contains only whitespace or a trivial placeholder; a self-contained \
                 fallback for the referenced check content is required"
            ),
        }
    }
}

/// Errors that can arise while parsing imported fix metadata.
#[derive(Debug)]
pub enum ImportedFixError {
    /// The `fix` object exists but `content` is missing or not a string.
    MissingContent,
    /// The fix `id` is present but not a valid XCCDF NCName.
    /// NCName: [A-Za-z_][\w.-]* (no whitespace, must start with letter or _).
    InvalidId,
    /// The fix `complexity` is present but not a valid XCCDF enumeration value.
    InvalidComplexity(String),
    /// The fix `disruption` is present but not a valid XCCDF enumeration value.
    InvalidDisruption(String),
    /// The fix `system` is present but empty or not a string.
    InvalidSystem,
}

impl std::fmt::Display for ImportedFixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingContent => write!(
                f,
                "compliance_metadata.fix.content is missing or not a string"
            ),
            Self::InvalidId => write!(
                f,
                "compliance_metadata.fix.id is present but is not a valid XCCDF NCName \
                 (must match [A-Za-z_][\\w.-]*)"
            ),
            Self::InvalidComplexity(val) => write!(
                f,
                "compliance_metadata.fix.complexity is {val:?} which is not a valid XCCDF \
                 enumeration; expected one of: unknown, low, medium, high"
            ),
            Self::InvalidDisruption(val) => write!(
                f,
                "compliance_metadata.fix.disruption is {val:?} which is not a valid XCCDF \
                 enumeration; expected one of: unknown, low, medium, high"
            ),
            Self::InvalidSystem => write!(
                f,
                "compliance_metadata.fix.system is present but empty or not a string"
            ),
        }
    }
}

impl XccdfPolicyExport {
    /// Parse the imported standard XCCDF check from compliance metadata.
    ///
    /// Returns `None` when no `check` key is present.
    /// Returns `Err(ImportedCheckError)` when a `check` object exists but is
    /// structurally invalid. Callers must propagate errors rather than silently
    /// replacing with a synthesized check.
    ///
    /// A reference-only check body is rejected unless `opaque_xml` is present
    /// to provide a self-contained fallback for standalone artifact consumers.
    pub fn parse_standard_check(&self) -> Result<Option<XccdfStandardCheck>, ImportedCheckError> {
        let value = match self.compliance_metadata.get("check") {
            Some(v) => v,
            None => return Ok(None),
        };

        let system = value
            .get("system")
            .and_then(|v| v.as_str())
            .ok_or(ImportedCheckError::MissingSystem)?
            .to_owned();

        let has_inline = value
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let has_href = value
            .get("content_ref_href")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        let has_name = value
            .get("content_ref_name")
            .and_then(|v| v.as_str())
            .is_some();

        let body = if has_inline && has_href {
            return Err(ImportedCheckError::AmbiguousBody);
        } else if has_name && !has_href {
            return Err(ImportedCheckError::RefNameWithoutHref);
        } else if has_inline {
            let content = value
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_owned();
            XccdfCheckBody::Inline { content }
        } else if has_href {
            // A reference-only check is not self-contained. It is valid only
            // when opaque_xml provides a meaningful fallback that readers can
            // use. An empty, whitespace-only, or trivial placeholder string
            // does not make the referenced content resolvable.
            match &self.opaque_xml {
                None => {
                    return Err(ImportedCheckError::ReferenceOnlyWithoutFallback);
                }
                Some(xml) if xml.trim().is_empty() || xml.len() < 20 => {
                    return Err(ImportedCheckError::ReferenceFallbackInsufficient);
                }
                _ => {}
            }
            let href = value
                .get("content_ref_href")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_owned();
            let name = value
                .get("content_ref_name")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            XccdfCheckBody::Reference { href, name }
        } else {
            return Err(ImportedCheckError::EmptyBody);
        };

        let selector = value
            .get("selector")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        let multi_check = value
            .get("multi-check")
            .and_then(|v| v.as_str())
            .map(|s| s.eq_ignore_ascii_case("true"));

        let negate = value
            .get("negate")
            .and_then(|v| v.as_str())
            .map(|s| s.eq_ignore_ascii_case("true"));

        // Capture any standard XCCDF check attributes not represented in the
        // typed fields. Known keys are excluded; the rest are preserved for
        // fidelity so the writer can emit a warning or embed them in opaque
        // source content.
        let known_keys: &[&str] = &[
            "system",
            "content",
            "content_ref_href",
            "content_ref_name",
            "selector",
            "multi-check",
            "negate",
        ];
        let mut unsupported_attrs = Vec::new();
        if let Some(obj) = value.as_object() {
            for (key, val) in obj {
                if !known_keys.contains(&key.as_str()) {
                    if let Some(s) = val.as_str() {
                        unsupported_attrs.push((key.clone(), s.to_owned()));
                    }
                }
            }
        }

        Ok(Some(XccdfStandardCheck {
            system,
            body,
            selector,
            multi_check,
            negate,
            unsupported_attrs,
        }))
    }

    /// Parse the imported standard XCCDF fix from compliance metadata.
    ///
    /// Returns `None` when no `fix` key is present.
    /// Returns `Err(ImportedFixError)` when a `fix` object exists but is
    /// structurally invalid. All fields are validated against XCCDF 1.2
    /// schema requirements before being accepted. Callers must propagate
    /// errors rather than silently replacing with a synthesized fix.
    pub fn parse_standard_fix(
        &self,
    ) -> Result<Option<XccdfStandardFix>, ImportedFixError> {
        let value = match self.compliance_metadata.get("fix") {
            Some(v) => v,
            None => return Ok(None),
        };

        let content = value
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or(ImportedFixError::MissingContent)?
            .to_owned();

        // Validate id as a XCCDF NCName: [A-Za-z_][\w.-]*
        let id = value.get("id").and_then(|v| v.as_str()).map(str::to_owned);
        if let Some(ref id_str) = id {
            if !is_xccdf_ncname(id_str) {
                return Err(ImportedFixError::InvalidId);
            }
        }

        // Validate system is a non-empty string if present.
        let system = match value.get("system").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Some(s.to_owned()),
            Some(_) => return Err(ImportedFixError::InvalidSystem),
            None => None,
        };

        // Validate complexity against XCCDF 1.2 enumeration.
        let complexity = match value.get("complexity").and_then(|v| v.as_str()) {
            Some(c) if VALID_FIX_COMPLEXITY.contains(&c) => Some(c.to_owned()),
            Some(c) => return Err(ImportedFixError::InvalidComplexity(c.to_owned())),
            None => None,
        };

        // Validate disruption against XCCDF 1.2 enumeration.
        let disruption = match value.get("disruption").and_then(|v| v.as_str()) {
            Some(d) if VALID_FIX_DISRUPTION.contains(&d) => Some(d.to_owned()),
            Some(d) => return Err(ImportedFixError::InvalidDisruption(d.to_owned())),
            None => None,
        };

        Ok(Some(XccdfStandardFix {
            id,
            system,
            content,
            complexity,
            disruption,
        }))
    }
}

/// A recursively ordered XCCDF group projection. `source_id` preserves an
/// imported foreign identifier; `generated_id` is always the NCName-safe ID
/// emitted into the XCCDF 1.2 document.
#[derive(Debug, Clone)]
pub struct XccdfGroupExport {
    pub generated_id: String,
    pub source_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub order: i32,
    pub children: Vec<XccdfGroupExport>,
    pub policies: Vec<Uuid>,
}

/// Complete data for a bundle version export.
///
/// This is the single input to the XML writer. It contains everything needed
/// to generate a valid XCCDF 1.2 Benchmark with CF extensions.
#[derive(Debug, Clone)]
pub struct XccdfBundleExport {
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub version: String,
    pub publication_state: PublicationState,
    pub semantic_digest: String,
    pub digest_algorithm: String,
    pub canonicalization_version: String,
    pub name: String,
    pub description: Option<String>,
    pub framework: String,
    pub framework_version: Option<String>,
    pub layer: String,
    pub owner: String,
    pub groups: Vec<XccdfGroupExport>,
    pub policies: Vec<XccdfPolicyExport>,
}

impl XccdfBundleExport {
    /// Stable benchmark ID derived from the bundle-version UUID.
    pub fn benchmark_id(&self) -> String {
        format!(
            "xccdf_crystalforge_benchmark_{}",
            self.bundle_version_id.simple()
        )
    }

    /// Stable baseline profile ID.
    pub fn profile_id(&self) -> String {
        format!(
            "xccdf_crystalforge_profile_{}_baseline",
            self.bundle_version_id.simple()
        )
    }
}

impl XccdfPolicyExport {
    /// Stable rule ID derived from the policy-version UUID.
    pub fn rule_id(&self) -> String {
        format!(
            "xccdf_crystalforge_rule_{}",
            self.policy_version_id.simple()
        )
    }
}

/// Errors produced when building the XCCDF group tree from policy metadata.
#[derive(Debug)]
pub enum GroupProjectionError {
    /// A cycle was detected that does not originate from any root node.
    CycleNotFromRoot(String),
    /// A non-root group node has a `parent_source_id` that does not exist and
    /// has no directly-assigned policies, making it an empty orphan.
    EmptyOrphan { group_source_id: String },
}

impl std::fmt::Display for GroupProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CycleNotFromRoot(key) => {
                write!(
                    f,
                    "cycle detected in group parent graph not reachable from a root: {key}"
                )
            }
            Self::EmptyOrphan { group_source_id } => {
                write!(
                    f,
                    "group {group_source_id} has no matching parent and no assigned policies"
                )
            }
        }
    }
}
