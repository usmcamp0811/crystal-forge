//! CVE (Common Vulnerabilities and Exposures) display components.

use std::collections::{BTreeMap, HashSet};

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::save_system_cve_justification;
use crate::api::models::{
    CveSeverity, ExactCveAuthorityFailureReason, SaveSystemCveJustificationRequest,
    SystemCveInventoryAuthority, SystemCveInventoryMetadata, SystemCveInventoryPageResponse,
    SystemCveInventoryRowIdentity, SystemCveInventorySource, SystemCveInventoryVulnerability,
};
use crate::components::poam::{CvePoamContext, CvePoamCreateModal};
use crate::theme;
use crate::views::poam_api::{
    CveObservationReference, CvePoamRelationship, PoamDetail, PoamSummary,
};

#[derive(Clone)]
struct GroupedCve {
    cve_id: String,
    severity: CveSeverity,
    cvss_score: Option<f32>,
    description: String,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
    package_instances: Vec<SystemCveInventoryVulnerability>,
    justification_category: Option<String>,
    justification_reason: Option<String>,
    justification_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A single CVE entry within a package group (design "package-first" view).
#[derive(Clone)]
struct PackageCve {
    stable_identity: SystemCveInventoryRowIdentity,
    cve_id: String,
    severity: CveSeverity,
    cvss_score: Option<f32>,
    description: String,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether a fix is available for this package instance.
    has_fix: bool,
    installed_version: String,
    fixed_version: Option<String>,
    remediation: Option<CvePoamRelationship>,
    remediation_conflict: bool,
    justification_category: Option<String>,
    justification_reason: Option<String>,
    justification_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Identifies one in-flight continuation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CveInventoryContinuation {
    generation: u64,
    /// Contains the opaque server-issued cursor for this request.
    pub cursor: String,
}

/// Stores loaded system CVE pages without discarding rows after continuation failures.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CveInventoryPaginationState {
    /// Contains the current source and all loaded unique rows.
    pub inventory: Option<SystemCveInventoryPageResponse>,
    /// Reports whether one continuation request is active.
    pub continuation_loading: bool,
    /// Contains the latest continuation failure while loaded rows remain visible.
    pub continuation_error: Option<String>,
    generation: u64,
}

impl CveInventoryPaginationState {
    /// Replaces all loaded pages with a new first page and invalidates old requests.
    pub fn reset(&mut self, inventory: Option<SystemCveInventoryPageResponse>) {
        self.generation = self.generation.wrapping_add(1);
        self.inventory = inventory;
        self.continuation_loading = false;
        self.continuation_error = None;
    }

    /// Starts one continuation request when another page is available.
    pub fn begin_continuation(&mut self) -> Option<CveInventoryContinuation> {
        if self.continuation_loading {
            return None;
        }
        let cursor = self.inventory.as_ref()?.next_cursor.clone()?;
        self.continuation_loading = true;
        self.continuation_error = None;
        Some(CveInventoryContinuation {
            generation: self.generation,
            cursor,
        })
    }

    /// Appends a matching response and ignores stale request completions.
    ///
    /// Returns `false` when the response belongs to a different source. The
    /// caller must then clear this state and request a new first page.
    pub fn complete_continuation(
        &mut self,
        request: &CveInventoryContinuation,
        page: SystemCveInventoryPageResponse,
    ) -> bool {
        if request.generation != self.generation || !self.continuation_loading {
            return true;
        }
        self.continuation_loading = false;
        let Some(current) = self.inventory.as_mut() else {
            return false;
        };
        if current.authority != page.authority
            || current.source != page.source
            || current.inventory_revision != page.inventory_revision
        {
            return false;
        }

        let mut identities = current
            .vulnerabilities
            .iter()
            .map(|row| row.stable_identity.clone())
            .collect::<HashSet<_>>();
        current.vulnerabilities.extend(
            page.vulnerabilities
                .into_iter()
                .filter(|row| identities.insert(row.stable_identity.clone())),
        );
        current.metadata = page.metadata;
        current.has_more = page.has_more;
        current.next_cursor = page.next_cursor;
        current.exact_authority_failure = page.exact_authority_failure;
        true
    }

    /// Records a matching continuation failure while preserving loaded rows.
    pub fn fail_continuation(&mut self, request: &CveInventoryContinuation, message: String) {
        if request.generation == self.generation && self.continuation_loading {
            self.continuation_loading = false;
            self.continuation_error = Some(message);
        }
    }
}

/// A package and the (deduplicated) CVEs affecting it, mirroring the design's
/// package-first grouping.
#[derive(Clone)]
struct PackageGroup {
    canonical_package_name: String,
    package_name: String,
    version: String,
    cves: Vec<PackageCve>,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    unknown: usize,
    fixable: usize,
    max_cvss: Option<f32>,
    /// Severity-weighted sort score (higher = more severe).
    sort_weight: i64,
}

const JUSTIFICATION_PRESETS: [(&str, &str); 5] = [
    ("false_positive", "False positive"),
    ("accepted_risk", "Accepted risk"),
    (
        "compensating_control",
        "Compensating controls are in place and documented",
    ),
    (
        "planned_remediation",
        "Planned remediation approved; waiting for patch window",
    ),
    (
        "vendor_pending_fix",
        "Vendor fix not yet available; temporary risk acceptance",
    ),
];

/// Shows package-grouped vulnerabilities and exact-CVE remediation for one named system.
#[component]
pub fn CvesTab(
    system_id: Uuid,
    hostname: String,
    vulnerabilities: Vec<SystemCveInventoryVulnerability>,
    /// Gives authoritative totals over the complete server-selected scope.
    inventory_metadata: SystemCveInventoryMetadata,
    /// Identifies the single inventory source selected by the server.
    inventory_authority: Option<SystemCveInventoryAuthority>,
    /// Gives real provenance for the selected completed scan.
    inventory_source: Option<SystemCveInventorySource>,
    /// Reports why exact remediation authority was unavailable.
    exact_authority_failure: Option<ExactCveAuthorityFailureReason>,
    allow_mutations: bool,
    on_saved: EventHandler<()>,
    /// Opens the common POA&M detail route or tray.
    on_open_poam: EventHandler<Uuid>,
    /// True while the vulnerabilities resource is still loading.
    #[props(default = false)]
    loading: bool,
    /// Error message when the vulnerabilities load failed. When set, the tab
    /// renders an error state instead of (mock) data — security data must never
    /// silently fall back to fake CVEs in production paths.
    #[props(default = None)]
    error: Option<String>,
    /// Reports whether another bounded page is available.
    #[props(default = false)]
    has_more: bool,
    /// Reports whether a continuation request is active.
    #[props(default = false)]
    continuation_loading: bool,
    /// Gives the latest continuation failure without replacing loaded rows.
    #[props(default = None)]
    continuation_error: Option<String>,
    /// Requests the next server-issued page.
    on_load_more: EventHandler<()>,
) -> Element {
    let mut expanded_cve: Signal<Option<String>> = use_signal(|| None);

    let mut editing_cve: Signal<Option<String>> = use_signal(|| None);
    let mut draft_category: Signal<Option<String>> = use_signal(|| None);
    let mut draft_reason: Signal<String> = use_signal(String::new);
    let mut save_status: Signal<Option<String>> = use_signal(|| None);
    let mut save_in_progress = use_signal(|| false);
    let mut create_context = use_signal(|| None::<CvePoamContext>);
    let mut created_relationship = use_signal(|| None::<(CveObservationReference, PoamSummary)>);

    // Package-first grouping matching the design reference. The System Detail CVE
    // example does not include a filter/search bar; filtering remains available on the
    // dedicated CVE surface, while this tab focuses on the per-system package rollup.
    let filtered_groups = group_vulnerabilities_by_package(&vulnerabilities);

    let shown_package_count = filtered_groups.len();
    let shown_package_suffix = if shown_package_count == 1 { "" } else { "s" };
    let shown_finding_count = vulnerabilities.len() as i64;
    let total_findings = inventory_metadata.total_findings;
    let exact_remediation_allowed =
        inventory_allows_exact_remediation(inventory_authority, allow_mutations);
    let ordinary_justification_allowed =
        inventory_allows_ordinary_justification(inventory_authority, allow_mutations);

    let status_is_error = save_status
        .read()
        .as_ref()
        .map(|message| message.starts_with("Failed") || message.contains("required"))
        .unwrap_or(false);

    // Loading state — show a spinner instead of an empty/fake list while the
    // vulnerabilities resource is still in flight.
    if loading {
        return rsx! {
            div {
                class: "empty",
                "data-testid": "system-cves-loading",
                crate::components::loading::DashboardLoadingSpinner {
                    label: "Loading vulnerabilities".to_string(),
                    size: 36,
                }
                div { "Fetching the latest CVE scan results." }
            }
        };
    }

    // Error state — never render mock CVEs on API failure (security data).
    if let Some(message) = error {
        return rsx! {
            div {
                class: "empty",
                "data-testid": "system-cves-error",
                h3 { "Unable to load vulnerabilities" }
                div { "{message}" }
            }
        };
    }

    rsx! {
            div {
                style: "display:flex;flex-direction:column;gap:14px;",

            match inventory_authority {
                Some(SystemCveInventoryAuthority::Exact) => rsx! {
                    div { class: "sd-callout sd-callout-success", "data-testid": "system-cves-exact",
                        strong { if total_findings == 0 { "Exact scan clean. " } else { "Exact scan findings. " } }
                        "This inventory is bound to the current evaluated deployment and supports exact remediation."
                        if let Some(source) = inventory_source.as_ref() {
                            div { class: "text-xs", "Completed {source.completed_at} by {source.scanner_name}." }
                        }
                    }
                },
                Some(SystemCveInventoryAuthority::Legacy) => rsx! {
                    div { class: "sd-callout sd-callout-warning", "data-testid": "system-cves-legacy",
                        strong { if total_findings == 0 { "Legacy scan clean. " } else { "Legacy scan findings. " } }
                        "The current evaluated deployment and a schema-1 CVE scan are required for POA&M, patch scheduling, verification, and closure. Ordinary inventory justification remains available and does not create exact remediation authority."
                        if let Some(reason) = exact_authority_failure {
                            div { class: "text-xs", "Exact authority unavailable: {exact_authority_reason_label(reason)}." }
                        }
                        if let Some(source) = inventory_source.as_ref() {
                            div { class: "text-xs", "Completed {source.completed_at} by {source.scanner_name}." }
                        }
                    }
                },
                Some(SystemCveInventoryAuthority::NoScan) => rsx! {
                    div { class: "sd-callout sd-callout-warning", "data-testid": "system-cves-no-scan",
                        strong { "No completed CVE scan. " }
                        "The current evaluated deployment and a CVE scan are required before inventory or exact remediation is available."
                        if let Some(reason) = exact_authority_failure {
                            div { class: "text-xs", "Exact authority unavailable: {exact_authority_reason_label(reason)}." }
                        }
                    }
                },
                None => rsx! {},
            }

            if let Some(message) = save_status() {
                div {
                    class: if status_is_error {
                        "px-4 py-2.5 rounded-lg border border-red-800/80 bg-red-950/50 text-sm text-red-200"
                    } else {
                        "px-4 py-2.5 rounded-lg border border-emerald-700/70 bg-emerald-950/40 text-sm text-emerald-200"
                    },
                    "{message}"
                }
            }

            // Results: package-first grouping matching the design reference.
            section {
                class: "card",
                style: "overflow: hidden;",

                div {
                    class: "sd-card-head",
                    style: "padding: 14px 18px;",
                    h2 { "Vulnerabilities" }
                    span {
                        class: "sd-card-meta",
                        "{format_count(shown_finding_count)} of {format_count(total_findings)} shown · {format_count(shown_package_count as i64)} of {format_count(inventory_metadata.total_packages)} package{shown_package_suffix} loaded"
                    }
                }

                if filtered_groups.is_empty() {
                    div {
                        class: "empty",
                        if inventory_authority == Some(SystemCveInventoryAuthority::NoScan) {
                            h3 { "No scan inventory available" }
                            div { "Run a CVE scan after the current deployment is evaluated." }
                        } else {
                            h3 { "No vulnerabilities detected" }
                            div { "The selected completed scan did not report package-level CVEs for this host." }
                        }
                    }
                } else {
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px; padding: 14px;",
                        for group in filtered_groups {
                            {
                                let group_key = group.canonical_package_name.clone();
                                let panel_id = package_disclosure_id(&group.canonical_package_name);
                                let is_open = *expanded_cve.read() == Some(group_key.clone());
                                let sev_color = package_group_color(&group);
                                let expanded_key = group_key.clone();
                                 let pending = group.cves.len().saturating_sub(group.fixable);
                                 let max_cvss = group
                                     .max_cvss
                                     .map_or_else(|| "unavailable".to_string(), |score| format!("{score:.1}"));
                                let cve_suffix = if group.cves.len() == 1 { "" } else { "s" };
                                let head_bg = if is_open {
                                    "color-mix(in oklab, var(--cf-brand-purple) 6%, var(--cf-card-bg))"
                                } else {
                                    "transparent"
                                };
                                let chevron_d = if is_open { "M19 9l-7 7-7-7" } else { "M9 5l7 7-7 7" };

                                rsx! {
                                    div {
                                        key: "{group_key}",
                                        class: "card",
                                        style: "overflow: hidden;",

                                        button {
                                            class: "focus-ring",
                                            style: "all: unset; display: grid; grid-template-columns: 24px 1fr auto; align-items: center; gap: 14px; padding: 12px 16px; cursor: pointer; width: 100%; box-sizing: border-box; border-left: 3px solid {sev_color}; background: {head_bg};",
                                            onclick: move |_| {
                                                let current = expanded_cve.read().clone();
                                                if current == Some(expanded_key.clone()) {
                                                    expanded_cve.set(None);
                                                } else {
                                                    expanded_cve.set(Some(expanded_key.clone()));
                                                }
                                            },
                                            "aria-expanded": is_open,
                                            "aria-controls": panel_id.clone(),

                                            svg {
                                                width: "14",
                                                height: "14",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                style: "color: var(--cf-text-muted);",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", d: "{chevron_d}" }
                                            }

                                            div { style: "min-width: 0;",
                                                div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap;",
                                                    span { class: "mono", style: "font-size: 14px; font-weight: 700;", "{group.package_name}" }
                                                    span { class: "mono", style: "font-size: 11px; color: var(--cf-text-muted);", "{group.version}" }
                                                    span { style: "font-size: 12px; color: var(--cf-text-muted);",
                                                        "{group.cves.len()} CVE{cve_suffix}"
                                                    }
                                                }
                                                div { style: "font-size: 11px; color: var(--cf-text-secondary); margin-top: 2px;",
                                                    "max CVSS {max_cvss} · {group.fixable} patchable · {pending} pending"
                                                }
                                            }

                                            div { style: "display: flex; gap: 5px; flex-wrap: wrap; justify-content: flex-end;",
                                                if group.critical > 0 {
                                                    span { class: "chip chip-critical", style: "font-size: 10px;", "{group.critical} crit" }
                                                }
                                                if group.high > 0 {
                                                    span { class: "chip chip-warning", style: "font-size: 10px;", "{group.high} high" }
                                                }
                                                if group.medium > 0 {
                                                    span { class: "chip chip-unknown", style: "font-size: 10px;", "{group.medium} med" }
                                                }
                                                if group.unknown > 0 {
                                                    span { class: "chip chip-neutral", style: "font-size: 10px;", "{group.unknown} unknown" }
                                                }
                                            }
                                        }

                                        if is_open {
                                            table { id: panel_id.clone(), class: "sys-table",
                                                thead {
                                                    tr {
                                                        th { "CVE" }
                                                        th { "Severity" }
                                                        th { "CVSS" }
                                                        th { "Fix" }
                                                        th { style: "text-align: right;", " " }
                                                    }
                                                }
                                                tbody {
                                                    for cve in group.cves.iter() {
                                                        {
                                                            let cve_id = cve.cve_id.clone();
                                                            let is_editing = *editing_cve.read() == Some(cve.cve_id.clone());
                                                            let has_justification = cve
                                                                .justification_reason
                                                                .as_ref()
                                                                .map(|value| !value.trim().is_empty())
                                                                .unwrap_or(false);
                                                            let cvss_label = cve
                                                                .cvss_score
                                                                .map(|score| format!("{score:.1}"))
                                                                .unwrap_or_else(|| "—".to_string());
                                                            let justification_updated_label = cve
                                                                .justification_updated_at
                                                                .map(|updated_at| updated_at.format("%Y-%m-%d %H:%M").to_string());
                                                            let action = exact_cve_action(
                                                                cve,
                                                                system_id,
                                                                created_relationship.read().as_ref(),
                                                            );
                                                             let row_key = format!(
                                                                 "{}\0{}",
                                                                 cve.stable_identity.canonical_cve_id,
                                                                 cve.stable_identity.canonical_package_name
                                                             );
                                                            let conflict_key = format!("{row_key}-conflict");

                                                            rsx! {
                                                                tr {
                                                                    key: "{row_key}",
                                                                    td { class: "mono", style: "color: var(--cf-text-primary);", "{cve.cve_id}" }
                                                                    td {
                                                                        span { class: "{severity_chip_class(&cve.severity)}", "{cve.severity.label()}" }
                                                                    }
                                                                    td { class: "mono", "{cvss_label}" }
                                                                    td {
                                                                        if cve.has_fix {
                                                                            span { class: "chip chip-healthy", "available" }
                                                                        } else {
                                                                            span { class: "chip chip-unknown", "pending" }
                                                                        }
                                                                    }
                                                                    td {
                                                                        div { class: "row-actions",
                                                                            match action {
                                                                                ExactCveAction::Create(ref relationship) => {
                                                                                    let context = CvePoamContext {
                                                                                        observation: relationship.observation.clone(),
                                                                                        hostname: hostname.clone(),
                                                                                        observed_package_name: relationship.observed_package_name.clone(),
                                                                                        installed_version: cve.installed_version.clone(),
                                                                                        fixed_version: cve.fixed_version.clone(),
                                                                                        severity: cve.severity.label().to_string(),
                                                                                        cvss_score: cve.cvss_score,
                                                                                    };
                                                                                     if exact_remediation_allowed {
                                                                                        rsx! { button { class: "btn btn-ghost xs focus-ring", title: "Create POA&M", onclick: move |_| create_context.set(Some(context.clone())), "Create POA&M" } }
                                                                                    } else {
                                                                                        rsx! {}
                                                                                    }
                                                                                }
                                                                                ExactCveAction::Active(ref poam) => { let poam_id = poam.id; rsx! { button { class: "poam-ref focus-ring", title: "Open active POA&M", onclick: move |_| on_open_poam.call(poam_id), span { class: "mono poam-human-id", "{poam.human_id}" } } } }
                                                                                ExactCveAction::Conflict => rsx! { span { role: "alert", class: "poam-chip poam-result-fail", "Context conflict" } },
                                                                                ExactCveAction::Whitelisted => rsx! { span { class: "poam-chip poam-result-waiver", "Whitelisted, not remediated" } },
                                                                                ExactCveAction::Justified => rsx! { span { class: "poam-chip poam-result-waiver", "Justified, not remediated" } },
                                                                                ExactCveAction::Unavailable => rsx! { span { class: "poam-muted", "Exact remediation unavailable" } },
                                                                            }
                                                                             if ordinary_justification_allowed { button {
                                                                                class: "btn-icon focus-ring",
                                                                                title: if has_justification { "Edit justification" } else { "Justify" },
                                                                                onclick: {
                                                                                    let cve_id = cve.cve_id.clone();
                                                                                    let existing_category = cve.justification_category.clone();
                                                                                    let existing_reason = cve.justification_reason.clone().unwrap_or_default();
                                                                                    move |_| {
                                                                                        if *editing_cve.read() == Some(cve_id.clone()) {
                                                                                            editing_cve.set(None);
                                                                                        } else {
                                                                                            editing_cve.set(Some(cve_id.clone()));
                                                                                            draft_category.set(existing_category.clone());
                                                                                            draft_reason.set(existing_reason.clone());
                                                                                            save_status.set(None);
                                                                                        }
                                                                                    }
                                                                                },
                                                                                if has_justification {
                                                                                    span { class: "chip chip-healthy", style: "font-size: 10px;", "justified" }
                                                                                } else {
                                                                                    svg {
                                                                                        width: "14",
                                                                                        height: "14",
                                                                                        fill: "none",
                                                                                        stroke: "currentColor",
                                                                                        stroke_width: "2",
                                                                                        view_box: "0 0 24 24",
                                                                                        path { stroke_linecap: "round", stroke_linejoin: "round", d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" }
                                                                                    }
                                                                                }
                                                                            } }
                                                                            a {
                                                                                class: "btn-icon focus-ring",
                                                                                title: "Open advisory",
                                                                                href: "https://nvd.nist.gov/vuln/detail/{cve.cve_id}",
                                                                                target: "_blank",
                                                                                rel: "noopener noreferrer",
                                                                                svg {
                                                                                    width: "14",
                                                                                    height: "14",
                                                                                    fill: "none",
                                                                                    stroke: "currentColor",
                                                                                    stroke_width: "2",
                                                                                    view_box: "0 0 24 24",
                                                                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5m9.656-1.328l1.5-1.5a4 4 0 00-5.656-5.656l-3 3a4 4 0 000 5.656" }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                if is_editing {
                                                                    tr {
                                                                        key: "{group.package_name}-{cve.cve_id}-justify",
                                                                        td {
                                                                            colspan: "5",
                                                                            style: "background: var(--cf-subtle-bg);",
                                                                            div {
                                                                                class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-3 space-y-2",
                                                                                div { class: "flex items-center justify-between gap-3",
                                                                                    h4 { class: "text-sm font-semibold {theme::text::PRIMARY}", "Justification — {cve.cve_id}" }
                                                                                    if let Some(ref justification_updated_label) = justification_updated_label {
                                                                                        span { class: "text-xs {theme::text::MUTED}", "Updated {justification_updated_label}" }
                                                                                    }
                                                                                }

                                                                                select {
                                                                                    class: "{theme::interactive::INPUT} w-full",
                                                                                    value: draft_category.read().clone().unwrap_or_else(|| "".to_string()),
                                                                                    onchange: move |evt| {
                                                                                        let value = evt.value();
                                                                                        if value.trim().is_empty() {
                                                                                            draft_category.set(None);
                                                                                            return;
                                                                                        }
                                                                                        draft_category.set(Some(value.clone()));
                                                                                        if let Some((_, default_reason)) = JUSTIFICATION_PRESETS
                                                                                            .iter()
                                                                                            .find(|(key, _)| *key == value)
                                                                                        {
                                                                                            if draft_reason.read().trim().is_empty() {
                                                                                                draft_reason.set((*default_reason).to_string());
                                                                                            }
                                                                                        }
                                                                                    },
                                                                                    option { value: "", "Select category (optional)" }
                                                                                    for (value, label) in JUSTIFICATION_PRESETS {
                                                                                        option { value: "{value}", "{label}" }
                                                                                    }
                                                                                }

                                                                                textarea {
                                                                                    class: "{theme::interactive::INPUT} w-full min-h-[100px]",
                                                                                    placeholder: "Document risk acceptance / mitigation rationale",
                                                                                    value: draft_reason.read().clone(),
                                                                                    oninput: move |evt| draft_reason.set(evt.value()),
                                                                                }

                                                                                div { class: "text-xs {theme::text::MUTED}", "This note is persisted per system + CVE for audit review." }

                                                                                div { class: "flex items-center gap-2",
                                                                                    button {
                                                                                        class: "px-3 py-2 rounded-md {theme::interactive::PRIMARY_BTN} text-sm font-semibold text-white transition-colors disabled:opacity-50 {theme::interactive::FOCUS_RING}",
                                                                                        disabled: *save_in_progress.read() || !ordinary_justification_allowed,
                                                                                        onclick: {
                                                                                            let cve_id = cve_id.clone();
                                                                                            move |_| {
                                                                                                let reason = draft_reason.read().trim().to_string();
                                                                                                if reason.is_empty() {
                                                                                                    save_status.set(Some("Justification reason is required".to_string()));
                                                                                                    return;
                                                                                                }

                                                                                                let category = draft_category.read().clone();
                                                                                                let cve_id_for_request = cve_id.clone();
                                                                                                save_in_progress.set(true);
                                                                                                save_status.set(None);

                                                                                                spawn(async move {
                                                                                                    let result = save_system_cve_justification(
                                                                                                        &system_id,
                                                                                                        &cve_id_for_request,
                                                                                                        &SaveSystemCveJustificationRequest { category, reason },
                                                                                                    ).await;

                                                                                                    save_in_progress.set(false);
                                                                                                    match result {
                                                                                                        Ok(_) => {
                                                                                                            save_status.set(Some("Justification saved".to_string()));
                                                                                                            editing_cve.set(None);
                                                                                                            on_saved.call(());
                                                                                                        }
                                                                                                        Err(err) => {
                                                                                                            save_status.set(Some(format!("Failed to save justification: {err}")));
                                                                                                        }
                                                                                                    }
                                                                                                });
                                                                                            }
                                                                                        },
                                                                                        if !ordinary_justification_allowed {
                                                                                            "Operator/Admin required"
                                                                                        } else if *save_in_progress.read() {
                                                                                            "Saving..."
                                                                                        } else {
                                                                                            "Save"
                                                                                        }
                                                                                    }
                                                                                    button {
                                                                                        class: "px-3 py-2 rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::interactive::HOVER_BG} text-sm {theme::text::PRIMARY} transition-colors {theme::interactive::FOCUS_RING}",
                                                                                        onclick: move |_| editing_cve.set(None),
                                                                                        "Cancel"
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                if cve.remediation_conflict {
                                                                    tr { key: "{conflict_key}", td { colspan: "5", div { role: "alert", class: "sd-callout sd-callout-danger", "Conflicting server-issued remediation identities were returned for this package and CVE. No remediation action is available until the evidence is refreshed." } } }
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

                if let Some(message) = continuation_error.as_ref() {
                    div {
                        role: "alert",
                        class: "sd-callout sd-callout-danger",
                        "data-testid": "system-cves-continuation-error",
                        div { "Unable to load more vulnerabilities: {message}" }
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: continuation_loading,
                            aria_label: "Retry loading more vulnerabilities",
                            onclick: move |_| on_load_more.call(()),
                            "Retry"
                        }
                    }
                }

                if has_more && continuation_error.is_none() {
                    div { style: "display:flex;justify-content:center;padding:0 14px 14px;",
                        button {
                            class: "btn btn-ghost focus-ring",
                            "data-testid": "system-cves-load-more",
                            disabled: continuation_loading,
                            aria_busy: continuation_loading,
                            aria_label: if continuation_loading { "Loading more vulnerabilities" } else { "Load more vulnerabilities" },
                            onclick: move |_| on_load_more.call(()),
                            if continuation_loading { "Loading..." } else { "Load more" }
                        }
                    }
                }
            }
            if let Some(context) = create_context() {
                CvePoamCreateModal {
                    context: context.clone(),
                    on_close: move |_| create_context.set(None),
                    on_created: move |detail: PoamDetail| {
                        created_relationship.set(Some((context.observation.clone(), detail.poam.clone())));
                        create_context.set(None);
                        on_saved.call(());
                        on_open_poam.call(detail.poam.id);
                    },
                }
            }
        }
    }
}

fn exact_authority_reason_label(reason: ExactCveAuthorityFailureReason) -> &'static str {
    match reason {
        ExactCveAuthorityFailureReason::MissingCurrentGeneration => "current generation missing",
        ExactCveAuthorityFailureReason::CurrentStoreMismatch => "current generation/store mismatch",
        ExactCveAuthorityFailureReason::RetainedGenerationUnavailable => {
            "retained generation unavailable"
        }
        ExactCveAuthorityFailureReason::RetainedStoreMismatch => {
            "retained generation/store mismatch"
        }
        ExactCveAuthorityFailureReason::LineageUnverified => "deployment lineage unverified",
        ExactCveAuthorityFailureReason::SnapshotUnavailable => "evaluation snapshot unavailable",
        ExactCveAuthorityFailureReason::SnapshotUnsupported => "evaluation snapshot unsupported",
        ExactCveAuthorityFailureReason::ExactDerivationUnavailable => {
            "exact derivation unavailable"
        }
        ExactCveAuthorityFailureReason::NoSchema1CurrentScan => {
            "no schema-1 scan for the current deployment"
        }
    }
}

fn inventory_allows_exact_remediation(
    authority: Option<SystemCveInventoryAuthority>,
    role_allows_mutation: bool,
) -> bool {
    role_allows_mutation && authority == Some(SystemCveInventoryAuthority::Exact)
}

fn inventory_allows_ordinary_justification(
    authority: Option<SystemCveInventoryAuthority>,
    role_allows_mutation: bool,
) -> bool {
    role_allows_mutation
        && matches!(
            authority,
            Some(SystemCveInventoryAuthority::Exact | SystemCveInventoryAuthority::Legacy)
        )
}

fn group_vulnerabilities_by_cve(
    vulnerabilities: &[SystemCveInventoryVulnerability],
) -> Vec<GroupedCve> {
    let mut grouped: BTreeMap<String, GroupedCve> = BTreeMap::new();

    for vuln in vulnerabilities {
        let entry = grouped
            .entry(vuln.cve_id.clone())
            .or_insert_with(|| GroupedCve {
                cve_id: vuln.cve_id.clone(),
                severity: vuln.severity.clone(),
                cvss_score: vuln.cvss_score,
                description: vuln.description.clone(),
                published_at: vuln.published_at,
                status: normalize_status(Some(vuln.status.as_str())),
                package_instances: Vec::new(),
                justification_category: vuln.justification_category.clone(),
                justification_reason: vuln.justification_reason.clone(),
                justification_updated_at: vuln.justification_updated_at,
            });

        // Deduplicate by package_name + installed_version to avoid showing
        // go-1.24.4 repeated 50+ times from different store paths/derivations
        let already_has_package = entry.package_instances.iter().any(|item| {
            item.package_name == vuln.package_name
                && item.installed_version == vuln.installed_version
        });

        if !already_has_package {
            entry.package_instances.push(vuln.clone());
        }

        if severity_rank(&vuln.severity) > severity_rank(&entry.severity) {
            entry.severity = vuln.severity.clone();
        }

        if entry.cvss_score.unwrap_or_default() < vuln.cvss_score.unwrap_or_default() {
            entry.cvss_score = vuln.cvss_score;
        }

        if entry.justification_reason.is_none() && vuln.justification_reason.is_some() {
            entry.justification_reason = vuln.justification_reason.clone();
            entry.justification_category = vuln.justification_category.clone();
            entry.justification_updated_at = vuln.justification_updated_at;
        }

        entry.status = reconcile_group_status(&entry.status, Some(vuln.status.as_str()));
    }

    let mut groups = grouped.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.package_instances.sort_by(|a, b| {
            a.package_name
                .cmp(&b.package_name)
                .then_with(|| a.installed_version.cmp(&b.installed_version))
        });
    }
    groups.sort_by(|a, b| {
        b.cvss_score
            .partial_cmp(&a.cvss_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cve_id.cmp(&b.cve_id))
    });
    groups
}

/// Group vulnerabilities by package name (inverse of `group_vulnerabilities_by_cve`).
/// Within each package, CVEs are deduplicated by CVE id and the worst severity /
/// highest CVSS / available-fix flag is retained.
fn group_vulnerabilities_by_package(
    vulnerabilities: &[SystemCveInventoryVulnerability],
) -> Vec<PackageGroup> {
    let mut packages: BTreeMap<String, PackageGroup> = BTreeMap::new();

    for vuln in vulnerabilities {
        let package_key = vuln.canonical_package_name.clone();
        let group = packages.entry(package_key).or_insert_with(|| PackageGroup {
            canonical_package_name: vuln.canonical_package_name.clone(),
            package_name: vuln.canonical_package_name.clone(),
            version: vuln.installed_version.clone(),
            cves: Vec::new(),
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
            unknown: 0,
            fixable: 0,
            max_cvss: None,
            sort_weight: 0,
        });

        if group.version != vuln.installed_version {
            group.version = "Multiple versions".to_string();
        }

        let has_fix = vuln.fixed_version.is_some();

        if let Some(existing) = group.cves.iter_mut().find(|c| {
            c.cve_id == vuln.cve_id
                && remediation_identity(&c.remediation) == remediation_identity(&vuln.remediation)
        }) {
            // Merge into the existing CVE entry, keeping the worst observed values.
            if severity_rank(&vuln.severity) > severity_rank(&existing.severity) {
                existing.severity = vuln.severity.clone();
            }
            if existing.cvss_score.unwrap_or_default() < vuln.cvss_score.unwrap_or_default() {
                existing.cvss_score = vuln.cvss_score;
            }
            existing.has_fix = existing.has_fix || has_fix;
            if existing.justification_reason.is_none() && vuln.justification_reason.is_some() {
                existing.justification_reason = vuln.justification_reason.clone();
                existing.justification_category = vuln.justification_category.clone();
                existing.justification_updated_at = vuln.justification_updated_at;
            }
        } else {
            let remediation_conflict = group.cves.iter().any(|c| c.cve_id == vuln.cve_id);
            if remediation_conflict {
                for existing in group.cves.iter_mut().filter(|c| c.cve_id == vuln.cve_id) {
                    existing.remediation_conflict = true;
                }
            }
            group.cves.push(PackageCve {
                stable_identity: vuln.stable_identity.clone(),
                cve_id: vuln.cve_id.clone(),
                severity: vuln.severity.clone(),
                cvss_score: vuln.cvss_score,
                description: vuln.description.clone(),
                published_at: vuln.published_at,
                has_fix,
                installed_version: vuln.installed_version.clone(),
                fixed_version: vuln.fixed_version.clone(),
                remediation: vuln.remediation.clone(),
                remediation_conflict,
                justification_category: vuln.justification_category.clone(),
                justification_reason: vuln.justification_reason.clone(),
                justification_updated_at: vuln.justification_updated_at,
            });
        }
    }

    let sev_weight = |severity: &CveSeverity| -> i64 {
        match severity {
            CveSeverity::Critical => 1000,
            CveSeverity::High => 100,
            CveSeverity::Medium => 10,
            CveSeverity::Low => 1,
            CveSeverity::Unknown => 0,
        }
    };

    let mut groups = packages.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.cves.sort_by(|a, b| {
            severity_rank(&b.severity)
                .cmp(&severity_rank(&a.severity))
                .then_with(|| {
                    b.cvss_score
                        .partial_cmp(&a.cvss_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.cve_id.cmp(&b.cve_id))
        });

        for cve in &group.cves {
            match cve.severity {
                CveSeverity::Critical => group.critical += 1,
                CveSeverity::High => group.high += 1,
                CveSeverity::Medium => group.medium += 1,
                CveSeverity::Low => group.low += 1,
                CveSeverity::Unknown => group.unknown += 1,
            }
            if cve.has_fix {
                group.fixable += 1;
            }
            if let Some(score) = cve.cvss_score {
                group.max_cvss = Some(group.max_cvss.map_or(score, |current| current.max(score)));
            }
            group.sort_weight += sev_weight(&cve.severity);
        }
    }

    groups.sort_by(|a, b| {
        b.sort_weight
            .cmp(&a.sort_weight)
            .then_with(|| a.package_name.cmp(&b.package_name))
    });
    groups
}

fn remediation_identity(
    remediation: &Option<CvePoamRelationship>,
) -> Option<&CveObservationReference> {
    remediation.as_ref().map(|value| &value.observation)
}

#[derive(Debug, Clone, PartialEq)]
enum ExactCveAction {
    Unavailable,
    Conflict,
    Whitelisted,
    Justified,
    Active(PoamSummary),
    Create(CvePoamRelationship),
}

fn exact_cve_action(
    cve: &PackageCve,
    system_id: Uuid,
    created: Option<&(CveObservationReference, PoamSummary)>,
) -> ExactCveAction {
    if cve.remediation_conflict {
        return ExactCveAction::Conflict;
    }
    let Some(relationship) = cve.remediation.as_ref() else {
        return ExactCveAction::Unavailable;
    };
    if relationship.observation.system_id != system_id
        || relationship.observation.canonical_cve_id != cve.cve_id
        || relationship.observed_package_name.is_empty()
        || relationship.observed_package_version != cve.installed_version
    {
        return ExactCveAction::Conflict;
    }
    if let Some((observation, poam)) = created {
        if observation == &relationship.observation {
            return ExactCveAction::Active(poam.clone());
        }
    }
    if let Some(poam) = relationship.active_poam.as_ref() {
        return ExactCveAction::Active(poam.clone());
    }
    if relationship.is_whitelisted {
        return ExactCveAction::Whitelisted;
    }
    if relationship.is_justified {
        return ExactCveAction::Justified;
    }
    ExactCveAction::Create(relationship.clone())
}

/// Left-border / accent color for a package group based on its worst severity,
/// matching the design reference palette.
fn package_group_color(group: &PackageGroup) -> &'static str {
    if group.critical > 0 {
        "#f87171"
    } else if group.high > 0 {
        "#fbbf24"
    } else if group.medium > 0 {
        "#60a5fa"
    } else {
        "#9ca3af"
    }
}

fn severity_chip_class(severity: &CveSeverity) -> &'static str {
    match severity {
        CveSeverity::Critical => "chip chip-critical",
        CveSeverity::High => "chip chip-warning",
        CveSeverity::Medium => "chip chip-unknown",
        CveSeverity::Low => "chip chip-unknown",
        CveSeverity::Unknown => "chip chip-neutral",
    }
}

fn severity_rank(severity: &CveSeverity) -> i32 {
    match severity {
        CveSeverity::Critical => 4,
        CveSeverity::High => 3,
        CveSeverity::Medium => 2,
        CveSeverity::Low => 1,
        CveSeverity::Unknown => 0,
    }
}

fn package_disclosure_id(canonical_package_name: &str) -> String {
    let encoded = canonical_package_name
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("system-cve-package-{encoded}")
}

fn normalize_status(value: Option<&str>) -> String {
    match value.unwrap_or("open") {
        "open" => "open".to_string(),
        "fix_available" => "fix_available".to_string(),
        "mixed" => "mixed".to_string(),
        other => other.trim().to_lowercase(),
    }
}

fn reconcile_group_status(current: &str, next: Option<&str>) -> String {
    let next_status = normalize_status(next);
    if current == "mixed" || next_status == "mixed" {
        return "mixed".to_string();
    }
    if current == next_status {
        return current.to_string();
    }
    "mixed".to_string()
}

fn status_label(status: &str) -> &'static str {
    match status {
        "fix_available" => "Fix available",
        "open" => "No known fix",
        "mixed" => "Mixed package status",
        _ => "Status unknown",
    }
}

fn justification_category_class(category: &str) -> &'static str {
    match category {
        "false_positive" => "text-violet-300 bg-violet-500/15 border border-violet-500/35",
        "accepted_risk" => "text-amber-300 bg-amber-500/15 border border-amber-500/35",
        "compensating_control" => "text-blue-300 bg-blue-500/15 border border-blue-500/35",
        "planned_remediation" => "text-emerald-300 bg-emerald-500/15 border border-emerald-500/35",
        "vendor_pending_fix" => "text-orange-300 bg-orange-500/15 border border-orange-500/35",
        _ => "text-slate-300 bg-slate-500/15 border border-slate-500/35",
    }
}

fn humanize_category(category: &str) -> String {
    category
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_count(value: i64) -> String {
    let negative = value < 0;
    let reversed: Vec<char> = value.abs().to_string().chars().rev().collect();
    let mut grouped = String::new();

    for (idx, ch) in reversed.iter().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*ch);
    }

    let mut formatted: String = grouped.chars().rev().collect();
    if negative {
        formatted.insert(0, '-');
    }
    formatted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(system_id: Uuid, scan_id: Uuid, occurrence: &str) -> CveObservationReference {
        CveObservationReference {
            system_id,
            scan_id,
            occurrence_derivation_path: occurrence.into(),
            canonical_cve_id: "CVE-2026-1000".into(),
            canonical_package_name: "openssl".into(),
        }
    }

    fn relationship(observation: CveObservationReference) -> CvePoamRelationship {
        CvePoamRelationship {
            cve_finding_id: None,
            observation,
            observed_package_name: "openssl-3.4.1".into(),
            observed_package_version: "3.4.1".into(),
            is_whitelisted: false,
            is_justified: false,
            active_poam: None,
            historical_poams: vec![],
            historical_has_more: false,
            historical_next_offset: None,
        }
    }

    fn vulnerability(
        installed_version: &str,
        remediation: Option<CvePoamRelationship>,
    ) -> SystemCveInventoryVulnerability {
        SystemCveInventoryVulnerability {
            stable_identity: SystemCveInventoryRowIdentity {
                canonical_cve_id: "CVE-2026-1000".into(),
                canonical_package_name: "openssl".into(),
            },
            cve_id: "CVE-2026-1000".into(),
            canonical_package_name: "openssl".into(),
            severity: CveSeverity::High,
            cvss_score: Some(8.1),
            description: "Test vulnerability".into(),
            package_name: "openssl".into(),
            installed_version: installed_version.into(),
            fixed_version: Some("3.4.2".into()),
            first_seen: None,
            published_at: None,
            status: "fix_available".into(),
            justification_category: None,
            justification_reason: None,
            justification_updated_at: None,
            remediation,
        }
    }

    fn inventory_page(
        scan_id: Uuid,
        rows: Vec<SystemCveInventoryVulnerability>,
        total_findings: i64,
        next_cursor: Option<&str>,
    ) -> SystemCveInventoryPageResponse {
        SystemCveInventoryPageResponse {
            authority: SystemCveInventoryAuthority::Exact,
            exact_authority_failure: None,
            source: Some(SystemCveInventorySource {
                scan_id,
                scanner_name: "vulnix".into(),
                scanner_version: Some("1.10.1".into()),
                completed_at: chrono::DateTime::parse_from_rfc3339("2026-09-14T21:00:00Z")
                    .expect("test timestamp should parse")
                    .with_timezone(&chrono::Utc),
            }),
            vulnerabilities: rows,
            metadata: SystemCveInventoryMetadata {
                total_findings,
                total_cves: total_findings,
                total_packages: total_findings,
                severity: Default::default(),
            },
            inventory_revision: scan_id.to_string(),
            has_more: next_cursor.is_some(),
            next_cursor: next_cursor.map(str::to_string),
        }
    }

    #[test]
    fn pagination_appends_unique_stable_identities_and_keeps_full_totals() {
        let scan_id = Uuid::from_u128(440);
        let first_row = vulnerability("3.4.1", None);
        let mut second_row = vulnerability("3.3.3", None);
        second_row.stable_identity.canonical_cve_id = "CVE-2026-1001".into();
        second_row.cve_id = "CVE-2026-1001".into();
        let mut state = CveInventoryPaginationState::default();
        state.reset(Some(inventory_page(
            scan_id,
            vec![first_row.clone()],
            1315,
            Some("page-2"),
        )));
        let request = state
            .begin_continuation()
            .expect("continuation should start");

        assert!(state.complete_continuation(
            &request,
            inventory_page(scan_id, vec![first_row, second_row], 1315, Some("page-3"))
        ));
        let inventory = state.inventory.expect("inventory should remain loaded");
        assert_eq!(inventory.vulnerabilities.len(), 2);
        assert_eq!(inventory.metadata.total_findings, 1315);
        assert_eq!(inventory.next_cursor.as_deref(), Some("page-3"));
    }

    #[test]
    fn pagination_source_change_requires_first_page_reset() {
        let mut state = CveInventoryPaginationState::default();
        state.reset(Some(inventory_page(
            Uuid::from_u128(440),
            vec![vulnerability("3.4.1", None)],
            2,
            Some("page-2"),
        )));
        let request = state
            .begin_continuation()
            .expect("continuation should start");
        let changed = inventory_page(
            Uuid::from_u128(441),
            vec![vulnerability("3.3.3", None)],
            1,
            None,
        );

        assert!(!state.complete_continuation(&request, changed.clone()));
        state.reset(Some(changed));
        assert_eq!(
            state
                .inventory
                .as_ref()
                .and_then(|inventory| inventory.source.as_ref())
                .map(|source| source.scan_id),
            Some(Uuid::from_u128(441))
        );
        assert_eq!(state.inventory.unwrap().vulnerabilities.len(), 1);
    }

    #[test]
    fn pagination_revision_change_requires_first_page_reset() {
        let scan_id = Uuid::from_u128(440);
        let mut state = CveInventoryPaginationState::default();
        state.reset(Some(inventory_page(
            scan_id,
            vec![vulnerability("3.4.1", None)],
            2,
            Some("page-2"),
        )));
        let request = state
            .begin_continuation()
            .expect("continuation should start");
        let mut changed = inventory_page(scan_id, vec![vulnerability("3.3.3", None)], 2, None);
        changed.inventory_revision = "changed-revision".into();

        assert!(!state.complete_continuation(&request, changed));
    }

    #[test]
    fn pagination_failure_preserves_rows_and_allows_retry() {
        let mut state = CveInventoryPaginationState::default();
        state.reset(Some(inventory_page(
            Uuid::from_u128(440),
            vec![vulnerability("3.4.1", None)],
            1315,
            Some("retry-cursor"),
        )));
        let request = state
            .begin_continuation()
            .expect("continuation should start");
        state.fail_continuation(&request, "HTTP 500: unavailable".into());

        assert_eq!(state.inventory.as_ref().unwrap().vulnerabilities.len(), 1);
        assert_eq!(
            state.continuation_error.as_deref(),
            Some("HTTP 500: unavailable")
        );
        assert_eq!(
            state.begin_continuation().map(|request| request.cursor),
            Some("retry-cursor".into())
        );
    }

    #[test]
    fn pagination_ignores_response_from_reset_generation() {
        let scan_id = Uuid::from_u128(440);
        let mut state = CveInventoryPaginationState::default();
        state.reset(Some(inventory_page(
            scan_id,
            vec![vulnerability("3.4.1", None)],
            2,
            Some("old-cursor"),
        )));
        let stale_request = state
            .begin_continuation()
            .expect("continuation should start");
        state.reset(Some(inventory_page(
            Uuid::from_u128(441),
            vec![vulnerability("4.0.0", None)],
            1,
            None,
        )));

        assert!(state.complete_continuation(
            &stale_request,
            inventory_page(scan_id, vec![vulnerability("3.3.3", None)], 2, None)
        ));
        let inventory = state.inventory.expect("new source should remain loaded");
        assert_eq!(inventory.source.unwrap().scan_id, Uuid::from_u128(441));
        assert_eq!(inventory.vulnerabilities[0].installed_version, "4.0.0");
    }

    #[test]
    fn package_groups_use_canonical_package_identity() {
        let groups = group_vulnerabilities_by_package(&[
            vulnerability("3.4.1", None),
            vulnerability("3.3.3", None),
        ]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].canonical_package_name, "openssl");
        assert_eq!(groups[0].version, "Multiple versions");
        assert!(
            groups
                .iter()
                .all(|group| { group.cves[0].fixed_version.as_deref() == Some("3.4.2") })
        );
    }

    #[test]
    fn unknown_only_package_has_neutral_semantics_and_no_cvss_score() {
        let mut unknown = vulnerability("3.4.1", None);
        unknown.severity = CveSeverity::Unknown;
        unknown.cvss_score = None;
        let groups = group_vulnerabilities_by_package(&[unknown]);

        assert_eq!(groups[0].unknown, 1);
        assert_eq!(groups[0].low, 0);
        assert_eq!(groups[0].max_cvss, None);
        assert_eq!(
            severity_chip_class(&CveSeverity::Unknown),
            "chip chip-neutral"
        );
        assert_eq!(CveSeverity::Unknown.color_class(), theme::cve::UNKNOWN_TEXT);
        assert_eq!(CveSeverity::Unknown.label(), "Unknown");
    }

    #[test]
    fn conflicting_remediation_identities_remain_separate_and_non_actionable() {
        let system_id = Uuid::from_u128(1);
        let first = relationship(observation(system_id, Uuid::from_u128(2), "/nix/store/a"));
        let second = relationship(observation(system_id, Uuid::from_u128(3), "/nix/store/b"));
        let groups = group_vulnerabilities_by_package(&[
            vulnerability("3.4.1", Some(first)),
            vulnerability("3.4.1", Some(second)),
        ]);

        assert_eq!(groups[0].cves.len(), 2);
        assert!(groups[0].cves.iter().all(|cve| cve.remediation_conflict));
        assert!(
            groups[0]
                .cves
                .iter()
                .all(|cve| exact_cve_action(cve, system_id, None) == ExactCveAction::Conflict)
        );
    }

    #[test]
    fn exact_action_requires_matching_server_issued_context() {
        let system_id = Uuid::from_u128(1);
        let relationship = relationship(observation(
            system_id,
            Uuid::from_u128(2),
            "/nix/store/opaque-occurrence",
        ));
        let groups =
            group_vulnerabilities_by_package(&[vulnerability("3.4.1", Some(relationship.clone()))]);
        let cve = &groups[0].cves[0];

        assert_eq!(
            exact_cve_action(cve, system_id, None),
            ExactCveAction::Create(relationship)
        );
        assert_eq!(
            exact_cve_action(cve, Uuid::from_u128(9), None),
            ExactCveAction::Conflict
        );
    }

    #[test]
    fn inventory_authority_distinguishes_remediation_from_justification() {
        assert!(inventory_allows_exact_remediation(
            Some(SystemCveInventoryAuthority::Exact),
            true
        ));
        assert!(!inventory_allows_exact_remediation(
            Some(SystemCveInventoryAuthority::Legacy),
            true
        ));
        assert!(!inventory_allows_exact_remediation(
            Some(SystemCveInventoryAuthority::NoScan),
            true
        ));
        assert!(!inventory_allows_exact_remediation(
            Some(SystemCveInventoryAuthority::Exact),
            false
        ));
        assert!(inventory_allows_ordinary_justification(
            Some(SystemCveInventoryAuthority::Legacy),
            true
        ));
        assert!(!inventory_allows_ordinary_justification(
            Some(SystemCveInventoryAuthority::NoScan),
            true
        ));
    }
}
