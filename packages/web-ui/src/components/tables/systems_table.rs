//! Systems table component with sorting.

use std::collections::HashMap;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::models::{
    ConfigurationAssurance, DeploymentStatus, GuestObservationRow, HealthStatus, ManagementMode,
    SystemSummary,
};
use crate::components::chips::{Chip, ChipVariant, EnvBadge, StatusDot};
use crate::components::environments::{normalize_color_hex, with_alpha};
use crate::components::heartbeat_spinner::HeartbeatSpinner;
use crate::components::icon::{Icon, IconName};
use crate::components::system::helpers::deployment_state_label;
use crate::components::system::management_mode::{
    CompactEvidenceCves, DeploymentCell, EvidenceFreshness, MgmtChip, ScanEvidenceChip,
    evidence_status, evidence_status_color,
};
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
    /// Contains bounded configuration reads keyed by resource identity.
    #[props(default)]
    evidence: HashMap<Uuid, Result<ConfigurationAssurance, String>>,
    /// Selected-scan freshness from the real schedule; missing keys are unknown.
    #[props(default)]
    freshness: HashMap<Uuid, EvidenceFreshness>,
    /// Indicates a pending evidence snapshot, independent of operational health.
    #[props(default)]
    evidence_loading: bool,
    /// Retries the owning list's evidence and observation reads.
    #[props(default)]
    on_retry_evidence: Option<EventHandler<()>>,
    /// Permits mutation controls for the current authenticated role.
    #[props(default)]
    can_mutate: bool,
    /// Resolves parent identities without inferring guest health.
    #[props(default)]
    parent_names: HashMap<Uuid, String>,
    /// Contains stored parent reports or read failures, independent of guest health.
    #[props(default)]
    observations: HashMap<Uuid, Result<Option<GuestObservationRow>, String>>,
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
    /// Optional environment name -> color hex mappings from environments API
    #[props(default)]
    environment_colors: Vec<(String, String)>,
    /// Optional flake context tuples (flake_id, name, branch, latest_commit)
    #[props(default)]
    flake_context: Vec<(i32, String, String, Option<String>)>,
    /// Per-system attention class strings keyed by system ID.
    /// Populated by the caller via `attention_row_class()`.
    #[props(default)]
    attention_classes: HashMap<Uuid, String>,
    /// Groups external guests under their managed parent host after sorting.
    #[props(default)]
    group_guests: bool,
    /// Hosts whose grouped guests are hidden.
    #[props(default)]
    collapsed_hosts: std::collections::HashSet<Uuid>,
    /// Shows or hides the grouped guests of one host.
    #[props(default)]
    on_toggle_guests: Option<EventHandler<Uuid>>,
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
    // Sorting orders hosts; grouping then keeps each guest under its host.
    let listed_systems = crate::views::systems_list::systems_list_helpers::group_guests(
        &sorted_systems,
        group_guests,
        &collapsed_hosts,
    );

    let current_col = *sort_column.read();
    let current_dir = *sort_direction.read();
    let table_class = if compact {
        "sys-table sys-table-mg compact"
    } else {
        "sys-table sys-table-mg"
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
                            th { "Heartbeat / runtime" }
                            // Actions — empty header, right-aligned
                            th { style: "text-align: right;", " " }
                        }
                    }
                    tbody {
                        for crate::views::systems_list::systems_list_helpers::ListedSystem { system, guests, indent } in listed_systems {
                            {let managed = system.management_mode == ManagementMode::Managed;
                            let guest_class = if indent { " row-guest" } else { "" };
                            let row_freshness = freshness.get(&system.id).copied().unwrap_or(EvidenceFreshness::Unknown);
                            let guests_collapsed = collapsed_hosts.contains(&system.id);
                            let host_id = system.id;
                            let assurance = evidence.get(&system.id).and_then(|r| r.as_ref().ok());
                            rsx! {
                            tr {
                                class: {
                                    // Compute row class including attention state.
                                    let base = if selected_id == Some(system.id) {
                                        "cursor-pointer selected"
                                    } else {
                                        "cursor-pointer"
                                    };
                                    attention_classes
                                        .get(&system.id)
                                        .filter(|s| !s.is_empty())
                                        .map(|ac| format!("{base} {ac}{guest_class}"))
                                        .unwrap_or_else(|| format!("{base}{guest_class}"))
                                },
                                onclick: move |_| {
                                    on_open.call(system.id);
                                },
                                // Hostname column with status dot
                                td {
                                    div {
                                        class: "sys-host-cell",
                                        StatusDot {
                                            // Nonmanaged: configuration evidence status, never guest health.
                                            color: if managed {
                                                status_color(&system.health_status).to_string()
                                            } else {
                                                assurance
                                                    .map(|value| evidence_status_color(evidence_status(value, row_freshness)))
                                                    .unwrap_or("var(--cf-gray-dot)")
                                                    .to_string()
                                            },
                                            large: false,
                                        }
                                        div {
                                            class: "min-w-0",
                                            div {
                                                class: "hostname",
                                                span { class: "hostname-text", title: "{system.hostname}", "{system.hostname}" }
                                                MgmtChip { mode: system.management_mode }
                                                crate::components::system::system_card_v2::GuestToggle {
                                                    guests,
                                                    collapsed: guests_collapsed,
                                                    on_toggle: on_toggle_guests.map(|handler| {
                                                        EventHandler::new(move |_| handler.call(host_id))
                                                    }),
                                                }
                                            }
                                            // Identity line: FQDN or configuration, then tags. One truncated line
                                            // keeps tagged rows at the same height; the full text is the tooltip.
                                            {
                                                let identity = if managed {
                                                    derived_fqdn(&system)
                                                } else {
                                                    system.system_configuration_name.clone().unwrap_or_else(|| system.hostname.clone())
                                                };
                                                let tags = system.tags.iter().map(|tag| format!("#{tag}")).collect::<Vec<_>>().join(" ");
                                                let line = if tags.is_empty() { identity } else { format!("{identity} · {tags}") };
                                                rsx! { div { class: "fqdn truncate", title: "{line}", "{line}" } }
                                            }
                                        }
                                    }
                                }
                                // Env
                                td {
                                    {
                                        let env = environment_label(&system);
                                        let colors = env_colors(&env, &environment_colors);
                                        rsx! {
                                            EnvBadge {
                                                name: env.clone(),
                                                fg: colors.fg,
                                                bg: colors.bg,
                                                border: colors.border,
                                            }
                                        }
                                    }
                                }
                                // Status
                                td {
                                    if managed {
                                    Chip {
                                        variant: health_chip_variant(&system.health_status),
                                        show_dot: true,
                                        "{system.health_status.label()}"
                                    }
                                    } else if let Some(value) = assurance {
                                        // The current selected-revision state. Older scan counts in the CVE
                                        // column carry their own "historical" marker.
                                        ScanEvidenceChip { status: evidence_status(value, row_freshness) }
                                    } else {
                                        // Raw read errors stay in the tooltip so they cannot expand the row.
                                        span {
                                            class: "chip chip-unknown",
                                            role: "status",
                                            title: match evidence.get(&system.id) {
                                                Some(Err(error)) => format!("Configuration evidence unavailable: {error}"),
                                                _ => String::new(),
                                            },
                                            if evidence_loading { "Loading evidence…" } else { "Evidence unavailable" }
                                        }
                                    }
                                    if evidence.get(&system.id).is_some_and(Result::is_err) || observations.get(&system.id).is_some_and(Result::is_err) {
                                        if let Some(retry) = on_retry_evidence { button { class: "btn btn-ghost xs focus-ring", title: "Retry evidence reads", disabled: evidence_loading, onclick: move |event| { event.stop_propagation(); retry.call(()); }, "Retry" } }
                                    }
                                }
                                // Flake · commit (design sub-line: "{commit} · {branch}")
                                td {
                                    {
                                        let (flake_name, flake_branch, flake_commit) = system
                                            .flake_id
                                            .and_then(|id| {
                                                flake_context
                                                    .iter()
                                                    .find(|(flake_id, ..)| *flake_id == id)
                                                    .map(|(_, name, branch, latest_commit)| {
                                                        (name.clone(), branch.clone(), latest_commit.clone())
                                                    })
                                            })
                                            .unwrap_or_else(|| ("—".to_string(), "—".to_string(), None));
                                        // Managed rows show the flake's latest commit and branch. Nonmanaged
                                        // rows show the exact selected revision and its branch. Both are
                                        // shortened here; the full values are in the tooltip.
                                        let (commit_full, branch) = if managed {
                                            (flake_commit.clone(), flake_branch.clone())
                                        } else {
                                            (
                                                assurance.and_then(|e| e.selected_revision.as_ref()).map(|r| r.sha.clone()),
                                                assurance.and_then(|e| e.branch.clone()).unwrap_or(flake_branch.clone()),
                                            )
                                        };
                                        let commit_short = commit_full
                                            .as_deref()
                                            .map(|hash| hash.chars().take(8).collect::<String>())
                                            .unwrap_or_else(|| if managed { "—".to_string() } else { "no revision".to_string() });
                                        let target = if managed {
                                            flake_name.clone()
                                        } else {
                                            format!("{flake_name}#{}", system.system_configuration_name.as_deref().unwrap_or(&system.hostname))
                                        };
                                        let title = format!("{target}\n{} · {branch}", commit_full.as_deref().unwrap_or(&commit_short));
                                        rsx! {
                                            div { class: "sys-flake-cell", title: "{title}",
                                                span { class: "mono sys-flake-name", "{target}" }
                                                span { class: "mono sys-flake-rev", "{commit_short} · {branch}" }
                                            }
                                        }
                                    }
                                }
                                // Deploy (design lowercase state labels)
                                td {
                                    DeploymentCell { mode: system.management_mode, status: system.deployment_status }
                                }
                                // CVEs (design: crit / high / med-when-comfortable / clean)
                                td {
                                    if managed {
                                    div {
                                        style: "display: flex; gap: 6px; flex-wrap: wrap;",
                                        if system.cve_counts.critical > 0 {
                                            span { class: "chip chip-critical", "{system.cve_counts.critical} crit" }
                                        }
                                        if system.cve_counts.high > 0 {
                                            span { class: "chip chip-warning", "{system.cve_counts.high} high" }
                                        }
                                        if !compact && system.cve_counts.medium > 0 {
                                            span { class: "chip chip-unknown", "{system.cve_counts.medium} med" }
                                        }
                                        if system.cve_counts.critical == 0
                                            && system.cve_counts.high == 0
                                            && (compact || system.cve_counts.medium == 0)
                                        {
                                            span { class: "chip chip-healthy", "✓ clean" }
                                        }
                                    }
                                    } else if let Some(value) = assurance {
                                        CompactEvidenceCves { evidence: value.clone(), freshness: row_freshness }
                                    } else { span { class: "chip chip-unknown", if evidence_loading { "loading…" } else { "unavailable" } } }
                                }
                                // Heartbeat countdown
                                td {
                                    if managed {
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

                                        let hb_interval = system.effective_heartbeat_interval_secs as i64;
                                        let next_in = system
                                            .last_seen
                                            .map(|dt| hb_interval as f64 - chrono::Utc::now().signed_duration_since(dt).num_seconds() as f64)
                                            .unwrap_or(0.0);

                                        rsx! {
                                            div {
                                                style: "display: flex; align-items: center; gap: 8px;",
                                                HeartbeatSpinner {
                                                    interval_sec: hb_interval,
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
                                    } else {
                                        // One compact runtime state, as designed, plus the parent name. The
                                        // reason a state is unknown (no report, read error) is in the tooltip.
                                        {
                                            let external = system.management_mode == ManagementMode::External;
                                            let (line, title) = match observations.get(&system.id) {
                                                _ if !external => ("Not a running machine".to_string(), String::new()),
                                                Some(Ok(Some(report))) => (
                                                    format!(
                                                        "unit {} · {}",
                                                        report.unit_active_state.as_deref().unwrap_or("unknown"),
                                                        crate::components::system::system_card_v2::ago(report.observed_at)
                                                    ),
                                                    format!(
                                                        "Host-observed {} at {}. A host unit observation, not guest health or configuration attestation.",
                                                        report.unit,
                                                        report.observed_at.to_rfc3339()
                                                    ),
                                                ),
                                                Some(Ok(None)) => ("Runtime unknown".into(), "No applicable host observation".into()),
                                                Some(Err(error)) => ("Runtime unknown".into(), format!("Host observation unavailable: {error}")),
                                                None if evidence_loading => ("Loading…".into(), "Loading host observation".into()),
                                                None => ("Runtime unknown".into(), "Host observation unavailable".into()),
                                            };
                                            let observed = matches!(observations.get(&system.id), Some(Ok(Some(_)))) && external;
                                            rsx! {
                                                div { class: "sys-runtime-cell", title: "{title}",
                                                    span { class: if observed { "mg-obs" } else if external { "mg-dim" } else { "mg-dim sys-runtime-wrap" }, "{line}" }
                                                    if let Some(parent) = system.parent_host_id {
                                                        span { class: "mg-dim", "on {parent_names.get(&parent).cloned().unwrap_or_else(|| parent.to_string())}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Row actions: Deploy | Update Key | Edit
                                td {
                                    div {
                                        class: "row-actions",
                                        // Deploy
                                        if !managed || can_mutate { button {
                                            class: "btn-icon focus-ring",
                                            title: if managed { "Deploy" } else { "Open evidence" },
                                            "aria-label": if managed { "Deploy" } else { "Open evidence" },
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_deploy.call(system.id);
                                            },
                                            Icon { name: if managed { IconName::Deploy } else { IconName::Shield }, size: 14 }
                                        } }
                                        // Existing public-key update flow
                                        if managed && can_mutate { button {
                                            class: "btn-icon focus-ring",
                                            title: "Update Key",
                                            "aria-label": "Update Key",
                                            "data-testid": "system-update-key",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_update_key.call(system.id);
                                            },
                                            Icon { name: IconName::Key, size: 14 }
                                        } }
                                        // Edit
                                        if can_mutate { button {
                                            class: "btn-icon focus-ring",
                                            title: "Edit",
                                            "aria-label": "Edit",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                on_edit.call(system.id);
                                            },
                                            Icon { name: IconName::Gear, size: 14 }
                                        } }
                                    }
                                }
                            }
                            }}
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

fn derived_fqdn(system: &SystemSummary) -> String {
    // Prefer the persisted operator-managed FQDN when set.
    if let Some(ref fqdn) = system.fqdn {
        if !fqdn.trim().is_empty() {
            return fqdn.clone();
        }
    }
    let env = system
        .environment
        .as_deref()
        .unwrap_or("unknown")
        .to_lowercase();
    format!("{}.{}.cf.internal", system.hostname, env)
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
    fg: String,
    bg: String,
    border: String,
}

fn env_colors(env_name: &str, environment_colors: &[(String, String)]) -> EnvColors {
    if let Some((_, color_hex)) = environment_colors
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(env_name))
    {
        let fg = normalize_color_hex(color_hex);
        return EnvColors {
            bg: with_alpha(&fg, 0.10),
            border: with_alpha(&fg, 0.25),
            fg,
        };
    }

    match env_name.to_lowercase().as_str() {
        "production" | "prod" => EnvColors {
            fg: "#f87171".to_string(),
            bg: "rgba(220,38,38,0.10)".to_string(),
            border: "rgba(248,113,113,0.25)".to_string(),
        },
        "staging" | "stage" => EnvColors {
            fg: "#fbbf24".to_string(),
            bg: "rgba(217,119,6,0.10)".to_string(),
            border: "rgba(251,191,36,0.25)".to_string(),
        },
        "dev" | "development" => EnvColors {
            fg: "#60a5fa".to_string(),
            bg: "rgba(37,99,235,0.10)".to_string(),
            border: "rgba(96,165,250,0.25)".to_string(),
        },
        "edge" => EnvColors {
            fg: "#2dd4bf".to_string(),
            bg: "rgba(15,118,110,0.12)".to_string(),
            border: "rgba(45,212,191,0.25)".to_string(),
        },
        "lab" => EnvColors {
            fg: "#a78bfa".to_string(),
            bg: "rgba(124,58,237,0.10)".to_string(),
            border: "rgba(167,139,250,0.25)".to_string(),
        },
        _ => EnvColors {
            fg: "#6b7280".to_string(),
            bg: "rgba(107,114,128,0.16)".to_string(),
            border: "rgba(107,114,128,0.25)".to_string(),
        },
    }
}

fn status_color(health: &HealthStatus) -> &'static str {
    match health {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
        HealthStatus::Unknown => "#6b7280",
    }
}

fn health_chip_variant(health: &HealthStatus) -> ChipVariant {
    match health {
        HealthStatus::Healthy => ChipVariant::Healthy,
        HealthStatus::Warning => ChipVariant::Warning,
        HealthStatus::Critical => ChipVariant::Critical,
        HealthStatus::Offline => ChipVariant::Unknown,
        HealthStatus::Unknown => ChipVariant::Unknown,
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
