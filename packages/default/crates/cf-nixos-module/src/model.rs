//! Domain model shared by the generator's layers.

use std::fmt;

use cf_compliance::xccdf::inference::NixosLiteralValue;
use uuid::Uuid;

/// Where a policy version came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyOrigin {
    /// Display label of the input artifact, normally the input file name.
    pub input_label: String,
    /// SHA-256 of the exact input bytes the policy was read from.
    pub source_sha256: String,
    /// Set when the policy arrived through a compliance bundle export.
    pub bundle_version_id: Option<Uuid>,
}

/// One immutable policy version resolved from an input artifact.
///
/// Field names and defaults mirror the canonical Crystal Forge policy version
/// so that identity, digest, and eligibility decisions stay consistent with the
/// server.
#[derive(Debug, Clone)]
pub struct ResolvedPolicy {
    /// Portable lineage identity, stable across versions.
    pub policy_id: Uuid,
    /// Portable version identity, unique to this exact version.
    pub policy_version_id: Uuid,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub implementation_state: String,
    pub execution_phase: String,
    pub config: serde_json::Value,
    pub compliance_metadata: serde_json::Value,
    /// `cf-model-json-1` semantic digest, recomputed from the normalized fields.
    pub semantic_digest: String,
    pub origin: PolicyOrigin,
}

impl ResolvedPolicy {
    /// Stable human identity used in diagnostics and generated comments.
    pub fn label(&self) -> String {
        format!("{} @ {}", self.name, self.policy_version_id)
    }

    /// Compliance requirement identifiers preserved from the source export.
    ///
    /// These are metadata only and never affect generated configuration. A
    /// policy that maps to several requirements still emits one implementation.
    pub fn compliance_mappings(&self) -> Vec<String> {
        let mut mappings = Vec::new();

        if let Some(rule_id) = self
            .compliance_metadata
            .get("source_rule_id")
            .and_then(serde_json::Value::as_str)
        {
            mappings.push(format!("Source rule: {rule_id}"));
        }
        if let Some(stig_id) = self
            .compliance_metadata
            .get("stig_id")
            .and_then(serde_json::Value::as_str)
        {
            mappings.push(format!("STIG ID: {stig_id}"));
        }
        if let Some(severity) = self
            .compliance_metadata
            .get("severity")
            .and_then(serde_json::Value::as_str)
        {
            mappings.push(format!("Severity: {severity}"));
        }
        if let Some(identifiers) = self
            .compliance_metadata
            .get("identifiers")
            .and_then(serde_json::Value::as_array)
        {
            for identifier in identifiers {
                let system = identifier.get("system").and_then(serde_json::Value::as_str);
                let value = identifier.get("value").and_then(serde_json::Value::as_str);
                if let (Some(system), Some(value)) = (system, value) {
                    mappings.push(format!("{system}: {value}"));
                }
            }
        }

        // Deterministic and free of duplicates so generated comments and the
        // manifest are byte-stable.
        mappings.sort();
        mappings.dedup();
        mappings
    }
}

/// A bundle version export and the exact membership it froze.
#[derive(Debug, Clone)]
pub struct ResolvedBundle {
    pub bundle_id: Uuid,
    pub bundle_version_id: Uuid,
    pub name: String,
    pub version: String,
    pub framework: Option<String>,
    pub framework_version: Option<String>,
    pub publication_state: String,
    pub semantic_digest: Option<String>,
    pub source_sha256: String,
    pub input_label: String,
    /// Policy version IDs selected by this exact bundle version, in bundle order.
    pub selected_policy_version_ids: Vec<Uuid>,
}

/// A single NixOS option assignment recovered from a policy implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionAssignment {
    /// Dotted NixOS option path, for example `services.openssh.settings.PermitRootLogin`.
    pub option_path: String,
    pub value: NixosLiteralValue,
}

/// Why a policy could not be converted into a NixOS module.
///
/// Every variant is reported explicitly; a skipped policy is never emitted as
/// though it were implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The policy is not a Crystal Forge-executable implementation.
    NotNative { implementation_state: String },
    /// The policy type has no declarative NixOS option representation.
    UnsupportedPolicyType { policy_type: String },
    /// An `any`-mode check does not determine a single configuration.
    AmbiguousRuleMode { mode: String },
    /// The policy carries no assertion expression at all.
    NoImplementation,
    /// An expression is not a plain `config.<path> == <literal>` assertion.
    UnrepresentableExpression { expression: String },
    /// The policy assigns the same option two different values.
    SelfContradictory {
        option_path: String,
        first: String,
        second: String,
    },
}

impl SkipReason {
    /// Stable machine-readable code, used in the manifest.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotNative { .. } => "not_native",
            Self::UnsupportedPolicyType { .. } => "unsupported_policy_type",
            Self::AmbiguousRuleMode { .. } => "ambiguous_rule_mode",
            Self::NoImplementation => "no_implementation",
            Self::UnrepresentableExpression { .. } => "unrepresentable_expression",
            Self::SelfContradictory { .. } => "self_contradictory",
        }
    }
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotNative {
                implementation_state,
            } => write!(
                f,
                "{implementation_state} policy has no NixOS implementation"
            ),
            Self::UnsupportedPolicyType { policy_type } => write!(
                f,
                "unsupported implementation type '{policy_type}' has no declarative NixOS representation"
            ),
            Self::AmbiguousRuleMode { mode } => write!(
                f,
                "rule mode '{mode}' does not determine a single NixOS configuration"
            ),
            Self::NoImplementation => {
                write!(f, "policy declares no assertion expression to convert")
            }
            Self::UnrepresentableExpression { expression } => write!(
                f,
                "expression is not a plain NixOS option assertion: {expression}"
            ),
            Self::SelfContradictory {
                option_path,
                first,
                second,
            } => write!(f, "policy assigns {option_path} both {first} and {second}"),
        }
    }
}

/// A policy that was reported rather than generated.
#[derive(Debug, Clone)]
pub struct SkippedPolicy {
    pub policy: ResolvedPolicy,
    pub reason: SkipReason,
}

/// A policy that produced a generated NixOS module.
#[derive(Debug, Clone)]
pub struct GeneratedPolicy {
    pub policy: ResolvedPolicy,
    pub assignments: Vec<OptionAssignment>,
    /// Output-relative path of the generated file.
    pub generated_file: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(compliance_metadata: serde_json::Value) -> ResolvedPolicy {
        ResolvedPolicy {
            policy_id: Uuid::nil(),
            policy_version_id: Uuid::nil(),
            version: "1".into(),
            name: "p".into(),
            description: None,
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: serde_json::json!({}),
            compliance_metadata,
            semantic_digest: String::new(),
            origin: PolicyOrigin {
                input_label: "x".into(),
                source_sha256: String::new(),
                bundle_version_id: None,
            },
        }
    }

    #[test]
    fn compliance_mappings_are_sorted_and_deduplicated() {
        let p = policy(serde_json::json!({
            "stig_id": "SV-230221r1",
            "severity": "high",
            "identifiers": [
                {"system": "http://cyber.mil/cci", "value": "CCI-000366"},
                {"system": "http://cyber.mil/cci", "value": "CCI-000366"},
                {"system": "NIST 800-53 Rev 5", "value": "IA-5"},
            ],
        }));
        let mappings = p.compliance_mappings();
        assert_eq!(
            mappings,
            vec![
                "NIST 800-53 Rev 5: IA-5".to_string(),
                "STIG ID: SV-230221r1".to_string(),
                "Severity: high".to_string(),
                "http://cyber.mil/cci: CCI-000366".to_string(),
            ]
        );
    }

    #[test]
    fn compliance_mappings_are_empty_without_metadata() {
        assert!(
            policy(serde_json::json!({}))
                .compliance_mappings()
                .is_empty()
        );
    }

    #[test]
    fn skip_reason_codes_are_stable() {
        assert_eq!(SkipReason::NoImplementation.code(), "no_implementation");
        assert_eq!(
            SkipReason::NotNative {
                implementation_state: "manual".into()
            }
            .code(),
            "not_native"
        );
    }

    #[test]
    fn manual_skip_reason_reads_like_the_documented_diagnostic() {
        let reason = SkipReason::NotNative {
            implementation_state: "manual".into(),
        };
        assert_eq!(
            reason.to_string(),
            "manual policy has no NixOS implementation"
        );
    }
}
