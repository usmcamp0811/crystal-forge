//! Canonical digest helpers for compliance requirement versions, and typed
//! structs for the requirement reconciliation pipeline.
//!
//! Follows the same `'pending'` sentinel + Rust-authoritative-compute pattern
//! established by [`super::digest`] for policy and bundle versions.
//!
//! # Canonical field set for a requirement version
//!
//! ```text
//! canonicalization_version, canonical_requirement_key, check_text,
//! description, external_id, fix_text, kind, metadata, severity, title
//! ```
//!
//! `parent_requirement_version_id` is intentionally excluded — it is a
//! structural/relational field within a release, not part of the requirement's
//! own semantic content.

use anyhow::{Context, Result};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use super::canonical::semantic_digest;

// ── Canonical DTO ─────────────────────────────────────────────────────────────

/// All semantic fields for a compliance requirement version digest.
#[derive(Debug, Clone)]
pub struct RequirementVersionCanonical {
    /// Stable key within the framework, e.g. `"V-268137"` or `"SC-45"`.
    pub canonical_requirement_key: String,
    /// Release-specific identifier, e.g. XCCDF Rule id.
    pub external_id: String,
    /// Human-readable title.
    pub title: Option<String>,
    /// Prose description of the requirement.
    pub description: Option<String>,
    /// Framework-specific node type, e.g. `"rule"`, `"control"`, `"family"`.
    pub kind: String,
    /// Severity level if applicable, e.g. `"high"`, `"medium"`, `"low"`.
    pub severity: Option<String>,
    /// Full check/verification text from the source document.
    pub check_text: Option<String>,
    /// Full fix/remediation text from the source document.
    pub fix_text: Option<String>,
    /// Framework-specific supplementary metadata (CCI IDs, SRG IDs, refs, …).
    pub metadata: Value,
}

/// Normalized DISA identifiers used as evidence when discovering related
/// policy candidates. These remain metadata-derived evidence, not a second
/// identifier store or an equivalence proof.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatedRequirementIdentifiers {
    pub cci_ids: BTreeSet<String>,
    pub srg_ids: BTreeSet<String>,
}

impl RelatedRequirementIdentifiers {
    pub fn from_metadata(metadata: &Value) -> Self {
        Self {
            cci_ids: normalized_metadata_ids(metadata, "cci_ids"),
            srg_ids: normalized_metadata_ids(metadata, "srg_ids"),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cci_ids.is_empty() && self.srg_ids.is_empty()
    }
}

fn normalized_metadata_ids(metadata: &Value, field: &str) -> BTreeSet<String> {
    metadata
        .get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .collect()
}

impl RequirementVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        json!({
            "canonicalization_version": "cf-model-json-1",
            "canonical_requirement_key": self.canonical_requirement_key,
            "check_text": self.check_text.as_deref().unwrap_or(""),
            "description": self.description.as_deref().unwrap_or(""),
            "external_id": self.external_id,
            "fix_text": self.fix_text.as_deref().unwrap_or(""),
            "kind": self.kind,
            "metadata": self.metadata,
            "severity": self.severity.as_deref().unwrap_or(""),
            "title": self.title.as_deref().unwrap_or(""),
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

// ── DB writer ─────────────────────────────────────────────────────────────────

/// Write the `cf-model-json-1` digest for a requirement version within `tx`.
///
/// Must be called immediately after the `INSERT` while the transaction is still
/// open.
pub async fn write_requirement_version_digest(
    tx: &mut Transaction<'_, Postgres>,
    requirement_version_id: Uuid,
    canonical: &RequirementVersionCanonical,
) -> Result<()> {
    let digest = canonical.compute_digest();
    sqlx::query(
        "UPDATE compliance_requirement_versions \
         SET semantic_digest = $1 \
         WHERE id = $2",
    )
    .bind(&digest)
    .bind(requirement_version_id)
    .execute(&mut **tx)
    .await
    .context("failed to write requirement version semantic digest")?;
    Ok(())
}

// ── Reconciliation types ──────────────────────────────────────────────────────

/// How an imported requirement relates to existing DB state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementReconciliationState {
    /// Same lineage, same semantic digest — nothing to do.
    ExistingUnchanged,
    /// Same lineage, different semantic digest — requirement was updated in this release.
    ExistingChanged,
    /// New canonical key not seen before in this framework.
    NewRequirement,
    /// Present in the previous release but absent from this import.
    RemovedFromRelease,
    /// A different lineage already owns this canonical key in the DB —
    /// requires explicit conflict resolution before commit is permitted.
    IdentityConflict,
}

/// Preview of how a single imported requirement reconciles with existing state.
#[derive(Debug, Clone)]
pub struct RequirementReconciliation {
    /// The canonical key from the import adapter.
    pub canonical_requirement_key: String,
    /// The external ID as it appears in the source document.
    pub external_id: String,
    pub state: RequirementReconciliationState,
    /// ID of the existing requirement lineage (if any).
    pub existing_requirement_id: Option<Uuid>,
    /// ID of the existing requirement version for this framework release (if any).
    pub existing_requirement_version_id: Option<Uuid>,
    /// Semantic digest of the existing version (for change detection).
    pub existing_digest: Option<String>,
}

/// Complete release-diff projection for an imported requirement set.
#[derive(Debug, Clone)]
pub struct RequirementReconciliationPreview {
    /// Reconciliation entries for requirements present in the incoming artifact.
    pub requirements: Vec<RequirementReconciliation>,
    /// Requirement versions in the previous release which are absent from the
    /// incoming artifact. Historical rows remain immutable; this is a preview
    /// classification only.
    pub removed_requirements: Vec<RequirementReconciliation>,
}

/// How an imported framework release reconciles with existing DB state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameworkReconciliationState {
    /// The exact artifact (same SHA-256) was already imported — full reuse.
    ExactArtifact,
    /// Same canonical_release_key but different artifact — existing release.
    ExistingRelease,
    /// New release not seen before for this framework.
    NewRelease,
    /// Same canonical_release_key but different semantic content — conflict.
    ReleaseConflict,
    /// This framework lineage has never been imported.
    NewFramework,
}

/// Preview of how an imported STIG/framework artifact reconciles with existing state.
#[derive(Debug, Clone)]
pub struct FrameworkReconciliation {
    pub state: FrameworkReconciliationState,
    /// Canonical source key determined by the adapter.
    pub canonical_source_key: String,
    /// Canonical release key determined by the adapter.
    pub canonical_release_key: String,
    /// Existing framework lineage ID (if any).
    pub existing_framework_id: Option<Uuid>,
    /// Existing framework version ID (if any).
    pub existing_framework_version_id: Option<Uuid>,
}

/// Candidate policy implementation for a requirement.
#[derive(Debug, Clone)]
pub struct PolicyCandidate {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub match_type: PolicyCandidateMatchType,
    /// 0–100 confidence score.
    pub confidence: u8,
    pub match_reasons: Vec<String>,
}

/// How a candidate policy was matched to a requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyCandidateMatchType {
    /// An authoritative `policy_requirement_mappings` row already exists.
    AuthoritativeMapping,
    /// An authoritative mapping exists on an unchanged previous requirement version.
    InheritedMapping,
    /// Exact normalized technical implementation (config hash) match.
    ExactTechnicalMatch,
    /// Cross-framework mapping via a shared CCI/SRG reference.
    RelatedMapping,
    /// Title/description similarity (informational only; never auto-accepted).
    FuzzySimilarity,
}

/// Server-computed reconciliation result for one requirement.
#[derive(Debug, Clone)]
pub struct PolicyReconciliation {
    pub requirement_reconciliation: RequirementReconciliation,
    /// Ordered candidates, highest confidence first.
    pub candidates: Vec<PolicyCandidate>,
    /// Whether the server can auto-resolve this requirement (no human review needed).
    pub auto_resolvable: bool,
}

/// Only deterministic evidence may resolve an import without review.
pub fn candidates_are_auto_resolvable(
    candidates: &[PolicyCandidate],
    inferred_enforcement: bool,
) -> bool {
    inferred_enforcement
        || candidates.iter().any(|candidate| {
            matches!(
                candidate.match_type,
                PolicyCandidateMatchType::AuthoritativeMapping
                    | PolicyCandidateMatchType::InheritedMapping
                    | PolicyCandidateMatchType::ExactTechnicalMatch
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(match_type: PolicyCandidateMatchType) -> PolicyCandidate {
        PolicyCandidate {
            policy_id: Uuid::nil(),
            policy_version_id: Uuid::nil(),
            policy_name: "test".to_string(),
            match_type,
            confidence: 70,
            match_reasons: vec![],
        }
    }

    #[test]
    fn related_metadata_identifiers_are_normalized_and_exact() {
        let ids = RelatedRequirementIdentifiers::from_metadata(&json!({
            "cci_ids": [" cci-000770", "CCI-000770", "CCI-000771"],
            "srg_ids": ["srg-os-000109-gpos-00051"]
        }));
        assert_eq!(
            ids.cci_ids.into_iter().collect::<Vec<_>>(),
            ["CCI-000770", "CCI-000771"]
        );
        assert_eq!(
            ids.srg_ids.into_iter().collect::<Vec<_>>(),
            ["SRG-OS-000109-GPOS-00051"]
        );
    }

    #[test]
    fn related_candidates_require_review() {
        assert!(!candidates_are_auto_resolvable(
            &[candidate(PolicyCandidateMatchType::RelatedMapping)],
            false
        ));
        assert!(candidates_are_auto_resolvable(
            &[candidate(PolicyCandidateMatchType::ExactTechnicalMatch)],
            false
        ));
        assert!(candidates_are_auto_resolvable(&[], true));
    }
}
