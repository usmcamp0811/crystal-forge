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

#[derive(Debug, Clone)]
pub struct XccdfStandardCheck {
    pub system: String,
    pub content: Option<String>,
    pub content_ref_href: Option<String>,
    pub content_ref_name: Option<String>,
    pub selector: Option<String>,
}

#[derive(Debug, Clone)]
pub struct XccdfStandardFix {
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

impl XccdfPolicyExport {
    pub fn standard_check(&self) -> Option<XccdfStandardCheck> {
        let value = self.compliance_metadata.get("check")?;
        Some(XccdfStandardCheck {
            system: value.get("system")?.as_str()?.to_owned(),
            content: value
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            content_ref_href: value
                .get("content_ref_href")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            content_ref_name: value
                .get("content_ref_name")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            selector: value
                .get("selector")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
        })
    }

    pub fn standard_fix(&self) -> Option<XccdfStandardFix> {
        let value = self.compliance_metadata.get("fix")?;
        Some(XccdfStandardFix {
            system: value
                .get("system")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            content: value.get("content")?.as_str()?.to_owned(),
            complexity: value
                .get("complexity")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            disruption: value
                .get("disruption")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
        })
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
