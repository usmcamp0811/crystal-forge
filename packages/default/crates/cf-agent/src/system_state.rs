//! Agent-side host inspection and heartbeat DTO construction.

use crate::network::{
    get_gateway_ip, get_network_interfaces, get_primary_ip, get_primary_mac, get_selinux_status,
};
use anyhow::Result;
use cf_protocol::agent::SystemState;
use chrono::Utc;
use std::path::Path;
use std::{fs, io::ErrorKind};
use sysinfo::System;
use tracing::debug;

/// Gather the live host state and populate the wire-level `SystemState` DTO.
pub fn gather_system_state(
    hostname: &str,
    change_reason: &str,
    store_path: &str,
) -> Result<SystemState> {
    let mut system = System::new_all();
    system.refresh_all();

    debug!("reading uptime_secs");
    let uptime_secs = System::uptime();
    debug!("reading os");
    let os = System::os_version();
    debug!("reading kernel");
    let kernel = System::kernel_version();
    debug!("reading memory_gb");
    let memory_gb = Some(system.total_memory() as f64 / 1024.0 / 1024.0);
    debug!("reading cpu_brand");
    let cpu_brand = system.cpus().first().map(|cpu| cpu.brand().to_string());
    let cpu_cores = Some(system.cpus().len() as i32);

    debug!("reading hardware identity");
    let board_serial = read_trimmed("/sys/class/dmi/id/board_serial")?;
    let product_uuid = read_trimmed("/sys/class/dmi/id/product_uuid")?;
    let rootfs_uuid = get_rootfs_uuid();
    let chassis_serial = read_trimmed("/sys/class/dmi/id/chassis_serial")?;
    let bios_version = read_trimmed("/sys/class/dmi/id/bios_version")?;
    let cpu_microcode = read_trimmed("/proc/cpuinfo")
        .ok()
        .flatten()
        .and_then(|contents| {
            contents
                .lines()
                .find(|line| line.contains("microcode"))
                .map(ToOwned::to_owned)
        });

    debug!("reading network identity");
    let network_interfaces = get_network_interfaces()
        .ok()
        .map(serde_json::Value::String);
    let primary_mac_address = get_primary_mac().ok();
    let primary_ip_address = get_primary_ip().ok();
    let gateway_ip = get_gateway_ip().ok();

    debug!("reading security state");
    let selinux_status = get_selinux_status().ok();
    let tpm_present = Some(Path::new("/dev/tpm0").exists());
    let secure_boot_enabled =
        read_trimmed("/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c")
            .ok()
            .map(|value| value == Some("1".to_string()));
    let fips_mode = read_trimmed("/proc/sys/crypto/fips_enabled")
        .ok()
        .map(|value| value == Some("1".to_string()));

    debug!("reading software identity");
    let agent_version = Some(env!("CARGO_PKG_VERSION").to_string());
    let agent_build_hash = option_env!("SRC_HASH").map(ToOwned::to_owned);
    let nixos_version = read_trimmed("/etc/os-release").ok().and_then(|contents| {
        contents?.lines().find_map(|line| {
            line.strip_prefix("VERSION=")
                .map(|version| version.replace('"', ""))
        })
    });

    let (generation, generation_matches_current_store_path) =
        current_system_generation_info(store_path);
    let boot_id = read_trimmed("/proc/sys/kernel/random/boot_id")?;

    Ok(SystemState {
        id: None,
        hostname: hostname.to_string(),
        change_reason: change_reason.to_string(),
        timestamp: Some(Utc::now()),
        store_path: Some(store_path.to_string()),
        generation,
        generation_matches_current_store_path,
        os,
        kernel,
        memory_gb,
        uptime_secs: Some(uptime_secs as i64),
        cpu_brand,
        cpu_cores,
        board_serial,
        product_uuid,
        rootfs_uuid,
        chassis_serial,
        bios_version,
        cpu_microcode,
        network_interfaces,
        primary_mac_address,
        primary_ip_address,
        gateway_ip,
        selinux_status,
        tpm_present,
        secure_boot_enabled,
        fips_mode,
        agent_version,
        agent_build_hash,
        nixos_version,
        agent_compatible: Some(true),
        partial_data: Some(false),
        boot_id,
    })
}

fn current_system_generation_info(current_store_path: &str) -> (Option<i32>, Option<bool>) {
    let profile_link_target = match fs::read_link("/nix/var/nix/profiles/system") {
        Ok(path) => path,
        Err(_) => return (None, None),
    };

    let generation = profile_link_target
        .file_name()
        .and_then(|name| parse_generation_from_profile_link_name(name.to_string_lossy().as_ref()));
    let profile_resolved = fs::canonicalize("/nix/var/nix/profiles/system").ok();
    let current_resolved = fs::canonicalize(current_store_path).ok();
    let matches_current = match (profile_resolved, current_resolved) {
        (Some(profile), Some(current)) => Some(profile == current),
        _ => None,
    };

    (generation, matches_current)
}

fn parse_generation_from_profile_link_name(name: &str) -> Option<i32> {
    name.strip_prefix("system-")?
        .strip_suffix("-link")?
        .parse()
        .ok()
}

fn get_rootfs_uuid() -> Option<String> {
    let output = std::process::Command::new("findmnt")
        .args(["-n", "-o", "SOURCE", "-T", "/"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let source = String::from_utf8_lossy(&output.stdout);
    let device = source.trim().split('[').next()?.trim();
    if device.is_empty() {
        return None;
    }

    let output = if device.starts_with("/dev/") {
        std::process::Command::new("blkid")
            .args(["-s", "UUID", "-o", "value", device])
            .output()
            .ok()?
    } else {
        std::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "guid", device])
            .output()
            .ok()?
    };

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn read_trimmed(path: impl AsRef<Path>) -> std::io::Result<Option<String>> {
    fs::read_to_string(path)
        .map(|contents| Some(contents.trim().to_string()))
        .or_else(|error| {
            if matches!(
                error.kind(),
                ErrorKind::PermissionDenied | ErrorKind::NotFound
            ) {
                Ok(None)
            } else {
                Err(error)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::parse_generation_from_profile_link_name;

    #[test]
    fn parses_generation_profile_links() {
        assert_eq!(
            parse_generation_from_profile_link_name("system-74-link"),
            Some(74)
        );
        assert_eq!(
            parse_generation_from_profile_link_name("system-1-link"),
            Some(1)
        );
    }

    #[test]
    fn rejects_invalid_generation_profile_links() {
        assert_eq!(parse_generation_from_profile_link_name("system-link"), None);
        assert_eq!(
            parse_generation_from_profile_link_name("system-abc-link"),
            None
        );
        assert_eq!(
            parse_generation_from_profile_link_name("/nix/store/foo"),
            None
        );
    }
}
