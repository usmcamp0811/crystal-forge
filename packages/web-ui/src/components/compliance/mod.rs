use dioxus::prelude::*;

use crate::api::models::{
    ComplianceBundleSummary, ComplianceControlEvidence, ComplianceControlStatus,
    ComplianceEvidenceResponse, ComplianceRollupTotals, ComplianceSystemRollup,
};
use crate::components::icon::{Icon, IconName};

pub mod refine_policy;
pub use refine_policy::{
    EvidenceRequirementDraft, ImportReview, PolicyAssertionDraft, RefinePolicyStep,
    RefinedPolicyDraft, RefinedRuleAction, RefinedStigRule, SourceCheck, SourceCheckBodyPart,
    SourceStigRule, action_to_import,
};

// ─── Bundle catalog left rail ────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct BundleCatalogProps {
    pub bundles: Vec<ComplianceBundleSummary>,
    pub selected_id: Option<uuid::Uuid>,
    pub on_select: EventHandler<uuid::Uuid>,
    #[props(default)]
    pub selected_version_id: Option<uuid::Uuid>,
    #[props(default)]
    pub on_select_version: EventHandler<uuid::Uuid>,
}

#[component]
pub fn BundleCatalog(props: BundleCatalogProps) -> Element {
    rsx! {
        div {
            class: "card",
            style: "padding:0;position:sticky;top:16px;max-height:calc(100vh - 160px);overflow:auto;",
            div {
                style: "padding:12px 14px;border-bottom:1px solid var(--cf-divider);font-size:11px;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);font-weight:600;",
                "Compliance bundles"
            }
            for bundle in props.bundles.iter() {
                {
                    let id = bundle.id;
                    let selected = props.selected_id == Some(id);
                    let env_count = bundle.environment_count;
                    let control_count = bundle.control_count;
                    let layer = bundle.layer.clone();
                    let framework = bundle.framework.clone();
                    let version = bundle.version.clone();
                    let name = bundle.name.clone();
                    let revisions = bundle.versions.clone();
                    rsx! {
                        button {
                            class: "focus-ring",
                            style: if selected {
                                "all:unset;cursor:pointer;display:block;padding:12px 14px;width:100%;box-sizing:border-box;border-left:3px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 8%,transparent);border-bottom:1px solid var(--cf-divider);"
                            } else {
                                "all:unset;cursor:pointer;display:block;padding:12px 14px;width:100%;box-sizing:border-box;border-left:3px solid transparent;background:transparent;border-bottom:1px solid var(--cf-divider);"
                            },
                            onclick: move |_| props.on_select.call(id),
                            div {
                                style: "display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:4px;",
                                span {
                                    style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);",
                                    "{name}"
                                }
                                span { class: "chip chip-unknown", style: "font-size:9px;padding:1px 6px;", "{layer}" }
                            }
                            div { style: "font-size:11px;color:var(--cf-text-muted);", "{framework} · {version}" }
                            div {
                                style: "font-size:11px;color:var(--cf-text-muted);margin-top:4px;",
                                "{control_count} controls · {env_count} env{env_count_suffix(env_count)}"
                                if revisions.len() > 1 { " · {revisions.len()} revisions" }
                            }
                            if revisions.len() > 1 && selected {
                                div { style: "display:flex;flex-direction:column;gap:4px;margin-top:8px;padding-top:8px;border-top:1px solid var(--cf-divider);",
                                    for revision in revisions.iter() {
                                        {
                                            let revision_id = revision.id;
                                            let version = revision.version.clone();
                                            let state = revision.publication_state.clone();
                                            let is_selected = props.selected_version_id == Some(revision_id);
                                            rsx! {
                                                button {
                                                    class: "focus-ring",
                                                    onclick: move |event| { event.stop_propagation(); props.on_select_version.call(revision_id); },
                                                    style: if is_selected { "all:unset;cursor:pointer;padding:5px 7px;border-radius:6px;background:color-mix(in oklab,var(--cf-brand-purple) 14%,transparent);border:1px solid var(--cf-brand-purple);font-size:10px;text-align:left;" } else { "all:unset;cursor:pointer;padding:5px 7px;border-radius:6px;background:var(--cf-subtle-bg);font-size:10px;text-align:left;" },
                                                    "{version} · {state}"
                                                    if revision.is_current_published { " · Current" }
                                                    if revision.is_current_draft { " · Draft" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn env_count_suffix(n: i64) -> &'static str {
    if n == 1 { "" } else { "s" }
}

// ─── Bundle header card ──────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct BundleHeaderProps {
    pub bundle: ComplianceBundleSummary,
    pub on_edit: EventHandler<()>,
    /// When false the Edit button is hidden — non-admin users get a read-only view.
    #[props(default = false)]
    pub is_admin: bool,
}

#[component]
pub fn BundleHeader(props: BundleHeaderProps) -> Element {
    let last_review = props
        .bundle
        .last_review
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "never".to_string());
    let description = props.bundle.description.clone().unwrap_or_default();
    let owner = props.bundle.owner.clone();
    let name = props.bundle.name.clone();
    let framework = props.bundle.framework.clone();
    let version = props.bundle.version.clone();
    let layer = props.bundle.layer.clone();

    rsx! {
        div {
            class: "card",
            style: "padding:18px;display:flex;flex-direction:column;gap:10px;",
            div {
                style: "display:flex;align-items:center;justify-content:space-between;gap:14px;flex-wrap:wrap;",
                div {
                    h2 { style: "margin:0;font-size:18px;font-weight:700;", "{name}" }
                    div {
                        style: "display:flex;gap:8px;margin-top:6px;align-items:center;flex-wrap:wrap;",
                        span { class: "chip chip-info", "{framework}" }
                        span { class: "chip chip-unknown", "{version}" }
                        span { class: "chip chip-unknown", "{layer}" }
                        span {
                            style: "font-size:11px;color:var(--cf-text-muted);",
                            "Owned by "
                            span { class: "mono", "{owner}" }
                            " · Last reviewed {last_review}"
                        }
                    }
                }
                div { style: "display:flex;gap:10px;align-items:center;flex-wrap:wrap;",
                    div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                        for env in props.bundle.required_envs.iter() {
                            {
                                let color = env.color_hex.clone();
                                let env_name = env.name.clone();
                                rsx! {
                                    span {
                                        style: "padding:4px 10px;border-radius:99px;font-size:11px;border:1px solid {color};background:color-mix(in oklab,{color} 14%,var(--cf-card-bg));color:{color};display:inline-flex;align-items:center;gap:6px;",
                                        span {
                                            style: "width:6px;height:6px;border-radius:50%;background:{color};",
                                        }
                                        "{env_name}"
                                    }
                                }
                            }
                        }
                    }
                    if props.is_admin {
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| props.on_edit.call(()),
                            Icon { name: IconName::Edit, size: 13 }
                            " Edit bundle"
                        }
                    }
                }
            }
            if !description.is_empty() {
                p { style: "margin:0;font-size:13px;color:var(--cf-text-secondary);line-height:1.5;", "{description}" }
            }
        }
    }
}

// ─── Score strip ─────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct ScoreStripProps {
    pub totals: ComplianceRollupTotals,
}

#[component]
pub fn ScoreStrip(props: ScoreStripProps) -> Element {
    let score = props.totals.overall_score;
    let score_color = if score >= 90 {
        "#34d399"
    } else if score >= 70 {
        "#fbbf24"
    } else {
        "#f87171"
    };

    rsx! {
        div { class: "stat-strip",
            // Overall score — wider stat with meta line
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:{score_color};" }
                div { class: "stat-label", "Overall score" }
                div { class: "stat-value", style: "color:{score_color};", "{score}%" }
                div { class: "stat-meta",
                    "{props.totals.fully_compliant_count} of {props.totals.system_count} hosts fully compliant"
                }
            }
            ScoreStat { label: "Pass",          value: props.totals.pass,          color: "#34d399" }
            ScoreStat { label: "Warn",          value: props.totals.warn,          color: "#fbbf24" }
            ScoreStat { label: "Fail",          value: props.totals.fail,          color: "#f87171" }
            ScoreStat { label: "Waiver",        value: props.totals.waiver,        color: "#a78bfa" }
            if props.totals.not_checked > 0 {
                ScoreStat { label: "Not checked",   value: props.totals.not_checked,   color: "#94a3b8" }
            }
            if props.totals.not_applicable > 0 {
                ScoreStat { label: "N/A",           value: props.totals.not_applicable, color: "#64748b" }
            }
            if props.totals.error > 0 {
                ScoreStat { label: "Error",         value: props.totals.error,          color: "#f43f5e" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ScoreStatProps {
    label: &'static str,
    value: i64,
    color: &'static str,
}

#[component]
fn ScoreStat(props: ScoreStatProps) -> Element {
    rsx! {
        div { class: "stat",
            span { class: "stat-accent", style: "--stat-color:{props.color};" }
            div { class: "stat-label", "{props.label}" }
            div { class: "stat-value", style: "color:{props.color};", "{props.value}" }
        }
    }
}

// ─── Systems matrix (BundleDrilldown) ────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct SystemsMatrixProps {
    pub systems: Vec<ComplianceSystemRollup>,
    pub on_evidence: EventHandler<uuid::Uuid>,
    pub filter: String,
    pub on_filter: EventHandler<String>,
}

#[component]
pub fn SystemsMatrix(props: SystemsMatrixProps) -> Element {
    let visible: Vec<_> = props
        .systems
        .iter()
        .filter(|row| match props.filter.as_str() {
            "fail" => row.fail > 0,
            "warn" => row.warn > 0 && row.fail == 0,
            "clean" => row.fail == 0 && row.warn == 0,
            _ => true,
        })
        .collect();

    rsx! {
        div { class: "card", style: "overflow:hidden;",
            // Header row: title + seg filter + host count
            div {
                style: "padding:12px 16px;border-bottom:1px solid var(--cf-divider);display:flex;gap:10px;align-items:center;flex-wrap:wrap;",
                h3 { style: "margin:0;font-size:13px;font-weight:600;", "Systems" }
                div { class: "seg",
                    for (v, l) in [("all","All"),("clean","Clean"),("warn","Warning"),("fail","Failing")] {
                        {
                            let v_str = v.to_string();
                            let is_active = props.filter == v;
                            rsx! {
                                button {
                                    class: if is_active { "active" } else { "" },
                                    onclick: move |_| props.on_filter.call(v_str.clone()),
                                    "{l}"
                                }
                            }
                        }
                    }
                }
                span { class: "filter-count", "{visible.len()} hosts" }
            }
            // Info callout
            div {
                class: "sd-callout sd-callout-info",
                style: "margin:12px 16px 0;",
                Icon { name: IconName::Shield, size: 13 }
                div {
                    style: "font-size:12px;",
                    "Select a host to step through its "
                    strong { "per-control evidence" }
                    " — the proof Crystal Forge collected that each control is satisfied."
                }
            }
            // Systems table
            table { class: "sys-table",
                thead { tr {
                    th { "Host" }
                    th { "Env" }
                    th { "Score" }
                    th { "Pass" }
                    th { "Warn" }
                    th { "Fail" }
                    th { "Waiver" }
                    th { style: "text-align:right;" }
                } }
                tbody {
                    for row in visible.iter() {
                        {
                            let system_id = row.system_id;
                            let hostname = row.hostname.clone();
                            let env = row.environment.clone().unwrap_or_else(|| "—".to_string());
                            let env_color = "#6b7280";
                            let score = row.score;
                            let score_color = if score >= 90 { "#34d399" } else if score >= 70 { "#fbbf24" } else { "#f87171" };
                            let pass = row.pass;
                            let warn = row.warn;
                            let fail = row.fail;
                            let waiver = row.waiver;
                            rsx! {
                                tr {
                                    style: "cursor:pointer;",
                                    onclick: move |_| props.on_evidence.call(system_id),
                                    td {
                                        div { style: "display:flex;align-items:center;gap:8px;",
                                            span {
                                                class: "status-dot",
                                                style: "--status-color:{score_color};",
                                            }
                                            span { class: "mono", style: "font-weight:600;font-size:13px;", "{hostname}" }
                                        }
                                    }
                                    td {
                                        span {
                                            style: "padding:2px 8px;border-radius:99px;font-size:11px;border:1px solid {env_color};background:color-mix(in oklab,{env_color} 14%,var(--cf-card-bg));color:{env_color};",
                                            "{env}"
                                        }
                                    }
                                    td {
                                        div { style: "display:flex;align-items:center;gap:8px;",
                                            div {
                                                style: "width:48px;height:5px;background:var(--cf-subtle-bg);border-radius:99px;overflow:hidden;",
                                                div {
                                                    style: "width:{score}%;height:100%;background:{score_color};",
                                                }
                                            }
                                            span { class: "mono", style: "font-size:12px;font-weight:600;color:{score_color};", "{score}%" }
                                        }
                                    }
                                    td { class: "mono", style: "color:#34d399;font-weight:600;", "{pass}" }
                                    td {
                                        class: "mono",
                                        style: if warn > 0 { "color:#fbbf24;font-weight:600;" } else { "color:var(--cf-text-muted);" },
                                        "{warn}"
                                    }
                                    td {
                                        class: "mono",
                                        style: if fail > 0 { "color:#f87171;font-weight:700;" } else { "color:var(--cf-text-muted);" },
                                        "{fail}"
                                    }
                                    td {
                                        class: "mono",
                                        style: if waiver > 0 { "color:#a78bfa;" } else { "color:var(--cf-text-muted);" },
                                        "{waiver}"
                                    }
                                    td { style: "text-align:right;",
                                        button {
                                            class: "btn btn-ghost focus-ring xs",
                                            onclick: move |e| { e.stop_propagation(); props.on_evidence.call(system_id); },
                                            "View evidence "
                                            Icon { name: IconName::ArrowRight, size: 11 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Controls evidence drawer ────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct EvidenceDrawerProps {
    pub evidence: ComplianceEvidenceResponse,
    pub bundle_name: String,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn EvidenceDrawer(props: EvidenceDrawerProps) -> Element {
    let mut active_idx = use_signal(|| 0usize);
    let total = props.evidence.controls.len();
    let hostname = props.evidence.hostname.clone();
    let bundle_name = props.bundle_name.clone();

    let active_control = props.evidence.controls.get(*active_idx.read()).cloned();

    rsx! {
        div { class: "fl-tray-backdrop", onclick: move |_| props.on_close.call(()) }
        aside {
            class: "fl-tray",
            style: "width:min(960px,96vw);",
            header {
                class: "fl-tray-head",
                div {
                    style: "display:flex;align-items:center;gap:12px;min-width:0;flex:1;",
                    span { style: "color:var(--cf-brand-purple);flex-shrink:0;display:inline-flex;",
                        Icon { name: IconName::Shield, size: 18 }
                    }
                    div { style: "min-width:0;",
                        div {
                            style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                            span { class: "mono", style: "font-weight:700;font-size:15px;", "{hostname}" }
                            span { style: "font-size:11px;color:var(--cf-text-muted);", "vs" }
                            span { class: "chip chip-info", "{bundle_name}" }
                        }
                        div {
                            style: "font-size:11px;color:var(--cf-text-muted);margin-top:2px;",
                            "Stepping through {total} controls · use "
                            kbd { class: "kbd", "j" }
                            "/"
                            kbd { class: "kbd", "k" }
                            " to navigate"
                        }
                    }
                }
                div { style: "display:flex;gap:6px;",
                    button {
                        class: "btn-icon focus-ring",
                        onclick: move |_| props.on_close.call(()),
                        Icon { name: IconName::X, size: 16 }
                    }
                }
            }

            div {
                style: "display:grid;grid-template-columns:260px 1fr;flex:1;min-height:0;overflow:hidden;",
                // Left: control nav
                nav {
                    style: "border-right:1px solid var(--cf-divider);overflow-y:auto;background:color-mix(in oklab,var(--cf-page-bg) 30%,var(--cf-card-bg));",
                    for (i, control) in props.evidence.controls.iter().enumerate() {
                        {
                            let is_sel = i == *active_idx.read();
                            let dot_color = control_status_color(&control.status);
                            let policy_name = control.policy_name.clone();
                            rsx! {
                                button {
                                    class: "focus-ring",
                                    style: if is_sel {
                                        "all:unset;cursor:pointer;display:block;padding:10px 14px;width:100%;box-sizing:border-box;border-left:3px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 8%,transparent);border-bottom:1px solid var(--cf-divider);"
                                    } else {
                                        "all:unset;cursor:pointer;display:block;padding:10px 14px;width:100%;box-sizing:border-box;border-left:3px solid transparent;background:transparent;border-bottom:1px solid var(--cf-divider);"
                                    },
                                    onclick: move |_| active_idx.set(i),
                                    div {
                                        style: "display:flex;justify-content:space-between;align-items:center;gap:8px;",
                                        span { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "{i+1:02}" }
                                        span { style: "width:8px;height:8px;border-radius:50%;background:{dot_color};" }
                                    }
                                    div {
                                        style: if is_sel {
                                            "font-size:12px;color:var(--cf-text-primary);margin-top:4px;font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;"
                                        } else {
                                            "font-size:12px;color:var(--cf-text-primary);margin-top:4px;font-weight:400;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;"
                                        },
                                        "{policy_name}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Right: evidence detail
                div {
                    style: "overflow:auto;padding:20px;display:flex;flex-direction:column;gap:16px;",
                    if let Some(ctrl) = active_control {
                        ControlEvidenceCard {
                            control: ctrl,
                            control_idx: *active_idx.read(),
                            total,
                        }
                    }
                }
            }
        }
    }
}

fn control_status_color(status: &ComplianceControlStatus) -> &'static str {
    match status {
        ComplianceControlStatus::Pass => "#34d399",
        ComplianceControlStatus::Warn => "#fbbf24",
        ComplianceControlStatus::Fail => "#f87171",
        ComplianceControlStatus::Waiver => "#a78bfa",
        ComplianceControlStatus::NotChecked => "#94a3b8",
        ComplianceControlStatus::NotApplicable => "#64748b",
        ComplianceControlStatus::Error => "#f43f5e",
    }
}

#[derive(Props, Clone, PartialEq)]
struct ControlEvidenceCardProps {
    control: ComplianceControlEvidence,
    control_idx: usize,
    total: usize,
}

#[component]
fn ControlEvidenceCard(props: ControlEvidenceCardProps) -> Element {
    let sc = control_status_color(&props.control.status);
    let sev_color = match props.control.severity.as_str() {
        "high" => "#f87171",
        "medium" => "#fbbf24",
        _ => "#60a5fa",
    };
    let status_label = match props.control.status {
        ComplianceControlStatus::Pass => "pass",
        ComplianceControlStatus::Warn => "warn",
        ComplianceControlStatus::Fail => "fail",
        ComplianceControlStatus::Waiver => "waiver",
        ComplianceControlStatus::NotChecked => "not checked",
        ComplianceControlStatus::NotApplicable => "not applicable",
        ComplianceControlStatus::Error => "error",
    };
    let policy_name = props.control.policy_name.clone();
    let summary = props.control.summary.clone();
    let framework_mapping = props.control.framework_mapping.clone();
    let evidence_count = props.control.evidence_items.len();
    let evidence_plural = if evidence_count == 1 { "" } else { "s" };

    rsx! {
        // Header: control # / status / severity
        div {
            div {
                style: "display:flex;align-items:center;gap:10px;flex-wrap:wrap;margin-bottom:8px;",
                span { style: "font-size:11px;color:var(--cf-text-muted);", "Control {props.control_idx + 1} of {props.total}" }
                span {
                    class: "chip",
                    style: "color:{sc};background:color-mix(in oklab,{sc} 14%,transparent);border:1px solid {sc};",
                    "{status_label}"
                }
                span {
                    class: "chip",
                    style: "color:{sev_color};background:color-mix(in oklab,{sev_color} 14%,transparent);",
                    "{props.control.severity} severity"
                }
            }
            h2 { class: "mono", style: "margin:0;font-size:18px;font-weight:700;", "{policy_name}" }
            p { style: "margin:6px 0 0;font-size:13px;color:var(--cf-text-secondary);line-height:1.5;", "{summary}" }
        }

        // Status callout
        match props.control.status {
            ComplianceControlStatus::Fail => rsx! {
                div { class: "sd-callout sd-callout-danger",
                    Icon { name: IconName::X, size: 13 }
                    div { style: "font-size:12px;",
                        strong { "Not compliant. " }
                        "The required configuration is not applied on this host."
                    }
                }
            },
            ComplianceControlStatus::Warn => rsx! {
                div { class: "sd-callout sd-callout-warn",
                    Icon { name: IconName::Warn, size: 13 }
                    div { style: "font-size:12px;",
                        strong { "Compliant with warnings. " }
                        "Auditor may request additional evidence."
                    }
                }
            },
            ComplianceControlStatus::Waiver => rsx! {
                div {
                    class: "sd-callout",
                    style: "background:rgba(167,139,250,0.08);border-color:rgba(167,139,250,0.25);",
                    span { style: "color:#a78bfa;display:inline-flex;", Icon { name: IconName::File, size: 13 } }
                    div { style: "font-size:12px;",
                        strong { "Waiver in effect. " }
                        "Risk accepted with compensating control. See evidence below."
                    }
                }
            },
            ComplianceControlStatus::Pass => rsx! {},
            ComplianceControlStatus::NotChecked => rsx! {
                div { class: "sd-callout",
                    style: "background:rgba(148,163,184,0.08);border-color:rgba(148,163,184,0.25);",
                    div { style: "font-size:12px;color:var(--cf-text-muted);",
                        strong { "Not checked. " }
                        "No applicable evaluation or evidence exists for this control."
                    }
                }
            },
            ComplianceControlStatus::NotApplicable => rsx! {
                div { class: "sd-callout",
                    style: "background:rgba(100,116,139,0.08);border-color:rgba(100,116,139,0.25);",
                    div { style: "font-size:12px;color:var(--cf-text-muted);",
                        strong { "Not applicable. " }
                        "This control does not apply to the current system configuration."
                    }
                }
            },
            ComplianceControlStatus::Error => rsx! {
                div { class: "sd-callout sd-callout-danger",
                    div { style: "font-size:12px;",
                        strong { "Evaluator error. " }
                        "The control could not be evaluated. Check system logs for details."
                    }
                }
            },
        }

        // Evidence items
        div {
            h3 {
                style: "font-size:11px;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);margin:0 0 8px;font-weight:600;",
                "Evidence · {evidence_count} item{evidence_plural}"
            }
            div { style: "display:flex;flex-direction:column;gap:10px;",
                for item in props.control.evidence_items.iter() {
                    {
                        let label = item.label.clone();
                        let body = item.body.clone();
                        let artifact_title = item.artifact.as_ref().map(|a| a.title.clone());
                        let artifact_body = item.artifact.as_ref().map(|a| a.body.clone());
                        rsx! {
                            div { class: "ev-item",
                                div { class: "ev-item-head",
                                    span { style: "color:var(--cf-brand-purple);flex-shrink:0;display:inline-flex;",
                                        Icon { name: IconName::File, size: 14 }
                                    }
                                    div { style: "min-width:0;flex:1;",
                                        div { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "{label}" }
                                        div { class: "mono", style: "font-size:11px;color:var(--cf-text-secondary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;", "{body}" }
                                    }
                                }
                                if let (Some(title), Some(art_body)) = (artifact_title, artifact_body) {
                                    div {
                                        class: "ev-art ev-art-terminal",
                                        style: "border-top:1px solid var(--cf-divider);",
                                        div {
                                            class: "ev-art-bar",
                                            style: "display:flex;align-items:center;gap:6px;padding:6px 10px;font-size:11px;color:var(--cf-text-muted);",
                                            Icon { name: IconName::File, size: 11 }
                                            span { class: "ev-art-title", "{title}" }
                                        }
                                        pre {
                                            class: "ev-art-body ev-art-body-terminal",
                                            style: "margin:0;padding:10px 12px;font-size:11px;",
                                            "{art_body}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Framework mapping
        div {
            style: "padding:12px;background:var(--cf-subtle-bg);border-radius:8px;font-size:11px;color:var(--cf-text-secondary);",
            strong { style: "color:var(--cf-text-primary);", "Framework mapping" }
            span { style: "margin-left:8px;", "—" }
            span { class: "mono", style: "margin-left:8px;", "{framework_mapping}" }
        }
    }
}
