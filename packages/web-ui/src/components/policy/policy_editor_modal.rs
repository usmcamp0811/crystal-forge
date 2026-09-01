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
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;

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
/// Supported rule kinds serialize into the policy API `config` payload.
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
    tz: String,
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
            tz: "America/New_York".to_string(),
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
        } else if kind == "packages_absent" {
            rule.packages = "telnet".to_string();
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

/// A single evidence-for-ATO source persisted in policy version metadata.
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

    /// Returns the first invalid field and its error message.
    fn validation_error(&self) -> Option<(&'static str, String)> {
        match self.kind.as_str() {
            "command" => {
                if self.cmd.is_empty() {
                    return Some(("cmd", "Command is required".to_string()));
                }
                if self.expect.is_empty() {
                    return Some(("expect", "Expected output is required".to_string()));
                }
            }
            "log" => {
                if self.unit.is_empty() {
                    return Some(("unit", "Unit/source is required".to_string()));
                }
                if self.r#match.is_empty() {
                    return Some(("match", "Match pattern is required".to_string()));
                }
            }
            "file" => {
                if self.path.is_empty() {
                    return Some(("path", "File path is required".to_string()));
                }
            }
            "unit_state" => {
                if self.unit.is_empty() {
                    return Some(("unit", "Unit is required".to_string()));
                }
                if self.state.is_empty() {
                    return Some(("state", "State is required".to_string()));
                }
            }
            "eval_attr" => {
                if self.attr.is_empty() {
                    return Some(("attr", "Attribute path is required".to_string()));
                }
            }
            "attestation" => {
                if self.note.is_empty() {
                    return Some(("note", "Attestation text is required".to_string()));
                }
            }
            _ => {
                return Some(("kind", format!("Unknown evidence kind: {}", self.kind)));
            }
        }
        None
    }

    /// Convert PolicyEvidence to EvidenceSpec for the API.
    /// The caller must validate the evidence before conversion.
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

const RULE_OPTIONS: [(&str, &str, bool); 8] = [
    ("nixos_option", "NixOS option equals", true),
    ("packages_installed", "Packages installed", true),
    ("packages_absent", "Packages absent", true),
    ("custom_eval", "Custom nix expression", true),
    ("cve_block", "CVE gate", true),
    ("eval_passed", "Eval must pass", true),
    ("pin_required", "Pinned commit required", true),
    ("time_window", "Time window", true),
];

const MAX_CUSTOM_EVAL_EXPRESSION_BYTES: usize = 16 * 1024;

/// Whether a rule kind is complete and persistable in the Phase 4 editor.
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

/// Category recommendations filtered to the complete, persistable kinds.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyEditorTab {
    Details,
    Mappings,
    Enforcement,
    Evidence,
    /// Read-only imported origin. Only rendered when the policy has
    /// authoritative provenance recorded at import time.
    Provenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyEditorTabMove {
    Previous,
    Next,
    First,
    Last,
}

const POLICY_EDITOR_TABS: [PolicyEditorTab; 5] = [
    PolicyEditorTab::Details,
    PolicyEditorTab::Enforcement,
    PolicyEditorTab::Mappings,
    PolicyEditorTab::Evidence,
    PolicyEditorTab::Provenance,
];

fn visible_policy_editor_tabs(include_provenance: bool) -> &'static [PolicyEditorTab] {
    if include_provenance {
        &POLICY_EDITOR_TABS
    } else {
        &POLICY_EDITOR_TABS[..POLICY_EDITOR_TABS.len() - 1]
    }
}

fn policy_editor_tab_id(tab: PolicyEditorTab) -> &'static str {
    match tab {
        PolicyEditorTab::Details => "policy-editor-tab-details",
        PolicyEditorTab::Mappings => "policy-editor-tab-mappings",
        PolicyEditorTab::Enforcement => "policy-editor-tab-enforcement",
        PolicyEditorTab::Evidence => "policy-editor-tab-evidence",
        PolicyEditorTab::Provenance => "policy-editor-tab-provenance",
    }
}

fn move_policy_editor_tab(
    active: PolicyEditorTab,
    movement: PolicyEditorTabMove,
    include_provenance: bool,
) -> PolicyEditorTab {
    let tabs = visible_policy_editor_tabs(include_provenance);
    let position = tabs.iter().position(|tab| *tab == active).unwrap_or(0);
    match movement {
        PolicyEditorTabMove::Previous => tabs[(position + tabs.len() - 1) % tabs.len()],
        PolicyEditorTabMove::Next => tabs[(position + 1) % tabs.len()],
        PolicyEditorTabMove::First => tabs[0],
        PolicyEditorTabMove::Last => tabs[tabs.len() - 1],
    }
}

fn focus_policy_editor_element(id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
}

fn reveal_policy_editor_tab(tab: PolicyEditorTab) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(tab) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(policy_editor_tab_id(tab)))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        else {
            return;
        };
        let Some(tablist) = tab
            .parent_element()
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        else {
            return;
        };
        let visible_left = tablist.scroll_left();
        let affordance_width = tablist
            .last_element_child()
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
            .map_or(0, |element| element.offset_width());
        let visible_right = visible_left + tablist.client_width() - affordance_width;
        let tab_left = tab.offset_left();
        let tab_right = tab_left + tab.offset_width();
        if tab_left < visible_left {
            tablist.set_scroll_left(tab_left);
        } else if tab_right > visible_right {
            tablist.set_scroll_left(tab_right - tablist.client_width() + affordance_width);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = tab;
}

fn reload_policy_page() {
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

fn evidence_field_id(index: usize, kind: &str, field: &str) -> String {
    format!("policy-evidence-{kind}-{field}-{index}")
}

fn evidence_error_id(index: usize) -> String {
    format!("policy-evidence-error-{index}")
}

#[cfg(target_arch = "wasm32")]
fn policy_editor_tabs_are_horizontal() -> bool {
    web_sys::window()
        .and_then(|window| window.inner_width().ok())
        .and_then(|width| width.as_f64())
        .is_some_and(|width| width <= 700.0)
}

#[cfg(target_arch = "wasm32")]
fn restore_policy_editor_focus(target: Option<&web_sys::HtmlElement>) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(target) = target {
        let tag_name = target.tag_name();
        let connected = js_sys::Reflect::get(target.as_ref(), &"isConnected".into())
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if connected && !matches!(tag_name.as_str(), "BODY" | "HTML") && target.focus().is_ok() {
            return;
        }
    }

    // The opener can disappear when a drawer closes or a save refreshes a list.
    // Focus the owning page instead of an unrelated policy card action.
    if let Ok(Some(element)) = document.query_selector("main h1, main [role='heading']")
        && let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
    {
        if element.tab_index() < 0 {
            let _ = element.set_attribute("tabindex", "-1");
        }
        let _ = element.focus();
    }
}

// Resolve the boundary from the rendered state so dynamic actions and disabled
// controls cannot create a gap in the modal's focus loop.
fn focus_policy_editor_boundary(forward: bool) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(dialog) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("policy-editor-dialog"))
        else {
            return;
        };
        let Ok(query_selector_all) = js_sys::Reflect::get(
            dialog.as_ref(),
            &wasm_bindgen::JsValue::from_str("querySelectorAll"),
        )
        .and_then(|value| value.dyn_into::<js_sys::Function>()) else {
            return;
        };
        let Ok(nodes) = query_selector_all.call1(
            dialog.as_ref(),
            &wasm_bindgen::JsValue::from_str("button, [href], input, select, textarea, [tabindex]"),
        ) else {
            return;
        };
        let focusable = js_sys::Array::from(&nodes)
            .iter()
            .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .filter(|element| {
                !element.class_list().contains("cf-focus-sentinel")
                    && !element.has_attribute("disabled")
                    && element.tab_index() >= 0
                    && !js_sys::Reflect::get(element.as_ref(), &"offsetParent".into())
                        .is_ok_and(|parent| parent.is_null())
            })
            .collect::<Vec<_>>();
        if let Some(element) = if forward {
            focusable.first()
        } else {
            focusable.last()
        } {
            let _ = element.focus();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = forward;
}

fn rule_label(kind: &str) -> &'static str {
    RULE_OPTIONS
        .iter()
        .find(|(id, _, _)| *id == kind)
        .map(|(_, label, _)| *label)
        .unwrap_or("Rule")
}

fn rule_blurb(kind: &str) -> &'static str {
    match kind {
        "nixos_option" => "Compare a typed value from the target NixOS configuration.",
        "packages_installed" => "Require package pnames in environment.systemPackages.",
        "packages_absent" => "Reject package pnames in environment.systemPackages.",
        "custom_eval" => "Evaluate a contained boolean Nix expression against config.",
        "cve_block" => "Gate promotion on the latest scan for the exact derivation.",
        "eval_passed" => "Require the target evaluation to finish successfully.",
        "pin_required" => "Require the evaluated source to resolve to an immutable revision.",
        "time_window" => "Restrict deployment to weekdays and times in an IANA timezone.",
        _ => "",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Payload mapping (UI rules → real API config)
// ─────────────────────────────────────────────────────────────────────────────

/// Build the persisted `(policy_type, config)` from the persistable rules.
/// Empty retains the zero-enforcement legacy representation; every
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
        "packages_absent" => serde_json::json!({
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
        "eval_passed" | "pin_required" => serde_json::json!({}),
        "time_window" => serde_json::json!({
            "days": split_days(&rule.days),
            "from": rule.from,
            "to": rule.to,
            "tz": rule.tz,
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
                    "packages_absent" => &["packages"][..],
                    "custom_eval" => &["expression", "message"][..],
                    "cve_block" => &["severity", "max_allowed"][..],
                    "eval_passed" | "pin_required" => &[][..],
                    "time_window" => &["days", "from", "to", "tz"][..],
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
            if let Err(error) = validate_nixos_option_path(&rule.path) {
                return Some(error.to_string());
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
        "packages_installed" | "packages_absent" => validate_package_list(&rule.packages),
        "custom_eval" => validate_custom_eval_client(&rule.expr),
        "cve_block" if rule.max_allowed.parse::<u32>().is_err() => {
            Some("CVE maximum must be a non-negative integer.".to_string())
        }
        "time_window" => validate_time_window(rule),
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

fn mapping_header_metadata(state: MappingLoadState, count: usize) -> (&'static str, String) {
    match state {
        MappingLoadState::Loading => ("chip chip-unknown", "Mappings loading".to_string()),
        MappingLoadState::Failed => ("chip chip-critical", "Mappings unavailable".to_string()),
        MappingLoadState::Loaded if count > 0 => ("chip chip-info", format!("Mapped · {count}")),
        MappingLoadState::Loaded => ("chip chip-unknown", "Unmapped".to_string()),
    }
}

fn danger_zone_visible(is_editing: bool, active_tab: PolicyEditorTab) -> bool {
    is_editing && active_tab == PolicyEditorTab::Details
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MappingCatalogRetry {
    Frameworks,
    Versions,
    Requirements,
}

/// Returns whether mappings claim compliance while the policy asserts nothing.
fn policy_is_mapped_not_enforced(
    mapping_state: MappingLoadState,
    mapping_count: usize,
    rule_count: usize,
) -> bool {
    mapping_state == MappingLoadState::Loaded && mapping_count > 0 && rule_count == 0
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

fn validate_package_pname(pname: &str) -> Result<(), &'static str> {
    if pname.is_empty() || pname.len() > 255 {
        return Err("Package pnames must be between 1 and 255 bytes.");
    }
    if !pname
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !pname
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.' | b'_'))
    {
        return Err(
            "Package pnames must start with an ASCII letter or digit and contain only letters, digits, +, -, ., or _.",
        );
    }
    Ok(())
}

fn validate_package_list(raw: &str) -> Option<String> {
    let packages = split_packages(raw);
    if packages.is_empty() {
        return Some("Enter at least one package.".to_string());
    }
    packages
        .iter()
        .find_map(|package| validate_package_pname(package).err().map(str::to_string))
}

fn validate_custom_eval_client(expression: &str) -> Option<String> {
    let expression = expression.trim();
    if expression.is_empty() {
        return Some("Custom Nix expression is required.".to_string());
    }
    if expression.len() > MAX_CUSTOM_EVAL_EXPRESSION_BYTES {
        return Some(format!(
            "Custom Nix expression exceeds the {MAX_CUSTOM_EVAL_EXPRESSION_BYTES}-byte limit."
        ));
    }

    let mut delimiters = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut comment = false;
    for byte in expression.bytes() {
        if comment {
            if byte == b'\n' {
                comment = false;
            }
            continue;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'#' => comment = true,
            b'"' => quoted = true,
            b'(' | b'[' | b'{' => delimiters.push(byte),
            b')' | b']' | b'}' => {
                let expected = match byte {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if delimiters.pop() != Some(expected) {
                    return Some("Custom Nix expression has mismatched delimiters.".to_string());
                }
            }
            _ => {}
        }
    }
    if quoted {
        return Some("Custom Nix expression has an unterminated quoted string.".to_string());
    }
    if !delimiters.is_empty() {
        return Some("Custom Nix expression has unclosed delimiters.".to_string());
    }
    None
}

fn split_days(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|day| day.trim().to_ascii_lowercase())
        .filter(|day| !day.is_empty())
        .collect()
}

fn validate_nixos_option_path(path: &str) -> Result<(), &'static str> {
    let path = path.trim();
    if path.is_empty() {
        return Err("NixOS option path is required.");
    }
    let bytes = path.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        if bytes[offset] == b'"' {
            let start = offset;
            offset += 1;
            let mut escaped = false;
            while offset < bytes.len() {
                let byte = bytes[offset];
                offset += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
            }
            if bytes.get(offset.saturating_sub(1)) != Some(&b'"') {
                return Err("NixOS option path has an unterminated quoted segment.");
            }
            let segment = serde_json::from_str::<String>(&path[start..offset])
                .map_err(|_| "NixOS option path has an invalid quoted segment.")?;
            if segment.is_empty() {
                return Err("NixOS option path segments cannot be empty.");
            }
        } else {
            let start = offset;
            while offset < bytes.len() && bytes[offset] != b'.' {
                let byte = bytes[offset];
                if byte == b'"'
                    || byte == b'\\'
                    || byte.is_ascii_whitespace()
                    || byte.is_ascii_control()
                {
                    return Err(
                        "Bare NixOS option path segments cannot contain quotes, escapes, whitespace, or control characters.",
                    );
                }
                offset += 1;
            }
            if start == offset {
                return Err("NixOS option path segments cannot be empty.");
            }
        }
        if offset == bytes.len() {
            break;
        }
        if bytes[offset] != b'.' {
            return Err("Quoted NixOS option path segments must be separated by dots.");
        }
        offset += 1;
        if offset == bytes.len() {
            return Err("NixOS option path segments cannot be empty.");
        }
    }
    Ok(())
}

fn validate_hh_mm(value: &str) -> bool {
    let mut parts = value.split(':');
    let (Some(hour), Some(minute), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    hour.parse::<u32>().is_ok_and(|hour| hour < 24)
        && minute.parse::<u32>().is_ok_and(|minute| minute < 60)
}

fn validate_time_window(rule: &PolicyRule) -> Option<String> {
    const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
    let days = split_days(&rule.days);
    if days.is_empty() || days.iter().any(|day| !WEEKDAYS.contains(&day.as_str())) {
        return Some("Days must contain only mon, tue, wed, thu, fri, sat, or sun.".to_string());
    }
    if !validate_hh_mm(&rule.from) {
        return Some("Start time must be a valid 24-hour HH:MM value.".to_string());
    }
    if !validate_hh_mm(&rule.to) {
        return Some("End time must be a valid 24-hour HH:MM value.".to_string());
    }
    if rule.tz.parse::<chrono_tz::Tz>().is_err() {
        return Some(format!(
            "Timezone must be a valid IANA timezone: {}.",
            rule.tz
        ));
    }
    None
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
                    "packages_installed" | "packages_absent" => rule_config
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
                    "eval_passed" | "pin_required" => rule_config.is_empty(),
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
                    "time_window" => {
                        let Some(days) = rule_config.get("days").and_then(|value| value.as_array())
                        else {
                            continue;
                        };
                        let Some(from) = rule_config.get("from").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        let Some(to) = rule_config.get("to").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        let Some(tz) = rule_config.get("tz").and_then(|value| value.as_str())
                        else {
                            continue;
                        };
                        if !days.iter().all(|day| day.as_str().is_some()) {
                            continue;
                        }
                        rule.days = days
                            .iter()
                            .filter_map(|day| day.as_str())
                            .collect::<Vec<_>>()
                            .join(",");
                        rule.from = from.to_string();
                        rule.to = to.to_string();
                        rule.tz = tz.to_string();
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
    mut dialog_busy: Signal<bool>,
    on_compliance_error: EventHandler<String>,
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
    let mut show_mapping_editor = use_signal(|| false);
    let mut editing_mapping_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut error_focus_pending = use_signal(|| false);
    let mut mapping_action_focus_pending = use_signal(|| false);
    let mut frameworks_loading = use_signal(|| true);
    let mut catalog_retry: Signal<Option<MappingCatalogRetry>> = use_signal(|| None);

    use_effect(move || {
        if error_focus_pending() && !dialog_busy() {
            focus_policy_editor_element("policy-mapping-editor-error");
            error_focus_pending.set(false);
        }
        if mapping_action_focus_pending() && !dialog_busy() {
            focus_policy_editor_element("policy-mapping-add-trigger");
            mapping_action_focus_pending.set(false);
        }
    });

    if !*loaded.read() {
        loaded.set(true);
        spawn(async move {
            match fetch_compliance_frameworks().await {
                Ok(value) => {
                    frameworks.set(value);
                    frameworks_loading.set(false);
                    catalog_retry.set(None);
                }
                Err(e) => {
                    error.set(Some(format!("Failed to load frameworks: {e}")));
                    frameworks_loading.set(false);
                    catalog_retry.set(Some(MappingCatalogRetry::Frameworks));
                    on_compliance_error.call("policy-mapping-editor-error".to_string());
                }
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
        div { class: "cf-policy-mappings-tab", style: "margin-top:6px;display:flex;flex-direction:column;gap:14px;",
            div { style: "font-size:12px;color:var(--cf-text-secondary);margin-bottom:2px;line-height:1.5;",
                "Map this policy to the compliance requirements it implements, supports, or provides evidence for. Policies can map to requirements from multiple frameworks."
            }
            if let Some(text) = &*error.read() {
                div { id: "policy-mapping-editor-error", class: "sd-callout sd-callout-error", role: "alert", tabindex: "-1", style: "font-size:11px;",
                    div { "{text}" }
                    if let Some(retry) = catalog_retry() {
                        button {
                            class: "btn btn-ghost xs focus-ring",
                            "data-testid": match retry {
                                MappingCatalogRetry::Frameworks => "policy-frameworks-retry",
                                MappingCatalogRetry::Versions => "policy-framework-versions-retry",
                                MappingCatalogRetry::Requirements => "policy-requirements-retry",
                            },
                            disabled: frameworks_loading(),
                            style: "margin-top:6px;",
                            onclick: move |_| {
                                error.set(None);
                                frameworks_loading.set(true);
                                catalog_retry.set(None);
                                spawn(async move {
                                    let result = match retry {
                                        MappingCatalogRetry::Frameworks => fetch_compliance_frameworks()
                                            .await
                                            .map(|value| frameworks.set(value))
                                            .map_err(|e| format!("Failed to load frameworks: {e}")),
                                        MappingCatalogRetry::Versions => match *framework_id.read() {
                                            Some(id) => fetch_compliance_framework_versions(&id)
                                                .await
                                                .map(|value| versions.set(value))
                                                .map_err(|e| format!("Failed to load framework versions: {e}")),
                                            None => Err("Select a framework before retrying its versions.".to_string()),
                                        },
                                        MappingCatalogRetry::Requirements => match *version_id.read() {
                                            Some(id) => {
                                                let query = search.read().clone();
                                                search_requirements(&id, Some(&query), None, 25, 0)
                                                    .await
                                                    .map(|value| results.set(value))
                                                    .map_err(|e| format!("Failed to search requirements: {e}"))
                                            }
                                            None => Err("Select a framework version before retrying requirement search.".to_string()),
                                        },
                                    };
                                    frameworks_loading.set(false);
                                    if let Err(message) = result {
                                        error.set(Some(message));
                                        catalog_retry.set(Some(retry));
                                        on_compliance_error.call("policy-mapping-editor-error".to_string());
                                    }
                                });
                            },
                            if frameworks_loading() { "Retrying…" } else { "Retry" }
                        }
                    }
                }
            }
            if mapping_state == MappingLoadState::Loading {
                div { class: "sd-callout sd-callout-info", "data-testid": "policy-mappings-loading",
                    div { style: "font-size:12px;", "Loading compliance mappings…" }
                }
            } else if mapping_state == MappingLoadState::Failed {
                div { id: "policy-mappings-error", class: "sd-callout sd-callout-warn", "data-testid": "policy-mappings-error", role: "alert", tabindex: "-1",
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
                                             on_compliance_error.call("policy-mappings-error".to_string());
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
                                        button { class: "btn btn-ghost xs focus-ring", style: "color:var(--cf-text-muted);padding:4px 6px;", title: "Remove mapping", onclick: move |_| { let mut next = pending_mappings.read().clone(); remove_pending_mapping(&mut next, requirement_version_id); pending_mappings.set(next); mapping_action_focus_pending.set(true); }, "×" }
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
                                        button { class: "btn btn-ghost xs focus-ring", style: "color:var(--cf-text-muted);padding:4px 6px;", title: "Remove mapping", onclick: move |_| {
                                            if dialog_busy() { return; }
                                            let row_id = row.id;
                                            focus_policy_editor_element("policy-editor-dialog");
                                            error.set(None);
                                            catalog_retry.set(None);
                                            dialog_busy.set(true);
                                            spawn(async move {
                                                match delete_policy_mapping(&policy_id, &row_id).await {
                                                    Ok(()) => match fetch_policy_requirement_mappings(&policy_id).await {
                                                        Ok(value) => {
                                                            mappings.set(value);
                                                            mapping_load_error.set(None);
                                                            mapping_load_state.set(MappingLoadState::Loaded);
                                                            mapping_action_focus_pending.set(true);
                                                        }
                                                        Err(e) => {
                                                            mapping_load_error.set(Some(format!("Mapping removed, but refresh failed: {e}")));
                                                            mapping_load_state.set(MappingLoadState::Failed);
                                                            on_compliance_error.call("policy-mappings-error".to_string());
                                                        }
                                                    },
                                                    Err(e) => {
                                                        error.set(Some(format!("Failed to remove mapping: {e}")));
                                                        error_focus_pending.set(true);
                                                    }
                                                }
                                                dialog_busy.set(false);
                                            });
                                        }, "×" }
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
                button { id: "policy-mapping-add-trigger", class: "btn btn-ghost focus-ring cf-policy-mapping-add-trigger", style: "align-self:flex-start;", onclick: move |_| show_mapping_editor.set(true), "+ Add mapping" }
            }
            if mapping_target != MappingEditorTarget::Unavailable && show_mapping_editor() {
                div { style: "border:1px solid var(--cf-brand-purple);border-radius:10px;padding:14px;background:color-mix(in oklab, var(--cf-brand-purple) 5%, var(--cf-card-bg));display:flex;flex-direction:column;gap:14px;margin-top:8px;",
                     div { style: "font-size:12.5px;font-weight:600;", if editing_mapping_id.read().is_some() { "Edit mapping" } else { "Add mapping" } }
                    div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:-4px;line-height:1.4;", "Map this policy to a compliance requirement it implements, supports, or provides evidence for." }
                    div { class: "field", label { r#for: "policy-mapping-framework", style: "font-size:11px;", "Framework" }, select { id: "policy-mapping-framework", class: "input focus-ring", onchange: move |event| { let value = event.value(); versions.set(Vec::new()); version_id.set(None); requirement_id.set(None); requirement.set(None); results.set(Vec::new()); error.set(None); catalog_retry.set(None); if let Ok(id) = value.parse() { framework_id.set(Some(id)); spawn(async move { match fetch_compliance_framework_versions(&id).await { Ok(value) => versions.set(value), Err(e) => { error.set(Some(format!("Failed to load framework versions: {e}"))); catalog_retry.set(Some(MappingCatalogRetry::Versions)); on_compliance_error.call("policy-mapping-editor-error".to_string()); } } }); } }, option { value: "", "— Select framework —" }, for item in frameworks.read().iter() { option { value: "{item.id}", "{item.name}" } } } }
                    if !versions.read().is_empty() { div { class: "field", label { r#for: "policy-mapping-version", style: "font-size:11px;", "Version" }, select { id: "policy-mapping-version", class: "input focus-ring", onchange: move |event| { version_id.set(event.value().parse().ok()); }, option { value: "", "— Select version —" }, for item in versions.read().iter() { option { value: "{item.id}", "{item.version}" } } } } }
                    if version_id.read().is_some() && requirement.read().is_none() {
                        div { class: "field",
                            label { r#for: "policy-mapping-requirement", style: "font-size:11px;", "Requirement" }
                            input { id: "policy-mapping-requirement", class: "input focus-ring", placeholder: "Search by ID, title, CCI, SRG…", value: "{search}", oninput: move |event| { let value = event.value(); search.set(value.clone()); error.set(None); catalog_retry.set(None); if let Some(id) = *version_id.read() { spawn(async move { match search_requirements(&id, Some(&value), None, 25, 0).await { Ok(value) => results.set(value), Err(e) => { results.set(Vec::new()); error.set(Some(format!("Failed to search requirements: {e}"))); catalog_retry.set(Some(MappingCatalogRetry::Requirements)); on_compliance_error.call("policy-mapping-editor-error".to_string()); } } }); } } }
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
                        div { style: "display:flex;justify-content:flex-end;gap:8px;",
                         button { class: "btn btn-ghost focus-ring", r#type: "button", onclick: move |_| { editing_mapping_id.set(None); show_mapping_editor.set(false); mapping_action_focus_pending.set(true); }, "Cancel" }
                         button { class: "btn btn-primary focus-ring", onclick: move |_| {
                            if dialog_busy() { return; }
                            error.set(None);
                            catalog_retry.set(None);
                            let Some(rv_id) = *requirement_id.read() else { error.set(Some("Select a requirement.".into())); error_focus_pending.set(true); return; };
                            let relationship_value = relationship.read().clone();
                            let coverage_value = coverage.read().clone();
                            let rationale_value = non_empty(rationale.read().clone());
                            match mapping_target {
                                MappingEditorTarget::Pending => {
                                    let Some(item) = requirement.read().clone() else { error.set(Some("Select a requirement.".into())); error_focus_pending.set(true); return; };
                                    let Some(fw_id) = *framework_id.read() else { error.set(Some("Select a framework.".into())); error_focus_pending.set(true); return; };
                                    let Some(fv_id) = *version_id.read() else { error.set(Some("Select a framework version.".into())); error_focus_pending.set(true); return; };
                                    let Some(fw) = frameworks.read().iter().find(|item| item.id == fw_id).cloned() else { error.set(Some("Selected framework is unavailable.".into())); error_focus_pending.set(true); return; };
                                    let Some(fv) = versions.read().iter().find(|item| item.id == fv_id).cloned() else { error.set(Some("Selected framework version is unavailable.".into())); error_focus_pending.set(true); return; };
                                    let mut next = pending_mappings.read().clone();
                                    match add_pending_mapping(&mut next, pending_mapping_from_selection(&fw, &fv, &item, relationship_value, coverage_value, rationale_value)) {
                                         Ok(()) => { pending_mappings.set(next); requirement_id.set(None); requirement.set(None); search.set(String::new()); results.set(Vec::new()); relationship.set("implements".into()); coverage.set("full".into()); rationale.set(String::new()); show_mapping_editor.set(false); mapping_action_focus_pending.set(true); }
                                        Err(e) => { error.set(Some(e.to_string())); error_focus_pending.set(true); }
                                    }
                                 }
                                 MappingEditorTarget::Persisted(policy_id) => {
                                     focus_policy_editor_element("policy-editor-dialog");
                                     dialog_busy.set(true);
                                     spawn(async move {
                                         let was_editing = editing_mapping_id.read().is_some();
                                         let result = if let Some(mapping_id) = *editing_mapping_id.read() {
                                             let request = UpdatePolicyMappingRequest { relationship: relationship_value, coverage: coverage_value, rationale: rationale_value };
                                             update_policy_mapping(&policy_id, &mapping_id, &request).await
                                         } else {
                                             let request = CreatePolicyMappingRequest { requirement_version_id: rv_id, relationship: relationship_value, coverage: coverage_value, rationale: rationale_value, provenance: "manual".into() };
                                             create_policy_mapping(&policy_id, &request).await
                                         };
                                         match result {
                                             Ok(_) => {
                                                 editing_mapping_id.set(None);
                                                 requirement_id.set(None);
                                                 requirement.set(None);
                                                 version_id.set(None);
                                                 framework_id.set(None);
                                                 search.set(String::new());
                                                 rationale.set(String::new());
                                                 results.set(Vec::new());
                                                 versions.set(Vec::new());
                                                 show_mapping_editor.set(false);
                                                 match fetch_policy_requirement_mappings(&policy_id).await {
                                                     Ok(value) => {
                                                         mappings.set(value);
                                                         mapping_load_error.set(None);
                                                         mapping_load_state.set(MappingLoadState::Loaded);
                                                         mapping_action_focus_pending.set(true);
                                                     }
                                                     Err(e) => {
                                                         mapping_load_error.set(Some(format!("Mapping saved, but refresh failed: {e}")));
                                                         mapping_load_state.set(MappingLoadState::Failed);
                                                         on_compliance_error.call("policy-mappings-error".to_string());
                                                     }
                                                 }
                                             }
                                             Err(e) => {
                                                 let action = if was_editing { "update" } else { "add" };
                                                 error.set(Some(format!("Failed to {action} mapping: {e}")));
                                                 error_focus_pending.set(true);
                                             }
                                         }
                                         dialog_busy.set(false);
                                     });
                                }
                                MappingEditorTarget::Unavailable => {}
                            }
                        }, if dialog_busy() { "Saving..." } else if editing_mapping_id.read().is_some() { "Save mapping" } else { "Add mapping" } }
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

/// Renders the server-backed policy create and exact-version edit workflow.
///
/// The modal owns draft authoring state and presentation. Server APIs remain
/// authoritative for editability, immutable provenance, normalized mappings,
/// deletion eligibility, and persisted policy versions.
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
    let mut tabs_horizontal = use_signal(|| {
        #[cfg(target_arch = "wasm32")]
        {
            return policy_editor_tabs_are_horizontal();
        }
        #[cfg(not(target_arch = "wasm32"))]
        false
    });
    #[cfg(target_arch = "wasm32")]
    let orientation_resize_handler = use_hook(|| {
        let handler = Closure::<dyn FnMut()>::new(move || {
            tabs_horizontal.set(policy_editor_tabs_are_horizontal());
        });
        if let Some(window) = web_sys::window() {
            let _ =
                window.add_event_listener_with_callback("resize", handler.as_ref().unchecked_ref());
        }
        Rc::new(handler)
    });
    #[cfg(target_arch = "wasm32")]
    {
        let handler = orientation_resize_handler.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "resize",
                    handler.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }
    use_effect(move || {
        let tab = *active_tab.read();
        if tabs_horizontal() {
            reveal_policy_editor_tab(tab);
        }
    });
    // Mount Compliance on first use and retain it afterward. Its local draft
    // survives tab changes without fetching framework data before it is needed.
    let mut mappings_tab_mounted = use_signal(|| false);
    let mut compliance_error_focus_pending: Signal<Option<String>> = use_signal(|| None);

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
                        mappings_tab_mounted.set(true);
                        active_tab.set(PolicyEditorTab::Mappings);
                        compliance_error_focus_pending
                            .set(Some("policy-mappings-error".to_string()));
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
    // A successful mutation is not retried when only the catalog refresh fails.
    // The fallback keeps a newly created policy visible if it is outside page 1.
    let mut catalog_refresh_failed = use_signal(|| false);
    let mut catalog_refresh_fallback: Signal<Option<PolicyDefinition>> = use_signal(|| None);
    let mut confirm_delete = use_signal(|| false);
    let mut delete_typed = use_signal(String::new);
    let mut delete_succeeded = use_signal(|| false);
    let mut delete_cancel_focus_pending = use_signal(|| false);
    let mut error_focus_pending = use_signal(|| false);
    let mut evidence_validation_attempted = use_signal(|| false);
    let mut evidence_focus_pending: Signal<Option<String>> = use_signal(|| None);
    let mut rule_action_focus_pending = use_signal(|| false);
    let mut evidence_action_focus_pending = use_signal(|| false);

    use_effect(move || {
        if delete_cancel_focus_pending() && !confirm_delete() {
            focus_policy_editor_element("policy-editor-delete-trigger");
            delete_cancel_focus_pending.set(false);
        }
        if error_focus_pending() && !is_saving() {
            focus_policy_editor_element("policy-editor-error");
            error_focus_pending.set(false);
        }
        let compliance_target = compliance_error_focus_pending.read().clone();
        if let Some(id) = compliance_target
            && !is_saving()
        {
            mappings_tab_mounted.set(true);
            active_tab.set(PolicyEditorTab::Mappings);
            focus_policy_editor_element(&id);
            compliance_error_focus_pending.set(None);
        }
        let evidence_target = evidence_focus_pending.read().clone();
        if let Some(id) = evidence_target {
            focus_policy_editor_element(&id);
            evidence_focus_pending.set(None);
        }
        if rule_action_focus_pending() {
            focus_policy_editor_element("policy-editor-add-rule");
            rule_action_focus_pending.set(false);
        }
        if evidence_action_focus_pending() {
            focus_policy_editor_element("policy-editor-add-evidence");
            evidence_action_focus_pending.set(false);
        }
    });

    // Restore the control that opened the editor. This follows the same
    // web_sys focus approach as the shared Import / Export menu.
    #[cfg(target_arch = "wasm32")]
    let restore_focus = use_hook(|| {
        web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.active_element())
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    });
    #[cfg(target_arch = "wasm32")]
    {
        let restore_focus = restore_focus.clone();
        let delete_succeeded_for_focus = delete_succeeded;
        use_drop(move || {
            let target = if delete_succeeded_for_focus() {
                None
            } else {
                restore_focus.as_ref()
            };
            restore_policy_editor_focus(target);
        });
    }

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
    let can_save = !name_missing
        && current_save_blocker.is_none()
        && !*is_saving.read()
        && !catalog_refresh_failed();
    let rule_count = rules.read().len();
    let evidence_count = evidence.read().len();
    let selected_category =
        PolicyCategory::from_id(category.read().as_str()).unwrap_or(PolicyCategory::Deployment);
    let is_security = selected_category == PolicyCategory::Security;
    // Guidance derived from the selected category. Recommendations and the
    // off-category notice are informational only; `rules` is never filtered.
    // Only complete, persistable kinds are surfaced as suggestions.
    let recommended_kinds = actionable_recommended_enforcement(selected_category);
    let recommended_labels = recommended_kinds
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
    let (mapping_header_class, mapping_header_label) =
        mapping_header_metadata(mapping_state, mapping_count);
    let mapped_not_enforced = !enforcement_opaque
        && policy_is_mapped_not_enforced(mapping_state, mapping_count, rule_count);
    let delete_matches = delete_typed.read().as_str() == name_value;

    rsx! {
        div {
            class: "modal-backdrop cf-modal-overlay-z50",
            role: "presentation",
            onclick: move |_| if !is_saving() { on_close.call(()) },
            div {
                id: "policy-editor-dialog",
                class: "modal cf-policy-modal-panel cf-policy-editor-dialog",
                "data-testid": "policy-editor-modal",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "policy-editor-title",
                aria_describedby: "policy-editor-subtitle",
                aria_busy: if is_saving() { "true" } else { "false" },
                tabindex: "-1",
                onclick: |evt| evt.stop_propagation(),
                onkeydown: move |event| {
                    if event.key() == Key::Tab && is_saving() {
                        event.prevent_default();
                        focus_policy_editor_element("policy-editor-dialog");
                        return;
                    }
                    if event.key() == Key::Escape && !is_saving() {
                        event.prevent_default();
                        if confirm_delete() {
                            delete_cancel_focus_pending.set(true);
                            confirm_delete.set(false);
                            delete_typed.set(String::new());
                        } else {
                            on_close.call(());
                        }
                    }
                },

                span {
                    class: "cf-focus-sentinel",
                    tabindex: "0",
                    aria_label: "End of policy editor",
                    onfocus: move |_| if is_saving() { focus_policy_editor_element("policy-editor-dialog"); } else { focus_policy_editor_boundary(false); },
                }

                if *confirm_delete.read() {
                    // ── Danger zone: typed-confirmation delete ──────────────────
                    div { class: "modal-head", style: "background:rgba(248,113,113,0.06);",
                        div {
                        h2 { id: "policy-editor-title", style: "color:var(--cf-policy-danger-text);display:flex;align-items:center;gap:8px;",
                            svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "var(--cf-policy-red)", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                path { d: "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                                path { d: "M12 9v4M12 17h.01" }
                            }
                            "Remove policy"
                        }
                        p { id: "policy-editor-subtitle",
                            "This deletes the "
                            span { class: "mono", style: "font-weight:600;", "{name_value}" }
                            " policy."
                        }
                        }
                        button {
                            class: "btn-icon focus-ring",
                            aria_label: "Close policy editor",
                            disabled: is_saving(),
                            onclick: move |_| on_close.call(()),
                            "×"
                        }
                    }
                    div { class: "modal-body cf-policy-delete-confirmation", "inert": is_saving().then_some(""), aria_busy: if is_saving() { "true" } else { "false" },
                        div { class: "field",
                            label {
                                r#for: "policy-editor-delete-confirm",
                                "Type "
                                span { class: "mono", style: "color:var(--cf-policy-danger-text);font-weight:700;", "{name_value}" }
                                " to confirm"
                            }
                            input {
                                id: "policy-editor-delete-confirm",
                                autofocus: true,
                                class: "input focus-ring mono",
                                disabled: is_saving(),
                                placeholder: "{name_value}",
                                value: "{delete_typed}",
                                aria_describedby: if save_error.read().is_empty() { "policy-editor-delete-help" } else { "policy-editor-delete-help policy-editor-error" },
                                oninput: move |event| delete_typed.set(event.value()),
                            }
                        }
                        p { id: "policy-editor-delete-help", class: "help", "Deletion is permanent." }
                        if !save_error.read().is_empty() {
                            div { id: "policy-editor-error", class: "text-xs rounded px-3 py-2 cf-policy-modal-error", role: "alert", tabindex: "-1",
                                div { "{save_error}" }
                                if delete_succeeded() {
                                    div { class: "cf-policy-delete-recovery",
                                        button {
                                            class: "btn btn-ghost xs focus-ring",
                                            "data-testid": "policy-delete-refresh-retry",
                                            disabled: is_saving(),
                                            onclick: move |_| {
                                                if is_saving() { return; }
                                                let mut policy_library = policy_library;
                                                let mut save_error = save_error;
                                                let mut is_saving = is_saving;
                                                let on_close = on_close;
                                                focus_policy_editor_element("policy-editor-dialog");
                                                is_saving.set(true);
                                                spawn(async move {
                                                    match policies_api::load_policies().await {
                                                        policies_api::PolicyLoadResult::Ok(latest) => {
                                                            policy_library.set(latest);
                                                            is_saving.set(false);
                                                            on_close.call(());
                                                        }
                                                        policies_api::PolicyLoadResult::Err(error) => {
                                                            save_error.set(format!("Policy removed, but refresh failed: {error}"));
                                                            is_saving.set(false);
                                                            error_focus_pending.set(true);
                                                        }
                                                    }
                                                });
                                            },
                                            if is_saving() { "Retrying…" } else { "Retry catalog refresh" }
                                        }
                                        button { class: "btn btn-ghost xs focus-ring", "data-testid": "policy-delete-close", disabled: is_saving(), onclick: move |_| on_close.call(()), "Close editor" }
                                        button { class: "btn btn-ghost xs focus-ring", "data-testid": "policy-delete-reload", disabled: is_saving(), onclick: move |_| reload_policy_page(), "Reload page" }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "modal-foot cf-policy-delete-actions", "inert": is_saving().then_some(""),
                        button {
                            id: "policy-editor-delete-cancel",
                            class: "btn btn-ghost focus-ring",
                            disabled: is_saving(),
                            onclick: move |_| {
                                delete_cancel_focus_pending.set(true);
                                confirm_delete.set(false);
                                delete_typed.set(String::new());
                            },
                            "Cancel"
                        }
                        button {
                            class: "btn focus-ring",
                            disabled: !delete_matches || is_saving() || delete_succeeded(),
                            style: if delete_matches { "background:#dc2626;color:white;" } else { "background:var(--cf-subtle-bg);color:var(--cf-text-muted);" },
                            onclick: move |_| {
                                if is_saving() || delete_succeeded() { return; }
                                let Some(policy_id) = *editing_policy_id.read() else { return; };
                                let mut policy_library = policy_library;
                                let mut save_error = save_error;
                                let mut is_saving = is_saving;
                                let on_close = on_close;
                                focus_policy_editor_element("policy-editor-dialog");
                                is_saving.set(true);
                                spawn(async move {
                                    match delete_deployment_policy(&policy_id).await {
                                        Ok(()) => {
                                             delete_succeeded.set(true);
                                             match policies_api::load_policies().await {
                                                 policies_api::PolicyLoadResult::Ok(latest) => {
                                                     policy_library.set(latest);
                                                     on_close.call(());
                                                 }
                                                  policies_api::PolicyLoadResult::Err(error) => {
                                                      save_error.set(format!("Policy removed, but refresh failed: {error}"));
                                                      is_saving.set(false);
                                                      error_focus_pending.set(true);
                                                  }
                                             }
                                        }
                                        Err(error) => {
                                            save_error.set(format!("Failed to remove policy: {error}"));
                                            is_saving.set(false);
                                            error_focus_pending.set(true);
                                        }
                                    }
                                });
                            },
                            if is_saving() { "Removing…" } else if delete_succeeded() { "Policy removed" } else { "Remove policy" }
                        }
                    }
                } else {
                    // ── Header ──────────────────────────────────────────────────
                    div { class: "modal-head",
                        div {
                        h2 { id: "policy-editor-title", style: "display:flex;align-items:center;gap:6px;",
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
                        p { id: "policy-editor-subtitle", "{subtitle}" }
                        if !edit_description.read().trim().is_empty() {
                            p { class: "cf-policy-editor-header-description", "{edit_description}" }
                        }
                        div { class: "cf-policy-editor-header-meta", "data-testid": "policy-editor-state",
                            span {
                                class: "chip",
                                style: "color:{selected_category.color()};background:color-mix(in oklab, {selected_category.color()} 14%, transparent);",
                                "{selected_category.short_label()}"
                            }
                            if is_imported {
                                span { class: "chip chip-info", "Imported" }
                            }
                            span {
                                class: "{mapping_header_class}",
                                "{mapping_header_label}"
                            }
                        }
                        }
                        button {
                            class: "btn-icon focus-ring",
                            aria_label: "Close policy editor",
                            disabled: is_saving(),
                            onclick: move |_| on_close.call(()),
                            "×"
                        }
                    }

                    // ── Body ────────────────────────────────────────────────────
                    div {
                        class: "modal-body cf-policy-modal-body",
                        style: "overflow-y:auto;",
                        "inert": is_saving().then_some(""),
                        // Section order follows the design: Basics, Enforcement,
                        // Compliance, Evidence, then read-only Provenance.
                        div { class: "cf-policy-editor-layout",
                            aside { class: "cf-policy-editor-rail",
                                div { class: "cf-modal-tabs cf-policy-editor-nav", role: "tablist", aria_label: "Policy editor sections", aria_orientation: if tabs_horizontal() { "horizontal" } else { "vertical" },
                                    PolicyEditorTabButton { tab: PolicyEditorTab::Details, active: *active_tab.read(), include_provenance: is_imported, horizontal: tabs_horizontal(), label: "Basics", on_select: move |tab| { if tab == PolicyEditorTab::Mappings { mappings_tab_mounted.set(true); } active_tab.set(tab); } }
                                    PolicyEditorTabButton { tab: PolicyEditorTab::Enforcement, active: *active_tab.read(), include_provenance: is_imported, horizontal: tabs_horizontal(), label: if rule_count > 0 { format!("Enforcement · {rule_count}") } else if is_imported { "Enforcement · Needs refinement".to_string() } else { "Enforcement · None".to_string() }, on_select: move |tab| { if tab == PolicyEditorTab::Mappings { mappings_tab_mounted.set(true); } active_tab.set(tab); } }
                                    PolicyEditorTabButton { tab: PolicyEditorTab::Mappings, active: *active_tab.read(), include_provenance: is_imported, horizontal: tabs_horizontal(), label: match mapping_state { MappingLoadState::Loading => "Compliance · …".to_string(), MappingLoadState::Failed => "Compliance · unavailable".to_string(), MappingLoadState::Loaded if mapping_count > 0 => format!("Compliance · {mapping_count}"), MappingLoadState::Loaded => "Compliance · Unmapped".to_string() }, on_select: move |tab| { mappings_tab_mounted.set(true); active_tab.set(tab); } }
                                    PolicyEditorTabButton { tab: PolicyEditorTab::Evidence, active: *active_tab.read(), include_provenance: is_imported, horizontal: tabs_horizontal(), label: format!("Evidence · {evidence_count}"), on_select: move |tab| { if tab == PolicyEditorTab::Mappings { mappings_tab_mounted.set(true); } active_tab.set(tab); } }
                                    if is_imported {
                                        PolicyEditorTabButton { tab: PolicyEditorTab::Provenance, active: *active_tab.read(), include_provenance: true, horizontal: tabs_horizontal(), label: "Provenance".to_string(), on_select: move |tab| { if tab == PolicyEditorTab::Mappings { mappings_tab_mounted.set(true); } active_tab.set(tab); } }
                                    }
                                    span { class: "cf-policy-editor-nav-affordance", aria_hidden: "true", "Scroll sections →" }
                                }
                            }
                            div { class: "cf-policy-editor-content",
                        if mapped_not_enforced {
                            div { class: "sd-callout sd-callout-warn", "data-testid": "policy-editor-mapped-not-enforced", style: "margin:8px 0 0;font-size:11.5px;",
                                strong { "Mapped, not enforced." }
                                span { " This policy claims {mapping_count} compliance requirement(s) but asserts nothing yet, so it cannot pass or fail. Add enforcement to make it real." }
                            }
                        }
                        div {
                        id: "policy-editor-panel",
                        class: "cf-modal-tab-panel",
                        role: "tabpanel",
                        aria_labelledby: policy_editor_tab_id(*active_tab.read()),
                        if *active_tab.read() == PolicyEditorTab::Details {
                        div { style: "display:grid;grid-template-columns:1fr;gap:14px;",
                            div { class: "field",
                                label { r#for: "policy-editor-name", "Name" }
                                input {
                                    id: "policy-editor-name",
                                    autofocus: true,
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
                                aria_label: "Framework",
                                class: "input focus-ring", value: "{framework}",
                                onchange: move |event| framework.set(event.value()),
                                option { value: "", "Select a framework" }
                                for standard in STANDARD_FRAMEWORKS { option { value: "{standard}", "{standard}" } }
                                for existing in framework_options.iter() { option { value: "{existing}", "{existing}" } }
                                option { value: "__custom__", "Define new framework..." }
                            }
                            if framework.read().as_str() == "__custom__" {
                                input {
                                    aria_label: "Framework name",
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
                            select { aria_label: "Control family", class: "input focus-ring", value: "{control_family}", onchange: move |event| control_family.set(event.value()),
                                option { value: "", "Unassigned" }
                                for family in NIST_CONTROL_FAMILIES { option { value: "{family}", "{family}" } }
                            }
                        }
                        }
                        if framework.read().as_str() == "CMMC 2.0" {
                        div { class: "field",
                            label { "CMMC level" }
                            select { aria_label: "CMMC level", class: "input focus-ring", value: "{cmmc_level}", onchange: move |event| cmmc_level.set(event.value()),
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
                            input { aria_label: "CIS section", class: "input focus-ring mono", placeholder: "e.g. 5.2.3", value: "{cis_section}", oninput: move |event| cis_section.set(event.value()) }
                        }
                        }
                        div { class: "field",
                            label { "Severity" }
                            div { class: "seg seg-sev", role: "radiogroup", aria_label: "Severity", style: "width:fit-content;",
                                for (value, label, color) in [("", "Unset", "var(--cf-text-muted)"), ("high", "High", "var(--cf-policy-red)"), ("medium", "Medium", "var(--cf-policy-amber)"), ("low", "Low", "var(--cf-policy-blue)")] {
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
                            textarea { aria_label: "Rationale", class: "input focus-ring", rows: "2", placeholder: "Why this policy exists — shown in detail view", style: "resize:vertical;", value: "{rationale}", oninput: move |event| rationale.set(event.value()) }
                        }
                        }
                        }

                           if mappings_tab_mounted() {
                           div { hidden: *active_tab.read() != PolicyEditorTab::Mappings,
                               PolicyMappingsTab {
                                  is_editing,
                                  editing_policy_version_id,
                                  mappings_editable,
                                  mappings,
                                   pending_mappings,
                                   mapping_load_state,
                                   mapping_load_error,
                                   dialog_busy: is_saving,
                                   on_compliance_error: move |id: String| {
                                       mappings_tab_mounted.set(true);
                                       active_tab.set(PolicyEditorTab::Mappings);
                                       compliance_error_focus_pending.set(Some(id));
                                   },
                               }
                           }
                           }

                        // Read-only imported origin, recorded at import time.
                        if *active_tab.read() == PolicyEditorTab::Provenance {
                            div { class: "cf-policy-provenance", "data-testid": "policy-editor-provenance",
                                div { class: "cf-policy-provenance-heading",
                                    label { "Provenance" }
                                    span { class: "chip chip-neutral", style: "font-size:10px;", "read-only" }
                                }
                                div { class: "cf-policy-provenance-intro",
                                    "Recorded when this control was imported. Editing where information came from would rewrite history, so it cannot be changed here. Compliance relationships live in Compliance."
                                }
                                for origin in provenance.iter() {
                                    div { key: "{origin.source_artifact_id}-{origin.origin_policy_version_id}-{origin.source_identity.clone().unwrap_or_default()}",
                                        class: "cf-policy-provenance-detail",
                                        div { class: "cf-policy-provenance-card-head",
                                            div { class: "cf-policy-provenance-artifact",
                                                span { "Source artifact" }
                                                strong { class: "mono", title: "{origin.filename}", "{origin.filename}" }
                                            }
                                            div { class: "cf-policy-provenance-badges",
                                                span { class: "chip chip-info", style: "font-size:10px;", "Imported" }
                                                if origin.inherited {
                                                    span { class: "chip chip-neutral", "data-testid": "policy-provenance-inherited", style: "font-size:10px;", "Inherited from source version" }
                                                }
                                                if let Some(fidelity) = origin.fidelity.as_ref() {
                                                    span { class: "chip chip-neutral", style: "font-size:10px;", "{fidelity}" }
                                                }
                                            }
                                        }
                                        div { class: "cf-policy-provenance-row", span { "Source type" }, span { class: "mono cf-policy-provenance-value", title: "{origin.media_type}", "{origin.media_type}" } }
                                        div { class: "cf-policy-provenance-row", span { "SHA-256" }, span { class: "mono cf-policy-provenance-value", title: "{origin.sha256}", "{origin.sha256}" } }
                                        if let Some(identity) = origin.source_identity.as_ref() {
                                            div { class: "cf-policy-provenance-row", span { {origin.object_kind.clone().map(|kind| format!("Source {kind} ID")).unwrap_or_else(|| "Source ID".to_string())} }, span { class: "mono cf-policy-provenance-value", title: "{identity}", "{identity}" } }
                                        }
                                        if let Some(xccdf) = origin.detected_xccdf_version.as_ref() {
                                            div { class: "cf-policy-provenance-row", span { "XCCDF version" }, span { class: "mono cf-policy-provenance-value", title: "{xccdf}", "{xccdf}" } }
                                        }
                                        div { class: "cf-policy-provenance-row", span { "Parser" }, span { class: "mono cf-policy-provenance-value", title: "{origin.parser_version}", "{origin.parser_version}" } }
                                        div { class: "cf-policy-provenance-row",
                                            span { "Imported" }
                                            span { class: "mono cf-policy-provenance-imported", {match origin.imported_by_display.as_ref() { Some(user) => format!("{} · {}", origin.imported_at.to_rfc3339(), user), None => origin.imported_at.to_rfc3339() }} }
                                        }
                                    }
                                }
                            }
                        }

                         // Enforcement builder.
                        if *active_tab.read() == PolicyEditorTab::Enforcement {
                        div { style: "margin-top:6px;",
                            div { class: "cf-policy-section-head",
                                h3 { style: "margin:0;font-size:12px;font-weight:600;color:var(--cf-text-primary);", "Enforcement · Assertions & gate rules ({rule_count})" }
                                span { style: "font-size:11px;color:var(--cf-text-muted);", "All must hold — each compiles to a policy check." }
                            }
                            if enforcement_opaque {
                                div { class: "sd-callout sd-callout-warn", "data-testid": "policy-enforcement-opaque", style: "margin-bottom:8px;font-size:12px;",
                                    strong { "Enforcement preserved but unavailable. " }
                                    "This composite contains unsupported, malformed, or partially hydrated data. Known-looking rows are intentionally hidden because showing only a subset would be misleading. The complete stored configuration remains read-only."
                                }
                            } else if rule_count == 0 {
                                div { class: if is_imported { "sd-callout sd-callout-warn" } else { "sd-callout sd-callout-info" }, "data-testid": "policy-enforcement-empty", style: "margin-bottom:8px;font-size:12px;",
                                    if is_imported {
                                        span { strong { "Enforcement needs refinement." } " This control was imported with its compliance mappings and provenance, but no assertion was inferred. Until one exists it asserts nothing." }
                                    } else {
                                        span { strong { "No enforcement defined." } " Add at least one requirement for this policy to have an effect. Saving with none is valid; the policy simply asserts nothing." }
                                    }
                                }
                            }
                            div { class: "sd-callout sd-callout-info", "data-testid": "policy-enforcement-recommendations", style: "margin-bottom:8px;font-size:11px;display:flex;flex-direction:column;gap:7px;",
                                div {
                                    h4 { style: "margin:0;font-size:11px;display:inline;", "Suggested for {selected_category.label()}" }
                                    span { " · Guidance only. Suggestions never add, remove, or restrict rules." }
                                }
                                if recommended_labels.is_empty() {
                                    span { "data-testid": "policy-enforcement-no-recommendations",
                                        "No category-specific suggestions are available. All eight complete kinds remain available below."
                                    }
                                } else {
                                    div { "data-testid": "policy-enforcement-suggestion-cards", style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(170px,1fr));gap:6px;",
                                        for kind in recommended_kinds.iter() {
                                            div { style: "padding:7px 8px;border:1px solid var(--cf-divider);border-radius:7px;background:var(--cf-subtle-bg);",
                                                div { style: "font-weight:650;color:var(--cf-text-primary);", "{rule_label(kind)}" }
                                                div { style: "margin-top:2px;color:var(--cf-text-muted);line-height:1.35;", "{rule_blurb(kind)}" }
                                            }
                                        }
                                    }
                                }
                                details { "data-testid": "policy-enforcement-alternatives",
                                    summary { style: "cursor:pointer;font-weight:600;", "All available alternatives · 8" }
                                    div { style: "margin-top:5px;display:flex;gap:5px;flex-wrap:wrap;",
                                        for (kind, label, persisted) in RULE_OPTIONS {
                                            if persisted { span { class: "chip chip-neutral", title: "{rule_blurb(kind)}", "{label}" } }
                                        }
                                    }
                                }
                            }
                            if !off_category_labels.is_empty() {
                                div { class: "sd-callout sd-callout-info", "data-testid": "policy-off-category-notice", style: "margin-bottom:8px;font-size:11px;",
                                    "{off_category_labels} " span { {if off_category_rule_count == 1 { "is" } else { "are" }} }
                                    " unusual for " strong { "{selected_category.label()}" } ". Nothing was changed or removed."
                                }
                            }
                            if !enforcement_opaque {
                                div { style: "display:flex;flex-direction:column;gap:6px;",
                                    for (index, rule) in rules.read().iter().cloned().enumerate() {
                                    div {
                                        key: "rule-{rule.id}",
                                        "data-testid": "policy-rule-row-{index}",
                                        "data-rule-id": "{rule.id}",
                                        "data-rule-kind": "{rule.kind}",
                                        class: "cf-policy-rule-shell",
                                        RuleEditorRow { index, rule: rule.clone(), rules, enforcement_changed }
                                        div { class: "cf-policy-rule-controls",
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
                                                     rule_action_focus_pending.set(true);
                                                 },
                                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                    path { d: "M18 6 6 18M6 6l12 12" }
                                                }
                                            }
                                        }
                                    }
                                    }
                                }
                            }
                            if !enforcement_opaque {
                                div { style: "margin-top:8px;display:flex;gap:8px;flex-wrap:wrap;",
                                select {
                                    id: "policy-editor-add-rule",
                                    class: "input focus-ring",
                                    "data-testid": "policy-editor-add-rule",
                                    aria_label: "Add enforcement rule",
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
                                    option { value: "", disabled: true, "+ Add enforcement · Add assertion / rule…" }
                                    for (kind, label, persisted) in RULE_OPTIONS {
                                        if persisted {
                                            option { value: "{kind}", "{label}" }
                                        }
                                    }
                                }
                                }
                            }
                        }
                        }

                        // Evidence for ATO builder.
                         if *active_tab.read() == PolicyEditorTab::Evidence {
                         div { style: "margin-top:6px;",
                             div { class: "cf-policy-section-head",
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
                                         EvidenceEditorRow { index, evidence: ev.clone(), evidence_list: evidence, show_validation: evidence_validation_attempted() }
                                         button {
                                             class: "btn-icon focus-ring",
                                             title: "Remove evidence",
                                             onclick: move |_| {
                                                 let mut next = evidence.read().clone();
                                                  if index < next.len() { next.remove(index); }
                                                  evidence.set(next);
                                                  evidence_action_focus_pending.set(true);
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
                                     id: "policy-editor-add-evidence",
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
                                         style: "color:var(--cf-policy-red);border-color:color-mix(in oklab, var(--cf-policy-red) 35%, transparent);",
                                         title: "Clear all evidence",
                                          onclick: move |_| {
                                              evidence.set(Vec::new());
                                              evidence_action_focus_pending.set(true);
                                          },
                                         "Clear all"
                                     }
                                 }
                             }
                         }
                         }

                        }

                        if danger_zone_visible(is_editing, *active_tab.read()) {
                            div { style: "margin-top:10px;padding-top:14px;border-top:1px solid var(--cf-divider);",
                                div { style: "font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);margin-bottom:8px;", "Danger zone" }
                                button {
                                    id: "policy-editor-delete-trigger",
                                    class: "btn btn-ghost focus-ring",
                                    style: "color:var(--cf-policy-red);border-color:color-mix(in oklab, var(--cf-policy-red) 35%, transparent);",
                                    onclick: move |_| confirm_delete.set(true),
                                    svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:6px;vertical-align:text-bottom;",
                                        path { d: "M18 6 6 18M6 6l12 12" }
                                    }
                                    "Remove policy"
                                }
                            }
                        }

                        if !save_error.read().is_empty() {
                            div { id: "policy-editor-error", class: "text-xs rounded px-3 py-2 cf-policy-modal-error", role: "alert", tabindex: "-1", style: "margin-top:10px;",
                                div { "{save_error}" }
                                if catalog_refresh_failed() {
                                    button {
                                        class: "btn btn-ghost xs focus-ring cf-policy-refresh-retry",
                                        "data-testid": "policy-catalog-refresh-retry",
                                        disabled: is_saving(),
                                        onclick: move |_| {
                                            if is_saving() { return; }
                                            let fallback = catalog_refresh_fallback.read().clone();
                                            let mut policy_library = policy_library;
                                            let mut save_error = save_error;
                                            let mut is_saving = is_saving;
                                            let mut catalog_refresh_failed = catalog_refresh_failed;
                                            let on_close = on_close;
                                            focus_policy_editor_element("policy-editor-dialog");
                                            is_saving.set(true);
                                            spawn(async move {
                                                match policies_api::load_policies().await {
                                                    policies_api::PolicyLoadResult::Ok(mut latest) => {
                                                        if let Some(fallback) = fallback
                                                            && !latest.iter().any(|policy| policy.id == fallback.id)
                                                        {
                                                            latest.insert(0, fallback);
                                                        }
                                                        policy_library.set(latest);
                                                        catalog_refresh_failed.set(false);
                                                        is_saving.set(false);
                                                        on_close.call(());
                                                    }
                                                    policies_api::PolicyLoadResult::Err(error) => {
                                                        save_error.set(format!("Policy saved, but catalog refresh failed: {error}"));
                                                        is_saving.set(false);
                                                        error_focus_pending.set(true);
                                                    }
                                                }
                                            });
                                        },
                                        if is_saving() { "Retrying…" } else { "Retry catalog refresh" }
                                    }
                                }
                            }
                        }

                        if let Some(blocker) = current_save_blocker.as_ref() {
                            div { id: "policy-editor-enforcement-error", role: "alert", class: "text-xs rounded px-3 py-2 cf-policy-modal-error", style: "margin-top:10px;", "{blocker}" }
                        }
                            }
                        }
                    }

                    // ── Footer ──────────────────────────────────────────────────
                    div { class: "modal-foot",
                        "inert": is_saving().then_some(""),
                        button {
                            id: "policy-editor-cancel",
                            class: "btn btn-ghost focus-ring",
                            disabled: is_saving(),
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            id: "policy-editor-save",
                            class: "btn btn-primary focus-ring",
                            disabled: !can_save,
                            onclick: move |_| {
                                if is_saving() { return; }
                                let name = edit_name.read().clone();
                                if name.trim().is_empty() {
                                    save_error.set("Policy name is required".to_string());
                                    error_focus_pending.set(true);
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
                                    error_focus_pending.set(true);
                                    return;
                                }
                                let mut policy_library = policy_library;
                                 let mut save_error = save_error;
                                 let mut is_saving = is_saving;
                                 let mut catalog_refresh_failed = catalog_refresh_failed;
                                 let mut catalog_refresh_fallback = catalog_refresh_fallback;
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
                                    error_focus_pending.set(true);
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

                                 let evidence_snapshot = evidence.read().clone();
                                 let pending_mapping_snapshot = pending_mappings.read().clone();
                                 let validation_errors: Vec<(usize, &'static str, String)> = evidence_snapshot
                                     .iter()
                                     .enumerate()
                                     .filter_map(|(idx, ev)| {
                                         ev.validation_error().map(|(field, error)| (idx, field, error))
                                     })
                                     .collect();
                                 if !validation_errors.is_empty() {
                                     save_error.set(validation_errors.iter().map(|(index, _, error)| format!("Evidence row {}: {error}", index + 1)).collect::<Vec<_>>().join("; "));
                                     evidence_validation_attempted.set(true);
                                     let (index, field, _) = &validation_errors[0];
                                     active_tab.set(PolicyEditorTab::Evidence);
                                     evidence_focus_pending.set(Some(evidence_field_id(*index, &evidence_snapshot[*index].kind, field)));
                                     return;
                                 }

                                  evidence_validation_attempted.set(false);
                                  save_error.set(String::new());
                                  catalog_refresh_failed.set(false);
                                  catalog_refresh_fallback.set(None);
                                  focus_policy_editor_element("policy-editor-dialog");
                                 is_saving.set(true);

                                 let initial_evidence_clone = initial_evidence.clone();
                                 spawn(async move {
                                     let result = if let Some(policy_id) = editing_id {
                                          // Determine evidence_specs dirty state:
                                          // Compare current evidence against initial state
                                          // - None if unchanged (preserve existing)
                                          // - Some([]) if cleared to empty
                                          // - Some(items) if modified/added
                                          let evidence_specs = {
                                              let current_count = evidence_snapshot.len();
                                              let initial_count = initial_evidence_clone.len();

                                              // No change = preserve
                                              if current_count == initial_count && evidence_snapshot == initial_evidence_clone {
                                                  None
                                              } else {
                                                  // Changed: convert and send (including empty array if cleared)
                                                  let specs: Vec<EvidenceSpec> = evidence_snapshot
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
                                               framework: Some(selected_framework),
                                               severity: Some(selected_severity),
                                               control_family: Some(selected_control_family),
                                               cmmc_level: Some(selected_cmmc_level),
                                               cis_section: Some(selected_cis_section),
                                               rationale: Some(selected_rationale),
                                              evidence_specs,
                                          };
                                        update_deployment_policy(&policy_id, &request).await.map(|_| None)
                                     } else {
                                           let evidence_specs: Vec<EvidenceSpec> = evidence_snapshot
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
                                              requirement_mappings: pending_mapping_snapshot
                                                  .iter()
                                                  .map(PendingPolicyMapping::mapping_request)
                                                  .collect(),
                                          };
                                        create_deployment_policy(&request).await.map(Some)
                                    };

                                    match result {
                                         Ok(created_opt) => {
                                             let created_fallback = created_opt
                                                 .as_ref()
                                                 .cloned()
                                                 .map(policies_api::policy_record_to_definition);
                                             // Fetch the updated list so edits (name changes, etc.) are
                                            // reflected globally. The list response includes the
                                            // current_version_id join that the create endpoint omits,
                                            // so for a new policy we prefer the entry from the refreshed
                                            // list over the raw create response. If the refresh fails we
                                            // fall back to the create response so the card is still shown.
                                            let created_id = created_opt.as_ref().map(|c| c.id);
                                            let refresh_succeeded = match policies_api::load_policies().await {
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
                                                    true
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
                                                     catalog_refresh_fallback.set(created_fallback);
                                                     catalog_refresh_failed.set(true);
                                                     save_error.set(format!("Policy saved, but catalog refresh failed: {error}"));
                                                     false
                                                 }
                                             };
                                             is_saving.set(false);
                                             if refresh_succeeded {
                                                 on_close.call(());
                                             } else {
                                                 error_focus_pending.set(true);
                                             }
                                        }
                                        Err(error) => {
                                             save_error.set(format!("Failed to save policy: {error}"));
                                             is_saving.set(false);
                                             error_focus_pending.set(true);
                                        }
                                    }
                                });
                            },
                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:6px;vertical-align:text-bottom;",
                                path { d: "M20 6 9 17l-5-5" }
                            }
                            if *is_saving.read() { "Saving…" } else if catalog_refresh_failed() { "Saved" } else { "{action_label}" }
                        }
                    }
                }
                span {
                    class: "cf-focus-sentinel",
                    tabindex: "0",
                    aria_label: "Start of policy editor",
                    onfocus: move |_| if is_saving() { focus_policy_editor_element("policy-editor-dialog"); } else { focus_policy_editor_boundary(true); },
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
    include_provenance: bool,
    horizontal: bool,
    label: String,
    on_select: EventHandler<PolicyEditorTab>,
) -> Element {
    let selected = tab == active;
    let class = match tab {
        PolicyEditorTab::Details => "source",
        PolicyEditorTab::Mappings => "mappings",
        PolicyEditorTab::Enforcement => "enforcement",
        PolicyEditorTab::Evidence => "evidence",
        PolicyEditorTab::Provenance => "provenance",
    };
    rsx! { button {
        id: policy_editor_tab_id(tab),
        class: if selected { format!("cf-modal-tab cf-modal-tab--active cf-modal-tab--{class}") } else { format!("cf-modal-tab cf-modal-tab--{class}") },
        role: "tab",
        aria_controls: "policy-editor-panel",
        aria_selected: if selected { "true" } else { "false" },
        tabindex: if selected { "0" } else { "-1" },
        "data-testid": policy_editor_tab_id(tab),
        onclick: move |_| on_select.call(tab),
        onkeydown: move |event| {
            let movement = match event.key() {
                Key::ArrowLeft if horizontal => Some(PolicyEditorTabMove::Previous),
                Key::ArrowRight if horizontal => Some(PolicyEditorTabMove::Next),
                Key::ArrowUp if !horizontal => Some(PolicyEditorTabMove::Previous),
                Key::ArrowDown if !horizontal => Some(PolicyEditorTabMove::Next),
                Key::Home => Some(PolicyEditorTabMove::First),
                Key::End => Some(PolicyEditorTabMove::Last),
                _ => None,
            };
            if let Some(movement) = movement {
                event.prevent_default();
                let next = move_policy_editor_tab(tab, movement, include_provenance);
                on_select.call(next);
                focus_policy_editor_element(policy_editor_tab_id(next));
            }
        },
        "{label}"
    } }
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
                "pin_required" => rsx! { span { style: "color:var(--cf-text-secondary);", "The evaluated flake source must resolve to an immutable revision" } },
                "cve_block" => rsx! {
                    div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        span { "Block deploy when" }
                        select {
                            "data-testid": "policy-rule-cve-severity-{index}",
                            aria_label: "CVE severity threshold",
                            class: "input focus-ring",
                            style: "width:auto;font-size:12px;padding:4px 8px;",
                            value: "{rule.severity}",
                            onchange: move |event| set_rule_field!(severity, event.value()),
                            option { value: "critical", "critical" }
                            option { value: "high", "high" }
                            option { value: "medium", "medium" }
                            option { value: "low", "low" }
                        }
                        span { "CVEs exceed" }
                        input {
                            "data-testid": "policy-rule-cve-max-{index}",
                            aria_label: "Maximum allowed CVEs",
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
                        "data-testid": "policy-rule-packages-installed-{index}",
                        class: "input focus-ring mono",
                        style: "font-size:12px;padding:5px 8px;",
                        placeholder: "openssh, auditd, aide",
                        value: "{rule.packages}",
                        oninput: move |event| set_rule_field!(packages, event.value()),
                    }
                },
                "packages_absent" => rsx! {
                    input {
                        "data-testid": "policy-rule-packages-absent-{index}",
                        class: "input focus-ring mono",
                        style: "font-size:12px;padding:5px 8px;",
                        placeholder: "telnet, rsh",
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
                            OptionSearchState::Error(error) => rsx! { span { "data-testid": "policy-option-search-error", class: "help", style: "color:var(--cf-policy-red);", "Search failed: {error}. You can still enter a path manually." } },
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
                        div { class: "cf-policy-rule-value-row",
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
                                "integer" => rsx! { input { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected integer value", r#type: "number", class: "input focus-ring mono", value: "{rule.value.as_i64().map(|value| value.to_string()).or_else(|| rule.value.as_str().map(str::to_string)).unwrap_or_default()}", oninput: move |event| { let raw = event.value(); if let Ok(value) = raw.parse::<i64>() { set_rule_field!(value, serde_json::json!(value)); } else { set_rule_field!(value, serde_json::Value::String(raw)); } } } },
                                "lines" => rsx! { textarea { "data-testid": "policy-rule-nixos-value-{index}", aria_label: "Expected multiline value", class: "input focus-ring mono code-editor cf-policy-multiline-value", rows: "8", value: "{rule.value.as_str().unwrap_or_default()}", oninput: move |event| set_rule_field!(value, serde_json::Value::String(event.value())) } },
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
                        "data-testid": "policy-rule-custom-eval-message-{index}",
                        class: "input focus-ring",
                        style: "font-size:11px;padding:5px 8px;",
                        placeholder: "Failure message shown when assertion fails",
                        value: "{rule.message}",
                        oninput: move |event| set_rule_field!(message, event.value()),
                    }
                    span { class: "help", "The browser checks size, quoted strings, and delimiters. The server's Nix parser performs authoritative syntax validation when you save." }
                },
                "time_window" => rsx! {
                    div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        span { "Only between" }
                        input { "data-testid": "policy-rule-time-from-{index}", aria_label: "Window start time", class: "input focus-ring mono", style: "width:70px;font-size:12px;padding:4px 8px;", value: "{rule.from}", oninput: move |event| set_rule_field!(from, event.value()) }
                        span { "–" }
                        input { "data-testid": "policy-rule-time-to-{index}", aria_label: "Window end time", class: "input focus-ring mono", style: "width:70px;font-size:12px;padding:4px 8px;", value: "{rule.to}", oninput: move |event| set_rule_field!(to, event.value()) }
                        span { "on" }
                        input { "data-testid": "policy-rule-time-days-{index}", aria_label: "Window days", class: "input focus-ring mono", style: "width:140px;font-size:12px;padding:4px 8px;", value: "{rule.days}", oninput: move |event| set_rule_field!(days, event.value()) }
                        input { "data-testid": "policy-rule-timezone-{index}", aria_label: "Window time zone", class: "input focus-ring mono", style: "width:170px;font-size:12px;padding:4px 8px;", value: "{rule.tz}", oninput: move |event| set_rule_field!(tz, event.value()) }
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
// Evidence editor row.
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EvidenceEditorRow(
    index: usize,
    evidence: PolicyEvidence,
    evidence_list: Signal<Vec<PolicyEvidence>>,
    show_validation: bool,
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
    let validation = show_validation
        .then(|| evidence.validation_error())
        .flatten();
    let invalid_field = validation.as_ref().map(|(field, _)| *field);
    let error_id = evidence_error_id(index);

    rsx! {
        div { style: "display:flex;flex-direction:column;gap:4px;font-size:12px;width:100%;",
            span { style: "display:flex;align-items:center;gap:6px;font-weight:600;", "{label}" }
            match kind.as_str() {
                "command" => rsx! {
                    input { id: evidence_field_id(index, &kind, "cmd"), "data-testid": "policy-evidence-command-cmd-{index}", aria_invalid: if invalid_field == Some("cmd") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("cmd")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "sshd -T | grep permitrootlogin", value: "{evidence.cmd}", oninput: move |event| set_ev_field!(cmd, event.value()) }
                    div { style: "display:flex;align-items:center;gap:6px;",
                        span { style: "font-size:11px;color:var(--cf-text-muted);", "expect output contains" }
                        input { id: evidence_field_id(index, &kind, "expect"), "data-testid": "policy-evidence-command-expect-{index}", aria_invalid: if invalid_field == Some("expect") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("expect")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;", placeholder: "permitrootlogin no", value: "{evidence.expect}", oninput: move |event| set_ev_field!(expect, event.value()) }
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
                        input { id: evidence_field_id(index, &kind, "unit"), "data-testid": "policy-evidence-log-unit-{index}", aria_invalid: if invalid_field == Some("unit") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("unit")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;min-width:140px;", placeholder: "auditd.service", value: "{evidence.unit}", oninput: move |event| set_ev_field!(unit, event.value()) }
                    }
                    input { id: evidence_field_id(index, &kind, "match"), "data-testid": "policy-evidence-log-match-{index}", aria_invalid: if invalid_field == Some("match") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("match")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "regex / substring to match", value: "{match_value}", oninput: move |event| set_ev_field!(r#match, event.value()) }
                },
                "file" => rsx! {
                    input { id: evidence_field_id(index, &kind, "path"), "data-testid": "policy-evidence-file-path-{index}", aria_invalid: if invalid_field == Some("path") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("path")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "/etc/issue", value: "{evidence.path}", oninput: move |event| set_ev_field!(path, event.value()) }
                    input { class: "input focus-ring", style: "font-size:11px;padding:5px 8px;", placeholder: "What to look for / why it proves compliance", value: "{evidence.note}", oninput: move |event| set_ev_field!(note, event.value()) }
                },
                "unit_state" => rsx! {
                    div { style: "display:flex;gap:6px;align-items:center;flex-wrap:wrap;",
                        input { id: evidence_field_id(index, &kind, "unit"), "data-testid": "policy-evidence-unit-name-{index}", aria_invalid: if invalid_field == Some("unit") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("unit")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;flex:1;min-width:140px;", placeholder: "auditd.service", value: "{evidence.unit}", oninput: move |event| set_ev_field!(unit, event.value()) }
                        span { style: "font-size:11px;color:var(--cf-text-muted);", "is" }
                        select {
                            id: evidence_field_id(index, &kind, "state"),
                            "data-testid": "policy-evidence-unit-state-{index}",
                            aria_invalid: if invalid_field == Some("state") { "true" } else { "false" },
                            aria_describedby: (invalid_field == Some("state")).then_some(error_id.as_str()),
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
                    input { id: evidence_field_id(index, &kind, "attr"), "data-testid": "policy-evidence-eval-attr-{index}", aria_invalid: if invalid_field == Some("attr") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("attr")).then_some(error_id.as_str()), class: "input focus-ring mono", style: "font-size:11px;padding:5px 8px;", placeholder: "config.services.openssh.settings.PermitRootLogin", value: "{evidence.attr}", oninput: move |event| set_ev_field!(attr, event.value()) }
                    span { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);", "Captured from the evaluated config — no host access needed." }
                },
                "attestation" => rsx! {
                    input { id: evidence_field_id(index, &kind, "note"), "data-testid": "policy-evidence-attestation-note-{index}", aria_invalid: if invalid_field == Some("note") { "true" } else { "false" }, aria_describedby: (invalid_field == Some("note")).then_some(error_id.as_str()), class: "input focus-ring", style: "font-size:11px;padding:5px 8px;", placeholder: "What the agent attests to (signed snapshot)", value: "{evidence.note}", oninput: move |event| set_ev_field!(note, event.value()) }
                    span { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);", "Ed25519-signed by the agent at collection time." }
                },
                _ => rsx! { span { style: "font-style:italic;", "{kind}" } },
            }
            if let Some((_, error)) = validation.as_ref() {
                span { id: "{error_id}", class: "help cf-policy-evidence-error", role: "alert", "{error}" }
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
    fn evidence_validation_identifies_the_first_invalid_control() {
        let mut evidence = PolicyEvidence::new("command");
        evidence.cmd.clear();
        evidence.expect.clear();
        assert_eq!(
            evidence.validation_error(),
            Some(("cmd", "Command is required".to_string()))
        );

        evidence.cmd = "true".to_string();
        assert_eq!(
            evidence.validation_error(),
            Some(("expect", "Expected output is required".to_string()))
        );
        assert_eq!(
            evidence_field_id(2, "command", "expect"),
            "policy-evidence-command-expect-2"
        );
    }

    #[test]
    fn tab_navigation_wraps_and_excludes_unavailable_provenance() {
        assert_eq!(
            visible_policy_editor_tabs(false),
            &[
                PolicyEditorTab::Details,
                PolicyEditorTab::Enforcement,
                PolicyEditorTab::Mappings,
                PolicyEditorTab::Evidence,
            ]
        );
        assert_eq!(visible_policy_editor_tabs(true), &POLICY_EDITOR_TABS);
        assert_eq!(
            move_policy_editor_tab(
                PolicyEditorTab::Details,
                PolicyEditorTabMove::Previous,
                false,
            ),
            PolicyEditorTab::Evidence
        );
        assert_eq!(
            move_policy_editor_tab(PolicyEditorTab::Evidence, PolicyEditorTabMove::Next, false,),
            PolicyEditorTab::Details
        );
        assert_eq!(
            move_policy_editor_tab(PolicyEditorTab::Evidence, PolicyEditorTabMove::Next, true,),
            PolicyEditorTab::Provenance
        );
    }

    #[test]
    fn tab_navigation_home_and_end_follow_visual_order() {
        assert_eq!(
            move_policy_editor_tab(PolicyEditorTab::Mappings, PolicyEditorTabMove::First, true,),
            PolicyEditorTab::Details
        );
        assert_eq!(
            move_policy_editor_tab(PolicyEditorTab::Details, PolicyEditorTabMove::Last, false,),
            PolicyEditorTab::Evidence
        );
        assert_eq!(
            move_policy_editor_tab(PolicyEditorTab::Details, PolicyEditorTabMove::Last, true,),
            PolicyEditorTab::Provenance
        );
    }

    #[test]
    fn editor_layout_styles_override_generic_modal_and_preserve_narrow_tabs() {
        let css = include_str!("../../../assets/app.css");
        let desktop = css
            .split_once(".modal.cf-policy-editor-dialog {")
            .expect("editor modal selector")
            .1
            .split_once('}')
            .expect("editor modal rule")
            .0;
        assert!(desktop.contains("width: min(1180px, calc(100vw - 48px))"));

        let narrow = css
            .split_once("@media (max-width: 700px)")
            .expect("narrow editor breakpoint")
            .1;
        assert!(narrow.contains(".modal.cf-policy-editor-dialog {\n    width: 100%;"));
        assert!(narrow.contains(".cf-policy-editor-nav {"));
        assert!(narrow.contains("overflow-x: auto"));
        assert!(narrow.contains(".cf-policy-editor-nav .cf-modal-tab {\n    flex: none;\n    width: auto;\n    inline-size: auto;\n    max-width: none;\n    min-width: 0;\n    min-inline-size: max-content;"));
        assert!(narrow.contains(
            ".cf-policy-editor-nav .cf-modal-tab--active {\n    border-bottom-color: currentColor;"
        ));
        assert!(narrow.contains(".cf-policy-editor-nav-affordance {"));
        assert!(narrow.contains("display: flex"));
    }

    #[test]
    fn mapping_action_stays_in_scrolling_content_above_footer() {
        let source = include_str!("policy_editor_modal.rs");
        let css = include_str!("../../../assets/app.css");
        let mappings = source.find("cf-policy-mappings-tab").expect("mappings tab");
        let add_mapping = source
            .find("id: \"policy-mapping-add-trigger\"")
            .expect("add mapping action");
        let footer = source.find("// ── Footer").expect("editor footer");

        assert!(mappings < add_mapping && add_mapping < footer);
        assert!(
            css.contains(".cf-policy-mappings-tab {\n  min-height: 100%;\n  padding-bottom: 72px;")
        );
        assert!(css.contains(".cf-policy-mapping-add-trigger {\n  margin-bottom: 16px;\n  scroll-margin-bottom: 104px;"));
        assert!(source.contains("class: \"cf-policy-editor-nav-affordance\""));
        assert!(source.contains("\"Scroll sections →\""));
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
    fn hidden_unsupported_rules_block_save_instead_of_creating_noop_policy() {
        let rules = vec![
            PolicyRule::new("approval_required"),
            PolicyRule::new("rollout_percent"),
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
    fn mapped_without_enforcement_requires_known_nonempty_mappings() {
        let loaded = MappingLoadState::Loaded;
        assert!(!policy_is_mapped_not_enforced(loaded, 0, 0));
        assert!(!policy_is_mapped_not_enforced(loaded, 2, 1));
        assert!(policy_is_mapped_not_enforced(loaded, 2, 0));
    }

    #[test]
    fn editor_header_mapping_metadata_is_honest_during_every_load_state() {
        assert_eq!(
            mapping_header_metadata(MappingLoadState::Loading, 0),
            ("chip chip-unknown", "Mappings loading".to_string())
        );
        assert_eq!(
            mapping_header_metadata(MappingLoadState::Failed, 0),
            ("chip chip-critical", "Mappings unavailable".to_string())
        );
        assert_eq!(
            mapping_header_metadata(MappingLoadState::Loaded, 0),
            ("chip chip-unknown", "Unmapped".to_string())
        );
        assert_eq!(
            mapping_header_metadata(MappingLoadState::Loaded, 3),
            ("chip chip-info", "Mapped · 3".to_string())
        );
    }

    #[test]
    fn danger_zone_is_limited_to_editing_basics() {
        assert!(danger_zone_visible(true, PolicyEditorTab::Details));
        assert!(!danger_zone_visible(false, PolicyEditorTab::Details));
        assert!(!danger_zone_visible(true, PolicyEditorTab::Enforcement));
        assert!(!danger_zone_visible(true, PolicyEditorTab::Mappings));
        assert!(!danger_zone_visible(true, PolicyEditorTab::Evidence));
        assert!(!danger_zone_visible(true, PolicyEditorTab::Provenance));
    }

    #[test]
    fn mapping_load_failure_is_never_reported_as_unmapped() {
        assert!(
            !policy_is_mapped_not_enforced(MappingLoadState::Loading, 2, 0)
                && !policy_is_mapped_not_enforced(MappingLoadState::Failed, 2, 0),
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
    fn addable_rules_are_exactly_the_eight_complete_kinds() {
        let addable: Vec<&str> = RULE_OPTIONS
            .iter()
            .filter(|(_, _, persisted)| *persisted)
            .map(|(id, _, _)| *id)
            .collect();
        assert_eq!(
            addable,
            vec![
                "nixos_option",
                "packages_installed",
                "packages_absent",
                "custom_eval",
                "cve_block",
                "eval_passed",
                "pin_required",
                "time_window"
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
    fn incomplete_approval_and_rollout_rule_kinds_are_not_addable() {
        for kind in ["approval_required", "rollout_percent", "build_succeeded"] {
            assert!(
                !rule_kind_is_persisted(kind),
                "{kind} must not be persistable, so Add Rule must not offer it"
            );
        }
    }

    #[test]
    fn recommendations_are_suggestions_for_complete_kinds_only() {
        for category in POLICY_CATEGORIES {
            let actionable = actionable_recommended_enforcement(category);
            for kind in &actionable {
                assert!(
                    rule_kind_is_persisted(kind),
                    "{kind} is recommended for {category:?} but is not persistable"
                );
            }
        }

        let pipeline = actionable_recommended_enforcement(PolicyCategory::Pipeline);
        assert_eq!(pipeline, vec!["eval_passed", "pin_required", "cve_block"]);
        assert_eq!(
            actionable_recommended_enforcement(PolicyCategory::Rollout),
            vec!["time_window"]
        );
        assert!(!RULE_OPTIONS.iter().any(|(kind, _, _)| matches!(
            *kind,
            "approval_required" | "rollout_percent" | "build_succeeded"
        )));
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

        // Saving is never blocked merely because a complete rule is off-category.
        let representable = serde_json::json!({"mode": "all", "rules": []});
        assert!(
            save_blocker(
                true,
                PolicyFormat::Json,
                "custom_check",
                &representable,
                &rules
            )
            .is_none(),
            "time-window and custom rules must save regardless of category"
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
    fn all_eight_complete_rule_kinds_round_trip_and_remain_editable() {
        let mut option = PolicyRule::new("nixos_option");
        option.path = "networking.firewall.enable".into();
        option.option_type = "boolean".into();
        option.value = serde_json::json!(true);
        let mut installed = PolicyRule::new("packages_installed");
        installed.packages = "openssh, auditd".into();
        let mut absent = PolicyRule::new("packages_absent");
        absent.packages = "telnet, rsh".into();
        let mut custom = PolicyRule::new("custom_eval");
        custom.expr = "config.networking.firewall.enable".into();
        custom.message = "Firewall required".into();
        let mut cve = PolicyRule::new("cve_block");
        cve.severity = "high".into();
        cve.max_allowed = "3".into();
        let eval = PolicyRule::new("eval_passed");
        let pin = PolicyRule::new("pin_required");
        let mut window = PolicyRule::new("time_window");
        window.days = "mon,wed,fri".into();
        window.from = "22:30".into();
        window.to = "02:15".into();
        window.tz = "America/Los_Angeles".into();
        let rules = vec![option, installed, absent, custom, cve, eval, pin, window];
        let ids = rules.iter().map(|rule| rule.id).collect::<Vec<_>>();

        let (policy_type, config) = build_persisted_payload(&rules).expect("serialize");
        assert_eq!(policy_type, "composite");
        assert_eq!(
            config["rules"][2]["config"]["packages"],
            serde_json::json!(["telnet", "rsh"])
        );
        assert_eq!(config["rules"][5]["config"], serde_json::json!({}));
        assert_eq!(config["rules"][6]["config"], serde_json::json!({}));
        assert_eq!(config["rules"][7]["config"]["tz"], "America/Los_Angeles");

        let mut reopened = rules_from_policy(&policy_type, &config);
        assert_eq!(reopened, rules);
        assert_eq!(reopened.iter().map(|rule| rule.id).collect::<Vec<_>>(), ids);
        assert!(
            reopened
                .iter()
                .all(|rule| rule_validation_error(rule).is_none())
        );

        reopened[0].path = r#"environment.etc."issue".text"#.into();
        reopened[0].option_type = "lines".into();
        reopened[0].value = serde_json::json!("authorized users only");
        reopened[1].packages = "openssh, auditd, aide".into();
        reopened[2].packages = "telnet".into();
        reopened[3].expr = "config.networking.firewall.enable == true".into();
        reopened[3].message = "Firewall must remain enabled".into();
        reopened[4].severity = "critical".into();
        reopened[4].max_allowed = "0".into();
        reopened[7].days = "sat,sun".into();
        reopened[7].from = "01:00".into();
        reopened[7].to = "03:00".into();
        reopened[7].tz = "UTC".into();
        let (_, edited) = build_persisted_payload(&reopened).expect("edit serialization");
        assert_eq!(
            edited["rules"][0]["config"]["path"],
            r#"environment.etc."issue".text"#
        );
        assert_eq!(
            edited["rules"][1]["config"]["packages"],
            serde_json::json!(["openssh", "auditd", "aide"])
        );
        assert_eq!(
            edited["rules"][2]["config"]["packages"],
            serde_json::json!(["telnet"])
        );
        assert_eq!(edited["rules"][7]["config"]["tz"], "UTC");
        assert_eq!(
            edited["rules"]
                .as_array()
                .unwrap()
                .iter()
                .map(|rule| rule["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ids.iter().map(Uuid::to_string).collect::<Vec<_>>()
        );
        assert!(composite_config_is_representable(&edited));
    }

    #[test]
    fn option_path_and_time_window_validation_matches_server_contract() {
        for valid in [
            "networking.firewall.enable",
            r#"environment.etc."issue".text"#,
            r#"services."quoted\"name\\suffix".enable"#,
        ] {
            assert!(validate_nixos_option_path(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            ".services.foo",
            "services..foo",
            "services.foo.",
            "services. bad",
            r#"services."unterminated"#,
        ] {
            assert!(validate_nixos_option_path(invalid).is_err(), "{invalid}");
        }

        let mut window = PolicyRule::new("time_window");
        assert!(validate_time_window(&window).is_none());
        window.days = "mon,funday".into();
        assert_eq!(
            validate_time_window(&window).unwrap(),
            "Days must contain only mon, tue, wed, thu, fri, sat, or sun."
        );
        window.days = "mon".into();
        window.from = "25:00".into();
        assert_eq!(
            validate_time_window(&window).unwrap(),
            "Start time must be a valid 24-hour HH:MM value."
        );
        window.from = "09:00".into();
        window.tz = "Mars/Olympus".into();
        assert_eq!(
            validate_time_window(&window).unwrap(),
            "Timezone must be a valid IANA timezone: Mars/Olympus."
        );
    }

    #[test]
    fn package_and_custom_eval_authoring_validation_matches_client_contract() {
        let mut packages = PolicyRule::new("packages_installed");
        packages.packages = "openssl, libcap-ng_2.0+test".into();
        assert!(rule_validation_error(&packages).is_none());

        packages.packages = "-openssl".into();
        assert_eq!(
            rule_validation_error(&packages).unwrap(),
            "Package pnames must start with an ASCII letter or digit and contain only letters, digits, +, -, ., or _."
        );
        packages.packages = "a".repeat(256);
        assert_eq!(
            rule_validation_error(&packages).unwrap(),
            "Package pnames must be between 1 and 255 bytes."
        );
        packages.packages = "caf\u{e9}".into();
        assert!(rule_validation_error(&packages).is_some());

        let mut custom = PolicyRule::new("custom_eval");
        custom.expr = "config.services.openssh.enable && (true".into();
        assert_eq!(
            rule_validation_error(&custom).unwrap(),
            "Custom Nix expression has unclosed delimiters."
        );
        custom.expr = "config.example == \"unterminated".into();
        assert_eq!(
            rule_validation_error(&custom).unwrap(),
            "Custom Nix expression has an unterminated quoted string."
        );
        custom.expr = "x".repeat(MAX_CUSTOM_EVAL_EXPRESSION_BYTES + 1);
        assert!(
            rule_validation_error(&custom)
                .unwrap()
                .contains("16384-byte limit")
        );
        custom.expr = "config.services.openssh.enable # { ignored comment".into();
        assert!(rule_validation_error(&custom).is_none());
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

        let partial = serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [
                {"id": "10000000-0000-4000-8000-000000000003", "kind": "eval_passed", "config": {}},
                {"id": "10000000-0000-4000-8000-000000000004", "kind": "future_kind", "config": {"must_survive": true}}
            ]
        });
        assert_eq!(rules_from_policy("composite", &partial).len(), 1);
        assert!(existing_enforcement_is_opaque(
            PolicyFormat::Json,
            "composite",
            &partial
        ));
        assert!(
            save_blocker(
                true,
                PolicyFormat::Json,
                "composite",
                &partial,
                &rules_from_policy("composite", &partial)
            )
            .is_some()
        );
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
