//! Policy editor modal for creating and editing policy definitions.
//!
//! This modal mirrors the design example `PolicyFormModal`: a single unified
//! create/edit modal (no Basic/Advanced toggle and no raw JSON/TOML editor) with
//! metadata, category, severity, rationale, an assertions/gate-rules builder, an
//! evidence-for-ATO builder, and an edit-mode danger zone with typed-confirmation
//! delete.
//!
//! Backend reality: the deployment-policy API persists only name, description,
//! policy_type, config (JSON), and enabled. Rules that map onto the existing
//! `config` shapes (custom_check / require_packages / require_cve_check) are
//! persisted. Everything else in this modal (category, severity, rationale,
//! evidence, and rollout/approval/time-window rules) is shown per the design but
//! is NOT persisted yet; those sections are visibly flagged as UI-only. Backend
//! follow-up is tracked in TASK-340.3.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{
    create_deployment_policy, delete_deployment_policy, update_deployment_policy,
};
use crate::api::models::{CreateDeploymentPolicyRequest, UpdateDeploymentPolicyRequest};
use crate::views::policies_api;

use super::types::{PolicyDefinition, PolicyFormat};

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
}

const CATEGORIES: [(&str, &str, &str, &str, &str); 4] = [
    (
        "deployment",
        "Deployment",
        "#60a5fa",
        "deploy",
        "Base strategy — how and when a system picks up a new configuration.",
    ),
    (
        "pipeline",
        "Pipeline gates",
        "#a78bfa",
        "build",
        "Gates on pipeline output — eval, build, and CVE results must pass before promotion.",
    ),
    (
        "rollout",
        "Rollout control",
        "#fbbf24",
        "sync",
        "Govern the timing, approvals, and staging of a rollout.",
    ),
    (
        "security",
        "Security & hardening",
        "#f87171",
        "shield",
        "Config-level assertions — STIG / hardening controls a system must satisfy.",
    ),
];

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
    for rule in persistable {
        if let Some(value) = rule_to_custom_check_entry(rule) {
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

fn rule_to_custom_check_entry(rule: &PolicyRule) -> Option<serde_json::Value> {
    match rule.kind.as_str() {
        "custom_eval" => Some(serde_json::json!({
            "expression": rule.expr.trim(),
            "description": if rule.message.trim().is_empty() { "Custom rule failed" } else { rule.message.trim() },
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

fn custom_check_config_is_representable(config: &serde_json::Value) -> bool {
    let mode_ok = config
        .get("mode")
        .and_then(|value| value.as_str())
        .unwrap_or("all")
        == "all";
    let strict_ok = config
        .get("strict")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    if !mode_ok || !strict_ok {
        return false;
    }

    if let Some(entries) = config.get("rules").and_then(|value| value.as_array()) {
        return entries.iter().all(|entry| {
            entry
                .get("strict")
                .and_then(|value| value.as_bool())
                .unwrap_or(true)
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
    let seed_category = match existing_type.as_str() {
        "require_cve_check" => "pipeline",
        "require_packages" | "custom_check" => "security",
        _ => "deployment",
    };

    let mut category = use_signal(|| seed_category.to_string());
    let mut severity = use_signal(|| "medium".to_string());
    let mut rationale = use_signal(String::new);
    let mut rules = use_signal(|| seed_rules);
    let mut evidence: Signal<Vec<PolicyEvidence>> = use_signal(Vec::new);
    let mut add_rule_kind = use_signal(String::new);
    let mut add_evidence_kind = use_signal(String::new);

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
                                            let latest = policies_api::load_policies_with_fallback().await;
                                            policy_library.set(latest);
                                            on_close.call(());
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
                    div { class: "modal-body", style: "overflow-y:auto;",
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

                        // Category (UI-only / not persisted)
                        div { class: "field",
                            label {
                                "Category "
                                span { class: "cf-policy-ui-only-badge", "UI only — not persisted yet" }
                            }
                            div { role: "radiogroup", style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:8px;",
                                for (id, label, color, icon, blurb) in CATEGORIES {
                                    button {
                                        key: "{id}",
                                        r#type: "button",
                                        role: "radio",
                                        aria_checked: if category.read().as_str() == id { "true" } else { "false" },
                                        class: if category.read().as_str() == id { "cf-policy-category-card cf-policy-category-card-active focus-ring" } else { "cf-policy-category-card focus-ring" },
                                        style: "--cf-policy-category-color:{color};",
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

                        // Severity (UI-only / not persisted)
                        div { class: "field",
                            label {
                                "Severity "
                                span { class: "cf-policy-ui-only-badge", "UI only — not persisted yet" }
                            }
                            div { class: "seg seg-sev", role: "radiogroup", style: "width:fit-content;",
                                for (value, label, color) in [("high", "High (CAT I)", "#f87171"), ("medium", "Medium (CAT II)", "#fbbf24"), ("low", "Low (CAT III)", "#60a5fa")] {
                                    button {
                                        key: "{value}",
                                        r#type: "button",
                                        role: "radio",
                                        aria_checked: if severity.read().as_str() == value { "true" } else { "false" },
                                        class: if severity.read().as_str() == value { "active" } else { "" },
                                        style: if severity.read().as_str() == value {
                                            "color:{color};background:color-mix(in oklab, {color} 16%, transparent);box-shadow:inset 0 0 0 1px color-mix(in oklab, {color} 45%, transparent);"
                                        } else {
                                            "color:var(--cf-text-secondary);background:transparent;box-shadow:none;"
                                        },
                                        onclick: move |_| severity.set(value.to_string()),
                                        span { style: "display:inline-flex;align-items:center;gap:6px;",
                                            span { style: "width:7px;height:7px;border-radius:50%;background:{color};" }
                                            "{label}"
                                        }
                                    }
                                }
                            }
                            div { class: "help", "Drives how failures of this control are weighted in compliance scoring and evidence reports." }
                        }

                        // Rationale (UI-only / not persisted)
                        div { class: "field",
                            label {
                                "Rationale "
                                span { class: "cf-policy-ui-only-badge", "UI only — not persisted yet" }
                            }
                            textarea {
                                class: "input focus-ring",
                                rows: "2",
                                placeholder: "Why this policy exists — shown in detail view",
                                style: "resize:vertical;",
                                value: "{rationale}",
                                oninput: move |event| rationale.set(event.value()),
                            }
                        }

                        // Assertions & gate rules builder
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

                        // Evidence for ATO builder (UI-only / not persisted)
                        div { style: "margin-top:6px;",
                            div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:8px;",
                                label { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);",
                                    "Evidence for ATO ({evidence_count}) "
                                    span { class: "cf-policy-ui-only-badge", "UI only — not persisted yet" }
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
                            div { style: "margin-top:8px;",
                                select {
                                    class: "input focus-ring",
                                    style: "max-width:260px;font-size:12px;",
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

                                save_error.set(String::new());
                                is_saving.set(true);

                                spawn(async move {
                                    let result = if let Some(policy_id) = editing_id {
                                        let request = UpdateDeploymentPolicyRequest {
                                            name: Some(name.clone()),
                                            description: Some(description.clone()),
                                            policy_type: Some(policy_type),
                                            config: Some(config),
                                            enabled: None,
                                        };
                                        update_deployment_policy(&policy_id, &request).await.map(|_| ())
                                    } else {
                                        let request = CreateDeploymentPolicyRequest {
                                            name: name.clone(),
                                            description: Some(description.clone()),
                                            policy_type,
                                            config,
                                            enabled: Some(true),
                                        };
                                        create_deployment_policy(&request).await.map(|_| ())
                                    };

                                    match result {
                                        Ok(()) => {
                                            let latest = policies_api::load_policies_with_fallback().await;
                                            policy_library.set(latest);
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
                        class: "input focus-ring mono",
                        rows: "2",
                        style: "font-size:11px;padding:6px 8px;resize:vertical;",
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
}
