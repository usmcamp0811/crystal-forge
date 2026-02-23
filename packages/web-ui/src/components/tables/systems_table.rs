//! Systems table component with sorting.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::models::SystemSummary;
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
            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm bg-gray-900/60",
            div {
                class: "overflow-x-auto",
                "data-testid": "systems-table",
                table {
                    class: "w-full",
                    thead {
                        class: "{theme::surface::SUBTLE_BG}",
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
                            th { class: "px-4 py-3 text-right text-xs font-medium text-gray-400 uppercase tracking-wider", "Actions" }
                        }
                    }
                    tbody {
                        class: "divide-y {theme::surface::DIVIDER}",
                        for system in sorted_systems {
                            tr {
                                class: "hover:bg-gray-800/40 transition cursor-pointer",
                                onclick: move |_| {
                                    navigator.push(Route::SystemDetailView { id: system.id.to_string() });
                                },
                                td { class: "{theme::spacing::TABLE_CELL} text-sm text-white", "{system.hostname}" }
                                td {
                                    class: "{theme::spacing::TABLE_CELL} text-sm text-gray-300 font-mono",
                                    "{ip_label(&system)}"
                                }
                                td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{environment_label(&system)}" }
                                td { class: "{theme::spacing::TABLE_CELL}",
                                    span { class: "text-xs {system.health_status.color_class()}", "{system.health_status.label()}" }
                                }
                                td { class: "{theme::spacing::TABLE_CELL}",
                                    span { class: "text-xs {system.deployment_status.color_class()}", "{system.deployment_status.label()}" }
                                }
                                td { class: "{theme::spacing::TABLE_CELL} text-xs",
                                    span { class: "{theme::cve::CRITICAL_TEXT} font-semibold", "{system.cve_counts.critical}" }
                                    span { class: "text-gray-500", " C  " }
                                    span { class: "{theme::cve::HIGH_TEXT} font-semibold", "{system.cve_counts.high}" }
                                    span { class: "text-gray-500", " H  " }
                                    span { class: "{theme::cve::MEDIUM_TEXT} font-semibold", "{system.cve_counts.medium}" }
                                    span { class: "text-gray-500", " M  " }
                                    span { class: "{theme::cve::LOW_TEXT} font-semibold", "{system.cve_counts.low}" }
                                    span { class: "text-gray-500", " L" }
                                }
                                td {
                                    class: "{theme::spacing::TABLE_CELL} text-right",
                                    button {
                                        class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            on_remove.call(system.id);
                                        },
                                        "Remove"
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
        .unwrap_or_else(|| "Unknown".to_string())
}
