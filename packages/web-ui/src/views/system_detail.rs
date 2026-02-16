//! System detail view — full information for a single NixOS system.

use dioxus::prelude::*;

use crate::api::models::{
    CveSummary, SystemDetail, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo,
};
use crate::components::layout::Card;
use crate::theme;
use crate::views::systems_list::{mock_system_detail_by_id, mock_system_details};

/// The system detail page, reached via `/systems/:id`.
#[component]
pub fn SystemDetailView(id: String) -> Element {
    // TODO: Replace with real API call using use_resource + fetch_system()
    let system = mock_system_detail_by_id(&id).unwrap_or_else(|| fallback_system_detail());

    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let env_style = environment_style(&environment);

    rsx! {
        div {
            class: "space-y-6",

            // Back link
            div {
                Link {
                    to: crate::routes::Route::SystemsView {},
                    class: "inline-flex items-center gap-1 text-sm {theme::text::SECONDARY} hover:text-white transition-colors",
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                    }
                    "Back to Systems"
                }
            }

            // Page header with hostname and environment
            header {
                class: "flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between",
                div {
                    class: "flex items-center gap-4",
                    h1 { class: "{theme::typography::PAGE_TITLE}", "{system.hostname}" }
                    span {
                        class: "inline-flex items-center px-3 py-1 rounded-md text-xs font-semibold uppercase tracking-wide {env_style.chip_bg} {env_style.chip_text}",
                        "{environment}"
                    }
                }
                div {
                    class: "flex flex-wrap items-center gap-2",
                    StatusBadge { label: system.health_status.label(), color_class: system.health_status.color_class(), bg_class: system.health_status.bg_class() }
                    StatusBadge { label: system.deployment_status.label(), color_class: system.deployment_status.color_class(), bg_class: system.deployment_status.bg_class() }
                }
            }

            // Main content grid
            div {
                class: "grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6",

                // System Info card
                SystemInfoCard { system: system.clone() }

                // Hardware card
                HardwareCard { hardware: system.hardware.clone(), uptime_secs: system.hardware.uptime_secs }

                // Network card
                NetworkCard { network: system.network.clone() }

                // Security card
                SecurityCard { security: system.security.clone() }

                // Vulnerabilities card
                VulnerabilitiesCard { cve_counts: system.cve_counts.clone() }

                // Agent card
                AgentCard { system: system.clone() }
            }

            // Store path (full width)
            if let Some(ref store_path) = system.current_store_path {
                Card {
                    title: Some("Current Store Path".to_string()),
                    children: rsx! {
                        code {
                            class: "block text-sm font-mono text-gray-300 bg-gray-800/50 px-4 py-3 rounded-lg overflow-x-auto",
                            "{store_path}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SystemInfoCard(system: SystemDetail) -> Element {
    rsx! {
        Card {
            title: Some("System".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    InfoRow { label: "Hostname", value: system.hostname.clone() }
                    if let Some(ref nixos_version) = system.nixos_version {
                        InfoRow { label: "NixOS Version", value: nixos_version.clone() }
                    }
                    if let Some(ref kernel) = system.kernel {
                        InfoRow { label: "Kernel", value: kernel.clone() }
                    }
                    InfoRow { label: "Deployment Policy", value: deployment_policy_label(&system.deployment_policy) }
                    if let Some(ref flake) = system.flake {
                        InfoRow { label: "Flake", value: flake.name.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn HardwareCard(hardware: SystemHardwareInfo, uptime_secs: Option<i64>) -> Element {
    rsx! {
        Card {
            title: Some("Hardware".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(ref cpu) = hardware.cpu_brand {
                        InfoRow { label: "CPU", value: cpu.clone() }
                    }
                    if let Some(cores) = hardware.cpu_cores {
                        InfoRow { label: "CPU Cores", value: cores.to_string() }
                    }
                    if let Some(mem) = hardware.memory_gb {
                        InfoRow { label: "Memory", value: format_memory(mem) }
                    }
                    if let Some(uptime) = uptime_secs {
                        InfoRow { label: "Uptime", value: format_uptime(uptime) }
                    }
                    if let Some(ref bios) = hardware.bios_version {
                        InfoRow { label: "BIOS Version", value: bios.clone() }
                    }
                    if let Some(ref serial) = hardware.board_serial {
                        InfoRow { label: "Board Serial", value: serial.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn NetworkCard(network: SystemNetworkInfo) -> Element {
    rsx! {
        Card {
            title: Some("Network".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(ref ip) = network.primary_ip {
                        InfoRowMono { label: "Primary IP", value: ip.clone() }
                    }
                    if let Some(ref mac) = network.primary_mac {
                        InfoRowMono { label: "MAC Address", value: mac.clone() }
                    }
                    if let Some(ref gateway) = network.gateway_ip {
                        InfoRowMono { label: "Gateway", value: gateway.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn SecurityCard(security: SystemSecurityInfo) -> Element {
    rsx! {
        Card {
            title: Some("Security".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(tpm) = security.tpm_present {
                        BooleanRow { label: "TPM Present", value: tpm }
                    }
                    if let Some(sb) = security.secure_boot_enabled {
                        BooleanRow { label: "Secure Boot", value: sb }
                    }
                    if let Some(fips) = security.fips_mode {
                        BooleanRow { label: "FIPS Mode", value: fips }
                    }
                    if let Some(ref selinux) = security.selinux_status {
                        InfoRow { label: "SELinux", value: selinux.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn VulnerabilitiesCard(cve_counts: CveSummary) -> Element {
    let total = cve_counts.total();
    rsx! {
        Card {
            title: Some("Vulnerabilities".to_string()),
            children: rsx! {
                div {
                    class: "space-y-4",
                    // Summary line
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "{total} known vulnerabilities"
                    }
                    // Severity breakdown
                    div {
                        class: "space-y-2",
                        CveBar { label: "Critical", count: cve_counts.critical, color: "bg-red-500" }
                        CveBar { label: "High", count: cve_counts.high, color: "bg-orange-500" }
                        CveBar { label: "Medium", count: cve_counts.medium, color: "bg-yellow-500" }
                        CveBar { label: "Low", count: cve_counts.low, color: "bg-blue-500" }
                    }
                }
            }
        }
    }
}

#[component]
fn AgentCard(system: SystemDetail) -> Element {
    rsx! {
        Card {
            title: Some("Agent".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(ref version) = system.agent_version {
                        InfoRow { label: "Version", value: version.clone() }
                    }
                    if let Some(ref last_seen) = system.last_seen {
                        InfoRow { label: "Last Seen", value: last_seen.format("%Y-%m-%d %H:%M:%S UTC").to_string() }
                    }
                    BooleanRow { label: "Active", value: system.is_active }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn StatusBadge(label: &'static str, color_class: &'static str, bg_class: &'static str) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium {color_class} {bg_class}",
            "{label}"
        }
    }
}

#[component]
fn InfoRow(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm text-gray-200", "{value}" }
        }
    }
}

#[component]
fn InfoRowMono(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm text-gray-200 font-mono", "{value}" }
        }
    }
}

#[component]
fn BooleanRow(label: &'static str, value: bool) -> Element {
    let (icon, color, text) = if value {
        ("✓", "text-emerald-400", "Enabled")
    } else {
        ("✗", "text-gray-500", "Disabled")
    };
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm font-medium {color}", "{icon} {text}" }
        }
    }
}

#[component]
fn CveBar(label: &'static str, count: i64, color: &'static str) -> Element {
    let text_color = match label {
        "Critical" => theme::cve::CRITICAL_TEXT,
        "High" => theme::cve::HIGH_TEXT,
        "Medium" => theme::cve::MEDIUM_TEXT,
        "Low" => theme::cve::LOW_TEXT,
        _ => "text-gray-400",
    };
    rsx! {
        div {
            class: "flex items-center gap-3",
            span { class: "w-16 text-xs {text_color}", "{label}" }
            div {
                class: "flex-1 h-2 bg-gray-800 rounded-full overflow-hidden",
                div {
                    class: "h-full {color}",
                    style: "width: {bar_width(count)}%",
                }
            }
            span { class: "w-8 text-right text-xs font-semibold {text_color}", "{count}" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn bar_width(count: i64) -> i64 {
    // Scale bar width - max width at 20 CVEs
    std::cmp::min(count * 5, 100)
}

fn format_memory(gb: f64) -> String {
    if gb >= 1000.0 {
        format!("{:.0} GB", gb / 1000.0)
    } else {
        format!("{:.1} GB", gb)
    }
}

fn format_uptime(seconds: i64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn deployment_policy_label(policy: &str) -> String {
    match policy {
        "Immediate" => "Auto-deploy: Immediate".to_string(),
        "Boot Only" => "Auto-deploy: On reboot".to_string(),
        _ => policy.to_string(),
    }
}

struct EnvStyle {
    chip_bg: &'static str,
    chip_text: &'static str,
}

fn environment_style(environment: &str) -> EnvStyle {
    match environment.to_lowercase().as_str() {
        "production" => EnvStyle {
            chip_bg: "bg-emerald-500/20",
            chip_text: "text-emerald-300",
        },
        "staging" => EnvStyle {
            chip_bg: "bg-amber-500/20",
            chip_text: "text-amber-300",
        },
        "development" => EnvStyle {
            chip_bg: "bg-blue-500/20",
            chip_text: "text-blue-300",
        },
        _ => EnvStyle {
            chip_bg: "bg-gray-500/20",
            chip_text: "text-gray-300",
        },
    }
}

fn fallback_system_detail() -> SystemDetail {
    mock_system_details()
        .into_iter()
        .next()
        .unwrap_or_else(|| SystemDetail {
            id: uuid::Uuid::new_v4(),
            hostname: "unknown".to_string(),
            environment: None,
            is_active: false,
            deployment_policy: "Unknown".to_string(),
            health_status: crate::api::models::HealthStatus::Offline,
            deployment_status: crate::api::models::DeploymentStatus::Unknown,
            pipeline_stage: None,
            nixos_version: None,
            kernel: None,
            agent_version: None,
            current_store_path: None,
            hardware: SystemHardwareInfo {
                cpu_brand: None,
                cpu_cores: None,
                memory_gb: None,
                uptime_secs: None,
                board_serial: None,
                bios_version: None,
            },
            network: SystemNetworkInfo {
                primary_ip: None,
                primary_mac: None,
                gateway_ip: None,
            },
            security: SystemSecurityInfo {
                tpm_present: None,
                secure_boot_enabled: None,
                fips_mode: None,
                selinux_status: None,
            },
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            flake: None,
            last_seen: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
}

