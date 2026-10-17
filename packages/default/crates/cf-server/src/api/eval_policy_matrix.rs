//! Evaluation-time policy identities and separate applicability/outcome states.
//!
//! A control is an exact policy version, not a framework requirement. Bundle
//! membership exists only in per-system authoritative resolver contributions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Describes the captured plan and outcomes for one represented attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicyMatrixSnapshot {
    /// Display identity copied at policy-plan capture, never reconstructed for
    /// historical records from the current registry. Older captures omit it.
    #[serde(default)]
    pub source_label: Option<EvalSourceLabel>,
    /// Exact-attempt post-discovery inventory, projected to current visibility.
    /// None means discovery is pending or unavailable, never an empty inventory.
    /// This extension is read-only and is not part of the pre-execution plan.
    #[serde(default)]
    pub inventory: Option<EvalAttemptInventory>,
    /// Configuration-level gate coverage for the visible selected inventory.
    /// Absent until exact-attempt discovery exists. This does not count controls.
    #[serde(default)]
    pub configuration_summary: Option<EvalConfigurationSummary>,
    /// Exact-attempt global gate failures, not assigned policy-version failures.
    /// Entries require complete authorization to the configuration-wide evidence.
    #[serde(default)]
    pub configuration_failures: Vec<EvalConfigurationFailure>,
    /// Identifies the immutable evaluation attempt, when recorded.
    pub evaluation_attempt_id: Option<Uuid>,
    /// Names the selected queue/history attempt in the same read snapshot.
    /// A queued retry can exceed the commit's started-attempt counter.
    pub attempt_number: i32,
    /// Exact commit SHA projected at read time; captures initialize None.
    #[serde(default)]
    pub commit_sha: Option<String>,
    /// Selected attempt's persisted status, with cancelling for the current
    /// latest attempt when commit cancellation is in progress. The wire value
    /// queued is the attempt state that the UI presents as Pending.
    /// Absent when no attempt exists; captures initialize None.
    #[serde(default)]
    pub attempt_status: Option<String>,
    /// Selected attempt's actual start time; queued attempts can have None.
    /// This read-time context is not part of the immutable capture.
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    /// Selected attempt's actual completion time, without inferred timestamps.
    /// This read-time context is not part of the immutable capture.
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    /// True when visible evidence includes isolated mock outcome records.
    /// Unrestricted reads include legacy attempts without a captured plan.
    /// Restricted reads require an authorized retained captured row, including
    /// mixed or ambiguous evidence. False does not exclude hidden mock records
    /// or prove native evaluation. Captures initialize this to false.
    #[serde(default)]
    pub mock_evidence: bool,
    /// Distinguishes recorded provenance from pre-capture evaluations.
    pub provenance: EvalMatrixProvenance,
    /// Exact eligible policy versions, deduplicated across planned rows.
    pub columns: Vec<EvalPolicyColumn>,
    /// Exact bundle versions of applicable contexts, including empty bundles.
    pub bundles: Vec<EvalBundleColumn>,
    /// True when all planned rows' assignment contexts were captured.
    /// Older captures and legacy rows retain false; absence cannot prove empty.
    pub empty_bundle_metadata_complete: bool,
    /// Planned rows survive missing, failed, or stale derivation results.
    pub rows: Vec<EvalMatrixRow>,
}

/// Preserves the watched source's display identity at capture time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSourceLabel {
    /// Flake display name at capture, not a generated reference-fixture label.
    pub name: String,
    /// Watched branch at capture; None does not imply a branch named main.
    pub branch: Option<String>,
}

/// Lists verified configuration names discovered by the selected attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalAttemptInventory {
    /// Native discovery or explicitly synthetic mock inventory.
    pub execution_mode: String,
    /// Sorted distinct visible discovered configuration identities.
    pub configurations: Vec<String>,
    /// Visible discovered identities selected for execution; not proof of success.
    pub selected_configurations: Vec<String>,
}

/// Counts distinct configuration gates, without duplicating shared host rows.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvalConfigurationSummary {
    /// Distinct visible planned or discovered configurations for this attempt.
    pub total: usize,
    /// Selected configurations with an unambiguous successful gate result.
    pub passed: usize,
    /// Selected configurations with an unambiguous failed policy/agent gate.
    pub failed: usize,
    /// Selected configurations with an authoritative Nix evaluation failure.
    pub evaluation_failed: usize,
    /// Selected configurations without a terminal result while the current
    /// active attempt can still complete their evaluation.
    pub pending: usize,
    /// Configurations with unavailable terminal evidence, denied requirement
    /// coverage, or a planned name absent from verified discovery.
    #[serde(default)]
    pub unavailable: usize,
    /// Discovered configurations deliberately excluded from execution selection.
    pub excluded: usize,
}

/// Reports a genuine configuration-level global gate failure without policy IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfigurationFailure {
    /// Identifies the actual configuration gate, without claiming a policy ID.
    #[serde(default)]
    pub kind: EvalConfigurationFailureKind,
    /// Exact discovered configuration key, not a fabricated system UUID.
    pub configuration_name: String,
    /// Saved failure detail from the matching-attempt global CF-agent result.
    /// Empty when the false gate is authoritative but no diagnostic was saved.
    pub detail: String,
}

/// Distinguishes the unconditional agent gate from unattributed requirement gates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalConfigurationFailureKind {
    /// The authoritative CF-agent configuration gate is false.
    #[default]
    CfAgent,
    /// The configuration requirement gate failed without a provable control cause.
    Requirements,
}

/// Describes whether historical assignment provenance was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalMatrixProvenance {
    /// The plan was persisted before evaluator execution.
    Captured,
    /// No plan exists; current catalog state must not fill the gap.
    LegacyUnavailable,
}

/// Identifies one immutable policy version without executable configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicyColumn {
    /// Exact version identity used as the control key.
    pub policy_version_id: Uuid,
    /// Portable policy lineage identity.
    pub policy_id: Uuid,
    /// Name copied from the referenced version at capture.
    pub name: String,
    /// Version label copied at capture.
    pub version: String,
    /// Declared identifiers and trusted exact-version requirement external IDs.
    /// These labels do not add controls or assert requirement compliance.
    #[serde(default)]
    pub identifiers: Vec<String>,
    /// Human-readable description from the exact policy version.
    #[serde(default)]
    pub description: Option<String>,
    /// Safe declarative assertion text, when an authoritative display source exists.
    /// Current capture has no such source and records None, never Nix expressions.
    #[serde(default)]
    pub assertion: Option<String>,
    /// Declared option path, when an authoritative display source exists.
    /// Current capture records None rather than inspecting executable config.
    #[serde(default)]
    pub attribute_path: Option<String>,
}

/// Identifies an exact bundle version without configuration or source payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalBundleColumn {
    /// Exact bundle version identity.
    pub bundle_version_id: Uuid,
    /// Portable bundle lineage identity.
    pub bundle_id: Uuid,
    /// Version-owned display name.
    pub name: String,
    /// Version-owned version label.
    pub version: String,
    /// Version-owned framework name; not a compliance assertion.
    pub framework: String,
    /// Version-owned optional framework release label.
    pub framework_version: Option<String>,
}

/// Records a planned system independently of its shared evaluator configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalMatrixRow {
    /// Explicit operator classification at capture: true is greenfield, false
    /// is established, and None is unknown. Used only for display annotation;
    /// never changes applicability, outcomes, counts, or deployment policy.
    /// Earlier captures remain unknown and are never enriched from current data.
    #[serde(default)]
    pub greenfield: Option<bool>,
    /// Historical system identity; absent for legacy derivation-only rows.
    pub system_id: Option<Uuid>,
    /// Hostname at capture, or configuration name for legacy rows.
    pub system_name: String,
    /// Exact evaluator configuration key.
    pub configuration_name: String,
    /// Whether an empty applicability set is authoritative.
    pub resolution: EvalMatrixResolution,
    /// Digest of the actual sorted executable and skipped report-only inputs.
    /// Absent when the loader could not produce a complete configuration plan.
    pub evaluation_digest: Option<String>,
    /// Applicable bundle contexts, including bundles with zero eligible controls.
    /// Context presence does not claim any policy passed or survived precedence.
    #[serde(default)]
    pub assigned_bundles: Vec<EvalBundleMembership>,
    /// Per-system applicability, never a configuration-wide union.
    pub policies: Vec<EvalPolicyBinding>,
    /// One outcome per column, in column order; populated at read time.
    pub cells: Vec<EvalMatrixCell>,
    /// Result detail strings parallel to columns/cells, populated at read time.
    /// Only applicable executable checks with valid matching-attempt outcomes
    /// supply details. Unavailable and nonapplicable cells have no detail.
    #[serde(default)]
    pub details: Vec<Option<String>>,
}

/// Describes applicability resolution independently of result availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalMatrixResolution {
    /// Resolution succeeded, including a genuinely empty eligible policy set.
    Resolved,
    /// The resolver returned a semantic conflict; gates were skipped as before.
    Conflict,
    /// No authoritative resolution was available.
    Unavailable,
    /// Another system's conflict skipped this shared configuration's gates.
    Skipped,
}

/// Records one policy's applicability and exact source memberships on one row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicyBinding {
    /// Exact effective policy version, including skipped report-only checks.
    pub policy_version_id: Uuid,
    /// True when failures are evidence-only and cannot block deployment.
    pub report_only: bool,
    /// False for an invalid report-only implementation skipped by the loader.
    pub executable: bool,
    /// Authoritative contexts only; superseded versions never create edges.
    pub memberships: Vec<EvalBundleMembership>,
}

/// Identifies an applicable bundle context or authoritative policy contribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalBundleMembership {
    /// Exact applicable bundle version; policy bindings require an authoritative
    /// contribution, while assigned contexts can contain no eligible controls.
    pub bundle_version_id: Uuid,
    /// Assignment lineage identity; absent only for a genuine virtual baseline.
    pub assignment_id: Option<Uuid>,
    /// Exact immutable snapshot; absent only for a genuine virtual baseline.
    pub assignment_version_id: Option<Uuid>,
    /// Distinguishes persisted assignments from resolver-created virtual contexts.
    #[serde(default)]
    pub source: EvalBundleSource,
}

/// Describes how a bundle context entered the authoritative resolver.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalBundleSource {
    /// A persisted assignment with exact lineage and version UUIDs.
    #[default]
    Assignment,
    /// A current-published version baseline with no persisted assignment UUIDs.
    VirtualBaseline,
}

/// Separates explicit nonapplicability from missing or invalid evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalMatrixCell {
    /// A matching-attempt check passed.
    Pass,
    /// A matching-attempt check failed without blocking enforcement.
    Warn,
    /// A matching-attempt enforce-mode check failed and blocks enforcement.
    Fail,
    /// Evidence is absent, stale, skipped, malformed, or resolution is incomplete.
    Unknown,
    /// Successful complete resolution explicitly excludes this policy version.
    NotAssigned,
}

#[cfg(test)]
mod display_contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn earlier_capture_display_fields_are_explicitly_unknown() {
        let column: EvalPolicyColumn = serde_json::from_value(json!({
            "policy_version_id": Uuid::new_v4(), "policy_id": Uuid::new_v4(),
            "name": "V-12345 is a name, not an identifier", "version": "1"
        }))
        .unwrap();
        assert!(column.identifiers.is_empty());
        assert!(column.description.is_none());
        assert!(column.assertion.is_none());
        assert!(column.attribute_path.is_none());
        let row: EvalMatrixRow = serde_json::from_value(json!({
            "system_id": null, "system_name": "greenfield", "configuration_name": "new",
            "resolution": "resolved", "evaluation_digest": null, "policies": [], "cells": []
        }))
        .unwrap();
        assert_eq!(row.greenfield, None);
        assert_eq!(
            serde_json::to_value(row).unwrap()["greenfield"],
            json!(null)
        );
    }
}
