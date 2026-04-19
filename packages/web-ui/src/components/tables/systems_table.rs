//! Systems table component with sorting.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::models::{HealthStatus, SystemSummary};
use crate::components::chips::{Chip, ChipVariant, EnvBadge, StatusDot};
use crate::components::tables::{SortDirection, SortableHeader};
use crate::routes::Route;
use crate::theme;

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
) -> Element {
    let navigator = use_navigator();
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

    rsx! {
        div {
            class: "card",
            style: "overflow: hidden;",
            div {
                class: "overflow-x-auto",
                "data-testid": "systems-table",
                table {
                    class: "sys-table",
                    thead {
                        tr {
                            SortableHeader {
                                label: "Hostname",
                                column: SystemsSortColumn::Hostname,
                                current_col: current_col,
                                current_dir: current_dir,
                                on_sort: move |(col, dir)| {
                                    sort_column.set(Some(col));
                                    sort_direction.set(dir);
                                }
                            }
                            SortableHeader {
                                label: "IP",
                                column: SystemsSortColumn::Ip,
                                current_col: current_col,
                                current_dir: current_dir,
                                on_sort: move |(col, dir)| {
                                    sort_column.set(Some(col));
                                    sort_direction.set(dir);
                                }
                            }
                            SortableHeader {
                                label: "Environment",
                                column: SystemsSortColumn::Environment,
                                current_col: current_col,
                                current_dir: current_dir,
                                on_sort: move |(col, dir)| {
                                    sort_column.set(Some(col));
                                    sort_direction.set(dir);
                                }
                            }
                            SortableHeader {
                                label: "Health",
                                column: SystemsSortColumn::Health,
                                current_col: current_col,
                                current_dir: current_dir,
                                on_sort: move |(col, dir)| {
                                    sort_column.set(Some(col));
                                    sort_direction.set(dir);
                                }
                            }
                            SortableHeader {
                                label: "Deployment",
                                column: SystemsSortColumn::Deployment,
                                current_col: current_col,
                                current_dir: current_dir,
                                on_sort: move |(col, dir)| {
                                    sort_column.set(Some(col));
                                    sort_direction.set(dir);
                                }
                            }
                            SortableHeader {
                                label: "CVEs",
                                column: SystemsSortColumn::Cves,
                                current_col: current_col,
                                current_dir: current_dir,
                                on_sort: move |(col, dir)| {
                                    sort_column.set(Some(col));
                                    sort_direction.set(dir);
                                }
                            }
                            th {
                                class: "text-left px-4 py-3 text-xs font-medium uppercase tracking-wider",
                                style: "color: var(--cf-text-muted); background: var(--cf-subtle-bg); letter-spacing: 0.08em;",
                                "Actions"
                            }
                        }
                    }
                    tbody {
                        for system in sorted_systems {
                            tr {
                                class: "cursor-pointer",
                                onclick: move |_| {
                                    navigator.push(Route::SystemDetailView { id: system.id.to_string() });
                                },
                                // Hostname column with status dot
                                td {
                                    div {
                                        class: "flex items-center gap-3",
                                        StatusDot {
                                            color: status_color(&system.health_status).to_string(),
                                            large: false,
                                        }
                                        div {
                                            class: "min-w-0",
                                            div {
                                                class: "font-semibold",
                                                style: "color: var(--cf-text-primary)",
                                                "{system.hostname}"
                                            }
                                            div {
                                                class: "text-xs mono truncate",
                                                style: "color: var(--cf-text-muted)",
                                                "{system.hostname}.local"
                                            }
                                        }
                                    }
                                }
                                // Environment badge
                                td {
                                    {
                                        let env = environment_label(&system);
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
                                // Health status chip
                                td {
                                    Chip {
                                        variant: health_chip_variant(&system.health_status),
                                        show_dot: true,
                                        "{system.health_status.label()}"
                                    }
                                }
                                // Deployment status
                                td {
                                    span {
                                        class: "text-xs",
                                        style: "color: var(--cf-text-secondary)",
                                        "{system.deployment_status.label()}"
                                    }
                                }
                                // CVEs with chips
                                td {
                                    div {
                                        class: "flex gap-2 flex-wrap",
                                        if system.cve_counts.critical > 0 {
                                            Chip {
                                                variant: ChipVariant::Critical,
                                                show_dot: false,
                                                "{system.cve_counts.critical} crit"
                                            }
                                        }
                                        if system.cve_counts.high > 0 {
                                            Chip {
                                                variant: ChipVariant::Warning,
                                                show_dot: false,
                                                "{system.cve_counts.high} high"
                                            }
                                        }
                                        if system.cve_counts.critical == 0 && system.cve_counts.high == 0 {
                                            span {
                                                class: "text-xs",
                                                style: "color: var(--cf-text-muted)",
                                                "{system.cve_counts.medium + system.cve_counts.low} total"
                                            }
                                        }
                                    }
                                }
                                td {
                                    div {
                                        class: "row-actions",
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
                                                path { d: "M5 12h14M12 5l7 7-7 7" }
                                            }
                                        }
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Edit",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_edit.call(system.id);
                                            },
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path { d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" }
                                            }
                                        }
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "More",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                            },
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                circle { cx: "12", cy: "12", r: "1" }
                                                circle { cx: "19", cy: "12", r: "1" }
                                                circle { cx: "5", cy: "12", r: "1" }
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
