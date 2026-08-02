//! Typed export models for XCCDF 1.2 bundle export.
//!
//! These models are the interface between the database snapshot loader and the
//! XML writer. They represent the complete set of fields needed to produce a
//! standards-valid CF-XCCDF document.

use serde_json::Value;
use uuid::Uuid;

use super::super::canonical::{ImplementationState, PublicationState};

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
#[derive(Debug, Clone)]
pub struct XccdfStandardCheck {
    pub system: String,
    pub body: XccdfCheckBody,
    pub selector: Option<String>,
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
        }
    }
}

/// Errors that can arise while parsing imported fix metadata.
#[derive(Debug)]
pub enum ImportedFixError {
    /// The `fix` object exists but `content` is missing or not a string.
    MissingContent,
    /// The `fix` object exists but `id` is present and not a valid NCName.
    InvalidId,
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
                "compliance_metadata.fix.id is present but empty"
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
            // when opaque_xml provides a fallback that readers can use.
            if self.opaque_xml.is_none() {
                return Err(ImportedCheckError::ReferenceOnlyWithoutFallback);
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

        Ok(Some(XccdfStandardCheck { system, body, selector }))
    }

    /// Parse the imported standard XCCDF fix from compliance metadata.
    ///
    /// Returns `None` when no `fix` key is present.
    /// Returns `Err(ImportedFixError)` when a `fix` object exists but is
    /// structurally invalid (missing content, empty id). Callers must propagate
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

        let id = value.get("id").and_then(|v| v.as_str()).map(str::to_owned);
        // Reject an empty id string — it produces invalid XCCDF.
        if let Some(ref id_str) = id {
            if id_str.is_empty() {
                return Err(ImportedFixError::InvalidId);
            }
        }

        Ok(Some(XccdfStandardFix {
            id,
            system: value
                .get("system")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            content,
            complexity: value
                .get("complexity")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            disruption: value
                .get("disruption")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
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
