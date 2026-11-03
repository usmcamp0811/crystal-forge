//! Systems statistics strip component.
//!
//! Displays fleet metrics with colored accent rails and spark bars showing
//! distribution across environments.

use dioxus::prelude::*;

use crate::api::models::{DeploymentStatus, HealthStatus, ManagementMode, SystemSummary};

/// Systems statistics calculated from the systems list.
#[derive(Clone, PartialEq)]
pub struct SystemsStats {
    pub total: usize,
    pub healthy: usize,
    pub warning: usize,
    pub critical: usize,
    pub offline: usize,
    pub critical_cves: i64,
    pub env_distribution: Vec<(String, usize)>,
}

impl SystemsStats {
    /// Calculates operational statistics for managed machines only.
    pub fn from_systems(systems: &[SystemSummary]) -> Self {
        // Operational health and runtime CVEs apply only to managed machines.
        let managed: Vec<_> = systems
            .iter()
            .filter(|s| s.management_mode == ManagementMode::Managed)
            .cloned()
            .collect();
        let systems = managed.as_slice();
        let total = systems.len();
        let healthy = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Healthy)
            .count();
        let warning = systems
            .iter()
            .filter(|s| {
                s.health_status == HealthStatus::Warning
                    || s.deployment_status == DeploymentStatus::Behind
            })
            .count();
        let critical = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Critical)
            .count();
        let offline = systems
            .iter()
            .filter(|s| s.health_status == HealthStatus::Offline)
            .count();
        let critical_cves: i64 = systems.iter().map(|s| s.cve_counts.critical).sum();

        // Calculate environment distribution
        let mut env_map = std::collections::HashMap::new();
        for system in systems {
            let env_name = system
                .environment
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
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
    let value_style = value_color
        .map(|c| format!("color: {}", c))
        .unwrap_or_default();

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
fn env_color(env_name: &str, environment_colors: &[(String, String)]) -> String {
    if let Some((_, color_hex)) = environment_colors
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(env_name))
    {
        return color_hex.clone();
    }

    match env_name.to_lowercase().as_str() {
        "production" | "prod" => "#f87171".to_string(),
        "staging" | "stage" => "#fbbf24".to_string(),
        "dev" | "development" => "#60a5fa".to_string(),
        "edge" => "#2dd4bf".to_string(),
        "lab" => "#a78bfa".to_string(),
        _ => "#6b7280".to_string(),
    }
}

/// Returns the value and caption of the "CVEs (critical)" card.
///
/// Managed counts come from current runtime evidence. External and reference
/// counts come from their latest sealed configuration scan; a resource without
/// one is absent, never zero. `unavailable` counts external/reference resources
/// whose evidence is loading or unreadable. Their findings are unknown, so the
/// total becomes a lower bound (`N+`) and the caption says how many are missing.
/// The card never shows `0` while any resource is unread.
pub(crate) fn critical_cve_card(
    managed_critical: i64,
    managed_with_critical: usize,
    configuration_critical: &[i64],
    unavailable: usize,
) -> (String, String) {
    let total = managed_critical + configuration_critical.iter().sum::<i64>();
    let resources_with_critical =
        managed_with_critical + configuration_critical.iter().filter(|c| **c > 0).count();
    let value = if unavailable > 0 {
        format!("{total}+")
    } else {
        total.to_string()
    };
    let mut meta = format!("across {resources_with_critical} scanned resources");
    if unavailable > 0 {
        meta.push_str(&format!(
            " · {unavailable} configuration {} not read yet",
            if unavailable == 1 { "scan" } else { "scans" }
        ));
    }
    (value, meta)
}

#[cfg(test)]
mod critical_card_tests {
    use super::critical_cve_card;

    #[test]
    fn complete_evidence_shows_an_exact_total() {
        assert_eq!(
            critical_cve_card(3, 2, &[4, 0, 1], 0),
            ("8".to_string(), "across 4 scanned resources".to_string())
        );
    }

    #[test]
    fn unread_configuration_evidence_makes_the_total_a_lower_bound() {
        let (value, meta) = critical_cve_card(3, 2, &[4], 2);
        assert_eq!(value, "7+");
        assert!(
            meta.ends_with("2 configuration scans not read yet"),
            "{meta}"
        );
        let (value, meta) = critical_cve_card(0, 0, &[], 1);
        assert_eq!(value, "0+", "nothing read yet is not zero findings");
        assert!(
            meta.ends_with("1 configuration scan not read yet"),
            "{meta}"
        );
    }

    #[test]
    fn no_resources_at_all_stays_a_plain_zero() {
        assert_eq!(
            critical_cve_card(0, 0, &[], 0),
            ("0".to_string(), "across 0 scanned resources".to_string())
        );
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
pub fn SystemsStatStrip(
    systems: Vec<SystemSummary>,
    #[props(default)] environment_colors: Vec<(String, String)>,
    /// Critical counts from the latest sealed configuration scan of each
    /// external/reference resource that has one. Resources without sealed
    /// evidence are absent, never counted as zero.
    #[props(default)]
    configuration_critical: Vec<i64>,
    /// External/reference resources whose evidence is still loading or could not
    /// be read. Their findings are unknown, so the total is a lower bound.
    #[props(default)]
    configuration_unavailable: usize,
) -> Element {
    let stats = SystemsStats::from_systems(&systems);
    let managed_with_critical = systems
        .iter()
        .filter(|s| s.management_mode == ManagementMode::Managed && s.cve_counts.critical > 0)
        .count();
    let (critical_value, critical_meta) = critical_cve_card(
        stats.critical_cves as i64,
        managed_with_critical,
        &configuration_critical,
        configuration_unavailable,
    );

    rsx! {
        div {
            class: "stat-strip",

            // Total systems with spark bar
            StatCard {
                label: "Managed".to_string(),
                value: format!("{}", stats.total),
                meta: format!("+ {} external · {} reference", systems.iter().filter(|s| s.management_mode == ManagementMode::External).count(), systems.iter().filter(|s| s.management_mode == ManagementMode::Reference).count()),
                accent_color: "#a78bfa".to_string(),
                value_color: None,
                children: rsx! {
                    if !stats.env_distribution.is_empty() && stats.total > 0 {
                        div {
                            class: "spark-bar",
                            for (env, count) in stats.env_distribution.iter() {
                                {
                                    let pct = (*count as f64 / stats.total as f64) * 100.0;
                                    let color = env_color(env, &environment_colors);

                                    let env_title = format!("{}: {}", env, count);
                                    let segment_style = format!("width: {:.1}%; background: {};", pct, color);
                                    rsx! {
                                        div {
                                            class: "spark-seg",
                                            style: "{segment_style}",
                                            title: "{env_title}"
                                        }
                                    }
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
                meta: format!("{}% of managed · agent-reported", if stats.total > 0 { (stats.healthy * 100) / stats.total } else { 0 }),
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

            // CVEs (critical). Design: one total "across N scanned resources".
            // Managed counts come from current runtime evidence; external and
            // reference counts come from their latest sealed configuration scan.
            StatCard {
                label: "CVEs (critical)".to_string(),
                value: critical_value,
                meta: critical_meta,
                accent_color: "#60a5fa".to_string(),
                value_color: None,
                children: rsx! {}
            }
        }
    }
}
