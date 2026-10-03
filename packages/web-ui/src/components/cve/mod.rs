//! CVE (Common Vulnerabilities and Exposures) display components.

pub(crate) mod triage;

use std::collections::{BTreeMap, HashMap, HashSet};

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::models::{
    CveSeverity, ExactCveAuthorityFailureReason, SystemCveCurrentAuthorityState,
    SystemCveInventoryAttempt, SystemCveInventoryAuthority, SystemCveInventoryMetadata,
    SystemCveInventoryPageResponse, SystemCveInventoryRowIdentity, SystemCveInventorySource,
    SystemCveInventoryVulnerability,
};
#[cfg(test)]
use crate::api::models::{
    SystemCveEvidenceRepresentation, SystemCveInventorySelection, SystemCveRunningTarget,
};
use crate::components::cve::triage::{SystemCveTriageDialog, fixed_version_label};
use crate::components::icon::{Icon, IconName};
#[cfg(test)]
use crate::theme;
use crate::views::poam_api::{
    self, CveObservationReference, CvePoamRelationship, SystemCveTriageDetail,
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

#[derive(Clone, Debug, PartialEq)]
enum SystemTriageCacheEntry {
    /// An expanded-package hydration request is active for this row.
    Loading,
    /// The detail passed CVE, package, and selected-system validation.
    Loaded(SystemCveTriageDetail),
    /// The authoritative detail could not be loaded or failed identity validation.
    Unavailable(String),
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
            || current.system_id != page.system_id
            || current.selection != page.selection
            || current.current_state != page.current_state
            || current.running_target != page.running_target
            || current.read_only != page.read_only
            || current.evidence_representation != page.evidence_representation
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
        // Lifecycle changes do not invalidate a stable completed source or its
        // cursor. A continuation can report a newer attempt independently.
        current.attempt = page.attempt;
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

/// Shows package-grouped vulnerabilities and exact-CVE remediation for one named system.
#[component]
pub fn CvesTab(
    system_id: Uuid,
    hostname: String,
    /// Identifies the selected inventory and invalidates revision-local UI state.
    inventory_target_key: String,
    vulnerabilities: Vec<SystemCveInventoryVulnerability>,
    /// Gives authoritative totals over the complete server-selected scope.
    inventory_metadata: SystemCveInventoryMetadata,
    /// Identifies the single inventory source selected by the server.
    inventory_authority: Option<SystemCveInventoryAuthority>,
    /// Distinguishes an absent exact scan from unavailable Current authority.
    #[props(default = None)]
    current_state: Option<SystemCveCurrentAuthorityState>,
    /// Gives real provenance for the selected completed scan.
    inventory_source: Option<SystemCveInventorySource>,
    /// Gives lifecycle metadata for the selected target, not evidence authority.
    #[props(default = None)]
    inventory_attempt: Option<SystemCveInventoryAttempt>,
    /// Reports why exact remediation authority was unavailable.
    exact_authority_failure: Option<ExactCveAuthorityFailureReason>,
    /// Is true when the selected revision is historical and immutable.
    #[props(default = false)]
    read_only: bool,
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
    /// Repeats the selected inventory read without scheduling a scan.
    on_retry_read: EventHandler<()>,
    /// Requests the next server-issued page.
    on_load_more: EventHandler<()>,
) -> Element {
    let _ = hostname;
    let mut expanded_cve: Signal<Option<String>> = use_signal(|| None);
    let mut default_expansion_applied = use_signal(|| false);

    let mut save_status: Signal<Option<String>> = use_signal(|| None);
    let mut triage_details: Signal<HashMap<SystemCveInventoryRowIdentity, SystemTriageCacheEntry>> =
        use_signal(HashMap::new);
    let mut triage_target: Signal<Option<PackageCve>> = use_signal(|| None);
    let mut triage_dialog_detail: Signal<Option<SystemCveTriageDetail>> = use_signal(|| None);
    let mut dialog_start_key: Signal<Option<String>> = use_signal(|| None);
    let mut triage_opening: Signal<Option<SystemCveInventoryRowIdentity>> = use_signal(|| None);
    let mut triage_open_generation = use_signal(|| 0_u64);
    let mut triage_hydration_generation = use_signal(|| 0_u64);
    let mut triage_hydration_tokens: Signal<HashMap<SystemCveInventoryRowIdentity, u64>> =
        use_signal(HashMap::new);
    use_effect(use_reactive(&inventory_target_key, move |_| {
        expanded_cve.set(None);
        default_expansion_applied.set(false);
        save_status.set(None);
        triage_details.write().clear();
        // CONCURRENCY: Refresh invalidates row hydration, but a mounted draft
        // retains its starting evidence and scope. The modal footer blocks
        // submission until the operator closes and reopens it from new evidence.
        if triage_dialog_detail.peek().is_none() {
            triage_target.set(None);
            dialog_start_key.set(None);
        }
        triage_opening.set(None);
        let next_open_generation = (*triage_open_generation.peek()).wrapping_add(1);
        triage_open_generation.set(next_open_generation);
        let next_hydration_generation = (*triage_hydration_generation.peek()).wrapping_add(1);
        triage_hydration_generation.set(next_hydration_generation);
        triage_hydration_tokens.write().clear();
    }));

    // Package-first grouping matching the design reference. The System Detail CVE
    // example does not include a filter/search bar; filtering remains available on the
    // dedicated CVE surface, while this tab focuses on the per-system package rollup.
    let filtered_groups = group_vulnerabilities_by_package(&vulnerabilities);
    let exact_remediation_allowed =
        inventory_allows_exact_remediation(inventory_authority, allow_mutations);
    let first_package = filtered_groups
        .first()
        .map(|group| group.canonical_package_name.clone());
    let first_package_rows = filtered_groups
        .first()
        .map(|group| group.cves.clone())
        .unwrap_or_default();
    use_effect(move || {
        if !default_expansion_applied() {
            if let Some(package) = first_package.clone() {
                expanded_cve.set(Some(package));
                default_expansion_applied.set(true);
                // A successful mutation refresh discards the old detail cache.
                // Hydrate the automatically expanded package as well, so a
                // persisted decision never looks like an unsaved new action
                // merely because no one manually toggled the package.
                if exact_remediation_allowed {
                    let generation = (*triage_hydration_generation.peek()).wrapping_add(1);
                    triage_hydration_generation.set(generation);
                    for target in first_package_rows.clone() {
                        if system_triage_row_state(inventory_authority, &target, system_id, None)
                            != SystemTriageRowState::Review
                            || triage_details.read().contains_key(&target.stable_identity)
                        {
                            continue;
                        }
                        let identity = target.stable_identity.clone();
                        let cve_id = target.cve_id.clone();
                        let package = identity.canonical_package_name.clone();
                        triage_details
                            .write()
                            .insert(identity.clone(), SystemTriageCacheEntry::Loading);
                        triage_hydration_tokens
                            .write()
                            .insert(identity.clone(), generation);
                        spawn(async move {
                            let result = poam_api::fetch_system_cve_triage_detail(
                                system_id, &cve_id, &package,
                            )
                            .await;
                            if triage_hydration_tokens.peek().get(&identity) != Some(&generation) {
                                return;
                            }
                            triage_hydration_tokens.write().remove(&identity);
                            let entry = match result {
                                Ok(detail)
                                    if system_triage_detail_matches(&identity, system_id, &detail) =>
                                {
                                    SystemTriageCacheEntry::Loaded(detail)
                                }
                                Ok(_) => SystemTriageCacheEntry::Unavailable(
                                    "The server returned triage for a different CVE, package, or system scope."
                                        .to_string(),
                                ),
                                Err(error) => SystemTriageCacheEntry::Unavailable(format!(
                                    "Authoritative triage is unavailable: {error}"
                                )),
                            };
                            triage_details.write().insert(identity, entry);
                        });
                    }
                }
            }
        }
    });

    let shown_package_count = filtered_groups.len();
    let shown_package_suffix = if shown_package_count == 1 { "" } else { "s" };
    let shown_finding_count = vulnerabilities.len() as i64;
    let total_findings = inventory_metadata.total_findings;
    let status_is_error = save_status
        .read()
        .as_ref()
        .map(|message| {
            message.starts_with("Failed")
                || message.contains("required")
                || message.contains("changed")
        })
        .unwrap_or(false);
    let (empty_title, empty_description) = cve_empty_state(current_state, read_only);
    let attempt_notice =
        newer_attempt_notice(inventory_attempt.as_ref(), inventory_source.as_ref());

    rsx! {
            div {
                style: "display:flex;flex-direction:column;gap:14px;",

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
                        if loading || error.is_some() || inventory_authority == Some(SystemCveInventoryAuthority::NoScan) {
                            "Inventory unavailable"
                        } else {
                            "{format_count(shown_finding_count)} of {format_count(total_findings)} shown · {format_count(shown_package_count as i64)} of {format_count(inventory_metadata.total_packages)} package{shown_package_suffix} loaded"
                            if let Some(source) = inventory_source.as_ref() { " · scan completed {source.completed_at}" }
                        }
                    }
                }

                if loading {
                    div { class: "empty", role: "status", "data-testid": "system-cves-loading",
                        crate::components::loading::DashboardLoadingSpinner { label: "Loading the selected inventory".to_string(), size: 36 }
                        div { "No scan is being started." }
                    }
                } else if let Some(message) = error.as_ref() {
                    div { class: "empty", role: "alert", "data-testid": "system-cves-error",
                        h3 { "Unable to load CVE inventory" }
                        div { "{message}" }
                        button { r#type: "button", class: "btn btn-ghost xs focus-ring", onclick: move |_| on_retry_read.call(()), "Retry inventory read" }
                    }
                } else if filtered_groups.is_empty() {
                    div {
                        class: "empty",
                        if inventory_source.is_none() {
                            h3 { "{empty_title}" }
                            div { "{empty_description}" }
                            if let Some(reason) = exact_authority_failure { div { "Exact proof: {exact_authority_reason_label(reason)}." } }
                        } else {
                            h3 { "No findings in the selected scan" }
                            if let Some(source) = inventory_source.as_ref() {
                                div { "Completed {source.completed_at} by {source.scanner_name}. This scan reported no eligible findings; it does not prove the host has no vulnerabilities." }
                            }
                            if let Some(message) = attempt_notice {
                                div { class: "sd-card-meta", "{message}" }
                            }
                        }
                    }
                } else {
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px; padding: 14px;",
                        if let Some(message) = attempt_notice {
                            div { class: "sd-card-meta", "{message}" }
                        }
                        for group in filtered_groups {
                            {
                                let group_key = group.canonical_package_name.clone();
                                let panel_id = package_disclosure_id(&group.canonical_package_name);
                                let is_open = *expanded_cve.read() == Some(group_key.clone());
                                let sev_color = package_group_color(&group);
                                let expanded_key = group_key.clone();
                                let hydration_targets = group.cves.clone();
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
                                                    let generation = (*triage_hydration_generation.peek()).wrapping_add(1);
                                                    triage_hydration_generation.set(generation);
                                                    triage_hydration_tokens.write().clear();
                                                    triage_details.write().retain(|_, entry| !matches!(entry, SystemTriageCacheEntry::Loading));
                                                    expanded_cve.set(None);
                                                } else {
                                                    expanded_cve.set(Some(expanded_key.clone()));
                                                    let generation = (*triage_hydration_generation.peek()).wrapping_add(1);
                                                    triage_hydration_generation.set(generation);
                                                    triage_hydration_tokens.write().clear();
                                                    triage_details.write().retain(|_, entry| !matches!(entry, SystemTriageCacheEntry::Loading));
                                                    let targets = hydration_targets
                                                        .iter()
                                                        .filter(|target| {
                                                            system_triage_row_state(
                                                                inventory_authority,
                                                                target,
                                                                system_id,
                                                                None,
                                                            ) == SystemTriageRowState::Review
                                                                && !triage_details
                                                                    .read()
                                                                    .contains_key(&target.stable_identity)
                                                        })
                                                        .cloned()
                                                        .collect::<Vec<_>>();
                                                    for target in targets {
                                                        let identity = target.stable_identity.clone();
                                                        let cve_id = target.cve_id.clone();
                                                        let package = identity.canonical_package_name.clone();
                                                        triage_details
                                                            .write()
                                                            .insert(identity.clone(), SystemTriageCacheEntry::Loading);
                                                        triage_hydration_tokens
                                                            .write()
                                                            .insert(identity.clone(), generation);
                                                        spawn(async move {
                                                            let result = poam_api::fetch_system_cve_triage_detail(
                                                                system_id,
                                                                &cve_id,
                                                                &package,
                                                            )
                                                            .await;
                                                            // CONCURRENCY: Each visible row owns one hydration token.
                                                            // A manual refetch removes only that row's token.
                                                            if triage_hydration_tokens
                                                                .peek()
                                                                .get(&identity)
                                                                != Some(&generation)
                                                            {
                                                                return;
                                                            }
                                                            triage_hydration_tokens.write().remove(&identity);
                                                            let entry = match result {
                                                                Ok(detail)
                                                                    if system_triage_detail_matches(
                                                                        &identity,
                                                                        system_id,
                                                                        &detail,
                                                                    ) =>
                                                                {
                                                                    SystemTriageCacheEntry::Loaded(detail)
                                                                }
                                                                Ok(_) => SystemTriageCacheEntry::Unavailable(
                                                                    "The server returned triage for a different CVE, package, or system scope."
                                                                        .to_string(),
                                                                ),
                                                                Err(error) => SystemTriageCacheEntry::Unavailable(
                                                                    format!("Authoritative triage is unavailable: {error}"),
                                                                ),
                                                            };
                                                            triage_details.write().insert(identity, entry);
                                                        });
                                                    }
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
                                                         th { "Triage" }
                                                         th { style: "text-align: right;", " " }
                                                    }
                                                }
                                                tbody {
                                                    for cve in group.cves.iter() {
                                                        {
                                                            let cvss_label = cve
                                                                .cvss_score
                                                                .map(|score| format!("{score:.1}"))
                                                                .unwrap_or_else(|| "—".to_string());
                                                             let row_key = format!(
                                                                "{}\0{}",
                                                                cve.stable_identity.canonical_cve_id,
                                                                cve.stable_identity.canonical_package_name
                                                             );
                                                             let target_key_for_dialog = inventory_target_key.clone();
                                                            let conflict_key = format!("{row_key}-conflict");
                                                            let cache_entry = triage_details
                                                                .read()
                                                                .get(&cve.stable_identity)
                                                                .cloned();
                                                            let cached_detail = match cache_entry.as_ref() {
                                                                  Some(SystemTriageCacheEntry::Loaded(detail))
                                                                      if system_triage_detail_matches(
                                                                          &cve.stable_identity,
                                                                          system_id,
                                                                          detail,
                                                                      ) => Some(detail),
                                                                  _ => None,
                                                            };
                                                            let row_opening = triage_opening
                                                                .read()
                                                                .as_ref()
                                                                == Some(&cve.stable_identity);
                                                            let triage_state = if row_opening
                                                                  || matches!(cache_entry.as_ref(), Some(SystemTriageCacheEntry::Loading))
                                                              {
                                                                  SystemTriageRowState::Loading
                                                              } else if matches!(
                                                                  cache_entry.as_ref(),
                                                                  Some(SystemTriageCacheEntry::Unavailable(_))
                                                                      | Some(SystemTriageCacheEntry::Loaded(_))
                                                              ) && cached_detail.is_none()
                                                              {
                                                                  SystemTriageRowState::LoadFailed
                                                              } else {
                                                                  system_triage_row_state(
                                                                      inventory_authority,
                                                                      cve,
                                                                      system_id,
                                                                      cached_detail,
                                                                  )
                                                            };
                                                            let triage_error = match cache_entry.as_ref() {
                                                                  Some(SystemTriageCacheEntry::Unavailable(message)) => {
                                                                      Some(message.as_str())
                                                                  }
                                                                  Some(SystemTriageCacheEntry::Loaded(_))
                                                                      if cached_detail.is_none() => Some(
                                                                          "Cached triage does not match this CVE, package, and system scope.",
                                                                      ),
                                                                  _ => None,
                                                            };
                                                            let triage_actionable = exact_remediation_allowed
                                                                  && system_triage_row_state(
                                                                      inventory_authority,
                                                                      cve,
                                                                      system_id,
                                                                      None,
                                                                   ) == SystemTriageRowState::Review;
                                                            let (action_icon, action_label) = system_triage_action(cached_detail);

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
                                                                            span { class: "chip chip-healthy", "{fixed_version_label(cve.fixed_version.as_deref(), true)}" }
                                                                        } else {
                                                                            span { class: "chip chip-unknown", "pending" }
                                                                         }
                                                                     }
                                                                     td {
                                                                          span {
                                                                              class: "chip {triage_state.chip_class()}",
                                                                              "data-testid": "system-cve-triage-state",
                                                                              title: triage_error.map(str::to_string).unwrap_or_else(|| system_triage_row_title(triage_state, cached_detail)),
                                                                              "{triage_state.label()}"
                                                                          }
                                                                     }
                                                                     td {
                                                                         div { class: "row-actions",
                                                                             if triage_actionable {
                                                                                 button {
                                                                                      class: "btn-icon focus-ring",
                                                                                      "data-testid": "system-cve-triage-open",
                                                                                      aria_label: "{action_label}",
                                                                                     aria_disabled: row_opening,
                                                                                     aria_busy: row_opening,
                                                                                      title: "{action_label}",
                                                                                      onclick: {
                                                                                          let target = cve.clone();
                                                                                          let start_key = target_key_for_dialog.clone();
                                                                                          move |_| {
                                                                                             // Keep the trigger focused while detail loads so the
                                                                                             // dialog can restore focus to it after unmount.
                                                                                             if row_opening {
                                                                                                 return;
                                                                                             }
                                                                                             let target = target.clone();
                                                                                             let identity = target.stable_identity.clone();
                                                                                             let cve_id = target.cve_id.clone();
                                                                                            let package = target.stable_identity.canonical_package_name.clone();
                                                                                              triage_target.set(Some(target));
                                                                                              dialog_start_key.set(Some(start_key.clone()));
                                                                                             triage_dialog_detail.set(None);
                                                                                            triage_hydration_tokens.write().remove(&identity);
                                                                                            triage_opening.set(Some(identity.clone()));
                                                                                            save_status.set(None);
                                                                                            let generation = (*triage_open_generation.peek()).wrapping_add(1);
                                                                                            triage_open_generation.set(generation);
                                                                                            spawn(async move {
                                                                                                let result = poam_api::fetch_system_cve_triage_detail(system_id, &cve_id, &package).await;
                                                                                                // CONCURRENCY: Only the newest row-open request can replace
                                                                                                // the selected authoritative triage detail.
                                                                                                if *triage_open_generation.peek() != generation {
                                                                                                    return;
                                                                                                }
                                                                                                match result {
                                                                                                     Ok(detail) if system_triage_detail_matches(&identity, system_id, &detail) => {
                                                                                                         triage_details.write().insert(identity, SystemTriageCacheEntry::Loaded(detail.clone()));
                                                                                                         triage_dialog_detail.set(Some(detail));
                                                                                                         triage_opening.set(None);
                                                                                                     }
                                                                                                     Ok(_) => {
                                                                                                         let message = "The server returned triage for a different CVE, package, or system scope.".to_string();
                                                                                                         triage_details.write().insert(identity, SystemTriageCacheEntry::Unavailable(message.clone()));
                                                                                                         triage_opening.set(None);
                                                                                                         triage_target.set(None);
                                                                                                         save_status.set(Some(format!("Failed to load authoritative triage: {message}")));
                                                                                                     }
                                                                                                     Err(error) => {
                                                                                                         triage_details.write().insert(identity, SystemTriageCacheEntry::Unavailable(format!("Authoritative triage is unavailable: {error}")));
                                                                                                         triage_opening.set(None);
                                                                                                         triage_target.set(None);
                                                                                                         save_status.set(Some(format!("Failed to load authoritative triage: {error}")));
                                                                                                     }
                                                                                                 }
                                                                                             });
                                                                                         }
                                                                                     },
                                                                                      Icon { name: action_icon, size: 14 }
                                                                                 }
                                                                             }
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
                                                                 if cve.remediation_conflict {
                                                                     tr { key: "{conflict_key}", td { colspan: "6", div { role: "alert", class: "sd-callout sd-callout-danger", "Conflicting server-issued remediation identities were returned for this package and CVE. No triage action is available until the evidence is refreshed." } } }
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
                        class: "sd-card-meta",
                        style: "padding: 0 14px 14px;",
                        "data-testid": "system-cves-continuation-error",
                        span { "More findings could not be loaded. The rows above are partial; retry the selected page without discarding them. {message}" }
                        button {
                            class: "btn btn-ghost xs focus-ring",
                            disabled: continuation_loading,
                            aria_label: "Retry loading more vulnerabilities",
                            onclick: move |_| on_load_more.call(()),
                            "Retry page"
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
            if let (Some(target), Some(detail)) = (triage_target(), triage_dialog_detail()) {
                {
                    let dialog_identity = target.stable_identity.clone();
                    rsx! {
                SystemCveTriageDialog {
                    key: "{detail.canonical_cve_id}|{detail.canonical_package_name}|{detail.scope.selected_system_id}",
                    system_id,
                    detail,
                    severity: target.severity.label().to_string(),
                    cvss_score: target.cvss_score,
                    fixed_version: target.fixed_version.clone(),
                    fix_available: target.has_fix,
                    submission_blocked: dialog_start_key.read().as_ref()
                        .filter(|start_key| *start_key != &inventory_target_key)
                        .map(|_| "Current evidence changed while this draft was open. Your edits are preserved; close this draft and reopen triage from the latest scan to continue.".to_string()),
                    on_close: move |_| {
                        let generation = (*triage_open_generation.peek()).wrapping_add(1);
                        triage_open_generation.set(generation);
                        triage_opening.set(None);
                        triage_dialog_detail.set(None);
                        triage_target.set(None);
                        dialog_start_key.set(None);
                    },
                    on_success: move |response: poam_api::SystemCveTriageResponse| {
                        if !system_triage_detail_matches(&dialog_identity, system_id, &response.detail) {
                            triage_details.write().insert(
                                dialog_identity.clone(),
                                SystemTriageCacheEntry::Unavailable(
                                    "The mutation response did not match the selected CVE, package, and system scope."
                                        .to_string(),
                                ),
                            );
                            triage_dialog_detail.set(None);
                            triage_target.set(None);
                            dialog_start_key.set(None);
                            save_status.set(Some("CVE evidence changed: the mutation response did not match the selected triage scope.".to_string()));
                            on_saved.call(());
                            return;
                        }
                        triage_details.write().insert(
                            dialog_identity.clone(),
                            SystemTriageCacheEntry::Loaded(response.detail),
                        );
                        triage_dialog_detail.set(None);
                        triage_target.set(None);
                        dialog_start_key.set(None);
                        save_status.set(Some("CVE triage updated. Accepted and scheduled states do not prove remediation or verification; closure requires later exact evidence.".to_string()));
                        on_saved.call(());
                        if let Some(poam_id) = response.poam_id {
                            on_open_poam.call(poam_id);
                        }
                    },
                    on_conflict: move |message: String| {
                        triage_dialog_detail.set(None);
                        triage_target.set(None);
                        dialog_start_key.set(None);
                        triage_details.write().clear();
                        save_status.set(Some(format!("CVE evidence changed: {message}")));
                        on_saved.call(());
                    },
                }
                    }
                }
             }
        }
    }
}

/// Returns the operator-facing name of one failed exact-remediation prerequisite.
pub(crate) fn exact_authority_reason_label(reason: ExactCveAuthorityFailureReason) -> &'static str {
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

// These states occupy the design's existing empty slot. None implies a clean
// scan: only a present completed source with zero findings may say that.
fn cve_empty_state(
    current: Option<SystemCveCurrentAuthorityState>,
    read_only: bool,
) -> (&'static str, &'static str) {
    match current {
        Some(SystemCveCurrentAuthorityState::UnmappedRunning) => (
            "Running configuration is unmapped",
            "The reported running output does not match a known configuration. No Current scan is available. Choose a known revision to inspect its own results.",
        ),
        Some(SystemCveCurrentAuthorityState::AmbiguousRunning) => (
            "Running target is ambiguous",
            "More than one registered target matches the reported running output. Current scan results cannot be selected.",
        ),
        Some(SystemCveCurrentAuthorityState::NoRunningReport) => (
            "No running configuration reported",
            "No usable running configuration has been reported. Known revisions can still be inspected.",
        ),
        Some(SystemCveCurrentAuthorityState::InvalidRunningReport) => (
            "Running target unavailable",
            "The latest running report has conflicting or incomplete target information. Current results are unavailable.",
        ),
        Some(
            SystemCveCurrentAuthorityState::MappedRunningNoScan
            | SystemCveCurrentAuthorityState::NoCurrentScan,
        ) => (
            "No completed CVE scan for this target",
            "The reported running target has no completed eligible scan. Another revision's results are not substituted; scanning cannot repair missing deployment proof.",
        ),
        Some(SystemCveCurrentAuthorityState::CurrentAuthorityUnavailable) => (
            "Running target unavailable",
            "The server could not prove Current identity. Choose a known revision to inspect its own results.",
        ),
        _ if read_only => (
            "No completed CVE scan for this target",
            "This selected revision has no completed scan. Another revision's results are not substituted.",
        ),
        _ => (
            "No completed CVE scan for this target",
            "No eligible completed scan exists for the selected target.",
        ),
    }
}

// The approved card has a source-local attempt notice. A missing source never
// becomes a clean scan, and an attempt cannot supply remediation authority.
fn newer_attempt_notice(
    attempt: Option<&SystemCveInventoryAttempt>,
    source: Option<&SystemCveInventorySource>,
) -> Option<&'static str> {
    let (attempt, source) = (attempt?, source?);
    // An attempt with a different ID is not necessarily newer than the
    // completed source: another active scan may have finished after it.
    if attempt.scan_id == source.scan_id
        || !attempt
            .created_at
            .is_some_and(|created_at| created_at > source.completed_at)
    {
        return None;
    }
    match attempt.status.as_str() {
        "failed" => Some("A newer scan failed. Showing the last completed scan for this target."),
        "pending" | "awaiting_build" | "awaiting_closure" => {
            Some("A newer scan is queued. Showing the last completed scan for this target.")
        }
        "in_progress" => {
            Some("A newer scan is in progress. Showing the last completed scan for this target.")
        }
        _ => None,
    }
}

fn inventory_allows_exact_remediation(
    authority: Option<SystemCveInventoryAuthority>,
    role_allows_mutation: bool,
) -> bool {
    role_allows_mutation && authority == Some(SystemCveInventoryAuthority::Exact)
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

        let normalized_fixed_version = vuln
            .fixed_version
            .as_ref()
            .filter(|version| !version.trim().is_empty())
            .cloned();
        let has_fix = normalized_fixed_version.is_some() || vuln.status == "fix_available";

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
            if existing.fixed_version.is_none() {
                existing.fixed_version = normalized_fixed_version.clone();
            }
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
                fixed_version: normalized_fixed_version,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemTriageRowState {
    Review,
    Loading,
    LoadFailed,
    Outstanding,
    Accepted,
    Scheduled,
    AcceptedEnvironment,
    ScheduledEnvironment,
    Legacy,
    ReadOnly,
    NoScan,
    Conflict,
    Whitelisted,
    Unavailable,
}

impl SystemTriageRowState {
    const fn label(self) -> &'static str {
        match self {
            Self::Review => "Review",
            Self::Loading => "Loading",
            Self::LoadFailed => "Load failed",
            Self::Outstanding => "Outstanding",
            Self::Accepted => "Accepted",
            Self::Scheduled => "Scheduled",
            Self::AcceptedEnvironment => "Accepted · env",
            Self::ScheduledEnvironment => "Scheduled · env",
            Self::Legacy => "Historical inventory",
            Self::ReadOnly => "Read-only",
            Self::NoScan => "No scan",
            Self::Conflict => "Conflict",
            Self::Whitelisted => "Whitelisted",
            Self::Unavailable => "Unavailable",
        }
    }

    const fn chip_class(self) -> &'static str {
        match self {
            Self::Accepted
            | Self::Scheduled
            | Self::AcceptedEnvironment
            | Self::ScheduledEnvironment => "chip-info",
            Self::Outstanding => "chip-critical",
            Self::Conflict => "chip-warning",
            Self::Review
            | Self::Loading
            | Self::LoadFailed
            | Self::Legacy
            | Self::ReadOnly
            | Self::NoScan
            | Self::Whitelisted
            | Self::Unavailable => "chip-unknown",
        }
    }
}

fn system_triage_detail_matches(
    identity: &SystemCveInventoryRowIdentity,
    system_id: Uuid,
    detail: &SystemCveTriageDetail,
) -> bool {
    detail.canonical_cve_id == identity.canonical_cve_id
        && detail.canonical_package_name == identity.canonical_package_name
        && detail.scope.selected_system_id == system_id
}

fn system_triage_action(detail: Option<&SystemCveTriageDetail>) -> (IconName, &'static str) {
    // A cached authoritative GET only hydrates the row. The effective
    // disposition, including an inherited environment decision, is the
    // server-owned evidence of a persisted triage decision.
    if detail.is_some_and(|detail| detail.effective_disposition.is_some()) {
        (IconName::File, "Edit triage")
    } else {
        (
            IconName::Shield,
            "Triage — accept the risk or schedule a patch",
        )
    }
}

fn system_triage_row_state(
    authority: Option<SystemCveInventoryAuthority>,
    cve: &PackageCve,
    system_id: Uuid,
    detail: Option<&SystemCveTriageDetail>,
) -> SystemTriageRowState {
    match authority {
        Some(SystemCveInventoryAuthority::Legacy) => return SystemTriageRowState::Legacy,
        Some(SystemCveInventoryAuthority::MappedRunning) => return SystemTriageRowState::ReadOnly,
        Some(SystemCveInventoryAuthority::NoScan) => return SystemTriageRowState::NoScan,
        None => return SystemTriageRowState::Unavailable,
        Some(SystemCveInventoryAuthority::Exact) => {}
    }
    if cve.remediation_conflict {
        return SystemTriageRowState::Conflict;
    }
    let Some(remediation) = cve.remediation.as_ref() else {
        return SystemTriageRowState::Unavailable;
    };
    if remediation.observation.system_id != system_id
        || remediation.observation.canonical_cve_id != cve.cve_id
        || remediation.observed_package_name.is_empty()
        || remediation.observed_package_version != cve.installed_version
    {
        return SystemTriageRowState::Conflict;
    }
    if remediation.is_whitelisted {
        return SystemTriageRowState::Whitelisted;
    }
    let detail = detail
        .filter(|detail| system_triage_detail_matches(&cve.stable_identity, system_id, detail));
    match detail.map(|detail| (&detail.effective_disposition, detail.effective_source)) {
        Some((
            Some(poam_api::CveEnvironmentDisposition::Accepted { .. }),
            poam_api::SystemCveEffectiveDispositionSource::Host,
        )) => SystemTriageRowState::Accepted,
        Some((
            Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }),
            poam_api::SystemCveEffectiveDispositionSource::Host,
        )) => SystemTriageRowState::Scheduled,
        Some((
            Some(poam_api::CveEnvironmentDisposition::Accepted { .. }),
            poam_api::SystemCveEffectiveDispositionSource::Environment,
        )) => SystemTriageRowState::AcceptedEnvironment,
        Some((
            Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }),
            poam_api::SystemCveEffectiveDispositionSource::Environment,
        )) => SystemTriageRowState::ScheduledEnvironment,
        Some(_) => SystemTriageRowState::Outstanding,
        None => SystemTriageRowState::Review,
    }
}

fn system_triage_row_title(
    state: SystemTriageRowState,
    detail: Option<&SystemCveTriageDetail>,
) -> String {
    let Some(detail) = detail else {
        return state.label().to_string();
    };
    match state {
        SystemTriageRowState::Accepted => format!(
            "Risk accepted for this host ({})",
            detail.scope.selected_system_hostname
        ),
        SystemTriageRowState::Scheduled => format!(
            "Patch scheduled for this host ({})",
            detail.scope.selected_system_hostname
        ),
        SystemTriageRowState::AcceptedEnvironment => {
            format!("Risk accepted for all of {}", detail.scope.environment_name)
        }
        SystemTriageRowState::ScheduledEnvironment => format!(
            "Patch scheduled for all of {}",
            detail.scope.environment_name
        ),
        _ => state.label().to_string(),
    }
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
            current_state: Some(SystemCveCurrentAuthorityState::ExactCurrentScan),
            system_id: Some(Uuid::from_u128(1)),
            running_target: Some(SystemCveRunningTarget {
                derivation_id: 42,
                generation: Some(7),
                commit_hash: "a".repeat(40),
                reported_at: chrono::DateTime::parse_from_rfc3339("2026-09-14T20:00:00Z")
                    .expect("test observation time should parse")
                    .with_timezone(&chrono::Utc),
            }),
            source: Some(SystemCveInventorySource {
                scan_id,
                scanner_name: "vulnix".into(),
                scanner_version: Some("1.10.1".into()),
                completed_at: chrono::DateTime::parse_from_rfc3339("2026-09-14T21:00:00Z")
                    .expect("test timestamp should parse")
                    .with_timezone(&chrono::Utc),
            }),
            attempt: None,
            selection: SystemCveInventorySelection::Current,
            evidence_representation: Some(SystemCveEvidenceRepresentation::Schema1Observations),
            read_only: false,
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
    fn attempt_notice_never_promotes_an_attempt_to_a_completed_source() {
        let source_id = Uuid::from_u128(41);
        let page = inventory_page(source_id, vec![], 0, None);
        let source = page.source.as_ref();
        let mut attempt = SystemCveInventoryAttempt {
            scan_id: Uuid::from_u128(42),
            derivation_id: 42,
            status: "failed".to_string(),
            created_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-09-15T10:00:00Z")
                    .expect("attempt timestamp should parse")
                    .with_timezone(&chrono::Utc),
            ),
        };
        assert_eq!(
            super::newer_attempt_notice(Some(&attempt), source),
            Some("A newer scan failed. Showing the last completed scan for this target.")
        );
        attempt.status = "pending".to_string();
        assert_eq!(
            super::newer_attempt_notice(Some(&attempt), source),
            Some("A newer scan is queued. Showing the last completed scan for this target.")
        );
        attempt.status = "in_progress".to_string();
        assert_eq!(
            super::newer_attempt_notice(Some(&attempt), source),
            Some("A newer scan is in progress. Showing the last completed scan for this target.")
        );
        assert_eq!(super::newer_attempt_notice(Some(&attempt), None), None);
        attempt.created_at = Some(
            chrono::DateTime::parse_from_rfc3339("2026-09-14T20:00:00Z")
                .expect("older timestamp should parse")
                .with_timezone(&chrono::Utc),
        );
        assert_eq!(super::newer_attempt_notice(Some(&attempt), source), None);
        attempt.scan_id = source_id;
        assert_eq!(super::newer_attempt_notice(Some(&attempt), source), None);
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
    fn pagination_updates_newest_attempt_without_replacing_completed_source() {
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
        let mut next_page = inventory_page(scan_id, vec![], 2, None);
        next_page.attempt = Some(SystemCveInventoryAttempt {
            scan_id: Uuid::from_u128(441),
            derivation_id: 42,
            status: "failed".into(),
            created_at: None,
        });
        assert!(state.complete_continuation(&request, next_page));
        let inventory = state
            .inventory
            .expect("completed source must remain loaded");
        assert_eq!(
            inventory.source.as_ref().map(|source| source.scan_id),
            Some(scan_id)
        );
        assert_eq!(
            inventory.attempt.as_ref().map(|attempt| attempt.scan_id),
            Some(Uuid::from_u128(441))
        );
        assert_eq!(inventory.vulnerabilities.len(), 1);
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
    fn pagination_rejects_changed_system_target_and_read_tier() {
        let scan_id = Uuid::from_u128(440);
        for changed in [
            {
                let mut page = inventory_page(scan_id, vec![], 2, None);
                page.system_id = Some(Uuid::from_u128(2));
                page
            },
            {
                let mut page = inventory_page(scan_id, vec![], 2, None);
                page.selection = SystemCveInventorySelection::ExactDerivation { derivation_id: 42 };
                page
            },
            {
                let mut page = inventory_page(scan_id, vec![], 2, None);
                page.read_only = true;
                page.authority = SystemCveInventoryAuthority::MappedRunning;
                page.current_state =
                    Some(SystemCveCurrentAuthorityState::MappedRunningReadOnlyScan);
                page
            },
            {
                let mut page = inventory_page(scan_id, vec![], 2, None);
                page.running_target.as_mut().unwrap().derivation_id = 99;
                page
            },
        ] {
            let mut state = CveInventoryPaginationState::default();
            state.reset(Some(inventory_page(
                scan_id,
                vec![vulnerability("3.4.1", None)],
                2,
                Some("page-2"),
            )));
            let request = state.begin_continuation().expect("page must be pending");
            assert!(!state.complete_continuation(&request, changed));
            assert_eq!(state.inventory.as_ref().unwrap().vulnerabilities.len(), 1);
        }
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
    fn package_groups_ignore_blank_fixed_versions_and_preserve_available_state() {
        let mut blank = vulnerability("3.4.1", None);
        blank.fixed_version = Some("  ".into());
        blank.status = "fix_available".into();
        let mut exact = vulnerability("3.4.1", None);
        exact.fixed_version = Some("3.4.3".into());
        let groups = group_vulnerabilities_by_package(&[blank.clone(), exact]);

        assert!(groups[0].cves[0].has_fix);
        assert_eq!(groups[0].cves[0].fixed_version.as_deref(), Some("3.4.3"));

        let groups = group_vulnerabilities_by_package(&[blank]);
        assert!(groups[0].cves[0].has_fix);
        assert_eq!(groups[0].cves[0].fixed_version, None);
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
        assert!(groups[0].cves.iter().all(|cve| {
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                None,
            ) == SystemTriageRowState::Conflict
        }));
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
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                None,
            ),
            SystemTriageRowState::Review
        );
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                Uuid::from_u128(9),
                None,
            ),
            SystemTriageRowState::Conflict
        );
    }

    #[test]
    fn inventory_authority_limits_exact_triage() {
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
    }

    #[test]
    fn triage_labels_preserve_inventory_authority_and_require_detail_for_disposition() {
        assert_eq!(SystemTriageRowState::Review.label(), "Review");
        assert_eq!(SystemTriageRowState::Outstanding.label(), "Outstanding");
        assert_eq!(SystemTriageRowState::Accepted.label(), "Accepted");
        assert_eq!(SystemTriageRowState::Scheduled.label(), "Scheduled");
        assert_eq!(SystemTriageRowState::Conflict.label(), "Conflict");
        assert_eq!(SystemTriageRowState::Whitelisted.label(), "Whitelisted");

        let system_id = Uuid::from_u128(1);
        let relationship = relationship(observation(
            system_id,
            Uuid::from_u128(2),
            "/nix/store/opaque-occurrence",
        ));
        let groups =
            group_vulnerabilities_by_package(&[vulnerability("3.4.1", Some(relationship))]);
        let cve = &groups[0].cves[0];

        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                None
            ),
            SystemTriageRowState::Review
        );
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Legacy),
                cve,
                system_id,
                None
            ),
            SystemTriageRowState::Legacy
        );
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::NoScan),
                cve,
                system_id,
                None
            ),
            SystemTriageRowState::NoScan
        );

        let accepted: SystemCveTriageDetail = serde_json::from_value(serde_json::json!({
            "canonical_cve_id": "CVE-2026-1000",
            "canonical_package_name": "openssl",
            "scope": {
                "kind": "current_exact_affected_hosts_in_environment",
                "selected_system_id": system_id,
                "selected_system_hostname": "prod-web-01",
                "environment_id": Uuid::from_u128(3),
                "environment_name": "Production",
                "exact_affected_system_count": 2
            },
            "systems": [],
            "host_disposition": {
                "state": "accepted",
                "justification": "Compensating controls are active.",
                "review_date": null,
                "actor": { "user_id": Uuid::from_u128(4), "display": "Operator" },
                "accepted_at": "2026-09-20T12:00:00Z"
            },
            "environment_disposition": null,
            "effective_disposition": {
                "state": "accepted",
                "justification": "Compensating controls are active.",
                "review_date": null,
                "actor": { "user_id": Uuid::from_u128(4), "display": "Operator" },
                "accepted_at": "2026-09-20T12:00:00Z"
            },
            "effective_source": "host",
            "disposition": {
                "state": "accepted",
                "justification": "Compensating controls are active.",
                "review_date": null,
                "actor": { "user_id": Uuid::from_u128(4), "display": "Operator" },
                "accepted_at": "2026-09-20T12:00:00Z"
            }
        }))
        .unwrap();
        let mut outstanding = accepted.clone();
        outstanding.host_disposition = None;
        outstanding.effective_disposition = None;
        outstanding.disposition = None;
        outstanding.effective_source = poam_api::SystemCveEffectiveDispositionSource::None;
        assert_eq!(system_triage_action(None).0, IconName::Shield);
        assert_eq!(system_triage_action(Some(&outstanding)).0, IconName::Shield);
        assert_eq!(
            system_triage_action(Some(&accepted)),
            (IconName::File, "Edit triage")
        );
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                Some(&accepted)
            ),
            SystemTriageRowState::Accepted
        );
        assert!(system_triage_detail_matches(
            &cve.stable_identity,
            system_id,
            &accepted
        ));
        assert_eq!(
            system_triage_row_title(SystemTriageRowState::Accepted, Some(&accepted)),
            "Risk accepted for this host (prod-web-01)"
        );

        let mut inherited = accepted.clone();
        inherited.host_disposition = None;
        inherited.environment_disposition = inherited.effective_disposition.clone();
        inherited.effective_source = poam_api::SystemCveEffectiveDispositionSource::Environment;
        assert_eq!(system_triage_action(Some(&inherited)).0, IconName::File);
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                Some(&inherited)
            ),
            SystemTriageRowState::AcceptedEnvironment
        );
        assert_eq!(
            SystemTriageRowState::AcceptedEnvironment.label(),
            "Accepted · env"
        );
        assert_eq!(
            system_triage_row_title(SystemTriageRowState::AcceptedEnvironment, Some(&inherited)),
            "Risk accepted for all of Production"
        );

        let scheduled_disposition: poam_api::CveEnvironmentDisposition =
            serde_json::from_value(serde_json::json!({
                "state": "scheduled",
                "poam_id": Uuid::from_u128(5),
                "poam": null,
                "actor": { "user_id": Uuid::from_u128(4), "display": "Operator" },
                "scheduled_at": "2026-09-20T12:00:00Z"
            }))
            .unwrap();
        inherited.environment_disposition = Some(scheduled_disposition.clone());
        inherited.effective_disposition = Some(scheduled_disposition);
        inherited.disposition = inherited.effective_disposition.clone();
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                Some(&inherited)
            ),
            SystemTriageRowState::ScheduledEnvironment
        );
        assert_eq!(
            SystemTriageRowState::ScheduledEnvironment.label(),
            "Scheduled · env"
        );
        assert_eq!(
            system_triage_row_title(SystemTriageRowState::ScheduledEnvironment, Some(&inherited)),
            "Patch scheduled for all of Production"
        );

        let mut wrong_package = accepted.clone();
        wrong_package.canonical_package_name = "libressl".to_string();
        assert!(!system_triage_detail_matches(
            &cve.stable_identity,
            system_id,
            &wrong_package
        ));
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                Some(&wrong_package)
            ),
            SystemTriageRowState::Review
        );

        let mut wrong_system = accepted.clone();
        wrong_system.scope.selected_system_id = Uuid::from_u128(99);
        assert!(!system_triage_detail_matches(
            &cve.stable_identity,
            system_id,
            &wrong_system
        ));
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                cve,
                system_id,
                Some(&wrong_system)
            ),
            SystemTriageRowState::Review
        );

        let mut justified = cve.clone();
        justified.remediation.as_mut().unwrap().is_justified = true;
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                &justified,
                system_id,
                None
            ),
            SystemTriageRowState::Review
        );
        let mut whitelisted = cve.clone();
        whitelisted.remediation.as_mut().unwrap().is_whitelisted = true;
        assert_eq!(
            system_triage_row_state(
                Some(SystemCveInventoryAuthority::Exact),
                &whitelisted,
                system_id,
                None
            ),
            SystemTriageRowState::Whitelisted
        );
    }
}
