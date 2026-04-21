use dioxus::prelude::*;
use std::collections::BTreeMap;

use crate::api::client::{
    ApiClientError, fetch_hardening_fleet_summary, fetch_hardening_system_postures,
    fetch_hardening_top_services,
};
use crate::api::models::{HardeningSystemPostureResponse, HardeningTopServiceResponse};
use crate::components::layout::Card;
use crate::components::stat_card::StatCard;
use crate::routes::Route;
use crate::theme;

#[component]
pub fn HardeningView() -> Element {
    let summary = use_resource(move || async move { fetch_hardening_fleet_summary().await });
    let top_services = use_resource(move || async move { fetch_hardening_top_services(Some(10)).await });
    let postures = use_resource(move || async move { fetch_hardening_system_postures().await });

    let content = match &*summary.read_unchecked() {
        Some(Ok(summary)) => {
            let top_services_card = match &*top_services.read_unchecked() {
                Some(Ok(rows)) => render_top_services(rows),
                Some(Err(_)) => {
                    rsx! { p { class: "{theme::text::SECONDARY}", "Failed to load top vulnerable services." } }
                }
                None => rsx! { p { class: "{theme::text::SECONDARY}", "Loading..." } },
            };

            let systems_card = match &*postures.read_unchecked() {
                Some(Ok(rows)) => render_environment_posture(rows),
                Some(Err(_)) => {
                    rsx! { p { class: "{theme::text::SECONDARY}", "Failed to load system posture." } }
                }
                None => rsx! { p { class: "{theme::text::SECONDARY}", "Loading..." } },
            };

            rsx! {
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4",
                    StatCard {
                        label: "Systems Scanned".to_string(),
                        value: summary.total_systems_scanned.to_string(),
                    }
                    StatCard {
                        label: "Average Fleet Score".to_string(),
                        value: summary
                            .avg_fleet_score
                            .map(|v| format!("{v:.1}"))
                            .unwrap_or_else(|| "n/a".to_string()),
                    }
                    StatCard {
                        label: "Vulnerable Services".to_string(),
                        value: summary.total_vulnerable_services.to_string(),
                        color_class: theme::health::CRITICAL_TEXT.to_string(),
                    }
                    StatCard {
                        label: "Well Hardened".to_string(),
                        value: summary.total_well_hardened_services.to_string(),
                        color_class: theme::health::HEALTHY_TEXT.to_string(),
                    }
                }

                div { class: "grid grid-cols-1 xl:grid-cols-2 gap-6 items-start",
                    div {
                        Card {
                            title: Some("Top Vulnerable Services".to_string()),
                            children: rsx! { {top_services_card} }
                        }
                    }

                    div {
                        Card {
                            title: Some("Environment Hardening Posture".to_string()),
                            children: rsx! { {systems_card} }
                        }
                    }
                }
            }
        }
        Some(Err(error)) => {
            let message = if is_forbidden_error(error) {
                "Admin privileges are required to view hardening dashboard data.".to_string()
            } else {
                format!("Failed to load hardening dashboard: {error}")
            };

            rsx! {
                Card {
                    title: Some("Hardening Status".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "{message}" }
                    }
                }
            }
        }
        None => rsx! {
            Card {
                title: Some("Hardening Status".to_string()),
                children: rsx! {
                    p { class: "{theme::text::SECONDARY}", "Loading hardening dashboard..." }
                }
            }
        },
    };

    rsx! {
        div {
            class: "space-y-6",
            h1 {
                class: "{theme::typography::PAGE_TITLE}",
                "Systemd Hardening"
            }
            {content}
        }
    }
}

fn render_top_services(rows: &[HardeningTopServiceResponse]) -> Element {
    if rows.is_empty() {
        return rsx! { p { class: "{theme::text::SECONDARY}", "No vulnerable services found." } };
    }

    rsx! {
        div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b {theme::surface::CARD_BORDER} text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "Service" }
                        th { class: "py-2 pr-3", "Affected Systems" }
                        th { class: "py-2 pr-3", "Avg Score" }
                        th { class: "py-2 pr-3", "Range" }
                    }
                }
                tbody {
                    for row in rows {
                        tr { class: "border-b {theme::surface::DIVIDER}",
                            td { class: "py-2 pr-3 font-mono {theme::text::PRIMARY}", "{row.service_name}" }
                            td { class: "py-2 pr-3 {theme::text::PRIMARY}", "{row.affected_systems_count}" }
                            td { class: "py-2 pr-3 {theme::text::PRIMARY}", {format!("{:.1}", row.avg_score)} }
                            td { class: "py-2 pr-3 {theme::text::SECONDARY}", "{row.min_score} - {row.max_score}" }
                        }
                    }
                }
            }
        }
    }
}

fn render_environment_posture(rows: &[HardeningSystemPostureResponse]) -> Element {
    if rows.is_empty() {
        return rsx! { p { class: "{theme::text::SECONDARY}", "No environments have completed hardening scans yet." } };
    }

    let grouped_rows = group_posture_rows(rows);
    let environments = aggregate_environments(&grouped_rows);

    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-start justify-between gap-3 flex-wrap",
                div { class: "space-y-1",
                    p { class: "text-sm {theme::text::PRIMARY}", "Posture rolled up by environment" }
                    p { class: "text-xs {theme::text::SECONDARY}",
                        "Each row summarizes the latest posture for systems in that environment and highlights where follow-up review is most needed."
                    }
                }
                span {
                    class: "inline-flex items-center rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} px-2 py-1 text-xs {theme::text::SECONDARY}",
                    "{environments.len()} environments"
                }
            }

            div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b {theme::surface::CARD_BORDER} text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "Environment" }
                        th { class: "py-2 pr-3", "Coverage" }
                        th { class: "py-2 pr-3", "Avg posture" }
                        th { class: "py-2 pr-3", "Needs review" }
                        th { class: "py-2 pr-3", "Watch list" }
                    }
                }
                tbody {
                    for group in environments {
                        tr { class: "border-b {theme::surface::DIVIDER}",
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1",
                                    span { class: "font-medium {theme::text::PRIMARY}", "{group.environment_name}" }
                                    p { class: "text-xs {theme::text::SECONDARY}",
                                        "{group.snapshot_count} scan snapshots represented"
                                    }
                                }
                            }
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1 text-xs",
                                    p {
                                        span { class: "text-lg font-semibold {theme::text::PRIMARY}", "{group.system_count}" }
                                        span { class: "ml-2 {theme::text::SECONDARY}", "systems" }
                                    }
                                    p { class: "{theme::text::SECONDARY}", "{group.service_count} services scanned" }
                                }
                            }
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1",
                                    div { class: "flex flex-wrap items-center gap-2",
                                    span { class: "text-lg font-semibold {theme::text::PRIMARY}", "{format_float(group.avg_score)}" }
                                    span {
                                        class: "inline-flex items-center rounded-md px-2 py-0.5 text-[11px] font-medium {risk_chip_class(Some(group.risk_band.as_str()))}",
                                        "{risk_label(Some(group.risk_band.as_str()))}"
                                    }
                                }
                                    p { class: "text-xs {theme::text::SECONDARY}", "Avg of latest system scans" }
                                }
                            }
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1 text-xs {theme::text::SECONDARY}",
                                    p { "{group.vulnerable_services} vulnerable services" }
                                    p { "{group.poorly_hardened_services} poorly hardened services" }
                                    p { class: "{theme::text::MUTED}", "{systems_needing_review(&group)} of {group.system_count} systems below target" }
                                }
                            }
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1.5 min-w-[240px]",
                                    for system in group.worst_systems.iter().take(3) {
                                        div { class: "flex items-center gap-2 flex-wrap text-xs",
                                            if let Some(system_id) = system.system_id {
                                                Link {
                                                    class: "{theme::deployment::AHEAD_TEXT} hover:underline font-medium",
                                                    to: Route::SystemDetailView { id: system_id.to_string() },
                                                    "{display_name(system)}"
                                                }
                                            } else {
                                                span { class: "font-medium {theme::text::PRIMARY}", "{display_name(system)}" }
                                            }
                                            span {
                                                class: "inline-flex items-center rounded-md px-2 py-0.5 text-[11px] font-medium {risk_chip_class(system.risk_level.as_deref())}",
                                                "{score_label(system.overall_score)}"
                                            }
                                            span { class: "{theme::text::MUTED}", "{system.config_name}" }
                                        }
                                    }
                                    if group.worst_systems.len() > 3 {
                                        p { class: "text-xs {theme::text::MUTED}", "+{group.worst_systems.len() - 3} more systems" }
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

#[derive(Clone)]
struct PostureGroup {
    current: HardeningSystemPostureResponse,
    snapshots: Vec<HardeningSystemPostureResponse>,
}

#[derive(Clone)]
struct EnvironmentPostureGroup {
    environment_name: String,
    system_count: usize,
    snapshot_count: usize,
    service_count: i32,
    avg_score: f64,
    vulnerable_services: i32,
    poorly_hardened_services: i32,
    risk_band: String,
    worst_systems: Vec<HardeningSystemPostureResponse>,
}

fn group_posture_rows(rows: &[HardeningSystemPostureResponse]) -> Vec<PostureGroup> {
    let mut grouped = BTreeMap::<String, Vec<HardeningSystemPostureResponse>>::new();

    for row in rows.iter().cloned() {
        let key = row
            .system_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| row.config_name.clone());
        grouped.entry(key).or_default().push(row);
    }

    let mut groups = grouped
        .into_values()
        .map(|mut snapshots| {
            snapshots.sort_by(|a, b| {
                b.last_scan_at
                    .cmp(&a.last_scan_at)
                    .then_with(|| b.derivation_id.cmp(&a.derivation_id))
            });
            let current = snapshots[0].clone();
            PostureGroup { current, snapshots }
        })
        .collect::<Vec<_>>();

    groups.sort_by(|a, b| {
        a.current
            .overall_score
            .unwrap_or(i32::MAX)
            .cmp(&b.current.overall_score.unwrap_or(i32::MAX))
            .then_with(|| display_name(&a.current).cmp(&display_name(&b.current)))
    });

    groups
}

fn display_name(row: &HardeningSystemPostureResponse) -> String {
    row.hostname
        .clone()
        .unwrap_or_else(|| row.config_name.clone())
}

fn aggregate_environments(groups: &[PostureGroup]) -> Vec<EnvironmentPostureGroup> {
    let mut map = BTreeMap::<String, Vec<PostureGroup>>::new();

    for group in groups.iter().cloned() {
        let env_name = group
            .current
            .environment_name
            .clone()
            .unwrap_or_else(|| "Unassigned".to_string());
        map.entry(env_name).or_default().push(group);
    }

    let mut environments = map
        .into_iter()
        .map(|(environment_name, mut systems)| {
            systems.sort_by(|a, b| {
                a.current
                    .overall_score
                    .unwrap_or(i32::MAX)
                    .cmp(&b.current.overall_score.unwrap_or(i32::MAX))
            });

            let system_count = systems.len();
            let snapshot_count = systems.iter().map(|s| s.snapshots.len()).sum();
            let service_count = systems
                .iter()
                .map(|s| s.current.total_services.unwrap_or(0))
                .sum();
            let vulnerable_services = systems
                .iter()
                .map(|s| s.current.vulnerable_count.unwrap_or(0))
                .sum();
            let poorly_hardened_services = systems
                .iter()
                .map(|s| s.current.poorly_hardened_count.unwrap_or(0))
                .sum();

            let scored = systems
                .iter()
                .filter_map(|s| s.current.overall_score.map(|score| score as f64))
                .collect::<Vec<_>>();
            let avg_score = if scored.is_empty() {
                0.0
            } else {
                scored.iter().sum::<f64>() / scored.len() as f64
            };

            let risk_band = risk_band_from_score(avg_score).to_string();
            let worst_systems = systems.into_iter().map(|s| s.current).collect::<Vec<_>>();

            EnvironmentPostureGroup {
                environment_name,
                system_count,
                snapshot_count,
                service_count,
                avg_score,
                vulnerable_services,
                poorly_hardened_services,
                risk_band,
                worst_systems,
            }
        })
        .collect::<Vec<_>>();

    environments.sort_by(|a, b| {
        a.avg_score
            .partial_cmp(&b.avg_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.environment_name.cmp(&b.environment_name))
    });

    environments
}

fn risk_band_from_score(score: f64) -> &'static str {
    if score >= 80.0 {
        "well_hardened"
    } else if score >= 60.0 {
        "moderately_hardened"
    } else if score >= 40.0 {
        "poorly_hardened"
    } else {
        "vulnerable"
    }
}

fn format_float(value: f64) -> String {
    format!("{value:.1}")
}

fn systems_needing_review(group: &EnvironmentPostureGroup) -> usize {
    group
        .worst_systems
        .iter()
        .filter(|system| system.overall_score.unwrap_or(0) < 60)
        .count()
}

fn score_label(score: Option<i32>) -> String {
    score.map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn risk_label(level: Option<&str>) -> &'static str {
    match level.unwrap_or("unknown") {
        "well_hardened" => "well hardened",
        "moderately_hardened" => "moderate",
        "poorly_hardened" => "poorly hardened",
        "vulnerable" => "vulnerable",
        _ => "unknown",
    }
}

fn risk_chip_class(level: Option<&str>) -> String {
    match level.unwrap_or("unknown") {
        "well_hardened" => format!("{} {} {}", theme::health::HEALTHY_BORDER, theme::health::HEALTHY_BG, theme::health::HEALTHY_TEXT),
        "moderately_hardened" => format!("{} {} {}", theme::health::WARNING_BORDER, theme::health::WARNING_BG, theme::health::WARNING_TEXT),
        "poorly_hardened" | "vulnerable" => format!("{} {} {}", theme::health::CRITICAL_BORDER, theme::health::CRITICAL_BG, theme::health::CRITICAL_TEXT),
        _ => format!("{} {} {}", theme::surface::CARD_BORDER, theme::surface::SUBTLE_BG, theme::text::SECONDARY),
    }
}


fn is_forbidden_error(error: &ApiClientError) -> bool {
    matches!(error, ApiClientError::Status { code: 403, .. })
}
