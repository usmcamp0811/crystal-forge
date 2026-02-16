//! Systems list view with table/card toggle.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::{window, Node};

use crate::api::models::{
    CveSummary, DeploymentStatus, FlakeSummary, HealthStatus, PipelineStage, SystemDetail,
    SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo, SystemSummary,
};
use crate::components::layout::Card;
use crate::components::system::SystemCard;
use crate::routes::Route;
use crate::theme;
use chrono::{Duration, TimeZone, Utc};
use uuid::Uuid;

const VIEW_PREF_KEY: &str = "crystal_forge.systems.view";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SystemsViewMode {
    Table,
    Cards,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterDropdown {
    Environment,
    Health,
    Deployment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortColumn {
    Hostname,
    Ip,
    Environment,
    Health,
    Deployment,
    Cves,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    fn toggle(self) -> Self {
        match self {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        }
    }
}

impl SystemsViewMode {
    fn from_storage(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("cards") => Self::Cards,
            _ => Self::Table,
        }
    }

    fn as_storage(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::Cards => "cards",
        }
    }
}


/// Systems list with toggles and filters.
#[component]
pub fn SystemsListView() -> Element {
    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| SystemsViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();
    let mut open_dropdown = use_signal(|| None::<FilterDropdown>);
    let container_id = use_memo(|| format!("systems-filters-{}", uuid::Uuid::new_v4()));

    use_effect(move || {
        if let Some(mode) = query_view {
            view_mode.set(mode);
            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
        }
    });

    {
        let mut open_dropdown = open_dropdown.clone();
        let container_id = container_id.clone();
        use_effect(move || {
            let Some(window) = window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let document_for_listener = document.clone();
            let handler = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
                if open_dropdown.read().is_none() {
                    return;
                }
                let target = match event.target() {
                    Some(target) => target,
                    None => return,
                };
                let node: Node = match target.dyn_into() {
                    Ok(node) => node,
                    Err(_) => return,
                };
                let container_id = container_id.read();
                if let Some(container) = document_for_listener.get_element_by_id(container_id.as_str()) {
                    if !container.contains(Some(&node)) {
                        open_dropdown.set(None);
                    }
                }
            });
            let _ = document.add_event_listener_with_callback(
                "mousedown",
                handler.as_ref().unchecked_ref(),
            );
            handler.forget();
        });
    }

    let search = use_signal(String::new);
    let environment_filter = use_signal(Vec::<String>::new);
    let health_filter = use_signal(Vec::<HealthStatus>::new);
    let deployment_filter = use_signal(Vec::<DeploymentStatus>::new);

    let mock_systems = mock_systems();

    let environments = unique_environments(&mock_systems);

    let filtered_systems: Vec<SystemSummary> = mock_systems
        .into_iter()
        .filter(|system| matches_environment(system, &environment_filter.read()))
        .filter(|system| matches_health(system, &health_filter.read()))
        .filter(|system| matches_deployment(system, &deployment_filter.read()))
        .filter(|system| matches_search(system, &search.read()))
        .collect();

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Systems" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Manage fleet systems and deployment status." }
                }
                ViewToggle {
                    view_mode: *view_mode.read(),
                    on_change: move |mode| {
                        view_mode.set(mode);
                        let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
                    }
                }
            }

            FiltersBar {
                environments: environments.clone(),
                search: search,
                environment_filter: environment_filter,
                health_filter: health_filter,
                deployment_filter: deployment_filter,
                open_dropdown: open_dropdown,
            }

            if filtered_systems.is_empty() {
                Card {
                    title: Some("No systems".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "No systems matched your filters." }
                    }
                }
            } else if *view_mode.read() == SystemsViewMode::Cards {
                div {
                    class: "grid grid-cols-1 xl:grid-cols-2 gap-6",
                    "data-testid": "systems-cards",
                    for system in filtered_systems.clone() {
                        SystemCard { system }
                    }
                }
            } else {
                SystemsTable { systems: filtered_systems.clone() }
            }
        }
    }
}

#[component]
fn ViewToggle(view_mode: SystemsViewMode, on_change: EventHandler<SystemsViewMode>) -> Element {
    let table_active = view_mode == SystemsViewMode::Table;
    let cards_active = view_mode == SystemsViewMode::Cards;

    rsx! {
        div {
            class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG}",
            button {
                class: "px-3 py-2 text-sm font-medium rounded-l-lg transition {theme::interactive::FOCUS_RING} {theme::text::SECONDARY} {table_class(table_active)}",
                onclick: move |_| on_change.call(SystemsViewMode::Table),
                "Table"
            }
            button {
                class: "px-3 py-2 text-sm font-medium rounded-r-lg transition {theme::interactive::FOCUS_RING} {theme::text::SECONDARY} {table_class(cards_active)}",
                onclick: move |_| on_change.call(SystemsViewMode::Cards),
                "Cards"
            }
        }
    }
}

#[component]
fn FiltersBar(
    environments: Vec<String>,
    search: Signal<String>,
    environment_filter: Signal<Vec<String>>,
    health_filter: Signal<Vec<HealthStatus>>,
    deployment_filter: Signal<Vec<DeploymentStatus>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    rsx! {
        div {
            class: "relative z-[2000] grid grid-cols-1 lg:grid-cols-4 gap-4",
            style: "position: sticky; top: 0;",
            input {
                class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                r#type: "search",
                placeholder: "Search hostname...",
                value: "{search.read()}",
                oninput: move |evt| search.set(evt.value()),
            }
            EnvironmentFilterDropdown {
                environments,
                selected: environment_filter,
                open_dropdown: open_dropdown,
            }
            HealthFilterDropdown {
                selected: health_filter,
                open_dropdown: open_dropdown,
            }
            DeploymentFilterDropdown {
                selected: deployment_filter,
                open_dropdown: open_dropdown,
            }
        }
    }
}

#[component]
fn SystemsTable(systems: Vec<SystemSummary>) -> Element {
    let navigator = use_navigator();
    let mut sort_column = use_signal(|| None::<SortColumn>);
    let mut sort_direction = use_signal(|| SortDirection::Asc);

    let sorted_systems = {
        let mut sorted = systems.clone();
        if let Some(column) = *sort_column.read() {
            let dir = *sort_direction.read();
            sorted.sort_by(|a, b| {
                let cmp = match column {
                    SortColumn::Hostname => a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()),
                    SortColumn::Ip => {
                        let a_ip = a.primary_ip.as_deref().unwrap_or("");
                        let b_ip = b.primary_ip.as_deref().unwrap_or("");
                        a_ip.cmp(b_ip)
                    }
                    SortColumn::Environment => {
                        let a_env = a.environment.as_deref().unwrap_or("");
                        let b_env = b.environment.as_deref().unwrap_or("");
                        a_env.to_lowercase().cmp(&b_env.to_lowercase())
                    }
                    SortColumn::Health => a.health_status.label().cmp(b.health_status.label()),
                    SortColumn::Deployment => a.deployment_status.label().cmp(b.deployment_status.label()),
                    SortColumn::Cves => {
                        let a_total = a.cve_counts.critical + a.cve_counts.high + a.cve_counts.medium + a.cve_counts.low;
                        let b_total = b.cve_counts.critical + b.cve_counts.high + b.cve_counts.medium + b.cve_counts.low;
                        a_total.cmp(&b_total)
                    }
                };
                match dir {
                    SortDirection::Asc => cmp,
                    SortDirection::Desc => cmp.reverse(),
                }
            });
        }
        sorted
    };

    let current_col = *sort_column.read();
    let current_dir = *sort_direction.read();

    rsx! {
        Card {
            title: Some("Fleet Systems".to_string()),
            children: rsx! {
                div {
                    class: "overflow-x-auto",
                    "data-testid": "systems-table",
                    table {
                        class: "w-full",
                        thead {
                            class: "{theme::surface::SUBTLE_BG}",
                            tr {
                                SortableHeader {
                                    label: "Hostname",
                                    column: SortColumn::Hostname,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "IP",
                                    column: SortColumn::Ip,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Environment",
                                    column: SortColumn::Environment,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Health",
                                    column: SortColumn::Health,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "Deployment",
                                    column: SortColumn::Deployment,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                                SortableHeader {
                                    label: "CVEs",
                                    column: SortColumn::Cves,
                                    current_col: current_col,
                                    current_dir: current_dir,
                                    sort_column: sort_column,
                                    sort_direction: sort_direction,
                                }
                            }
                        }
                        tbody {
                            class: "divide-y {theme::surface::DIVIDER}",
                            for system in sorted_systems {
                                tr {
                                    class: "hover:bg-gray-900/60 transition cursor-pointer",
                                    onclick: move |_| {
                                        navigator.push(Route::SystemDetailView { id: system.id.to_string() });
                                    },
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm text-white", "{system.hostname}" }
                                    td {
                                        class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono",
                                        "{ip_label(&system)}"
                                    }
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{environment_label(&system)}" }
                                    td { class: "{theme::spacing::TABLE_CELL}",
                                        span { class: "text-xs {system.health_status.color_class()}", "{system.health_status.label()}" }
                                    }
                                    td { class: "{theme::spacing::TABLE_CELL}",
                                        span { class: "text-xs {system.deployment_status.color_class()}", "{system.deployment_status.label()}" }
                                    }
                                    td { class: "{theme::spacing::TABLE_CELL} text-xs",
                                        span { class: "{theme::cve::CRITICAL_TEXT} font-semibold", "{system.cve_counts.critical}" }
                                        span { class: "text-gray-500", " C  " }
                                        span { class: "{theme::cve::HIGH_TEXT} font-semibold", "{system.cve_counts.high}" }
                                        span { class: "text-gray-500", " H  " }
                                        span { class: "{theme::cve::MEDIUM_TEXT} font-semibold", "{system.cve_counts.medium}" }
                                        span { class: "text-gray-500", " M  " }
                                        span { class: "{theme::cve::LOW_TEXT} font-semibold", "{system.cve_counts.low}" }
                                        span { class: "text-gray-500", " L" }
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

fn environment_label(system: &SystemSummary) -> String {
    system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string())
}

fn ip_label(system: &SystemSummary) -> String {
    system
        .primary_ip
        .clone()
        .unwrap_or_else(|| "-".to_string())
}

fn format_multi_label(values: &[String], placeholder: &str) -> String {
    if values.is_empty() {
        placeholder.to_string()
    } else if values.len() == 1 {
        values[0].clone()
    } else {
        format!("{} selected", values.len())
    }
}

fn format_status_label(values: &[HealthStatus]) -> String {
    if values.is_empty() {
        "All health".to_string()
    } else if values.len() == 1 {
        values[0].label().to_string()
    } else {
        format!("{} selected", values.len())
    }
}

fn format_deployment_label(values: &[DeploymentStatus]) -> String {
    if values.is_empty() {
        "All deployment".to_string()
    } else if values.len() == 1 {
        values[0].label().to_string()
    } else {
        format!("{} selected", values.len())
    }
}

fn matches_environment(system: &SystemSummary, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }

    system
        .environment
        .as_deref()
        .is_some_and(|env| filters.iter().any(|filter| env.eq_ignore_ascii_case(filter)))
}

fn matches_health(system: &SystemSummary, filters: &[HealthStatus]) -> bool {
    if filters.is_empty() {
        return true;
    }

    filters.contains(&system.health_status)
}

fn matches_deployment(system: &SystemSummary, filters: &[DeploymentStatus]) -> bool {
    if filters.is_empty() {
        return true;
    }

    filters.contains(&system.deployment_status)
}

#[component]
fn EnvironmentFilterDropdown(
    environments: Vec<String>,
    selected: Signal<Vec<String>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    let label = format_multi_label(&selected.read(), "All environments");
    let is_open = *open_dropdown.read() == Some(FilterDropdown::Environment);

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(FilterDropdown::Environment));
                    }
                },
                span { "{label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }

            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "All environments"
                    }
                    for env in environments {
                        {
                            let is_selected = selected.read().contains(&env);
                            let env_clone = env.clone();
                            rsx! {
                                button {
                                    key: "{env_clone}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&env_clone) {
                                            next.retain(|value| value != &env_clone);
                                        } else {
                                            next.push(env_clone.clone());
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{env_clone}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HealthFilterDropdown(
    selected: Signal<Vec<HealthStatus>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    let label = format_status_label(&selected.read());
    let options = vec![HealthStatus::Healthy, HealthStatus::Warning, HealthStatus::Critical, HealthStatus::Offline];
    let is_open = *open_dropdown.read() == Some(FilterDropdown::Health);

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(FilterDropdown::Health));
                    }
                },
                span { "{label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }
            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "All health"
                    }
                    for status in options {
                        {
                            let is_selected = selected.read().contains(&status);
                            let label = status.label();
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&status) {
                                            next.retain(|value| value != &status);
                                        } else {
                                            next.push(status);
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DeploymentFilterDropdown(
    selected: Signal<Vec<DeploymentStatus>>,
    open_dropdown: Signal<Option<FilterDropdown>>,
) -> Element {
    let label = format_deployment_label(&selected.read());
    let options = vec![
        DeploymentStatus::UpToDate,
        DeploymentStatus::Behind,
        DeploymentStatus::Ahead,
        DeploymentStatus::NeverDeployed,
        DeploymentStatus::Unknown,
    ];
    let is_open = *open_dropdown.read() == Some(FilterDropdown::Deployment);

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(FilterDropdown::Deployment));
                    }
                },
                span { "{label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }
            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "All deployment"
                    }
                    for status in options {
                        {
                            let is_selected = selected.read().contains(&status);
                            let label = status.label();
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&status) {
                                            next.retain(|value| value != &status);
                                        } else {
                                            next.push(status);
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}


fn matches_search(system: &SystemSummary, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }

    system
        .hostname
        .to_lowercase()
        .contains(&query.to_lowercase())
}

fn unique_environments(systems: &[SystemSummary]) -> Vec<String> {
    let mut values: Vec<String> = systems
        .iter()
        .filter_map(|system| system.environment.clone())
        .collect();

    values.sort();
    values.dedup();
    values
}

fn mock_systems() -> Vec<SystemSummary> {
    mock_system_details()
        .into_iter()
        .map(|system| SystemSummary {
            id: system.id,
            hostname: system.hostname,
            environment: system.environment,
            primary_ip: system.network.primary_ip,
            health_status: system.health_status,
            deployment_status: system.deployment_status,
            pipeline_stage: system.pipeline_stage,
            cve_counts: system.cve_counts,
            nixos_version: system.nixos_version,
            last_seen: system.last_seen,
            deployment_policy: system.deployment_policy,
        })
        .collect()
}

pub fn mock_system_detail_by_id(id: &str) -> Option<SystemDetail> {
    let parsed = Uuid::parse_str(id).ok()?;
    mock_system_details()
        .into_iter()
        .find(|system| system.id == parsed)
}

pub fn mock_system_details() -> Vec<SystemDetail> {
    let base_time = Utc.with_ymd_and_hms(2026, 2, 8, 15, 19, 29).unwrap();
    let flake_core = FlakeSummary {
        id: 1,
        name: "nixos-configs".to_string(),
        repo_url: "https://github.com/example/nixos-configs".to_string(),
        latest_commit: Some("04af6a5".to_string()),
    };
    let flake_staging = FlakeSummary {
        id: 2,
        name: "nixos-staging".to_string(),
        repo_url: "https://github.com/example/nixos-staging".to_string(),
        latest_commit: Some("a12ce19".to_string()),
    };
    let flake_dev = FlakeSummary {
        id: 3,
        name: "workstations".to_string(),
        repo_url: "https://github.com/example/workstations".to_string(),
        latest_commit: Some("9bd421f".to_string()),
    };

    vec![
        build_system_detail(
            base_time,
            1,
            "atlas-01",
            "production",
            "10.42.1.11",
            "c8:7f:54:5d:ae:11",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::BuildComplete),
            "24.11",
            "6.12.66",
            "Immediate",
            "0.2.1",
            "m6wc1njr09pbpv49q60c1",
            "AMD EPYC 7543P 32-Core Processor",
            32,
            128.0,
            177_536,
            "ATLAS-01-BRD",
            "2.5.4",
            true,
            true,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 1, medium: 4, low: 12 },
            Some(flake_core.clone()),
            2,
        ),
        build_system_detail(
            base_time,
            2,
            "atlas-02",
            "production",
            "10.42.1.12",
            "c8:7f:54:5d:ae:12",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::ReadyForDeploy),
            "24.11",
            "6.12.66",
            "Immediate",
            "0.2.1",
            "b51f0bb7e6b58b4b9c84",
            "AMD EPYC 7713 64-Core Processor",
            64,
            256.0,
            202_415,
            "ATLAS-02-BRD",
            "2.6.1",
            true,
            true,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 2, medium: 5, low: 10 },
            Some(flake_core.clone()),
            1,
        ),
        build_system_detail(
            base_time,
            3,
            "atlas-03",
            "production",
            "10.42.1.13",
            "c8:7f:54:5d:ae:13",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::BuildComplete),
            "24.11",
            "6.12.66",
            "Immediate",
            "0.2.0",
            "d9029ab7c4ff9811b0ff",
            "Intel Xeon Gold 6338",
            32,
            192.0,
            154_022,
            "ATLAS-03-BRD",
            "2.4.9",
            true,
            true,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 1, medium: 3, low: 8 },
            Some(flake_core.clone()),
            4,
        ),
        build_system_detail(
            base_time,
            4,
            "atlas-04",
            "production",
            "10.42.1.14",
            "c8:7f:54:5d:ae:14",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.11",
            "6.12.60",
            "Immediate",
            "0.2.1",
            "c7ce0b0b05a99e6d7d4e",
            "Intel Xeon Silver 4310",
            24,
            96.0,
            88_450,
            "ATLAS-04-BRD",
            "2.4.7",
            true,
            false,
            false,
            Some("permissive"),
            CveSummary { critical: 0, high: 1, medium: 2, low: 6 },
            Some(flake_core.clone()),
            6,
        ),
        build_system_detail(
            base_time,
            5,
            "atlas-05",
            "production",
            "10.42.1.15",
            "c8:7f:54:5d:ae:15",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::DryRun),
            "24.11",
            "6.12.60",
            "Immediate",
            "0.2.0",
            "03d23b17c1d80c7e3f62",
            "AMD EPYC 7302P 16-Core Processor",
            16,
            64.0,
            91_007,
            "ATLAS-05-BRD",
            "2.4.7",
            false,
            false,
            false,
            Some("disabled"),
            CveSummary { critical: 0, high: 0, medium: 2, low: 5 },
            Some(flake_core.clone()),
            8,
        ),
        build_system_detail(
            base_time,
            6,
            "luna-01",
            "staging",
            "10.42.2.21",
            "b8:5f:f7:1a:22:21",
            "10.42.2.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::ReadyForDeploy),
            "24.05",
            "6.9.12",
            "Immediate",
            "0.2.1",
            "a11c209f7b0b1a9c437f",
            "AMD Ryzen 9 5950X 16-Core Processor",
            16,
            64.0,
            72_340,
            "LUNA-01-BRD",
            "1.9.2",
            true,
            false,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 1, medium: 3, low: 7 },
            Some(flake_staging.clone()),
            3,
        ),
        build_system_detail(
            base_time,
            7,
            "luna-02",
            "staging",
            "10.42.2.22",
            "b8:5f:f7:1a:22:22",
            "10.42.2.1",
            HealthStatus::Warning,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.05",
            "6.9.12",
            "Boot Only",
            "0.2.0",
            "7a2f0b12e117a3db3f2a",
            "AMD Ryzen 9 5900X 12-Core Processor",
            12,
            48.0,
            39_210,
            "LUNA-02-BRD",
            "1.8.8",
            true,
            false,
            false,
            Some("enforcing"),
            CveSummary { critical: 1, high: 2, medium: 4, low: 8 },
            Some(flake_staging.clone()),
            12,
        ),
        build_system_detail(
            base_time,
            8,
            "orion-01",
            "production",
            "10.42.1.31",
            "c8:7f:54:5d:ae:31",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::BuildComplete),
            "24.11",
            "6.12.66",
            "Immediate",
            "0.2.1",
            "24ed117db226b5f87520",
            "AMD EPYC 75F3 32-Core Processor",
            32,
            128.0,
            132_996,
            "ORION-01-BRD",
            "2.6.0",
            true,
            true,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 1, medium: 3, low: 6 },
            Some(flake_core.clone()),
            1,
        ),
        build_system_detail(
            base_time,
            9,
            "ws-001",
            "development",
            "10.42.3.11",
            "e8:2f:2a:44:11:01",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::BuildComplete),
            "24.05",
            "6.9.12",
            "Immediate",
            "0.2.1",
            "d10c01f3fda6b36b845e",
            "AMD Ryzen 7 5800X 8-Core Processor",
            8,
            32.0,
            54_210,
            "WS-001-BRD",
            "1.3.1",
            false,
            false,
            false,
            Some("disabled"),
            CveSummary { critical: 0, high: 1, medium: 2, low: 4 },
            Some(flake_dev.clone()),
            2,
        ),
        build_system_detail(
            base_time,
            10,
            "ws-002",
            "development",
            "10.42.3.12",
            "e8:2f:2a:44:11:02",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.05",
            "6.9.12",
            "Boot Only",
            "0.2.0",
            "f21b209f2a44d19abce9",
            "Intel Core i7-12700 12-Core Processor",
            12,
            32.0,
            29_300,
            "WS-002-BRD",
            "1.2.8",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 0, medium: 2, low: 3 },
            Some(flake_dev.clone()),
            5,
        ),
        build_system_detail(
            base_time,
            11,
            "ws-003",
            "development",
            "10.42.3.13",
            "e8:2f:2a:44:11:03",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.05",
            "6.9.12",
            "Boot Only",
            "0.2.0",
            "1a82c0b1b6c0aaf4d3c3",
            "Intel Core i5-13600K 14-Core Processor",
            14,
            24.0,
            21_950,
            "WS-003-BRD",
            "1.2.8",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 1, medium: 2, low: 3 },
            Some(flake_dev.clone()),
            10,
        ),
        build_system_detail(
            base_time,
            12,
            "ws-004",
            "development",
            "10.42.3.14",
            "e8:2f:2a:44:11:04",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::DryRun),
            "24.05",
            "6.9.12",
            "Boot Only",
            "0.2.0",
            "4a80ab7c9f40d2149a12",
            "AMD Ryzen 5 5600X 6-Core Processor",
            6,
            16.0,
            18_450,
            "WS-004-BRD",
            "1.1.4",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 1, medium: 2, low: 3 },
            Some(flake_dev.clone()),
            14,
        ),
        build_system_detail(
            base_time,
            13,
            "ws-005",
            "development",
            "10.42.3.15",
            "e8:2f:2a:44:11:05",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::DryRun),
            "24.05",
            "6.9.10",
            "Boot Only",
            "0.2.0",
            "2cb1209f1d08cfed51a1",
            "Intel Core i5-12400 6-Core Processor",
            6,
            16.0,
            15_002,
            "WS-005-BRD",
            "1.1.3",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 1, medium: 1, low: 2 },
            Some(flake_dev.clone()),
            20,
        ),
        build_system_detail(
            base_time,
            14,
            "ws-006",
            "development",
            "10.42.3.16",
            "e8:2f:2a:44:11:06",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.05",
            "6.9.10",
            "Boot Only",
            "0.2.0",
            "5c92110f09985e4c6bff",
            "Intel Core i3-12100 4-Core Processor",
            4,
            8.0,
            9_120,
            "WS-006-BRD",
            "1.0.9",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 0, medium: 1, low: 2 },
            Some(flake_dev.clone()),
            28,
        ),
        build_system_detail(
            base_time,
            15,
            "ws-007",
            "development",
            "10.42.3.17",
            "e8:2f:2a:44:11:07",
            "10.42.3.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.05",
            "6.9.10",
            "Boot Only",
            "0.2.0",
            "7f0110b5d7a7dc5e3d6d",
            "Intel Core i5-11600 6-Core Processor",
            6,
            16.0,
            8_540,
            "WS-007-BRD",
            "1.0.7",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 0, medium: 1, low: 2 },
            Some(flake_dev.clone()),
            30,
        ),
        build_system_detail(
            base_time,
            16,
            "ws-008",
            "development",
            "10.42.3.18",
            "e8:2f:2a:44:11:08",
            "10.42.3.1",
            HealthStatus::Offline,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::Unknown),
            "24.05",
            "6.9.8",
            "Boot Only",
            "0.1.9",
            "a20110ef11a5b0cc65f3",
            "Intel Core i5-10400 6-Core Processor",
            6,
            16.0,
            2_340,
            "WS-008-BRD",
            "0.9.1",
            false,
            false,
            false,
            None,
            CveSummary { critical: 0, high: 0, medium: 1, low: 1 },
            Some(flake_dev.clone()),
            240,
        ),
        build_system_detail(
            base_time,
            17,
            "edge-us-east",
            "production",
            "10.42.1.41",
            "d0:3e:7a:22:11:41",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::UpToDate,
            Some(PipelineStage::ReadyForDeploy),
            "24.11",
            "6.12.66",
            "Immediate",
            "0.2.1",
            "cc0ef72b3a2bd04f1e77",
            "Intel Xeon D-2146NT 8-Core Processor",
            8,
            32.0,
            61_220,
            "EDGE-USEAST",
            "1.6.0",
            true,
            false,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 1, medium: 3, low: 6 },
            Some(flake_core.clone()),
            4,
        ),
        build_system_detail(
            base_time,
            18,
            "edge-us-west",
            "production",
            "10.42.1.42",
            "d0:3e:7a:22:11:42",
            "10.42.1.1",
            HealthStatus::Warning,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.11",
            "6.12.60",
            "Immediate",
            "0.2.0",
            "cb4f21c2e80d9c9a7a5a",
            "Intel Xeon D-2123IT 4-Core Processor",
            4,
            24.0,
            33_018,
            "EDGE-USWEST",
            "1.5.4",
            true,
            false,
            false,
            Some("enforcing"),
            CveSummary { critical: 1, high: 1, medium: 3, low: 5 },
            Some(flake_core.clone()),
            18,
        ),
        build_system_detail(
            base_time,
            19,
            "edge-eu-west",
            "production",
            "10.42.1.43",
            "d0:3e:7a:22:11:43",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::NeverDeployed,
            Some(PipelineStage::ReadyForBuild),
            "24.11",
            "6.12.60",
            "Immediate",
            "0.2.0",
            "a3bd1e33f2b44d8f7610",
            "Intel Xeon D-2145NT 8-Core Processor",
            8,
            32.0,
            27_880,
            "EDGE-EUWEST",
            "1.5.2",
            true,
            false,
            false,
            Some("enforcing"),
            CveSummary { critical: 0, high: 1, medium: 2, low: 4 },
            Some(flake_core.clone()),
            20,
        ),
        build_system_detail(
            base_time,
            20,
            "edge-eu-central",
            "production",
            "10.42.1.44",
            "d0:3e:7a:22:11:44",
            "10.42.1.1",
            HealthStatus::Healthy,
            DeploymentStatus::Unknown,
            Some(PipelineStage::Unknown),
            "24.11",
            "6.12.60",
            "Immediate",
            "0.2.0",
            "dde12cf299e0bf10a1d2",
            "Intel Xeon D-1718DML 8-Core Processor",
            8,
            16.0,
            25_120,
            "EDGE-EUCENTRAL",
            "1.4.8",
            false,
            false,
            false,
            Some("disabled"),
            CveSummary { critical: 0, high: 0, medium: 1, low: 2 },
            Some(flake_core.clone()),
            26,
        ),
        build_system_detail(
            base_time,
            21,
            "edge-ap-south",
            "production",
            "10.42.1.45",
            "d0:3e:7a:22:11:45",
            "10.42.1.1",
            HealthStatus::Offline,
            DeploymentStatus::Unknown,
            Some(PipelineStage::Unknown),
            "24.11",
            "6.12.58",
            "Immediate",
            "0.1.8",
            "ffe102c449b11c28c3cd",
            "Intel Atom C3758 8-Core Processor",
            8,
            16.0,
            4_350,
            "EDGE-APSOUTH",
            "1.4.2",
            false,
            false,
            false,
            Some("disabled"),
            CveSummary { critical: 0, high: 0, medium: 1, low: 2 },
            Some(flake_core.clone()),
            360,
        ),
    ]
}

fn build_system_detail(
    base_time: chrono::DateTime<Utc>,
    id: u128,
    hostname: &str,
    environment: &str,
    primary_ip: &str,
    primary_mac: &str,
    gateway_ip: &str,
    health_status: HealthStatus,
    deployment_status: DeploymentStatus,
    pipeline_stage: Option<PipelineStage>,
    nixos_version: &str,
    kernel: &str,
    deployment_policy: &str,
    agent_version: &str,
    store_hash: &str,
    cpu_brand: &str,
    cpu_cores: i32,
    memory_gb: f64,
    uptime_secs: i64,
    board_serial: &str,
    bios_version: &str,
    tpm_present: bool,
    secure_boot_enabled: bool,
    fips_mode: bool,
    selinux_status: Option<&str>,
    cve_counts: CveSummary,
    flake: Option<FlakeSummary>,
    last_seen_offset_hours: i64,
) -> SystemDetail {
    let last_seen = base_time - Duration::hours(last_seen_offset_hours);
    let is_active = !matches!(health_status, HealthStatus::Offline);
    let store_path = if matches!(deployment_status, DeploymentStatus::Unknown) {
        None
    } else {
        Some(format!(
            "/nix/store/{store_hash}-nixos-system-{hostname}-{nixos_version}"
        ))
    };

    SystemDetail {
        id: Uuid::from_u128(id),
        hostname: hostname.to_string(),
        environment: Some(environment.to_string()),
        is_active,
        deployment_policy: deployment_policy.to_string(),
        health_status,
        deployment_status,
        pipeline_stage,
        nixos_version: Some(nixos_version.to_string()),
        kernel: Some(kernel.to_string()),
        agent_version: Some(agent_version.to_string()),
        current_store_path: store_path,
        hardware: SystemHardwareInfo {
            cpu_brand: Some(cpu_brand.to_string()),
            cpu_cores: Some(cpu_cores),
            memory_gb: Some(memory_gb),
            uptime_secs: Some(uptime_secs),
            board_serial: Some(board_serial.to_string()),
            bios_version: Some(bios_version.to_string()),
        },
        network: SystemNetworkInfo {
            primary_ip: Some(primary_ip.to_string()),
            primary_mac: Some(primary_mac.to_string()),
            gateway_ip: Some(gateway_ip.to_string()),
        },
        security: SystemSecurityInfo {
            tpm_present: Some(tpm_present),
            secure_boot_enabled: Some(secure_boot_enabled),
            fips_mode: Some(fips_mode),
            selinux_status: selinux_status.map(|value| value.to_string()),
        },
        cve_counts,
        flake,
        last_seen: Some(last_seen),
        created_at: base_time - Duration::days(420),
        updated_at: base_time - Duration::hours(1),
    }
}

fn table_class(is_active: bool) -> &'static str {
    if is_active {
        "bg-gray-800 text-white"
    } else {
        ""
    }
}

#[component]
fn SortableHeader(
    label: &'static str,
    column: SortColumn,
    current_col: Option<SortColumn>,
    current_dir: SortDirection,
    mut sort_column: Signal<Option<SortColumn>>,
    mut sort_direction: Signal<SortDirection>,
) -> Element {
    let is_active = current_col == Some(column);
    let icon = if is_active {
        match current_dir {
            SortDirection::Asc => "↑",
            SortDirection::Desc => "↓",
        }
    } else {
        "↕"
    };

    rsx! {
        th {
            class: "px-4 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider cursor-pointer hover:text-white transition select-none",
            onclick: move |_| {
                if current_col == Some(column) {
                    // Toggle direction - use the passed-in value, not a fresh read
                    let new_dir = current_dir.toggle();
                    sort_direction.set(new_dir);
                } else {
                    sort_column.set(Some(column));
                    sort_direction.set(SortDirection::Asc);
                }
            },
            span { class: "inline-flex items-center gap-1",
                "{label}"
                span { class: if is_active { "text-blue-400" } else { "text-gray-600" }, "{icon}" }
            }
        }
    }
}

fn prefers_view_from_query() -> Option<SystemsViewMode> {
    let location = window()?.location();
    let search = location.search().ok().unwrap_or_default();
    let hash = location.hash().ok().unwrap_or_default();
    let combined = format!("{search}{hash}");

    if combined.contains("view=cards") {
        return Some(SystemsViewMode::Cards);
    }
    if combined.contains("view=table") {
        return Some(SystemsViewMode::Table);
    }
    None
}
