//! Evaluation-time policy identities and separate applicability/outcome states.
//!
//! A control is an exact policy version, not a framework requirement. Bundle
//! membership exists only in per-system authoritative resolver contributions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Describes the captured plan and outcomes for one represented attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicyMatrixSnapshot {
    /// Identifies the immutable evaluation attempt, when recorded.
    pub evaluation_attempt_id: Option<Uuid>,
    /// Names the latest queue/history attempt in the same read snapshot.
    /// A queued retry can exceed the commit's started-attempt counter.
    pub attempt_number: i32,
    /// True when the represented attempt has isolated mock outcome records.
    /// Computed at read time, including mixed or ambiguous evidence and legacy
    /// attempts without a captured plan. False means no mock evidence was found;
    /// it does not prove native evaluation. Captures initialize this to false.
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
