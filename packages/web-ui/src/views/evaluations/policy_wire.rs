//! Mirrors the additive server matrix contract without resolving catalog state.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Contains identities and evidence from one represented evaluation attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalPolicyMatrixSnapshot {
    /// Identifies the captured attempt when available.
    pub evaluation_attempt_id: Option<Uuid>,
    /// Numbers the represented queue/history attempt.
    pub attempt_number: i32,
    /// States whether the represented attempt has isolated mock outcomes.
    /// This flag is independent of captured membership provenance and current
    /// server mode. False does not prove native evaluation; older APIs omit it.
    #[serde(default)]
    pub mock_evidence: bool,
    /// States whether historical memberships were captured.
    pub provenance: EvalMatrixProvenance,
    /// Lists exact eligible policy versions in cell order.
    pub columns: Vec<EvalPolicyColumn>,
    /// Lists captured bundle versions, including empty applicable contexts.
    pub bundles: Vec<EvalBundleColumn>,
    /// States whether all planned rows' bundle contexts were captured.
    /// Older captures remain false; absent contexts cannot prove nonassignment.
    pub empty_bundle_metadata_complete: bool,
    /// Retains planned systems even when evidence is missing.
    pub rows: Vec<EvalMatrixRow>,
}

/// Distinguishes historical capture from unavailable legacy provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalMatrixProvenance {
    /// Memberships were captured before execution.
    Captured,
    /// Historical memberships are unavailable and must not be reconstructed.
    LegacyUnavailable,
}

/// Names an exact captured policy version, not a framework requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalPolicyColumn {
    /// Keys outcomes and memberships by exact version.
    pub policy_version_id: Uuid,
    /// Identifies the policy lineage independently of its version.
    pub policy_id: Uuid,
    /// Contains the captured version-owned name.
    pub name: String,
    /// Contains the captured version label.
    pub version: String,
}

/// Describes an exact captured bundle version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalBundleColumn {
    /// Keys the immutable version.
    pub bundle_version_id: Uuid,
    /// Identifies the bundle lineage.
    pub bundle_id: Uuid,
    /// Contains the version-owned name.
    pub name: String,
    /// Contains the version label.
    pub version: String,
    /// Names the framework without asserting requirement compliance.
    pub framework: String,
    /// Labels the optional framework release.
    pub framework_version: Option<String>,
}

/// Separates per-system applicability from shared-configuration evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalMatrixRow {
    /// Identifies the historical system, absent on legacy rows.
    pub system_id: Option<Uuid>,
    /// Contains the captured system name.
    pub system_name: String,
    /// Keys the evaluator configuration and compatibility evidence.
    pub configuration_name: String,
    /// States whether applicability is authoritative.
    pub resolution: EvalMatrixResolution,
    /// Identifies the resolved executable and skipped report-only inputs.
    pub evaluation_digest: Option<String>,
    /// Contains applicable contexts, including bundles with no eligible controls.
    /// Context presence does not establish a policy contribution or a pass.
    #[serde(default)]
    pub assigned_bundles: Vec<EvalBundleMembership>,
    /// Contains per-system effective policy memberships.
    pub policies: Vec<EvalPolicyBinding>,
    /// Contains outcomes in snapshot column order.
    pub cells: Vec<EvalMatrixCell>,
    /// Contains matching-attempt result text in column/cell order.
    /// Unknown and nonapplicable cells have no detail. Older responses can omit
    /// the vector; absent or short vectors do not establish result evidence.
    #[serde(default)]
    pub details: Vec<Option<String>>,
}

/// Describes resolution independently of outcome availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalMatrixResolution {
    /// Applicability resolved successfully, including an empty set.
    Resolved,
    /// A semantic conflict prevented complete resolution.
    Conflict,
    /// Authoritative resolution is unavailable.
    Unavailable,
    /// A shared configuration conflict skipped evaluation.
    Skipped,
}

/// Records one effective policy's per-system applicability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalPolicyBinding {
    /// Identifies the exact effective policy version.
    pub policy_version_id: Uuid,
    /// States whether failures are evidence-only.
    pub report_only: bool,
    /// States whether the loader could execute the implementation.
    pub executable: bool,
    /// Contains authoritative assignment contributions only.
    pub memberships: Vec<EvalBundleMembership>,
}

/// Records an applicable bundle context or authoritative policy contribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalBundleMembership {
    /// Identifies the contributing bundle version.
    pub bundle_version_id: Uuid,
    /// Identifies the assignment lineage; absent for a genuine virtual baseline.
    pub assignment_id: Option<Uuid>,
    /// Identifies the assignment snapshot; absent for a genuine virtual baseline.
    pub assignment_version_id: Option<Uuid>,
    /// Distinguishes persisted assignments from resolver-created contexts.
    #[serde(default)]
    pub source: EvalBundleSource,
}

/// Describes how an authoritative bundle context entered the resolver.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalBundleSource {
    /// A persisted assignment with exact lineage and version UUIDs.
    #[default]
    Assignment,
    /// A captured published-version baseline without assignment UUIDs.
    VirtualBaseline,
}

/// Separates explicit nonapplicability from unavailable evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalMatrixCell {
    /// Matching-attempt evidence passed.
    Pass,
    /// Matching-attempt evidence produced a nonblocking failure.
    Warn,
    /// Matching-attempt evidence produced a blocking failure.
    Fail,
    /// Evidence is missing, stale, skipped, or invalid.
    Unknown,
    /// Successful resolution explicitly excludes this policy version.
    NotAssigned,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot_json(provenance: &str) -> serde_json::Value {
        json!({
            "evaluation_attempt_id": null,
            "attempt_number": 1,
            "provenance": provenance,
            "columns": [],
            "bundles": [],
            "empty_bundle_metadata_complete": false,
            "rows": []
        })
    }

    #[test]
    fn old_json_without_mock_evidence_defaults_false() {
        for provenance in ["captured", "legacy_unavailable"] {
            let snapshot: EvalPolicyMatrixSnapshot =
                serde_json::from_value(snapshot_json(provenance)).unwrap();
            assert!(!snapshot.mock_evidence);
        }
    }

    #[test]
    fn explicit_mock_evidence_survives_both_provenance_paths_and_roundtrip() {
        for provenance in ["captured", "legacy_unavailable"] {
            let mut json = snapshot_json(provenance);
            json["mock_evidence"] = json!(true);
            let snapshot: EvalPolicyMatrixSnapshot = serde_json::from_value(json).unwrap();
            assert!(snapshot.mock_evidence);
            let roundtrip: EvalPolicyMatrixSnapshot =
                serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap();
            assert_eq!(roundtrip, snapshot);
        }
    }

    #[test]
    fn explicit_false_remains_false_with_captured_or_legacy_provenance() {
        for provenance in ["captured", "legacy_unavailable"] {
            let mut json = snapshot_json(provenance);
            json["mock_evidence"] = json!(false);
            let snapshot: EvalPolicyMatrixSnapshot = serde_json::from_value(json).unwrap();
            assert!(!snapshot.mock_evidence);
        }
    }
}
