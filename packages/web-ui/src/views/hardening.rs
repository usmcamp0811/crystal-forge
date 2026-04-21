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
                Some(Ok(rows)) => render_system_posture(rows),
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

                Card {
                    title: Some("Top Vulnerable Services".to_string()),
                    children: rsx! { {top_services_card} }
                }

                Card {
                    title: Some("System Hardening Posture".to_string()),
                    children: rsx! { {systems_card} }
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

fn render_system_posture(rows: &[HardeningSystemPostureResponse]) -> Element {
    if rows.is_empty() {
        return rsx! { p { class: "{theme::text::SECONDARY}", "No systems have completed hardening scans yet." } };
    }

    let grouped_rows = group_posture_rows(rows);

    rsx! {
        div { class: "space-y-3",
            div { class: "flex items-start justify-between gap-3 flex-wrap",
                div { class: "space-y-1",
                    p { class: "text-sm {theme::text::PRIMARY}", "Latest posture per system" }
                    p { class: "text-xs {theme::text::SECONDARY}",
                        "Multiple scan snapshots from different evaluated revisions are grouped under each system."
                    }
                }
                span {
                    class: "inline-flex items-center rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} px-2 py-1 text-xs {theme::text::SECONDARY}",
                    "{grouped_rows.len()} systems"
                }
            }

            div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b {theme::surface::CARD_BORDER} text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "System" }
                        th { class: "py-2 pr-3", "Current posture" }
                        th { class: "py-2 pr-3", "Revision history" }
                    }
                }
                tbody {
                    for group in grouped_rows {
                        tr { class: "border-b {theme::surface::DIVIDER}",
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1",
                                    if let Some(system_id) = group.current.system_id {
                                        Link {
                                            class: "{theme::deployment::AHEAD_TEXT} hover:underline font-medium",
                                            to: Route::SystemDetailView { id: system_id.to_string() },
                                            "{display_name(&group.current)}"
                                        }
                                    } else {
                                        span { class: "font-medium {theme::text::PRIMARY}", "{display_name(&group.current)}" }
                                    }
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        span { class: "font-mono text-xs {theme::text::SECONDARY}", "{group.current.config_name}" }
                                        if group.snapshots.len() > 1 {
                                            span {
                                                class: "inline-flex items-center rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} px-2 py-0.5 text-[11px] {theme::text::SECONDARY}",
                                                "{group.snapshots.len()} eval snapshots"
                                            }
                                        }
                                    }
                                }
                            }
                            td { class: "py-2 pr-3",
                                div { class: "flex flex-wrap items-center gap-2",
                                    span { class: "text-lg font-semibold {theme::text::PRIMARY}", "{score_label(group.current.overall_score)}" }
                                    span {
                                        class: "inline-flex items-center rounded-md px-2 py-0.5 text-[11px] font-medium {risk_chip_class(group.current.risk_level.as_deref())}",
                                        "{risk_label(group.current.risk_level.as_deref())}"
                                    }
                                    span {
                                        class: "text-xs {theme::text::SECONDARY}",
                                        "{group.current.total_services.unwrap_or(0)} services"
                                    }
                                }
                            }
                            td { class: "py-2 pr-3",
                                div { class: "space-y-1.5 min-w-[260px]",
                                    for snapshot in group.snapshots.iter().take(3) {
                                        div { class: "flex items-center gap-2 flex-wrap text-xs",
                                            span {
                                                class: if snapshot.derivation_id == group.current.derivation_id {
                                                    "inline-flex items-center rounded-md border px-2 py-0.5 {theme::surface::CARD_BORDER} {theme::deployment::AHEAD_BG} {theme::deployment::AHEAD_TEXT}"
                                                } else {
                                                    "inline-flex items-center rounded-md border px-2 py-0.5 {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::text::SECONDARY}"
                                                },
                                                if snapshot.derivation_id == group.current.derivation_id {
                                                    "Latest eval #{snapshot.derivation_id}"
                                                } else {
                                                    "Eval #{snapshot.derivation_id}"
                                                }
                                            }
                                            if let Some(scanned_at) = format_scan_time(snapshot.last_scan_at) {
                                                span { class: "{theme::text::SECONDARY}", "{scanned_at}" }
                                            } else {
                                                span { class: "{theme::text::MUTED}", "scan time unavailable" }
                                            }
                                        }
                                    }
                                    if group.snapshots.len() > 3 {
                                        p { class: "text-xs {theme::text::MUTED}", "+{group.snapshots.len() - 3} older snapshots" }
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

fn format_scan_time(value: Option<chrono::DateTime<chrono::Utc>>) -> Option<String> {
    value.map(|ts| ts.format("%Y-%m-%d %H:%M UTC").to_string())
}

fn is_forbidden_error(error: &ApiClientError) -> bool {
    matches!(error, ApiClientError::Status { code: 403, .. })
}
