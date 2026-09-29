//! Presents authorized fleet CVE inventory and exact package triage.
//!
//! Paged pair identities drive rows and drawer URLs. Fleet-wide grouped
//! aggregates may label package headers only for the same filters; scoped or
//! missing groups show loaded-pair counts without inferring distinct hosts.
//! Row selection is local presentation state and never submits a bulk mutation.

use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

use crate::alerts::{NAV_BADGES, acknowledge_with_cursor_and_ids, should_flash};

use crate::api::client;
use crate::api::models::{
    CveAffectedSystemDetail, CveFilters, CveFleetStats, CveInventoryGroup, CveInventoryMember,
    CveInventoryPairPage, CveInventoryQuery, CveListItem, CvePackageGroup, EnvironmentSummary,
    FleetCveInventorySection, SystemCveInventoryAuthority,
};
use crate::components::chips::EnvBadge;
use crate::components::cve::triage::{
    CveTriageDraft, EnvironmentTriageChoice, catalog_contains_assignee, fixed_version_label,
};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::components::layout::Card;
use crate::components::notifications::Toast;
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::views::poam_api::{self, PoamApiError};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExactCveSelection {
    cve_id: String,
    package: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CveSeenState {
    user_id: String,
    pairs: BTreeSet<(String, String)>,
}

fn seen_storage_key(user_id: &str) -> String {
    format!("cf.cves.seen.v1.{user_id}")
}

fn authenticated_user_id(auth: &Option<crate::api::models::AuthContext>) -> Option<String> {
    auth.as_ref()
        .filter(|context| context.is_authenticated)
        .and_then(|context| context.user.as_ref())
        .map(|user| user.id.clone())
        .filter(|id| !id.is_empty())
}

fn seen_for_user<'a>(
    state: Option<&'a CveSeenState>,
    user_id: Option<&str>,
) -> Option<&'a CveSeenState> {
    state.filter(|seen| user_id.is_some_and(|id| !id.is_empty() && seen.user_id == id))
}

fn recently_observed_unseen(
    item: &CveListItem,
    now: DateTime<Utc>,
    seen: Option<&CveSeenState>,
) -> bool {
    let (Some(package), Some(last_seen), Some(seen)) =
        (item.package_name.as_ref(), item.last_seen, seen)
    else {
        return false;
    };
    let elapsed = now.signed_duration_since(last_seen);
    elapsed >= Duration::zero()
        && elapsed < Duration::hours(24)
        && !seen.pairs.contains(&(item.cve_id.clone(), package.clone()))
}

fn persist_seen_pairs(
    mut state: Signal<Option<CveSeenState>>,
    user_id: Option<&str>,
    selections: &[ExactCveSelection],
) {
    if selections.is_empty() {
        return;
    }
    let Some(user_id) = user_id.filter(|id| !id.is_empty()) else {
        return;
    };
    let Some(mut current) = seen_for_user(state.peek().as_ref(), Some(user_id)).cloned() else {
        return;
    };
    let key = seen_storage_key(user_id);
    if let Ok(stored) = LocalStorage::get::<BTreeSet<(String, String)>>(&key) {
        current.pairs.extend(stored);
    }
    for selection in selections {
        current
            .pairs
            .insert((selection.cve_id.clone(), selection.package.clone()));
    }
    if LocalStorage::set(&key, &current.pairs).is_ok() {
        state.set(Some(current));
    }
}

fn triage_status_presentation(status: &str) -> (&'static str, &'static str, &'static str) {
    match status {
        "accepted" => ("accepted", "chip-info", "Risk accepted"),
        "scheduled" => (
            "patch scheduled",
            "chip-info",
            "Patch remediation is scheduled through POA&M",
        ),
        "inventory_only" => (
            "inventory only",
            "chip-unknown",
            "Read-only scheduled configuration or historical evidence; not outstanding",
        ),
        _ => (
            "outstanding",
            "chip-critical",
            "Current exposure needs triage",
        ),
    }
}

fn inventory_section_label(section: FleetCveInventorySection) -> &'static str {
    match section {
        FleetCveInventorySection::Current => "Current exposure",
        FleetCveInventorySection::ScheduledDeploymentTarget => "Scheduled configuration exposure",
        FleetCveInventorySection::Historical => "Historical evidence",
    }
}

fn inventory_section_row_label(section: FleetCveInventorySection) -> &'static str {
    match section {
        FleetCveInventorySection::Current => "CURRENT",
        FleetCveInventorySection::ScheduledDeploymentTarget => "SCHEDULED CONFIGURATION",
        FleetCveInventorySection::Historical => "HISTORICAL",
    }
}

fn inventory_section_value(section: FleetCveInventorySection) -> &'static str {
    match section {
        FleetCveInventorySection::Current => "current",
        FleetCveInventorySection::ScheduledDeploymentTarget => "scheduled_deployment_target",
        FleetCveInventorySection::Historical => "historical",
    }
}

fn systems_in_inventory_section(
    systems: &[CveAffectedSystemDetail],
    section: FleetCveInventorySection,
) -> Vec<CveAffectedSystemDetail> {
    systems
        .iter()
        .filter(|system| system.inventory_section == section)
        .cloned()
        .collect()
}

fn environment_triage_eligible(environment: &poam_api::CveAffectedEnvironment) -> bool {
    environment.inventory_counts().0 > 0 && environment.exact_affected_system_count > 0
}

fn fleet_triage_draft(detail: &poam_api::FleetCveDetail) -> CveTriageDraft {
    let eligible_environment_ids = detail
        .environments
        .iter()
        .filter(|environment| environment_triage_eligible(environment))
        .map(|environment| environment.environment_id)
        .collect::<Vec<_>>();
    let mut draft = CveTriageDraft::from_fleet_detail(detail);
    draft
        .environments
        .retain(|environment| eligible_environment_ids.contains(&environment.environment_id));
    draft
}

fn selection_from_query() -> Option<ExactCveSelection> {
    Some(ExactCveSelection {
        cve_id: query_param("cve")?,
        package: query_param("cve_package")?,
    })
}

fn focused_cve_from_query() -> Option<String> {
    query_param("focus_cve").filter(|value| is_canonical_cve_id(value))
}

fn is_canonical_cve_id(value: &str) -> bool {
    let Some((prefix, remainder)) = value.split_once('-') else {
        return false;
    };
    let Some((year, sequence)) = remainder.split_once('-') else {
        return false;
    };
    prefix == "CVE"
        && year.len() == 4
        && year.bytes().all(|byte| byte.is_ascii_digit())
        && year.parse::<u16>().is_ok_and(|year| year >= 1999)
        && sequence.len() >= 4
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

// Notification focus can resolve evidence in any inventory section, but a
// metadata-only CVE cannot identify a package drawer or an affected inventory row.
fn has_retained_package_evidence(item: &CveListItem) -> bool {
    let (current, scheduled, historical) = item.inventory_counts();
    current > 0 || scheduled > 0 || historical > 0
}

fn unique_retained_package_for_cve(cve_id: &str, items: &[CveListItem]) -> Option<String> {
    let mut packages = items
        .iter()
        .filter(|item| item.cve_id == cve_id && has_retained_package_evidence(item))
        .filter_map(|item| item.package_name.as_deref())
        .collect::<Vec<_>>();
    packages.sort_unstable();
    packages.dedup();
    (packages.len() == 1).then(|| packages[0].to_string())
}

fn complete_focus_package(page: &CveInventoryPairPage, focused: &str) -> Option<String> {
    (page.next_offset.is_none() && page.total == page.items.len() as i64)
        .then(|| unique_retained_package_for_cve(focused, &page.items))
        .flatten()
}

async fn resolve_focused_package(
    mut query: CveInventoryQuery,
    focused: &str,
) -> Result<Option<String>, client::ApiClientError> {
    let mut total = None;
    let mut single_page_package = None;
    let mut pages = 0;
    loop {
        if pages == 50 {
            return Err(client::ApiClientError::Deserialize(
                "Focused CVE exceeds the bounded discovery window".into(),
            ));
        }
        pages += 1;
        let page = client::fetch_cve_inventory_pairs(&query).await?;
        if total.is_some_and(|previous| previous != page.total)
            || page.items.is_empty() && page.next_offset.is_some()
        {
            return Err(client::ApiClientError::Deserialize(
                "Focused CVE pages changed during loading".into(),
            ));
        }
        total = Some(page.total);
        if pages == 1 {
            single_page_package = complete_focus_package(&page, focused);
        }
        let Some(next) = page.next_offset else { break };
        if next <= query.offset
            || next != query.offset + page.items.len() as i64
            || next > page.total
        {
            return Err(client::ApiClientError::Deserialize(
                "Invalid focused CVE continuation".into(),
            ));
        }
        // Offset pages do not share a snapshot. Read all pages for discovery,
        // but never auto-open from a collection that could shift between pages.
        query.offset = next;
    }
    // Offset pages do not share a snapshot. A multi-page search remains a
    // paged focused list, even if its loaded rows contain just one package.
    Ok(if pages == 1 {
        single_page_package
    } else {
        None
    })
}

fn query_param(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    let query = search.trim_start_matches('?');
    if query.is_empty() {
        return None;
    }

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key == name {
            return js_sys::decode_uri_component(value)
                .ok()
                .map(|v| v.as_string().unwrap_or_default());
        }
    }

    None
}

const SUCCESS_TOAST_DURATION_MS: u32 = 3000;

#[derive(Clone, Copy)]
struct ToastPublication {
    generation: u64,
    auto_dismiss: bool,
}

#[derive(Default)]
struct ToastLifecycle {
    generation: u64,
}

impl ToastLifecycle {
    fn publish(&mut self, is_success: bool) -> ToastPublication {
        self.generation = self.generation.wrapping_add(1);
        ToastPublication {
            generation: self.generation,
            auto_dismiss: is_success,
        }
    }

    fn dismiss(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn expire(&mut self, publication: ToastPublication) -> bool {
        if publication.auto_dismiss && self.generation == publication.generation {
            self.dismiss();
            true
        } else {
            false
        }
    }
}

fn sync_cve_url_query(
    severity: Option<&str>,
    fix_status: Option<&str>,
    triage_status: Option<&str>,
    package: Option<&str>,
    search: Option<&str>,
    focus_cve: Option<&str>,
    sort: &str,
    view: &str,
    environment_id: Option<crate::api::models::Uuid>,
    selection: Option<&ExactCveSelection>,
    push_history: bool,
) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let mut parts: Vec<String> = Vec::new();
    let push = |parts: &mut Vec<String>, key: &str, value: &str| {
        if !value.trim().is_empty() {
            let encoded: String = js_sys::encode_uri_component(value).into();
            parts.push(format!("{key}={encoded}"));
        }
    };

    if let Some(v) = severity {
        push(&mut parts, "severity", v);
    }
    if let Some(v) = fix_status {
        push(&mut parts, "fix_status", v);
    }
    if let Some(v) = triage_status {
        push(&mut parts, "triage_status", v);
    }
    if let Some(v) = package {
        push(&mut parts, "package", v);
    }
    if let Some(v) = search {
        push(&mut parts, "search", v);
    }
    if let Some(v) = focus_cve {
        push(&mut parts, "focus_cve", v);
    }
    if sort != "severity" {
        push(&mut parts, "sort", sort);
    }
    if view != "grouped" {
        push(&mut parts, "view", view);
    }
    if let Some(id) = environment_id {
        push(&mut parts, "environment_id", &id.to_string());
    }
    if let Some(selection) = selection {
        push(&mut parts, "cve", &selection.cve_id);
        push(&mut parts, "cve_package", &selection.package);
    }

    let query = if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    };

    let pathname = window
        .location()
        .pathname()
        .ok()
        .unwrap_or_else(|| "/cves".to_string());
    if let Ok(history) = window.history() {
        let url = format!("{pathname}{query}");
        if push_history {
            let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        } else {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }
}

fn sync_cve_url_state(
    severity: Option<String>,
    fix_status: Option<String>,
    triage_status: Option<String>,
    package: Option<String>,
    search: String,
    focus_cve: Option<String>,
    sort: String,
    view: String,
    environment_id: Option<crate::api::models::Uuid>,
    selection: Option<&ExactCveSelection>,
    push_history: bool,
) {
    sync_cve_url_query(
        severity.as_deref(),
        fix_status.as_deref(),
        triage_status.as_deref(),
        package.as_deref(),
        (!search.trim().is_empty()).then_some(search.as_str()),
        focus_cve.as_deref(),
        &sort,
        &view,
        environment_id,
        selection,
        push_history,
    );
}

/// Renders the fleet CVE dashboard and its reload-safe exact-CVE selection.
///
/// `query` makes filter and drawer state part of the Dioxus route during a
/// direct page load. The view reads the browser URL so later selection changes
/// and `popstate` events remain authoritative.
#[component]
pub fn CvesView(query: String) -> Element {
    let _ = query;
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let active_user = authenticated_user_id(&app_state.read().auth);
    let initial_severity = query_param("severity");
    let initial_fix = query_param("fix_status").or_else(|| query_param("fix"));
    let initial_triage = query_param("triage_status").or_else(|| query_param("triage"));
    let initial_package = query_param("package");
    let initial_focus_cve = focused_cve_from_query();
    let initial_search = initial_focus_cve
        .clone()
        .or_else(|| query_param("search"))
        .unwrap_or_default();
    let initial_sort = query_param("sort").unwrap_or_else(|| "severity".to_string());
    let initial_view = query_param("view").unwrap_or_else(|| {
        if initial_focus_cve.is_some() {
            "flat".to_string()
        } else {
            "grouped".to_string()
        }
    });
    let initial_selection = selection_from_query();
    let initial_environment = query_param("environment_id").and_then(|id| id.parse().ok());

    // Filter state
    let mut severity_filter = use_signal(move || initial_severity.clone());
    let mut fix_status_filter = use_signal(move || initial_fix.clone());
    let mut triage_status_filter = use_signal(move || initial_triage.clone());
    let mut package_filter = use_signal(move || initial_package.clone());
    let mut search_query = use_signal(move || initial_search.clone());
    let mut focus_cve = use_signal(move || initial_focus_cve.clone());
    let mut focus_resolved_query = use_signal(|| None::<CveInventoryQuery>);
    let mut sort_by = use_signal(move || initial_sort.clone());
    let mut view_mode = use_signal(move || initial_view.clone()); // "flat" or "grouped"
    let mut environment_filter = use_signal(move || initial_environment);
    let mut selected_cve = use_signal(move || initial_selection.clone());
    let mut seen_state = use_signal(|| None::<CveSeenState>);
    let mut seen_clock = use_signal(Utc::now);
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(60_000).await;
                seen_clock.set(Utc::now());
            }
        });
    });
    let mut selection_hydrated = use_signal(|| false);
    use_effect(move || {
        let user = authenticated_user_id(&app_state.read().auth);
        let Some(user_id) = user else {
            if seen_state.peek().is_some() {
                seen_state.set(None);
            }
            return;
        };
        if seen_state
            .peek()
            .as_ref()
            .is_some_and(|seen| seen.user_id == user_id)
        {
            return;
        }
        let pairs = LocalStorage::get(&seen_storage_key(&user_id)).unwrap_or_default();
        seen_state.set(Some(CveSeenState { user_id, pairs }));
    });
    let current_seen = seen_for_user(seen_state.read().as_ref(), active_user.as_deref()).cloned();
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None);
    // CONCURRENCY: Publishing or dismissing feedback advances the lifecycle.
    // A success timer can clear only the publication that created the timer.
    let mut toast_lifecycle = use_signal(ToastLifecycle::default);
    let mut fleet_rescan_pending = use_signal(|| false);
    let mut export_open = use_signal(|| false);

    // Attention flash signal for the Critical stat card (set by the use_effect after stats resolves).
    let mut flash_crit_signal = use_signal(|| false);
    let flash_crit = flash_crit_signal();

    use_effect(move || {
        if !selection_hydrated() {
            // CONCURRENCY: The first effect can run before the mounted URL
            // selection reaches component state. Hydrate and yield so this
            // run cannot erase the deep link with `replaceState`.
            selected_cve.set(selection_from_query());
            selection_hydrated.set(true);
            return;
        }

        let severity = severity_filter();
        let fix_status = fix_status_filter();
        let triage_status = triage_status_filter();
        let package = package_filter();
        let search = search_query();
        let sort = sort_by();
        let view = view_mode();
        let selection = selected_cve();

        sync_cve_url_query(
            severity.as_deref(),
            fix_status.as_deref(),
            triage_status.as_deref(),
            package.as_deref(),
            if search.trim().is_empty() {
                None
            } else {
                Some(search.as_str())
            },
            focus_cve().as_deref(),
            &sort,
            &view,
            environment_filter(),
            selection.as_ref(),
            false,
        );
    });

    #[cfg(target_arch = "wasm32")]
    {
        let popstate_listener = use_hook(|| {
            let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                selected_cve.set(selection_from_query());
                severity_filter.set(query_param("severity"));
                fix_status_filter.set(query_param("fix_status").or_else(|| query_param("fix")));
                triage_status_filter
                    .set(query_param("triage_status").or_else(|| query_param("triage")));
                package_filter.set(query_param("package"));
                environment_filter
                    .set(query_param("environment_id").and_then(|id| id.parse().ok()));
                sort_by.set(query_param("sort").unwrap_or_else(|| "severity".into()));
                view_mode.set(query_param("view").unwrap_or_else(|| "grouped".into()));
                search_query.set(query_param("search").unwrap_or_default());
                let focused = focused_cve_from_query();
                focus_cve.set(focused.clone());
                focus_resolved_query.set(None);
                if let Some(focused) = focused {
                    search_query.set(focused);
                    if query_param("view").is_none() {
                        view_mode.set("flat".to_string());
                    }
                }
            });
            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "popstate",
                    callback.as_ref().unchecked_ref(),
                );
            }
            Rc::new(callback)
        });
        let listener_for_drop = popstate_listener.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "popstate",
                    listener_for_drop.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }

    // Data resources
    let stats = use_resource(move || async move { client::fetch_cve_fleet_stats().await });

    // Attention flash for the Critical stat card when critical CVEs exist (TASK-385).
    // Must be placed after `stats` is declared so it can be captured in the closure.
    use_effect(move || {
        if let Some(Ok(s)) = stats.read().as_ref() {
            let crit_count = s.critical as i64;
            if should_flash("cves", crit_count > 0) {
                flash_crit_signal.set(true);
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(3200).await;
                    flash_crit_signal.set(false);
                });
            }
        }
    });

    let package_names =
        use_resource(move || async move { client::fetch_cve_package_names().await });
    let environments = use_resource(|| async move { client::fetch_environments().await });

    let pair_metadata = use_resource(move || {
        let query = CveInventoryQuery {
            group_by: "environment".into(),
            environment_id: environment_filter(),
            group_id: None,
            filters: CveFilters {
                severity: severity_filter(),
                fix_status: fix_status_filter(),
                triage_status: triage_status_filter(),
                package: package_filter(),
                search: (!search_query().is_empty()).then(|| search_query()),
                sort: Some(sort_by()),
                limit: None,
            },
            offset: 0,
            limit: 200,
        };
        async move {
            (
                query.clone(),
                client::fetch_cve_inventory_pairs(&query).await,
            )
        }
    });

    let focus_result = use_resource(move || {
        let focused = focus_cve();
        let query = CveInventoryQuery {
            group_by: "environment".into(),
            environment_id: environment_filter(),
            group_id: None,
            filters: CveFilters {
                severity: severity_filter(),
                fix_status: fix_status_filter(),
                triage_status: triage_status_filter(),
                package: package_filter(),
                search: focused.clone(),
                sort: Some(sort_by()),
                limit: None,
            },
            offset: 0,
            limit: 200,
        };
        async move {
            let result = if let Some(focused) = &focused {
                Some(resolve_focused_package(query.clone(), focused).await)
            } else {
                None
            };
            (query, focused, result)
        }
    });

    use_effect(move || {
        let Some(focused) = focus_cve() else {
            return;
        };
        let focus_read = focus_result.read();
        let Some((query, result_focus, Some(result))) = focus_read.as_ref() else {
            return;
        };
        if result_focus.as_ref() != Some(&focused)
            || query.environment_id != environment_filter()
            || query.filters.severity != severity_filter()
            || query.filters.fix_status != fix_status_filter()
            || query.filters.triage_status != triage_status_filter()
            || query.filters.package != package_filter()
            || query.filters.sort != Some(sort_by())
            || focus_resolved_query().as_ref() == Some(query)
        {
            return;
        }
        if let Ok(Some(package)) = result {
            if selected_cve().is_none() {
                selected_cve.set(Some(ExactCveSelection {
                    cve_id: focused,
                    package: package.clone(),
                }));
            }
        }
        focus_resolved_query.set(Some(query.clone()));
    });

    let current_pair_filters = CveFilters {
        severity: severity_filter(),
        fix_status: fix_status_filter(),
        triage_status: triage_status_filter(),
        package: package_filter(),
        search: (!search_query().is_empty()).then(|| search_query()),
        sort: Some(sort_by()),
        limit: None,
    };
    let scoped_metadata = pair_metadata.read().as_ref().and_then(|(request, result)| {
        (request.environment_id == environment_filter() && request.filters == current_pair_filters)
            .then(|| result.clone())
    });
    let export_count = scoped_metadata
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|page| page.total.to_string())
        .unwrap_or_else(|| "…".into());

    use_effect(move || {
        if let (Some(Ok(_s)), Some((request, Ok(_)))) =
            (stats.read().as_ref(), pair_metadata.read().as_ref())
            && request.environment_id == environment_filter()
            && request.filters == current_pair_filters
        {
            let Some(cursor) = NAV_BADGES.read_unchecked().observed_at.clone() else {
                return;
            };
            let occurrence_ids = NAV_BADGES.read_unchecked().cves_occurrence_ids.clone();
            acknowledge_with_cursor_and_ids("cves", cursor, occurrence_ids);
        }
    });

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 16px;",

            // Page Header
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "CVEs" }
                    if let Some(Ok(s)) = stats.read().as_ref() {
                        p {
                            class: "page-subtitle",
                            title: "Fleet-wide totals. Affected systems are distinct across current and scheduled configuration exposure.",
                            "{s.total_cves} vulnerabilities · {s.systems_affected} systems affected · {s.fixable} have fixed versions"
                        }
                    }
                    if environment_filter().is_some() {
                        p { class: "page-subtitle", "KPIs are fleet-wide · findings below are environment-scoped" }
                    }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    if is_admin_user {
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: fleet_rescan_pending(),
                            onclick: move |_| {
                                if fleet_rescan_pending() {
                                    return;
                                }
                                fleet_rescan_pending.set(true);
                                spawn(async move {
                                    let result = client::trigger_cve_fleet_rescan().await;
                                    fleet_rescan_pending.set(false);
                                    match result {
                                        Ok(response) => {
                                            let publication = toast_lifecycle.write().publish(true);
                                            toast_message.set(Some((response.message, true)));
                                            gloo_timers::future::TimeoutFuture::new(SUCCESS_TOAST_DURATION_MS).await;
                                            if toast_lifecycle.write().expire(publication) {
                                                toast_message.set(None);
                                            }
                                        }
                                        Err(err) => {
                                            toast_lifecycle.write().publish(false);
                                            toast_message.set(Some((format!("Fleet rescan failed: {err}"), false)));
                                        }
                                    }
                                });
                            },
                            // Sync icon
                            svg {
                                width: "14",
                                height: "14",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                            }
                            if fleet_rescan_pending() { " Rescanning…" } else { " Rescan fleet" }
                        }
                    }
                    div { class: "cve-export rr-export",
                        button {
                            class: "btn btn-ghost focus-ring",
                            aria_expanded: "{export_open()}",
                            onclick: move |_| export_open.set(!export_open()),
                            Icon { name: IconName::Download, size: 14 }
                            " Export {export_count} ▾"
                        }
                        if export_open() {
                            div { class: "rr-export-pop card", role: "menu", aria_label: "Export CVEs",
                                div { class: "rr-export-title", "Export CVEs" }
                                button {
                                    class: "rr-export-item focus-ring",
                                    role: "menuitem",
                                    disabled: environment_filter().is_some(),
                                    title: if environment_filter().is_some() { "Environment-scoped CSV is unavailable" } else { "Export filtered fleet-wide CVE pairs" },
                                    onclick: move |_| {
                            export_open.set(false);
                            let mut toast_message = toast_message;
                            let mut toast_lifecycle = toast_lifecycle;
                            spawn(async move {
                                match client::export_cves_csv(&CveFilters {
                                    severity: severity_filter(),
                                    fix_status: fix_status_filter(),
                                    triage_status: triage_status_filter(),
                                    package: package_filter(),
                                    search: if search_query().is_empty() { None } else { Some(search_query()) },
                                    sort: Some(sort_by()),
                                    limit: None,
                                }).await {
                                    Ok(_) => {
                                        let publication = toast_lifecycle.write().publish(true);
                                        toast_message.set(Some(("CSV export started".to_string(), true)));
                                        gloo_timers::future::TimeoutFuture::new(SUCCESS_TOAST_DURATION_MS).await;
                                        if toast_lifecycle.write().expire(publication) {
                                            toast_message.set(None);
                                        }
                                    }
                                    Err(e) => {
                                        toast_lifecycle.write().publish(false);
                                        toast_message.set(Some((format!("Export failed: {e}"), false)));
                                    }
                                }
                            });
                                    },
                                    span { class: "rr-export-item-l", "CSV" }
                                    span { class: "rr-export-item-sub", if environment_filter().is_some() { "Environment-scoped CSV unavailable" } else { "Filtered fleet-wide findings" } }
                                }
                            }
                        }
                    }
                }
            }

            // Statistics Strip
            if let Some(Ok(fleet_stats)) = stats.read().as_ref() {
                div {
                    class: "stat-strip",

                    // Critical
                    div {
                        class: if flash_crit { "stat attention-flash" } else { "stat" },
                        span { class: "stat-accent", style: "--stat-color: #f87171;" }
                        div { class: "stat-label", "Critical" }
                        div { class: "stat-value", style: "color: #f87171;", "{fleet_stats.critical}" }
                        div { class: "stat-meta", "{fleet_stats.exploited} actively exploited" }
                    }

                    // High
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #fbbf24;" }
                        div { class: "stat-label", "High" }
                        div { class: "stat-value", style: "color: #fbbf24;", "{fleet_stats.high}" }
                    }

                    // Patchable
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #60a5fa;" }
                        div { class: "stat-label", "Patchable now" }
                        div { class: "stat-value", style: "color: #60a5fa;", "{fleet_stats.fixable}" }
                        div { class: "stat-meta", "Fixed package version available" }
                    }

                    // Accepted Risk
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #a78bfa;" }
                        div { class: "stat-label", "Accepted risk" }
                        div { class: "stat-value", style: "color: #a78bfa;", "{fleet_stats.accepted}" }
                        div { class: "stat-meta", "{fleet_stats.scheduled} scheduled separately" }
                    }

                    // Outstanding
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #34d399;" }
                        div { class: "stat-label", "Outstanding" }
                        div {
                            class: "stat-value",
                            style: if fleet_stats.outstanding > 20 { "color: #f87171;" } else { "color: #34d399;" },
                            "{fleet_stats.outstanding}"
                        }
                        div { class: "stat-meta", "need triage" }
                    }

                }
            }

            // Filter Bar
            div {
                class: "filterbar",

                // Search input
                div {
                    class: "filter-search",
                    style: "max-width: 300px;",
                    svg {
                        width: "14",
                        height: "14",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.3-4.3" }
                    }
                    input {
                        class: "input focus-ring",
                        r#type: "text",
                        placeholder: "Search CVE / package / title…",
                        value: "{search_query}",
                        oninput: move |evt| {
                            search_query.set(evt.value());
                            focus_cve.set(None);
                            focus_resolved_query.set(None);
                        },
                    }
                }

                // Severity Filter
                div {
                    class: "seg",
                    for (sev, label) in [("all", "All"), ("critical", "Critical"), ("high", "High"), ("medium", "Medium"), ("low", "Low")] {
                        button {
                            class: if severity_filter().as_deref() == if sev == "all" { None } else { Some(sev) } { "active" } else { "" },
                            onclick: move |_| {
                                if sev == "all" {
                                    severity_filter.set(None);
                                } else {
                                    severity_filter.set(Some(sev.to_string()));
                                }
                            },
                            "{label}"
                        }
                    }
                }

                // Fix Status Filter
                div {
                    class: "seg",
                    for status in [("all", "Any status"), ("available", "Has patch"), ("pending", "No patch"), ("exploited", "Exploited")] {
                        button {
                            class: if fix_status_filter().as_deref() == if status.0 == "all" { None } else { Some(status.0) } { "active" } else { "" },
                            onclick: move |_| {
                                if status.0 == "all" {
                                    fix_status_filter.set(None);
                                } else {
                                    fix_status_filter.set(Some(status.0.to_string()));
                                }
                            },
                            "{status.1}"
                        }
                    }
                }

                // Triage Status Filter
                div {
                    class: "seg",
                    for status in [("all", "Any triage"), ("outstanding", "Outstanding"), ("scheduled", "Patch scheduled"), ("accepted", "Accepted"), ("inventory_only", "Inventory only")] {
                        button {
                            class: if triage_status_filter().as_deref() == if status.0 == "all" { None } else { Some(status.0) } { "active" } else { "" },
                            onclick: move |_| {
                                if status.0 == "all" {
                                    triage_status_filter.set(None);
                                } else {
                                    triage_status_filter.set(Some(status.0.to_string()));
                                }
                            },
                            "{status.1}"
                        }
                    }
                }

                // Package filter
                div {
                    style: "position: relative; max-width: 200px;",
                    input {
                        class: "input focus-ring mono",
                        style: if package_filter().is_some() { "font-size: 12px; padding-right: 28px;" } else { "font-size: 12px; padding-right: 12px;" },
                        r#type: "text",
                        list: "cve-pkg-list",
                        placeholder: "All packages…",
                        value: "{package_filter().unwrap_or_default()}",
                        oninput: move |evt| {
                            let value = evt.value();
                            if value.trim().is_empty() {
                                package_filter.set(None);
                            } else {
                                package_filter.set(Some(value));
                            }
                        },
                    }
                    datalist {
                        id: "cve-pkg-list",
                        if let Some(Ok(packages)) = package_names.read().as_ref() {
                            for package in packages {
                                option { value: "{package}" }
                            }
                        }
                    }
                    if package_filter().is_some() {
                        button {
                            class: "btn-icon focus-ring",
                            style: "position: absolute; right: 4px; top: 50%; transform: translateY(-50%); padding: 4px;",
                            title: "Clear",
                            onclick: move |_| package_filter.set(None),
                            svg {
                                width: "11",
                                height: "11",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M18 6 6 18" }
                                path { d: "M6 6l12 12" }
                            }
                        }
                    }
                }

                div {
                    select {
                        class: "input focus-ring",
                        aria_label: "Environment",
                        value: "{environment_filter().map(|id| id.to_string()).unwrap_or_default()}",
                        onchange: move |evt| environment_filter.set(evt.value().parse().ok()),
                        option { value: "", "All environments" }
                        if let Some(Ok(items)) = environments.read().as_ref() {
                            for env in items {
                                option { key: "{env.id}", value: "{env.id}", "{env.name}" }
                            }
                        }
                    }
                }

                // Group label + toggle
                span {
                    class: "filter-count",
                    style: "margin-left: auto; margin-right: 0;",
                    "Group"
                }
                div {
                    class: "seg",
                    button {
                        class: if view_mode() == "grouped" { "active" } else { "" },
                        onclick: move |_| view_mode.set("grouped".to_string()),
                        "Package"
                    }
                    button {
                        class: if view_mode() == "environment" { "active" } else { "" },
                        onclick: move |_| view_mode.set("environment".to_string()),
                        "Environment"
                    }
                    button {
                        class: if view_mode() == "host" { "active" } else { "" },
                        onclick: move |_| view_mode.set("host".to_string()),
                        "Env › System"
                    }
                    button {
                        class: if view_mode() == "flat" { "active" } else { "" },
                        onclick: move |_| view_mode.set("flat".to_string()),
                        "None"
                    }
                }

                if matches!(view_mode().as_str(), "grouped" | "flat") {
                    span {
                        class: "filter-count",
                        style: "margin-left: 0; margin-right: 0;",
                        "Sort"
                    }
                    div {
                        class: "seg",
                        for sort in [("severity", "Severity"), ("cvss", "CVSS"), ("age", "Newest"), ("affected", "Most affected") ] {
                            button {
                                class: if sort_by() == sort.0 { "active" } else { "" },
                                title: if sort.0 == "affected" { "Sort by distinct current or scheduled configuration affected systems" } else { sort.1 },
                                onclick: move |_| sort_by.set(sort.0.to_string()),
                                "{sort.1}"
                            }
                        }
                    }
                }
            }

            // CVE List
            if let Some(Err(error)) = environments.read().as_ref() {
                div { class: "page-subtitle", "Environment choices unavailable: {error}" }
            }
            if matches!(view_mode().as_str(), "environment" | "host") {
                div {
                    key: "{severity_filter():?}|{fix_status_filter():?}|{triage_status_filter():?}|{package_filter():?}|{search_query()}|{view_mode()}|{environment_filter():?}",
                    style: "display: flex; flex-direction: column; gap: 10px;",
                    if let Some(Err(error)) = &scoped_metadata {
                        p { class: "page-subtitle", "Unable to load finding details: {error}" }
                    }
                    CveInventoryGroupsView {
                    query: CveInventoryQuery {
                        group_by: "environment".into(),
                        environment_id: environment_filter(),
                        group_id: None,
                        filters: CveFilters {
                            severity: severity_filter(), fix_status: fix_status_filter(),
                            triage_status: triage_status_filter(), package: package_filter(),
                            search: (!search_query().is_empty()).then(|| search_query()),
                            sort: None, limit: None,
                        },
                        offset: 0, limit: 50,
                    },
                    nested_hosts: view_mode() == "host",
                    environments: environments.read().as_ref().and_then(|result| result.as_ref().ok()).cloned().unwrap_or_default(),
                    pairs: scoped_metadata.as_ref().and_then(|result| result.as_ref().ok()).map(|page| page.items.clone()).unwrap_or_default(),
                    seen: current_seen.clone(),
                    now: seen_clock(),
                    on_open_cve: {
                        move |selection: ExactCveSelection| {
                        let user_id = authenticated_user_id(&app_state.read().auth);
                        persist_seen_pairs(seen_state, user_id.as_deref(), &[selection.clone()]);
                        sync_cve_url_state(
                            severity_filter(), fix_status_filter(), triage_status_filter(),
                            package_filter(), search_query(), focus_cve(), sort_by(), view_mode(),
                            environment_filter(), Some(&selection), true,
                        );
                        selected_cve.set(Some(selection));
                        }
                    }
                    }
                }
            } else {
                div {
                    key: "{severity_filter():?}|{fix_status_filter():?}|{triage_status_filter():?}|{package_filter():?}|{search_query()}|{sort_by()}|{view_mode()}|{environment_filter():?}|{active_user:?}",
                    style: "display: flex; flex-direction: column; gap: 10px;",
                    if let Some((_, _, Some(Err(error)))) = focus_result.read().as_ref() {
                        p { class: "page-subtitle", "Focused CVE resolution unavailable: {error}" }
                    }
                    CvePairsView {
                    fleet_stats: stats.read().as_ref().and_then(|result| result.as_ref().ok()).cloned(),
                    query: CveInventoryQuery {
                        group_by: "environment".into(), environment_id: environment_filter(), group_id: None,
                        filters: CveFilters {
                            severity: severity_filter(), fix_status: fix_status_filter(),
                            triage_status: triage_status_filter(), package: package_filter(),
                            search: (!search_query().is_empty()).then(|| search_query()),
                            sort: Some(sort_by()), limit: None,
                        },
                        offset: 0, limit: 200,
                    },
                    grouped: view_mode() == "grouped",
                    focused: focus_cve(),
                    user_id: active_user.clone(),
                    seen_state,
                    now: seen_clock(),
                    on_mark_seen: {
                        move |selections: Vec<ExactCveSelection>| {
                            let user_id = authenticated_user_id(&app_state.read().auth);
                            persist_seen_pairs(seen_state, user_id.as_deref(), &selections);
                        }
                    },
                    on_open_cve: {
                        move |selection: ExactCveSelection| {
                        let user_id = authenticated_user_id(&app_state.read().auth);
                        persist_seen_pairs(seen_state, user_id.as_deref(), &[selection.clone()]);
                        sync_cve_url_state(
                            severity_filter(), fix_status_filter(), triage_status_filter(),
                            package_filter(), search_query(), focus_cve(), sort_by(), view_mode(), environment_filter(),
                            Some(&selection), true,
                        );
                        selected_cve.set(Some(selection));
                        }
                    }
                    }
                }
            }

            // CVE Detail Drawer
            if let Some(selection) = selected_cve() {
                ExactCveFleetDrawer {
                    key: "{selection.cve_id}|{selection.package}",
                    selection,
                    on_close: move |_| {
                        sync_cve_url_state(
                            severity_filter(), fix_status_filter(), triage_status_filter(),
                            package_filter(), search_query(), focus_cve(), sort_by(), view_mode(), environment_filter(), None, true,
                        );
                        selected_cve.set(None);
                        focus_resolved_query.set(focus_result.read().as_ref().map(|(query, _, _)| query.clone()));
                    }
                }
            }

            if let Some((ref message, is_success)) = *toast_message.read() {
                Toast {
                    message: message.clone(),
                    is_success,
                    on_dismiss: move |_| {
                        toast_lifecycle.write().dismiss();
                        toast_message.set(None);
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Flat List Components
// ─────────────────────────────────────────────────────────────────────────────

fn loaded_package_groups(items: &[CveListItem]) -> BTreeMap<String, Vec<CveListItem>> {
    let mut groups = BTreeMap::<String, Vec<CveListItem>>::new();
    for item in items {
        groups
            .entry(
                item.package_name
                    .clone()
                    .unwrap_or_else(|| "Unknown package".into()),
            )
            .or_default()
            .push(item.clone());
    }
    groups
}

/// Applies the same pair-level predicates as the scoped inventory request.
/// The environment restriction is enforced by the server and its request
/// identity, because a pair's environment names are not stable UUID scopes.
fn matches_pair_filters(item: &CveListItem, filters: &CveFilters) -> bool {
    filters
        .severity
        .as_deref()
        .is_none_or(|severity| item.severity.eq_ignore_ascii_case(severity))
        && filters
            .triage_status
            .as_deref()
            .is_none_or(|status| item.triage_status.eq_ignore_ascii_case(status))
        && filters
            .fix_status
            .as_deref()
            .is_none_or(|status| match status {
                "available" => item.fix_status == "fix_available",
                "pending" => item.fix_status == "open",
                "exploited" => item.exploited,
                _ => false,
            })
        && filters.package.as_deref().is_none_or(|package| {
            item.package_name
                .as_deref()
                .is_some_and(|name| name.to_lowercase().contains(&package.to_lowercase()))
        })
        && filters.search.as_deref().is_none_or(|search| {
            let search = search.to_lowercase();
            [
                Some(item.cve_id.as_str()),
                item.package_name.as_deref(),
                Some(item.title.as_str()),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.to_lowercase().contains(&search))
        })
}

fn severity_weight(severity: &str) -> u8 {
    match severity.to_ascii_uppercase().as_str() {
        "CRITICAL" => 4,
        "HIGH" => 3,
        "MEDIUM" => 2,
        "LOW" => 1,
        _ => 0,
    }
}

fn ordered_package_groups(
    items: &[CveListItem],
    now: DateTime<Utc>,
    seen: Option<&CveSeenState>,
) -> Vec<(String, Vec<CveListItem>)> {
    let mut groups = loaded_package_groups(items).into_iter().collect::<Vec<_>>();
    groups.sort_by(|(left_name, left), (right_name, right)| {
        let rank = |rows: &[CveListItem]| {
            (
                rows.iter()
                    .any(|item| recently_observed_unseen(item, now, seen)),
                rows.iter()
                    .map(|item| severity_weight(&item.severity))
                    .max()
                    .unwrap_or(0),
            )
        };
        rank(right)
            .cmp(&rank(left))
            .then_with(|| left_name.cmp(right_name))
    });
    groups
}

fn unseen_new_pairs(
    items: &[CveListItem],
    now: DateTime<Utc>,
    seen: Option<&CveSeenState>,
) -> Vec<ExactCveSelection> {
    items
        .iter()
        .filter(|item| recently_observed_unseen(item, now, seen))
        .filter_map(|item| {
            item.package_name.as_ref().map(|name| ExactCveSelection {
                cve_id: item.cve_id.clone(),
                package: name.clone(),
            })
        })
        .collect()
}

// CONCURRENCY: A grouped response belongs only to the filter set that
// requested it. A previous response must not label the next filter's rows.
fn matching_group<'a>(
    result: &'a Option<(
        CveFilters,
        Result<Vec<CvePackageGroup>, client::ApiClientError>,
    )>,
    filters: &CveFilters,
    package: &str,
) -> Option<&'a CvePackageGroup> {
    let (requested, response) = result.as_ref()?;
    let groups = response.as_ref().ok()?;
    (requested == filters)
        .then(|| groups.iter().find(|g| g.package_name == package))
        .flatten()
}

fn pair_selection(mut selected: Signal<BTreeSet<ExactCveSelection>>, selection: ExactCveSelection) {
    if !selected.write().insert(selection.clone()) {
        selected.write().remove(&selection);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum QuickCveSelection {
    Critical,
    High,
    Patchable,
    Outstanding,
}

impl QuickCveSelection {
    fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Patchable => "Patchable",
            Self::Outstanding => "Outstanding",
        }
    }

    fn matches(self, pair: &CveListItem) -> bool {
        match self {
            Self::Critical => pair.severity.eq_ignore_ascii_case("critical"),
            Self::High => pair.severity.eq_ignore_ascii_case("high"),
            Self::Patchable => pair.fix_status == "fix_available",
            Self::Outstanding => pair.triage_status == "outstanding",
        }
    }

    // A conflicting filter has no matches. Keep every other active filter;
    // `exploited` can overlap `patchable` and is checked on returned rows.
    fn query(self, base: &CveInventoryQuery) -> Option<CveInventoryQuery> {
        let mut request = base.clone();
        request.offset = 0;
        request.limit = 200;
        match self {
            Self::Critical | Self::High => {
                let severity = if self == Self::Critical {
                    "critical"
                } else {
                    "high"
                };
                if request
                    .filters
                    .severity
                    .as_deref()
                    .is_some_and(|value| value != severity)
                {
                    return None;
                }
                request.filters.severity = Some(severity.into());
            }
            Self::Patchable => match request.filters.fix_status.as_deref() {
                Some("pending") => return None,
                None => request.filters.fix_status = Some("available".into()),
                _ => {}
            },
            Self::Outstanding => {
                if request
                    .filters
                    .triage_status
                    .as_deref()
                    .is_some_and(|value| value != "outstanding")
                {
                    return None;
                }
                request.filters.triage_status = Some("outstanding".into());
            }
        }
        Some(request)
    }
}

fn toggle_quick_pairs(
    selected: &BTreeSet<ExactCveSelection>,
    matches: &BTreeSet<ExactCveSelection>,
) -> BTreeSet<ExactCveSelection> {
    if matches.is_subset(selected) {
        selected.difference(matches).cloned().collect()
    } else {
        selected.union(matches).cloned().collect()
    }
}

/// Resolves every matching exact pair before a quick-select changes state.
///
/// CONCURRENCY: Offset pages are separate server reads, not a snapshot. A
/// changed total, repeated pair, missing package or broken continuation fails
/// the entire gesture; selecting a partial set would misrepresent the badge.
async fn fetch_quick_pairs(
    base: &CveInventoryQuery,
    kind: QuickCveSelection,
) -> Result<BTreeSet<ExactCveSelection>, String> {
    let Some(mut request) = kind.query(base) else {
        return Ok(BTreeSet::new());
    };
    let mut expected_total = None;
    let mut read_count = 0_i64;
    let mut found = BTreeSet::new();
    loop {
        let page = client::fetch_cve_inventory_pairs(&request)
            .await
            .map_err(|error| format!("Matching CVEs could not be loaded: {error}"))?;
        if expected_total.is_some_and(|total| total != page.total) || page.total < 0 {
            return Err("Matching CVEs changed during selection. Refresh and try again.".into());
        }
        expected_total = Some(page.total);
        read_count += page.items.len() as i64;
        for pair in page.items {
            if !matches_pair_filters(&pair, &base.filters) {
                return Err(
                    "Matching CVEs changed during selection. Refresh and try again.".into(),
                );
            }
            if kind.matches(&pair) {
                let Some(package) = pair.package_name else {
                    return Err(
                        "A matching CVE has no package identity and cannot be selected.".into(),
                    );
                };
                if !found.insert(ExactCveSelection {
                    cve_id: pair.cve_id,
                    package,
                }) {
                    return Err(
                        "Matching CVEs changed during selection. Refresh and try again.".into(),
                    );
                }
            }
        }
        if let Some(next) = page.next_offset {
            if next <= request.offset || next > page.total || read_count >= page.total {
                return Err("Matching CVE pagination changed. Refresh and try again.".into());
            }
            request.offset = next;
        } else if read_count == page.total {
            return Ok(found);
        } else {
            return Err("Matching CVE pagination was incomplete. Refresh and try again.".into());
        }
    }
}

fn request_quick_selection(
    base: CveInventoryQuery,
    kind: QuickCveSelection,
    generation: Signal<u64>,
    mut pending: Signal<Option<QuickCveSelection>>,
    mut feedback: Signal<Option<(String, bool)>>,
    mut selected: Signal<BTreeSet<ExactCveSelection>>,
    mut resolved: Signal<BTreeMap<QuickCveSelection, BTreeSet<ExactCveSelection>>>,
) {
    if pending().is_some() {
        return;
    }
    pending.set(Some(kind));
    feedback.set(None);
    let version = generation();
    spawn(async move {
        let result = fetch_quick_pairs(&base, kind).await;
        // CONCURRENCY: A filter change invalidates this request. In particular,
        // it must not write stale pair identities after the new page mounts.
        if generation() != version {
            return;
        }
        match result {
            Ok(matches) if matches.is_empty() => {
                feedback.set(Some((
                    "No matching CVEs in the current filters.".into(),
                    true,
                )));
            }
            Ok(matches) => {
                let next = toggle_quick_pairs(&selected(), &matches);
                let removed = next.len() < selected().len();
                let count = matches.len();
                selected.set(next);
                resolved.write().insert(kind, matches);
                feedback.set(Some((
                    format!(
                        "{} {count} matching {} CVE/package pairs.",
                        if removed { "Cleared" } else { "Selected" },
                        kind.label()
                    ),
                    true,
                )));
            }
            Err(message) => feedback.set(Some((message, false))),
        }
        pending.set(None);
    });
}

fn pair_metadata_for_member<'a>(
    member: &CveInventoryMember,
    pairs: &'a [CveListItem],
) -> Option<&'a CveListItem> {
    pairs
        .iter()
        .find(|pair| pair.cve_id == member.cve_id && pair.package_name == member.package_name)
}

fn deployment_dot(status: Option<&str>) -> Option<(&'static str, &'static str)> {
    match status? {
        "up_to_date" => Some(("#34d399", "up to date")),
        "behind" => Some(("#fbbf24", "behind")),
        "ahead" => Some(("#60a5fa", "ahead")),
        "no_deployment" => Some(("#9ca3af", "not deployed")),
        "unknown" => Some(("#9ca3af", "unknown")),
        _ => None,
    }
}

fn initially_expanded(group_by: &str, offset: i64, index: usize) -> bool {
    offset == 0 && index < if group_by == "host" { 2 } else { 3 }
}

#[component]
fn CvePairsView(
    fleet_stats: Option<CveFleetStats>,
    query: CveInventoryQuery,
    grouped: bool,
    focused: Option<String>,
    user_id: Option<String>,
    seen_state: Signal<Option<CveSeenState>>,
    now: DateTime<Utc>,
    on_mark_seen: EventHandler<Vec<ExactCveSelection>>,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
    let mut offset = use_signal(|| 0_i64);
    let mut loaded = use_signal(Vec::<CveListItem>::new);
    let mut next = use_signal(|| None::<i64>);
    let mut total = use_signal(|| 0_i64);
    let mut applied = use_signal(|| None::<i64>);
    let mut expanded_packages = use_signal(BTreeMap::<String, bool>::new);
    let mut default_package = use_signal(|| None::<String>);
    let mut default_applied = use_signal(|| false);
    let mut selected = use_signal(BTreeSet::<ExactCveSelection>::new);
    let mut quick_scope = use_signal(|| query.clone());
    let mut quick_generation = use_signal(|| 0_u64);
    let mut quick_pending = use_signal(|| None::<QuickCveSelection>);
    let mut quick_feedback = use_signal(|| None::<(String, bool)>);
    let mut quick_resolved =
        use_signal(BTreeMap::<QuickCveSelection, BTreeSet<ExactCveSelection>>::new);
    let mut loaded_identity = use_signal(|| query.clone());
    // CONCURRENCY: A page from the previous filter identity cannot supply
    // package headers or children after the operator changes any filter.
    use_effect(use_reactive(&query, move |request| {
        if *quick_scope.peek() != request {
            quick_scope.set(request.clone());
            let next_generation = quick_generation.peek().wrapping_add(1);
            quick_generation.set(next_generation);
            quick_pending.set(None);
            quick_feedback.set(None);
            quick_resolved.write().clear();
        }
        if *loaded_identity.peek() != request {
            loaded_identity.set(request);
            loaded.set(Vec::new());
            offset.set(0);
            next.set(None);
            total.set(0);
            applied.set(None);
            expanded_packages.set(BTreeMap::new());
            default_package.set(None);
            default_applied.set(false);
            selected.set(BTreeSet::new());
        }
    }));
    let grouped_response = use_resource(use_reactive(
        &(query.filters.clone(), query.environment_id, grouped),
        move |(filters, environment, is_grouped)| async move {
            let response = if is_grouped && environment.is_none() {
                client::fetch_cves_grouped(&filters).await
            } else {
                Ok(Vec::new())
            };
            (filters, response)
        },
    ));
    let page = use_resource(use_reactive(&query, move |mut request| {
        request.offset = offset();
        async move {
            (
                request.clone(),
                client::fetch_cve_inventory_pairs(&request).await,
            )
        }
    }));
    use_effect(move || {
        if let Some((request, Ok(response))) = page.read().as_ref()
            && request.offset == offset()
            && (CveInventoryQuery {
                offset: 0,
                ..request.clone()
            }) == *loaded_identity.read()
            && applied() != Some(offset())
        {
            loaded.write().extend(response.items.iter().cloned());
            next.set(response.next_offset.filter(|value| *value > offset()));
            total.set(response.total);
            applied.set(Some(offset()));
        }
    });
    let visible = if *loaded_identity.read() == query {
        loaded()
    } else {
        Vec::new()
    }
    .into_iter()
    .filter(|item| {
        matches_pair_filters(item, &query.filters)
            && focused.as_ref().is_none_or(|id| {
                item.cve_id == *id
                    && item.package_name.is_some()
                    && has_retained_package_evidence(item)
            })
    })
    .collect::<Vec<_>>();
    let seen = seen_for_user(seen_state.read().as_ref(), user_id.as_deref()).cloned();
    let ordered = ordered_package_groups(&visible, now, seen.as_ref())
        .into_iter()
        .map(|(package, cves)| {
            let is_new = cves
                .iter()
                .any(|item| recently_observed_unseen(item, now, seen.as_ref()));
            let expanded = expanded_packages()
                .get(&package)
                .copied()
                .unwrap_or(default_package().as_ref() == Some(&package));
            let count = cves.len();
            let newly_observed = unseen_new_pairs(&cves, now, seen.as_ref());
            (package, cves, is_new, expanded, count, newly_observed)
        })
        .collect::<Vec<_>>();
    let effect_user = user_id.clone();
    let default_query = query.clone();
    use_effect(move || {
        if !grouped || default_applied() || applied().is_none() {
            return;
        }
        let current_seen = seen_state.read();
        let Some(seen) = seen_for_user(current_seen.as_ref(), effect_user.as_deref()) else {
            return;
        };
        let rows = if *loaded_identity.read() == default_query {
            loaded()
                .into_iter()
                .filter(|row| matches_pair_filters(row, &default_query.filters))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let ordered = ordered_package_groups(&rows, now, Some(seen));
        if let Some((package, _)) = ordered.into_iter().find(|(_, pairs)| {
            pairs
                .iter()
                .any(|item| recently_observed_unseen(item, now, Some(seen)))
        }) {
            default_package.set(Some(package));
            default_applied.set(true);
        }
    });
    rsx! {
        div { class: "cve-selection-strip", role: "toolbar", aria_label: "CVE selection",
            span { class: "cve-selection-label", "SELECT" }
            if let Some(stats) = fleet_stats.as_ref() {
                for (kind, count) in [(QuickCveSelection::Critical, stats.critical), (QuickCveSelection::High, stats.high), (QuickCveSelection::Patchable, stats.fixable), (QuickCveSelection::Outstanding, stats.outstanding)] {
                    { let request = query.clone();
                      let on = quick_resolved.read().get(&kind)
                          .is_some_and(|matches| !matches.is_empty() && matches.is_subset(&selected()));
                      rsx! { button {
                          class: if on { "cve-selection-chip focus-ring on" } else { "cve-selection-chip focus-ring" },
                          "data-kind": "{kind.label().to_lowercase()}",
                          aria_pressed: on,
                          aria_busy: quick_pending() == Some(kind),
                          disabled: count == 0 || quick_pending().is_some(),
                          title: "Fleet-wide count. Selects all matching authorized pairs within the active filters; no triage action is submitted.",
                          onclick: move |_| request_quick_selection(request.clone(), kind, quick_generation, quick_pending, quick_feedback, selected, quick_resolved),
                          "{kind.label()} " span { class: "mono", "{count}" }
                      } }
                    }
                }
            }
            if !selected().is_empty() {
                span { class: "mono", "{selected().len()} selected" }
                button { class: "btn btn-ghost xs focus-ring", disabled: quick_pending().is_some(), onclick: move |_| selected.set(BTreeSet::new()), "Clear" }
            }
            span { class: "cve-selection-hint", "Ctrl/Cmd-click to select" }
        }
        if let Some((message, success)) = quick_feedback() {
            p { class: "page-subtitle", role: if success { "status" } else { "alert" }, "{message}" }
        }
        if let Some((request, Err(error))) = page.read().as_ref()
            && request.offset == offset() && request.filters == query.filters && request.environment_id == query.environment_id {
            div { class: "empty", "Unable to load CVE pairs: {error}" }
        }
        if loaded().is_empty() && page.read().is_none() {
            div { class: "empty", "Loading CVE pairs..." }
        } else if focused.is_none() && loaded().is_empty() && applied() == Some(offset()) {
            div { class: "empty", "No CVEs match the current filters." }
        }
        if focused.is_some() && visible.is_empty() && applied().is_some() {
            div {
                class: "empty",
                if next().is_some() {
                    "No retained package finding for this CVE in loaded pages. More pairs remain."
                } else {
                    "No retained package finding matches this focused CVE."
                }
            }
        }
        if grouped {
            for (package, cves, is_new, is_expanded, count, newly_observed) in ordered {
                { // The fleet aggregate gives a distinct-host union only when
                  // every filtered pair is loaded for this exact scope. All
                  // other visible metrics come directly from child rows.
                let patchable_shown = cves.iter().filter(|c| c.fix_status == "fix_available").count();
                let outstanding_shown = cves.iter().filter(|c| c.triage_status == "outstanding").count();
                let worst = cves.iter().filter_map(|c| c.cvss_v3_score).reduce(f32::max);
                let aggregate = if query.environment_id.is_none() && next().is_none() && total() == loaded().len() as i64 {
                    matching_group(&grouped_response.read(), &query.filters, &package).cloned()
                        .filter(|g| g.cve_count == count as i64
                            && g.critical_count == cves.iter().filter(|c| c.severity.eq_ignore_ascii_case("critical")).count() as i64
                            && g.high_count == cves.iter().filter(|c| c.severity.eq_ignore_ascii_case("high")).count() as i64
                            && g.medium_count == cves.iter().filter(|c| c.severity.eq_ignore_ascii_case("medium")).count() as i64
                            && g.low_count == cves.iter().filter(|c| c.severity.eq_ignore_ascii_case("low")).count() as i64
                            && g.fixable_count == patchable_shown as i64
                            && g.outstanding_count == outstanding_shown as i64)
                  } else { None };
                let severities = (cves.iter().any(|c| c.severity.eq_ignore_ascii_case("critical")), cves.iter().any(|c| c.severity.eq_ignore_ascii_case("high")), cves.iter().any(|c| c.severity.eq_ignore_ascii_case("medium")));
                let meter_color = if severities.0 { "#f87171" } else if severities.1 { "#fbbf24" } else if severities.2 { "#60a5fa" } else { "#9ca3af" };
                rsx! {
                div {
                    class: "card cve-package-card",
                    key: "{package}",
                    style: "overflow: hidden;",
                    button {
                        class: "focus-ring cve-package-toggle",
                        style: if is_expanded { "border-left-color:{meter_color};background:color-mix(in oklab,var(--cf-brand-purple) 6%,var(--cf-card-bg));" } else { "border-left-color:{meter_color};" },
                        aria_expanded: "{is_expanded}",
                        onclick: {
                            let package = package.clone();
                            let newly_observed = newly_observed.clone();
                            move |_| {
                                let is_expanded = expanded_packages().get(&package).copied()
                                    .unwrap_or(default_package().as_ref() == Some(&package));
                                expanded_packages.write().insert(package.clone(), !is_expanded);
                                if !is_expanded {
                                    on_mark_seen.call(newly_observed.clone());
                                }
                            }
                        },
                        span { class: "cve-package-chevron", Icon { name: if is_expanded { IconName::ChevronDown } else { IconName::ChevronRight }, size: 14 } }
                        span { class: "cve-package-info",
                            span { class: "cve-package-name mono", "{package}" }
                            if is_new { span { class: "chip chip-info", "new" } }
                            span { class: "cve-package-count", "{count} CVEs shown" }
                            small {
                                if let Some(g) = &aggregate {
                                    "{g.total_affected_systems} distinct systems affected · {patchable_shown} patchable · {outstanding_shown} outstanding"
                                } else {
                                    "{count} shown pairs · {patchable_shown} patchable · {outstanding_shown} outstanding · host total unavailable"
                                }
                            }
                        }
                        span { class: "cve-package-severities",
                                for (severity, label, class) in [("critical", "crit", "chip-critical"), ("high", "high", "chip-warning"), ("medium", "med", "chip-info"), ("low", "low", "chip-unknown")] {
                                    { let n = cves.iter().filter(|c| c.severity.eq_ignore_ascii_case(severity)).count(); rsx! { if n > 0 { span { class: "chip {class}", "{n} {label} shown" } } } }
                                }
                        }
                        span { class: "cve-package-cvss",
                            small { "Worst CVSS" }
                            if let Some(score) = worst {
                                span { class: "cve-package-meter", span { style: "width: {score * 10.0}%; background: {meter_color};" } }
                                strong { class: "mono", "{score:.1}" }
                            } else { strong { "—" } }
                        }
                    }
                    if is_expanded {
                        table { class: "sys-table",
                            thead { tr { th { "CVE" } th { "Severity" } th { "CVSS" } th { "Title" } th { "Affected" } th { "Fix" } th { "Triage" } th { "Age" } } }
                            tbody {
                                for cve in cves {
                                    CveRowInGroup { key: "{cve.cve_id}|{cve.package_name:?}", is_new: recently_observed_unseen(&cve, now, seen.as_ref()), is_selected: cve.package_name.as_ref().is_some_and(|p| selected().contains(&ExactCveSelection { cve_id: cve.cve_id.clone(), package: p.clone() })), selected: Some(selected), cve, total_systems: 0, on_open: on_open_cve }
                                }
                            }
                        }
                    }
                }
                } }
            }
        } else {
            div { class: "card", style: "overflow-x: auto;",
                table { class: "sys-table",
                    thead { tr { th { "CVE" } th { "Severity" } th { "CVSS" } th { "Package" } th { "Title" } th { "Affected" } th { "Fix" } th { "Triage" } th { "Age" } th { " " } } }
                    tbody {
                        for cve in visible {
                            CveRow { key: "{cve.cve_id}|{cve.package_name:?}", is_new: recently_observed_unseen(&cve, now, seen.as_ref()), is_selected: cve.package_name.as_ref().is_some_and(|p| selected().contains(&ExactCveSelection { cve_id: cve.cve_id.clone(), package: p.clone() })), selected: Some(selected), cve, total_systems: 0, on_open: on_open_cve }
                        }
                    }
                }
            }
        }
        if let Some(more) = next() {
            button {
                class: "btn btn-ghost focus-ring",
                disabled: applied() != Some(offset()),
                onclick: move |_| offset.set(more),
                "Show more findings ({loaded().len()} of {total()})"
            }
        }
    }
}

#[component]
fn CveRow(
    cve: CveListItem,
    total_systems: i64,
    is_new: bool,
    #[props(default)] is_selected: bool,
    #[props(default)] selected: Option<Signal<BTreeSet<ExactCveSelection>>>,
    on_open: EventHandler<ExactCveSelection>,
) -> Element {
    let (current_affected, scheduled_configuration, historical) = cve.inventory_counts();
    let (triage_label, triage_class, triage_title) = triage_status_presentation(&cve.triage_status);
    let sev_cls = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "chip-critical",
        "HIGH" => "chip-warning",
        "MEDIUM" => "chip-info",
        _ => "chip-unknown",
    };
    let sev_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "#f87171",
        "HIGH" => "#fbbf24",
        "MEDIUM" => "#60a5fa",
        _ => "#9ca3af",
    };

    let cve_id_for_onclick = cve.cve_id.clone();
    let selection_for_row = cve.package_name.clone().map(|package| ExactCveSelection {
        cve_id: cve_id_for_onclick.clone(),
        package,
    });
    let selection_for_open = selection_for_row.clone();
    let cve_id_for_link = cve_id_for_onclick.clone();

    rsx! {
        tr {
            class: if is_selected { "cve-row-selected" } else { "" },
            style: if is_new { "cursor: pointer; background: color-mix(in oklab, var(--cf-brand-purple) 8%, transparent);" } else { "cursor: pointer;" },
            "data-testid": "cve-row",
            onclick: move |event| {
                if let Some(selection) = selection_for_row.clone() {
                    if event.modifiers().ctrl() || event.modifiers().meta() {
                        if let Some(selected) = selected { pair_selection(selected, selection); }
                    } else { on_open.call(selection); }
                }
            },

            // CVE ID
            td {
                div {
                    class: "mono",
                    style: "font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 8px;",
                    "{cve.cve_id}"
                    if is_new { span { class: "chip chip-info", "new" } }
                    if cve.exploited {
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            title: "Actively exploited in the wild",
                            "exploited"
                        }
                    }
                }
            }

            // Severity
            td {
                span {
                    class: "chip {sev_cls}",
                    span {
                        class: "chip-dot",
                        style: "background: {sev_color};",
                    }
                    "{cve.severity}"
                }
            }

            // CVSS
            td {
                if let Some(cvss) = cve.cvss_v3_score {
                    div {
                        style: "display: flex; align-items: center; gap: 6px;",
                        div {
                            style: "width: 40px; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                            div {
                                style: "width: {cvss * 10.0}%; height: 100%; background: {sev_color};",
                            }
                        }
                        span {
                            class: "mono",
                            style: "font-size: 12px; color: var(--cf-text-primary); font-weight: 600;",
                            "{cvss:.1}"
                        }
                    }
                }
            }

            // Package
            td {
                class: "mono",
                style: "font-size: 12px;",
                "{cve.package_name.as_deref().unwrap_or(\"\")}"
            }

            // Title
            td {
                style: "font-size: 13px; max-width: 340px;",
                div {
                    class: "truncate",
                    title: "{cve.title}",
                    "{cve.title}"
                }
            }

            // Affected
            td {
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    // Server icon
                    svg {
                        width: "11",
                        height: "11",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "color: var(--cf-text-muted);",
                        rect { x: "2", y: "2", width: "20", height: "8", rx: "2", ry: "2" }
                        rect { x: "2", y: "14", width: "20", height: "8", rx: "2", ry: "2" }
                        line { x1: "6", y1: "6", x2: "6.01", y2: "6" }
                        line { x1: "6", y1: "18", x2: "6.01", y2: "18" }
                    }
                    span {
                        class: "mono",
                        title: "{current_affected} current · {scheduled_configuration} scheduled configuration · {historical} historical evidence. Affected is the distinct current or scheduled union.",
                        style: if cve.affected_count > 0 { "font-size: 12px; font-weight: 600; color: var(--cf-text-primary);" } else { "font-size: 12px; font-weight: 600; color: var(--cf-text-muted);" },
                        "{cve.affected_count}"
                    }
                    if total_systems > 0 {
                        span {
                            style: "font-size: 11px; color: var(--cf-text-muted);",
                            "/ {total_systems}"
                        }
                    }
                }
            }

            // Fix Status
            td {
                if cve.fix_status == "fix_available" {
                    span {
                        class: "chip chip-healthy",
                        title: "{fixed_version_label(cve.fixed_version.as_deref(), true)}",
                        // Check icon
                        svg {
                            width: "10",
                            height: "10",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "3",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "display: inline; vertical-align: middle;",
                            polyline { points: "20 6 9 17 4 12" }
                        }
                        " {fixed_version_label(cve.fixed_version.as_deref(), true)}"
                    }
                } else {
                    span {
                        class: "chip chip-warning",
                        "no patch yet"
                    }
                }
            }

            // Triage Status
            td {
                span { class: "chip {triage_class}", title: "{triage_title}", "{triage_label}" }
            }

            // Age
            td {
                style: "font-size: 12px; color: var(--cf-text-muted);",
                "{cve.age_days}d"
            }

            // Actions
            td {
                div {
                    class: "row-actions",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Open advisory",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            let _ = web_sys::window().and_then(|w| {
                                w.open_with_url_and_target(
                                    &format!("https://nvd.nist.gov/vuln/detail/{}", cve_id_for_link),
                                    "_blank"
                                ).ok()
                            });
                        },
                        // Link icon
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                            path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                        }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        title: "Details",
                        aria_label: "Open fleet inventory for {cve.cve_id} {cve.package_name.as_deref().unwrap_or(\"unknown package\")}",
                        "data-testid": "cve-fleet-open",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if let Some(selection) = selection_for_open.clone() {
                                on_open.call(selection);
                            }
                        },
                        // Arrow-right icon
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line { x1: "5", y1: "12", x2: "19", y2: "12" }
                            polyline { points: "12 5 19 12 12 19" }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grouped View Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CveInventoryGroupsView(
    query: CveInventoryQuery,
    nested_hosts: bool,
    environments: Vec<EnvironmentSummary>,
    pairs: Vec<CveListItem>,
    seen: Option<CveSeenState>,
    now: DateTime<Utc>,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
    let mut offset = use_signal(|| 0_i64);
    let mut loaded = use_signal(Vec::<CveInventoryGroup>::new);
    let mut next = use_signal(|| None::<i64>);
    let mut total = use_signal(|| 0_i64);
    let mut applied = use_signal(|| None::<i64>);
    let resource_query = query.clone();
    let page = use_resource(move || {
        let mut request = resource_query.clone();
        request.offset = offset();
        async move {
            (
                request.offset,
                client::fetch_cve_inventory_groups(&request).await,
            )
        }
    });
    use_effect(move || {
        if let Some((response_offset, Ok(response))) = page.read().as_ref()
            && *response_offset == offset()
            && applied() != Some(offset())
        {
            loaded.write().extend(response.items.iter().cloned());
            next.set(response.next_offset.filter(|value| *value > offset()));
            total.set(response.total);
            applied.set(Some(offset()));
        }
    });

    rsx! {
        if let Some((_, Err(error))) = page.read().as_ref() {
            div { class: "empty", "Unable to load CVE groups: {error}" }
        }
        if loaded().is_empty() && page.read().is_none() {
            div { class: "empty", "Loading CVE groups..." }
        } else if loaded().is_empty() && page.read().as_ref().is_some_and(|(_, result)| result.is_ok()) {
            div { class: "empty", "No matching finding groups." }
        }
        for (index, group) in loaded().into_iter().enumerate() {
            CveInventoryGroupCard {
                key: "{query.group_by}|{group.group_id:?}",
                group,
                query: query.clone(),
                nested_hosts,
                initial_expanded: initially_expanded(&query.group_by, offset(), index),
                environments: environments.clone(),
                pairs: pairs.clone(),
                seen: seen.clone(),
                now,
                on_open_cve,
            }
        }
        if let Some(more) = next() {
            button {
                class: "btn btn-ghost focus-ring",
                disabled: applied() != Some(offset()),
                onclick: move |_| offset.set(more),
                "Show more groups ({loaded().len()} of {total()})"
            }
        }
    }
}

#[component]
fn CveInventoryGroupCard(
    group: CveInventoryGroup,
    query: CveInventoryQuery,
    nested_hosts: bool,
    initial_expanded: bool,
    environments: Vec<EnvironmentSummary>,
    pairs: Vec<CveListItem>,
    seen: Option<CveSeenState>,
    now: DateTime<Utc>,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
    let mut expanded = use_signal(move || initial_expanded);
    let group_id = group.group_id;
    let is_host = query.group_by == "host";
    let color = (!is_host)
        .then(|| {
            environments
                .iter()
                .find(|env| Some(env.id) == group_id)
                .map(|env| env.color_hex.clone())
        })
        .flatten();
    let deployment = deployment_dot(group.deployment_status.as_deref());
    rsx! {
        div { class: "card", style: "overflow: hidden;",
            div { style: "display: flex; align-items: center; gap: 8px;",
                button {
                    class: "focus-ring",
                    style: "all: unset; box-sizing: border-box; display: flex; align-items: center; gap: 14px; flex: 1; padding: 14px 18px; cursor: pointer; min-width: 0;",
                    aria_expanded: "{expanded()}",
                    onclick: move |_| expanded.set(!expanded()),
                    span { style: "color: var(--cf-text-muted);", if expanded() { "▾" } else { "›" } }
                    if is_host {
                        if let Some((dot_color, label)) = deployment {
                            span { class: "status-dot", style: "--status-color: {dot_color};", title: "Deployment: {label}" }
                        }
                        span { style: "flex: 1; font-weight: 700; text-align: left;", "{group.name}" }
                    } else {
                        EnvBadge {
                            name: group.name.clone(),
                            fg: color.clone(),
                            bg: color.as_ref().map(|value| format!("color-mix(in oklab, {value} 14%, var(--cf-card-bg))")),
                            border: color.clone(),
                        }
                        span { style: "flex: 1;" }
                    }
                    span { class: "mono", "{group.cve_count} distinct CVEs" }
                }
                if is_host {
                    if let Some(id) = group_id {
                        Link {
                            class: "btn btn-ghost focus-ring",
                            to: Route::SystemDetailView { id: id.to_string(), tab: "cves".into(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() },
                            "Open"
                        }
                    }
                }
            }
            div { style: "padding: 0 18px 12px; font-size: 12px; color: var(--cf-text-secondary); display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                span { "{group.cve_package_count} CVE/package pairs" }
                if !is_host {
                    span {
                        "{group.host_count} hosts with findings"
                        if let Some(total) = group.total_active_hosts { " of {total} active hosts" }
                    }
                }
                span { "{group.current_host_count} current · {group.scheduled_host_count} scheduled · {group.historical_host_count} historical" }
                span { "{group.patchable_pair_count} patchable pairs" }
                if let Some(flake) = &group.flake_name { span { "Flake: {flake}" } }
                if is_host {
                    if let Some((_, label)) = deployment { span { "Deployment: {label}" } }
                    else { span { "Deployment unavailable" } }
                }
            }
            div { style: "padding: 0 18px 12px; display: flex; align-items: center; gap: 5px; flex-wrap: wrap;",
                if group.critical_pair_count > 0 { span { class: "chip chip-critical", "{group.critical_pair_count} critical pairs" } }
                if group.high_pair_count > 0 { span { class: "chip chip-warning", "{group.high_pair_count} high pairs" } }
                if group.medium_pair_count > 0 { span { class: "chip chip-info", "{group.medium_pair_count} medium pairs" } }
                if group.low_pair_count > 0 { span { class: "chip chip-unknown", "{group.low_pair_count} low pairs" } }
                if group.unknown_pair_count > 0 { span { class: "chip chip-unknown", "{group.unknown_pair_count} unknown severity pairs" } }
                if group.exploited_pair_count > 0 { span { class: "chip chip-critical", "{group.exploited_pair_count} exploited pairs" } }
            }
            if expanded() {
                if nested_hosts && group_id.is_some() {
                    CveInventoryGroupsView {
                        key: "hosts-{group_id:?}",
                        query: CveInventoryQuery {
                            group_by: "host".into(), environment_id: group_id,
                            group_id: None, ..query.clone()
                        },
                        nested_hosts: false,
                        environments: environments.clone(),
                        pairs: pairs.clone(),
                        seen: seen.clone(),
                        now,
                        on_open_cve,
                    }
                } else {
                    CveInventoryMembersView {
                        key: "members-{group_id:?}",
                        query: CveInventoryQuery { group_id, ..query.clone() },
                        pairs: pairs.clone(),
                        seen: seen.clone(),
                        now,
                        on_open_cve,
                    }
                }
            }
        }
    }
}

#[component]
fn CveInventoryMembersView(
    query: CveInventoryQuery,
    pairs: Vec<CveListItem>,
    seen: Option<CveSeenState>,
    now: DateTime<Utc>,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
    let mut offset = use_signal(|| 0_i64);
    let mut loaded = use_signal(Vec::<CveInventoryMember>::new);
    let mut next = use_signal(|| None::<i64>);
    let mut total = use_signal(|| 0_i64);
    let mut applied = use_signal(|| None::<i64>);
    let page = use_resource(move || {
        let mut request = query.clone();
        request.offset = offset();
        async move {
            (
                request.offset,
                client::fetch_cve_inventory_members(&request).await,
            )
        }
    });
    use_effect(move || {
        if let Some((response_offset, Ok(response))) = page.read().as_ref()
            && *response_offset == offset()
            && applied() != Some(offset())
        {
            loaded.write().extend(response.items.iter().cloned());
            next.set(response.next_offset.filter(|value| *value > offset()));
            total.set(response.total);
            applied.set(Some(offset()));
        }
    });
    rsx! {
        if let Some((_, Err(error))) = page.read().as_ref() {
            div { class: "empty", "Unable to load group members: {error}" }
        }
        if loaded().is_empty() && page.read().is_none() {
            div { class: "empty", "Loading group members..." }
        }
        div { style: "overflow-x: auto;",
            table { class: "sys-table",
                thead { tr { th { "CVE" } th { "Severity" } th { "Package" } th { "System" } th { "Evidence" } th { "Version" } th { " " } } }
                tbody {
                    for member in loaded() {
                        tr {
                            key: "{member.cve_id}|{member.package_name:?}|{member.system_id}|{member.inventory_section}",
                            style: if pair_metadata_for_member(&member, &pairs).is_some_and(|pair| recently_observed_unseen(pair, now, seen.as_ref())) { "background: color-mix(in oklab, var(--cf-brand-purple) 8%, transparent);" } else { "" },
                            td { class: "mono",
                                "{member.cve_id}"
                                if pair_metadata_for_member(&member, &pairs).is_some_and(|pair| recently_observed_unseen(pair, now, seen.as_ref())) {
                                    span { class: "chip chip-info", "new" }
                                }
                            }
                            td {
                                if let Some(pair) = pair_metadata_for_member(&member, &pairs) {
                                    span { class: "chip", title: "{pair.title}", "{pair.severity}" }
                                    if pair.exploited { span { class: "chip chip-critical", "Exploited" } }
                                } else {
                                    span { "Severity unavailable" }
                                }
                            }
                            td { class: "mono", {member.package_name.as_deref().unwrap_or("Unknown package")} }
                            td { "{member.hostname}" }
                            td {
                                {match member.inventory_section.as_str() {
                                    "current" => "Current",
                                    "scheduled_deployment_target" => "Scheduled configuration",
                                    "historical" => "Historical evidence",
                                    _ => "Unknown evidence",
                                }}
                            }
                            td { class: "mono", "{member.installed_version}" }
                            td {
                                if let Some(package) = &member.package_name {
                                    button {
                                        class: "btn btn-ghost focus-ring",
                                        onclick: {
                                            let selection = ExactCveSelection { cve_id: member.cve_id.clone(), package: package.clone() };
                                            move |_| on_open_cve.call(selection.clone())
                                        },
                                        "Details"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(more) = next() {
            button {
                class: "btn btn-ghost focus-ring",
                disabled: applied() != Some(offset()),
                onclick: move |_| offset.set(more),
                "Show more findings ({loaded().len()} of {total()})"
            }
        }
    }
}

#[component]
fn CvePackageGroupsView(
    filters: CveFilters,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
    let grouped_cves = use_resource(move || {
        let f = filters.clone();
        async move { client::fetch_cves_grouped(&f).await }
    });

    rsx! {
        match &*grouped_cves.read_unchecked() {
            Some(Ok(groups)) => rsx! {
                if groups.is_empty() {
                    div {
                        class: "empty",
                        style: "margin: 0;",
                        h3 { "No CVEs match" }
                        div { "Try clearing a filter." }
                    }
                } else {
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px;",
                        for group in groups {
                            CvePackageGroupCard {
                                group: group.clone(),
                                on_open_cve: on_open_cve
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "empty",
                    style: "margin: 0;",
                    h3 { "Error loading CVEs" }
                    div { "{err}" }
                }
            },
            None => rsx! {
                div {
                    class: "empty",
                    style: "margin: 0;",
                    h3 { "Loading CVEs..." }
                }
            },
        }
    }
}

#[component]
fn CvePackageGroupCard(
    group: CvePackageGroup,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
    let mut is_expanded = use_signal(|| false);
    let (current_affected, scheduled_configuration, historical) = group.inventory_counts();

    let sev_color = if group.critical_count > 0 {
        "#f87171"
    } else if group.high_count > 0 {
        "#fbbf24"
    } else if group.medium_count > 0 {
        "#60a5fa"
    } else {
        "#9ca3af"
    };

    rsx! {
        div {
            class: "card",
            style: "overflow: hidden;",

            // Header button
            button {
                class: "focus-ring",
                style: format!(
                    "all: unset; display: grid; grid-template-columns: 24px 1fr auto auto; align-items: center; gap: 14px; padding: 14px 18px; cursor: pointer; width: 100%; background: {}; border-left: 3px solid {}; box-sizing: border-box;",
                    if is_expanded() { "color-mix(in oklab, var(--cf-brand-purple) 6%, var(--cf-card-bg))" } else { "transparent" },
                    sev_color
                ),
                onclick: move |_| is_expanded.set(!is_expanded()),

                // Chevron icon
                svg {
                    width: "14",
                    height: "14",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    style: "color: var(--cf-text-muted);",
                    if is_expanded() {
                        polyline { points: "6 9 12 15 18 9" }
                    } else {
                        polyline { points: "9 18 15 12 9 6" }
                    }
                }

                // Package info
                div {
                    style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",

                    // First row: package name + CVE count + exploited chip
                    div {
                        style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap;",
                        span {
                            class: "mono",
                            style: "font-size: 14px; font-weight: 700;",
                            "{group.package_name}"
                        }
                        span {
                            style: "font-size: 12px; color: var(--cf-text-muted);",
                            {
                                let cve_plural = if group.cve_count == 1 { "" } else { "s" };
                                format!("{} CVE{}", group.cve_count, cve_plural)
                            }
                        }
                        if group.exploited_count > 0 {
                            span {
                                class: "chip chip-critical",
                                style: "font-size: 10px;",
                                "{group.exploited_count} exploited"
                            }
                        }
                    }

                    // Second row: systems/patchable/outstanding
                    div {
                        style: "font-size: 11px; color: var(--cf-text-secondary);",
                        title: "{current_affected} current · {scheduled_configuration} scheduled configuration · {historical} historical evidence. Affected is the distinct current or scheduled union.",
                        {
                            let sys_plural = if group.total_affected_systems == 1 { "" } else { "s" };
                            format!("{} system{} affected · {} patchable · {} outstanding",
                                group.total_affected_systems, sys_plural,
                                group.fixable_count, group.outstanding_count)
                        }
                    }
                }

                // Severity chips
                div {
                    style: "display: flex; gap: 5px; flex-wrap: wrap; justify-content: flex-end;",
                    if group.critical_count > 0 {
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            "{group.critical_count} crit"
                        }
                    }
                    if group.high_count > 0 {
                        span {
                            class: "chip chip-warning",
                            style: "font-size: 10px;",
                            "{group.high_count} high"
                        }
                    }
                    if group.medium_count > 0 {
                        span {
                            class: "chip chip-info",
                            style: "font-size: 10px;",
                            "{group.medium_count} med"
                        }
                    }
                    if group.low_count > 0 {
                        span {
                            class: "chip chip-unknown",
                            style: "font-size: 10px;",
                            "{group.low_count} low"
                        }
                    }
                }

                // Max CVSS
                div {
                    style: "display: flex; flex-direction: column; align-items: flex-end; gap: 2px; min-width: 96px;",
                    div {
                        style: "font-size: 10px; color: var(--cf-text-muted); text-transform: uppercase; letter-spacing: 0.06em;",
                        "Worst CVSS"
                    }
                    if let Some(cvss) = group.max_cvss {
                        div {
                            style: "display: flex; align-items: center; gap: 6px;",
                            div {
                                style: "width: 50px; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                                div {
                                    style: "width: {cvss * 10.0}%; height: 100%; background: {sev_color};",
                                }
                            }
                            span {
                                class: "mono",
                                style: "font-size: 12px; color: var(--cf-text-primary); font-weight: 600;",
                                "{cvss:.1}"
                            }
                        }
                    }
                }
            }

            // Expanded CVE list
            if is_expanded() {
                if let Some(cves) = &group.cves {
                    div {
                        style: "border-top: 1px solid var(--cf-divider);",
                        table {
                            class: "sys-table",
                            style: "font-size: 12px;",
                            thead {
                                tr {
                                    th { "CVE" }
                                    th { "Severity" }
                                    th { "CVSS" }
                                    th { "Title" }
                                    th { title: "Distinct systems in current or scheduled configuration exposure", "Affected" }
                                    th { "Fix" }
                                    th { "Triage" }
                                    th { "Age" }
                                }
                            }
                            tbody {
                                for cve in cves {
                                    CveRowInGroup {
                                        cve: cve.clone(),
                                        total_systems: group.total_affected_systems,
                                        on_open: move |selection: ExactCveSelection| {
                                            on_open_cve.call(selection);
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

/// CVE row inside a grouped package card (no actions column, matching JSX reference)
#[component]
fn CveRowInGroup(
    cve: CveListItem,
    total_systems: i64,
    #[props(default)] is_new: bool,
    #[props(default)] is_selected: bool,
    #[props(default)] selected: Option<Signal<BTreeSet<ExactCveSelection>>>,
    on_open: EventHandler<ExactCveSelection>,
) -> Element {
    let (current_affected, scheduled_configuration, historical) = cve.inventory_counts();
    let (triage_label, triage_class, triage_title) = triage_status_presentation(&cve.triage_status);
    let sev_cls = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "chip-critical",
        "HIGH" => "chip-warning",
        "MEDIUM" => "chip-info",
        _ => "chip-unknown",
    };
    let sev_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "#f87171",
        "HIGH" => "#fbbf24",
        "MEDIUM" => "#60a5fa",
        _ => "#9ca3af",
    };

    let selection_for_row = cve.package_name.clone().map(|package| ExactCveSelection {
        cve_id: cve.cve_id.clone(),
        package,
    });

    rsx! {
        tr {
            class: if is_selected { "cve-row-selected" } else { "" },
            style: if is_new { "cursor: pointer; background: color-mix(in oklab, var(--cf-brand-purple) 8%, transparent);" } else { "cursor: pointer;" },
            "data-testid": "cve-row",
            onclick: move |event| {
                if let Some(selection) = selection_for_row.clone() {
                    if event.modifiers().ctrl() || event.modifiers().meta() {
                        if let Some(selected) = selected { pair_selection(selected, selection); }
                    } else { on_open.call(selection); }
                }
            },

            // CVE ID
            td {
                div {
                    class: "mono",
                    style: "font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 8px;",
                    "{cve.cve_id}"
                    if is_new { span { class: "chip chip-info", "new" } }
                    if cve.exploited {
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            title: "Actively exploited in the wild",
                            "exploited"
                        }
                    }
                }
            }

            // Severity
            td {
                span {
                    class: "chip {sev_cls}",
                    span {
                        class: "chip-dot",
                        style: "background: {sev_color};",
                    }
                    "{cve.severity}"
                }
            }

            // CVSS
            td {
                if let Some(cvss) = cve.cvss_v3_score {
                    div {
                        style: "display: flex; align-items: center; gap: 6px;",
                        div {
                            style: "width: 40px; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                            div {
                                style: "width: {cvss * 10.0}%; height: 100%; background: {sev_color};",
                            }
                        }
                        span {
                            class: "mono",
                            style: "font-size: 12px; color: var(--cf-text-primary); font-weight: 600;",
                            "{cvss:.1}"
                        }
                    }
                }
            }

            // Title
            td {
                style: "font-size: 13px; max-width: 340px;",
                div {
                    class: "truncate",
                    title: "{cve.title}",
                    "{cve.title}"
                }
            }

            // Affected
            td {
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    // Server icon
                    svg {
                        width: "11",
                        height: "11",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "color: var(--cf-text-muted);",
                        rect { x: "2", y: "2", width: "20", height: "8", rx: "2", ry: "2" }
                        rect { x: "2", y: "14", width: "20", height: "8", rx: "2", ry: "2" }
                        line { x1: "6", y1: "6", x2: "6.01", y2: "6" }
                        line { x1: "6", y1: "18", x2: "6.01", y2: "18" }
                    }
                    span {
                        class: "mono",
                        title: "{current_affected} current · {scheduled_configuration} scheduled configuration · {historical} historical evidence. Affected is the distinct current or scheduled union.",
                        style: if cve.affected_count > 0 { "font-size: 12px; font-weight: 600; color: var(--cf-text-primary);" } else { "font-size: 12px; font-weight: 600; color: var(--cf-text-muted);" },
                        "{cve.affected_count}"
                    }
                    if total_systems > 0 {
                        span {
                            style: "font-size: 11px; color: var(--cf-text-muted);",
                            "/ {total_systems}"
                        }
                    }
                }
            }

            // Fix Status
            td {
                if cve.fix_status == "fix_available" {
                    span {
                        class: "chip chip-healthy",
                        title: "{fixed_version_label(cve.fixed_version.as_deref(), true)}",
                        // Check icon
                        svg {
                            width: "10",
                            height: "10",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "3",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "display: inline; vertical-align: middle;",
                            polyline { points: "20 6 9 17 4 12" }
                        }
                        " {fixed_version_label(cve.fixed_version.as_deref(), true)}"
                    }
                } else {
                    span {
                        class: "chip chip-warning",
                        "no patch yet"
                    }
                }
            }

            // Triage Status
            td {
                span { class: "chip {triage_class}", title: "{triage_title}", "{triage_label}" }
            }

            // Age
            td {
                style: "font-size: 12px; color: var(--cf-text-muted);",
                "{cve.age_days}d"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CVE Detail Drawer
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum FleetDetailState {
    Loading,
    Loaded(poam_api::FleetCveDetail),
    Empty,
    Unauthorized,
    Error(String),
}

fn fleet_error_state(error: &PoamApiError) -> FleetDetailState {
    match error {
        PoamApiError::Server(server) if server.status == 401 || server.status == 403 => {
            FleetDetailState::Unauthorized
        }
        PoamApiError::Server(server) if server.status == 404 => FleetDetailState::Empty,
        _ => FleetDetailState::Error(error.to_string()),
    }
}

fn request_token_is_current(component_active: bool, requested: u64, current: u64) -> bool {
    component_active && requested == current
}

fn fleet_rollup_label(rollup: poam_api::FleetCveTriageRollup) -> &'static str {
    match rollup {
        poam_api::FleetCveTriageRollup::Outstanding => "OUTSTANDING",
        poam_api::FleetCveTriageRollup::Accepted => "ACCEPTED",
        poam_api::FleetCveTriageRollup::Scheduled => "SCHEDULED",
        poam_api::FleetCveTriageRollup::Partial => "MIXED",
    }
}

fn fleet_rollup_class(rollup: poam_api::FleetCveTriageRollup) -> &'static str {
    match rollup {
        poam_api::FleetCveTriageRollup::Outstanding => "chip-critical",
        poam_api::FleetCveTriageRollup::Accepted => "chip-info",
        poam_api::FleetCveTriageRollup::Scheduled => "chip-info",
        poam_api::FleetCveTriageRollup::Partial => "chip-warning",
    }
}

fn fleet_severity_color(severity: &str) -> &'static str {
    match severity.to_ascii_lowercase().as_str() {
        "critical" => "#f87171",
        "high" => "#fbbf24",
        "medium" => "#60a5fa",
        _ => "#9ca3af",
    }
}

fn fleet_severity_chip_class(severity: &str) -> &'static str {
    match severity.to_ascii_lowercase().as_str() {
        "critical" => "chip-critical",
        "high" => "chip-warning",
        "medium" => "chip-info",
        _ => "chip-unknown",
    }
}

fn fleet_human_date(date: chrono::NaiveDate) -> String {
    date.format("%b %-d, %Y").to_string()
}

fn fleet_fix_label(detail: &poam_api::FleetCveDetail) -> String {
    fixed_version_label(
        detail.cve.fixed_version.as_deref(),
        detail.cve.fix_status == "fix_available",
    )
}

fn fleet_assignee_label(assignee: &poam_api::PoamAssigneeView) -> String {
    match assignee {
        poam_api::PoamAssigneeView::User { display, .. }
        | poam_api::PoamAssigneeView::OidcGroup { display, .. }
        | poam_api::PoamAssigneeView::Legacy { display } => display.clone(),
        poam_api::PoamAssigneeView::Unassigned => "Unassigned".to_string(),
    }
}

fn fleet_risk_label(risk: poam_api::PoamRisk) -> &'static str {
    match risk {
        poam_api::PoamRisk::High => "CAT I - High",
        poam_api::PoamRisk::Medium => "CAT II - Medium",
        poam_api::PoamRisk::Low => "CAT III - Low",
    }
}

#[component]
fn ExactCveFleetDrawer(selection: ExactCveSelection, on_close: EventHandler<()>) -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let can_triage = auth::is_operator_or_above(&app_state.read().auth);
    let navigator = navigator();
    let mut state = use_signal(|| FleetDetailState::Loading);
    let mut load_generation = use_signal(|| 0_u64);
    let mut refresh_generation = use_signal(|| 0_u64);
    let mut refreshing = use_signal(|| false);
    let mut triage_open = use_signal(|| false);
    let mut mutation_error = use_signal(|| None::<String>);
    let mut result_poam = use_signal(|| None::<(uuid::Uuid, bool)>);
    let mut maximized = use_signal(|| false);
    let component_active = use_hook(|| Rc::new(Cell::new(true)));
    {
        let component_active = component_active.clone();
        use_drop(move || component_active.set(false));
    }

    use_effect({
        let selection = selection.clone();
        let component_active = component_active.clone();
        move || {
            let _refresh = refresh_generation();
            let generation = (*load_generation.peek()).wrapping_add(1);
            load_generation.set(generation);
            if !matches!(&*state.peek(), FleetDetailState::Loaded(_)) {
                state.set(FleetDetailState::Loading);
            }
            let cve_id = selection.cve_id.clone();
            let package = selection.package.clone();
            let component_active = component_active.clone();
            spawn(async move {
                let result = poam_api::fetch_fleet_cve_detail(&cve_id, &package).await;
                // CONCURRENCY: Only the newest request for this mounted exact
                // selection can replace authoritative drawer state.
                if !request_token_is_current(
                    component_active.get(),
                    generation,
                    *load_generation.peek(),
                ) {
                    return;
                }
                refreshing.set(false);
                match result {
                    Ok(detail) => state.set(FleetDetailState::Loaded(detail)),
                    Err(error) => state.set(fleet_error_state(&error)),
                }
            });
        }
    });

    let dialog_label = format!("{} {} fleet inventory", selection.cve_id, selection.package);
    let severity_color = match &*state.read() {
        FleetDetailState::Loaded(detail) => fleet_severity_color(&detail.cve.severity),
        _ => "#9ca3af",
    };
    rsx! {
        DialogFocusRestore {}
        button {
            class: "fl-tray-backdrop cve-fleet-backdrop",
            aria_label: "Close {dialog_label}",
            tabindex: "-1",
            onclick: move |_| on_close.call(()),
        }
        aside {
            id: "cve-fleet-drawer",
            class: if maximized() { "fl-tray cve-fleet-drawer cve-fleet-expanded" } else { "fl-tray cve-fleet-drawer" },
            role: "dialog",
            aria_modal: "true",
            aria_label: "{dialog_label}",
            "data-testid": "cve-fleet-drawer",
            tabindex: "-1",
            onkeydown: move |event| if event.key() == Key::Escape && !triage_open() { on_close.call(()); },
            DialogFocusSentinel { dialog_id: "cve-fleet-drawer", boundary: DialogFocusBoundary::Last }
            header { class: "fl-tray-head cve-fleet-head",
                div { class: "cve-fleet-heading",
                    span { style: "color:{severity_color};", Icon { name: IconName::Shield, size: 18 } }
                    div { class: "cve-fleet-heading-copy",
                        div { class: "cve-fleet-identity",
                            span { class: "mono", "{selection.cve_id}" }
                            if let FleetDetailState::Loaded(detail) = &*state.read() {
                                span { class: "chip {fleet_severity_chip_class(&detail.cve.severity)}",
                                    span { class: "chip-dot" }
                                    "{detail.cve.severity}"
                                }
                                if detail.cve.exploited { span { class: "chip chip-critical", "exploited in the wild" } }
                            }
                        }
                        if let FleetDetailState::Loaded(detail) = &*state.read() {
                            div { class: "cve-fleet-title", title: "{detail.cve.title}", "{detail.cve.title}" }
                        } else {
                            div { class: "cve-fleet-title mono", "{selection.package}" }
                        }
                    }
                }
                div { class: "cve-fleet-actions",
                    if let FleetDetailState::Loaded(detail) = &*state.read() {
                        a { class: "btn btn-ghost xs focus-ring", href: "https://nvd.nist.gov/vuln/detail/{detail.cve.cve_id}", target: "_blank", rel: "noopener noreferrer", title: "Open NVD advisory", "data-testid": "cve-advisory-link", Icon { name: IconName::Link, size: 11 } " Advisory" }
                        if can_triage {
                            button {
                                class: if detail.exact_mutation_target_count == 0 {
                                    "btn btn-primary xs focus-ring cve-triage-disabled"
                                } else if detail.rollup == poam_api::FleetCveTriageRollup::Outstanding {
                                    "btn btn-primary xs focus-ring"
                                } else {
                                    "btn btn-ghost xs focus-ring"
                                },
                                "data-testid": "cve-triage-open",
                                disabled: detail.exact_mutation_target_count == 0,
                                title: if detail.exact_mutation_target_count == 0 {
                                    "Exact current scan evidence is required for fleet triage."
                                } else if detail.rollup == poam_api::FleetCveTriageRollup::Outstanding {
                                    "Triage exact affected environments"
                                } else {
                                    "Edit triage for exact affected environments"
                                },
                                onclick: move |_| { mutation_error.set(None); triage_open.set(true); },
                                Icon { name: IconName::Shield, size: 11 }
                                if detail.exact_mutation_target_count == 0 {
                                    " Triage unavailable"
                                } else if detail.rollup == poam_api::FleetCveTriageRollup::Outstanding {
                                    " Triage"
                                } else {
                                    " Edit triage"
                                }
                            }
                        }
                    }
                    button { class: "btn-icon focus-ring", aria_label: if maximized() { "Restore fleet inventory" } else { "Maximize fleet inventory" }, title: if maximized() { "Restore" } else { "Maximize" }, onclick: move |_| maximized.toggle(), Icon { name: if maximized() { IconName::Minimize } else { IconName::Maximize }, size: 15 } }
                    button { class: "btn-icon focus-ring", aria_label: "Close fleet inventory", autofocus: true, onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
                }
            }
            div { class: "ed-body cve-fleet-body",
                if refreshing() {
                    div { class: "sd-callout sd-callout-warn", role: "status", "Fleet evidence changed. Refreshing authoritative detail..." }
                }
                if let Some(error) = mutation_error() {
                    div { class: "sd-callout sd-callout-danger", role: "alert", "{error}" }
                }
                if let Some((poam_id, reused)) = result_poam() {
                    div { class: "sd-callout sd-callout-info", role: "status",
                        if reused { "Scheduled environments use the existing POA&M. " } else { "Scheduled environments now use a new POA&M. " }
                        button { class: "btn btn-ghost xs focus-ring", "data-testid": "cve-triage-poam-link", onclick: move |_| { navigator.push(Route::ComplianceView { bundle: String::new(), version: String::new(), system: String::new(), policy: String::new(), poam: poam_id.to_string(), view: String::new() }); }, "Open POA&M" }
                    }
                }
                match &*state.read() {
                    FleetDetailState::Loading => rsx! { div { class: "empty", role: "status", "Loading fleet inventory..." } },
                    FleetDetailState::Empty => rsx! { div { class: "empty", h3 { "No current inventory findings" } p { "No visible active system currently reports this CVE and package." } } },
                    FleetDetailState::Unauthorized => rsx! { div { class: "empty", role: "alert", h3 { "Fleet detail unavailable" } p { "Your session cannot read this fleet inventory." } } },
                    FleetDetailState::Error(error) => rsx! { div { class: "empty", role: "alert", h3 { "Could not load fleet detail" } p { "{error}" } button { class: "btn btn-ghost focus-ring", onclick: move |_| { refreshing.set(true); let next = (*refresh_generation.peek()).wrapping_add(1); refresh_generation.set(next); }, "Retry" } } },
                    FleetDetailState::Loaded(detail) => rsx! { FleetCveDetailBody { detail: detail.clone() } },
                }
            }
            DialogFocusSentinel { dialog_id: "cve-fleet-drawer", boundary: DialogFocusBoundary::First }
        }
        if triage_open() {
            if let FleetDetailState::Loaded(detail) = &*state.read() {
                FleetCveTriageDialog {
                    key: "{detail.cve.cve_id}|{detail.canonical_package_name}",
                    detail: detail.clone(),
                    on_close: move |_| triage_open.set(false),
                    on_success: move |response: poam_api::FleetCveTriageResponse| {
                        result_poam.set(response.poam_id.map(|id| (id, response.poam_reused)));
                        mutation_error.set(None);
                        triage_open.set(false);
                        refreshing.set(true);
                        let next = (*refresh_generation.peek()).wrapping_add(1);
                        refresh_generation.set(next);
                    },
                    on_conflict: move |message: String| {
                        mutation_error.set(Some(message));
                        triage_open.set(false);
                        refreshing.set(true);
                        let next = (*refresh_generation.peek()).wrapping_add(1);
                        refresh_generation.set(next);
                    },
                }
            }
        }
    }
}

#[component]
fn FleetCveDetailBody(detail: poam_api::FleetCveDetail) -> Element {
    let total = detail.affected_system_count;
    let (current_affected, scheduled_configuration, historical) = detail.inventory_counts();
    let legacy = detail.legacy_affected_system_count;
    let no_scan = detail.no_scan_system_count;
    let no_scan_hosts = if no_scan == 1 {
        "active host"
    } else {
        "active hosts"
    };
    let no_scan_verb = if no_scan == 1 { "has" } else { "have" };
    let historical_hosts = if historical == 1 { "host" } else { "hosts" };
    let legacy_hosts = if legacy == 1 { "host" } else { "hosts" };
    let cvss = detail
        .cve
        .cvss_v3_score
        .map(|score| format!("{score:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let fixed_version = fleet_fix_label(&detail);
    let installed_version = detail
        .cve
        .installed_version
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let published = detail
        .cve
        .published_date
        .map(fleet_human_date)
        .unwrap_or_else(|| "Unknown".to_string());
    let cvss_vector = detail
        .cve
        .cvss_vector
        .clone()
        .unwrap_or_else(|| "Not published".to_string());
    let advisory_url = format!("https://nvd.nist.gov/vuln/detail/{}", detail.cve.cve_id);
    let dispositioned = detail
        .environments
        .iter()
        .filter(|environment| {
            environment_triage_eligible(environment) && environment.disposition.is_some()
        })
        .map(|environment| environment.inventory_counts().0)
        .sum::<i64>();
    rsx! {
        div { class: "ed-stats cve-fleet-stats",
            div { class: "ed-stat", div { class: "ed-stat-label", "CVSS" } div { class: "ed-stat-val", "{cvss}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Package" } div { class: "ed-stat-val mono", "{detail.canonical_package_name}" } }
            div { class: "ed-stat", title: "Distinct current or scheduled configuration systems", div { class: "ed-stat-label", "Affected" } div { class: "ed-stat-val", "{total}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Fix" } div { class: "ed-stat-val mono cve-fix-value", "{fixed_version}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Published" } div { class: "ed-stat-val cve-date-value", "{published}" } }
        }
        section { class: "cve-fleet-section cve-vector", "data-testid": "cve-cvss-vector",
            h3 { "CVSS vector" }
            code { class: "mono", "{cvss_vector}" }
        }
        section { class: "cve-fleet-section", "data-testid": "cve-triage-status",
            div { class: "cve-section-head",
                h3 { "Triage status" }
                if detail.exact_mutation_target_count > 0 {
                    span { "{dispositioned} of {detail.exact_mutation_target_count} current hosts dispositioned" }
                }
            }
            p { class: "cve-fleet-truth", "Decisions apply to current exact findings. Accepting risk records rationale; it does not pass or remediate a result." }
            if detail.environments.is_empty() {
                div { class: "empty", "No visible affected environments." }
            }
            for environment in detail.environments.iter().filter(|environment| environment_triage_eligible(environment)).cloned() {
                FleetEnvironmentCard { environment }
            }
            if detail.exact_mutation_target_count == 0 {
                p { class: "cve-fleet-readonly-note", "No current exact scan findings are available for fleet triage." }
            }
        }
        section { class: "cve-fleet-section cve-remediation", "data-testid": "cve-remediation",
            h3 { "Remediation" }
            if detail.cve.fixed_version.as_ref().is_some_and(|version| !version.trim().is_empty()) {
                div { class: "sd-callout sd-callout-info", Icon { name: IconName::Check, size: 13 } div { "Fixed in " strong { class: "mono", "{detail.canonical_package_name}-{fixed_version}" } ". Affected systems clear only after deployment and an exact follow-up scan verifies absence." } }
            } else if detail.cve.fix_status == "fix_available" {
                div { class: "sd-callout sd-callout-info", Icon { name: IconName::Check, size: 13 } div { strong { "A patched release is available, but the exact version is pending. " } "Affected systems clear only after deployment and an exact follow-up scan verifies absence." } }
            } else {
                div { class: "sd-callout sd-callout-danger", Icon { name: IconName::Warn, size: 13 } div { strong { "No upstream patch is reported. " } "Watch the advisory and record compensating controls in accepted-risk rationale or the remediation plan." } }
            }
            dl { class: "kv-grid cve-remediation-meta",
                dt { "Observed version" } dd { class: "mono", "{installed_version}" }
                dt { "Fixed in" } dd { class: "mono", "{fixed_version}" }
                dt { "Advisory" } dd { a { href: "{advisory_url}", target: "_blank", rel: "noopener noreferrer", "nvd.nist.gov" } }
            }
        }
        details { class: "cve-authority-details", "data-testid": "cve-authority-details",
            summary {
                "Evidence authority · {current_affected} current · {scheduled_configuration} scheduled · {historical} historical"
            }
            div { class: "cve-authority-detail-grid",
                div { span { "Current exact exposure" } strong { "{current_affected}" } }
                div { span { "Scheduled target exposure" } strong { "{scheduled_configuration}" } }
                div { span { "Historical retained evidence" } strong { "{historical}" } }
                div { span { "Legacy evidence" } strong { "{legacy}" } }
                div { span { "Actionable current hosts" } strong { "{detail.exact_mutation_target_count}" } }
                div { span { "Triage rollup" } strong { class: "chip {fleet_rollup_class(detail.rollup)}", "{fleet_rollup_label(detail.rollup)}" } }
            }
            if historical > 0 || legacy > 0 {
                p { "Historical and legacy evidence is read-only and cannot authorize fleet triage. {historical} {historical_hosts} have retained historical findings; {legacy} {legacy_hosts} are legacy-only evidence." }
            }
            if scheduled_configuration > 0 {
                p { "Scheduled configuration findings describe deployment intent. They are not POA&M patch scheduling or fleet-triage targets." }
            }
            if no_scan > 0 {
                p { "{no_scan} {no_scan_hosts} {no_scan_verb} no usable completed CVE scan. They are not counted as affected." }
            }
        }
        section { class: "cve-fleet-section", "data-testid": "cve-affected-systems",
            h3 { "Affected systems · {total} current or scheduled" }
            for (section, section_count, description) in [
                (FleetCveInventorySection::Current, current_affected, "Exact current deployment findings. Environment-assigned exact subjects can be triaged."),
                (FleetCveInventorySection::ScheduledDeploymentTarget, scheduled_configuration, "Exact active deployment targets. Scheduled configuration is read-only and is not POA&M Patch scheduled triage."),
                (FleetCveInventorySection::Historical, historical, "Retained evidence outside current and scheduled configuration exposure. Historical evidence is read-only."),
            ] {
                if section_count > 0 {
                    div { class: "cve-inventory-section", "data-testid": "cve-inventory-section", "data-section": "{inventory_section_value(section)}",
                        div { class: "cve-section-head", h3 { "{inventory_section_label(section)} · {section_count}" } }
                        p { class: "cve-fleet-truth", "{description}" }
                        for environment in detail.environments.clone() {
                            FleetAffectedEnvironment { environment, section }
                        }
                        { let unassigned = systems_in_inventory_section(&detail.unassigned_systems, section); rsx! {
                            if !unassigned.is_empty() {
                                article { class: "cve-inventory-env", "data-testid": "cve-fleet-unassigned",
                                    header { div { strong { "Unassigned" } span { "{unassigned.len()} host(s)" } } span { class: "chip", "READ ONLY" } }
                                    small { "These Admin-visible hosts have no environment. They cannot be fleet triage targets." }
                                    FleetHostRows { systems: unassigned }
                                }
                            }
                        } }
                    }
                }
            }
        }
    }
}

#[component]
fn FleetEnvironmentCard(environment: poam_api::CveAffectedEnvironment) -> Element {
    let current_affected = environment.inventory_counts().0;
    let can_triage = environment_triage_eligible(&environment);
    let state_label = match &environment.disposition {
        Some(poam_api::CveEnvironmentDisposition::Accepted { .. }) => "Risk accepted",
        Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }) => "Patch scheduled",
        None => "Outstanding",
    };
    let state_class = match &environment.disposition {
        Some(poam_api::CveEnvironmentDisposition::Accepted { .. }) => "chip-info cve-env-accepted",
        Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }) => {
            "chip-info cve-env-scheduled"
        }
        None => "chip-critical",
    };
    let state_test_id = state_label.to_ascii_lowercase().replace(' ', "-");
    rsx! {
        article { class: "cve-fleet-env", "data-testid": "cve-fleet-environment", "data-state": "{state_test_id}",
            header {
                div { strong { "{environment.environment_name}" } span { class: "mono", "{current_affected} host" if current_affected != 1 { "s" } }
                    span { class: "chip {state_class}", "{state_label}" }
                }
            }
            match &environment.disposition {
                Some(poam_api::CveEnvironmentDisposition::Accepted { justification, review_date, actor, accepted_at }) => { let accepted_date = fleet_human_date(accepted_at.date_naive()); rsx! {
                    p { class: "cve-fleet-env-rationale", "{justification}" }
                    small { class: "cve-fleet-env-meta", "Accepted by {actor.display} · {accepted_date}" if let Some(review_date) = review_date { " · review {fleet_human_date(*review_date)}" } else { " · no review date" } }
                } },
                Some(poam_api::CveEnvironmentDisposition::Scheduled { poam_id, poam, actor, scheduled_at }) => { let scheduled_date = fleet_human_date(scheduled_at.date_naive()); let label = poam.as_ref().map(|poam| format!("{}: {}", poam.human_id, poam.title)).unwrap_or_else(|| format!("POA&M {poam_id}")); rsx! {
                    if let Some(poam) = poam {
                        if !poam.plan.trim().is_empty() { p { class: "cve-scheduled-plan", "{poam.plan}" } }
                        div { class: "cve-scheduled-meta",
                            span { "Owner" strong { "{fleet_assignee_label(&poam.assignee)}" } }
                            span { "Target" strong { "{fleet_human_date(poam.target_date)}" } }
                            span { "Risk" strong { "{fleet_risk_label(poam.risk)}" } }
                        }
                    }
                    div { class: "cve-fleet-scheduled", span { "Scheduled by {actor.display} · {scheduled_date}" } Link { to: Route::ComplianceView { bundle: String::new(), version: String::new(), system: String::new(), policy: String::new(), poam: poam_id.to_string(), view: String::new() }, class: "poam-ref focus-ring", Icon { name: IconName::File, size: 11 } " {label}" } }
                } },
                None => rsx! { small { class: "cve-fleet-env-meta", if can_triage { "No disposition covers {current_affected} current hosts." } else { "No source-authorized triage target is available." } } },
            }
        }
    }
}

#[component]
fn FleetAffectedEnvironment(
    environment: poam_api::CveAffectedEnvironment,
    section: FleetCveInventorySection,
) -> Element {
    let systems = systems_in_inventory_section(&environment.systems, section);
    let counts = environment.inventory_counts();
    let section_count = match section {
        FleetCveInventorySection::Current => counts.0,
        FleetCveInventorySection::ScheduledDeploymentTarget => counts.1,
        FleetCveInventorySection::Historical => counts.2,
    };
    if section_count == 0 {
        return rsx! {};
    }
    rsx! {
        article { class: "cve-inventory-env", "data-testid": "cve-affected-environment",
            header {
                div { strong { "{environment.environment_name}" } span { class: "mono", "{section_count} host(s)" } }
                span { class: "cve-inventory-authority", "{inventory_section_row_label(section)}" }
            }
            FleetHostRows { systems: systems.clone() }
            if systems.len() < section_count.max(0) as usize {
                small { "Showing {systems.len()} of {section_count} hosts in this section." }
            }
        }
    }
}

#[component]
fn FleetHostRows(systems: Vec<crate::api::models::CveAffectedSystemDetail>) -> Element {
    rsx! {
        div { class: "cve-fleet-hosts",
            for system in systems {
                div { class: "cve-fleet-host", "data-testid": "cve-fleet-host",
                    Link { to: Route::SystemDetailView { id: system.system_id.to_string(), tab: "cves".to_string(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }, class: "mono focus-ring cve-host-name", "{system.hostname}" }
                    span { class: "mono truncate", title: "{system.flake_name.as_deref().unwrap_or(\"Unknown flake\")}", "{system.flake_name.as_deref().unwrap_or(\"Unknown flake\")}" }
                    span { class: "mono truncate", title: "{system.commit_hash.as_deref().unwrap_or(\"Unknown revision\")}", "{system.commit_hash.as_deref().unwrap_or(\"Unknown revision\")}" }
                    span { class: "mono", "{system.current_package_version.as_deref().unwrap_or(\"Unknown version\")}" }
                    span { class: "chip", "{inventory_section_row_label(system.inventory_section)}" }
                    span { class: "chip", if system.inventory_authority == SystemCveInventoryAuthority::Exact { "EXACT" } else { "LEGACY EVIDENCE" } }
                    Link { to: Route::SystemDetailView { id: system.system_id.to_string(), tab: "cves".to_string(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }, class: "btn-icon focus-ring", aria_label: "Open {system.hostname}", Icon { name: IconName::ArrowRight, size: 13 } }
                }
            }
        }
    }
}

#[component]
fn FleetCveTriageDialog(
    detail: poam_api::FleetCveDetail,
    on_close: EventHandler<()>,
    on_success: EventHandler<poam_api::FleetCveTriageResponse>,
    on_conflict: EventHandler<String>,
) -> Element {
    let mut draft = use_signal(|| fleet_triage_draft(&detail));
    let mut catalog = use_signal(|| None::<Result<poam_api::PoamAssigneeCatalog, String>>);
    let mut error = use_signal(|| None::<String>);
    let mut pending = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            catalog.set(Some(
                poam_api::fetch_assignee_catalog()
                    .await
                    .map_err(|error| error.to_string()),
            ));
        });
    });
    let scheduled = draft
        .read()
        .environments
        .iter()
        .any(|environment| environment.choice == EnvironmentTriageChoice::Scheduled);
    let existing_poam_reuse = scheduled && draft.read().reuses_existing_poam();
    let hydrated_assignee = draft.read().hydrated_assignee.clone();
    let hydrated_assignee_in_catalog = hydrated_assignee.as_ref().is_some_and(|assignee| {
        catalog
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .is_some_and(|catalog| catalog_contains_assignee(catalog, &assignee.value))
    });
    let accepted_count = draft
        .read()
        .environments
        .iter()
        .filter(|environment| environment.choice == EnvironmentTriageChoice::Accepted)
        .map(|environment| {
            detail
                .environments
                .iter()
                .find(|candidate| candidate.environment_id == environment.environment_id)
                .map_or(0, |candidate| candidate.inventory_counts().0)
        })
        .sum::<i64>();
    let scheduled_count = draft
        .read()
        .environments
        .iter()
        .filter(|environment| environment.choice == EnvironmentTriageChoice::Scheduled)
        .map(|environment| {
            detail
                .environments
                .iter()
                .find(|candidate| candidate.environment_id == environment.environment_id)
                .map_or(0, |candidate| candidate.inventory_counts().0)
        })
        .sum::<i64>();
    let open_count = detail.exact_mutation_target_count - accepted_count - scheduled_count;
    let cvss = detail
        .cve
        .cvss_v3_score
        .map(|score| format!("{score:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let fix = fleet_fix_label(&detail);
    let dialog_label = format!(
        "Triage {} {}",
        detail.cve.cve_id, detail.canonical_package_name
    );
    let submit_detail = detail.clone();
    let submit = move |_: MouseEvent| {
        let request = match draft
            .read()
            .fleet_request(&submit_detail.canonical_package_name)
        {
            Ok(request) => request,
            Err(message) => {
                error.set(Some(message));
                return;
            }
        };
        pending.set(true);
        error.set(None);
        let cve_id = submit_detail.cve.cve_id.clone();
        spawn(async move {
            match poam_api::triage_fleet_cve(&cve_id, &request).await {
                Ok(response) => {
                    pending.set(false);
                    on_success.call(response);
                }
                Err(PoamApiError::Server(server))
                    if server.status == 409 || server.status == 412 =>
                {
                    pending.set(false);
                    on_conflict.call(format!(
                        "{} Refresh completed with the server's current exact fleet state.",
                        server.message
                    ));
                }
                Err(request_error) => {
                    pending.set(false);
                    error.set(Some(format!("Triage was not applied: {request_error}")));
                }
            }
        });
    };
    rsx! {
        DialogFocusRestore {}
        // The triage editor opens on top of the fleet drawer, so the browser
        // never applies its close button's `autofocus`. Move focus explicitly
        // to keep the nested modal keyboard-reachable and trapped.
        DialogInitialFocus { dialog_id: "cve-triage-dialog" }
        button { class: "modal-backdrop cve-triage-backdrop", aria_label: "Close {dialog_label}", tabindex: "-1", onclick: move |_| if !pending() { on_close.call(()) } }
        div { id: "cve-triage-dialog", class: "modal cve-triage-modal", role: "dialog", aria_modal: "true", aria_label: "{dialog_label}", "data-testid": "cve-triage-dialog", tabindex: "-1", onkeydown: move |event| if event.key() == Key::Escape && !pending() { event.stop_propagation(); on_close.call(()); },
            DialogFocusSentinel { dialog_id: "cve-triage-dialog", boundary: DialogFocusBoundary::Last }
            div { class: "modal-head", div { h2 { "Triage {detail.cve.cve_id}" } p { "Decide per environment. Historical inventory remains read-only." } } button { class: "btn-icon focus-ring", aria_label: "Close triage editor", autofocus: true, disabled: pending(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } } }
            div { class: "modal-body cve-triage-body",
                p { "Choose one intention for each environment with current exact exposure. Scheduled configuration and historical evidence remain read-only. The server recomputes current exact host scope when you submit." }
                div { class: "cve-triage-context", "data-testid": "cve-triage-context",
                    header { Icon { name: IconName::Shield, size: 12 } " Vulnerability" span { "Exact scope is carried over automatically" } }
                    div { class: "cve-triage-context-grid",
                        div { span { "CVE" } strong { class: "mono", "{detail.cve.cve_id}" } }
                        div { span { "Package" } strong { class: "mono", "{detail.canonical_package_name}" } }
                        div { span { "CVSS" } strong { "{cvss} · {detail.cve.severity}" } }
                        div { span { "Actionable hosts" } strong { "{detail.exact_mutation_target_count}" } }
                        div { span { "Fix" } strong { class: "mono", "{fix}" } }
                        div { span { "Exploited" } strong { if detail.cve.exploited { "yes — in the wild" } else { "not observed" } } }
                    }
                }
                if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                for environment in detail.environments.clone().into_iter().filter(|environment| environment_triage_eligible(environment)) {
                    { let environment_id = environment.environment_id; let current = draft.read().environments.iter().find(|item| item.environment_id == environment_id).cloned(); rsx! {
                        fieldset { class: "cve-triage-env", "data-testid": "cve-triage-environment",
                            legend { "{environment.environment_name} · {environment.inventory_counts().0} current exact host(s)" }
                            div { class: "seg", role: "group", aria_label: "Disposition for {environment.environment_name}",
                                for (choice, label) in [(EnvironmentTriageChoice::Open, "Leave open"), (EnvironmentTriageChoice::Accepted, "Accept risk"), (EnvironmentTriageChoice::Scheduled, "Schedule patch through POA&M")] {
                                    button { r#type: "button", class: if current.as_ref().map(|item| item.choice) == Some(choice) { "active" } else { "" }, aria_pressed: if current.as_ref().map(|item| item.choice) == Some(choice) { "true" } else { "false" }, "data-action": "{choice.value()}", onclick: move |_| draft.write().set_choice(environment_id, choice), "{label}" }
                                }
                            }
                            if current.as_ref().map(|item| item.choice) == Some(EnvironmentTriageChoice::Accepted) {
                                label { class: "field", span { "Justification · required" } textarea { value: "{current.as_ref().map(|item| item.justification.as_str()).unwrap_or_default()}", "data-testid": "cve-accept-justification", oninput: move |event| if let Some(item) = draft.write().environments.iter_mut().find(|item| item.environment_id == environment_id) { item.justification = event.value(); } } }
                                label { class: "field", span { "Review date · optional" } input { r#type: "date", value: "{current.as_ref().map(|item| item.review_date.as_str()).unwrap_or_default()}", "data-testid": "cve-accept-review-date", oninput: move |event| if let Some(item) = draft.write().environments.iter_mut().find(|item| item.environment_id == environment_id) { item.review_date = event.value(); } } }
                            }
                        }
                    } }
                }
                if scheduled {
                    fieldset { class: "cve-triage-poam", legend { "Shared POA&M for scheduled environments" }
                        if existing_poam_reuse {
                            div { class: "sd-callout sd-callout-info", "This schedule will reuse the existing compatible POA&M. Its metadata and milestones are not changed. Verification and closure require a later exact scan that no longer reports this CVE and package." }
                        } else {
                            div { class: "sd-callout sd-callout-info", "The POA&M owns remediation for scheduled exact subjects. Verification and closure require a later exact scan that no longer reports this CVE and package." }
                        }
                        if let Some(message) = &draft.read().preservation_error { div { class: "sd-callout sd-callout-warn", role: "alert", "{message}" } }
                        div { class: "cve-triage-poam-grid",
                            label { class: "field", span { "Owner" }
                                select { value: "{draft.read().assignee}", "data-testid": "cve-poam-assignee", disabled: existing_poam_reuse || catalog.read().is_none(), onchange: move |event| draft.write().assignee = event.value(),
                                    option { value: "", disabled: true, "Select a user or group" }
                                    if let Some(assignee) = hydrated_assignee.as_ref().filter(|_| !hydrated_assignee_in_catalog) { option { value: "{assignee.value}", "{assignee.label} (current)" } }
                                    if let Some(Ok(catalog)) = &*catalog.read() {
                                        optgroup { label: "People", for person in &catalog.people { option { value: "user:{person.user_id}", "{person.label}" } } }
                                        optgroup { label: "Groups", for group in &catalog.groups { option { value: "group:{group.group_name}", "{group.group_name}" } } }
                                    }
                                }
                                if let Some(Err(message)) = &*catalog.read() { small { role: "alert", "Assignees unavailable: {message}" } }
                            }
                            label { class: "field", span { "Target completion" } input { r#type: "date", value: "{draft.read().target_date}", "data-testid": "cve-poam-target", readonly: existing_poam_reuse, oninput: move |event| draft.write().target_date = event.value() } }
                        }
                        label { class: "field", span { "Remediation plan · optional now, expected before review" } textarea { value: "{draft.read().plan}", "data-testid": "cve-poam-plan", readonly: existing_poam_reuse, oninput: move |event| draft.write().plan = event.value() } }
                        if existing_poam_reuse {
                            small { "Existing milestones remain unchanged." }
                        } else {
                            label { class: "poam-check", input { r#type: "checkbox", checked: draft.read().default_milestones, onchange: move |event| draft.write().default_milestones = event.checked() } span { "Start from standard patch milestones " small { "— identify version, staging, rollout, verify scan. Editable after creation." } } }
                        }
                    }
                }
            }
            div { class: "modal-foot cve-triage-foot",
                div { class: "cve-triage-outcome", "{accepted_count} accepted · {scheduled_count} scheduled · {open_count.max(0)} open" }
                button { class: "btn btn-ghost focus-ring", disabled: pending(), onclick: move |_| on_close.call(()), "Cancel" }
                button { class: "btn btn-primary focus-ring", "data-testid": "cve-triage-submit", disabled: pending() || (scheduled && draft.read().preservation_error.is_some()), onclick: submit, if pending() { "Applying..." } else { "Apply triage" } }
            }
            DialogFocusSentinel { dialog_id: "cve-triage-dialog", boundary: DialogFocusBoundary::First }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CveSeenState, FleetDetailState, QuickCveSelection, ToastLifecycle, authenticated_user_id,
        complete_focus_package, deployment_dot, environment_triage_eligible, fleet_error_state,
        fleet_fix_label, fleet_triage_draft, has_retained_package_evidence, initially_expanded,
        inventory_section_label, is_canonical_cve_id, loaded_package_groups, matches_pair_filters,
        ordered_package_groups, pair_metadata_for_member, recently_observed_unseen,
        request_token_is_current, seen_for_user, seen_storage_key, systems_in_inventory_section,
        toggle_quick_pairs, triage_status_presentation, unique_retained_package_for_cve,
        unseen_new_pairs,
    };
    use crate::api::models::{
        AuthContext, AuthMode, AuthUser, CveFilters, CveInventoryMember, CveInventoryPairPage,
        CveInventoryQuery, CveListItem, FleetCveInventorySection, SystemCveInventoryAuthority,
    };
    use crate::components::cve::triage::{
        CveTriageDraft, EnvironmentTriageChoice, EnvironmentTriageDraft,
    };
    use crate::views::poam_api::{
        self, CveEnvironmentTriageAction, PoamApiError, PoamRisk, PoamServerError,
    };
    use chrono::{Duration, TimeZone, Utc};
    use std::collections::BTreeSet;
    use uuid::Uuid;

    #[test]
    fn seen_storage_requires_authenticated_nonempty_user_id() {
        let user = AuthUser {
            id: "user-a".into(),
            email: "a@example.test".into(),
            display_name: None,
        };
        let mut context = AuthContext {
            is_authenticated: false,
            user: Some(user),
            roles: vec![],
            auth_mode: AuthMode::Local,
        };
        assert_eq!(authenticated_user_id(&Some(context.clone())), None);
        context.is_authenticated = true;
        assert_eq!(
            authenticated_user_id(&Some(context.clone())),
            Some("user-a".into())
        );
        context.user.as_mut().unwrap().id.clear();
        assert_eq!(authenticated_user_id(&Some(context)), None);
        assert_eq!(authenticated_user_id(&None), None);
    }

    #[test]
    fn new_marker_uses_exact_observation_and_authenticated_user_namespace() {
        let now = Utc
            .with_ymd_and_hms(2026, 9, 26, 12, 0, 0)
            .single()
            .unwrap();
        let mut row = cve_item("CVE-2025-1234", Some("openssl"), Some(1));
        row.age_days = 900;
        row.last_seen = Some(now - Duration::hours(24) + Duration::milliseconds(1));
        let mut a = CveSeenState {
            user_id: "user-a".into(),
            pairs: BTreeSet::new(),
        };
        let b = CveSeenState {
            user_id: "user-b".into(),
            pairs: BTreeSet::new(),
        };
        assert_ne!(seen_storage_key("user-a"), seen_storage_key("user-b"));
        assert!(recently_observed_unseen(
            &row,
            now,
            seen_for_user(Some(&a), Some("user-a"))
        ));
        a.pairs.insert((row.cve_id.clone(), "openssl".into()));
        let serialized = serde_json::to_string(&a.pairs).unwrap();
        let restored = CveSeenState {
            user_id: "user-a".into(),
            pairs: serde_json::from_str(&serialized).unwrap(),
        };
        assert_eq!(seen_for_user(Some(&restored), Some("user-a")), Some(&a));
        assert!(!recently_observed_unseen(
            &row,
            now,
            seen_for_user(Some(&a), Some("user-a"))
        ));
        assert!(recently_observed_unseen(
            &row,
            now,
            seen_for_user(Some(&b), Some("user-b"))
        ));
        assert!(seen_for_user(Some(&a), Some("user-b")).is_none());
        assert!(seen_for_user(Some(&a), None).is_none());
        assert!(!recently_observed_unseen(&row, now, None));
        row.last_seen = Some(now - Duration::hours(24));
        assert!(!recently_observed_unseen(&row, now, Some(&b)));
        row.last_seen = Some(now + Duration::milliseconds(1));
        assert!(!recently_observed_unseen(&row, now, Some(&b)));
        row.last_seen = None;
        assert!(!recently_observed_unseen(&row, now, Some(&b)));
    }

    #[test]
    fn package_order_prefers_unseen_new_then_highest_severity() {
        let now = Utc
            .with_ymd_and_hms(2026, 9, 26, 12, 0, 0)
            .single()
            .unwrap();
        let seen = CveSeenState {
            user_id: "user-a".into(),
            pairs: BTreeSet::new(),
        };
        let mut old_critical = cve_item("CVE-2025-0001", Some("old"), Some(1));
        old_critical.last_seen = Some(now - Duration::hours(25));
        let mut fresh_high = cve_item("CVE-2025-0002", Some("high"), Some(1));
        fresh_high.severity = "high".into();
        fresh_high.last_seen = Some(now - Duration::hours(1));
        let mut fresh_low = cve_item("CVE-2025-0003", Some("low"), Some(1));
        fresh_low.severity = "low".into();
        fresh_low.last_seen = Some(now - Duration::hours(1));
        let groups =
            ordered_package_groups(&[old_critical, fresh_low, fresh_high], now, Some(&seen));
        assert_eq!(
            groups
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["high", "low", "old"]
        );
        assert_eq!(unseen_new_pairs(&groups[0].1, now, Some(&seen)).len(), 1);
        assert!(unseen_new_pairs(&groups[2].1, now, Some(&seen)).is_empty());
        assert!(unseen_new_pairs(&groups[0].1, now, None).is_empty());
    }

    #[test]
    fn package_groups_only_count_loaded_pairs_and_metadata_matches_exact_identity() {
        let items = vec![
            cve_item("CVE-2025-1234", Some("openssl"), Some(1)),
            cve_item("CVE-2025-5678", Some("openssl"), Some(1)),
            cve_item("CVE-2025-1234", Some("glibc"), Some(1)),
        ];
        let groups = loaded_package_groups(&items);
        assert_eq!(groups["openssl"].len(), 2);
        assert_eq!(groups["glibc"].len(), 1);
        let member = CveInventoryMember {
            cve_id: "CVE-2025-1234".into(),
            package_name: Some("openssl".into()),
            system_id: Uuid::new_v4(),
            environment_id: None,
            hostname: "host".into(),
            inventory_section: "historical".into(),
            installed_version: "1".into(),
            deployment_status: None,
            flake_name: None,
        };
        assert_eq!(
            pair_metadata_for_member(&member, &items)
                .unwrap()
                .package_name
                .as_deref(),
            Some("openssl")
        );
        let other = CveInventoryMember {
            package_name: Some("unloaded".into()),
            ..member
        };
        assert!(pair_metadata_for_member(&other, &items).is_none());
    }

    #[test]
    fn quick_selection_toggles_exact_pairs_without_losing_other_selected_families() {
        let a = super::ExactCveSelection {
            cve_id: "CVE-2026-1000".into(),
            package: "nginx".into(),
        };
        let b = super::ExactCveSelection {
            cve_id: "CVE-2026-1000".into(),
            package: "openssl".into(),
        };
        let c = super::ExactCveSelection {
            cve_id: "CVE-2026-2000".into(),
            package: "nginx".into(),
        };
        let matching = BTreeSet::from([a.clone(), b.clone()]);
        assert_eq!(toggle_quick_pairs(&BTreeSet::new(), &matching), matching);
        assert_eq!(
            toggle_quick_pairs(&BTreeSet::from([a.clone(), c.clone()]), &matching),
            BTreeSet::from([a, b, c.clone()])
        );
        assert_eq!(
            toggle_quick_pairs(
                &BTreeSet::from_iter(matching.iter().cloned().chain([c.clone()])),
                &matching
            ),
            BTreeSet::from([c])
        );
    }

    #[test]
    fn quick_selection_retains_active_scope_and_does_not_replace_conflicting_filters() {
        let query = CveInventoryQuery {
            group_by: "environment".into(),
            environment_id: Some(Uuid::from_u128(11)),
            group_id: None,
            filters: CveFilters {
                severity: Some("low".into()),
                search: Some("nginx".into()),
                sort: Some("severity".into()),
                ..Default::default()
            },
            offset: 200,
            limit: 200,
        };
        assert!(QuickCveSelection::Critical.query(&query).is_none());
        let low = QuickCveSelection::Patchable.query(&query).unwrap();
        assert_eq!(low.environment_id, query.environment_id);
        assert_eq!(low.offset, 0);
        assert_eq!(low.filters.severity, query.filters.severity);
        assert_eq!(low.filters.search, query.filters.search);
        assert_eq!(low.filters.fix_status.as_deref(), Some("available"));
        assert_eq!(query.filters.fix_status, None);
    }

    #[test]
    fn every_filtered_package_group_has_matching_children_and_no_excluded_counts() {
        let now = Utc
            .with_ymd_and_hms(2026, 9, 26, 12, 0, 0)
            .single()
            .unwrap();
        let mut critical = cve_item("CVE-2025-1234", Some("nginx"), Some(1));
        critical.severity = "critical".into();
        critical.triage_status = "outstanding".into();
        critical.fix_status = "fix_available".into();
        let mut low = cve_item("CVE-2025-5678", Some("nginx"), Some(1));
        low.severity = "low".into();
        low.triage_status = "accepted".into();
        low.fix_status = "open".into();
        let mut other = cve_item("CVE-2025-9876", Some("openssl"), Some(1));
        other.severity = "critical".into();
        let all = [critical, low.clone(), other];
        let groups_for = |filters: CveFilters| {
            ordered_package_groups(
                &all.iter()
                    .filter(|row| matches_pair_filters(row, &filters))
                    .cloned()
                    .collect::<Vec<_>>(),
                now,
                None,
            )
        };
        let low_groups = groups_for(CveFilters {
            severity: Some("low".into()),
            ..Default::default()
        });
        assert_eq!(low_groups.len(), 1);
        assert_eq!(low_groups[0].0, "nginx");
        assert_eq!(low_groups[0].1.len(), 1);
        assert_eq!(low_groups[0].1[0].cve_id, low.cve_id);
        assert!(
            groups_for(CveFilters {
                severity: Some("medium".into()),
                ..Default::default()
            })
            .is_empty()
        );
        for filters in [
            CveFilters {
                triage_status: Some("accepted".into()),
                ..Default::default()
            },
            CveFilters {
                fix_status: Some("pending".into()),
                ..Default::default()
            },
            CveFilters {
                package: Some("nginx".into()),
                search: Some("5678".into()),
                ..Default::default()
            },
        ] {
            let groups = groups_for(filters);
            assert_eq!(groups.len(), 1);
            assert_eq!(groups[0].0, "nginx");
            assert_eq!(groups[0].1.len(), 1);
            assert!(groups.iter().all(|(_, children)| !children.is_empty()));
        }
        assert_eq!(groups_for(CveFilters::default()).len(), 2);
    }

    #[test]
    fn focus_never_opens_from_a_partial_or_ambiguous_pair_page() {
        let row = cve_item("CVE-2025-1234", Some("openssl"), Some(1));
        let mut page = CveInventoryPairPage {
            items: vec![row.clone()],
            total: 2,
            next_offset: Some(1),
        };
        assert_eq!(complete_focus_package(&page, "CVE-2025-1234"), None);
        page.total = 1;
        page.next_offset = None;
        assert_eq!(
            complete_focus_package(&page, "CVE-2025-1234"),
            Some("openssl".into())
        );
        page.items
            .push(cve_item("CVE-2025-1234", Some("glibc"), Some(1)));
        page.total = 2;
        assert_eq!(complete_focus_package(&page, "CVE-2025-1234"), None);
        page.items = vec![row];
        page.total = 2;
        assert_eq!(complete_focus_package(&page, "CVE-2025-1234"), None);
    }

    #[test]
    fn projection_pages_keep_group_counts_and_exact_member_grain() {
        use crate::api::models::{CveInventoryGroupPage, CveInventoryMemberPage};
        let environment = Uuid::new_v4();
        let system = Uuid::new_v4();
        let groups: CveInventoryGroupPage = serde_json::from_value(serde_json::json!({
            "items": [{"group_id": environment, "name": "Production",
                "cve_package_count": 2, "cve_count": 1,
                "critical_pair_count": 1, "high_pair_count": 1,
                "medium_pair_count": 0, "low_pair_count": 0,
                "unknown_pair_count": 0, "exploited_pair_count": 1,
                "patchable_pair_count": 1, "host_count": 1, "total_active_hosts": 4,
                "current_host_count": 1, "scheduled_host_count": 1,
                "historical_host_count": 0, "flake_name": null, "deployment_status": null}],
            "total": 3, "next_offset": 1
        }))
        .unwrap();
        assert_eq!(groups.items[0].cve_count, 1);
        assert_eq!(groups.items[0].cve_package_count, 2);
        assert_eq!(groups.items[0].host_count, 1);
        assert_eq!(groups.items[0].total_active_hosts, Some(4));
        assert_eq!(
            groups.items[0].critical_pair_count + groups.items[0].high_pair_count,
            2
        );
        assert_eq!(groups.items[0].exploited_pair_count, 1);
        assert_eq!(groups.items[0].patchable_pair_count, 1);
        assert_eq!(groups.total, 3);
        assert_eq!(groups.next_offset, Some(1));
        let members: CveInventoryMemberPage = serde_json::from_value(serde_json::json!({
            "items": [
                {"cve_id": "CVE-2025-1234", "package_name": "openssl", "system_id": system,
                 "environment_id": environment, "hostname": "host-a", "inventory_section": "current",
                 "installed_version": "1", "deployment_status": "behind", "flake_name": "fleet"},
                {"cve_id": "CVE-2025-1234", "package_name": "openssl", "system_id": system,
                 "environment_id": environment, "hostname": "host-a", "inventory_section": "scheduled_deployment_target",
                 "installed_version": "2", "deployment_status": "behind", "flake_name": "fleet"}
            ], "total": 3, "next_offset": 2
        })).unwrap();
        assert_eq!(members.items.len(), 2);
        assert_eq!(members.items[0].system_id, members.items[1].system_id);
        assert_ne!(
            members.items[0].inventory_section,
            members.items[1].inventory_section
        );
        assert_eq!(members.total, 3);
        assert_eq!(members.next_offset, Some(2));
    }

    #[test]
    fn group_defaults_and_deployment_dot_do_not_infer_scan_health() {
        assert!(initially_expanded("environment", 0, 2));
        assert!(!initially_expanded("environment", 0, 3));
        assert!(initially_expanded("host", 0, 1));
        assert!(!initially_expanded("host", 0, 2));
        assert!(!initially_expanded("host", 50, 0));
        assert_eq!(deployment_dot(Some("up_to_date")).unwrap().1, "up to date");
        assert_eq!(deployment_dot(Some("behind")).unwrap().1, "behind");
        assert_eq!(
            deployment_dot(Some("no_deployment")).unwrap().1,
            "not deployed"
        );
        assert!(deployment_dot(Some("unexpected")).is_none());
        assert!(deployment_dot(None).is_none());
    }

    fn cve_item(cve_id: &str, package: Option<&str>, current_count: Option<i64>) -> CveListItem {
        CveListItem {
            cve_id: cve_id.to_string(),
            cvss_v3_score: None,
            severity: "critical".to_string(),
            title: "Test finding".to_string(),
            cvss_vector: None,
            published_date: None,
            exploited: false,
            package_name: package.map(str::to_string),
            installed_version: None,
            fixed_version: None,
            fix_status: "unknown".to_string(),
            affected_count: 0,
            exact_affected_count: 0,
            legacy_affected_count: 0,
            current_affected_count: current_count,
            scheduled_deployment_target_count: None,
            historical_inventory_count: None,
            affected_environments: None,
            first_seen: None,
            last_seen: None,
            age_days: 0,
            triage_status: "outstanding".to_string(),
        }
    }

    #[test]
    fn notification_focus_accepts_only_canonical_cve_identifiers() {
        assert!(is_canonical_cve_id("CVE-2025-1234"));
        assert!(is_canonical_cve_id("CVE-2025-123456"));
        for value in [
            "CVE-25-1234",
            "CVE-1998-1234",
            "CVE-0000-1234",
            "CVE-2025-123",
            "cve-2025-1234",
            "CVE-2025-1234-extra",
            "CVE-2025-12A4",
        ] {
            assert!(
                !is_canonical_cve_id(value),
                "accepted non-canonical {value}"
            );
        }
    }

    #[test]
    fn notification_focus_resolves_unique_packages_across_inventory_sections() {
        let items = vec![
            cve_item("CVE-2025-1234", Some("openssl"), Some(2)),
            cve_item("CVE-2025-1234", Some("openssl"), Some(1)),
            cve_item("CVE-2025-1234", Some("glibc"), Some(0)),
            cve_item("CVE-2025-9999", Some("glibc"), Some(4)),
        ];
        assert_eq!(
            unique_retained_package_for_cve("CVE-2025-1234", &items),
            Some("openssl".to_string())
        );
        let mut scheduled_package = cve_item("CVE-2025-1234", Some("openssl"), Some(0));
        scheduled_package.scheduled_deployment_target_count = Some(1);
        let mut historical_package = cve_item("CVE-2025-1234", Some("glibc"), Some(0));
        historical_package.scheduled_deployment_target_count = Some(0);
        historical_package.historical_inventory_count = Some(1);
        let multiple = vec![scheduled_package, historical_package];
        assert_eq!(
            unique_retained_package_for_cve("CVE-2025-1234", &multiple),
            None,
            "Distinct scheduled and historical packages must remain a flat list"
        );
        let mut scheduled = cve_item("CVE-2025-1234", Some("openssl"), Some(0));
        scheduled.scheduled_deployment_target_count = Some(2);
        let mut historical = cve_item("CVE-2025-1234", Some("openssl"), Some(0));
        historical.scheduled_deployment_target_count = Some(0);
        historical.historical_inventory_count = Some(1);
        assert_eq!(
            unique_retained_package_for_cve("CVE-2025-1234", &[scheduled]),
            Some("openssl".to_string()),
            "A unique scheduled-target package must resolve without current exposure"
        );
        assert_eq!(
            unique_retained_package_for_cve("CVE-2025-1234", &[historical]),
            Some("openssl".to_string()),
            "A unique historical package must remain discoverable"
        );
        let stale = vec![cve_item("CVE-2025-1234", Some("openssl"), Some(0))];
        assert_eq!(
            unique_retained_package_for_cve("CVE-2025-1234", &stale),
            None
        );
        assert_eq!(unique_retained_package_for_cve("CVE-2025-4321", &[]), None);
    }

    #[test]
    fn notification_focus_ignores_metadata_rows_without_package_evidence() {
        let mut metadata_only = cve_item("CVE-2025-1234", None, Some(0));
        metadata_only.scheduled_deployment_target_count = Some(0);
        metadata_only.historical_inventory_count = Some(0);

        assert!(!has_retained_package_evidence(&metadata_only));
        assert_eq!(
            unique_retained_package_for_cve("CVE-2025-1234", &[metadata_only]),
            None,
            "Metadata without package-level inventory must not fabricate a drawer target"
        );
    }

    fn environment(
        id: &str,
        name: &str,
        choice: EnvironmentTriageChoice,
    ) -> EnvironmentTriageDraft {
        EnvironmentTriageDraft {
            environment_id: Uuid::parse_str(id).unwrap(),
            environment_name: name.to_string(),
            choice,
            justification: String::new(),
            review_date: String::new(),
        }
    }

    fn triage_draft() -> CveTriageDraft {
        CveTriageDraft {
            environments: vec![
                environment(
                    "00000000-0000-0000-0000-0000000000a1",
                    "Development",
                    EnvironmentTriageChoice::Accepted,
                ),
                environment(
                    "00000000-0000-0000-0000-0000000000b2",
                    "Production",
                    EnvironmentTriageChoice::Scheduled,
                ),
                environment(
                    "00000000-0000-0000-0000-0000000000c3",
                    "Lab",
                    EnvironmentTriageChoice::Open,
                ),
            ],
            title: "Patch openssl fleet".to_string(),
            default_plan: "Upgrade openssl and verify with an exact follow-up scan.".to_string(),
            target_date: "2026-10-15".to_string(),
            plan: "Promote the fixed package through environments.".to_string(),
            assignee: "group:platform-operators".to_string(),
            hydrated_assignee: None,
            risk: PoamRisk::High,
            preservation_error: None,
            default_milestones: true,
            existing_poam_reuse: false,
        }
    }

    fn scheduled_detail(
        assignee: serde_json::Value,
        include_metadata: bool,
        second_poam_id: Option<&str>,
    ) -> poam_api::FleetCveDetail {
        let poam_id = "00000000-0000-0000-0000-0000000000d1";
        let metadata = serde_json::json!({
            "id": poam_id,
            "human_id": "POAM-0042",
            "title": "Existing fleet remediation",
            "plan": "Preserve the exact remediation plan.",
            "target_date": "2026-11-20",
            "risk": "medium",
            "assignee": assignee,
        });
        let disposition = |id: &str| {
            let mut value = serde_json::json!({
                "state": "scheduled",
                "poam_id": id,
                "actor": {
                    "user_id": "00000000-0000-0000-0000-0000000000f1",
                    "display": "Operator"
                },
                "scheduled_at": "2026-09-13T12:00:00Z"
            });
            if include_metadata {
                let mut row_metadata = metadata.clone();
                row_metadata["id"] = serde_json::Value::String(id.to_string());
                value["poam"] = row_metadata;
            }
            value
        };
        let mut detail: poam_api::FleetCveDetail = serde_json::from_value(serde_json::json!({
            "cve": {
                "cve_id": "CVE-2026-44010",
                "cvss_v3_score": 9.8,
                "severity": "critical",
                "title": "Test CVE",
                "cvss_vector": null,
                "cwe_id": null,
                "published_date": null,
                "modified_date": null,
                "exploited": false,
                "package_name": "openssl",
                "installed_version": "3.0.1",
                "fixed_version": "3.0.2",
                "detection_method": "test",
                "fix_status": "fix_available"
            },
            "canonical_package_name": "openssl",
            "rollup": "scheduled",
            "affected_system_count": 2,
            "exact_affected_system_count": 2,
            "exact_mutation_target_count": 2,
            "legacy_affected_system_count": 0,
            "no_scan_system_count": 0,
            "environments": [
                {
                    "environment_id": "00000000-0000-0000-0000-0000000000e1",
                    "environment_name": "Production",
                    "affected_system_count": 1,
                    "exact_affected_system_count": 1,
                    "legacy_affected_system_count": 0,
                    "systems": [],
                    "disposition": disposition(poam_id)
                },
                {
                    "environment_id": "00000000-0000-0000-0000-0000000000e2",
                    "environment_name": "Staging",
                    "affected_system_count": 1,
                    "exact_affected_system_count": 1,
                    "legacy_affected_system_count": 0,
                    "systems": [],
                    "disposition": disposition(second_poam_id.unwrap_or(poam_id))
                }
            ]
        }))
        .unwrap();
        detail.normalize_inventory_counts();
        detail
    }

    #[test]
    fn exact_fleet_request_token_rejects_stale_and_unmounted_responses() {
        assert!(request_token_is_current(true, 2, 2));
        assert!(!request_token_is_current(true, 1, 2));
        assert!(!request_token_is_current(false, 2, 2));
    }

    #[test]
    fn inventory_sections_group_independently_from_authority() {
        let systems = [
            ("current-legacy", "current", "legacy"),
            ("scheduled-exact", "scheduled_deployment_target", "exact"),
            ("historical-exact", "historical", "exact"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (hostname, section, authority))| {
            serde_json::from_value(serde_json::json!({
                "system_id": Uuid::from_u128(index as u128 + 1),
                "hostname": hostname,
                "environment_id": Uuid::from_u128(10),
                "environment": "Production",
                "primary_ip_address": null,
                "flake_name": "platform",
                "flake_id": 1,
                "commit_hash": null,
                "deployment_policy": "manual",
                "current_package_version": "1.0",
                "inventory_authority": authority,
                "inventory_section": section
            }))
            .unwrap()
        })
        .collect::<Vec<_>>();

        let scheduled = systems_in_inventory_section(
            &systems,
            FleetCveInventorySection::ScheduledDeploymentTarget,
        );
        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0].hostname, "scheduled-exact");
        assert_eq!(
            scheduled[0].inventory_authority,
            SystemCveInventoryAuthority::Exact
        );
        assert_eq!(
            inventory_section_label(FleetCveInventorySection::Historical),
            "Historical evidence"
        );
    }

    #[test]
    fn inventory_only_status_is_read_only_not_outstanding() {
        let (label, _, title) = triage_status_presentation("inventory_only");
        assert_eq!(label, "inventory only");
        assert!(title.contains("Read-only"));
        assert_ne!(label, "outstanding");
    }

    #[test]
    fn triage_draft_includes_only_current_exact_environments() {
        let mut detail = scheduled_detail(
            serde_json::json!({
                "kind": "oidc_group",
                "group_name": "platform-operators",
                "display": "platform-operators",
                "available": true
            }),
            true,
            None,
        );
        detail.current_affected_system_count = Some(1);
        detail.scheduled_deployment_target_count = Some(1);
        detail.environments[0].current_affected_system_count = Some(1);
        detail.environments[0].scheduled_deployment_target_count = Some(0);
        detail.environments[1].current_affected_system_count = Some(0);
        detail.environments[1].scheduled_deployment_target_count = Some(1);

        assert!(environment_triage_eligible(&detail.environments[0]));
        assert!(!environment_triage_eligible(&detail.environments[1]));
        let draft = fleet_triage_draft(&detail);
        assert_eq!(draft.environments.len(), 1);
        assert_eq!(draft.environments[0].environment_name, "Production");
    }

    #[test]
    fn toast_lifecycle_fences_timers_and_manual_dismissal() {
        let mut lifecycle = ToastLifecycle::default();

        let older = lifecycle.publish(true);
        let newer = lifecycle.publish(true);
        assert!(!lifecycle.expire(older));

        lifecycle.dismiss();
        assert!(!lifecycle.expire(newer));

        let persistent_error = lifecycle.publish(false);
        assert!(!lifecycle.expire(persistent_error));

        let current_success = lifecycle.publish(true);
        assert!(lifecycle.expire(current_success));
        assert!(!lifecycle.expire(current_success));
    }

    #[test]
    fn triage_validation_requires_environment_specific_acceptance_fields() {
        let mut draft = triage_draft();
        assert_eq!(
            draft.fleet_request("openssl").unwrap_err(),
            "Enter an acceptance justification of 10 to 2000 bytes for Development."
        );

        draft.environments[0].justification = "too short".to_string();
        assert_eq!(
            draft.fleet_request("openssl").unwrap_err(),
            "Enter an acceptance justification of 10 to 2000 bytes for Development."
        );

        draft.environments[0].justification = "Internal-only service.".to_string();
        draft.environments[0].review_date = "not-a-date".to_string();
        assert_eq!(
            draft.fleet_request("openssl").unwrap_err(),
            "Enter a valid review date for Development."
        );
    }

    #[test]
    fn triage_generates_optional_plan_but_rejects_invalid_assignee_shape() {
        let mut draft = triage_draft();
        draft.environments[0].justification = "Internal-only service.".to_string();
        draft.plan.clear();
        assert_eq!(
            draft.fleet_request("openssl").unwrap().poam.unwrap().plan,
            draft.default_plan
        );

        draft.plan = "Promote and verify the fixed package.".to_string();
        draft.assignee = "platform-operators".to_string();
        assert_eq!(
            draft.fleet_request("openssl").unwrap_err(),
            "Select a valid POA&M assignee."
        );
    }

    #[test]
    fn mixed_triage_request_contains_only_environment_intentions_and_shared_poam() {
        let mut draft = triage_draft();
        draft.environments[0].justification = "Internal-only service.".to_string();
        draft.environments[0].review_date = "2026-10-01".to_string();
        let request = draft.fleet_request("openssl").unwrap();

        assert!(matches!(
            request.actions[0],
            CveEnvironmentTriageAction::AcceptRisk { .. }
        ));
        assert!(matches!(
            request.actions[1],
            CveEnvironmentTriageAction::SchedulePatch { .. }
        ));
        assert!(matches!(
            request.actions[2],
            CveEnvironmentTriageAction::LeaveOpen { .. }
        ));
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["canonical_package_name"], "openssl");
        assert_eq!(json["poam"]["assignee"]["kind"], "oidc_group");
        assert!(json.get("scope").is_none());
        let serialized = json.to_string();
        assert!(!serialized.contains("system_id"));
        assert!(!serialized.contains("hostname"));
        assert!(!serialized.contains("actor"));
        assert!(!serialized.contains("evidence"));
    }

    #[test]
    fn fleet_fix_copy_distinguishes_availability_from_an_exact_version() {
        let assignee = serde_json::json!({
            "kind": "oidc_group",
            "group_name": "platform-operators",
            "display": "platform-operators",
            "available": true
        });
        let mut detail = scheduled_detail(assignee, true, None);
        detail.cve.fixed_version = Some("  ".to_string());
        detail.cve.fix_status = "fix_available".to_string();
        assert_eq!(fleet_fix_label(&detail), "available — version pending");

        detail.cve.fix_status = "pending".to_string();
        assert_eq!(fleet_fix_label(&detail), "pending");
    }

    #[test]
    fn scheduled_draft_initializes_exact_user_and_group_metadata() {
        for (assignee, expected) in [
            (
                serde_json::json!({
                    "kind": "user",
                    "user_id": "00000000-0000-0000-0000-0000000000f2",
                    "display": "Fleet Owner",
                    "available": true
                }),
                "user:00000000-0000-0000-0000-0000000000f2",
            ),
            (
                serde_json::json!({
                    "kind": "oidc_group",
                    "group_name": "platform-operators",
                    "display": "platform-operators",
                    "available": true
                }),
                "group:platform-operators",
            ),
        ] {
            let mut draft =
                CveTriageDraft::from_fleet_detail(&scheduled_detail(assignee, true, None));
            assert_eq!(draft.title, "Existing fleet remediation");
            assert_eq!(draft.plan, "Preserve the exact remediation plan.");
            assert_eq!(draft.target_date, "2026-11-20");
            assert_eq!(draft.risk, PoamRisk::Medium);
            assert_eq!(draft.assignee, expected);
            assert!(draft.reuses_existing_poam());
            let hydrated = draft.hydrated_assignee.as_ref().unwrap();
            assert_eq!(hydrated.value, expected);
            let hydrated_label = hydrated.label.clone();
            draft.assignee = "group:replacement-owner".to_string();
            assert_eq!(draft.hydrated_assignee.as_ref().unwrap().value, expected);
            assert_eq!(
                draft.hydrated_assignee.as_ref().unwrap().label,
                hydrated_label
            );
            draft.assignee = expected.to_string();
            assert!(draft.preservation_error.is_none());
            let request = draft.fleet_request("openssl").unwrap();
            let poam = request.poam.unwrap();
            assert_eq!(poam.risk, PoamRisk::Medium);
            assert!(!poam.default_milestones);
        }
    }

    #[test]
    fn scheduled_draft_blocks_old_or_conflicting_metadata_but_allows_removal() {
        let assignee = serde_json::json!({
            "kind": "oidc_group",
            "group_name": "platform-operators",
            "display": "platform-operators",
            "available": true
        });
        let mut old_server =
            CveTriageDraft::from_fleet_detail(&scheduled_detail(assignee.clone(), false, None));
        assert!(
            old_server
                .fleet_request("openssl")
                .unwrap_err()
                .contains("server version")
        );
        for environment in &mut old_server.environments {
            environment.choice = EnvironmentTriageChoice::Open;
        }
        assert!(old_server.fleet_request("openssl").unwrap().poam.is_none());

        let conflicting = CveTriageDraft::from_fleet_detail(&scheduled_detail(
            assignee,
            true,
            Some("00000000-0000-0000-0000-0000000000d2"),
        ));
        assert!(
            conflicting
                .fleet_request("openssl")
                .unwrap_err()
                .contains("different POA&Ms")
        );

        for unavailable in [
            serde_json::json!({
                "kind": "user",
                "user_id": "00000000-0000-0000-0000-0000000000f2",
                "display": "Former owner",
                "available": false
            }),
            serde_json::json!({"kind": "legacy", "display": "Historical owner"}),
        ] {
            let draft =
                CveTriageDraft::from_fleet_detail(&scheduled_detail(unavailable, true, None));
            assert!(
                draft
                    .fleet_request("openssl")
                    .unwrap_err()
                    .contains("no longer available")
            );
        }
    }

    #[test]
    fn triage_draft_excludes_legacy_only_environments() {
        let mut detail = scheduled_detail(
            serde_json::json!({
                "kind": "oidc_group",
                "group_name": "platform-operators",
                "display": "platform-operators",
                "available": true
            }),
            true,
            None,
        );
        detail.environments[1].exact_affected_system_count = 0;
        detail.environments[1].legacy_affected_system_count = 1;
        detail.environments[1].disposition = None;

        let draft = CveTriageDraft::from_fleet_detail(&detail);

        assert_eq!(draft.environments.len(), 1);
        assert_eq!(draft.environments[0].environment_name, "Production");
    }

    #[test]
    fn fleet_detail_errors_preserve_empty_unauthorized_and_retryable_states() {
        let error = |status| {
            PoamApiError::Server(PoamServerError {
                status,
                code: "test_error".to_string(),
                message: "test failure".to_string(),
                details: None,
            })
        };

        assert_eq!(fleet_error_state(&error(404)), FleetDetailState::Empty);
        assert_eq!(
            fleet_error_state(&error(403)),
            FleetDetailState::Unauthorized
        );
        assert_eq!(
            fleet_error_state(&error(401)),
            FleetDetailState::Unauthorized
        );
        assert!(matches!(
            fleet_error_state(&error(500)),
            FleetDetailState::Error(_)
        ));
    }
}
