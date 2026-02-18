//! System detail card components.
//!
//! Provides card components for displaying system information
//! including hardware, network, security, and agent details.

use dioxus::prelude::*;

use crate::api::models::{SystemDetail, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo};
use crate::components::layout::Card;

use super::helpers::{deployment_policy_label, format_memory, format_uptime};
use super::info_row::{BooleanRow, InfoRow, InfoRowMono};

/// System information card displaying hostname, NixOS version, kernel, and deployment policy.
#[component]
pub fn SystemInfoCard(system: SystemDetail) -> Element {
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
                }
            }
        }
    }
}

/// Hardware information card displaying CPU, memory, uptime, and BIOS details.
#[component]
pub fn HardwareCard(hardware: SystemHardwareInfo) -> Element {
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
                    if let Some(uptime) = hardware.uptime_secs {
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

/// Network information card displaying IP, MAC, and gateway.
#[component]
pub fn NetworkCard(network: SystemNetworkInfo) -> Element {
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

/// Security information card displaying TPM, Secure Boot, FIPS, and SELinux status.
#[component]
pub fn SecurityCard(security: SystemSecurityInfo) -> Element {
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

/// Agent information card displaying version, last seen, and active status.
#[component]
pub fn AgentCard(system: SystemDetail) -> Element {
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
