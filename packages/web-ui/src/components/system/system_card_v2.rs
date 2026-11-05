//! Redesigned system card component matching the design system.
//!
//! Implements the refined card layout from the design example with:
//! - Status rail indicator
//! - Environment badges with custom colors
//! - Two-column metadata layout
//! - Chip-based status indicators
//! - Enhanced hover states

use dioxus::prelude::*;

use crate::api::models::{
    ConfigurationAssurance, GuestObservationRow, HealthStatus, ManagementMode, SystemSummary,
};
use crate::components::chips::{Chip, ChipVariant, EnvBadge, StatusDot};
use crate::components::environments::{normalize_color_hex, with_alpha};
use crate::components::heartbeat_spinner::HeartbeatSpinner;
use crate::components::icon::{Icon, IconName};
use crate::components::system::helpers::deployment_state_label;
use crate::components::system::management_mode::{
    DeploymentCell, EvidenceCves, EvidenceFreshness, MgmtChip, ScanEvidenceChip, evidence_status,
};

/// Formats an elapsed time as the compact relative label used on cards and
/// the Edit pinned-commit list.
pub(crate) fn ago(at: chrono::DateTime<chrono::Utc>) -> String {
    let diff = chrono::Utc::now().signed_duration_since(at);
    if diff.num_seconds() < 60 {
        format!("{}s ago", diff.num_seconds().max(0))
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes().max(0))
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours().max(0))
    } else {
        format!("{}d ago", diff.num_days().max(0))
    }
}

/// Environment color configuration for badges and styling.
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

fn deployment_chip_variant(status: &crate::api::models::DeploymentStatus) -> ChipVariant {
    use crate::api::models::DeploymentStatus;
    match status {
        DeploymentStatus::UpToDate => ChipVariant::Healthy,
        DeploymentStatus::Behind => ChipVariant::Warning,
        DeploymentStatus::Ahead => ChipVariant::Info,
        DeploymentStatus::NeverDeployed | DeploymentStatus::NoCommitsAvailable => {
            ChipVariant::Unknown
        }
        DeploymentStatus::Unknown => ChipVariant::Unknown,
    }
}

fn derived_fqdn(hostname: &str, environment: &str) -> String {
    format!("{}.{}.cf.internal", hostname, environment.to_lowercase())
}

/// Redesigned system card with modern styling and layout.
#[component]
pub fn SystemCardV2(
    system: SystemSummary,
    /// Exact configuration evidence, absent while loading or unavailable.
    #[props(default)]
    evidence: Option<ConfigurationAssurance>,
    /// Selected-scan freshness computed by the owning list from the real
    /// schedule; absent when the schedule or evidence is unavailable.
    #[props(default)]
    freshness: Option<EvidenceFreshness>,
    /// Indicates that the bounded list evidence read is still pending.
    #[props(default)]
    evidence_loading: bool,
    /// Contains the real read failure when evidence is unavailable.
    #[props(default)]
    evidence_error: Option<String>,
    /// Contains the associated parent identity, without a guest health claim.
    #[props(default)]
    parent_label: Option<String>,
    /// Contains a stored parent report, never verified guest health.
    #[props(default)]
    observation: Option<GuestObservationRow>,
    /// Keeps a failed latest-report read distinct from an absent observation.
    #[props(default)]
    observation_error: Option<String>,
    /// Retries the owning list's bounded evidence/observation reads.
    #[props(default)]
    on_retry_evidence: Option<EventHandler<()>>,
    /// Permits mutation controls only for an authorized operator or administrator.
    #[props(default)]
    can_mutate: bool,
    #[props(default = false)] compact: bool,
    #[props(default = false)] selected: bool,
    #[props(default)] environment_colors: Vec<(String, String)>,
    #[props(default)] flake_context: Vec<(i32, String, String, Option<String>)>,
    /// Attention-row class (e.g. "attention-row attention-flash") for alerting state.
    #[props(default)]
    attention_class: String,
    /// Set to true to show the attention-flash pulse animation on this card.
    #[props(default = false)]
    flash: bool,
    /// Number of external guests grouped under this managed host. Zero hides
    /// the guest toggle.
    #[props(default)]
    guests: usize,
    /// True when this host's grouped guests are hidden.
    #[props(default)]
    guests_collapsed: bool,
    /// Shows or hides this host's grouped guests.
    #[props(default)]
    on_toggle_guests: Option<EventHandler<()>>,
    /// True when this card is an external guest shown under its parent host.
    #[props(default)]
    indent: bool,
    on_open: EventHandler<()>,
    on_remove: EventHandler<()>,
    on_update_key: EventHandler<()>,
    on_deploy: EventHandler<()>,
) -> Element {
    if system.management_mode != ManagementMode::Managed {
        let freshness = freshness.unwrap_or(EvidenceFreshness::Unknown);
        let mode = system.management_mode;
        let selected_class = match (selected, indent) {
            (true, true) => "selected sys-card-guest",
            (true, false) => "selected",
            (false, true) => "sys-card-guest",
            (false, false) => "",
        };
        let config = system
            .system_configuration_name
            .as_deref()
            .unwrap_or(&system.hostname);
        let flake = system
            .flake_id
            .and_then(|id| flake_context.iter().find(|(fid, ..)| *fid == id))
            .map(|(_, name, ..)| name.as_str())
            .unwrap_or("Not linked");
        let environment = system.environment.as_deref().unwrap_or("unknown");
        let env = env_colors(environment, &environment_colors);
        let subtitle = match (mode, parent_label.as_deref()) {
            (ManagementMode::External, Some(parent)) => format!("guest of {parent}"),
            (ManagementMode::External, None) => "externally administered".to_string(),
            _ => format!("{flake}#{config}"),
        };
        let revision = evidence
            .as_ref()
            .and_then(|e| {
                let rev = e.selected_revision.as_ref()?;
                let sha = rev.sha.get(..7).unwrap_or(&rev.sha);
                Some(match e.branch.as_deref() {
                    Some(branch) => format!("{sha} · {branch}"),
                    None => sha.to_string(),
                })
            })
            .unwrap_or_else(|| {
                if evidence_loading {
                    "Loading…".into()
                } else {
                    "Unavailable".into()
                }
            });
        // The most recent terminal scan time, which may predate the selected
        // revision; freshness and revision mismatch are shown by the scan chip.
        let scanned = match &evidence {
            Some(e) => e
                .latest_terminal_scan
                .as_ref()
                .or(e.selected_scan.as_ref())
                .and_then(|s| s.completed_at)
                .map(|at| format!("scanned {}", ago(at)))
                .unwrap_or_else(|| "not scanned".into()),
            None if evidence_loading => "Loading evidence…".into(),
            None => "Evidence unavailable".into(),
        };
        let evidence_color = match evidence
            .as_ref()
            .map(|e| evidence_status(e, freshness).chip_class())
        {
            Some("chip-critical") => "var(--cf-red)",
            Some("chip-warning") => "var(--cf-amber)",
            Some("chip-info") => "var(--cf-blue)",
            Some("chip-healthy") => "var(--cf-emerald)",
            _ => "var(--cf-gray-dot)",
        };
        return rsx! {
            div { class: "sys-card mg-list-card {selected_class}", onclick: move |_| on_open.call(()),
                span { class: "status-rail", style: "--status-color: {evidence_color}" }
                div { class: "sys-card-head",
                    div { class: "sys-title", div { class: "sys-hostname", "{system.hostname}" }
                        div { class: "sys-fqdn", "{subtitle}" }
                    }
                    div { class: "chips-row", MgmtChip { mode }
                        for tag in &system.tags { span { class: "chip chip-unknown", "#{tag}" } }
                        EnvBadge { name: environment.to_string(), fg: env.fg, bg: env.bg, border: env.border }
                    }
                }
                if !compact {
                    div { class: "sys-card-body",
                        div { div { class: "sys-kv-key", "Configuration" } div { class: "sys-kv-val", "{flake}#{config}" } }
                        div { div { class: "sys-kv-key", "Revision" } div { class: "sys-kv-val", "{revision}" } }
                        div { div { class: "sys-kv-key", "Evidence" }
                            div { class: "sys-kv-val", style: "font-family: var(--font-sans);", title: evidence_error.clone().unwrap_or_default(), "{scanned}" }
                        }
                        div { div { class: "sys-kv-key", "Runtime" }
                            div { class: "sys-kv-val", style: "font-family: var(--font-sans);",
                                if mode == ManagementMode::Reference { span { class: "mg-dim", "Not a running machine" } }
                                else if let Some(report) = observation {
                                    // SECURITY: a parent-reported unit state is neither guest
                                    // health nor proof that the scanned configuration runs.
                                    span { class: "mg-obs", title: "Parent-reported unit state; not guest health or configuration attestation",
                                        span { class: "chip-dot", style: if report.unit_active_state.as_deref() == Some("active") { "background:#60a5fa" } else { "background:#fbbf24" } }
                                        {format!("host-observed · unit {} · {}", report.unit_active_state.as_deref().unwrap_or("unknown"), ago(report.observed_at))}
                                    }
                                } else if observation_error.is_some() { span { class: "mg-dim", "Host observation unavailable" } }
                                else if evidence_loading { span { class: "mg-dim", "Loading host observation…" } }
                                else { span { class: "mg-dim", "Runtime unknown" } }
                            }
                        }
                    }
                }
                div { class: "sys-card-foot",
                    div { class: "chips-row",
                        if let Some(value) = &evidence { ScanEvidenceChip { status: evidence_status(value, freshness) } }
                        DeploymentCell { mode, status: system.deployment_status }
                        if let Some(value) = evidence { EvidenceCves { evidence: value, freshness } }
                        else { span { class: "chip chip-unknown", role: "status", if evidence_loading { "Loading evidence…" } else { "Evidence unavailable" } } }
                    }
                    button { class: "btn btn-subtle focus-ring", style: "padding: 4px 10px; font-size: 12px;", title: "Open configuration evidence",
                        onclick: move |e| { e.stop_propagation(); on_deploy.call(()); },
                        Icon { name: IconName::Shield, size: 12 } " Evidence"
                    }
                }
                if let Some(error) = &evidence_error { p { class: "mg-dim", role: "status", "Configuration evidence unavailable: {error}" } }
                if let Some(error) = &observation_error { p { class: "mg-dim", role: "status", "Host observation unavailable: {error}" } }
                if evidence_error.is_some() || observation_error.is_some() {
                    if let Some(retry) = on_retry_evidence {
                        button { class: "btn btn-ghost xs focus-ring", disabled: evidence_loading,
                            onclick: move |event| { event.stop_propagation(); retry.call(()); }, "Retry evidence reads" }
                    }
                }
            }
        };
    }
    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let env = env_colors(&environment, &environment_colors);
    let status_col = status_color(&system.health_status);

    // Get flake info from loaded flake context.
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
    let flake_with_branch = format!("{flake_name} · {flake_branch}");
    let flake_commit_short = flake_commit
        .as_deref()
        .map(|hash| hash.chars().take(8).collect::<String>())
        .unwrap_or_else(|| "—".to_string());

    // Format last seen as relative time
    let last_seen = system
        .last_seen
        .map(ago)
        .unwrap_or_else(|| "Never".to_string());

    let heartbeat_interval_sec = system.effective_heartbeat_interval_secs as i64;
    let heartbeat_next_in_sec = system
        .last_seen
        .map(|dt| {
            let elapsed = chrono::Utc::now().signed_duration_since(dt).num_seconds() as f64;
            heartbeat_interval_sec as f64 - elapsed
        })
        .unwrap_or(0.0);

    let compact_class = if compact { " compact" } else { "" };
    let selected_class = if selected { " selected" } else { "" };

    // Compose the final class string: base + selected + attention + flash.
    let card_class = if attention_class.is_empty() {
        if flash {
            format!("sys-card{compact_class}{selected_class} attention-flash")
        } else {
            format!("sys-card{compact_class}{selected_class}")
        }
    } else {
        format!("sys-card{compact_class}{selected_class} {attention_class}")
    };

    rsx! {
        div {
            class: "{card_class}",
            onclick: move |_| {
                on_open.call(());
            },

            // Status rail (colored left edge)
            span {
                class: "status-rail",
                style: "--status-color: {status_col}"
            }

            // Card head with hostname and environment badge
            div {
                class: "sys-card-head",
                div {
                    class: "sys-title",
                    div {
                        class: "sys-hostname",
                        StatusDot {
                            color: status_col.to_string(),
                            large: false,
                        }
                        "{system.hostname}"
                    }
                    div {
                        class: "sys-fqdn",
                        {
                            // Prefer persisted operator-managed FQDN when set.
                            let display_fqdn = system.fqdn.clone()
                                .filter(|v| !v.trim().is_empty())
                                .unwrap_or_else(|| derived_fqdn(&system.hostname, &environment));
                            rsx! { "{display_fqdn}" }
                        }
                    }
                }
                EnvBadge {
                    name: environment.clone(),
                    fg: env.fg,
                    bg: env.bg,
                    border: env.border,
                }
                MgmtChip { mode: system.management_mode }
                for tag in &system.tags { span { class: "chip chip-unknown", "#{tag}" } }
            }

            // Card body with two-column metadata
            if !compact {
                div {
                    class: "sys-card-body",
                    // Flake · branch
                    div {
                        div { class: "sys-kv-key", "Flake · branch" }
                        div { class: "sys-kv-val", "{flake_with_branch}" }
                    }
                    // Commit
                    div {
                        div { class: "sys-kv-key", "Commit" }
                        div { class: "sys-kv-val", "{flake_commit_short}" }
                    }
                    // Heartbeat
                    div {
                        div { class: "sys-kv-key", "Heartbeat" }
                        div {
                            class: "sys-kv-val",
                            style: "font-family: var(--font-sans); display: flex; align-items: center; gap: 8px;",
                            HeartbeatSpinner {
                                interval_sec: heartbeat_interval_sec,
                                next_in_sec: heartbeat_next_in_sec,
                                size: 22,
                                show_label: false,
                            }
                            span { "{last_seen}" }
                        }
                    }
                    // Policy
                    div {
                        div { class: "sys-kv-key", "Policy" }
                        div {
                            class: "sys-kv-val",
                            style: "font-family: var(--font-sans);",
                            "{system.deployment_policy}"
                        }
                    }
                }
            }

            // Compact body: condensed inline metadata
            if compact {
                div {
                    style: "display: flex; gap: 12px; font-size: 12px; color: var(--cf-text-secondary); flex-wrap: wrap;",
                    span {
                        class: "mono",
                        style: "color: var(--cf-text-primary);",
                        "{flake_name}"
                    }
                    span { class: "mono", "{flake_commit_short} · {flake_branch}" }
                    span { "{last_seen}" }
                }
            }

            // Card footer with status chips and deploy button
            div {
                class: "sys-card-foot",
                div {
                    class: "chips-row",
                    GuestToggle { guests, collapsed: guests_collapsed, on_toggle: on_toggle_guests }
                    // Health chip
                    Chip {
                        variant: health_chip_variant(&system.health_status),
                        show_dot: true,
                        "{system.health_status.label()}"
                    }
                    // Deployment chip (design lowercase state labels)
                    Chip {
                        variant: deployment_chip_variant(&system.deployment_status),
                        show_dot: false,
                        "{deployment_state_label(&system.deployment_status)}"
                    }
                    // CVE chips: crit/high when present, otherwise a clean chip
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
                        span { class: "chip chip-healthy", "✓ clean" }
                    }
                }
                div {
                    style: "display: flex; gap: 6px;",
                    if can_mutate { button {
                        class: "btn btn-subtle focus-ring",
                        "data-testid": "system-update-key",
                        style: "padding: 4px 10px; font-size: 12px;",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_update_key.call(());
                        },
                        "Update Key"
                    } }
                    if can_mutate { button {
                        class: "btn btn-subtle focus-ring",
                        style: "padding: 4px 10px; font-size: 12px;",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_deploy.call(());
                        },
                        svg {
                            class: "w-3 h-3",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" }
                        }
                        " Deploy"
                    } }
                }
            }
        }
    }
}

/// Shows the number of external guests grouped under a managed host and
/// toggles their visibility. Renders nothing when `guests` is zero.
///
/// The click does not propagate, so it does not open the host's preview.
#[component]
pub fn GuestToggle(
    guests: usize,
    collapsed: bool,
    on_toggle: Option<EventHandler<()>>,
) -> Element {
    if guests == 0 {
        return rsx! {};
    }
    let plural = if guests > 1 { "s" } else { "" };
    let title = if collapsed { "Show external guests" } else { "Hide external guests" };
    let rotation = if collapsed { "transform: rotate(-90deg); display: inline-flex;" } else { "display: inline-flex;" };
    rsx! {
        button {
            r#type: "button",
            class: "chip chip-unknown guest-toggle focus-ring",
            "aria-expanded": "{!collapsed}",
            title: "{title}",
            onclick: move |evt| {
                evt.stop_propagation();
                if let Some(handler) = on_toggle {
                    handler.call(());
                }
            },
            span { style: "{rotation}", Icon { name: IconName::ChevronDown, size: 10 } }
            "{guests} guest{plural}"
        }
    }
}
