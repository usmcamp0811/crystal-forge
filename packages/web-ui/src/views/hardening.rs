use dioxus::prelude::*;

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

    rsx! {
        div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b {theme::surface::CARD_BORDER} text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "System" }
                        th { class: "py-2 pr-3", "Config" }
                        th { class: "py-2 pr-3", "Score" }
                        th { class: "py-2 pr-3", "Risk" }
                        th { class: "py-2 pr-3", "Services" }
                    }
                }
                tbody {
                    for row in rows {
                        tr { class: "border-b {theme::surface::DIVIDER}",
                            td { class: "py-2 pr-3",
                                if let Some(system_id) = row.system_id {
                                    Link {
                                        class: "{theme::deployment::AHEAD_TEXT} hover:underline",
                                        to: Route::SystemDetailView { id: system_id.to_string() },
                                        "{row.hostname.clone().unwrap_or_else(|| row.config_name.clone())}"
                                    }
                                } else {
                                    span { class: "{theme::text::PRIMARY}", "{row.hostname.clone().unwrap_or_else(|| row.config_name.clone())}" }
                                }
                            }
                            td { class: "py-2 pr-3 font-mono {theme::text::SECONDARY}", "{row.config_name}" }
                            td { class: "py-2 pr-3 {theme::text::PRIMARY}", "{row.overall_score.map(|v| v.to_string()).unwrap_or_else(|| \"n/a\".to_string())}" }
                            td { class: "py-2 pr-3 {theme::text::SECONDARY}", "{row.risk_level.clone().unwrap_or_else(|| \"unknown\".to_string())}" }
                            td { class: "py-2 pr-3 {theme::text::SECONDARY}", "{row.total_services.map(|v| v.to_string()).unwrap_or_else(|| \"0\".to_string())}" }
                        }
                    }
                }
            }
        }
    }
}

fn is_forbidden_error(error: &ApiClientError) -> bool {
    matches!(error, ApiClientError::Status { code: 403, .. })
}
