//! Systems list view with table/card toggle.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::{window, Node};

use crate::api::models::{CveSummary, DeploymentStatus, HealthStatus, PipelineStage, SystemSummary};
use crate::components::layout::Card;
use crate::components::system::SystemCard;
use crate::theme;

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
                                    class: "hover:bg-gray-900/60 transition",
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
    let systems = vec![
        ("atlas-01", "production", "10.42.1.11", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::BuildComplete), "24.11", "Immediate", CveSummary { critical: 0, high: 1, medium: 4, low: 12 }),
        ("atlas-02", "production", "10.42.1.12", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::ReadyForDeploy), "24.11", "Immediate", CveSummary { critical: 0, high: 2, medium: 5, low: 10 }),
        ("atlas-03", "production", "10.42.1.13", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::BuildComplete), "24.11", "Immediate", CveSummary { critical: 0, high: 1, medium: 3, low: 8 }),
        ("atlas-04", "production", "10.42.1.14", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.11", "Immediate", CveSummary { critical: 0, high: 1, medium: 2, low: 6 }),
        ("atlas-05", "production", "10.42.1.15", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::DryRun), "24.11", "Immediate", CveSummary { critical: 0, high: 0, medium: 2, low: 5 }),
        ("luna-01", "staging", "10.42.2.21", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::ReadyForDeploy), "24.05", "Immediate", CveSummary { critical: 0, high: 1, medium: 3, low: 7 }),
        ("luna-02", "staging", "10.42.2.22", HealthStatus::Warning, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.05", "Boot Only", CveSummary { critical: 1, high: 2, medium: 4, low: 8 }),
        ("orion-01", "production", "10.42.1.31", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::BuildComplete), "24.11", "Immediate", CveSummary { critical: 0, high: 1, medium: 3, low: 6 }),
        ("ws-001", "development", "10.42.3.11", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::BuildComplete), "24.05", "Immediate", CveSummary { critical: 0, high: 1, medium: 2, low: 4 }),
        ("ws-002", "development", "10.42.3.12", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.05", "Boot Only", CveSummary { critical: 0, high: 0, medium: 2, low: 3 }),
        ("ws-003", "development", "10.42.3.13", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.05", "Boot Only", CveSummary { critical: 0, high: 1, medium: 2, low: 3 }),
        ("ws-004", "development", "10.42.3.14", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::DryRun), "24.05", "Boot Only", CveSummary { critical: 0, high: 1, medium: 2, low: 3 }),
        ("ws-005", "development", "10.42.3.15", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::DryRun), "24.05", "Boot Only", CveSummary { critical: 0, high: 1, medium: 1, low: 2 }),
        ("ws-006", "development", "10.42.3.16", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.05", "Boot Only", CveSummary { critical: 0, high: 0, medium: 1, low: 2 }),
        ("ws-007", "development", "10.42.3.17", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.05", "Boot Only", CveSummary { critical: 0, high: 0, medium: 1, low: 2 }),
        ("ws-008", "development", "10.42.3.18", HealthStatus::Offline, DeploymentStatus::NeverDeployed, Some(PipelineStage::Unknown), "24.05", "Boot Only", CveSummary { critical: 0, high: 0, medium: 1, low: 1 }),
        ("edge-us-east", "production", "10.42.1.41", HealthStatus::Healthy, DeploymentStatus::UpToDate, Some(PipelineStage::ReadyForDeploy), "24.11", "Immediate", CveSummary { critical: 0, high: 1, medium: 3, low: 6 }),
        ("edge-us-west", "production", "10.42.1.42", HealthStatus::Warning, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.11", "Immediate", CveSummary { critical: 1, high: 1, medium: 3, low: 5 }),
        ("edge-eu-west", "production", "10.42.1.43", HealthStatus::Healthy, DeploymentStatus::NeverDeployed, Some(PipelineStage::ReadyForBuild), "24.11", "Immediate", CveSummary { critical: 0, high: 1, medium: 2, low: 4 }),
        ("edge-eu-central", "production", "10.42.1.44", HealthStatus::Healthy, DeploymentStatus::Unknown, Some(PipelineStage::Unknown), "24.11", "Immediate", CveSummary { critical: 0, high: 0, medium: 1, low: 2 }),
        ("edge-ap-south", "production", "10.42.1.45", HealthStatus::Offline, DeploymentStatus::Unknown, Some(PipelineStage::Unknown), "24.11", "Immediate", CveSummary { critical: 0, high: 0, medium: 1, low: 2 }),
    ];

    systems
        .into_iter()
        .map(|(hostname, environment, ip, health_status, deployment_status, pipeline_stage, nixos_version, deployment_policy, cve_counts)| SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: hostname.to_string(),
            environment: Some(environment.to_string()),
            primary_ip: Some(ip.to_string()),
            health_status,
            deployment_status,
            pipeline_stage,
            cve_counts,
            nixos_version: Some(nixos_version.to_string()),
            last_seen: None,
            deployment_policy: deployment_policy.to_string(),
        })
        .collect()
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
