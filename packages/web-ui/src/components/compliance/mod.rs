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
    SourceStigRule, action_to_import, mapping_semantics_for,
};

// ─── Bundle catalog table ────────────────────────────────────────────────────

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
    let mut query = use_signal(String::new);
    let mut framework = use_signal(|| "all".to_string());
    let query_value = query.read().trim().to_ascii_lowercase();
    let frameworks = {
        let mut counts = std::collections::BTreeMap::<String, usize>::new();
        for bundle in props.bundles.iter() {
            *counts.entry(bundle.framework.clone()).or_default() += 1;
        }
        let mut values: Vec<_> = counts.into_iter().collect();
        values.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        values
    };
    let active_framework = framework.read().clone();
    let visible: Vec<_> = props
        .bundles
        .iter()
        .filter(|bundle| {
            (active_framework == "all" || bundle.framework == active_framework)
                && (query_value.is_empty()
                    || bundle.name.to_ascii_lowercase().contains(&query_value)
                    || bundle.framework.to_ascii_lowercase().contains(&query_value)
                    || bundle.version.to_ascii_lowercase().contains(&query_value))
        })
        .collect();

    rsx! {
            div {
                class: "card",
                style: "padding:0;overflow:hidden;",
                div { style: "padding:10px 16px;border-bottom:1px solid var(--cf-card-border);display:flex;flex-direction:column;gap:10px;",
                    div { style: "display:flex;gap:6px;flex-wrap:nowrap;overflow-x:auto;",
                        button { class: if active_framework == "all" { "cf-fw-chip active" } else { "cf-fw-chip" }, onclick: move |_| framework.set("all".to_string()), "All ", span { "{props.bundles.len()}" } }
                        for (name, count) in frameworks.iter() {
                            {
                                let name = name.clone();
                                let active = active_framework == name;
                                rsx! { button { class: if active { "cf-fw-chip active" } else { "cf-fw-chip" }, onclick: move |_| framework.set(name.clone()), "{name} ", span { "{count}" } } }
                            }
                        }
                    }
                    div { class: "q-search", style: "margin-left:0;width:100%;box-sizing:border-box;",
                        Icon { name: IconName::Search, size: 13 }
                        input { class: "q-search-input", placeholder: "Search bundles…", value: "{query}", oninput: move |event| query.set(event.value()) }
                        if !query.read().is_empty() {
                            span { class: "q-search-count", "{visible.len()} of {props.bundles.len()}" }
                            button { class: "btn-icon xs focus-ring", title: "Clear search", onclick: move |_| query.set(String::new()), Icon { name: IconName::X, size: 13 } }
                        }
                    }
                }
                if visible.is_empty() {
                    div { class: "q-empty", Icon { name: IconName::Search, size: 20 }, div { "No bundles match “{query}”." } }
                } else {
                table { class: "sys-table sys-table-fixed",
                    colgroup {
                        col { style: "width:38%;" }
                        col { style: "width:16%;" }
                        col { style: "width:18%;" }
                        col { style: "width:18%;" }
                        col { style: "width:10%;" }
                    }
                    thead { tr { th { "Bundle" } th { "Framework" } th { "Version" } th { "Score" } th { "" } } }
                    tbody {
                for bundle in visible.iter() {
                    {
                        let id = bundle.id;
                        let selected = props.selected_id == Some(id);
                        let framework = bundle.framework.clone();
                        let version = bundle.version.clone();
                        let name = bundle.name.clone();
                        let revisions = bundle.versions.clone();
                        let score = bundle.aggregate_score;
                        let score_color = score.map_or("var(--cf-text-muted)", |score| if score >= 90 { "#34d399" } else if score >= 70 { "#fbbf24" } else { "#f87171" });
                        let score_label = score.map_or_else(|| "—".to_string(), |score| format!("{score}%"));
                        let system_count_label = format!("{} system{}", bundle.applicable_system_count, if bundle.applicable_system_count == 1 { "" } else { "s" });
                        rsx! {
                            tr { class: if selected { "selected" } else { "" }, onclick: move |_| props.on_select.call(id),
                                td {
                                    div { style: "display:flex;align-items:center;gap:8px;min-width:0;",
                                        span { style: "width:7px;height:7px;border-radius:50%;flex-shrink:0;background:{score_color};" }
                                        span { style: "font-size:13px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;", "{name}" }
                                    }
                                    div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:2px;",
                                        "{bundle.requirement_count} requirements · {bundle.policy_count} policies"
                                        if revisions.len() > 1 { " · {revisions.len()} revisions" }
                                    }
                                }
                                td { span { class: "chip chip-info", "{framework}" } }
                                td { div { class: "mono", style: "font-size:12px;", "{version}" } div { style: "margin-top:3px;", span { class: "chip", style: "font-size:9px;padding:1px 6px;", "{revisions.first().map(|v| v.publication_state.as_str()).unwrap_or(\"draft\")}" } } }
                                td { span { class: "mono", style: "font-size:13px;font-weight:600;color:{score_color};", "{score_label}" } div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:2px;", "{system_count_label}" } }
                                td { style: "text-align:right;", div { class: "row-actions", style: "opacity:1;justify-content:flex-end;", button { class: "btn-icon focus-ring", title: "View bundle", onclick: move |event| { event.stop_propagation(); props.on_select.call(id); }, Icon { name: IconName::ArrowRight, size: 14 } } } }
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
    #[props(default = false)]
    pub cardless: bool,
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
            class: if props.cardless { "" } else { "card" },
            style: if props.cardless { "display:flex;flex-direction:column;gap:10px;" } else { "padding:18px;display:flex;flex-direction:column;gap:10px;" },
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
        div { style: "overflow:hidden;border-top:1px solid var(--cf-divider);",
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
            table { class: "sys-table compact sys-table-dense sys-table-fixed",
                colgroup {
                    col { style: "width:22%;" }
                    col { style: "width:90px;" }
                    col { style: "width:120px;" }
                    col { style: "width:110px;" }
                    col { style: "width:60px;" }
                    col { style: "width:70px;" }
                    col { style: "width:60px;" }
                    col { style: "width:76px;" }
                    col { style: "width:52px;" }
                }
                thead { tr {
                    th { "Host" }
                    th { "Env" }
                    th { "Assignment" }
                    th { "Score" }
                    th { style: "text-align:right;", "Pass" }
                    th { style: "text-align:right;", "Warn" }
                    th { style: "text-align:right;", "Fail" }
                    th { style: "text-align:right;", "Waiver" }
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
                                     td { style: "font-size:11px;color:var(--cf-text-muted);", "—" }
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
                                     td { class: "mono", style: "text-align:right;color:#34d399;font-weight:600;", "{pass}" }
                                    td {
                                        class: "mono",
                                         style: if warn > 0 { "text-align:right;color:#fbbf24;font-weight:600;" } else { "text-align:right;color:var(--cf-text-muted);" },
                                        "{warn}"
                                    }
                                    td {
                                        class: "mono",
                                         style: if fail > 0 { "text-align:right;color:#f87171;font-weight:700;" } else { "text-align:right;color:var(--cf-text-muted);" },
                                        "{fail}"
                                    }
                                    td {
                                        class: "mono",
                                         style: if waiver > 0 { "text-align:right;color:#a78bfa;" } else { "text-align:right;color:var(--cf-text-muted);" },
                                        "{waiver}"
                                    }
                                     td { style: "text-align:right;",
                                         button {
                                             class: "btn-icon focus-ring",
                                             title: "View evidence",
                                             onclick: move |e| { e.stop_propagation(); props.on_evidence.call(system_id); },
                                             Icon { name: IconName::ArrowRight, size: 14 }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvidenceGrouping {
    Severity,
    ControlFamily,
    CmmcLevel,
    CisSection,
    Flat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EvidenceNavigatorGroup {
    key: String,
    label: String,
    controls: Vec<usize>,
}

fn evidence_grouping(framework: Option<&str>) -> EvidenceGrouping {
    let framework = framework.unwrap_or_default().to_ascii_lowercase();
    if framework.contains("stig") {
        EvidenceGrouping::Severity
    } else if framework.contains("800-53") || framework.contains("nist") {
        EvidenceGrouping::ControlFamily
    } else if framework.contains("cmmc") {
        EvidenceGrouping::CmmcLevel
    } else if framework.contains("cis") {
        EvidenceGrouping::CisSection
    } else {
        EvidenceGrouping::Flat
    }
}

fn control_matches(control: &ComplianceControlEvidence, query: &str) -> bool {
    query.is_empty()
        || control.policy_name.to_ascii_lowercase().contains(query)
        || format!("{:?}", control.status)
            .to_ascii_lowercase()
            .contains(query)
}

fn cis_sort_key(section: &str) -> Vec<u32> {
    section
        .split('.')
        .map(|part| part.trim().parse().unwrap_or(u32::MAX))
        .collect()
}

fn navigator_groups(
    controls: &[ComplianceControlEvidence],
    framework: Option<&str>,
    query: &str,
) -> Vec<EvidenceNavigatorGroup> {
    let mut groups = std::collections::BTreeMap::<String, (String, Vec<usize>)>::new();
    let grouping = evidence_grouping(framework);
    for (index, control) in controls
        .iter()
        .enumerate()
        .filter(|(_, control)| control_matches(control, query))
    {
        let (key, label) = match grouping {
            EvidenceGrouping::Severity => match control.severity.to_ascii_lowercase().as_str() {
                "high" | "cat i" | "cat 1" => ("01-cat-i".to_string(), "CAT I".to_string()),
                "medium" | "cat ii" | "cat 2" => ("02-cat-ii".to_string(), "CAT II".to_string()),
                "low" | "cat iii" | "cat 3" => ("03-cat-iii".to_string(), "CAT III".to_string()),
                _ => ("04-unrated".to_string(), "Unrated".to_string()),
            },
            EvidenceGrouping::ControlFamily => {
                let family = control
                    .control_family
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_uppercase();
                let position = ["AC", "AU", "CM", "IA", "SC", "SI", "MP"]
                    .iter()
                    .position(|known| *known == family)
                    .map(|position| format!("{:02}-{family}", position + 1));
                match position {
                    Some(key) => (key, family),
                    None => ("99-ungrouped".to_string(), "Ungrouped".to_string()),
                }
            }
            EvidenceGrouping::CmmcLevel => match control.cmmc_level {
                Some(3) => ("01-l3".to_string(), "Level 3".to_string()),
                Some(2) => ("02-l2".to_string(), "Level 2".to_string()),
                Some(1) => ("03-l1".to_string(), "Level 1".to_string()),
                _ => ("04-unrated".to_string(), "Unrated".to_string()),
            },
            EvidenceGrouping::CisSection => match control.cis_section.as_deref().map(str::trim) {
                Some(section) if !section.is_empty() => {
                    (section.to_string(), format!("Section {section}"))
                }
                _ => ("~unmapped".to_string(), "Unmapped".to_string()),
            },
            EvidenceGrouping::Flat => ("unmapped".to_string(), "Unmapped".to_string()),
        };
        groups
            .entry(key)
            .or_insert_with(|| (label, Vec::new()))
            .1
            .push(index);
    }

    let mut groups: Vec<_> = groups
        .into_iter()
        .map(|(key, (label, controls))| EvidenceNavigatorGroup {
            key,
            label,
            controls,
        })
        .collect();
    if grouping == EvidenceGrouping::CisSection {
        groups.sort_by(
            |left, right| match (left.key.as_str(), right.key.as_str()) {
                ("~unmapped", "~unmapped") => std::cmp::Ordering::Equal,
                ("~unmapped", _) => std::cmp::Ordering::Greater,
                (_, "~unmapped") => std::cmp::Ordering::Less,
                _ => cis_sort_key(&left.key).cmp(&cis_sort_key(&right.key)),
            },
        );
    }
    groups
}

fn visible_control_order(groups: &[EvidenceNavigatorGroup], collapsed: &[String]) -> Vec<usize> {
    groups
        .iter()
        .filter(|group| !collapsed.iter().any(|key| key == &group.key))
        .flat_map(|group| group.controls.iter().copied())
        .collect()
}

fn reconciled_active(active: usize, visible: &[usize]) -> Option<usize> {
    visible
        .contains(&active)
        .then_some(active)
        .or_else(|| visible.first().copied())
}

#[component]
pub fn EvidenceDrawer(props: EvidenceDrawerProps) -> Element {
    let mut active_idx = use_signal(|| 0usize);
    let mut filter = use_signal(String::new);
    let mut collapsed = use_signal(Vec::<String>::new);
    let total = props.evidence.controls.len();
    let hostname = props.evidence.hostname.clone();
    let bundle_name = props.bundle_name.clone();

    let query = filter.read().trim().to_ascii_lowercase();
    let groups = navigator_groups(
        &props.evidence.controls,
        props.evidence.framework.as_deref(),
        &query,
    );
    let visible = visible_control_order(&groups, &collapsed.read());
    let active_control = reconciled_active(*active_idx.read(), &visible)
        .and_then(|index| props.evidence.controls.get(index).cloned());
    let visible_for_reconciliation = visible.clone();
    use_effect(move || {
        let active = *active_idx.read();
        if let Some(index) = reconciled_active(active, &visible_for_reconciliation) {
            if index != active {
                active_idx.set(index);
            }
        }
    });
    let groups_for_keyboard = groups.clone();

    rsx! {
        div { class: "fl-tray-backdrop", onclick: move |_| props.on_close.call(()) }
        aside {
            class: "fl-tray",
            style: "width:min(960px,96vw);",
            onkeydown: move |event| {
                let visible = visible_control_order(&groups_for_keyboard, &collapsed.read());
                let key = event.key().to_string();
                match key.as_str() {
                    "Escape" => props.on_close.call(()),
                    "ArrowDown" | "j" | "J" => {
                        let active = *active_idx.read();
                        if let Some(current) = reconciled_active(active, &visible) {
                            let position = visible.iter().position(|index| *index == current).unwrap_or(0);
                            if let Some(next) = visible.get((position + 1).min(visible.len().saturating_sub(1))) {
                                event.prevent_default();
                                active_idx.set(*next);
                            }
                        }
                    }
                    "ArrowUp" | "k" | "K" => {
                        let active = *active_idx.read();
                        if let Some(current) = reconciled_active(active, &visible) {
                            let position = visible.iter().position(|index| *index == current).unwrap_or(0);
                            if let Some(previous) = visible.get(position.saturating_sub(1)) {
                                event.prevent_default();
                                active_idx.set(*previous);
                            }
                        }
                    }
                    _ => {}
                }
            },
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
                style: "display:grid;grid-template-columns:minmax(0,260px) minmax(0,1fr);flex:1;min-height:0;overflow:hidden;",
                // Left: control nav
                nav {
                    style: "border-right:1px solid var(--cf-divider);overflow-y:auto;overflow-x:hidden;background:color-mix(in oklab,var(--cf-page-bg) 30%,var(--cf-card-bg));",
                    div { style: "position:sticky;top:0;z-index:1;padding:8px;background:color-mix(in oklab,var(--cf-page-bg) 55%,var(--cf-card-bg));border-bottom:1px solid var(--cf-divider);",
                        input {
                            class: "input focus-ring",
                            placeholder: "Filter controls…",
                            value: "{filter}",
                            style: "width:100%;box-sizing:border-box;font-size:11.5px;padding:6px 8px;",
                            onkeydown: move |event| event.stop_propagation(),
                            oninput: move |event| filter.set(event.value()),
                        }
                    }
                    if groups.is_empty() {
                        div { style: "padding:20px 14px;font-size:12px;color:var(--cf-text-muted);text-align:center;", "No controls match." }
                    }
                    for group in groups.iter() {
                        {
                            let key = group.key.clone();
                            let label = group.label.clone();
                            let controls = group.controls.clone();
                            let groups_for_collapse = groups.clone();
                            let is_collapsed = collapsed.read().iter().any(|collapsed_key| collapsed_key == &key);
                            rsx! {
                                div {
                                    button {
                                        class: "focus-ring",
                                        style: "all:unset;cursor:pointer;display:flex;align-items:center;gap:6px;width:100%;box-sizing:border-box;padding:9px 14px 5px;font-size:9.5px;text-transform:uppercase;letter-spacing:0.06em;font-weight:700;color:var(--cf-text-muted);",
                                        onclick: move |_| {
                                            let mut next = collapsed.read().clone();
                                            if let Some(position) = next.iter().position(|collapsed_key| collapsed_key == &key) {
                                                next.remove(position);
                                            } else {
                                                next.push(key.clone());
                                            }
                                            let visible = visible_control_order(&groups_for_collapse, &next);
                                            collapsed.set(next);
                                            let active = *active_idx.read();
                                            if let Some(index) = reconciled_active(active, &visible) {
                                                active_idx.set(index);
                                            }
                                        },
                                        Icon { name: if is_collapsed { IconName::ChevronRight } else { IconName::ChevronDown }, size: 10 }
                                        span { style: "flex:1;text-align:left;", "{label} · {controls.len()}" }
                                    }
                                    if !is_collapsed {
                                        for index in controls {
                                            {
                                                let control = props.evidence.controls[index].clone();
                                                let is_sel = index == *active_idx.read();
                                                let dot_color = control_status_color(&control.status);
                                                let policy_name = control.policy_name.clone();
                                                rsx! {
                                                    button {
                                                        class: "focus-ring",
                                                        style: if is_sel { "all:unset;cursor:pointer;display:block;padding:10px 14px;width:100%;box-sizing:border-box;border-left:3px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 8%,transparent);border-bottom:1px solid var(--cf-divider);" } else { "all:unset;cursor:pointer;display:block;padding:10px 14px;width:100%;box-sizing:border-box;border-left:3px solid transparent;background:transparent;border-bottom:1px solid var(--cf-divider);" },
                                                        onclick: move |_| active_idx.set(index),
                                                        div { style: "display:flex;justify-content:space-between;align-items:center;gap:8px;",
                                                            span { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "{index+1:02}" }
                                                            span { style: "width:8px;height:8px;border-radius:50%;background:{dot_color};" }
                                                        }
                                                        div { style: if is_sel { "font-size:12px;color:var(--cf-text-primary);margin-top:4px;font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" } else { "font-size:12px;color:var(--cf-text-primary);margin-top:4px;font-weight:400;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;" }, "{policy_name}" }
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

                // Right: evidence detail
                div {
                    style: "overflow:auto;padding:20px;display:flex;flex-direction:column;gap:16px;",
                    if let Some(ctrl) = active_control {
                        ControlEvidenceCard {
                            control: ctrl,
                            control_idx: reconciled_active(*active_idx.read(), &visible).unwrap_or(0),
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn control(
        name: &str,
        cmmc_level: Option<i32>,
        cis_section: Option<&str>,
    ) -> ComplianceControlEvidence {
        ComplianceControlEvidence {
            policy_id: Uuid::nil(),
            policy_name: name.to_string(),
            status: ComplianceControlStatus::Pass,
            severity: "medium".to_string(),
            summary: String::new(),
            evidence_items: Vec::new(),
            framework_mapping: String::new(),
            control_family: Some("AC".to_string()),
            cmmc_level,
            cis_section: cis_section.map(str::to_string),
        }
    }

    #[test]
    fn grouping_uses_bundle_framework_not_control_severity() {
        assert_eq!(
            evidence_grouping(Some("DISA STIG")),
            EvidenceGrouping::Severity
        );
        assert_eq!(
            evidence_grouping(Some("NIST SP 800-53")),
            EvidenceGrouping::ControlFamily
        );
        assert_eq!(
            evidence_grouping(Some("CMMC 2.0")),
            EvidenceGrouping::CmmcLevel
        );
        assert_eq!(
            evidence_grouping(Some("CIS Benchmark")),
            EvidenceGrouping::CisSection
        );
        assert_eq!(
            evidence_grouping(Some("Internal baseline")),
            EvidenceGrouping::Flat
        );
    }

    #[test]
    fn visible_order_reconciles_filter_and_collapsed_groups() {
        let controls = vec![
            control("Account management", None, None),
            control("Audit", None, None),
        ];
        let groups = navigator_groups(&controls, Some("NIST 800-53"), "audit");
        assert_eq!(visible_control_order(&groups, &[]), vec![1]);
        assert_eq!(reconciled_active(0, &[1]), Some(1));
        assert_eq!(reconciled_active(1, &[]), None);

        let collapsed = vec![groups[0].key.clone()];
        assert!(visible_control_order(&groups, &collapsed).is_empty());
    }

    #[test]
    fn cis_sections_sort_naturally_and_unmapped_last() {
        let controls = vec![
            control("ten", None, Some("1.10")),
            control("two", None, Some("1.2")),
            control("one", None, Some("1.1")),
            control("unmapped", None, None),
        ];
        let labels: Vec<_> = navigator_groups(&controls, Some("CIS Benchmark"), "")
            .into_iter()
            .map(|group| group.label)
            .collect();
        assert_eq!(
            labels,
            vec!["Section 1.1", "Section 1.2", "Section 1.10", "Unmapped"]
        );
    }
}
