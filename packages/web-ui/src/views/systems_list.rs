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
                    "data-testid": "systems-cards",
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
                    "data-testid": "systems-table",
                    table {
                        class: "w-full",
                        thead {
                            class: "{theme::surface::SUBTLE_BG}",
                            tr {
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER} text-left", div { class: "flex items-center", "Hostname" } }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER} text-left", div { class: "flex items-center", "IP" } }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER} text-left", div { class: "flex items-center", "Environment" } }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER} text-left", div { class: "flex items-center", "Health" } }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER} text-left", div { class: "flex items-center", "Deployment" } }
                                th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER} text-left", div { class: "flex items-center", "CVEs" } }
                            }
                        }
                        tbody {
                            class: "divide-y {theme::surface::DIVIDER}",
                            for system in systems {
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

fn ip_label(system: &SystemSummary) -> String {
    system
        .primary_ip
        .clone()
        .unwrap_or_else(|| "-".to_string())
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
