//! System card component for summary views.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use std::collections::HashMap;

use crate::api::models::{CveSummary, SystemSummary};
use crate::routes::Route;
use crate::theme;

/// Card displaying a system summary.
#[component]
pub fn SystemCard(system: SystemSummary, on_remove: EventHandler) -> Element {
    let navigator = use_navigator();
    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let pipeline_label = system
        .pipeline_stage
        .map(|stage| stage.label())
        .unwrap_or("Unknown");
    let primary_ip = system.primary_ip.clone().unwrap_or_else(|| "-".to_string());

    let deployment_label = deployment_policy_label(&system.deployment_policy);
    let env_style = environment_style(&environment);
    let system_id = system.id;

    rsx! {
        div {
            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm",

            // Header section with environment tab
            Link {
                to: Route::SystemDetailView { id: system.id.to_string() },
                class: "block",
                div {
                    class: "flex items-center justify-between px-6 py-4 border-b border-gray-800 hover:bg-gray-800/30 transition",
                    style: "{env_style.header_style}",
                    h3 { class: "text-lg font-semibold text-white pl-0.5", "{system.hostname}" }
                    span {
                        class: "inline-flex items-center px-3 py-1 rounded-md text-[10px] font-semibold uppercase tracking-wide",
                        style: "{env_style.chip_style}",
                        "{environment}"
                    }
                }

                // Status section
                div {
                    class: "px-6 py-3 bg-gray-800/50",
                    p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-2", "Status" }
                    div {
                        class: "flex flex-wrap gap-2",
                        StatusBadge { label: system.health_status.label(), color_class: system.health_status.color_class(), bg_class: system.health_status.bg_class() }
                        StatusBadge { label: system.deployment_status.label(), color_class: system.deployment_status.color_class(), bg_class: system.deployment_status.bg_class() }
                    }
                }

                // Details section
                div {
                    class: "px-6 py-3 bg-gray-900",
                    p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-3", "Details" }
                    div {
                        class: "grid grid-cols-2 gap-3 text-sm",
                        div {
                            span { class: "text-gray-500 text-xs block mb-0.5", "IP Address" }
                            span { class: "font-mono text-gray-200", "{primary_ip}" }
                        }
                        div {
                            span { class: "text-gray-500 text-xs block mb-0.5", "Deploy Policy" }
                            span { class: "text-gray-300", "{deployment_label}" }
                        }
                        if let Some(nixos_version) = system.nixos_version {
                            div {
                                span { class: "text-gray-500 text-xs block mb-0.5", "Operating System" }
                                span { class: "text-gray-200", "NixOS {nixos_version}" }
                            }
                        }
                    }
                }

                // Vulnerabilities section
                div {
                    class: "px-6 py-3 bg-gray-800/50",
                    p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500 mb-3", "Vulnerabilities" }
                    CveSummaryRow { cve_counts: system.cve_counts }
                }
            }

            // Actions footer
            div {
                class: "px-6 py-3 bg-gray-800/50 flex items-center justify-end border-t {theme::surface::CARD_BORDER}",
                button {
                    class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                    onclick: move |_| on_remove.call(()),
                    "Remove"
                }
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
        "manual" => "Deploy policy: Manual".to_string(),
        "auto_latest" => "Deploy policy: Auto latest".to_string(),
        "pinned" => "Deploy policy: Pinned".to_string(),
        "Immediate" => "Auto-deploy: Immediate".to_string(),
        "Boot Only" => "Auto-deploy: On reboot".to_string(),
        _ => policy.to_string(),
    }
}

const ENV_COLOR_STORAGE_KEY: &str = "crystal_forge.environments.colors";

struct EnvStyle {
    chip_style: String,
    header_style: String,
}

fn environment_style(environment: &str) -> EnvStyle {
    let color = environment_color_for(environment);
    EnvStyle {
        chip_style: format!(
            "background: {}; border: 1px solid {}; color: #F8FAFC;",
            rgba(&color, 0.24),
            rgba(&color, 0.75)
        ),
        header_style: format!(
            "background: linear-gradient(135deg, {} 0%, rgba(17, 24, 39, 0.92) 100%);",
            rgba(&color, 0.42)
        ),
    }
}

fn environment_color_for(environment: &str) -> String {
    if let Ok(map) = LocalStorage::get::<HashMap<String, String>>(ENV_COLOR_STORAGE_KEY) {
        if let Some(value) = map.get(&environment.to_lowercase()) {
            return normalize_color_hex(value);
        }
    }

    match environment.to_lowercase().as_str() {
        "production" => "#0F766E".to_string(),
        "staging" => "#B45309".to_string(),
        "development" => "#2563EB".to_string(),
        _ => "#6B7280".to_string(),
    }
}

fn normalize_color_hex(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].chars().all(|ch| ch.is_ascii_hexdigit())
    {
        trimmed.to_uppercase()
    } else {
        "#6B7280".to_string()
    }
}

fn rgba(hex: &str, alpha: f32) -> String {
    let color = normalize_color_hex(hex);
    let r = u8::from_str_radix(&color[1..3], 16).unwrap_or(107);
    let g = u8::from_str_radix(&color[3..5], 16).unwrap_or(114);
    let b = u8::from_str_radix(&color[5..7], 16).unwrap_or(128);
    format!("rgba({r}, {g}, {b}, {alpha})")
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
