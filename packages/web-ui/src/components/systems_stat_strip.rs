//! Systems statistics strip component.
//!
//! Displays fleet metrics with colored accent rails and spark bars showing
//! distribution across environments.

use dioxus::prelude::*;

use crate::api::models::{HealthStatus, SystemSummary};

/// Systems statistics calculated from the systems list.
#[derive(Clone, PartialEq)]
pub struct SystemsStats {
    pub total: usize,
    pub healthy: usize,
    pub warning: usize,
    pub critical: usize,
    pub offline: usize,
    pub critical_cves: usize,
    pub env_distribution: Vec<(String, usize)>,
}

impl SystemsStats {
    /// Calculate stats from a list of systems.
    pub fn from_systems(systems: &[SystemSummary]) -> Self {
        let total = systems.len();
        let healthy = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Healthy)
            .count();
        let warning = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Warning)
            .count();
        let critical = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Critical)
            .count();
        let offline = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Offline)
            .count();
        let critical_cves: usize = systems.iter().map(|s| s.cve_counts.critical).sum();

        // Calculate environment distribution
        let mut env_map = std::collections::HashMap::new();
        for system in systems {
            let env_name = system.environment.clone().unwrap_or_else(|| "unknown".to_string());
            *env_map.entry(env_name).or_insert(0) += 1;
        }
        let mut env_distribution: Vec<(String, usize)> = env_map.into_iter().collect();
        env_distribution.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by count descending

        Self {
            total,
            healthy,
            warning,
            critical,
            offline,
            critical_cves,
            env_distribution,
        }
    }
}

/// Stat card with colored accent rail.
#[component]
fn StatCard(
    label: String,
    value: String,
    meta: String,
    accent_color: String,
    value_color: Option<String>,
    children: Element,
) -> Element {
    let style = format!("--stat-color: {}", accent_color);
    let value_style = value_color.map(|c| format!("color: {}", c)).unwrap_or_default();

    rsx! {
        div {
            class: "stat",
            span { class: "stat-accent", style: "{style}" }
            div { class: "stat-label", "{label}" }
            div {
                class: "stat-value",
                style: "{value_style}",
                "{value}"
            }
            div { class: "stat-meta", "{meta}" }
            {children}
        }
    }
}

/// Environment color mapping for spark bar visualization.
fn env_color(env_name: &str) -> &'static str {
    match env_name.to_lowercase().as_str() {
        "production" | "prod" => "#f87171",
        "staging" | "stage" => "#fbbf24",
        "dev" | "development" => "#60a5fa",
        "edge" => "#2dd4bf",
        "lab" => "#a78bfa",
        _ => "#6b7280",
    }
}

/// Systems statistics strip with 5 key metrics.
///
/// # Example
/// ```
/// rsx! {
///     SystemsStatStrip {
///         systems: systems_list.clone(),
///     }
/// }
/// ```
#[component]
pub fn SystemsStatStrip(systems: Vec<SystemSummary>) -> Element {
    let stats = SystemsStats::from_systems(&systems);
    let needing_attention = stats.warning + stats.critical + stats.offline;
    let env_count = stats.env_distribution.len();

    rsx! {
        div {
            class: "stat-strip",

            // Total systems with spark bar
            StatCard {
                label: "Total".to_string(),
                value: format!("{}", stats.total),
                meta: format!("across {} environment{}", env_count, if env_count != 1 { "s" } else { "" }),
                accent_color: "#a78bfa".to_string(),
                value_color: None,
                children: rsx! {
                    if !stats.env_distribution.is_empty() && stats.total > 0 {
                        div {
                            class: "spark-bar",
                            for (env, count) in stats.env_distribution.iter() {
                                div {
                                    class: "spark-seg",
                                    style: "width: {((*count as f64 / stats.total as f64) * 100.0)}%; background: {env_color(env)}",
                                    title: "{env}: {count}"
                                }
                            }
                        }
                    }
                }
            }

            // Healthy
            StatCard {
                label: "Healthy".to_string(),
                value: format!("{}", stats.healthy),
                meta: format!("{}% of fleet", if stats.total > 0 { (stats.healthy * 100) / stats.total } else { 0 }),
                accent_color: "#34d399".to_string(),
                value_color: Some("#34d399".to_string()),
                children: rsx! {}
            }

            // Warning / drift
            StatCard {
                label: "Warning / drift".to_string(),
                value: format!("{}", stats.warning),
                meta: "behind or drifted".to_string(),
                accent_color: "#fbbf24".to_string(),
                value_color: Some("#fbbf24".to_string()),
                children: rsx! {}
            }

            // Critical / offline
            StatCard {
                label: "Critical / offline".to_string(),
                value: format!("{}", stats.critical + stats.offline),
                meta: format!("{} failing · {} offline", stats.critical, stats.offline),
                accent_color: "#f87171".to_string(),
                value_color: Some("#f87171".to_string()),
                children: rsx! {}
            }

            // CVEs (critical)
            StatCard {
                label: "CVEs (critical)".to_string(),
                value: format!("{}", stats.critical_cves),
                meta: format!("across {} host{}", 
                    systems.iter().filter(|s| s.cve_counts.critical > 0).count(),
                    if systems.iter().filter(|s| s.cve_counts.critical > 0).count() != 1 { "s" } else { "" }
                ),
                accent_color: "#60a5fa".to_string(),
                value_color: None,
                children: rsx! {}
            }
        }
    }
}
