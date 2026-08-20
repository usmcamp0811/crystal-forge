//! Policy editor modal for creating and editing policy definitions.
//!
//! This modal mirrors the design example `PolicyFormModal`: a single unified
//! create/edit modal (no Basic/Advanced toggle and no raw JSON/TOML editor) with
//! metadata, category, severity, rationale, an assertions/gate-rules builder, an
//! evidence-for-ATO builder, and an edit-mode danger zone with typed-confirmation
//! delete.
//!
//! The deployment-policy API persists classification with the exact policy
//! version, while Evidence remains unavailable in the current API.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{
    create_deployment_policy, create_policy_mapping, delete_deployment_policy,
    delete_policy_mapping, fetch_compliance_framework_versions, fetch_compliance_frameworks,
    fetch_policy_requirement_mappings, search_requirements, update_deployment_policy,
    update_policy_mapping,
};
use crate::api::models::{
    ComplianceFrameworkSummary, ComplianceFrameworkVersionSummary, CreateDeploymentPolicyRequest,
    CreatePolicyMappingRequest, EvidenceKind, EvidenceSpec, PolicyMappingRow, RequirementVersionSummary,
    UpdateDeploymentPolicyRequest, UpdatePolicyMappingRequest,
};
use crate::views::policies_api;

use super::types::{PolicyCategory, PolicyDefinition, PolicyFormat, is_policy_version_editable};

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
    value: String,
    // custom_eval
    expr: String,
    message: String,
}

impl PolicyRule {
    fn new(kind: &str) -> Self {
        let mut rule = Self {
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
            value: "\"no\"".to_string(),
            expr: "config.services.openssh.enable == true".to_string(),
            message: "SSH must be enabled".to_string(),
        };
        if kind == "packages_installed" {
            rule.packages = "openssh, auditd".to_string();
        }
        rule
    }

    /// Whether this rule kind can be persisted via the existing policy API config.
    fn is_persisted(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "cve_block" | "packages_installed" | "nixos_option" | "custom_eval"
        )
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
        }
    }

    /// Convert PolicyEvidence to EvidenceSpec for the API.
    fn to_evidence_spec(&self) -> Option<EvidenceSpec> {
        let kind = match self.kind.as_str() {
            "command" if !self.cmd.is_empty() && !self.expect.is_empty() => {
                EvidenceKind::Command {
                    cmd: self.cmd.clone(),
                    expect: self.expect.clone(),
                }
            }
            "log" if !self.unit.is_empty() && !self.r#match.is_empty() => {
                EvidenceKind::Log {
                    source: self.source.clone(),
                    unit: self.unit.clone(),
                    match_text: self.r#match.clone(),
                }
            }
            "file" if !self.path.is_empty() => {
                EvidenceKind::File {
                    path: self.path.clone(),
                    note: if self.note.is_empty() { None } else { Some(self.note.clone()) },
                }
            }
            "unit_state" if !self.unit.is_empty() && !self.state.is_empty() => {
                EvidenceKind::UnitState {
                    unit: self.unit.clone(),
                    state: self.state.clone(),
                }
            }
            "eval_attr" if !self.attr.is_empty() => {
                EvidenceKind::EvalAttr {
                    attr: self.attr.clone(),
                }
            }
            "attestation" if !self.note.is_empty() => {
                EvidenceKind::Attestation {
                    note: self.note.clone(),
                }
            }
            _ => return None,
        };
        Some(EvidenceSpec {
            kind,
            required_fields: std::collections::HashMap::new(),
        })
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

/// Build the persisted `(policy_type, config)` from the persistable rules only.
///
/// Persistable rules are mapped into a `custom_check` with a `rules[]` array (or a
/// single CVE gate / packages policy when that is the only rule), matching the
/// shapes the real API already round-trips.
fn build_persisted_payload(rules: &[PolicyRule]) -> Option<(String, serde_json::Value)> {
    if has_cve_combination(rules) {
        return None;
    }

    let persistable: Vec<&PolicyRule> = rules.iter().filter(|r| r.is_persisted()).collect();
    if persistable.is_empty() {
        return None;
    }

    // Single CVE gate → require_cve_check
    if persistable.len() == 1 && persistable[0].kind == "cve_block" {
        let rule = persistable[0];
        let max = rule.max_allowed.trim().parse::<u32>().unwrap_or(0);
        let config = if rule.severity == "critical" {
            serde_json::json!({ "max_critical": max, "max_high": null, "strict": true, "when_no_scan": "block" })
        } else {
            serde_json::json!({ "max_critical": 0, "max_high": max, "strict": true, "when_no_scan": "block" })
        };
        return Some(("require_cve_check".to_string(), config));
    }

    // Single packages rule → require_packages
    if persistable.len() == 1 && persistable[0].kind == "packages_installed" {
        let packages = split_packages(&persistable[0].packages);
        return Some((
            "require_packages".to_string(),
            serde_json::json!({ "packages": packages, "strict": true }),
        ));
    }

    // Otherwise → custom_check with rules[]
    let mut json_rules = Vec::new();
    for (index, rule) in persistable.into_iter().enumerate() {
        if let Some(value) = rule_to_custom_check_entry(rule, index) {
            json_rules.push(value);
        }
    }
    if json_rules.is_empty() {
        return None;
    }
    Some((
        "custom_check".to_string(),
        serde_json::json!({ "rules": json_rules, "mode": "all", "strict": true }),
    ))
}

fn rule_to_custom_check_entry(rule: &PolicyRule, index: usize) -> Option<serde_json::Value> {
    let field_name = format!("policy_rule_{}", index + 1);

    match rule.kind.as_str() {
        "custom_eval" => Some(serde_json::json!({
            "expression": rule.expr.trim(),
            "description": if rule.message.trim().is_empty() { "Custom rule failed" } else { rule.message.trim() },
            "field_name": field_name,
            "strict": true,
        })),
        "nixos_option" => {
            let expression = format!(
                "config.{} {} {}",
                rule.path.trim(),
                rule.op.trim(),
                rule.value.trim()
            );
            Some(serde_json::json!({
                "expression": expression,
                "description": format!("config.{} must be {} {}", rule.path.trim(), rule.op.trim(), rule.value.trim()),
                "field_name": field_name,
                "strict": true,
            }))
        }
        "packages_installed" => {
            let packages = split_packages(&rule.packages);
            let checks = packages
                .iter()
                .map(|p| format!("builtins.any (x: (x.pname or \"\") == \"{p}\") config.environment.systemPackages"))
                .collect::<Vec<_>>()
                .join(" && ");
            Some(serde_json::json!({
                "expression": if checks.is_empty() { "true".to_string() } else { checks },
                "description": format!("Packages installed: {}", packages.join(", ")),
                "field_name": field_name,
                "strict": true,
            }))
        }
        "cve_block" => None,
        _ => None,
    }
}

fn unsupported_rule_labels(rules: &[PolicyRule]) -> Vec<&'static str> {
    rules
        .iter()
        .filter(|rule| !rule.is_persisted())
        .map(|rule| rule_label(&rule.kind))
        .collect()
}

fn has_cve_combination(rules: &[PolicyRule]) -> bool {
    let persistable = rules
        .iter()
        .filter(|rule| rule.is_persisted())
        .collect::<Vec<_>>();
    persistable.len() > 1 && persistable.iter().any(|rule| rule.kind == "cve_block")
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

    if has_cve_combination(rules) {
        return Some(
            "CVE gates cannot be combined with other rules until backend multi-rule CVE support exists."
                .to_string(),
        );
    }

    if build_persisted_payload(rules).is_none() {
        return Some("Add at least one backend-supported assertion before saving.".to_string());
    }

    None
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
            if let Some(id) = editing_policy_version_id {
                match fetch_policy_requirement_mappings(&id).await {
                    Ok(value) => mappings.set(value),
                    Err(e) => error.set(Some(format!("Failed to load mappings: {e}"))),
                }
            }
        });
    }

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
            if grouped.is_empty() && pending_grouped.is_empty() {
                div { class: "sd-callout sd-callout-info", style: "margin-bottom:10px;",
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
                                     let row_read_only = !mappings_editable || row.provenance == "imported" || row.trust_state == "suggested";
                                     let edit_row = row.clone();
                                     rsx! { div { key: "{row.id}", "data-testid": "policy-mapping-row", style: "display:grid;grid-template-columns:1fr auto;gap:8px;align-items:start;padding:9px 11px;background:var(--cf-subtle-bg);border:1px solid var(--cf-divider);border-radius:8px;font-size:12px;margin-bottom:6px;",
                                    div { style: "display:flex;flex-direction:column;gap:2px;",
                                        div { style: "font-size:11px;font-weight:700;color:var(--cf-text-muted);text-transform:uppercase;letter-spacing:0.04em;", "{row.framework_name} {row.framework_version}" }
                                        div { class: "mono", style: "font-size:12.5px;font-weight:600;margin-top:2px;", span { "{row.requirement_external_id}" } if let Some(title) = &row.requirement_title { span { style: "font-family:inherit;font-weight:400;color:var(--cf-text-secondary);", " · {title}" } } }
                                        div { style: "display:flex;gap:6px;margin-top:2px;",
                                            span { class: "chip chip-neutral", style: "font-size:10px;", {match row.relationship.as_str() { "implements" => "Implements", "supports" => "Supports", _ => "Evidence for" }} }
                                            span { class: if row.coverage == "full" { "chip chip-success" } else { "chip chip-warn" }, style: "font-size:10px;", {if row.coverage == "full" { "Full" } else { "Partial" }} }
                                            span { style: "font-size:10px;color:var(--cf-text-muted);", {if row.provenance == "imported" { "Imported from benchmark" } else { "Manual mapping" }} }
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
    let seed_rules = if is_editing {
        rules_from_policy(&existing_type, &existing_config)
    } else {
        vec![
            PolicyRule::new("eval_passed"),
            PolicyRule::new("build_succeeded"),
        ]
    };
    let existing_policy = editing_policy_id.read().and_then(|id| {
        policy_library
            .read()
            .iter()
            .find(|policy| policy.id == id)
            .cloned()
    });
    let seed_category = existing_policy
        .as_ref()
        .and_then(|policy| policy.category.as_deref())
        .unwrap_or(match existing_type.as_str() {
            "require_cve_check" => "pipeline",
            "require_packages" | "custom_check" => "security",
            _ => "deployment",
        });

    let mut domain = use_signal(|| {
        if seed_category.eq_ignore_ascii_case("security") {
            "security".to_string()
        } else {
            "platform".to_string()
        }
    });
    let mut platform_category = use_signal(|| match seed_category {
        "pipeline" => "pipeline".to_string(),
        "rollout" => "rollout".to_string(),
        _ => "deployment".to_string(),
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
    let mut rules = use_signal(|| seed_rules);
     let mut evidence: Signal<Vec<PolicyEvidence>> = use_signal(Vec::new);
     let initial_evidence_count = existing_policy
         .as_ref()
         .and_then(|p| p.evidence_specs.as_ref())
         .map(|specs| specs.len())
         .unwrap_or(0);
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
    let can_save = !name_missing && current_save_blocker.is_none() && !*is_saving.read();
    let rule_count = rules.read().len();
    let evidence_count = evidence.read().len();
    let delete_matches = delete_typed.read().as_str() == name_value;

    rsx! {
        div {
            class: "modal-backdrop cf-modal-overlay-z50",
            onclick: move |_| on_close.call(()),
            div {
                class: "modal cf-policy-modal-panel",
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
                        div { class: "cf-modal-tabs", role: "tablist", aria_label: "Policy editor sections",
                            PolicyEditorTabButton { tab: PolicyEditorTab::Details, active: *active_tab.read(), label: "Details", test_id: "policy-editor-tab-details", on_select: move |_| active_tab.set(PolicyEditorTab::Details) }
                             PolicyEditorTabButton { tab: PolicyEditorTab::Mappings, active: *active_tab.read(), label: format!("Mappings · {}", mappings.read().len() + pending_mappings.read().len()), test_id: "policy-editor-tab-mappings", on_select: move |_| active_tab.set(PolicyEditorTab::Mappings) }
                            PolicyEditorTabButton { tab: PolicyEditorTab::Enforcement, active: *active_tab.read(), label: format!("Enforcement · {rule_count}"), test_id: "policy-editor-tab-enforcement", on_select: move |_| active_tab.set(PolicyEditorTab::Enforcement) }
                            PolicyEditorTabButton { tab: PolicyEditorTab::Evidence, active: *active_tab.read(), label: format!("Evidence · {evidence_count}"), test_id: "policy-editor-tab-evidence", on_select: move |_| active_tab.set(PolicyEditorTab::Evidence) }
                        }
                        div { class: "cf-modal-tab-panel",
                        if *active_tab.read() == PolicyEditorTab::Details {
                        div { style: "display:grid;grid-template-columns:1fr;gap:14px;",
                            div { class: "field",
                                label { "Name" }
                                input {
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
                            label { "Description" }
                            input {
                                class: "input focus-ring",
                                placeholder: "One-line summary shown in the registry",
                                value: "{edit_description}",
                                oninput: move |event| edit_description.set(event.value()),
                            }
                        }

                        div { class: "field",
                            label { "Domain" }
                            div { class: "seg", role: "radiogroup", style: "width:fit-content;",
                                for (value, label) in [("platform", "Platform"), ("security", "Security controls")] {
                                    button {
                                        key: "domain-{value}", r#type: "button", role: "radio",
                                        aria_checked: if domain.read().as_str() == value { "true" } else { "false" },
                                        class: if domain.read().as_str() == value { "active" } else { "" },
                                        onclick: move |_| domain.set(value.to_string()),
                                        "{label}"
                                    }
                                }
                            }
                        }

                        if domain.read().as_str() == "platform" {
                        div { class: "field",
                            label { "Category" }
                            div { role: "radiogroup", style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:8px;",
                                for policy_category in [PolicyCategory::Deployment, PolicyCategory::Pipeline, PolicyCategory::Rollout] {
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
                                        aria_checked: if platform_category.read().as_str() == id { "true" } else { "false" },
                                        class: if platform_category.read().as_str() == id { "cf-policy-category-card cf-policy-category-card-active focus-ring" } else { "cf-policy-category-card focus-ring" },
                                        style: "--cf-policy-category-color:{color};",
                                        onclick: move |_| platform_category.set(id.to_string()),
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
                                            span { style: if platform_category.read().as_str() == id { "display:block;font-size:12px;font-weight:600;color:{color};" } else { "display:block;font-size:12px;font-weight:600;color:var(--cf-text-primary);" }, "{label}" }
                                            span { style: "display:block;font-size:10.5px;color:var(--cf-text-muted);line-height:1.35;margin-top:2px;", "{blurb}" }
                                        }
                                    }
                                        }
                                    }
                                }
                            }
                        }
                        }

                        if domain.read().as_str() == "security" {
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
                              }
                          }
                         // Assertions & gate rules builder
                        if *active_tab.read() == PolicyEditorTab::Enforcement {
                        div { style: "margin-top:6px;",
                            div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px;",
                                label { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "Assertions & gate rules ({rule_count})" }
                                span { style: "font-size:11px;color:var(--cf-text-muted);", "All must hold — each compiles to a policy check." }
                            }
                            div { style: "display:flex;flex-direction:column;gap:6px;",
                                for (index, rule) in rules.read().iter().cloned().enumerate() {
                                    div {
                                        key: "rule-{index}",
                                        style: "display:grid;grid-template-columns:1fr auto;gap:8px;align-items:center;padding:8px 10px;background:var(--cf-subtle-bg);border-radius:8px;",
                                        RuleEditorRow { index, rule: rule.clone(), rules }
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Remove rule",
                                            onclick: move |_| {
                                                let mut next = rules.read().clone();
                                                if index < next.len() { next.remove(index); }
                                                rules.set(next);
                                            },
                                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                path { d: "M18 6 6 18M6 6l12 12" }
                                            }
                                        }
                                    }
                                }
                            }
                            div { style: "margin-top:8px;display:flex;gap:8px;flex-wrap:wrap;",
                                select {
                                    class: "input focus-ring",
                                    style: "max-width:260px;font-size:12px;",
                                    value: "{add_rule_kind}",
                                    onchange: move |event| {
                                        let kind = event.value();
                                        if !kind.is_empty() {
                                            let mut next = rules.read().clone();
                                            next.push(PolicyRule::new(&kind));
                                            rules.set(next);
                                        }
                                        add_rule_kind.set(String::new());
                                    },
                                    option { value: "", disabled: true, "+ Add assertion / rule…" }
                                    optgroup { label: "NixOS config assertions",
                                        option { value: "packages_installed", "Packages installed" }
                                        option { value: "nixos_option", "NixOS option equals" }
                                        option { value: "custom_eval", "Custom nix expression" }
                                    }
                                    optgroup { label: "Pipeline gates",
                                        option { value: "eval_passed", "Eval must pass" }
                                        option { value: "build_succeeded", "Build must succeed" }
                                        option { value: "cve_block", "CVE gate" }
                                    }
                                    optgroup { label: "Rollout gates",
                                        option { value: "time_window", "Time window" }
                                        option { value: "approval_required", "Approval required" }
                                        option { value: "rollout_percent", "Canary rollout" }
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

                                let Some((policy_type, config)) = build_persisted_payload(&current_rules) else {
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
                                let is_security = domain.read().as_str() == "security";
                                let selected_framework = if framework.read().as_str() == "__custom__" {
                                    non_empty(custom_framework.read().clone())
                                } else {
                                    non_empty(framework.read().clone())
                                };
                                let selected_cmmc_level = cmmc_level.read().trim().parse::<i32>().ok();
                                let selected_category = if is_security {
                                    Some("security".to_string())
                                } else {
                                    Some(platform_category.read().clone())
                                };
                                let selected_severity = if is_security { non_empty(severity.read().clone()) } else { None };
                                let selected_control_family = if is_security && selected_framework.as_deref() == Some("NIST 800-53") { non_empty(control_family.read().clone()) } else { None };
                                let selected_cis_section = if is_security && selected_framework.as_deref() == Some("CIS Benchmark") { non_empty(cis_section.read().clone()) } else { None };
                                let selected_rationale = non_empty(rationale.read().clone());

                                save_error.set(String::new());
                                is_saving.set(true);

                                spawn(async move {
                                    let result = if let Some(policy_id) = editing_id {
                                         // Determine evidence_specs dirty state:
                                         // - None if evidence count hasn't changed (preserve)
                                         // - Some([]) if evidence was cleared
                                         // - Some(items) if evidence was added/modified
                                         let evidence_specs = {
                                             let current_evidence = evidence.read();
                                             let current_count = current_evidence.len();
                                             if current_count == initial_evidence_count && current_count == 0 {
                                                 // Both zero and unchanged: preserve
                                                 None
                                             } else if current_count != initial_evidence_count || current_count > 0 {
                                                 // Either count changed or there's evidence: send it
                                                 let specs: Vec<EvidenceSpec> = current_evidence
                                                     .iter()
                                                     .filter_map(|ev| ev.to_evidence_spec())
                                                     .collect();
                                                 Some(specs)
                                             } else {
                                                 None
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
                                             .filter_map(|ev| ev.to_evidence_spec())
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
    };
    rsx! { button { class: if selected { format!("cf-modal-tab cf-modal-tab--active cf-modal-tab--{class}") } else { format!("cf-modal-tab cf-modal-tab--{class}") }, role: "tab", aria_selected: if selected { "true" } else { "false" }, "data-testid": "{test_id}", onclick: move |_| on_select.call(()), "{label}" } }
}

#[component]
fn RuleEditorRow(index: usize, rule: PolicyRule, rules: Signal<Vec<PolicyRule>>) -> Element {
    let kind = rule.kind.clone();
    let persisted = rule.is_persisted();

    // Mutate one field of the rule at `index` and write it back to the signal.
    macro_rules! set_rule_field {
        ($field:ident, $value:expr) => {{
            let mut next = rules.read().clone();
            if let Some(target) = next.get_mut(index) {
                target.$field = $value;
            }
            rules.set(next);
        }};
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
                    div { style: "display:flex;gap:6px;align-items:center;flex-wrap:wrap;",
                        input {
                            class: "input focus-ring mono",
                            style: "font-size:11px;padding:5px 8px;flex:1;min-width:200px;",
                            placeholder: "services.openssh.settings.PermitRootLogin",
                            value: "{rule.path}",
                            oninput: move |event| set_rule_field!(path, event.value()),
                        }
                        select {
                            class: "input focus-ring mono",
                            style: "width:auto;font-size:12px;padding:5px 6px;",
                            value: "{rule.op}",
                            onchange: move |event| set_rule_field!(op, event.value()),
                            option { value: "==", "==" }
                            option { value: "!=", "!=" }
                            option { value: ">=", "≥" }
                            option { value: "<=", "≤" }
                        }
                        input {
                            class: "input focus-ring mono",
                            style: "width:90px;font-size:11px;padding:5px 8px;",
                            value: "{rule.value}",
                            oninput: move |event| set_rule_field!(value, event.value()),
                        }
                    }
                },
                "custom_eval" => rsx! {
                    textarea {
                        class: "input focus-ring mono code-editor",
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
                    input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "sshd -T | grep permitrootlogin", value: "{evidence.cmd}", oninput: move |event| set_ev_field!(cmd, event.value()) }
                    div { style: "display:flex;align-items:center;gap:6px;",
                        span { style: "font-size:11px;color:var(--cf-text-muted);", "expect output contains" }
                        input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;", placeholder: "permitrootlogin no", value: "{evidence.expect}", oninput: move |event| set_ev_field!(expect, event.value()) }
                    }
                },
                "log" => rsx! {
                    div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                        select {
                            class: "input focus-ring",
                            style: "font-size:11px;padding:5px 8px;width:auto;",
                            value: "{evidence.source}",
                            onchange: move |event| set_ev_field!(source, event.value()),
                            option { value: "journald", "journald" }
                            option { value: "auditd", "auditd" }
                            option { value: "file", "file" }
                        }
                        input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;min-width:140px;", placeholder: "auditd.service", value: "{evidence.unit}", oninput: move |event| set_ev_field!(unit, event.value()) }
                    }
                    input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "regex / substring to match", value: "{match_value}", oninput: move |event| set_ev_field!(r#match, event.value()) }
                },
                "file" => rsx! {
                    input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "/etc/issue", value: "{evidence.path}", oninput: move |event| set_ev_field!(path, event.value()) }
                    input { class: "input focus-ring", style: "font-size:11px;padding:5px 8px;", placeholder: "What to look for / why it proves compliance", value: "{evidence.note}", oninput: move |event| set_ev_field!(note, event.value()) }
                },
                "unit_state" => rsx! {
                    div { style: "display:flex;gap:6px;align-items:center;flex-wrap:wrap;",
                        input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;min-width:140px;", placeholder: "auditd.service", value: "{evidence.unit}", oninput: move |event| set_ev_field!(unit, event.value()) }
                        span { style: "font-size:11px;color:var(--cf-text-muted);", "is" }
                        select {
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
                    input { class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "config.services.openssh.settings.PermitRootLogin", value: "{evidence.attr}", oninput: move |event| set_ev_field!(attr, event.value()) }
                    span { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);", "Captured from the evaluated config — no host access needed." }
                },
                "attestation" => rsx! {
                    input { class: "input focus-ring", style: "font-size:11px;padding:5px 8px;", placeholder: "What the agent attests to (signed snapshot)", value: "{evidence.note}", oninput: move |event| set_ev_field!(note, event.value()) }
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
    fn cve_rule_is_not_serialized_as_always_true_custom_check() {
        let rules = vec![PolicyRule::new("cve_block"), PolicyRule::new("custom_eval")];

        assert!(has_cve_combination(&rules));
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
}
