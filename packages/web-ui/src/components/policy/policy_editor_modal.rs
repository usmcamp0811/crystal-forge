//! Policy editor modal for creating and editing policy definitions.
//!
//! This modal mirrors the design example `PolicyFormModal`: a single unified
//! create/edit modal (no Basic/Advanced toggle and no raw JSON/TOML editor) with
//! metadata, category, severity, rationale, an assertions/gate-rules builder, an
//! evidence-for-ATO builder, and an edit-mode danger zone with typed-confirmation
//! delete.
//!
//! The deployment-policy API persists classification and evidence specifications
//! with each policy version.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{
    ApiClientError, create_deployment_policy, create_policy_mapping, delete_deployment_policy,
    delete_policy_mapping, fetch_compliance_framework_versions, fetch_compliance_frameworks,
    fetch_policy_requirement_mappings, search_nixos_options, search_requirements,
    update_deployment_policy, update_policy_mapping,
};
use crate::api::models::{
    ComplianceFrameworkSummary, ComplianceFrameworkVersionSummary, CreateDeploymentPolicyRequest,
    CreatePolicyMappingRequest, EvidenceKind, EvidenceSpec, NixosOptionMetadata, PolicyMappingRow,
    RequirementVersionSummary, UpdateDeploymentPolicyRequest, UpdatePolicyMappingRequest,
};
use crate::views::policies_api;

use super::types::{
    POLICY_CATEGORIES, PolicyCategory, PolicyDefinition, PolicyFormat, is_policy_version_editable,
    off_category_rule_kinds, recommended_enforcement,
};

#[derive(Clone, Debug, PartialEq)]
struct PendingPolicyMapping {
    requirement_version_id: Uuid,
    framework_name: String,
    framework_version: String,
    requirement_external_id: String,
    requirement_kind: String,
    requirement_title: Option<String>,
    relationship: String,
    coverage: String,
    rationale: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MappingEditorTarget {
    Pending,
    Persisted(Uuid),
    Unavailable,
}

fn mapping_editor_target(
    is_editing: bool,
    editing_policy_version_id: Option<Uuid>,
    mappings_editable: bool,
) -> MappingEditorTarget {
    if !is_editing {
        MappingEditorTarget::Pending
    } else if mappings_editable {
        editing_policy_version_id
            .map(MappingEditorTarget::Persisted)
            .unwrap_or(MappingEditorTarget::Unavailable)
    } else {
        MappingEditorTarget::Unavailable
    }
}

fn pending_mapping_from_selection(
    framework: &ComplianceFrameworkSummary,
    framework_version: &ComplianceFrameworkVersionSummary,
    requirement: &RequirementVersionSummary,
    relationship: String,
    coverage: String,
    rationale: Option<String>,
) -> PendingPolicyMapping {
    PendingPolicyMapping {
        requirement_version_id: requirement.id,
        framework_name: framework.name.clone(),
        framework_version: framework_version.version.clone(),
        requirement_external_id: requirement.external_id.clone(),
        requirement_kind: requirement.kind.clone(),
        requirement_title: requirement.title.clone(),
        relationship,
        coverage,
        rationale,
    }
}

impl PendingPolicyMapping {
    fn mapping_request(&self) -> CreatePolicyMappingRequest {
        CreatePolicyMappingRequest {
            requirement_version_id: self.requirement_version_id,
            relationship: self.relationship.clone(),
            coverage: self.coverage.clone(),
            rationale: self.rationale.clone(),
            provenance: "manual".to_string(),
        }
    }
}

fn add_pending_mapping(
    mappings: &mut Vec<PendingPolicyMapping>,
    mapping: PendingPolicyMapping,
) -> Result<(), &'static str> {
    if mappings
        .iter()
        .any(|existing| existing.requirement_version_id == mapping.requirement_version_id)
    {
        return Err("This requirement is already mapped.");
    }
    mappings.push(mapping);
    Ok(())
}

fn remove_pending_mapping(mappings: &mut Vec<PendingPolicyMapping>, requirement_version_id: Uuid) {
    mappings.retain(|mapping| mapping.requirement_version_id != requirement_version_id);
}

const STANDARD_FRAMEWORKS: [&str; 4] = ["DISA STIG", "NIST 800-53", "CMMC 2.0", "CIS Benchmark"];
const NIST_CONTROL_FAMILIES: [&str; 7] = ["AC", "AU", "CM", "IA", "SC", "SI", "MP"];
const MAPPING_RELATIONSHIPS: [(&str, &str, &str); 3] = [
    (
        "implements",
        "Implements",
        "The policy directly satisfies this requirement.",
    ),
    (
        "supports",
        "Supports",
        "The policy contributes to satisfying the requirement but does not satisfy it alone.",
    ),
    (
        "provides_evidence_for",
        "Provides evidence for",
        "The policy gathers or produces evidence relevant to determining compliance.",
    ),
];

// ─────────────────────────────────────────────────────────────────────────────
// Rule + evidence model (mirrors the design example)
// ─────────────────────────────────────────────────────────────────────────────

/// A single assertion / gate rule in the builder.
///
/// `persisted` indicates whether this rule kind can currently be encoded into the
/// real policy API `config` payload. Non-persisted kinds are still shown so the
/// modal matches the design, but are flagged in the UI.
#[derive(Clone, Debug, PartialEq)]
struct PolicyRule {
    /// Persisted identity. New IDs are allocated only by `new`, when the user
    /// adds a rule, and are thereafter carried through every state transition.
    id: Uuid,
    kind: String,
    // cve_block
    severity: String,
    max_allowed: String,
    // time_window
    from: String,
    to: String,
    days: String,
    // approval_required
    count: String,
    role: String,
    // rollout_percent
    percent: String,
    observe_min: String,
    // packages_installed
    packages: String,
    // nixos_option
    path: String,
    op: String,
    value: serde_json::Value,
    option_type: String,
    option_values: Vec<String>,
    option_description: String,
    option_unit: Option<String>,
    baseline_option_type: Option<String>,
    // custom_eval
    expr: String,
    message: String,
}

impl PolicyRule {
    fn new(kind: &str) -> Self {
        Self::new_with_id(kind, Uuid::new_v4())
    }

    fn new_with_id(kind: &str, id: Uuid) -> Self {
        let mut rule = Self {
            id,
            kind: kind.to_string(),
            severity: "critical".to_string(),
            max_allowed: "0".to_string(),
            from: "09:00".to_string(),
            to: "17:00".to_string(),
            days: "mon,tue,wed,thu,fri".to_string(),
            count: "2".to_string(),
            role: "admin".to_string(),
            percent: "25".to_string(),
            observe_min: "30".to_string(),
            packages: "openssh, auditd".to_string(),
            path: "services.openssh.settings.PermitRootLogin".to_string(),
            op: "==".to_string(),
            value: serde_json::Value::String("no".to_string()),
            option_type: "unknown".to_string(),
            option_values: Vec::new(),
            option_description: String::new(),
            option_unit: None,
            baseline_option_type: None,
            expr: "config.services.openssh.enable == true".to_string(),
            message: "SSH must be enabled".to_string(),
        };
        if kind == "packages_installed" {
            rule.packages = "openssh, auditd".to_string();
        }
        rule
    }

    fn apply_option_metadata(&mut self, metadata: &NixosOptionMetadata) {
        self.path = metadata.path.clone();
        self.option_type = metadata.value_type.as_str().to_string();
        self.enrich_option_metadata(metadata);
        self.op = "==".to_string();
        self.value = default_option_value(&self.option_type, &self.option_values);
    }

    /// Attach authoring guidance without changing persisted target semantics.
    fn enrich_option_metadata(&mut self, metadata: &NixosOptionMetadata) {
        self.option_values = metadata
            .enum_values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect();
        self.option_description = metadata.description.clone().unwrap_or_default();
        self.option_unit = None;
        self.baseline_option_type = Some(metadata.value_type.as_str().to_string());
    }

    fn baseline_advisory(&self) -> Option<String> {
        let baseline_type = self.baseline_option_type.as_deref()?;
        let stored_type = normalize_option_type(&self.option_type);
        if stored_type != normalize_option_type(baseline_type) {
            return Some(format!(
                "This policy keeps the target-specific {stored_type} semantics. Crystal Forge's pinned nixpkgs baseline currently describes this option as {baseline_type}; target evaluation remains authoritative."
            ));
        }
        if stored_type == "enum"
            && !self.option_values.is_empty()
            && self.value.as_str().is_some_and(|value| {
                !self
                    .option_values
                    .iter()
                    .any(|candidate| candidate == value)
            })
        {
            return Some(
                "This policy keeps a target-specific enum value that is not listed by Crystal Forge's pinned nixpkgs baseline; target evaluation remains authoritative."
                    .to_string(),
            );
        }
        None
    }

    /// Whether this rule kind can be persisted via the existing policy API config.
    fn is_persisted(&self) -> bool {
        rule_kind_is_persisted(&self.kind)
    }
}

fn normalize_option_type(option_type: &str) -> &str {
    match option_type {
        "bool" | "boolean" => "boolean",
        "enum" => "enum",
        "int" | "integer" => "integer",
        "str" | "string" => "string",
        "lines" => "lines",
        _ => "unknown",
    }
}

fn default_option_value(option_type: &str, values: &[String]) -> serde_json::Value {
    match normalize_option_type(option_type) {
        "boolean" => serde_json::Value::Bool(false),
        "integer" => serde_json::json!(0),
        "enum" => serde_json::Value::String(values.first().cloned().unwrap_or_default()),
        _ => serde_json::Value::String(String::new()),
    }
}

/// A single evidence-for-ATO source. None of these persist yet (no backend).
#[derive(Clone, Debug, PartialEq)]
struct PolicyEvidence {
    kind: String,
    cmd: String,
    expect: String,
    source: String,
    unit: String,
    r#match: String,
    path: String,
    note: String,
    state: String,
    attr: String,
    /// Preserved from original EvidenceSpec to prevent round-trip destruction
    required_fields: std::collections::HashMap<String, String>,
}

impl PolicyEvidence {
    fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            cmd: "sshd -T | grep permitrootlogin".to_string(),
            expect: "permitrootlogin no".to_string(),
            source: "journald".to_string(),
            unit: "auditd.service".to_string(),
            r#match: "audit: rules loaded".to_string(),
            path: "/etc/issue".to_string(),
            note: "Must contain USG banner text".to_string(),
            state: "active".to_string(),
            attr: "config.services.openssh.settings.PermitRootLogin".to_string(),
            required_fields: std::collections::HashMap::new(),
        }
    }

    /// Convert EvidenceSpec to editable PolicyEvidence form.
    /// Handles all evidence kinds and reconstructs form fields for editing.
    fn from_evidence_spec(spec: &EvidenceSpec) -> Self {
        let mut evidence = match &spec.kind {
            EvidenceKind::Command { cmd, expect } => Self {
                kind: "command".to_string(),
                cmd: cmd.clone(),
                expect: expect.clone(),
                ..Self::new("command")
            },
            EvidenceKind::Log {
                source,
                unit,
                match_text,
            } => Self {
                kind: "log".to_string(),
                source: source.clone(),
                unit: unit.clone(),
                r#match: match_text.clone(),
                ..Self::new("log")
            },
            EvidenceKind::File { path, note } => Self {
                kind: "file".to_string(),
                path: path.clone(),
                note: note.clone().unwrap_or_default(),
                ..Self::new("file")
            },
            EvidenceKind::UnitState { unit, state } => Self {
                kind: "unit_state".to_string(),
                unit: unit.clone(),
                state: state.clone(),
                ..Self::new("unit_state")
            },
            EvidenceKind::EvalAttr { attr } => Self {
                kind: "eval_attr".to_string(),
                attr: attr.clone(),
                ..Self::new("eval_attr")
            },
            EvidenceKind::Attestation { note } => Self {
                kind: "attestation".to_string(),
                note: note.clone(),
                ..Self::new("attestation")
            },
        };
        // Preserve required_fields metadata from original spec
        evidence.required_fields = spec.required_fields.clone();
        evidence
    }

    /// Validate this evidence row and return error message if invalid.
    /// Returns None if valid, Some(error_msg) if invalid.
    fn validate(&self) -> Option<String> {
        match self.kind.as_str() {
            "command" => {
                if self.cmd.is_empty() {
                    return Some("Command is required".to_string());
                }
                if self.expect.is_empty() {
                    return Some("Expected output is required".to_string());
                }
            }
            "log" => {
                if self.unit.is_empty() {
                    return Some("Unit/source is required".to_string());
                }
                if self.r#match.is_empty() {
                    return Some("Match pattern is required".to_string());
                }
            }
            "file" => {
                if self.path.is_empty() {
                    return Some("File path is required".to_string());
                }
            }
            "unit_state" => {
                if self.unit.is_empty() {
                    return Some("Unit is required".to_string());
                }
                if self.state.is_empty() {
                    return Some("State is required".to_string());
                }
            }
            "eval_attr" => {
                if self.attr.is_empty() {
                    return Some("Attribute path is required".to_string());
                }
            }
            "attestation" => {
                if self.note.is_empty() {
                    return Some("Attestation text is required".to_string());
                }
            }
            _ => return Some(format!("Unknown evidence kind: {}", self.kind)),
        }
        None
    }

    /// Convert PolicyEvidence to EvidenceSpec for the API.
    /// Does NOT validate - call validate() first.
    /// Preserves required_fields metadata loaded from original spec.
    fn to_evidence_spec(&self) -> EvidenceSpec {
        let kind = match self.kind.as_str() {
            "command" => EvidenceKind::Command {
                cmd: self.cmd.clone(),
                expect: self.expect.clone(),
            },
            "log" => EvidenceKind::Log {
                source: self.source.clone(),
                unit: self.unit.clone(),
                match_text: self.r#match.clone(),
            },
            "file" => EvidenceKind::File {
                path: self.path.clone(),
                note: if self.note.is_empty() {
                    None
                } else {
                    Some(self.note.clone())
                },
            },
            "unit_state" => EvidenceKind::UnitState {
                unit: self.unit.clone(),
                state: self.state.clone(),
            },
            "eval_attr" => EvidenceKind::EvalAttr {
                attr: self.attr.clone(),
            },
            "attestation" => EvidenceKind::Attestation {
                note: self.note.clone(),
            },
            _ => EvidenceKind::Attestation {
                note: "invalid".to_string(),
            },
        };
        EvidenceSpec {
            kind,
            required_fields: self.required_fields.clone(),
        }
    }
}

const RULE_OPTIONS: [(&str, &str, bool); 9] = [
    ("packages_installed", "Packages installed", true),
    ("nixos_option", "NixOS option equals", true),
    ("custom_eval", "Custom nix expression", true),
    ("eval_passed", "Eval must pass", false),
    ("build_succeeded", "Build must succeed", false),
    ("cve_block", "CVE gate", true),
    ("time_window", "Time window", false),
    ("approval_required", "Approval required", false),
    ("rollout_percent", "Canary rollout", false),
];

/// Whether a rule kind can be persisted by Phase 2.
///
/// This is the single source of truth for persistence capability, derived from
/// the third field of `RULE_OPTIONS`. The Add Rule control, the per-rule
/// `is_persisted` check, and the category recommendations all read it so the
/// three cannot drift. Unknown kinds fail closed as non-persistable.
fn rule_kind_is_persisted(kind: &str) -> bool {
    RULE_OPTIONS
        .iter()
        .find(|(id, _, _)| *id == kind)
        .is_some_and(|(_, _, persisted)| *persisted)
}

/// Category recommendations filtered to the kinds Phase 2 can actually persist.
///
/// `recommended_enforcement` stays the full conceptual model; this narrows it so
/// the editor never suggests a rule type it cannot save.
fn actionable_recommended_enforcement(category: PolicyCategory) -> Vec<&'static str> {
    recommended_enforcement(category)
        .iter()
        .copied()
        .filter(|kind| rule_kind_is_persisted(kind))
        .collect()
}

const EVIDENCE_OPTIONS: [(&str, &str); 6] = [
    ("command", "Command output"),
    ("log", "Log line match"),
    ("file", "File contents"),
    ("unit_state", "systemd unit state"),
    ("eval_attr", "Nix eval attribute"),
    ("attestation", "Signed attestation"),
];

#[derive(Clone, Copy, PartialEq)]
enum PolicyEditorTab {
    Details,
    Mappings,
    Enforcement,
    Evidence,
    /// Read-only imported origin. Only rendered when the policy has
    /// authoritative provenance recorded at import time.
    Provenance,
}

fn rule_label(kind: &str) -> &'static str {
    RULE_OPTIONS
        .iter()
        .find(|(id, _, _)| *id == kind)
        .map(|(_, label, _)| *label)
        .unwrap_or("Rule")
}

// ─────────────────────────────────────────────────────────────────────────────
// Payload mapping (UI rules → real API config)
// ─────────────────────────────────────────────────────────────────────────────

/// Build the persisted `(policy_type, config)` from the persistable rules.
/// Empty remains the Phase-2 zero-enforcement legacy representation; every
/// authored rule set uses the versioned composite representation.
fn build_persisted_payload(rules: &[PolicyRule]) -> Option<(String, serde_json::Value)> {
    let persistable: Vec<&PolicyRule> = rules.iter().filter(|r| r.is_persisted()).collect();
    if persistable.is_empty() && !rules.is_empty() {
        return None;
    }
    if persistable.is_empty() {
        // "No enforcement" is a valid, persistable policy state.
        //
        // The canonical representation is an explicit empty `custom_check` rule
        // set. The server validator accepts exactly this shape, and the
        // evaluator's record parser skips it, so a policy that claims no
        // enforcement really does assert nothing at runtime — it never becomes
        // an always-pass check. Only fields this editor can round-trip are
        // emitted, so the policy can be reopened and saved again unchanged.
        return Some((
            "custom_check".to_string(),
            serde_json::json!({ "mode": "all", "rules": [] }),
        ));
    }

    let json_rules = persistable
        .into_iter()
        .map(rule_to_composite_entry)
        .collect::<Option<Vec<_>>>()?;
    Some((
        "composite".to_string(),
        serde_json::json!({ "schema_version": 1, "mode": "all", "rules": json_rules }),
    ))
}

fn persisted_payload_for_save(
    is_editing: bool,
    enforcement_changed: bool,
    existing_type: &str,
    existing_config: &serde_json::Value,
    rules: &[PolicyRule],
) -> Option<(String, serde_json::Value)> {
    if is_editing && !enforcement_changed {
        Some((existing_type.to_string(), existing_config.clone()))
    } else {
        build_persisted_payload(rules)
    }
}

fn rule_to_composite_entry(rule: &PolicyRule) -> Option<serde_json::Value> {
    let config = match rule.kind.as_str() {
        "nixos_option" => serde_json::json!({
            "path": rule.path,
            "operator": rule.op,
            "value_type": normalize_option_type(&rule.option_type),
            "value": rule.value,
        }),
        "packages_installed" => serde_json::json!({
            "packages": split_packages(&rule.packages),
        }),
        "custom_eval" => serde_json::json!({
            "expression": rule.expr,
            "message": rule.message,
        }),
        "cve_block" => serde_json::json!({
            "severity": rule.severity,
            "max_allowed": rule.max_allowed.parse::<u32>().ok()?,
        }),
        _ => return None,
    };
    Some(serde_json::json!({
        "id": rule.id,
        "kind": rule.kind,
        "config": config,
    }))
}

fn unsupported_rule_labels(rules: &[PolicyRule]) -> Vec<&'static str> {
    rules
        .iter()
        .filter(|rule| !rule.is_persisted())
        .map(|rule| rule_label(&rule.kind))
        .collect()
}

fn cve_config_is_representable(config: &serde_json::Value) -> bool {
    let max_critical = config.get("max_critical").and_then(|value| value.as_u64());
    let max_high = config.get("max_high").and_then(|value| value.as_u64());
    let has_critical_gate = max_critical.is_some_and(|value| value > 0) || max_high.is_none();
    let has_high_gate = max_high.is_some();
    let gate_count = u8::from(has_critical_gate) + u8::from(has_high_gate);

    gate_count == 1
        && config
            .get("strict")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
        && config
            .get("when_no_scan")
            .and_then(|value| value.as_str())
            .unwrap_or("block")
            == "block"
        && !config
            .get("require_high_justification")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
}

fn object_keys_are_subset(value: &serde_json::Value, allowed: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object
            .keys()
            .all(|key| allowed.iter().any(|allowed_key| key == allowed_key))
    })
}

fn custom_check_config_is_representable(config: &serde_json::Value) -> bool {
    if !object_keys_are_subset(
        config,
        &["rules", "expression", "description", "mode", "strict"],
    ) {
        return false;
    }

    if config.get("rules").is_some() && config.get("expression").is_some() {
        return false;
    }

    let mode_ok = config
        .get("mode")
        .is_none_or(|value| value.as_str() == Some("all"));
    let strict_ok = config
        .get("strict")
        .is_none_or(|value| value.as_bool() == Some(true));

    if !mode_ok || !strict_ok {
        return false;
    }

    if let Some(entries) = config.get("rules").and_then(|value| value.as_array()) {
        return entries.iter().all(|entry| {
            object_keys_are_subset(
                entry,
                &["expression", "description", "field_name", "strict"],
            ) && entry
                .get("description")
                .is_none_or(|value| value.as_str().is_some())
                && entry
                    .get("strict")
                    .is_none_or(|value| value.as_bool() == Some(true))
                && entry
                    .get("expression")
                    .and_then(|value| value.as_str())
                    .is_some()
        });
    }

    config
        .get("expression")
        .and_then(|value| value.as_str())
        .is_some()
        && config
            .get("description")
            .is_none_or(|value| value.as_str().is_some())
}

fn require_packages_config_is_representable(config: &serde_json::Value) -> bool {
    object_keys_are_subset(config, &["packages", "strict"])
        && config
            .get("packages")
            .and_then(|value| value.as_array())
            .is_some_and(|items| items.iter().all(|item| item.as_str().is_some()))
        && config
            .get("strict")
            .is_none_or(|value| value.as_bool() == Some(true))
}

fn existing_enforcement_is_opaque(
    format: PolicyFormat,
    policy_type: &str,
    config: &serde_json::Value,
) -> bool {
    if format == PolicyFormat::Toml {
        return true;
    }
    match policy_type {
        "require_cve_check" => !cve_config_is_representable(config),
        "require_packages" => !require_packages_config_is_representable(config),
        "custom_check" => !custom_check_config_is_representable(config),
        "composite" => !composite_config_is_representable(config),
        _ => true,
    }
}

fn composite_config_is_representable(config: &serde_json::Value) -> bool {
    let hydrated = rules_from_policy("composite", config);
    let exact_round_trip =
        build_persisted_payload(&hydrated).is_some_and(|(policy_type, hydrated_config)| {
            policy_type == "composite" && hydrated_config == *config
        });
    let unique_ids = hydrated
        .iter()
        .map(|rule| rule.id)
        .collect::<std::collections::HashSet<_>>()
        .len()
        == hydrated.len();
    let exact_rule_shapes = config
        .get("rules")
        .and_then(|value| value.as_array())
        .is_some_and(|rules| {
            rules.iter().all(|rule| {
                if !object_keys_are_subset(rule, &["id", "kind", "config"]) {
                    return false;
                }
                let Some(kind) = rule.get("kind").and_then(|value| value.as_str()) else {
                    return false;
                };
                let Some(rule_config) = rule.get("config") else {
                    return false;
                };
                let allowed = match kind {
                    "nixos_option" => &["path", "operator", "value_type", "value"][..],
                    "packages_installed" => &["packages"][..],
                    "custom_eval" => &["expression", "message"][..],
                    "cve_block" => &["severity", "max_allowed"][..],
                    _ => return false,
                };
                object_keys_are_subset(rule_config, allowed)
            })
        });
    config
        .get("schema_version")
        .and_then(|value| value.as_u64())
        == Some(1)
        && config.get("mode").and_then(|value| value.as_str()) == Some("all")
        && object_keys_are_subset(config, &["schema_version", "mode", "rules"])
        && config
            .get("rules")
            .and_then(|value| value.as_array())
            .is_some_and(|rules| !rules.is_empty() && hydrated.len() == rules.len())
        && unique_ids
        && exact_rule_shapes
        && exact_round_trip
}

fn rule_validation_error(rule: &PolicyRule) -> Option<String> {
    match rule.kind.as_str() {
        "nixos_option" => {
            if rule.path.trim().is_empty() {
                return Some("NixOS option path is required.".to_string());
            }
            let valid_operator = match normalize_option_type(&rule.option_type) {
                "integer" => matches!(rule.op.as_str(), "==" | "!=" | ">=" | "<="),
                _ => matches!(rule.op.as_str(), "==" | "!="),
            };
            if !valid_operator {
                return Some(
                    "The selected operator is not valid for this option type.".to_string(),
                );
            }
            match normalize_option_type(&rule.option_type) {
                "boolean" if !rule.value.is_boolean() => {
                    Some("Boolean options require a true or false value.".to_string())
                }
                "integer" if rule.value.as_i64().is_none() => {
                    Some("Integer options require a valid 64-bit integer.".to_string())
                }
                "enum" | "string" | "lines" | "unknown" if !rule.value.is_string() => {
                    Some("This option requires a string value.".to_string())
                }
                _ => None,
            }
        }
        "packages_installed" if split_packages(&rule.packages).is_empty() => {
            Some("Enter at least one required package.".to_string())
        }
        "custom_eval" if rule.expr.trim().is_empty() => {
            Some("Custom Nix expression is required.".to_string())
        }
        "cve_block" if rule.max_allowed.parse::<u32>().is_err() => {
            Some("CVE maximum must be a non-negative integer.".to_string())
        }
        _ => None,
    }
}

fn save_blocker(
    is_editing: bool,
    format: PolicyFormat,
    existing_type: &str,
    existing_config: &serde_json::Value,
    rules: &[PolicyRule],
) -> Option<String> {
    if is_editing && format == PolicyFormat::Toml {
        return Some(
            "TOML policies are read-only in this form to avoid rewriting them as JSON.".to_string(),
        );
    }

    if is_editing
        && !matches!(
            existing_type,
            "require_cve_check" | "require_packages" | "custom_check" | "composite"
        )
    {
        return Some(format!(
            "This {existing_type} policy is not supported by this form. Its enforcement is preserved and cannot be saved here."
        ));
    }

    if is_editing
        && existing_type == "composite"
        && !composite_config_is_representable(existing_config)
    {
        return Some(
            "This composite policy contains unsupported or opaque fields. Its enforcement cannot be safely edited in this form."
                .to_string(),
        );
    }

    if existing_type == "require_cve_check" && !cve_config_is_representable(existing_config) {
        return Some(
            "This CVE policy uses backend fields this form cannot preserve yet; edit it in the raw policy editor after backend parity lands."
                .to_string(),
        );
    }

    if is_editing
        && existing_type == "custom_check"
        && !custom_check_config_is_representable(existing_config)
    {
        return Some(
            "This custom policy uses JSON fields this form cannot preserve yet; edit it in the raw policy editor after backend parity lands."
                .to_string(),
        );
    }

    if is_editing
        && existing_type == "require_packages"
        && !require_packages_config_is_representable(existing_config)
    {
        return Some(
            "This package policy uses backend fields this form cannot preserve yet; edit it in the raw policy editor after backend parity lands."
                .to_string(),
        );
    }

    let unsupported = unsupported_rule_labels(rules);
    if !unsupported.is_empty() {
        return Some(format!(
            "Remove UI-only rules before saving; not persisted yet: {}.",
            unsupported.join(", ")
        ));
    }

    if let Some((index, error)) = rules
        .iter()
        .enumerate()
        .find_map(|(index, rule)| rule_validation_error(rule).map(|error| (index, error)))
    {
        return Some(format!("Rule {}: {error}", index + 1));
    }

    None
}

/// How compliance mappings are currently known.
///
/// A failed request must never be presented as an authoritative "no mappings"
/// answer, so loading and error are first-class states alongside `Loaded`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MappingLoadState {
    Loading,
    Failed,
    Loaded,
}

/// Independent editor state dimensions.
///
/// Enforcement, compliance, and evidence are separate concepts, and the
/// enforcement wording additionally depends on the policy's origin: an imported
/// control with no assertion needs refinement, while a custom policy simply has
/// no enforcement defined yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PolicyEditorState {
    enforcement: &'static str,
    compliance: &'static str,
    evidence: &'static str,
    /// True when the policy claims compliance requirements while asserting
    /// nothing, which cannot pass or fail and warrants an explicit warning.
    mapped_not_enforced: bool,
}

fn policy_editor_state(
    is_imported: bool,
    mapping_state: MappingLoadState,
    mapping_count: usize,
    rule_count: usize,
    evidence_count: usize,
) -> PolicyEditorState {
    let enforcement = match (rule_count > 0, is_imported) {
        (true, _) => "Enforced",
        (false, true) => "Enforcement needs refinement",
        (false, false) => "No enforcement defined",
    };

    let compliance = match mapping_state {
        MappingLoadState::Loading => "Compliance mappings loading",
        MappingLoadState::Failed => "Compliance mappings unavailable",
        MappingLoadState::Loaded if mapping_count > 0 => "Mapped",
        MappingLoadState::Loaded => "Unmapped",
    };

    PolicyEditorState {
        enforcement,
        compliance,
        evidence: if evidence_count > 0 {
            "Evidence collected"
        } else {
            "No evidence"
        },
        mapped_not_enforced: mapping_state == MappingLoadState::Loaded
            && mapping_count > 0
            && rule_count == 0,
    }
}

/// A requirement mapping may only be edited or removed when the server will
/// accept the mutation: the version must be mutable and the mapping's
/// provenance must be exactly `manual`. Every other provenance the schema
/// allows (`imported`, `inherited`, `inferred`, `suggested`) is authoritative
/// and read-only.
fn mapping_row_is_editable(version_editable: bool, provenance: &str) -> bool {
    version_editable && provenance == "manual"
}

/// Human-readable label for a mapping's recorded provenance. Unknown values are
/// rendered verbatim rather than being relabelled as manual.
fn mapping_provenance_label(provenance: &str) -> String {
    match provenance {
        "manual" => "Manual mapping".to_string(),
        "imported" => "Imported from benchmark".to_string(),
        "inherited" => "Inherited from source version".to_string(),
        "inferred" => "Inferred at import".to_string(),
        "suggested" => "Suggested · not authoritative".to_string(),
        other => format!("{other} · read-only"),
    }
}

fn split_packages(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

fn custom_frameworks(policies: &[PolicyDefinition]) -> Vec<String> {
    let mut frameworks = policies
        .iter()
        .filter_map(|policy| policy.framework.as_deref())
        .map(str::trim)
        .filter(|framework| {
            !framework.is_empty()
                && !STANDARD_FRAMEWORKS
                    .iter()
                    .any(|standard| framework.eq_ignore_ascii_case(standard))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    frameworks.sort_by_key(|framework| framework.to_ascii_lowercase());
    frameworks.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    frameworks
}

/// Reconstruct builder rules from an existing policy definition (best-effort) so
/// edit mode is pre-populated with what the backend stored.
fn rules_from_policy(policy_type: &str, config: &serde_json::Value) -> Vec<PolicyRule> {
    let mut rules = Vec::new();
    match policy_type {
        "require_cve_check" => {
            let mut rule = PolicyRule::new("cve_block");
            if let Some(max_high) = config.get("max_high").and_then(|v| v.as_u64()) {
                rule.severity = "high".to_string();
                rule.max_allowed = max_high.to_string();
            } else if let Some(max_critical) = config.get("max_critical").and_then(|v| v.as_u64()) {
                rule.severity = "critical".to_string();
                rule.max_allowed = max_critical.to_string();
            }
            rules.push(rule);
        }
        "require_packages" => {
            let mut rule = PolicyRule::new("packages_installed");
            if let Some(packages) = config.get("packages").and_then(|v| v.as_array()) {
                rule.packages = packages
                    .iter()
                    .filter_map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            rules.push(rule);
        }
        "custom_check" => {
            if let Some(entries) = config.get("rules").and_then(|v| v.as_array()) {
                for entry in entries {
                    let mut rule = PolicyRule::new("custom_eval");
                    if let Some(expr) = entry.get("expression").and_then(|v| v.as_str()) {
                        rule.expr = expr.to_string();
                    }
                    if let Some(message) = entry.get("description").and_then(|v| v.as_str()) {
                        rule.message = message.to_string();
                    }
                    rules.push(rule);
                }
            } else if let Some(expr) = config.get("expression").and_then(|v| v.as_str()) {
                let mut rule = PolicyRule::new("custom_eval");
                rule.expr = expr.to_string();
                if let Some(message) = config.get("description").and_then(|v| v.as_str()) {
                    rule.message = message.to_string();
                }
                rules.push(rule);
            }
        }
        "composite" => {
            let Some(entries) = config.get("rules").and_then(|value| value.as_array()) else {
                return rules;
            };
            for entry in entries {
                let Some(id) = entry
                    .get("id")
                    .and_then(|value| value.as_str())
                    .and_then(|value| Uuid::parse_str(value).ok())
                    .filter(|id| !id.is_nil())
                else {
                    continue;
                };
                let Some(kind) = entry.get("kind").and_then(|value| value.as_str()) else {
                    continue;
                };
                if !rule_kind_is_persisted(kind) {
                    continue;
                }
                let Some(rule_config) = entry.get("config").and_then(|value| value.as_object())
                else {
                    continue;
                };
                let mut rule = PolicyRule::new_with_id(kind, id);
                let valid = match kind {
                    "nixos_option" => {
                        let Some(path) = rule_config.get("path").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        let Some(operator) =
                            rule_config.get("operator").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        let Some(value) = rule_config.get("value") else {
                            continue;
                        };
                        let Some(value_type) = rule_config
                            .get("value_type")
                            .and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        rule.path = path.to_string();
                        rule.op = operator.to_string();
                        rule.value = value.clone();
                        rule.option_type = normalize_option_type(value_type).to_string();
                        true
                    }
                    "packages_installed" => rule_config
                        .get("packages")
                        .and_then(|value| value.as_array())
                        .filter(|values| values.iter().all(|value| value.as_str().is_some()))
                        .map(|values| {
                            rule.packages = values
                                .iter()
                                .filter_map(|value| value.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                        })
                        .is_some(),
                    "custom_eval" => rule_config
                        .get("expression")
                        .and_then(|value| value.as_str())
                        .map(|expression| {
                            rule.expr = expression.to_string();
                            rule.message = rule_config
                                .get("message")
                                .and_then(|value| value.as_str())
                                .unwrap_or_default()
                                .to_string();
                        })
                        .is_some(),
                    "cve_block" => {
                        let Some(severity) =
                            rule_config.get("severity").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        let Some(max_allowed) = rule_config
                            .get("max_allowed")
                            .and_then(|value| value.as_u64())
                        else {
                            continue;
                        };
                        rule.severity = severity.to_string();
                        rule.max_allowed = max_allowed.to_string();
                        true
                    }
                    _ => false,
                };
                if valid {
                    rules.push(rule);
                }
            }
        }
        _ => {}
    }
    rules
}

fn parse_existing(body: &str, format: PolicyFormat) -> (String, serde_json::Value) {
    if format == PolicyFormat::Json {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            let policy_type = value
                .get("policy_type")
                .or_else(|| value.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("custom_check")
                .to_string();
            let config = value
                .get("config")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            return (policy_type, config);
        }
    }
    ("custom_check".to_string(), serde_json::Value::Null)
}

// ─────────────────────────────────────────────────────────────────────────────
#[component]
fn PolicyMappingsTab(
    is_editing: bool,
    editing_policy_version_id: Option<Uuid>,
    mappings_editable: bool,
    mut mappings: Signal<Vec<PolicyMappingRow>>,
    mut pending_mappings: Signal<Vec<PendingPolicyMapping>>,
    /// Owned by the modal so mappings are loaded when the editor opens, not
    /// when this tab is first shown.
    mut mapping_load_state: Signal<MappingLoadState>,
    mut mapping_load_error: Signal<Option<String>>,
) -> Element {
    let mapping_target =
        mapping_editor_target(is_editing, editing_policy_version_id, mappings_editable);
    let mut loaded = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut frameworks: Signal<Vec<ComplianceFrameworkSummary>> = use_signal(Vec::new);
    let mut versions: Signal<Vec<ComplianceFrameworkVersionSummary>> = use_signal(Vec::new);
    let mut results: Signal<Vec<RequirementVersionSummary>> = use_signal(Vec::new);
    let mut framework_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut version_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut requirement_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut requirement: Signal<Option<RequirementVersionSummary>> = use_signal(|| None);
    let mut search = use_signal(String::new);
    let mut relationship = use_signal(|| "implements".to_string());
    let mut coverage = use_signal(|| "full".to_string());
    let mut rationale = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut show_mapping_editor = use_signal(|| false);
    let mut editing_mapping_id: Signal<Option<Uuid>> = use_signal(|| None);

    if !*loaded.read() {
        loaded.set(true);
        spawn(async move {
            match fetch_compliance_frameworks().await {
                Ok(value) => frameworks.set(value),
                Err(e) => error.set(Some(format!("Failed to load frameworks: {e}"))),
            }
        });
    }

    let mapping_state = *mapping_load_state.read();
    let rows = mappings.read().clone();
    let pending_rows = pending_mappings.read().clone();
    let mut pending_grouped: Vec<(String, Vec<PendingPolicyMapping>)> = Vec::new();
    for row in pending_rows {
        let name = format!("{} · {}", row.framework_name, row.framework_version);
        if let Some(group) = pending_grouped.iter_mut().find(|(key, _)| *key == name) {
            group.1.push(row);
        } else {
            pending_grouped.push((name, vec![row]));
        }
    }
    let mut grouped: Vec<(String, Vec<PolicyMappingRow>)> = Vec::new();
    for row in rows {
        let name = format!("{} · {}", row.framework_name, row.framework_version);
        if let Some(group) = grouped.iter_mut().find(|(key, _)| *key == name) {
            group.1.push(row);
        } else {
            grouped.push((name, vec![row]));
        }
    }

    rsx! {
        div { style: "margin-top:6px;display:flex;flex-direction:column;gap:14px;",
            div { style: "font-size:12px;color:var(--cf-text-secondary);margin-bottom:2px;line-height:1.5;",
                "Map this policy to the compliance requirements it implements, supports, or provides evidence for. Policies can map to requirements from multiple frameworks."
            }
            if mapping_state == MappingLoadState::Loading {
                div { class: "sd-callout sd-callout-info", "data-testid": "policy-mappings-loading",
                    div { style: "font-size:12px;", "Loading compliance mappings…" }
                }
            } else if mapping_state == MappingLoadState::Failed {
                div { class: "sd-callout sd-callout-warn", "data-testid": "policy-mappings-error",
                    div { style: "font-size:12px;",
                        strong { "Compliance mappings unavailable." }
                        span { " " }
                        {mapping_load_error.read().clone().unwrap_or_else(|| "The mapping request failed.".to_string())}
                        span { " This policy is not necessarily unmapped." }
                    }
                    if let Some(policy_version_id) = editing_policy_version_id {
                        button {
                            class: "btn btn-ghost xs focus-ring",
                            "data-testid": "policy-mappings-retry",
                            style: "margin-top:6px;",
                            onclick: move |_| {
                                mapping_load_error.set(None);
                                mapping_load_state.set(MappingLoadState::Loading);
                                spawn(async move {
                                    match fetch_policy_requirement_mappings(&policy_version_id).await {
                                        Ok(value) => { mappings.set(value); mapping_load_state.set(MappingLoadState::Loaded); }
                                        Err(e) => {
                                            mapping_load_error.set(Some(format!("Failed to load compliance mappings: {e}")));
                                            mapping_load_state.set(MappingLoadState::Failed);
                                        }
                                    }
                                });
                            },
                            "Retry"
                        }
                    }
                }
            } else if grouped.is_empty() && pending_grouped.is_empty() {
                div { class: "sd-callout sd-callout-info", "data-testid": "policy-mappings-unmapped", style: "margin-bottom:10px;",
                    div { style: "font-size:12.5px;font-weight:700;margin-bottom:2px;", "Unmapped" }
                    div { style: "font-size:12px;",
                        {match mapping_target {
                            MappingEditorTarget::Pending => "No compliance mappings yet. This policy can still be used as an operational/custom policy with zero mappings. Add mappings below; they will be saved when this policy is created.",
                            MappingEditorTarget::Persisted(_) => "No compliance mappings yet. This policy can still be used as an operational/custom policy with zero mappings.",
                            MappingEditorTarget::Unavailable => "No compliance mappings yet. This policy can still be be used as an operational/custom policy with zero mappings.",
                        }}
                    }
                }
            } else {
                div { style: "display:flex;flex-direction:column;gap:10px;",
                    for (name, group) in pending_grouped {
                        div { key: "pending-{name}",
                            div { style: "font-size:11.5px;font-weight:700;color:var(--cf-text-primary);margin-bottom:6px;", "{name}" }
                            for row in group {
                                {
                                    let requirement_version_id = row.requirement_version_id;
                                    rsx! { div { style: "display:grid;grid-template-columns:1fr auto;gap:8px;align-items:start;padding:9px 11px;background:var(--cf-subtle-bg);border:1px solid var(--cf-divider);border-radius:8px;font-size:12px;margin-bottom:6px;",
                                        div { style: "display:flex;flex-direction:column;gap:2px;",
                                            div { style: "font-size:11px;font-weight:700;color:var(--cf-text-muted);text-transform:uppercase;letter-spacing:0.04em;", "{row.framework_name} {row.framework_version}" }
                                            div { class: "mono", style: "font-size:12.5px;font-weight:600;margin-top:2px;", span { "{row.requirement_external_id}" } span { style: "font-family:inherit;font-weight:400;color:var(--cf-text-secondary);", " · {row.requirement_title.clone().unwrap_or_default()}" } }
                                            div { style: "display:flex;gap:6px;margin-top:2px;", span { class: "chip chip-neutral", style: "font-size:10px;", {match row.relationship.as_str() { "implements" => "Implements", "supports" => "Supports", _ => "Evidence for" }} }, span { class: if row.coverage == "full" { "chip chip-success" } else { "chip chip-warn" }, style: "font-size:10px;", {if row.coverage == "full" { "Full" } else { "Partial" }} }, span { class: "chip chip-neutral", style: "font-size:10px;", "Pending" } }
                                            if let Some(text) = &row.rationale { if !text.is_empty() { div { style: "color:var(--cf-text-muted);font-size:11px;margin-top:2px;", "{text}" } } }
                                        }
                                        button { class: "btn btn-ghost xs focus-ring", style: "color:var(--cf-text-muted);padding:4px 6px;", title: "Remove mapping", onclick: move |_| { let mut next = pending_mappings.read().clone(); remove_pending_mapping(&mut next, requirement_version_id); pending_mappings.set(next); }, "×" }
                                    } }
                                }
                            }
                        }
                    }
                }
                div { style: "display:flex;flex-direction:column;gap:10px;",
                    for (name, group) in grouped {
                        div { key: "{name}",
                            div { style: "font-size:11.5px;font-weight:700;color:var(--cf-text-primary);margin-bottom:6px;", "{name}" }
                             for row in group {
                                 {
                                     // Editability uses the same rule the server enforces:
                                     // a mutable version plus provenance == "manual".
                                     let row_read_only = !mapping_row_is_editable(mappings_editable, &row.provenance);
                                     let provenance_label = mapping_provenance_label(&row.provenance);
                                     let edit_row = row.clone();
                                     rsx! { div { key: "{row.id}", "data-testid": "policy-mapping-row", style: "display:grid;grid-template-columns:1fr auto;gap:8px;align-items:start;padding:9px 11px;background:var(--cf-subtle-bg);border:1px solid var(--cf-divider);border-radius:8px;font-size:12px;margin-bottom:6px;",
                                    div { style: "display:flex;flex-direction:column;gap:2px;",
                                        div { style: "font-size:11px;font-weight:700;color:var(--cf-text-muted);text-transform:uppercase;letter-spacing:0.04em;", "{row.framework_name} {row.framework_version}" }
                                        div { class: "mono", style: "font-size:12.5px;font-weight:600;margin-top:2px;", span { "{row.requirement_external_id}" } if let Some(title) = &row.requirement_title { span { style: "font-family:inherit;font-weight:400;color:var(--cf-text-secondary);", " · {title}" } } }
                                        div { style: "display:flex;gap:6px;margin-top:2px;",
                                            span { class: "chip chip-neutral", style: "font-size:10px;", {match row.relationship.as_str() { "implements" => "Implements", "supports" => "Supports", _ => "Evidence for" }} }
                                            span { class: if row.coverage == "full" { "chip chip-success" } else { "chip chip-warn" }, style: "font-size:10px;", {if row.coverage == "full" { "Full" } else { "Partial" }} }
                                            span { "data-testid": "policy-mapping-provenance", style: "font-size:10px;color:var(--cf-text-muted);", "{provenance_label}" }
                                        }
                                        if let Some(text) = &row.rationale { if !text.is_empty() { div { style: "color:var(--cf-text-muted);font-size:11px;margin-top:2px;", "{text}" } } }
                                    }
                                    if let Some(policy_id) = editing_policy_version_id {
                                         if !row_read_only {
                                             div { style: "display:flex;gap:4px;",
                                                 button { class: "btn btn-ghost xs focus-ring", style: "color:var(--cf-text-muted);padding:4px 6px;", onclick: move |_| {
                                                     editing_mapping_id.set(Some(edit_row.id));
                                                     framework_id.set(Some(edit_row.framework_id));
                                                     version_id.set(Some(edit_row.framework_version_id));
                                                     requirement_id.set(Some(edit_row.requirement_version_id));
                                                     requirement.set(Some(RequirementVersionSummary {
                                                         id: edit_row.requirement_version_id,
                                                         requirement_id: edit_row.requirement_version_id,
                                                         framework_version_id: edit_row.framework_version_id,
                                                         external_id: edit_row.requirement_external_id.clone(),
                                                         title: edit_row.requirement_title.clone(),
                                                         kind: "rule".to_string(),
                                                         severity: None,
                                                         parent_requirement_version_id: None,
                                                         semantic_digest: String::new(),
                                                     }));
                                                     relationship.set(edit_row.relationship.clone());
                                                     coverage.set(edit_row.coverage.clone());
                                                     rationale.set(edit_row.rationale.clone().unwrap_or_default());
                                                     show_mapping_editor.set(true);
                                                 }, "Edit" }
                                                 button { class: "btn btn-ghost xs focus-ring", style: "color:var(--cf-text-muted);padding:4px 6px;", title: "Remove mapping", onclick: move |_| { let row_id = row.id; spawn(async move { if let Err(e) = delete_policy_mapping(&policy_id, &row_id).await { error.set(Some(format!("Failed to remove mapping: {e}"))); } if let Ok(value) = fetch_policy_requirement_mappings(&policy_id).await { mappings.set(value); } }); }, "×" }
                                             }
                                         } else { span { class: "chip chip-neutral", style: "font-size:10px;", "Read-only" } }
                                    }
                                } }
                                }
                            }
                        }
                    }
                }
            }
            if mapping_target != MappingEditorTarget::Unavailable && !show_mapping_editor() {
                button { class: "btn btn-ghost focus-ring", style: "align-self:flex-start;", onclick: move |_| show_mapping_editor.set(true), "+ Add mapping" }
            }
            if mapping_target != MappingEditorTarget::Unavailable && show_mapping_editor() {
                div { style: "border:1px solid var(--cf-brand-purple);border-radius:10px;padding:14px;background:color-mix(in oklab, var(--cf-brand-purple) 5%, var(--cf-card-bg));display:flex;flex-direction:column;gap:14px;margin-top:8px;",
                     div { style: "font-size:12.5px;font-weight:600;", if editing_mapping_id.read().is_some() { "Edit mapping" } else { "Add mapping" } }
                    div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:-4px;line-height:1.4;", "Map this policy to a compliance requirement it implements, supports, or provides evidence for." }
                    div { class: "field", label { r#for: "policy-mapping-framework", style: "font-size:11px;", "Framework" }, select { id: "policy-mapping-framework", class: "input focus-ring", onchange: move |event| { let value = event.value(); if let Ok(id) = value.parse() { framework_id.set(Some(id)); spawn(async move { if let Ok(value) = fetch_compliance_framework_versions(&id).await { versions.set(value); } }); } }, option { value: "", "— Select framework —" }, for item in frameworks.read().iter() { option { value: "{item.id}", "{item.name}" } } } }
                    if !versions.read().is_empty() { div { class: "field", label { r#for: "policy-mapping-version", style: "font-size:11px;", "Version" }, select { id: "policy-mapping-version", class: "input focus-ring", onchange: move |event| { version_id.set(event.value().parse().ok()); }, option { value: "", "— Select version —" }, for item in versions.read().iter() { option { value: "{item.id}", "{item.version}" } } } } }
                    if version_id.read().is_some() && requirement.read().is_none() {
                        div { class: "field",
                            label { r#for: "policy-mapping-requirement", style: "font-size:11px;", "Requirement" }
                            input { id: "policy-mapping-requirement", class: "input focus-ring", placeholder: "Search by ID, title, CCI, SRG…", value: "{search}", oninput: move |event| { let value = event.value(); search.set(value.clone()); if let Some(id) = *version_id.read() { spawn(async move { if let Ok(value) = search_requirements(&id, Some(&value), None, 25, 0).await { results.set(value); } }); } } }
                            if requirement_id.read().is_none() {
                                for item in results.read().iter() {
                                    {
                                        let item = item.clone();
                                        let display_label = format!("{} · {} · {}", item.external_id, item.kind, item.title.as_deref().unwrap_or(""));
                                        rsx! { button {
                                            class: "btn btn-ghost focus-ring",
                                            style: "width:100%;text-align:left;padding:6px 10px;font-size:11px;",
                                             onclick: move |_| { requirement_id.set(Some(item.id)); requirement.set(Some(item.clone())); search.set(display_label.clone()); results.set(Vec::new()); },
                                            span { "{display_label}" }
                                        } }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(selected) = requirement.read().clone() {
                        div { style: "display:flex;align-items:center;justify-content:space-between;padding:8px 10px;background:var(--cf-subtle-bg);border-radius:8px;border:1px solid var(--cf-divider);",
                            div {
                                div { class: "mono", style: "font-size:12.5px;font-weight:600;", "{selected.external_id}" span { style: "font-family:inherit;font-weight:400;color:var(--cf-text-secondary);", " · {selected.title.clone().unwrap_or_default()}" } }
                                if let Some(parent_id) = selected.parent_requirement_version_id { div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:2px;", "Parent requirement · {parent_id}" } }
                                if selected.parent_requirement_version_id.is_none() { div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:2px;", "Root requirement" } }
                            }
                            button { class: "btn btn-ghost focus-ring xs", onclick: move |_| { requirement_id.set(None); requirement.set(None); search.set(String::new()); results.set(Vec::new()); }, "Change" }
                        }
                    }
                    if requirement_id.read().is_some() {
                        div { class: "field", label { style: "font-size:11px;", "Relationship" },
                            div { style: "display:flex;flex-direction:column;gap:6px;",
                                for (value, label, blurb) in MAPPING_RELATIONSHIPS {
                                    button { r#type: "button", class: "focus-ring", onclick: move |_| relationship.set(value.to_string()), style: if relationship() == value { "all:unset;cursor:pointer;display:flex;flex-direction:column;gap:2px;padding:8px 10px;border-radius:8px;background:color-mix(in oklab, var(--cf-brand-purple) 12%, transparent);border:1px solid var(--cf-brand-purple);" } else { "all:unset;cursor:pointer;display:flex;flex-direction:column;gap:2px;padding:8px 10px;border-radius:8px;background:var(--cf-subtle-bg);border:1px solid var(--cf-divider);" },
                                        span { style: "font-size:12px;font-weight:600;", "{label}" }
                                        span { style: "font-size:10.5px;color:var(--cf-text-muted);", "{blurb}" }
                                    }
                                }
                            }
                        }
                        div { class: "field", label { style: "font-size:11px;", "Coverage" },
                            div { class: "seg", style: "width:fit-content;",
                                button { class: if coverage() == "full" { "active" } else { "" }, onclick: move |_| coverage.set("full".to_string()), "Full" }
                                button { class: if coverage() == "partial" { "active" } else { "" }, onclick: move |_| coverage.set("partial".to_string()), "Partial" }
                            }
                        }
                        div { class: "field", label { r#for: "policy-mapping-rationale", style: "font-size:11px;", "Mapping rationale " span { style: "color:var(--cf-text-muted);font-weight:400;", "· optional" } }, textarea { id: "policy-mapping-rationale", class: "input focus-ring", rows: "2", value: "{rationale}", placeholder: "Why this policy satisfies the requirement", style: "resize:vertical;", oninput: move |event| rationale.set(event.value()) } }
                        if let Some(text) = &*error.read() { div { class: "sd-callout sd-callout-error", style: "font-size:11px;", "{text}" } }
                        div { style: "display:flex;justify-content:flex-end;gap:8px;",
                         button { class: "btn btn-ghost focus-ring", r#type: "button", onclick: move |_| { editing_mapping_id.set(None); show_mapping_editor.set(false); }, "Cancel" }
                         button { class: "btn btn-primary focus-ring", disabled: *saving.read(), onclick: move |_| {
                            error.set(None);
                            let Some(rv_id) = *requirement_id.read() else { error.set(Some("Select a requirement.".into())); return; };
                            let relationship_value = relationship.read().clone();
                            let coverage_value = coverage.read().clone();
                            let rationale_value = non_empty(rationale.read().clone());
                            match mapping_target {
                                MappingEditorTarget::Pending => {
                                    let Some(item) = requirement.read().clone() else { error.set(Some("Select a requirement.".into())); return; };
                                    let Some(fw_id) = *framework_id.read() else { error.set(Some("Select a framework.".into())); return; };
                                    let Some(fv_id) = *version_id.read() else { error.set(Some("Select a framework version.".into())); return; };
                                    let Some(fw) = frameworks.read().iter().find(|item| item.id == fw_id).cloned() else { error.set(Some("Selected framework is unavailable.".into())); return; };
                                    let Some(fv) = versions.read().iter().find(|item| item.id == fv_id).cloned() else { error.set(Some("Selected framework version is unavailable.".into())); return; };
                                    let mut next = pending_mappings.read().clone();
                                    match add_pending_mapping(&mut next, pending_mapping_from_selection(&fw, &fv, &item, relationship_value, coverage_value, rationale_value)) {
                                         Ok(()) => { pending_mappings.set(next); requirement_id.set(None); requirement.set(None); search.set(String::new()); results.set(Vec::new()); relationship.set("implements".into()); coverage.set("full".into()); rationale.set(String::new()); show_mapping_editor.set(false); }
                                        Err(e) => error.set(Some(e.to_string())),
                                    }
                                }
                                 MappingEditorTarget::Persisted(policy_id) => {
                                     saving.set(true);
                                     spawn(async move {
                                         let result = if let Some(mapping_id) = *editing_mapping_id.read() {
                                             let request = UpdatePolicyMappingRequest { relationship: relationship_value, coverage: coverage_value, rationale: rationale_value };
                                             update_policy_mapping(&policy_id, &mapping_id, &request).await
                                         } else {
                                             let request = CreatePolicyMappingRequest { requirement_version_id: rv_id, relationship: relationship_value, coverage: coverage_value, rationale: rationale_value, provenance: "manual".into() };
                                             create_policy_mapping(&policy_id, &request).await
                                         };
                                         match result {
                                              Ok(_) => { if let Ok(value) = fetch_policy_requirement_mappings(&policy_id).await { mappings.set(value); } editing_mapping_id.set(None); requirement_id.set(None); requirement.set(None); version_id.set(None); framework_id.set(None); search.set(String::new()); rationale.set(String::new()); results.set(Vec::new()); versions.set(Vec::new()); show_mapping_editor.set(false); }
                                            Err(e) => error.set(Some(format!("Failed to add mapping: {e}"))),
                                        }
                                        saving.set(false);
                                    });
                                }
                                MappingEditorTarget::Unavailable => {}
                            }
                        }, if *saving.read() { "Saving..." } else if editing_mapping_id.read().is_some() { "Save mapping" } else { "Add mapping" } }
                        }
                    }
                }
            } else if editing_policy_version_id.is_some() {
                div { class: "sd-callout sd-callout-info", style: "font-size:11px;", "This policy version is immutable. Create or edit a draft revision to change its requirement mappings." }
            }
        }
    }
}

// Modal component
// ─────────────────────────────────────────────────────────────────────────────

/// Modal for creating or editing a policy definition (design-faithful).
#[component]
pub fn PolicyEditorModal(
    editing_policy_id: Signal<Option<Uuid>>,
    edit_name: Signal<String>,
    edit_description: Signal<String>,
    edit_body: Signal<String>,
    edit_format: Signal<PolicyFormat>,
    /// Comma-separated SRG IDs (e.g. "SRG-OS-000298-GPOS-00116, SRG-OS-000096").
    /// Seeded from the current version's compliance_metadata on edit; blank on create.
    /// PERSISTED to compliance_metadata via the server API.
    edit_srg_ids: Signal<String>,
    /// Comma-separated CCI IDs (e.g. "CCI-000205, CCI-000196").
    /// PERSISTED to compliance_metadata via the server API.
    edit_cci_ids: Signal<String>,
    policy_library: Signal<Vec<PolicyDefinition>>,
    /// The exact policy version being edited.
    ///
    /// Every origin (catalog card/row, policy drawer revision, compliance
    /// drawer) hands the editor one coherent version so classification,
    /// enforcement, evidence, and imported provenance always describe the same
    /// revision. Falling back to a catalog lookup would mix a selected revision
    /// with the lineage-current one.
    #[props(default)]
    editing_policy: Option<PolicyDefinition>,
    on_close: EventHandler<()>,
) -> Element {
    let is_editing = editing_policy_id.read().is_some();
    let editing_name = edit_name.read().clone();
    let title = if is_editing {
        format!("Edit {editing_name}")
    } else {
        "New custom policy".to_string()
    };
    let subtitle = if is_editing {
        "Update the rules and rationale."
    } else {
        "Compose a policy from gate rules. Systems can be assigned this policy from their edit dialog."
    };
    let action_label = if is_editing {
        "Save changes"
    } else {
        "Create policy"
    };

    // Seed builder state from any existing payload.
    let (existing_type, existing_config) =
        parse_existing(&edit_body.read().clone(), *edit_format.read());
    // A new policy starts with no enforcement. Seeding UI-only rule kinds that
    // cannot be persisted forced the user to delete them before the very first
    // save, and contradicted "No enforcement defined" being a valid state.
    let existing_policy = editing_policy.clone().or_else(|| {
        editing_policy_id.read().and_then(|id| {
            policy_library
                .read()
                .iter()
                .find(|policy| policy.id == id)
                .cloned()
        })
    });
    let seed_category = existing_policy
        .as_ref()
        .and_then(|policy| policy.category.as_deref())
        .unwrap_or(match existing_type.as_str() {
            "require_cve_check" => "pipeline",
            "require_packages" | "custom_check" => "security",
            _ => "deployment",
        });

    // One four-value category, exactly as the design models it. Security is a
    // peer category, not a separate "domain".
    let mut category = use_signal(|| {
        PolicyCategory::from_id(seed_category)
            .unwrap_or(PolicyCategory::Deployment)
            .id()
            .to_string()
    });
    let seed_framework = existing_policy
        .as_ref()
        .and_then(|policy| policy.framework.clone())
        .unwrap_or_default();
    let framework_is_standard = STANDARD_FRAMEWORKS
        .iter()
        .any(|standard| seed_framework.eq_ignore_ascii_case(standard));
    let mut framework = use_signal(|| {
        if framework_is_standard || seed_framework.is_empty() {
            seed_framework.clone()
        } else {
            "__custom__".to_string()
        }
    });
    let mut custom_framework = use_signal(|| {
        (!framework_is_standard)
            .then_some(seed_framework.clone())
            .unwrap_or_default()
    });
    let mut severity = use_signal(|| {
        existing_policy
            .as_ref()
            .and_then(|policy| policy.severity.clone())
            .unwrap_or_default()
    });
    let mut control_family = use_signal(|| {
        existing_policy
            .as_ref()
            .and_then(|policy| policy.control_family.clone())
            .unwrap_or_default()
    });
    let mut cmmc_level = use_signal(|| {
        existing_policy
            .as_ref()
            .and_then(|policy| policy.cmmc_level)
            .map(|level| level.to_string())
            .unwrap_or_default()
    });
    let mut cis_section = use_signal(|| {
        existing_policy
            .as_ref()
            .and_then(|policy| policy.cis_section.clone())
            .unwrap_or_default()
    });
    let mut rationale = use_signal(|| {
        existing_policy
            .as_ref()
            .and_then(|policy| policy.rationale.clone())
            .unwrap_or_default()
    });
    let framework_options = custom_frameworks(&policy_library.read());
    let initial_existing_type = existing_type.clone();
    let initial_existing_config = existing_config.clone();
    let mut rules = use_signal(move || {
        if is_editing {
            rules_from_policy(&initial_existing_type, &initial_existing_config)
        } else {
            Vec::new()
        }
    });
    // Enforcement dirtiness is separate from metadata/category edits so an
    // untouched legacy payload can be sent back byte-for-byte semantically.
    let mut enforcement_changed = use_signal(|| false);
    // Initialize evidence from existing policy specs, or empty if creating new
    let initial_evidence: Vec<PolicyEvidence> = existing_policy
        .as_ref()
        .and_then(|p| p.evidence_specs.as_ref())
        .map(|specs| {
            specs
                .iter()
                .map(PolicyEvidence::from_evidence_spec)
                .collect()
        })
        .unwrap_or_default();
    let initial_evidence_count = initial_evidence.len();
    let mut evidence: Signal<Vec<PolicyEvidence>> = use_signal({
        let ev = initial_evidence.clone();
        move || ev.clone()
    });
    let mut add_rule_kind = use_signal(String::new);
    let mut add_evidence_kind = use_signal(String::new);
    let mut active_tab = use_signal(|| PolicyEditorTab::Details);

    // ── Mappings tab state ────────────────────────────────────────────────────
    let mut mappings: Signal<Vec<PolicyMappingRow>> = use_signal(Vec::new);
    let mut pending_mappings: Signal<Vec<PendingPolicyMapping>> = use_signal(Vec::new);

    // Capture the editing policy version ID for mapping API calls.
    let editing_policy_version_id: Option<Uuid> =
        existing_policy.as_ref().and_then(|p| p.version_id);
    let mappings_editable = existing_policy
        .as_ref()
        .is_some_and(is_policy_version_editable);

    // Mappings load when the editor opens, not when the Compliance section is
    // first shown: otherwise a mapped policy briefly claims to be Unmapped.
    let mut mapping_load_state = use_signal(|| {
        if editing_policy_version_id.is_some() {
            MappingLoadState::Loading
        } else {
            MappingLoadState::Loaded
        }
    });
    let mut mapping_load_error: Signal<Option<String>> = use_signal(|| None);
    let mut mappings_requested = use_signal(|| false);
    if !*mappings_requested.read() {
        mappings_requested.set(true);
        if let Some(version_id) = editing_policy_version_id {
            spawn(async move {
                match fetch_policy_requirement_mappings(&version_id).await {
                    Ok(value) => {
                        mappings.set(value);
                        mapping_load_state.set(MappingLoadState::Loaded);
                    }
                    Err(error) => {
                        mapping_load_error
                            .set(Some(format!("Failed to load compliance mappings: {error}")));
                        mapping_load_state.set(MappingLoadState::Failed);
                    }
                }
            });
        }
    }

    // Authoritative imported origin for the exact version being edited.
    let provenance = existing_policy
        .as_ref()
        .map(|policy| policy.provenance.clone())
        .unwrap_or_default();
    let is_imported = !provenance.is_empty();

    let mut save_error = use_signal(String::new);
    let mut is_saving = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let mut delete_typed = use_signal(String::new);

    let name_value = edit_name.read().clone();
    let name_missing = name_value.trim().is_empty();
    let current_rules = rules.read().clone();
    let current_save_blocker = save_blocker(
        is_editing,
        *edit_format.read(),
        &existing_type,
        &existing_config,
        &current_rules,
    );
    let enforcement_opaque = is_editing
        && existing_enforcement_is_opaque(*edit_format.read(), &existing_type, &existing_config);
    let can_save = !name_missing && current_save_blocker.is_none() && !*is_saving.read();
    let rule_count = rules.read().len();
    let evidence_count = evidence.read().len();
    let selected_category =
        PolicyCategory::from_id(category.read().as_str()).unwrap_or(PolicyCategory::Deployment);
    let is_security = selected_category == PolicyCategory::Security;
    // Guidance derived from the selected category. Recommendations and the
    // off-category notice are informational only; `rules` is never filtered.
    // Only kinds Phase 2 can actually persist are surfaced as suggestions.
    let recommended_labels = actionable_recommended_enforcement(selected_category)
        .iter()
        .map(|kind| rule_label(kind))
        .collect::<Vec<_>>()
        .join(", ");
    let current_rule_kinds: Vec<String> =
        current_rules.iter().map(|rule| rule.kind.clone()).collect();
    let off_category_kinds = off_category_rule_kinds(selected_category, &current_rule_kinds);
    let off_category_rule_count = off_category_kinds.len();
    let off_category_labels = off_category_kinds
        .iter()
        .map(|kind| rule_label(kind))
        .collect::<Vec<_>>()
        .join(", ");

    let mapping_count = mappings.read().len() + pending_mappings.read().len();
    let mapping_state = *mapping_load_state.read();
    let mut policy_state = policy_editor_state(
        is_imported,
        mapping_state,
        mapping_count,
        rule_count,
        evidence_count,
    );
    if enforcement_opaque {
        policy_state.enforcement = "Enforcement unavailable in this editor";
        policy_state.mapped_not_enforced = false;
    }
    let delete_matches = delete_typed.read().as_str() == name_value;

    rsx! {
        div {
            class: "modal-backdrop cf-modal-overlay-z50",
            onclick: move |_| on_close.call(()),
            div {
                class: "modal cf-policy-modal-panel",
                "data-testid": "policy-editor-modal",
                style: "width:min(680px,96vw);max-height:92vh;",
                onclick: |evt| evt.stop_propagation(),

                if *confirm_delete.read() {
                    // ── Danger zone: typed-confirmation delete ──────────────────
                    div { class: "modal-head", style: "background:rgba(248,113,113,0.06);",
                        h2 { style: "color:#fecaca;display:flex;align-items:center;gap:8px;",
                            svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "#f87171", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                                path { d: "M12 9v4M12 17h.01" }
                            }
                            "Remove policy"
                        }
                        p {
                            "This deletes the "
                            span { class: "mono", style: "font-weight:600;", "{name_value}" }
                            " policy."
                        }
                    }
                    div { class: "modal-body",
                        div { class: "field",
                            label {
                                "Type "
                                span { class: "mono", style: "color:#fecaca;font-weight:700;", "{name_value}" }
                                " to confirm"
                            }
                            input {
                                class: "input focus-ring mono",
                                placeholder: "{name_value}",
                                value: "{delete_typed}",
                                oninput: move |event| delete_typed.set(event.value()),
                            }
                        }
                        if !save_error.read().is_empty() {
                            div { class: "text-xs rounded px-3 py-2 cf-policy-modal-error", "{save_error}" }
                        }
                    }
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| {
                                confirm_delete.set(false);
                                delete_typed.set(String::new());
                            },
                            "Cancel"
                        }
                        button {
                            class: "btn focus-ring",
                            disabled: !delete_matches,
                            style: if delete_matches { "background:#dc2626;color:white;" } else { "background:var(--cf-subtle-bg);color:var(--cf-text-muted);" },
                            onclick: move |_| {
                                let Some(policy_id) = *editing_policy_id.read() else { return; };
                                let mut policy_library = policy_library;
                                let mut save_error = save_error;
                                let on_close = on_close;
                                spawn(async move {
                                    match delete_deployment_policy(&policy_id).await {
                                        Ok(()) => {
                                             match policies_api::load_policies().await {
                                                 policies_api::PolicyLoadResult::Ok(latest) => {
                                                     policy_library.set(latest);
                                                     on_close.call(());
                                                 }
                                                 policies_api::PolicyLoadResult::Err(error) => {
                                                     save_error.set(format!("Policy removed, but refresh failed: {error}"));
                                                 }
                                             }
                                        }
                                        Err(error) => save_error.set(format!("Failed to remove policy: {error}")),
                                    }
                                });
                            },
                            "Remove policy"
                        }
                    }
                } else {
                    // ── Header ──────────────────────────────────────────────────
                    div { class: "modal-head",
                        h2 { style: "display:flex;align-items:center;gap:6px;white-space:nowrap;",
                            svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "flex-shrink:0;",
                                if is_editing {
                                    circle { cx: "12", cy: "12", r: "3" }
                                    path { d: "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" }
                                } else {
                                    path { d: "M12 5v14M5 12h14" }
                                }
                            }
                            "{title}"
                        }
                        p { style: "white-space:nowrap;", "{subtitle}" }
                    }

                    // ── Body ────────────────────────────────────────────────────
                    div { class: "modal-body cf-policy-modal-body", style: "overflow-y:auto;",
                        // Section order follows the design: Basics, Enforcement,
                        // Compliance, Evidence, then read-only Provenance.
                        div { class: "cf-modal-tabs", role: "tablist", aria_label: "Policy editor sections",
                            PolicyEditorTabButton { tab: PolicyEditorTab::Details, active: *active_tab.read(), label: "Basics", test_id: "policy-editor-tab-details", on_select: move |_| active_tab.set(PolicyEditorTab::Details) }
                            PolicyEditorTabButton { tab: PolicyEditorTab::Enforcement, active: *active_tab.read(), label: if rule_count > 0 { format!("Enforcement · {rule_count}") } else if is_imported { "Enforcement · Needs refinement".to_string() } else { "Enforcement · None".to_string() }, test_id: "policy-editor-tab-enforcement", on_select: move |_| active_tab.set(PolicyEditorTab::Enforcement) }
                            PolicyEditorTabButton { tab: PolicyEditorTab::Mappings, active: *active_tab.read(), label: match mapping_state { MappingLoadState::Loading => "Compliance · …".to_string(), MappingLoadState::Failed => "Compliance · unavailable".to_string(), MappingLoadState::Loaded if mapping_count > 0 => format!("Compliance · {mapping_count}"), MappingLoadState::Loaded => "Compliance · Unmapped".to_string() }, test_id: "policy-editor-tab-mappings", on_select: move |_| active_tab.set(PolicyEditorTab::Mappings) }
                            PolicyEditorTabButton { tab: PolicyEditorTab::Evidence, active: *active_tab.read(), label: format!("Evidence · {evidence_count}"), test_id: "policy-editor-tab-evidence", on_select: move |_| active_tab.set(PolicyEditorTab::Evidence) }
                            if is_imported {
                                PolicyEditorTabButton { tab: PolicyEditorTab::Provenance, active: *active_tab.read(), label: "Provenance".to_string(), test_id: "policy-editor-tab-provenance", on_select: move |_| active_tab.set(PolicyEditorTab::Provenance) }
                            }
                        }
                        div { class: "sd-callout sd-callout-info", "data-testid": "policy-editor-state", style: "margin:10px 0 0;font-size:11px;",
                            "Policy state: "
                            strong { "{policy_state.enforcement}" }
                            " · "
                            strong { "{policy_state.compliance}" }
                            " · "
                            span { "{policy_state.evidence}" }
                            if is_imported {
                                span { " · " }
                                span { class: "chip chip-info", style: "font-size:10px;", "Imported" }
                            }
                        }
                        if policy_state.mapped_not_enforced {
                            div { class: "sd-callout sd-callout-warn", "data-testid": "policy-editor-mapped-not-enforced", style: "margin:8px 0 0;font-size:11.5px;",
                                strong { "Mapped, not enforced." }
                                span { " This policy claims {mapping_count} compliance requirement(s) but asserts nothing yet, so it cannot pass or fail. Add enforcement to make it real." }
                            }
                        }
                        div { class: "cf-modal-tab-panel",
                        if *active_tab.read() == PolicyEditorTab::Details {
                        div { style: "display:grid;grid-template-columns:1fr;gap:14px;",
                            div { class: "field",
                                label { r#for: "policy-editor-name", "Name" }
                                input {
                                    id: "policy-editor-name",
                                    class: if name_missing { "input focus-ring mono cf-policy-modal-field-error" } else { "input focus-ring mono" },
                                    placeholder: "e.g. canary-25",
                                    value: "{edit_name}",
                                    oninput: move |event| {
                                        edit_name.set(event.value());
                                        save_error.set(String::new());
                                    },
                                }
                            }
                        }
                        div { class: "field",
                            label { r#for: "policy-editor-description", "Description" }
                            input {
                                id: "policy-editor-description",
                                class: "input focus-ring",
                                placeholder: "One-line summary shown in the registry",
                                value: "{edit_description}",
                                oninput: move |event| edit_description.set(event.value()),
                            }
                        }

                        div { class: "field",
                            label { "Category" }
                            div { role: "radiogroup", style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:8px;",
                                for policy_category in POLICY_CATEGORIES {
                                    {
                                        let id = policy_category.id();
                                        let label = policy_category.label();
                                        let color = policy_category.color();
                                        let icon = policy_category.icon();
                                        let blurb = policy_category.blurb();
                                        rsx! {
                                    button {
                                        key: "{id}",
                                        r#type: "button",
                                        role: "radio",
                                        "data-testid": "policy-category-{id}",
                                        aria_checked: if category.read().as_str() == id { "true" } else { "false" },
                                        class: if category.read().as_str() == id { "cf-policy-category-card cf-policy-category-card-active focus-ring" } else { "cf-policy-category-card focus-ring" },
                                        style: "--cf-policy-category-color:{color};",
                                        // Category is guidance only: selecting a
                                        // different one never touches `rules`.
                                        onclick: move |_| category.set(id.to_string()),
                                        span { style: "flex-shrink:0;width:24px;height:24px;border-radius:6px;display:grid;place-items:center;background:color-mix(in oklab, {color} 16%, transparent);color:{color};",
                                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                if icon == "deploy" {
                                                    path { d: "M12 3v12M6 9l6-6 6 6" }
                                                    rect { x: "4", y: "17", width: "16", height: "4", rx: "1" }
                                                } else if icon == "build" {
                                                    path { d: "M12 3l9 5-9 5-9-5 9-5z" }
                                                    path { d: "M3 13l9 5 9-5" }
                                                } else if icon == "sync" {
                                                    path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8" }
                                                    path { d: "M21 3v5h-5M3 21v-5h5" }
                                                } else {
                                                    path { d: "M12 3l8 3v6c0 4.5-3.3 8.5-8 9-4.7-.5-8-4.5-8-9V6l8-3z" }
                                                }
                                            }
                                        }
                                        span { style: "min-width:0;",
                                            span { style: if category.read().as_str() == id { "display:block;font-size:12px;font-weight:600;color:{color};" } else { "display:block;font-size:12px;font-weight:600;color:var(--cf-text-primary);" }, "{label}" }
                                            span { style: "display:block;font-size:10.5px;color:var(--cf-text-muted);line-height:1.35;margin-top:2px;", "{blurb}" }
                                        }
                                    }
                                        }
                                    }
                                }
                            }
                            div { class: "help", "Category guides which enforcement mechanisms are suggested. It never restricts them, and changing it never edits existing rules." }
                        }

                        if !off_category_labels.is_empty() {
                            div { class: "sd-callout sd-callout-info", "data-testid": "policy-basics-off-category", style: "margin:2px 0 14px;font-size:11.5px;",
                                "{off_category_rule_count} existing "
                                {if off_category_rule_count == 1 { "requirement" } else { "requirements" }}
                                " ({off_category_labels}) "
                                {if off_category_rule_count == 1 { "is" } else { "are" }}
                                " unusual for "
                                strong { "{selected_category.label()}" }
                                ". Nothing was changed or removed."
                            }
                        }

                        if is_security {
                        div { class: "field",
                            label { "Framework" }
                            select {
                                class: "input focus-ring", value: "{framework}",
                                onchange: move |event| framework.set(event.value()),
                                option { value: "", "Select a framework" }
                                for standard in STANDARD_FRAMEWORKS { option { value: "{standard}", "{standard}" } }
                                for existing in framework_options.iter() { option { value: "{existing}", "{existing}" } }
                                option { value: "__custom__", "Define new framework..." }
                            }
                            if framework.read().as_str() == "__custom__" {
                                input {
                                    class: "input focus-ring", style: "margin-top:8px;",
                                    placeholder: "Framework name", value: "{custom_framework}",
                                    oninput: move |event| custom_framework.set(event.value()),
                                }
                            }
                        }

                        if framework.read().as_str() == "DISA STIG" {
                        div { class: "field",
                            label { "SRG IDs" }
                            input {
                                class: "input focus-ring mono",
                                r#type: "text",
                                placeholder: "SRG-OS-000298-GPOS-00116, SRG-OS-000096-GPOS-00050",
                                value: "{edit_srg_ids}",
                                oninput: move |event| edit_srg_ids.set(event.value()),
                            }
                            div { class: "help",
                                "Comma-separated Security Requirements Guide IDs this control satisfies — searchable from the policy list. Persisted to policy version compliance metadata."
                            }
                        }

                        // CCI IDs — PERSISTED to compliance_metadata
                        div { class: "field",
                            label { "CCI IDs" }
                            input {
                                class: "input focus-ring mono",
                                r#type: "text",
                                placeholder: "CCI-000205, CCI-000196",
                                value: "{edit_cci_ids}",
                                oninput: move |event| edit_cci_ids.set(event.value()),
                            }
                            div { class: "help",
                                "Comma-separated CCI mappings, if applicable. Persisted to policy version compliance metadata."
                            }
                        }
                        }
                        if framework.read().as_str() == "NIST 800-53" {
                        div { class: "field",
                            label { "Control family" }
                            select { class: "input focus-ring", value: "{control_family}", onchange: move |event| control_family.set(event.value()),
                                option { value: "", "Unassigned" }
                                for family in NIST_CONTROL_FAMILIES { option { value: "{family}", "{family}" } }
                            }
                        }
                        }
                        if framework.read().as_str() == "CMMC 2.0" {
                        div { class: "field",
                            label { "CMMC level" }
                            select { class: "input focus-ring", value: "{cmmc_level}", onchange: move |event| cmmc_level.set(event.value()),
                                option { value: "", "Unassigned" }
                                option { value: "1", "Level 1" }
                                option { value: "2", "Level 2" }
                                option { value: "3", "Level 3" }
                            }
                        }
                        }
                        if framework.read().as_str() == "CIS Benchmark" {
                        div { class: "field",
                            label { "CIS section" }
                            input { class: "input focus-ring mono", placeholder: "e.g. 5.2.3", value: "{cis_section}", oninput: move |event| cis_section.set(event.value()) }
                        }
                        }
                        div { class: "field",
                            label { "Severity" }
                            div { class: "seg seg-sev", role: "radiogroup", style: "width:fit-content;",
                                for (value, label, color) in [("", "Unset", "var(--cf-text-muted)"), ("high", "High", "#f87171"), ("medium", "Medium", "#fbbf24"), ("low", "Low", "#60a5fa")] {
                                    button {
                                        key: "{value}", r#type: "button", role: "radio",
                                        aria_checked: if severity.read().as_str() == value { "true" } else { "false" },
                                        class: if severity.read().as_str() == value { "active" } else { "" },
                                        style: if severity.read().as_str() == value { "color:{color};background:color-mix(in oklab, {color} 16%, transparent);box-shadow:inset 0 0 0 1px color-mix(in oklab, {color} 45%, transparent);" } else { "color:var(--cf-text-secondary);background:transparent;box-shadow:none;" },
                                        onclick: move |_| severity.set(value.to_string()),
                                        span { style: "display:inline-flex;align-items:center;gap:6px;", span { style: "width:7px;height:7px;border-radius:50%;background:{color};" } "{label}" }
                                    }
                                }
                            }
                            div { class: "help", "Records the control's stated severity independently from enforcement behavior." }
                        }
                        div { class: "field",
                            label { "Rationale" }
                            textarea { class: "input focus-ring", rows: "2", placeholder: "Why this policy exists — shown in detail view", style: "resize:vertical;", value: "{rationale}", oninput: move |event| rationale.set(event.value()) }
                        }
                        }
                        }

                           if *active_tab.read() == PolicyEditorTab::Mappings {
                               PolicyMappingsTab {
                                  is_editing,
                                  editing_policy_version_id,
                                  mappings_editable,
                                  mappings,
                                   pending_mappings,
                                   mapping_load_state,
                                   mapping_load_error,
                               }
                           }

                        // Read-only imported origin, recorded at import time.
                        if *active_tab.read() == PolicyEditorTab::Provenance {
                            div { "data-testid": "policy-editor-provenance", style: "margin-top:6px;display:flex;flex-direction:column;gap:10px;",
                                div { style: "display:flex;align-items:baseline;gap:8px;",
                                    label { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "Provenance" }
                                    span { class: "chip chip-neutral", style: "font-size:10px;", "read-only" }
                                }
                                div { style: "font-size:11.5px;color:var(--cf-text-secondary);line-height:1.5;",
                                    "Recorded when this control was imported. Editing where information came from would rewrite history, so it cannot be changed here. Compliance relationships live in Compliance."
                                }
                                for origin in provenance.iter() {
                                    div { key: "{origin.source_artifact_id}-{origin.origin_policy_version_id}-{origin.source_identity.clone().unwrap_or_default()}",
                                        style: "display:flex;flex-direction:column;gap:3px;padding:9px 11px;background:var(--cf-subtle-bg);border:1px solid var(--cf-divider);border-radius:8px;font-size:11.5px;",
                                        div { style: "display:flex;gap:6px;flex-wrap:wrap;align-items:center;",
                                            span { class: "chip chip-info", style: "font-size:10px;", "Imported" }
                                            if origin.inherited {
                                                span { class: "chip chip-neutral", "data-testid": "policy-provenance-inherited", style: "font-size:10px;", "Inherited from source version" }
                                            }
                                            if let Some(fidelity) = origin.fidelity.as_ref() {
                                                span { class: "chip chip-neutral", style: "font-size:10px;", "{fidelity}" }
                                            }
                                        }
                                        div { style: "display:flex;justify-content:space-between;gap:10px;", span { "Artifact" }, span { class: "mono", "{origin.filename}" } }
                                        div { style: "display:flex;justify-content:space-between;gap:10px;", span { "Source type" }, span { class: "mono", "{origin.media_type}" } }
                                        div { style: "display:flex;justify-content:space-between;gap:10px;", span { "SHA-256" }, span { class: "mono", style: "overflow-wrap:anywhere;", "{origin.sha256}" } }
                                        if let Some(identity) = origin.source_identity.as_ref() {
                                            div { style: "display:flex;justify-content:space-between;gap:10px;", span { {origin.object_kind.clone().map(|kind| format!("Source {kind} ID")).unwrap_or_else(|| "Source ID".to_string())} }, span { class: "mono", "{identity}" } }
                                        }
                                        if let Some(xccdf) = origin.detected_xccdf_version.as_ref() {
                                            div { style: "display:flex;justify-content:space-between;gap:10px;", span { "XCCDF version" }, span { class: "mono", "{xccdf}" } }
                                        }
                                        div { style: "display:flex;justify-content:space-between;gap:10px;", span { "Parser" }, span { class: "mono", "{origin.parser_version}" } }
                                        div { style: "display:flex;justify-content:space-between;gap:10px;",
                                            span { "Imported" }
                                            span { class: "mono", {match origin.imported_by_display.as_ref() { Some(user) => format!("{} · {}", origin.imported_at.to_rfc3339(), user), None => origin.imported_at.to_rfc3339() }} }
                                        }
                                    }
                                }
                            }
                        }

                         // Assertions & gate rules builder
                        if *active_tab.read() == PolicyEditorTab::Enforcement {
                        div { style: "margin-top:6px;",
                            div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px;",
                                label { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "Assertions & gate rules ({rule_count})" }
                                span { style: "font-size:11px;color:var(--cf-text-muted);", "All must hold — each compiles to a policy check." }
                            }
                            if rule_count == 0 {
                                div { class: if is_imported { "sd-callout sd-callout-warn" } else { "sd-callout sd-callout-info" }, "data-testid": "policy-enforcement-empty", style: "margin-bottom:8px;font-size:12px;",
                                    if enforcement_opaque {
                                        span { strong { "Enforcement preserved but unavailable." } " This policy uses an unsupported or opaque representation. It is not a zero-enforcement policy, and this form will not rewrite it." }
                                    } else if is_imported {
                                        span { strong { "Enforcement needs refinement." } " This control was imported with its compliance mappings and provenance, but no assertion was inferred. Until one exists it asserts nothing." }
                                    } else {
                                        span { strong { "No enforcement defined." } " Add at least one requirement for this policy to have an effect. Saving with none is valid; the policy simply asserts nothing." }
                                    }
                                }
                            }
                            div { class: "sd-callout sd-callout-info", "data-testid": "policy-enforcement-recommendations", style: "margin-bottom:8px;font-size:11px;",
                                "Suggested for " strong { "{selected_category.label()}" } ": "
                                if recommended_labels.is_empty() {
                                    span { "data-testid": "policy-enforcement-no-recommendations",
                                        "No rollout-specific enforcement is available in this editor yet. Existing cross-category rules are preserved."
                                    }
                                } else {
                                    span { class: "mono", "{recommended_labels}" }
                                    span { " · Suggestions follow the category; they are never restrictions." }
                                }
                            }
                            if !off_category_labels.is_empty() {
                                div { class: "sd-callout sd-callout-info", "data-testid": "policy-off-category-notice", style: "margin-bottom:8px;font-size:11px;",
                                    "{off_category_labels} " span { {if off_category_rule_count == 1 { "is" } else { "are" }} }
                                    " unusual for " strong { "{selected_category.label()}" } ". Nothing was changed or removed."
                                }
                            }
                            div { style: "display:flex;flex-direction:column;gap:6px;",
                                for (index, rule) in rules.read().iter().cloned().enumerate() {
                                    div {
                                        key: "rule-{rule.id}",
                                        style: "display:grid;grid-template-columns:1fr auto;gap:8px;align-items:center;padding:8px 10px;background:var(--cf-subtle-bg);border-radius:8px;",
                                        RuleEditorRow { index, rule: rule.clone(), rules, enforcement_changed }
                                        div { style: "display:flex;flex-direction:column;gap:3px;",
                                            button { class: "btn-icon focus-ring", title: "Move rule up", disabled: index == 0, onclick: move |_| { let mut next = rules.read().clone(); if index > 0 && index < next.len() { next.swap(index, index - 1); rules.set(next); enforcement_changed.set(true); } }, "↑" }
                                            button { class: "btn-icon focus-ring", title: "Move rule down", disabled: index + 1 >= rule_count, onclick: move |_| { let mut next = rules.read().clone(); if index + 1 < next.len() { next.swap(index, index + 1); rules.set(next); enforcement_changed.set(true); } }, "↓" }
                                            button {
                                                class: "btn-icon focus-ring",
                                                "data-testid": "policy-rule-remove-{index}",
                                                title: "Remove rule",
                                                onclick: move |_| {
                                                    let mut next = rules.read().clone();
                                                    if index < next.len() { next.remove(index); }
                                                    rules.set(next);
                                                    enforcement_changed.set(true);
                                                },
                                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                    path { d: "M18 6 6 18M6 6l12 12" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { style: "margin-top:8px;display:flex;gap:8px;flex-wrap:wrap;",
                                select {
                                    class: "input focus-ring",
                                    "data-testid": "policy-editor-add-rule",
                                    style: "max-width:260px;font-size:12px;",
                                    value: "{add_rule_kind}",
                                    onchange: move |event| {
                                        let kind = event.value();
                                        // Defense in depth: never trust a manipulated
                                        // DOM value. Only kinds the editor can persist
                                        // may be pushed as new rules.
                                        if !kind.is_empty() && rule_kind_is_persisted(&kind) {
                                            let mut next = rules.read().clone();
                                            next.push(PolicyRule::new(&kind));
                                            rules.set(next);
                                            enforcement_changed.set(true);
                                        }
                                        add_rule_kind.set(String::new());
                                    },
                                    option { value: "", disabled: true, "+ Add assertion / rule…" }
                                    for (kind, label, persisted) in RULE_OPTIONS {
                                        if persisted {
                                            option { value: "{kind}", "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                        }

                        // Evidence for ATO builder (UI-only / not persisted)
                         if *active_tab.read() == PolicyEditorTab::Evidence {
                         div { style: "margin-top:6px;",
                             div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px;",
                                 label { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);",
                                     "Evidence for ATO ({evidence_count})"
                                 }
                                 span { style: "font-size:11px;color:var(--cf-text-muted);", "Artifacts collected to prove compliance to an assessor." }
                             }
                             if evidence_count == 0 {
                                 div { class: "sd-callout sd-callout-info", style: "margin-bottom:8px;",
                                     svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                         path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                                         polyline { points: "14 2 14 8 20 8" }
                                     }
                                     div { style: "font-size:12px;",
                                         "No evidence defined. Without it, this policy gates deploys but produces nothing for an audit package. Add command output, logs, or attestations."
                                     }
                                 }
                             }
                             div { style: "display:flex;flex-direction:column;gap:6px;",
                                 for (index, ev) in evidence.read().iter().cloned().enumerate() {
                                     div {
                                         key: "ev-{index}",
                                         style: "display:grid;grid-template-columns:1fr auto;gap:8px;align-items:flex-start;padding:8px 10px;background:var(--cf-subtle-bg);border-radius:8px;",
                                         EvidenceEditorRow { index, evidence: ev.clone(), evidence_list: evidence }
                                         button {
                                             class: "btn-icon focus-ring",
                                             title: "Remove evidence",
                                             onclick: move |_| {
                                                 let mut next = evidence.read().clone();
                                                 if index < next.len() { next.remove(index); }
                                                 evidence.set(next);
                                             },
                                             svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                 path { d: "M18 6 6 18M6 6l12 12" }
                                             }
                                         }
                                     }
                                 }
                             }
                             div { style: "margin-top:8px;display:flex;gap:6px;",
                                 select {
                                     class: "input focus-ring",
                                     "data-testid": "policy-editor-add-evidence",
                                     style: "flex:1;font-size:12px;",
                                     value: "{add_evidence_kind}",
                                     onchange: move |event| {
                                         let kind = event.value();
                                         if !kind.is_empty() {
                                             let mut next = evidence.read().clone();
                                             next.push(PolicyEvidence::new(&kind));
                                             evidence.set(next);
                                         }
                                         add_evidence_kind.set(String::new());
                                     },
                                     option { value: "", "+ Add evidence source…" }
                                     for (id, label) in EVIDENCE_OPTIONS {
                                         option { value: "{id}", "{label}" }
                                     }
                                 }
                                 if evidence_count > 0 {
                                     button {
                                         class: "btn btn-ghost focus-ring",
                                         style: "color:#f87171;border-color:rgba(248,113,113,0.3);",
                                         title: "Clear all evidence",
                                         onclick: move |_| {
                                             evidence.set(Vec::new());
                                         },
                                         "Clear all"
                                     }
                                 }
                             }
                         }
                         }

                        }

                        if is_editing {
                            div { style: "margin-top:10px;padding-top:14px;border-top:1px solid var(--cf-divider);",
                                div { style: "font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);margin-bottom:8px;", "Danger zone" }
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    style: "color:#f87171;border-color:rgba(248,113,113,0.3);",
                                    onclick: move |_| confirm_delete.set(true),
                                    svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:6px;vertical-align:text-bottom;",
                                        path { d: "M18 6 6 18M6 6l12 12" }
                                    }
                                    "Remove policy"
                                }
                            }
                        }

                        if !save_error.read().is_empty() {
                            div { class: "text-xs rounded px-3 py-2 cf-policy-modal-error", style: "margin-top:10px;", "{save_error}" }
                        }

                        if let Some(blocker) = current_save_blocker.as_ref() {
                            div { class: "text-xs rounded px-3 py-2 cf-policy-modal-error", style: "margin-top:10px;", "{blocker}" }
                        }
                    }

                    // ── Footer ──────────────────────────────────────────────────
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: "btn btn-primary focus-ring",
                            disabled: !can_save,
                            onclick: move |_| {
                                let name = edit_name.read().clone();
                                if name.trim().is_empty() {
                                    save_error.set("Policy name is required".to_string());
                                    return;
                                }
                                let description = edit_description.read().clone();
                                let editing_id = *editing_policy_id.read();
                                let current_rules = rules.read().clone();
                                if let Some(blocker) = save_blocker(
                                    editing_id.is_some(),
                                    *edit_format.read(),
                                    &existing_type,
                                    &existing_config,
                                    &current_rules,
                                ) {
                                    save_error.set(blocker);
                                    return;
                                }
                                let mut policy_library = policy_library;
                                let mut save_error = save_error;
                                let mut is_saving = is_saving;
                                let on_close = on_close;

                                let persisted = persisted_payload_for_save(
                                    editing_id.is_some(),
                                    *enforcement_changed.read(),
                                    &existing_type,
                                    &existing_config,
                                    &current_rules,
                                );
                                let Some((policy_type, config)) = persisted else {
                                    save_error.set("Add at least one backend-supported assertion before saving.".to_string());
                                    return;
                                };

                                // Parse comma-separated SRG/CCI input into vectors.
                                let srg_raw: Vec<String> = edit_srg_ids
                                    .read()
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                let cci_raw: Vec<String> = edit_cci_ids
                                    .read()
                                    .split(',')
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                let selected_framework = if framework.read().as_str() == "__custom__" {
                                    non_empty(custom_framework.read().clone())
                                } else {
                                    non_empty(framework.read().clone())
                                };
                                let selected_cmmc_level = if is_security { cmmc_level.read().trim().parse::<i32>().ok() } else { None };
                                let selected_category = Some(category.read().clone());
                                let selected_severity = if is_security { non_empty(severity.read().clone()) } else { None };
                                let selected_control_family = if is_security && selected_framework.as_deref() == Some("NIST 800-53") { non_empty(control_family.read().clone()) } else { None };
                                let selected_cis_section = if is_security && selected_framework.as_deref() == Some("CIS Benchmark") { non_empty(cis_section.read().clone()) } else { None };
                                let selected_rationale = non_empty(rationale.read().clone());

                                 save_error.set(String::new());
                                is_saving.set(true);

                                 // Validate evidence rows BEFORE async block
                                 {
                                     let current_evidence = evidence.read();
                                     let validation_errors: Vec<String> = current_evidence
                                         .iter()
                                         .enumerate()
                                         .filter_map(|(idx, ev)| {
                                             ev.validate().map(|err| format!("Evidence row {}: {}", idx + 1, err))
                                         })
                                         .collect();

                                     if !validation_errors.is_empty() {
                                         save_error.set(validation_errors.join("; "));
                                         is_saving.set(false);
                                         return;
                                     }
                                 }

                                 let initial_evidence_clone = initial_evidence.clone();
                                 spawn(async move {
                                     let result = if let Some(policy_id) = editing_id {
                                          // Determine evidence_specs dirty state:
                                          // Compare current evidence against initial state
                                          // - None if unchanged (preserve existing)
                                          // - Some([]) if cleared to empty
                                          // - Some(items) if modified/added
                                          let evidence_specs = {
                                              let current_evidence = evidence.read();
                                              let current_count = current_evidence.len();
                                              let initial_count = initial_evidence_clone.len();

                                              // No change = preserve
                                              if current_count == initial_count && current_evidence.clone() == initial_evidence_clone {
                                                  None
                                              } else {
                                                  // Changed: convert and send (including empty array if cleared)
                                                  let specs: Vec<EvidenceSpec> = current_evidence
                                                      .iter()
                                                      .map(|ev| ev.to_evidence_spec())
                                                      .collect();
                                                  Some(specs)
                                              }
                                          };

                                         let request = UpdateDeploymentPolicyRequest {
                                              name: Some(name.clone()),
                                              description: Some(description.clone()),
                                              policy_type: Some(policy_type),
                                              config: Some(config),
                                              enabled: None,
                                              // Always send Some(...) so the server replaces
                                              // the curated mapping (Some([]) clears it).
                                              srg_ids: Some(srg_raw),
                                              cci_ids: Some(cci_raw),
                                              category: selected_category,
                                              framework: selected_framework,
                                              severity: selected_severity,
                                              control_family: selected_control_family,
                                              cmmc_level: selected_cmmc_level,
                                              cis_section: selected_cis_section,
                                              rationale: selected_rationale,
                                              evidence_specs,
                                          };
                                        update_deployment_policy(&policy_id, &request).await.map(|_| None)
                                     } else {
                                          let evidence_specs: Vec<EvidenceSpec> = evidence.read()
                                              .iter()
                                              .map(|ev| ev.to_evidence_spec())
                                              .collect();

                                         let request = CreateDeploymentPolicyRequest {
                                              name: name.clone(),
                                              description: Some(description.clone()),
                                              policy_type,
                                              config,
                                              enabled: Some(true),
                                              srg_ids: srg_raw,
                                              cci_ids: cci_raw,
                                              category: selected_category,
                                              framework: selected_framework,
                                              severity: selected_severity,
                                              control_family: selected_control_family,
                                              cmmc_level: selected_cmmc_level,
                                              cis_section: selected_cis_section,
                                              rationale: selected_rationale,
                                              evidence_specs,
                                              requirement_mappings: pending_mappings
                                                  .read()
                                                  .iter()
                                                  .map(PendingPolicyMapping::mapping_request)
                                                  .collect(),
                                          };
                                        create_deployment_policy(&request).await.map(Some)
                                    };

                                    match result {
                                        Ok(created_opt) => {
                                            // Fetch the updated list so edits (name changes, etc.) are
                                            // reflected globally. The list response includes the
                                            // current_version_id join that the create endpoint omits,
                                            // so for a new policy we prefer the entry from the refreshed
                                            // list over the raw create response. If the refresh fails we
                                            // fall back to the create response so the card is still shown.
                                            let created_id = created_opt.as_ref().map(|c| c.id);
                                            match policies_api::load_policies().await {
                                                policies_api::PolicyLoadResult::Ok(mut latest) => {
                                                    // For new policies, ensure the entry is at the front.
                                                    // The list response carries the full current_version_id,
                                                    // so prefer it over the raw create response.
                                                    if let Some(id) = created_id {
                                                        if !latest.iter().any(|p| p.id == id) {
                                                            // Not on first page — fall back to create response.
                                                            if let Some(created) = created_opt {
                                                                let def = policies_api::policy_record_to_definition(created);
                                                                latest.insert(0, def);
                                                            }
                                                        } else {
                                                            // Reorder so newly created policy is first.
                                                            if let Some(pos) = latest.iter().position(|p| p.id == id) {
                                                                let item = latest.remove(pos);
                                                                latest.insert(0, item);
                                                            }
                                                        }
                                                    }
                                                    policy_library.set(latest);
                                                }
                                                policies_api::PolicyLoadResult::Err(error) => {
                                                    // Best-effort: if a new policy was created, still
                                                    // insert it so the UI is not broken.
                                                    if let Some(created) = created_opt {
                                                        let def = policies_api::policy_record_to_definition(created);
                                                        let mut current = policy_library.read().clone();
                                                        current.retain(|p| p.id != def.id);
                                                        current.insert(0, def);
                                                        policy_library.set(current);
                                                    }
                                                    save_error.set(format!("Policy saved, but list refresh failed: {error}"));
                                                }
                                            }
                                            is_saving.set(false);
                                            on_close.call(());
                                        }
                                        Err(error) => {
                                            save_error.set(format!("Failed to save policy: {error}"));
                                            is_saving.set(false);
                                        }
                                    }
                                });
                            },
                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:6px;vertical-align:text-bottom;",
                                path { d: "M20 6 9 17l-5-5" }
                            }
                            if *is_saving.read() { "Saving…" } else { "{action_label}" }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rule editor row
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum OptionSearchState {
    Idle,
    Loading,
    Error(String),
    Unavailable(String),
    Zero,
    Results(Vec<NixosOptionMetadata>),
}

#[component]
fn PolicyEditorTabButton(
    tab: PolicyEditorTab,
    active: PolicyEditorTab,
    label: String,
    test_id: String,
    on_select: EventHandler<()>,
) -> Element {
    let selected = tab == active;
    let class = match tab {
        PolicyEditorTab::Details => "source",
        PolicyEditorTab::Mappings => "mappings",
        PolicyEditorTab::Enforcement => "enforcement",
        PolicyEditorTab::Evidence => "evidence",
        PolicyEditorTab::Provenance => "provenance",
    };
    rsx! { button { class: if selected { format!("cf-modal-tab cf-modal-tab--active cf-modal-tab--{class}") } else { format!("cf-modal-tab cf-modal-tab--{class}") }, role: "tab", aria_selected: if selected { "true" } else { "false" }, "data-testid": "{test_id}", onclick: move |_| on_select.call(()), "{label}" } }
}

#[component]
fn RuleEditorRow(
    index: usize,
    rule: PolicyRule,
    rules: Signal<Vec<PolicyRule>>,
    mut enforcement_changed: Signal<bool>,
) -> Element {
    let kind = rule.kind.clone();
    let persisted = rule.is_persisted();
    let mut option_search = use_signal(|| OptionSearchState::Idle);
    let mut search_generation = use_signal(|| 0_u64);
    let mut initial_metadata_requested = use_signal(|| false);

    // Mutate one field of the rule at `index` and write it back to the signal.
    macro_rules! set_rule_field {
        ($field:ident, $value:expr) => {{
            let mut next = rules.read().clone();
            if let Some(target) = next.get_mut(index) {
                target.$field = $value;
            }
            rules.set(next);
            enforcement_changed.set(true);
        }};
    }

    if kind == "nixos_option" && !*initial_metadata_requested.read() {
        initial_metadata_requested.set(true);
        let path = rule.path.clone();
        let mut rules_for_lookup = rules;
        spawn(async move {
            if let Ok(response) = search_nixos_options(&path, 10).await {
                if let Some(metadata) = response.iter().find(|item| item.path == path) {
                    let mut next = rules_for_lookup.read().clone();
                    if let Some(target) = next
                        .get_mut(index)
                        .filter(|target| target.id == rule.id && target.path == path)
                    {
                        target.enrich_option_metadata(metadata);
                        rules_for_lookup.set(next);
                    }
                }
            }
        });
    }

    rsx! {
        div { style: "display:flex;flex-direction:column;gap:4px;font-size:12px;width:100%;",
            div { style: "display:flex;align-items:center;gap:6px;",
                span { style: "font-weight:600;", "{rule_label(&kind)}" }
                if !persisted {
                    span { class: "cf-policy-ui-only-badge", "UI only" }
                }
            }

            match kind.as_str() {
                "eval_passed" => rsx! { span { style: "color:var(--cf-text-secondary);", "Evaluation must pass" } },
                "build_succeeded" => rsx! { span { style: "color:var(--cf-text-secondary);", "Build must succeed" } },
                "cve_block" => rsx! {
                    div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        span { "Block deploy when" }
                        select {
                            "data-testid": "policy-evidence-log-source-{index}",
                            class: "input focus-ring",
                            style: "width:auto;font-size:12px;padding:4px 8px;",
                            value: "{rule.severity}",
                            onchange: move |event| set_rule_field!(severity, event.value()),
                            option { value: "critical", "critical" }
                            option { value: "high", "high" }
                            option { value: "medium", "medium" }
                        }
                        span { "CVEs exceed" }
                        input {
                            r#type: "number", min: "0",
                            class: "input focus-ring mono",
                            style: "width:60px;font-size:12px;padding:4px 8px;",
                            value: "{rule.max_allowed}",
                            oninput: move |event| set_rule_field!(max_allowed, event.value()),
                        }
                    }
                },
                "packages_installed" => rsx! {
                    input {
                        class: "input focus-ring mono",
                        style: "font-size:12px;padding:5px 8px;",
                        placeholder: "openssh, auditd, aide",
                        value: "{rule.packages}",
                        oninput: move |event| set_rule_field!(packages, event.value()),
                    }
                },
                "nixos_option" => rsx! {
                    div { style: "display:flex;flex-direction:column;gap:6px;",
                        input {
                            "data-testid": "policy-rule-nixos-path-{index}",
                            class: "input focus-ring mono",
                            style: "font-size:11px;padding:5px 8px;width:100%;",
                            placeholder: "services.openssh.settings.PermitRootLogin",
                            value: "{rule.path}",
                            oninput: move |event| {
                                let query = event.value();
                                let mut next = rules.read().clone();
                                if let Some(target) = next.get_mut(index) {
                                    target.path = query.clone();
                                    target.option_type = "unknown".to_string();
                                    target.option_values.clear();
                                    target.option_description.clear();
                                    target.option_unit = None;
                                    target.baseline_option_type = None;
                                    if !target.value.is_string() {
                                        target.value = serde_json::Value::String(target.value.to_string());
                                    }
                                }
                                rules.set(next);
                                enforcement_changed.set(true);
                                let generation = *search_generation.read() + 1;
                                search_generation.set(generation);
                                if query.trim().is_empty() {
                                    option_search.set(OptionSearchState::Idle);
                                    return;
                                }
                                option_search.set(OptionSearchState::Loading);
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(250).await;
                                    let result = search_nixos_options(&query, 20).await;
                                    if *search_generation.read() != generation { return; }
                                    match result {
                                        Ok(response) if response.is_empty() => option_search.set(OptionSearchState::Zero),
                                        Ok(response) => option_search.set(OptionSearchState::Results(response)),
                                        Err(ApiClientError::Status { code: 503, body }) => option_search.set(OptionSearchState::Unavailable(if body.is_empty() { "Crystal Forge baseline metadata is unavailable. Manual paths remain available, and target evaluation remains authoritative.".to_string() } else { body })),
                                        Err(error) => option_search.set(OptionSearchState::Error(error.to_string())),
                                    }
                                });
                            },
                        }
                        match option_search.read().clone() {
                            OptionSearchState::Idle => rsx! { span { "data-testid": "policy-option-search-idle", class: "help", "Type any option path, or search the metadata index." } },
                            OptionSearchState::Loading => rsx! { span { "data-testid": "policy-option-search-loading", class: "help", "Searching NixOS options…" } },
                            OptionSearchState::Error(error) => rsx! { span { "data-testid": "policy-option-search-error", class: "help", style: "color:#f87171;", "Search failed: {error}. You can still enter a path manually." } },
                            OptionSearchState::Unavailable(message) => rsx! { span { "data-testid": "policy-option-search-unavailable", class: "help", "{message}" } },
                            OptionSearchState::Zero => rsx! { span { "data-testid": "policy-option-search-zero", class: "help", "No Crystal Forge baseline metadata matches. The option may still be valid for the target and will be kept as a custom string option." } },
                            OptionSearchState::Results(results) => rsx! {
                                div { "data-testid": "policy-option-search-results", style: "display:flex;flex-direction:column;max-height:150px;overflow:auto;border:1px solid var(--cf-divider);border-radius:7px;",
                                    for metadata in results {
                                        {
                                            let selected = metadata.clone();
                                            rsx! { button { r#type: "button", class: "btn btn-ghost focus-ring", style: "text-align:left;display:block;padding:6px 8px;border-radius:0;", onclick: move |_| {
                                                let mut next = rules.read().clone();
                                                if let Some(target) = next.get_mut(index) { target.apply_option_metadata(&selected); }
                                                rules.set(next);
                                                enforcement_changed.set(true);
                                                option_search.set(OptionSearchState::Idle);
                                            },
                                                span { class: "mono", style: "display:block;font-size:11px;", "{metadata.path}" }
                                                span { style: "display:block;font-size:10px;color:var(--cf-text-muted);", "{metadata.value_type.as_str()} · {metadata.description.as_deref().unwrap_or_default()}" }
                                            } }
                                        }
                                    }
                                }
                            },
                        }
                        if !rule.option_description.is_empty() {
                            span { class: "help", "{rule.option_description}" }
                        }
                        if let Some(advisory) = rule.baseline_advisory() {
                            div {
                                "data-testid": "policy-rule-nixos-baseline-advisory-{index}",
                                class: "sd-callout sd-callout-warn",
                                style: "font-size:11px;padding:7px 9px;",
                                "{advisory}"
                            }
                        }
                        div { style: "display:flex;gap:6px;align-items:flex-start;flex-wrap:wrap;",
                            select {
                                "data-testid": "policy-rule-nixos-operator-{index}",
                                aria_label: "Comparison operator",
                                class: "input focus-ring mono",
                                style: "width:auto;font-size:12px;padding:5px 6px;",
                                value: "{rule.op}",
                                onchange: move |event| set_rule_field!(op, event.value()),
                                option { value: "==", "==" }
                                option { value: "!=", "!=" }
                                if normalize_option_type(&rule.option_type) == "integer" {
                                    option { value: ">=", "≥" }
                                    option { value: "<=", "≤" }
                                }
                            }
                            match normalize_option_type(&rule.option_type) {
                                "boolean" => rsx! { select { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected boolean value", class: "input focus-ring mono", value: if rule.value.as_bool().unwrap_or(false) { "true" } else { "false" }, onchange: move |event| set_rule_field!(value, serde_json::Value::Bool(event.value() == "true")), option { value: "true", "true" } option { value: "false", "false" } } },
                                "enum" if !rule.option_values.is_empty() && rule.value.as_str().is_some_and(|value| rule.option_values.iter().any(|candidate| candidate == value)) => rsx! { select { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected enum value", class: "input focus-ring mono", value: "{rule.value.as_str().unwrap_or_default()}", onchange: move |event| set_rule_field!(value, serde_json::Value::String(event.value())), for value in rule.option_values.iter() { option { value: "{value}", selected: rule.value.as_str() == Some(value.as_str()), "{value}" } } } },
                                "integer" => rsx! { input { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected integer value", r#type: "number", class: "input focus-ring mono", value: "{rule.value.as_i64().unwrap_or_default()}", oninput: move |event| { if let Ok(value) = event.value().parse::<i64>() { set_rule_field!(value, serde_json::json!(value)); } } } },
                                "lines" => rsx! { textarea { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected multiline value", class: "input focus-ring mono code-editor", rows: "8", style: "flex:1;min-width:260px;resize:vertical;", value: "{rule.value.as_str().unwrap_or_default()}", oninput: move |event| set_rule_field!(value, serde_json::Value::String(event.value())) } },
                                _ => rsx! { input { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected string value", class: "input focus-ring mono", style: "flex:1;min-width:180px;", value: "{rule.value.as_str().unwrap_or_default()}", oninput: move |event| set_rule_field!(value, serde_json::Value::String(event.value())) } },
                            }
                            span { class: "chip chip-neutral", style: "font-size:10px;", "{normalize_option_type(&rule.option_type)}" }
                            if let Some(unit) = rule.option_unit.as_ref() { span { class: "help", "{unit}" } }
                        }
                    }
                },
                "custom_eval" => rsx! {
                    textarea {
                        class: "input focus-ring mono code-editor",
                        "data-testid": "policy-rule-custom-eval-expr-{index}",
                        rows: "3",
                        style: "font-size:12px;resize:vertical;",
                        placeholder: "config.networking.firewall.enable == true",
                        value: "{rule.expr}",
                        oninput: move |event| set_rule_field!(expr, event.value()),
                    }
                    input {
                        class: "input focus-ring",
                        style: "font-size:11px;padding:5px 8px;",
                        placeholder: "Failure message shown when assertion fails",
                        value: "{rule.message}",
                        oninput: move |event| set_rule_field!(message, event.value()),
                    }
                },
                "time_window" => rsx! {
                    div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        span { "Only between" }
                        input { class: "input focus-ring mono", style: "width:70px;font-size:12px;padding:4px 8px;", value: "{rule.from}", oninput: move |event| set_rule_field!(from, event.value()) }
                        span { "–" }
                        input { class: "input focus-ring mono", style: "width:70px;font-size:12px;padding:4px 8px;", value: "{rule.to}", oninput: move |event| set_rule_field!(to, event.value()) }
                        span { "on" }
                        input { class: "input focus-ring mono", style: "width:140px;font-size:12px;padding:4px 8px;", value: "{rule.days}", oninput: move |event| set_rule_field!(days, event.value()) }
                        span { class: "mono", style: "color:var(--cf-text-muted);font-size:11px;", "America/New_York" }
                    }
                },
                "approval_required" => rsx! {
                    div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        span { "Require" }
                        input { r#type: "number", min: "1", class: "input focus-ring mono", style: "width:50px;font-size:12px;padding:4px 8px;", value: "{rule.count}", oninput: move |event| set_rule_field!(count, event.value()) }
                        span { "approver(s) with role" }
                        select {
                            class: "input focus-ring",
                            style: "width:auto;font-size:12px;padding:4px 8px;",
                            value: "{rule.role}",
                            onchange: move |event| set_rule_field!(role, event.value()),
                            option { value: "admin", "admin" }
                            option { value: "operator", "operator" }
                            option { value: "any", "any" }
                        }
                    }
                },
                "rollout_percent" => rsx! {
                    div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        span { "Roll out" }
                        input { r#type: "number", min: "1", max: "100", class: "input focus-ring mono", style: "width:55px;font-size:12px;padding:4px 8px;", value: "{rule.percent}", oninput: move |event| set_rule_field!(percent, event.value()) }
                        span { "% at a time, observe" }
                        input { r#type: "number", min: "1", class: "input focus-ring mono", style: "width:55px;font-size:12px;padding:4px 8px;", value: "{rule.observe_min}", oninput: move |event| set_rule_field!(observe_min, event.value()) }
                        span { "min" }
                    }
                },
                _ => rsx! { span { style: "font-style:italic;", "{kind}" } },
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Evidence editor row (all UI-only)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EvidenceEditorRow(
    index: usize,
    evidence: PolicyEvidence,
    evidence_list: Signal<Vec<PolicyEvidence>>,
) -> Element {
    let kind = evidence.kind.clone();

    // Mutate one field of the evidence at `index` and write it back to the signal.
    macro_rules! set_ev_field {
        ($field:ident, $value:expr) => {{
            let mut next = evidence_list.read().clone();
            if let Some(target) = next.get_mut(index) {
                target.$field = $value;
            }
            evidence_list.set(next);
        }};
    }

    let label = EVIDENCE_OPTIONS
        .iter()
        .find(|(id, _)| *id == kind)
        .map(|(_, l)| *l)
        .unwrap_or("Evidence");

    // `match` is a reserved word; bind it locally for use in the format string.
    let match_value = evidence.r#match.clone();

    rsx! {
        div { style: "display:flex;flex-direction:column;gap:4px;font-size:12px;width:100%;",
            span { style: "display:flex;align-items:center;gap:6px;font-weight:600;", "{label}" }
            match kind.as_str() {
                "command" => rsx! {
                    input { "data-testid": "policy-evidence-command-cmd-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "sshd -T | grep permitrootlogin", value: "{evidence.cmd}", oninput: move |event| set_ev_field!(cmd, event.value()) }
                    div { style: "display:flex;align-items:center;gap:6px;",
                        span { style: "font-size:11px;color:var(--cf-text-muted);", "expect output contains" }
                        input { "data-testid": "policy-evidence-command-expect-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;", placeholder: "permitrootlogin no", value: "{evidence.expect}", oninput: move |event| set_ev_field!(expect, event.value()) }
                    }
                },
                "log" => rsx! {
                    div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                        select {
                            "data-testid": "policy-evidence-log-source-{index}",
                            class: "input focus-ring",
                            style: "font-size:11px;padding:5px 8px;width:auto;",
                            value: "{evidence.source}",
                            onchange: move |event| set_ev_field!(source, event.value()),
                            option { value: "journald", "journald" }
                            option { value: "auditd", "auditd" }
                            option { value: "file", "file" }
                        }
                        input { "data-testid": "policy-evidence-log-unit-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;min-width:140px;", placeholder: "auditd.service", value: "{evidence.unit}", oninput: move |event| set_ev_field!(unit, event.value()) }
                    }
                    input { "data-testid": "policy-evidence-log-match-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "regex / substring to match", value: "{match_value}", oninput: move |event| set_ev_field!(r#match, event.value()) }
                },
                "file" => rsx! {
                    input { "data-testid": "policy-evidence-file-path-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "/etc/issue", value: "{evidence.path}", oninput: move |event| set_ev_field!(path, event.value()) }
                    input { class: "input focus-ring", style: "font-size:11px;padding:5px 8px;", placeholder: "What to look for / why it proves compliance", value: "{evidence.note}", oninput: move |event| set_ev_field!(note, event.value()) }
                },
                "unit_state" => rsx! {
                    div { style: "display:flex;gap:6px;align-items:center;flex-wrap:wrap;",
                        input { "data-testid": "policy-evidence-unit-name-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;min-width:140px;", placeholder: "auditd.service", value: "{evidence.unit}", oninput: move |event| set_ev_field!(unit, event.value()) }
                        span { style: "font-size:11px;color:var(--cf-text-muted);", "is" }
                        select {
                            "data-testid": "policy-evidence-unit-state-{index}",
                            class: "input focus-ring",
                            style: "font-size:11px;padding:5px 8px;width:auto;",
                            value: "{evidence.state}",
                            onchange: move |event| set_ev_field!(state, event.value()),
                            option { value: "active", "active" }
                            option { value: "enabled", "enabled" }
                            option { value: "masked", "masked" }
                        }
                    }
                },
                "eval_attr" => rsx! {
                    input { "data-testid": "policy-evidence-eval-attr-{index}", class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "config.services.openssh.settings.PermitRootLogin", value: "{evidence.attr}", oninput: move |event| set_ev_field!(attr, event.value()) }
                    span { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);", "Captured from the evaluated config — no host access needed." }
                },
                "attestation" => rsx! {
                    input { "data-testid": "policy-evidence-attestation-note-{index}", class: "input focus-ring", style: "font-size:11px;padding:5px 8px;", placeholder: "What the agent attests to (signed snapshot)", value: "{evidence.note}", oninput: move |event| set_ev_field!(note, event.value()) }
                    span { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);", "Ed25519-signed by the agent at collection time." }
                },
                _ => rsx! { span { style: "font-style:italic;", "{kind}" } },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(id: u128) -> PendingPolicyMapping {
        PendingPolicyMapping {
            requirement_version_id: Uuid::from_u128(id),
            framework_name: "NIST 800-53".into(),
            framework_version: "Rev 5".into(),
            requirement_external_id: "SC-45".into(),
            requirement_kind: "control".into(),
            requirement_title: Some("System time synchronization".into()),
            relationship: "supports".into(),
            coverage: "partial".into(),
            rationale: Some("reviewed mapping".into()),
        }
    }

    #[test]
    fn pending_mapping_helpers_preserve_convert_and_reject_duplicates() {
        let mut mappings = Vec::new();
        let mapping = pending(1);
        assert!(add_pending_mapping(&mut mappings, mapping.clone()).is_ok());
        let request = mappings[0].mapping_request();
        assert_eq!(request.requirement_version_id, Uuid::from_u128(1));
        assert_eq!(request.relationship, "supports");
        assert_eq!(request.coverage, "partial");
        assert_eq!(request.rationale.as_deref(), Some("reviewed mapping"));
        assert_eq!(request.provenance, "manual");
        assert!(add_pending_mapping(&mut mappings, mapping).is_err());
        remove_pending_mapping(&mut mappings, Uuid::from_u128(1));
        assert!(mappings.is_empty());
    }

    #[test]
    fn mapping_editor_target_distinguishes_create_edit_and_immutable_modes() {
        let version = Uuid::from_u128(7);
        assert_eq!(
            mapping_editor_target(false, None, false),
            MappingEditorTarget::Pending
        );
        assert_eq!(
            mapping_editor_target(true, Some(version), true),
            MappingEditorTarget::Persisted(version)
        );
        assert_eq!(
            mapping_editor_target(true, Some(version), false),
            MappingEditorTarget::Unavailable
        );
        assert_eq!(
            mapping_editor_target(true, None, true),
            MappingEditorTarget::Unavailable
        );
    }

    #[test]
    fn pending_mapping_builder_captures_selection_metadata() {
        let framework = ComplianceFrameworkSummary {
            id: Uuid::from_u128(1),
            name: "NIST 800-53".into(),
            publisher: None,
            canonical_source_key: "nist".into(),
            description: None,
            version_count: 1,
        };
        let version = ComplianceFrameworkVersionSummary {
            id: Uuid::from_u128(2),
            framework_id: framework.id,
            version: "Rev 5".into(),
            canonical_release_key: "rev5".into(),
            title: None,
            published_at: None,
            semantic_digest: "digest".into(),
            migration_recovery_status: "finalized".into(),
            migration_recovery_reason: None,
            requirement_count: 1,
        };
        let requirement = RequirementVersionSummary {
            id: Uuid::from_u128(3),
            requirement_id: Uuid::from_u128(4),
            framework_version_id: version.id,
            external_id: "SC-45".into(),
            title: Some("System time synchronization".into()),
            kind: "control".into(),
            severity: None,
            parent_requirement_version_id: None,
            semantic_digest: "req".into(),
        };
        let mapping = pending_mapping_from_selection(
            &framework,
            &version,
            &requirement,
            "supports".into(),
            "partial".into(),
            Some("reviewed".into()),
        );
        assert_eq!(mapping.requirement_version_id, requirement.id);
        assert_eq!(mapping.framework_name, "NIST 800-53");
        assert_eq!(mapping.framework_version, "Rev 5");
        assert_eq!(mapping.requirement_external_id, "SC-45");
        assert_eq!(mapping.requirement_kind, "control");
        assert_eq!(
            mapping.requirement_title.as_deref(),
            Some("System time synchronization")
        );
        assert_eq!(mapping.relationship, "supports");
        assert_eq!(mapping.coverage, "partial");
        assert_eq!(mapping.rationale.as_deref(), Some("reviewed"));
    }

    #[test]
    fn cve_rule_combination_is_a_typed_composite_not_an_always_true_check() {
        let rules = vec![PolicyRule::new("cve_block"), PolicyRule::new("custom_eval")];

        let (policy_type, config) = build_persisted_payload(&rules).expect("composite");
        assert_eq!(policy_type, "composite");
        assert_eq!(config["schema_version"], 1);
        assert_eq!(config["mode"], "all");
        assert_eq!(config["rules"][0]["kind"], "cve_block");
        assert_eq!(config["rules"][1]["kind"], "custom_eval");
        assert!(!config.to_string().contains("\"expression\":\"true\""));
        assert!(
            save_blocker(
                false,
                PolicyFormat::Json,
                "custom_check",
                &serde_json::Value::Null,
                &rules
            )
            .is_none()
        );
    }

    #[test]
    fn high_cve_policy_reconstructs_as_high_threshold() {
        let config = serde_json::json!({
            "max_critical": 0,
            "max_high": 7,
            "strict": true,
            "when_no_scan": "block"
        });

        let rules = rules_from_policy("require_cve_check", &config);

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].severity, "high");
        assert_eq!(rules[0].max_allowed, "7");
        assert!(cve_config_is_representable(&config));
    }

    #[test]
    fn complex_cve_policy_is_blocked_from_destructive_edit() {
        let config = serde_json::json!({
            "max_critical": 1,
            "max_high": 5,
            "strict": true,
            "when_no_scan": "block"
        });
        let rules = rules_from_policy("require_cve_check", &config);

        assert!(
            save_blocker(
                true,
                PolicyFormat::Json,
                "require_cve_check",
                &config,
                &rules
            )
            .is_some()
        );
    }

    #[test]
    fn unsupported_rules_block_save_instead_of_creating_noop_policy() {
        let rules = vec![
            PolicyRule::new("eval_passed"),
            PolicyRule::new("build_succeeded"),
        ];

        assert!(build_persisted_payload(&rules).is_none());
        assert!(
            save_blocker(
                false,
                PolicyFormat::Json,
                "custom_check",
                &serde_json::Value::Null,
                &rules
            )
            .is_some()
        );
    }

    #[test]
    fn editor_state_keeps_origin_enforcement_and_mapping_independent() {
        let loaded = MappingLoadState::Loaded;

        // custom + rules > 0 + mappings == 0 => Unmapped
        let custom_enforced = policy_editor_state(false, loaded, 0, 1, 0);
        assert_eq!(custom_enforced.enforcement, "Enforced");
        assert_eq!(custom_enforced.compliance, "Unmapped");
        assert!(!custom_enforced.mapped_not_enforced);

        // custom + rules == 0 + mappings == 0 => No enforcement defined + Unmapped
        let custom_empty = policy_editor_state(false, loaded, 0, 0, 0);
        assert_eq!(custom_empty.enforcement, "No enforcement defined");
        assert_eq!(custom_empty.compliance, "Unmapped");
        assert!(!custom_empty.mapped_not_enforced);

        // custom + rules == 0 + mappings > 0 => Mapped but no enforcement
        let custom_mapped = policy_editor_state(false, loaded, 2, 0, 0);
        assert_eq!(custom_mapped.enforcement, "No enforcement defined");
        assert_eq!(custom_mapped.compliance, "Mapped");
        assert!(custom_mapped.mapped_not_enforced);

        // imported + rules == 0 + mappings == 0 => needs refinement + Unmapped
        let imported_empty = policy_editor_state(true, loaded, 0, 0, 0);
        assert_eq!(imported_empty.enforcement, "Enforcement needs refinement");
        assert_eq!(imported_empty.compliance, "Unmapped");
        assert!(!imported_empty.mapped_not_enforced);

        // imported + rules == 0 + mappings > 0 => needs refinement + warning
        let imported_mapped = policy_editor_state(true, loaded, 4, 0, 1);
        assert_eq!(imported_mapped.enforcement, "Enforcement needs refinement");
        assert_eq!(imported_mapped.compliance, "Mapped");
        assert!(imported_mapped.mapped_not_enforced);
        assert_eq!(imported_mapped.evidence, "Evidence collected");
    }

    #[test]
    fn mapping_load_failure_is_never_reported_as_unmapped() {
        let loading = policy_editor_state(false, MappingLoadState::Loading, 0, 1, 0);
        assert_eq!(loading.compliance, "Compliance mappings loading");
        assert!(!loading.mapped_not_enforced);

        let failed = policy_editor_state(true, MappingLoadState::Failed, 0, 0, 0);
        assert_eq!(failed.compliance, "Compliance mappings unavailable");
        assert!(
            !failed.mapped_not_enforced,
            "an unknown mapping set must not raise the mapped-not-enforced warning"
        );
    }

    #[test]
    fn only_manual_mappings_are_editable() {
        assert!(mapping_row_is_editable(true, "manual"));
        for provenance in ["imported", "inherited", "inferred", "suggested", "other"] {
            assert!(
                !mapping_row_is_editable(true, provenance),
                "{provenance} mappings are authoritative and must stay read-only"
            );
        }
        assert!(
            !mapping_row_is_editable(false, "manual"),
            "an immutable version cannot expose mapping mutations"
        );
    }

    #[test]
    fn mapping_provenance_labels_are_accurate() {
        assert_eq!(mapping_provenance_label("manual"), "Manual mapping");
        assert_eq!(
            mapping_provenance_label("imported"),
            "Imported from benchmark"
        );
        assert_eq!(
            mapping_provenance_label("inherited"),
            "Inherited from source version"
        );
        assert_eq!(mapping_provenance_label("inferred"), "Inferred at import");
        assert_eq!(
            mapping_provenance_label("suggested"),
            "Suggested · not authoritative"
        );
        assert_eq!(mapping_provenance_label("novel"), "novel · read-only");
    }

    #[test]
    fn addable_rules_are_exactly_the_persistable_kinds() {
        // The Add Rule control must only offer kinds Phase 2 can persist.
        let addable: Vec<&str> = RULE_OPTIONS
            .iter()
            .filter(|(_, _, persisted)| *persisted)
            .map(|(id, _, _)| *id)
            .collect();
        assert_eq!(
            addable,
            vec![
                "packages_installed",
                "nixos_option",
                "custom_eval",
                "cve_block"
            ]
        );
        // Every addable kind must be reported persistable by the capability fn,
        // so the two cannot drift.
        for kind in &addable {
            assert!(
                rule_kind_is_persisted(kind),
                "{kind} is offered by Add Rule but is not persistable"
            );
        }
    }

    #[test]
    fn known_unsupported_rule_kinds_are_not_addable() {
        for kind in [
            "eval_passed",
            "build_succeeded",
            "time_window",
            "approval_required",
            "rollout_percent",
        ] {
            assert!(
                !rule_kind_is_persisted(kind),
                "{kind} must not be persistable, so Add Rule must not offer it"
            );
        }
    }

    #[test]
    fn actionable_recommendations_are_all_persistable_and_rollout_is_empty() {
        for category in POLICY_CATEGORIES {
            let actionable = actionable_recommended_enforcement(category);
            for kind in &actionable {
                assert!(
                    rule_kind_is_persisted(kind),
                    "{kind} is recommended for {category:?} but is not persistable"
                );
            }
        }

        // Pipeline must recommend the CVE gate but never the UI-only pipeline gates.
        let pipeline = actionable_recommended_enforcement(PolicyCategory::Pipeline);
        assert!(
            pipeline.contains(&"cve_block"),
            "Pipeline must recommend cve_block"
        );
        assert!(
            !pipeline.contains(&"eval_passed"),
            "Pipeline must not recommend eval_passed"
        );
        assert!(
            !pipeline.contains(&"build_succeeded"),
            "Pipeline must not recommend build_succeeded"
        );

        // Rollout has no currently persistable category-specific recommendation.
        assert!(
            actionable_recommended_enforcement(PolicyCategory::Rollout).is_empty(),
            "Rollout must not surface unsupported rollout gates as recommendations"
        );
    }

    #[test]
    fn category_change_preserves_every_rule_and_only_changes_guidance() {
        // A deliberately mixed rule set: a rollout gate, a security assertion,
        // and a package assertion.
        let mut time_window = PolicyRule::new("time_window");
        time_window.from = "22:00".to_string();
        let mut custom = PolicyRule::new("custom_eval");
        custom.expr = "config.networking.firewall.enable == true".to_string();
        let mut packages = PolicyRule::new("packages_installed");
        packages.packages = "auditd".to_string();
        let rules = vec![time_window, custom, packages];
        let before = rules.clone();

        let kinds: Vec<String> = rules.iter().map(|rule| rule.kind.clone()).collect();

        // Guidance changes with the category...
        assert_ne!(
            recommended_enforcement(PolicyCategory::Rollout),
            recommended_enforcement(PolicyCategory::Security)
        );
        let rollout_off = off_category_rule_kinds(PolicyCategory::Rollout, &kinds);
        let security_off = off_category_rule_kinds(PolicyCategory::Security, &kinds);
        assert_ne!(rollout_off, security_off);
        assert!(rollout_off.contains(&"custom_eval".to_string()));
        assert!(security_off.contains(&"time_window".to_string()));

        // ...while the rule data itself is untouched: count, order, kinds, values.
        assert_eq!(rules, before);
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[0].kind, "time_window");
        assert_eq!(rules[0].from, "22:00");
        assert_eq!(rules[1].kind, "custom_eval");
        assert_eq!(
            rules[1].expr,
            "config.networking.firewall.enable == true".to_string()
        );
        assert_eq!(rules[2].kind, "packages_installed");
        assert_eq!(rules[2].packages, "auditd".to_string());

        // Saving is only ever blocked by genuinely unpersistable rule kinds,
        // never by a rule being off-category.
        let representable = serde_json::json!({"mode": "all", "rules": []});
        assert!(
            save_blocker(
                true,
                PolicyFormat::Json,
                "custom_check",
                &representable,
                &rules
            )
            .is_some_and(|blocker| blocker.contains("UI-only")),
            "the only blocker here must be the UI-only time-window rule"
        );

        let on_category_only = vec![PolicyRule::new("custom_eval")];
        for category in POLICY_CATEGORIES {
            let _ = category;
            assert!(
                save_blocker(
                    true,
                    PolicyFormat::Json,
                    "custom_check",
                    &representable,
                    &on_category_only
                )
                .is_none(),
                "a persistable rule must save regardless of the selected category"
            );
        }
    }

    #[test]
    fn new_policy_editor_does_not_seed_unsavable_rules() {
        // A newly opened editor must be immediately persistable: no seeded
        // UI-only rules that the user has to discover and delete first.
        let seed: Vec<PolicyRule> = Vec::new();
        assert!(
            save_blocker(
                false,
                PolicyFormat::Json,
                "custom_check",
                &serde_json::Value::Null,
                &seed
            )
            .is_none()
        );
        let (policy_type, config) =
            build_persisted_payload(&seed).expect("an empty editor must be savable");
        assert_eq!(policy_type, "custom_check");
        assert_eq!(config, serde_json::json!({"mode": "all", "rules": []}));
    }

    #[test]
    fn empty_rules_are_a_persistable_no_enforcement_state() {
        let (policy_type, config) = build_persisted_payload(&[]).expect("empty rules save");
        assert_eq!(policy_type, "custom_check");
        assert_eq!(
            config,
            serde_json::json!({"mode": "all", "rules": []}),
            "only fields this editor can round-trip may be persisted"
        );
        assert!(
            save_blocker(
                false,
                PolicyFormat::Json,
                "custom_check",
                &serde_json::Value::Null,
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn saved_no_enforcement_policy_can_be_reopened_and_saved_again() {
        // Regression: the previous payload carried `context`/`binding`, which
        // this editor's own representability guard rejects, so a saved
        // no-enforcement policy could never be saved again after reload.
        let (policy_type, config) =
            build_persisted_payload(&[]).expect("no-enforcement policy must serialize");

        assert!(
            custom_check_config_is_representable(&config),
            "the editor must be able to represent the config it just wrote"
        );

        let reopened_rules = rules_from_policy(&policy_type, &config);
        assert!(
            reopened_rules.is_empty(),
            "reopening a no-enforcement policy must show zero rules"
        );

        assert!(
            save_blocker(
                true,
                PolicyFormat::Json,
                &policy_type,
                &config,
                &reopened_rules
            )
            .is_none(),
            "an unchanged no-enforcement policy must remain savable after reload"
        );

        assert_eq!(
            build_persisted_payload(&reopened_rules).expect("re-serialize"),
            (policy_type, config),
            "round-tripping must be stable"
        );
    }

    fn design_dod_consent_banner() -> String {
        let path = option_env!("CRYSTAL_FORGE_DESIGN_ENFORCEMENT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../docs/design/CrystalForge/data-enforcement.js")
            });
        let source = std::fs::read_to_string(path).expect("read data-enforcement.js test oracle");
        source
            .split_once("const DOD_CONSENT_BANNER = `")
            .expect("DOD_CONSENT_BANNER declaration")
            .1
            .split_once("`;\n\n// Known NixOS option types")
            .expect("DOD_CONSENT_BANNER terminator")
            .0
            .to_string()
    }

    #[test]
    fn composite_semantic_values_and_stable_ids_round_trip_exactly() {
        let banner = design_dod_consent_banner();
        let difficult =
            "  leading \"quotes\" and \\\\slashes ${config.foo}\n\nblank line\ntrailing  \n";
        let mut boolean = PolicyRule::new("nixos_option");
        boolean.path = "services.openssh.enable".into();
        boolean.option_type = "boolean".into();
        boolean.value = serde_json::Value::Bool(true);
        let mut integer = PolicyRule::new("nixos_option");
        integer.path = "services.openssh.settings.ClientAliveInterval".into();
        integer.option_type = "integer".into();
        integer.op = ">=".into();
        integer.value = serde_json::json!(-9_i64);
        let mut lines = PolicyRule::new("nixos_option");
        lines.path = "environment.etc.\"issue\".text".into();
        lines.option_type = "lines".into();
        lines.value = serde_json::Value::String(banner.clone());
        let mut custom_string = PolicyRule::new("nixos_option");
        custom_string.path = "services.example.difficult".into();
        custom_string.value = serde_json::Value::String(difficult.to_string());
        let mut rules = vec![boolean, integer, lines, custom_string];
        let original_ids = rules.iter().map(|rule| rule.id).collect::<Vec<_>>();

        let (policy_type, config) = build_persisted_payload(&rules).expect("serialize");
        assert_eq!(policy_type, "composite");
        assert_eq!(config["schema_version"], 1);
        assert_eq!(config["mode"], "all");
        assert_eq!(config["rules"][0]["config"]["value"], true);
        assert_eq!(config["rules"][1]["config"]["value"], -9);
        assert_eq!(config["rules"][2]["config"]["value"], banner.as_str());
        assert_eq!(config["rules"][3]["config"]["value"], difficult);
        assert!(!config.to_string().contains("''"));

        let reopened = rules_from_policy(&policy_type, &config);
        assert_eq!(
            reopened.iter().map(|rule| rule.id).collect::<Vec<_>>(),
            original_ids
        );
        assert_eq!(reopened[2].value.as_str(), Some(banner.as_str()));
        assert_eq!(reopened[3].value.as_str(), Some(difficult));
        assert_eq!(
            build_persisted_payload(&reopened),
            Some((policy_type, config.clone()))
        );

        rules.swap(0, 3);
        let (_, reordered) = build_persisted_payload(&rules).expect("reorder");
        assert_eq!(reordered["rules"][0]["id"], original_ids[3].to_string());
        assert_eq!(reordered["rules"][3]["id"], original_ids[0].to_string());
    }

    #[test]
    fn metadata_enrichment_preserves_persisted_target_semantics() {
        let mut rule = PolicyRule::new("nixos_option");
        rule.path = "networking.firewall.backend".into();
        rule.option_type = "unknown".into();
        rule.op = "!=".into();
        rule.value = serde_json::json!("target-specific-backend");
        let original_id = rule.id;
        let metadata = NixosOptionMetadata {
            path: "networking.firewall.backend".into(),
            value_type: crate::api::models::NixosOptionValueType::Enum,
            enum_values: vec![serde_json::json!("iptables"), serde_json::json!("nftables")],
            description: Some("Firewall implementation".into()),
        };

        rule.enrich_option_metadata(&metadata);

        assert_eq!(rule.id, original_id);
        assert_eq!(rule.path, "networking.firewall.backend");
        assert_eq!(rule.option_type, "unknown");
        assert_eq!(rule.op, "!=");
        assert_eq!(rule.value, serde_json::json!("target-specific-backend"));
        assert_eq!(rule.baseline_option_type.as_deref(), Some("enum"));
        assert!(rule.baseline_advisory().is_some());
        assert!(rule_validation_error(&rule).is_none());
    }

    #[test]
    fn baseline_enum_domain_is_advisory_for_persisted_values() {
        let mut rule = PolicyRule::new("nixos_option");
        rule.path = "networking.firewall.backend".into();
        rule.option_type = "enum".into();
        rule.value = serde_json::json!("target-specific-backend");
        rule.enrich_option_metadata(&NixosOptionMetadata {
            path: rule.path.clone(),
            value_type: crate::api::models::NixosOptionValueType::Enum,
            enum_values: vec![serde_json::json!("iptables"), serde_json::json!("nftables")],
            description: None,
        });

        assert!(rule.baseline_advisory().is_some());
        assert!(rule_validation_error(&rule).is_none());
    }

    #[test]
    fn non_idempotent_composite_shapes_are_opaque() {
        let missing_message = serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "10000000-0000-0000-0000-000000000001",
                "kind": "custom_eval",
                "config": { "expression": "config.services.openssh.enable" }
            }]
        });
        assert!(!composite_config_is_representable(&missing_message));

        let whitespace_package = serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{
                "id": "10000000-0000-0000-0000-000000000002",
                "kind": "packages_installed",
                "config": { "packages": [" openssh"] }
            }]
        });
        assert!(!composite_config_is_representable(&whitespace_package));
    }

    #[test]
    fn untouched_legacy_enforcement_is_preserved_until_explicitly_changed() {
        let legacy = serde_json::json!({
            "mode": "all",
            "rules": [{
                "expression": "config.networking.firewall.enable == true",
                "description": "Firewall",
                "field_name": "firewall",
                "strict": true
            }],
            "strict": true
        });
        let rules = rules_from_policy("custom_check", &legacy);
        assert_eq!(
            persisted_payload_for_save(true, false, "custom_check", &legacy, &rules),
            Some(("custom_check".to_string(), legacy.clone()))
        );
        assert_eq!(
            persisted_payload_for_save(true, true, "custom_check", &legacy, &rules)
                .expect("changed payload")
                .0,
            "composite"
        );
    }

    #[test]
    fn opaque_existing_policy_is_not_presented_as_zero_enforcement() {
        let opaque = serde_json::json!({"opaque": {"must_survive": true}});
        let rules = rules_from_policy("future_policy", &opaque);
        assert!(rules.is_empty());
        assert!(existing_enforcement_is_opaque(
            PolicyFormat::Json,
            "future_policy",
            &opaque
        ));
        let blocker = save_blocker(true, PolicyFormat::Json, "future_policy", &opaque, &rules)
            .expect("opaque policy must be blocked");
        assert!(blocker.contains("not supported"));
    }

    #[test]
    fn toml_edit_is_read_only_in_design_form() {
        let rules = vec![PolicyRule::new("custom_eval")];

        assert!(
            save_blocker(
                true,
                PolicyFormat::Toml,
                "custom_check",
                &serde_json::Value::Null,
                &rules
            )
            .is_some()
        );
    }

    #[test]
    fn custom_check_with_unrepresented_semantics_is_blocked() {
        let config = serde_json::json!({
            "rules": [
                { "expression": "config.foo", "description": "foo", "strict": false }
            ],
            "mode": "any",
            "strict": true
        });
        let rules = rules_from_policy("custom_check", &config);

        assert!(save_blocker(true, PolicyFormat::Json, "custom_check", &config, &rules).is_some());
    }

    #[test]
    fn custom_check_extra_top_level_field_is_blocked() {
        let config = serde_json::json!({
            "rules": [
                { "expression": "config.foo", "description": "foo", "strict": true }
            ],
            "mode": "all",
            "strict": true,
            "metadata": { "control": "AC-3" }
        });
        let rules = rules_from_policy("custom_check", &config);

        assert!(save_blocker(true, PolicyFormat::Json, "custom_check", &config, &rules).is_some());
    }

    #[test]
    fn custom_check_extra_rule_field_is_blocked() {
        let config = serde_json::json!({
            "rules": [
                {
                    "expression": "config.foo",
                    "description": "foo",
                    "strict": true,
                    "remediation": "enable foo"
                }
            ],
            "mode": "all",
            "strict": true
        });
        let rules = rules_from_policy("custom_check", &config);

        assert!(save_blocker(true, PolicyFormat::Json, "custom_check", &config, &rules).is_some());
    }

    #[test]
    fn require_packages_strict_false_is_blocked() {
        let config = serde_json::json!({
            "packages": ["openssh", "auditd"],
            "strict": false
        });
        let rules = rules_from_policy("require_packages", &config);

        assert!(
            save_blocker(
                true,
                PolicyFormat::Json,
                "require_packages",
                &config,
                &rules
            )
            .is_some()
        );
    }

    #[test]
    fn custom_frameworks_excludes_standard_and_empty_values() {
        let policy = |framework: Option<&str>| PolicyDefinition {
            id: Uuid::new_v4(),
            lineage_id: Uuid::new_v4(),
            version_id: None,
            revision: None,
            publication_state: None,
            semantic_digest: None,
            revisions: Vec::new(),
            name: "test".to_string(),
            description: String::new(),
            format: PolicyFormat::Json,
            body: "{}".to_string(),
            policy_type: None,
            updated_at: String::new(),
            system_count: 0,
            srg_ids: Vec::new(),
            cci_ids: Vec::new(),
            category: Some("security".to_string()),
            framework: framework.map(str::to_string),
            severity: None,
            control_family: None,
            cmmc_level: None,
            cis_section: None,
            rationale: None,
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
            evidence_specs: None,
            provenance: Vec::new(),
        };

        let frameworks = custom_frameworks(&[
            policy(Some("DISA STIG")),
            policy(Some("  Internal Baseline  ")),
            policy(Some("internal baseline")),
            policy(Some("")),
            policy(None),
        ]);

        assert_eq!(frameworks, vec!["Internal Baseline"]);
    }

    #[test]
    fn evidence_required_fields_round_trip_preserves_metadata() {
        use std::collections::HashMap;

        // Create an EvidenceSpec with non-empty required_fields metadata
        let mut required_fields = HashMap::new();
        required_fields.insert("field1".to_string(), "value1".to_string());
        required_fields.insert("field2".to_string(), "value2".to_string());

        let original_spec = EvidenceSpec {
            kind: EvidenceKind::Command {
                cmd: "systemctl status ssh".to_string(),
                expect: "active".to_string(),
            },
            required_fields: required_fields.clone(),
        };

        // Round-trip: EvidenceSpec -> PolicyEvidence -> EvidenceSpec
        let evidence = PolicyEvidence::from_evidence_spec(&original_spec);
        assert_eq!(
            evidence.required_fields, required_fields,
            "from_evidence_spec should preserve required_fields"
        );

        let round_tripped_spec = evidence.to_evidence_spec();

        // Assert the required_fields map survived intact
        assert_eq!(
            round_tripped_spec.required_fields, original_spec.required_fields,
            "required_fields must be exactly equal after round-trip"
        );

        // Assert the metadata is not empty (regression guard)
        assert!(
            !round_tripped_spec.required_fields.is_empty(),
            "required_fields must not be destroyed to empty HashMap"
        );
    }

    #[test]
    fn evidence_required_fields_empty_round_trip_preserves_empty() {
        // Verify that empty required_fields also round-trips correctly
        let original_spec = EvidenceSpec {
            kind: EvidenceKind::File {
                path: "/etc/ssh/sshd_config".to_string(),
                note: Some("SSH config".to_string()),
            },
            required_fields: std::collections::HashMap::new(),
        };

        let evidence = PolicyEvidence::from_evidence_spec(&original_spec);
        let round_tripped_spec = evidence.to_evidence_spec();

        assert!(
            round_tripped_spec.required_fields.is_empty(),
            "empty required_fields must remain empty after round-trip"
        );
    }
}
