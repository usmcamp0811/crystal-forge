//! System card component for summary views.

use dioxus::prelude::*;

use crate::api::models::{CveSummary, SystemSummary};
use crate::routes::Route;
use crate::theme;

/// Card displaying a system summary.
#[component]
pub fn SystemCard(system: SystemSummary) -> Element {
    let environment = system.environment.clone().unwrap_or_else(|| "Unknown".to_string());
    let pipeline_label = system
        .pipeline_stage
        .map(|stage| stage.label())
        .unwrap_or("Unknown");
    let primary_ip = system.primary_ip.clone().unwrap_or_else(|| "-".to_string());

    let deployment_label = deployment_policy_label(&system.deployment_policy);
    let env_style = environment_style(&environment);

    rsx! {
        Link {
            to: Route::SystemDetailView { id: system.id.to_string() },
            class: "block rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm hover:border-gray-600 transition",

            // Header section with environment tab
            div {
                class: "flex items-center justify-between px-6 py-4 border-b border-gray-800",
                style: "{env_style.header_bg}",
                h3 { class: "text-lg font-semibold text-white pl-0.5", "{system.hostname}" }
                span {
                    class: "inline-flex items-center px-3 py-1 rounded-md text-[10px] font-semibold uppercase tracking-wide {env_style.chip_bg} {env_style.chip_text}",
                    "{environment}"
                }
            }

            // Status section
            div {
                class: "px-5 py-3 bg-gray-800/50",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Status" }
                div {
                    class: "flex flex-wrap gap-2",
                    StatusBadge { label: system.health_status.label(), color_class: system.health_status.color_class(), bg_class: system.health_status.bg_class() }
                    StatusBadge { label: system.deployment_status.label(), color_class: system.deployment_status.color_class(), bg_class: system.deployment_status.bg_class() }
                }
            }

            // Details section
            div {
                class: "px-5 py-3 bg-gray-900",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Details" }
                div {
                    class: "flex flex-wrap gap-2 text-xs {theme::text::MUTED}",
                    span { "{pipeline_label}" }
                    span { "•" }
                    span { "{primary_ip}" }
                    span { "•" }
                    span { "{deployment_label}" }
                    if let Some(nixos_version) = system.nixos_version {
                        span { "•" }
                        span { "NixOS {nixos_version}" }
                    }
                }
            }

            // Vulnerabilities section
            div {
                class: "px-5 py-3 bg-gray-800/50",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Vulnerabilities" }
                CveSummaryRow { cve_counts: system.cve_counts }
            }
        }
    }
}

/// Small status badge used for health and deployment indicators.
#[component]
fn StatusBadge(label: &'static str, color_class: &'static str, bg_class: &'static str) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium {color_class} {bg_class}",
            "{label}"
        }
    }
}

fn deployment_policy_label(policy: &str) -> String {
    match policy {
        "Immediate" => "Auto-deploy: Immediate".to_string(),
        "Boot Only" => "Auto-deploy: On reboot".to_string(),
        _ => policy.to_string(),
    }
}

struct EnvStyle {
    chip_bg: &'static str,
    chip_text: &'static str,
    header_bg: &'static str,
}

fn environment_style(environment: &str) -> EnvStyle {
    match environment.to_lowercase().as_str() {
        "production" => EnvStyle {
            chip_bg: "bg-emerald-500/20",
            chip_text: "text-emerald-300",
            header_bg: "background: rgba(6, 78, 59, 0.5);", // emerald-900 with alpha
        },
        "staging" => EnvStyle {
            chip_bg: "bg-amber-500/20",
            chip_text: "text-amber-300",
            header_bg: "background: rgba(120, 53, 15, 0.5);", // amber-900 with alpha
        },
        "development" => EnvStyle {
            chip_bg: "bg-blue-500/20",
            chip_text: "text-blue-300",
            header_bg: "background: rgba(30, 58, 138, 0.5);", // blue-900 with alpha
        },
        _ => EnvStyle {
            chip_bg: "bg-gray-500/20",
            chip_text: "text-gray-300",
            header_bg: "background: rgba(31, 41, 55, 1);", // gray-800
        },
    }
}

/// CVE summary row with severity counts.
#[component]
fn CveSummaryRow(cve_counts: CveSummary) -> Element {
    rsx! {
        div {
            class: "flex flex-wrap gap-3 text-xs",
            CveCount { label: "Critical", count: cve_counts.critical, color_class: theme::cve::CRITICAL_TEXT }
            CveCount { label: "High", count: cve_counts.high, color_class: theme::cve::HIGH_TEXT }
            CveCount { label: "Medium", count: cve_counts.medium, color_class: theme::cve::MEDIUM_TEXT }
            CveCount { label: "Low", count: cve_counts.low, color_class: theme::cve::LOW_TEXT }
        }
    }
}

/// Individual CVE severity count.
#[component]
fn CveCount(label: &'static str, count: i64, color_class: &'static str) -> Element {
    rsx! {
        span {
            class: "flex items-center gap-1 {color_class}",
            span { class: "font-semibold", "{count}" }
            span { class: "text-gray-400", "{label}" }
        }
    }
}
