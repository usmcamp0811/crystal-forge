//! CrystalForgelatest-style Environments cards and table.

use dioxus::prelude::*;

use super::{EnvironmentDeploymentPolicy, EnvironmentHealthBreakdown, EnvironmentItem, PolicyOption};
use crate::components::icon::{Icon, IconName};

#[derive(Props, Clone, PartialEq)]
pub struct EnvironmentCardProps {
    pub environment: EnvironmentItem,
    pub policy_library: Vec<PolicyOption>,
    pub on_edit: EventHandler<EnvironmentItem>,
    /// Whether the card should show the attention-flash pulse animation.
    pub flash: bool,
    /// Persistent attention-row class(es) — e.g. "attention-row".
    /// Applied alongside the flash class to keep the card highlighted.
    #[props(default)]
    pub attention_class: String,
    pub on_view: EventHandler<EnvironmentItem>,
}

#[derive(Props, Clone, PartialEq)]
pub struct EnvironmentTableProps {
    pub environments: Vec<EnvironmentItem>,
    pub policy_library: Vec<PolicyOption>,
    pub on_edit: EventHandler<EnvironmentItem>,
    /// Per-item flash booleans, one per environment in the same order.
    pub flashes: Vec<bool>,
    /// Per-item attention class strings, one per environment in the same order.
    #[props(default)]
    pub attention_classes: Vec<String>,
    pub on_view: EventHandler<EnvironmentItem>,
}

#[component]
pub fn EnvironmentCard(props: EnvironmentCardProps) -> Element {
    let env = props.environment.clone();
    let env_for_header = env.clone();
    let env_for_body = env.clone();
    let display_policy = env.default_policy;
    let display_auto_sync = env.auto_sync;
    let display_requires_approval = env.requires_approval;
    let display_role_assignment_count = env.role_assignment_count;
    let total = env.health.total().max(env.system_count).max(1);

    let card_class = if props.flash {
        if props.attention_class.is_empty() {
            "env-card attention-flash".to_string()
        } else {
            format!("env-card attention-flash {}", props.attention_class)
        }
    } else if props.attention_class.is_empty() {
        "env-card".to_string()
    } else {
        format!("env-card {}", props.attention_class)
    };

    rsx! {
        div {
            class: "{card_class}",
            style: "cursor:pointer;",
            onclick: move |_| props.on_view.call(env_for_body.clone()),
            div { class: "env-card-rail", style: "background:{env.color_hex};" }
            div { class: "env-card-head",
                div {
                    div { class: "env-card-title",
                        span { class: "env-dot", style: "background:{env.color_hex};" }
                        span { "{env.name}" }
                        if env.is_production.unwrap_or(false) {
                            span { class: "env-prod-badge", Icon { name: IconName::Shield, size: 9 } " PROD" }
                        }
                        // Persistent "needs attention" indicator (TASK-385 follow-up).
                        // The one-shot attention-flash pulse alone wasn't enough to
                        // identify WHICH environment(s) triggered the sidebar badge
                        // once the flash had already fired/faded, so this stays
                        // visible for as long as the condition holds.
                        if env.health.critical > 0 {
                            span {
                                class: "chip chip-critical",
                                title: "{env.health.critical} system(s) reporting critical health",
                                "{env.health.critical} critical"
                            }
                        }
                        if env.health.offline > 0 {
                            span {
                                class: "chip chip-critical",
                                title: "{env.health.offline} system(s) offline",
                                "{env.health.offline} offline"
                            }
                        }
                    }
                    if let Some(description) = env.description.clone() {
                        div { class: "env-card-desc", "{description}" }
                    }
                }
                div { style: "display:flex; gap:4px;",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Edit",
                        onclick: move |e| {
                            e.stop_propagation();
                            props.on_edit.call(env_for_header.clone());
                        },
                        Icon { name: IconName::Gear, size: 14 }
                    }
                }
            }

            div { class: "env-card-stat",
                div { class: "env-card-stat-num", "{env.system_count}" }
                div { class: "env-card-stat-label", "systems" }
                div { style: "flex:1;" }
                div { class: "env-card-flakes",
                    if env.flake_names.is_empty() {
                        span { class: "chip chip-unknown", style: "font-size:10px;", "no flakes" }
                    } else {
                        for flake in env.flake_names.iter().take(3) {
                            span { class: "chip chip-unknown mono", style: "font-size:10px;", "{flake}" }
                        }
                        if env.flake_names.len() > 3 {
                            span { class: "chip chip-unknown", style: "font-size:10px;", "+{env.flake_names.len() - 3}" }
                        }
                    }
                }
            }

            HealthBar { health: env.health.clone(), total }
            HealthLegend { health: env.health.clone(), cve_critical_high: env.cve_critical_high }

            dl { class: "env-kv",
                dt { "Deploy" }
                dd { PolicyChip { policy: display_policy } }
                dt { "Enforcement" }
                dd { EnforcementChips { environment: env.clone(), policy_library: props.policy_library.clone() } }
                dt { "Cache" }
                dd { CacheSummary { environment: env.clone() } }
                dt { "Auto-sync" }
                dd { ToggleChip { enabled: display_auto_sync, on_label: "on", off_label: "off" } }
                dt { "Approval" }
                dd { ToggleChip { enabled: display_requires_approval, on_label: "required", off_label: "not required" } }
            }

            div { class: "env-card-foot",
                if let Some(count) = display_role_assignment_count {
                    span { style: "font-size:11px; color:var(--cf-text-muted);", "{count} role assignments" }
                } else {
                    span { style: "font-size:11px; color:var(--cf-text-muted);", "no role assignments" }
                }
            }
        }
    }
}

#[component]
pub fn EnvironmentTable(props: EnvironmentTableProps) -> Element {
    rsx! {
        div { class: "card", style: "overflow:hidden;",
            table { class: "sys-table",
                thead {
                    tr {
                        th { "Environment" }
                        th { "Systems" }
                        th { "Health" }
                        th { "Deploy" }
                        th { "Enforcement" }
                        th { "Cache" }
                        th { "Auto-sync" }
                        th { "Approval" }
                        th { style: "text-align:right;", " " }
                    }
                }
                tbody {
                    for (env, flash, attention_class) in props
                        .environments
                        .iter()
                        .zip(props.flashes.iter())
                        .zip(props.attention_classes.iter().chain(std::iter::repeat(&String::new())))
                        .map(|((e, f), a)| (e, f, a))
                    {
                        EnvironmentRow {
                            key: "{env.id}",
                            environment: env.clone(),
                            policy_library: props.policy_library.clone(),
                            on_edit: props.on_edit,
                            flash: *flash,
                            attention_class: attention_class.clone(),
                            on_view: props.on_view,
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EnvironmentRowProps {
    environment: EnvironmentItem,
    policy_library: Vec<PolicyOption>,
    on_edit: EventHandler<EnvironmentItem>,
    flash: bool,
    #[props(default)]
    attention_class: String,
    on_view: EventHandler<EnvironmentItem>,
}

#[component]
fn EnvironmentRow(props: EnvironmentRowProps) -> Element {
    let env = props.environment.clone();
    let env_for_row = env.clone();
    let env_for_button = env.clone();
    let display_policy = env.default_policy;
    let display_auto_sync = env.auto_sync;
    let display_requires_approval = env.requires_approval;
    let total = env.health.total().max(env.system_count).max(1);

    let row_class = if props.flash {
        if props.attention_class.is_empty() {
            "attention-flash".to_string()
        } else {
            format!("attention-flash {}", props.attention_class)
        }
    } else {
        props.attention_class.clone()
    };

    rsx! {
        tr {
            class: "{row_class}",
            style: "cursor:pointer;",
            onclick: move |_| props.on_view.call(env_for_row.clone()),
            td {
                div { style: "display:flex; align-items:center; gap:8px;",
                    span { class: "env-dot", style: "background:{env.color_hex};" }
                    div {
                        div { class: "mono", style: "font-weight:600; font-size:13px; display:flex; align-items:center; gap:7px;",
                            "{env.name}"
                            if env.is_production.unwrap_or(false) {
                                span { class: "env-prod-badge", Icon { name: IconName::Shield, size: 9 } " PROD" }
                            }
                            // Persistent "needs attention" indicator — see EnvironmentCard.
                            if env.health.critical > 0 {
                                span {
                                    class: "chip chip-critical",
                                    style: "font-size:10px;",
                                    title: "{env.health.critical} system(s) reporting critical health",
                                    "{env.health.critical} critical"
                                }
                            }
                            if env.health.offline > 0 {
                                span {
                                    class: "chip chip-critical",
                                    style: "font-size:10px;",
                                    title: "{env.health.offline} system(s) offline",
                                    "{env.health.offline} offline"
                                }
                            }
                        }
                        if let Some(description) = env.description.clone() {
                            div { style: "font-size:11px; color:var(--cf-text-muted);", "{description}" }
                        }
                    }
                }
            }
            td { class: "mono", style: "font-size:13px;", "{env.system_count}" }
            td {
                div { style: "display:flex; align-items:center; gap:6px; min-width:140px;",
                    HealthBar { health: env.health.clone(), total, compact: true }
                    span { class: "mono", style: "font-size:11px; color:var(--cf-text-muted);", "{env.health.healthy}/{env.system_count}" }
                }
            }
            td { PolicyChip { policy: display_policy } }
            td { EnforcementChips { environment: env.clone(), policy_library: props.policy_library.clone(), compact: true } }
            td { CacheSummary { environment: env.clone(), compact: true } }
            td { ToggleChip { enabled: display_auto_sync, on_label: "on", off_label: "off" } }
            td { ToggleChip { enabled: display_requires_approval, on_label: "required", off_label: "not required" } }
            td {
                div { class: "row-actions",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Edit",
                        onclick: move |e| {
                            e.stop_propagation();
                            props.on_edit.call(env_for_button.clone());
                        },
                        Icon { name: IconName::Gear, size: 14 }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct HealthBarProps {
    health: EnvironmentHealthBreakdown,
    total: usize,
    #[props(default = false)]
    compact: bool,
}

#[component]
fn HealthBar(props: HealthBarProps) -> Element {
    let total = props.total.max(1) as f64;
    let class = if props.compact {
        "env-health-bar compact"
    } else {
        "env-health-bar"
    };
    rsx! {
        div { class,
            if props.health.healthy > 0 { div { style: "width:{pct(props.health.healthy, total)}%; background:#34d399;", title: "{props.health.healthy} healthy" } }
            if props.health.warning > 0 { div { style: "width:{pct(props.health.warning, total)}%; background:#fbbf24;", title: "{props.health.warning} warning" } }
            if props.health.critical > 0 { div { style: "width:{pct(props.health.critical, total)}%; background:#f87171;", title: "{props.health.critical} critical" } }
            if props.health.offline > 0 { div { style: "width:{pct(props.health.offline, total)}%; background:#6b7280;", title: "{props.health.offline} offline" } }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct HealthLegendProps {
    health: EnvironmentHealthBreakdown,
    cve_critical_high: usize,
}

#[component]
fn HealthLegend(props: HealthLegendProps) -> Element {
    rsx! {
        div { class: "env-health-legend",
            if props.health.healthy > 0 { span { span { class: "env-health-sw", style: "background:#34d399;" } "{props.health.healthy}" } }
            if props.health.warning > 0 { span { span { class: "env-health-sw", style: "background:#fbbf24;" } "{props.health.warning}" } }
            if props.health.critical > 0 { span { span { class: "env-health-sw", style: "background:#f87171;" } "{props.health.critical}" } }
            if props.health.offline > 0 { span { span { class: "env-health-sw", style: "background:#6b7280;" } "{props.health.offline}" } }
            if props.cve_critical_high > 0 {
                span { style: "margin-left:auto;", Icon { name: IconName::Shield, size: 10 } " {props.cve_critical_high} CVE" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PolicyChipProps {
    policy: Option<EnvironmentDeploymentPolicy>,
}

#[component]
fn PolicyChip(props: PolicyChipProps) -> Element {
    if let Some(policy) = props.policy {
        let class = match policy {
            EnvironmentDeploymentPolicy::Manual | EnvironmentDeploymentPolicy::Pinned => {
                "chip chip-warning"
            }
            EnvironmentDeploymentPolicy::AutoLatest => "chip chip-healthy",
        };
        rsx! { span { class, "{policy.label()}" } }
    } else {
        rsx! { span { class: "chip chip-unknown", "not set" } }
    }
}

#[derive(Props, Clone, PartialEq)]
struct EnforcementChipsProps {
    environment: EnvironmentItem,
    policy_library: Vec<PolicyOption>,
    #[props(default = false)]
    compact: bool,
}

#[component]
fn EnforcementChips(props: EnforcementChipsProps) -> Element {
    let env = props.environment;
    let policy_count = env.required_policy_ids.len();
    let compliance_label = env
        .compliance_bundle
        .as_ref()
        .map(|bundle| bundle.framework.clone());
    rsx! {
        div { style: "display:flex; gap:6px; align-items:center; flex-wrap:wrap;",
            if let Some(label) = compliance_label.clone() {
                span { class: "chip chip-info", title: "Compliance bundle assigned to this environment", Icon { name: IconName::Shield, size: 9 } " {label}" }
            }
            if policy_count > 0 {
                span { class: "chip chip-unknown", title: "Required deployment policies (gates) for this environment", "{policy_count} gate{plural(policy_count)}" }
            }
            if compliance_label.is_none() && policy_count == 0 {
                span { style: "font-size:11px; color:var(--cf-text-muted);", if props.compact { "—" } else { "none" } }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct CacheSummaryProps {
    environment: EnvironmentItem,
    #[props(default = false)]
    compact: bool,
}

#[component]
fn CacheSummary(props: CacheSummaryProps) -> Element {
    let env = props.environment;
    rsx! {
        if let Some(cache) = env.cache {
            span {
                class: "mono truncate",
                style: "font-size:11px;",
                title: "{cache.url} ({cache.status})",
                if !props.compact { Icon { name: IconName::Download, size: 10 } " " }
                "{cache.url}"
            }
        } else {
            span { style: "font-size:11px; color:var(--cf-text-muted); font-style:italic;", if props.compact { "none" } else { "not configured" } }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ToggleChipProps {
    enabled: Option<bool>,
    on_label: &'static str,
    off_label: &'static str,
}

#[component]
fn ToggleChip(props: ToggleChipProps) -> Element {
    if let Some(enabled) = props.enabled {
        if enabled {
            rsx! { span { class: "chip chip-healthy", "{props.on_label}" } }
        } else {
            rsx! { span { class: "chip chip-unknown", "{props.off_label}" } }
        }
    } else {
        rsx! { span { class: "chip chip-unknown", "not set" } }
    }
}

fn pct(count: usize, total: f64) -> i32 {
    ((count as f64 / total) * 100.0).round() as i32
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}
