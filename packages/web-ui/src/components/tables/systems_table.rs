//! Systems table component with sorting.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::models::{DeploymentStatus, HealthStatus, SystemSummary};
use crate::components::chips::{Chip, ChipVariant, EnvBadge, StatusDot};
use crate::components::heartbeat_spinner::HeartbeatSpinner;
use crate::components::tables::{SortDirection, SortableHeader};

/// Column that can be sorted in the systems table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemsSortColumn {
    Hostname,
    Ip,
    Environment,
    Health,
    Deployment,
    Cves,
}

/// Table displaying a list of systems with sortable columns.
///
/// Each row shows system info including hostname, IP, environment, health,
/// deployment status, and CVE counts. Clicking a row navigates to the
/// system detail view.
#[component]
pub fn SystemsTable(
    /// Systems to display
    systems: Vec<SystemSummary>,
    /// Called when user clicks remove on a system
    on_remove: EventHandler<Uuid>,
    /// Called when user clicks update key on a system
    on_update_key: EventHandler<Uuid>,
    /// Called when user clicks edit on a system
    on_edit: EventHandler<Uuid>,
    /// Called when user clicks deploy on a system
    on_deploy: EventHandler<Uuid>,
    /// Called when user clicks a row/open action
    on_open: EventHandler<Uuid>,
    /// Currently selected row (for preview drawer highlight)
    #[props(default = None)]
    selected_id: Option<Uuid>,
    /// Whether to use compact density
    #[props(default = false)]
    compact: bool,
) -> Element {
    let mut sort_column = use_signal(|| None::<SystemsSortColumn>);
    let mut sort_direction = use_signal(|| SortDirection::Asc);

    let sorted_systems = {
        let mut sorted = systems.clone();
        if let Some(column) = *sort_column.read() {
            let dir = *sort_direction.read();
            sorted.sort_by(|a, b| {
                let cmp = match column {
                    SystemsSortColumn::Hostname => {
                        a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase())
                    }
                    SystemsSortColumn::Ip => {
                        let a_ip = a.primary_ip.as_deref().unwrap_or("");
                        let b_ip = b.primary_ip.as_deref().unwrap_or("");
                        a_ip.cmp(b_ip)
                    }
                    SystemsSortColumn::Environment => {
                        let a_env = a.environment.as_deref().unwrap_or("");
                        let b_env = b.environment.as_deref().unwrap_or("");
                        a_env.to_lowercase().cmp(&b_env.to_lowercase())
                    }
                    SystemsSortColumn::Health => {
                        a.health_status.label().cmp(b.health_status.label())
                    }
                    SystemsSortColumn::Deployment => {
                        a.deployment_status.label().cmp(b.deployment_status.label())
                    }
                    SystemsSortColumn::Cves => {
                        let a_total = a.cve_counts.critical
                            + a.cve_counts.high
                            + a.cve_counts.medium
                            + a.cve_counts.low;
                        let b_total = b.cve_counts.critical
                            + b.cve_counts.high
                            + b.cve_counts.medium
                            + b.cve_counts.low;
                        a_total.cmp(&b_total)
                    }
                };
                match dir {
                    SortDirection::Asc => cmp,
                    SortDirection::Desc => cmp.reverse(),
                }
            });
        }
        sorted
    };

    let current_col = *sort_column.read();
    let current_dir = *sort_direction.read();
    let table_class = if compact {
        "sys-table compact"
    } else {
        "sys-table"
    };

    rsx! {
        div {
            class: "card",
            style: "overflow: hidden;",
            div {
                class: "overflow-x-auto",
                "data-testid": "systems-table",
                table {
                    class: "{table_class}",
                    thead {
                        tr {
                            // Host — 22% width matching design
                            th {
                                style: "width: 22%;",
                                "Host"
                            }
                            th { "Env" }
                            th { "Status" }
                            th { "Flake · commit" }
                            th { "Deploy" }
                            th { "CVEs" }
                            th { "Heartbeat" }
                            // Actions — empty header, right-aligned
                            th { style: "text-align: right;", " " }
                        }
                    }
                    tbody {
                        for system in sorted_systems {
                            tr {
                                class: if selected_id == Some(system.id) {
                                    "cursor-pointer selected"
                                } else {
                                    "cursor-pointer"
                                },
                                onclick: move |_| {
                                    on_open.call(system.id);
                                },
                                // Hostname column with status dot
                                td {
                                    div {
                                        class: "sys-host-cell",
                                        StatusDot {
                                            color: status_color(&system.health_status).to_string(),
                                            large: false,
                                        }
                                        div {
                                            class: "min-w-0",
                                            div {
                                                class: "hostname",
                                                "{system.hostname}"
                                            }
                                            div {
                                                class: "fqdn truncate",
                                                "{system.hostname}.local"
                                            }
                                        }
                                    }
                                }
                                // Env
                                td {
                                    {
                                        let env = environment_label(&system);

                                        // Debug: log environment value to console
                                        #[cfg(debug_assertions)]
                                        {
                                            use wasm_bindgen::prelude::*;
                                            #[wasm_bindgen]
                                            extern "C" {
                                                #[wasm_bindgen(js_namespace = console)]
                                                fn log(s: &str);
                                            }
                                            log(&format!("SystemsTable: hostname={}, environment='{}' (raw: {:?})",
                                                system.hostname, env, system.environment));
                                        }

                                        let colors = env_colors(&env);
                                        rsx! {
                                            EnvBadge {
                                                name: env.clone(),
                                                fg: colors.fg.to_string(),
                                                bg: colors.bg.to_string(),
                                                border: colors.border.to_string(),
                                            }
                                        }
                                    }
                                }
                                // Status
                                td {
                                    Chip {
                                        variant: health_chip_variant(&system.health_status),
                                        show_dot: true,
                                        "{system.health_status.label()}"
                                    }
                                }
                                // Flake · commit
                                td {
                                    {
                                        // TODO: Backend needs to include flake name and commit in SystemSummary/view_system_list
                                        let flake_display = system.flake_id
                                            .map(|id| format!("flake-{}", id))
                                            .unwrap_or_else(|| "—".to_string());
                                        rsx! {
                                            div {
                                                style: "display: flex; flex-direction: column; line-height: 1.3;",
                                                span {
                                                    class: "mono",
                                                    style: "font-size: 12px; color: var(--cf-text-muted)",
                                                    "{flake_display}"
                                                }
                                            }
                                        }
                                    }
                                }
                                // Deploy
                                td {
                                    Chip {
                                        variant: deployment_chip_variant(&system.deployment_status),
                                        show_dot: false,
                                        "{system.deployment_status.label()}"
                                    }
                                }
                                // CVEs
                                td {
                                    div {
                                        style: "display: flex; gap: 6px; flex-wrap: wrap;",
                                        if system.cve_counts.critical > 0 {
                                            span { class: "chip chip-critical", "{system.cve_counts.critical} crit" }
                                        }
                                        if system.cve_counts.high > 0 {
                                            span { class: "chip chip-warning", "{system.cve_counts.high} high" }
                                        }
                                        if system.cve_counts.critical == 0 && system.cve_counts.high == 0 {
                                            span { class: "chip chip-healthy", "✓ clean" }
                                        }
                                    }
                                }
                                // Heartbeat countdown
                                td {
                                    {
                                        let last_seen_text = system
                                            .last_seen
                                            .map(|dt| {
                                                let diff = chrono::Utc::now().signed_duration_since(dt);
                                                if diff.num_seconds() < 60 {
                                                    format!("{}s ago", diff.num_seconds().max(0))
                                                } else if diff.num_minutes() < 60 {
                                                    format!("{}m ago", diff.num_minutes().max(0))
                                                } else if diff.num_hours() < 24 {
                                                    format!("{}h ago", diff.num_hours().max(0))
                                                } else {
                                                    format!("{}d ago", diff.num_days().max(0))
                                                }
                                            })
                                            .unwrap_or_else(|| "Never".to_string());

                                        let next_in = system
                                            .last_seen
                                            .map(|dt| 60.0 - chrono::Utc::now().signed_duration_since(dt).num_seconds() as f64)
                                            .unwrap_or(0.0);

                                        rsx! {
                                            div {
                                                style: "display: flex; align-items: center; gap: 8px;",
                                                HeartbeatSpinner {
                                                    interval_sec: 60,
                                                    next_in_sec: next_in,
                                                    size: 20,
                                                    show_label: false,
                                                }
                                                span {
                                                    class: "text-xs",
                                                    style: "color: var(--cf-text-secondary)",
                                                    "{last_seen_text}"
                                                }
                                            }
                                        }
                                    }
                                }
                                // Row actions: Deploy | Evaluate | More (matching design)
                                td {
                                    div {
                                        class: "row-actions",
                                        // Deploy
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Deploy",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_deploy.call(system.id);
                                            },
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path { d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" }
                                            }
                                        }
                                        // Evaluate
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Evaluate",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                            },
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path { d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" }
                                            }
                                        }
                                        // More
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "More",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                            },
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "currentColor",
                                                view_box: "0 0 24 24",
                                                circle { cx: "5", cy: "12", r: "2" }
                                                circle { cx: "12", cy: "12", r: "2" }
                                                circle { cx: "19", cy: "12", r: "2" }
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
}

/// Get IP label for a system (or "-" if not set).
fn ip_label(system: &SystemSummary) -> String {
    system.primary_ip.clone().unwrap_or_else(|| "-".to_string())
}

/// Get environment label for a system (or "Unknown" if not set).
fn environment_label(system: &SystemSummary) -> String {
    system
        .environment
        .clone()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Environment color configuration for badges.
struct EnvColors {
    fg: &'static str,
    bg: &'static str,
    border: &'static str,
}

fn env_colors(env_name: &str) -> EnvColors {
    match env_name.to_lowercase().as_str() {
        "production" | "prod" => EnvColors {
            fg: "#f87171",
            bg: "rgba(220,38,38,0.10)",
            border: "rgba(248,113,113,0.25)",
        },
        "staging" | "stage" => EnvColors {
            fg: "#fbbf24",
            bg: "rgba(217,119,6,0.10)",
            border: "rgba(251,191,36,0.25)",
        },
        "dev" | "development" => EnvColors {
            fg: "#60a5fa",
            bg: "rgba(37,99,235,0.10)",
            border: "rgba(96,165,250,0.25)",
        },
        "edge" => EnvColors {
            fg: "#2dd4bf",
            bg: "rgba(15,118,110,0.12)",
            border: "rgba(45,212,191,0.25)",
        },
        "lab" => EnvColors {
            fg: "#a78bfa",
            bg: "rgba(124,58,237,0.10)",
            border: "rgba(167,139,250,0.25)",
        },
        _ => EnvColors {
            fg: "#6b7280",
            bg: "rgba(107,114,128,0.16)",
            border: "rgba(107,114,128,0.25)",
        },
    }
}

fn status_color(health: &HealthStatus) -> &'static str {
    match health {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
    }
}

fn health_chip_variant(health: &HealthStatus) -> ChipVariant {
    match health {
        HealthStatus::Healthy => ChipVariant::Healthy,
        HealthStatus::Warning => ChipVariant::Warning,
        HealthStatus::Critical => ChipVariant::Critical,
        HealthStatus::Offline => ChipVariant::Unknown,
    }
}

fn deployment_chip_variant(status: &DeploymentStatus) -> ChipVariant {
    match status {
        DeploymentStatus::UpToDate => ChipVariant::Healthy,
        DeploymentStatus::Behind => ChipVariant::Warning,
        DeploymentStatus::Ahead => ChipVariant::Info,
        DeploymentStatus::NeverDeployed
        | DeploymentStatus::NoCommitsAvailable
        | DeploymentStatus::Unknown => ChipVariant::Unknown,
    }
}
