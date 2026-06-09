//! Fleet health breakdown — stacked bar + stat tiles (design-reference parity).

use dioxus::prelude::*;

use crate::api::models::FleetHealthSummary;

/// Fleet health rollup: big healthy count, stacked usage bar, and 4 stat tiles.
#[component]
pub fn FleetHealthBreakdown(
    health: FleetHealthSummary,
    #[props(default)] flake_filter: Option<String>,
    #[props(default)] healthy_hosts: Vec<String>,
    #[props(default)] warning_hosts: Vec<String>,
    #[props(default)] critical_hosts: Vec<String>,
    #[props(default)] offline_hosts: Vec<String>,
) -> Element {
    let _ = (healthy_hosts, warning_hosts, critical_hosts, offline_hosts);
    let filter_label = flake_filter;

    let total = health.total();
    let total_f = total.max(1) as f64;
    let pct = |count: i64| (count as f64 / total_f) * 100.0;

    let tiles = [
        ("Healthy", "#34d399", health.healthy),
        ("Warning", "#fbbf24", health.warning),
        ("Critical", "#f87171", health.critical),
        ("Offline", "#6b7280", health.offline),
    ];

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:14px;",
            "data-testid": "fleet-health-breakdown",

            if let Some(flake_name) = filter_label {
                div {
                    style: "font-size:11px; color: var(--cf-text-muted);",
                    "{flake_name} (global fleet health)"
                }
            }

            // Big healthy count
            div {
                style: "display:flex; align-items:baseline; gap:10px;",
                span {
                    style: "font-size:32px; font-weight:700; color: var(--cf-text-primary); line-height:1; font-variant-numeric:tabular-nums;",
                    "{health.healthy}"
                }
                span {
                    style: "font-size:14px; color: var(--cf-text-muted);",
                    "of {total} healthy"
                }
            }

            // Stacked bar
            div {
                style: "display:flex; height:8px; border-radius:99px; overflow:hidden; background: var(--cf-subtle-bg);",
                if health.healthy > 0 {
                    div { style: "width:{pct(health.healthy)}%; background:#34d399;" }
                }
                if health.warning > 0 {
                    div { style: "width:{pct(health.warning)}%; background:#fbbf24;" }
                }
                if health.critical > 0 {
                    div { style: "width:{pct(health.critical)}%; background:#f87171;" }
                }
                if health.offline > 0 {
                    div { style: "width:{pct(health.offline)}%; background:#6b7280;" }
                }
            }

            // Stat tiles
            div {
                style: "display:grid; grid-template-columns: repeat(4, 1fr); gap:8px; font-size:11px;",
                for (label, color, n) in tiles {
                    div {
                        key: "{label}",
                        "data-testid": "fleet-health-tile",
                        "data-status": "{label.to_lowercase()}",
                        "data-count": "{n}",
                        style: "padding:8px 10px; border-radius:6px; background: var(--cf-subtle-bg);",
                        div {
                            style: "display:flex; align-items:center; gap:5px;",
                            span { style: "width:6px; height:6px; border-radius:50%; background:{color};" }
                            span { style: "color: var(--cf-text-muted);", "{label}" }
                        }
                        div {
                            style: "font-size:18px; font-weight:700; color:{color}; margin-top:2px; font-variant-numeric:tabular-nums;",
                            "{n}"
                        }
                    }
                }
            }
        }
    }
}
