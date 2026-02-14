//! Systems list view with table/card toggle.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use web_sys::window;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusFilter {
    All,
    Healthy,
    Warning,
    Critical,
    Offline,
}

impl StatusFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Healthy => "Healthy",
            Self::Warning => "Warning",
            Self::Critical => "Critical",
            Self::Offline => "Offline",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeploymentFilter {
    All,
    UpToDate,
    Behind,
    Ahead,
    NeverDeployed,
    Unknown,
}

impl DeploymentFilter {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::UpToDate => "Up to Date",
            Self::Behind => "Behind",
            Self::Ahead => "Ahead",
            Self::NeverDeployed => "Never Deployed",
            Self::Unknown => "Unknown",
        }
    }
}

/// Systems list with toggles and filters.
#[component]
pub fn SystemsListView() -> Element {
    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| SystemsViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();

    use_effect(move || {
        if let Some(mode) = query_view {
            view_mode.set(mode);
            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
        }
    });
    let search = use_signal(String::new);
    let environment_filter = use_signal(String::new);
    let health_filter = use_signal(|| StatusFilter::All);
    let deployment_filter = use_signal(|| DeploymentFilter::All);

    let mock_systems = mock_systems();

    let filtered_systems: Vec<SystemSummary> = mock_systems
        .into_iter()
        .filter(|system| matches_environment(system, &environment_filter.read()))
        .filter(|system| matches_health(system, *health_filter.read()))
        .filter(|system| matches_deployment(system, *deployment_filter.read()))
        .filter(|system| matches_search(system, &search.read()))
        .collect();

    let environments = unique_environments(&filtered_systems);

    rsx! {
        div {
            class: "space-y-6",
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
                    for system in filtered_systems.clone() {
                        SystemCard { system }
                    }
                }
            } else {
                SystemsTable { systems: filtered_systems }
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
    environment_filter: Signal<String>,
    health_filter: Signal<StatusFilter>,
    deployment_filter: Signal<DeploymentFilter>,
) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-1 lg:grid-cols-4 gap-4",
            input {
                class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                r#type: "search",
                placeholder: "Search hostname...",
                value: "{search.read()}",
                oninput: move |evt| search.set(evt.value()),
            }
            select {
                class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                value: "{environment_filter}",
                onchange: move |evt| environment_filter.set(evt.value()),
                option { value: "", "All environments" }
                for environment in environments {
                    option { value: "{environment}", "{environment}" }
                }
            }
            select {
                class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                value: "{health_filter.read().label()}",
                onchange: move |evt| health_filter.set(parse_health_filter(&evt.value())),
                option { value: "All", "All health" }
                option { value: "Healthy", "Healthy" }
                option { value: "Warning", "Warning" }
                option { value: "Critical", "Critical" }
                option { value: "Offline", "Offline" }
            }
            select {
                class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                value: "{deployment_filter.read().label()}",
                onchange: move |evt| deployment_filter.set(parse_deployment_filter(&evt.value())),
                option { value: "All", "All deployment" }
                option { value: "Up to Date", "Up to Date" }
                option { value: "Behind", "Behind" }
                option { value: "Ahead", "Ahead" }
                option { value: "Never Deployed", "Never Deployed" }
                option { value: "Unknown", "Unknown" }
            }
        }
    }
}

#[component]
fn SystemsTable(systems: Vec<SystemSummary>) -> Element {
    rsx! {
        Card {
            title: Some("Fleet Systems".to_string()),
            children: rsx! {
                div {
                    class: "overflow-x-auto",
                    table {
                        class: "w-full",
                        thead {
                            class: "{theme::surface::SUBTLE_BG}",
                            tr {
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Hostname" }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Environment" }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Health" }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Deployment" }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "CVEs" }
                            }
                        }
                        tbody {
                            class: "divide-y {theme::surface::DIVIDER}",
                            for system in systems {
                                tr {
                                    class: "hover:bg-gray-900/60 transition",
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm text-white", "{system.hostname}" }
                                    td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{environment_label(&system)}" }
                                    td { class: "{theme::spacing::TABLE_CELL}",
                                        span { class: "text-xs {system.health_status.color_class()}", "{system.health_status.label()}" }
                                    }
                                    td { class: "{theme::spacing::TABLE_CELL}",
                                        span { class: "text-xs {system.deployment_status.color_class()}", "{system.deployment_status.label()}" }
                                    }
                                    td { class: "{theme::spacing::TABLE_CELL} text-xs {theme::text::SECONDARY}",
                                        "C {system.cve_counts.critical} · H {system.cve_counts.high} · M {system.cve_counts.medium} · L {system.cve_counts.low}"
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

fn parse_health_filter(value: &str) -> StatusFilter {
    match value {
        "Healthy" => StatusFilter::Healthy,
        "Warning" => StatusFilter::Warning,
        "Critical" => StatusFilter::Critical,
        "Offline" => StatusFilter::Offline,
        _ => StatusFilter::All,
    }
}

fn parse_deployment_filter(value: &str) -> DeploymentFilter {
    match value {
        "Up to Date" => DeploymentFilter::UpToDate,
        "Behind" => DeploymentFilter::Behind,
        "Ahead" => DeploymentFilter::Ahead,
        "Never Deployed" => DeploymentFilter::NeverDeployed,
        "Unknown" => DeploymentFilter::Unknown,
        _ => DeploymentFilter::All,
    }
}

fn environment_label(system: &SystemSummary) -> String {
    system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string())
}

fn matches_environment(system: &SystemSummary, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }

    system
        .environment
        .as_deref()
        .is_some_and(|env| env.eq_ignore_ascii_case(filter))
}

fn matches_health(system: &SystemSummary, filter: StatusFilter) -> bool {
    match filter {
        StatusFilter::All => true,
        StatusFilter::Healthy => system.health_status == HealthStatus::Healthy,
        StatusFilter::Warning => system.health_status == HealthStatus::Warning,
        StatusFilter::Critical => system.health_status == HealthStatus::Critical,
        StatusFilter::Offline => system.health_status == HealthStatus::Offline,
    }
}

fn matches_deployment(system: &SystemSummary, filter: DeploymentFilter) -> bool {
    match filter {
        DeploymentFilter::All => true,
        DeploymentFilter::UpToDate => system.deployment_status == DeploymentStatus::UpToDate,
        DeploymentFilter::Behind => system.deployment_status == DeploymentStatus::Behind,
        DeploymentFilter::Ahead => system.deployment_status == DeploymentStatus::Ahead,
        DeploymentFilter::NeverDeployed => system.deployment_status == DeploymentStatus::NeverDeployed,
        DeploymentFilter::Unknown => true,
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
    vec![
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "atlas-01".to_string(),
            environment: Some("production".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary {
                critical: 0,
                high: 1,
                medium: 4,
                low: 12,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: None,
            deployment_policy: "Immediate".to_string(),
        },
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "luna-02".to_string(),
            environment: Some("staging".to_string()),
            health_status: HealthStatus::Warning,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 1,
                high: 3,
                medium: 6,
                low: 9,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: None,
            deployment_policy: "Boot Only".to_string(),
        },
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "orion-03".to_string(),
            environment: Some("production".to_string()),
            health_status: HealthStatus::Critical,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::Building),
            cve_counts: CveSummary {
                critical: 4,
                high: 6,
                medium: 8,
                low: 10,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: None,
            deployment_policy: "Immediate".to_string(),
        },
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "vega-04".to_string(),
            environment: Some("development".to_string()),
            health_status: HealthStatus::Offline,
            deployment_status: DeploymentStatus::NeverDeployed,
            pipeline_stage: Some(PipelineStage::DryRun),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 2,
                low: 3,
            },
            nixos_version: Some("23.11".to_string()),
            last_seen: None,
            deployment_policy: "Boot Only".to_string(),
        },
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "nova-05".to_string(),
            environment: Some("staging".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::Ahead,
            pipeline_stage: Some(PipelineStage::ReadyForBuild),
            cve_counts: CveSummary {
                critical: 0,
                high: 2,
                medium: 1,
                low: 5,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: None,
            deployment_policy: "Immediate".to_string(),
        },
    ]
}

fn table_class(is_active: bool) -> &'static str {
    if is_active {
        "bg-gray-800 text-white"
    } else {
        ""
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
