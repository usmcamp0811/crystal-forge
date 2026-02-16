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

    rsx! {
        Link {
            to: Route::SystemDetailView { id: system.id.to_string() },
            class: "block rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-6 shadow-sm hover:border-gray-600 transition",
            div {
                class: "flex items-center justify-between mb-4",
                div {
                    class: "space-y-1",
                    h3 { class: "text-lg font-semibold", "{system.hostname}" }
                    p { class: "text-xs {theme::text::MUTED}", "{environment} • {pipeline_label} • {primary_ip}" }
                }
                div {
                    class: "text-xs {theme::text::MUTED}",
                    "{system.deployment_policy}"
                }
            }

            div {
                class: "flex flex-wrap gap-2 mb-4",
                StatusBadge { label: system.health_status.label(), color_class: system.health_status.color_class(), bg_class: system.health_status.bg_class() }
                StatusBadge { label: system.deployment_status.label(), color_class: system.deployment_status.color_class(), bg_class: system.deployment_status.bg_class() }
            }

            CveSummaryRow { cve_counts: system.cve_counts }

            if let Some(nixos_version) = system.nixos_version {
                p { class: "mt-4 text-xs {theme::text::MUTED}", "NixOS {nixos_version}" }
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
