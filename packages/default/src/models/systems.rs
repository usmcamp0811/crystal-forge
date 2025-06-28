use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;
use std::option::Option;
use std::{fs, io::ErrorKind, path::Path, process::Command};
use sysinfo::System;
use tracing::debug;

// Import these from your network_interfaces.rs
use crate::models::network_interfaces::{
    get_gateway_ip, get_network_interfaces, get_primary_ip, get_primary_mac, get_selinux_status,
};

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct SystemState {
    // ───── Identification ─────
    pub id: Option<i32>,
    pub hostname: String,
    pub context: String,
    pub timestamp: Option<DateTime<Utc>>,

    // ───── System Info ─────
    pub derivation_path: Option<String>,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,

    // ───── Hardware IDs ─────
    pub board_serial: Option<String>,
    pub product_uuid: Option<String>,
    pub rootfs_uuid: Option<String>,
    pub chassis_serial: Option<String>,
    pub bios_version: Option<String>,
    pub cpu_microcode: Option<String>,

    // ───── Network Identity ─────
    pub network_interfaces: Option<serde_json::Value>,
    pub primary_mac_address: Option<String>,
    pub primary_ip_address: Option<String>,
    pub gateway_ip: Option<String>,

    // ───── Security & Compliance ─────
    pub selinux_status: Option<String>,
    pub tpm_present: Option<bool>,
    pub secure_boot_enabled: Option<bool>,
    pub fips_mode: Option<bool>,

    // ───── Software Identity ─────
    pub agent_version: Option<String>,
    pub agent_build_hash: Option<String>,
    pub nixos_version: Option<String>,
}

impl SystemState {
    pub fn gather(hostname: &str, context: &str, derivation_path: &str) -> Result<Self> {
        let mut sys = System::new_all();
        sys.refresh_all();

        debug!("🔍 reading uptime_secs");
        let uptime_secs = System::uptime();

        debug!("🔍 reading os");
        let os = System::os_version();
        debug!("🔍 reading kernel");
        let kernel = System::kernel_version();

        debug!("🔍 reading memory_gb");
        let memory_gb = Some(sys.total_memory() as f64 / 1024.0 / 1024.0);
        debug!("🔍 reading cpu_brand");
        let cpu_brand = sys.cpus().get(0).map(|c| c.brand().to_string());
        let cpu_cores = Some(sys.cpus().len() as i32);

        debug!("🔍 reading board_serial");
        let board_serial = read_trimmed("/sys/class/dmi/id/board_serial")?;
        debug!("🔍 reading product_uuid");
        let product_uuid = read_trimmed("/sys/class/dmi/id/product_uuid")?;
        debug!("🔍 reading rootfs_uuid");
        let rootfs_uuid = get_rootfs_uuid();

        debug!("🔍 reading chassis_serial");
        let chassis_serial = read_trimmed("/sys/class/dmi/id/chassis_serial")?;
        debug!("🔍 reading bios_version");
        let bios_version = read_trimmed("/sys/class/dmi/id/bios_version")?;
        debug!("🔍 reading cpu_microcode");
        let cpu_microcode = read_trimmed("/proc/cpuinfo").ok().flatten().and_then(|c| {
            c.lines()
                .find(|l| l.contains("microcode"))
                .map(|l| l.to_string())
        });

        debug!("🔍 reading network interfaces");
        let network_interfaces = get_network_interfaces()
            .ok()
            .map(|interfaces| serde_json::to_value(interfaces).unwrap_or(serde_json::Value::Null));
        debug!("🔍 reading primary_mac_address");
        let primary_mac_address = get_primary_mac().ok();
        debug!("🔍 reading primary_ip_address");
        let primary_ip_address = get_primary_ip().ok();
        debug!("🔍 reading gateway_ip");
        let gateway_ip = get_gateway_ip().ok();

        debug!("🔍 reading selinux_status");
        let selinux_status = get_selinux_status().ok();
        debug!("🔍 reading tpm_present");
        let tpm_present = Some(Path::new("/dev/tpm0").exists());
        debug!("🔍 reading secure_boot_enabled");
        let secure_boot_enabled = read_trimmed(
            "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c",
        )
        .ok()
        .map(|v| v == Some("1".to_string()));
        debug!("🔍 reading fips_mode");
        let fips_mode = read_trimmed("/proc/sys/crypto/fips_enabled")
            .ok()
            .map(|v| v == Some("1".to_string()));

        debug!("🔍 reading software versions");
        let agent_version = Some(env!("CARGO_PKG_VERSION").to_string());
        let agent_build_hash = option_env!("SRC_HASH").map(|s| s.to_string());
        let nixos_version = read_trimmed("/etc/os-release").ok().and_then(|c| {
            c?.lines()
                .find(|l| l.starts_with("VERSION="))
                .map(|l| l.trim_start_matches("VERSION=").replace('"', ""))
        });

        Ok(SystemState {
            id: None,
            timestamp: Some(Utc::now()),
            hostname: hostname.to_string(),
            derivation_path: Some(derivation_path.to_string()),
            context: context.to_string(),
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
        })
    }
}

impl fmt::Display for SystemState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let uptime_days = self.uptime_secs.unwrap_or(0) / 86400; // days
        let uptime_hours = (self.uptime_secs.unwrap_or(0) % 86400) / 3600; // hours

        write!(
            f,
            "✅ accepted agent: {}\n   • context:      {}\n   • hostname:     {}\n   • hash:         {}\n   • os:           {}\n   • kernel:       {}\n   • memory:       {} GB\n   • uptime:       {}d {}h\n   • cpu:          {} ({})\n   • board_serial: {}\n   • uuid:         {}",
            self.hostname,
            self.context,
            self.hostname,
            self.derivation_path.as_deref().unwrap_or("unknown"),
            self.os.as_deref().unwrap_or("unknown"),
            self.kernel.as_deref().unwrap_or("unknown"),
            self.memory_gb.unwrap_or(0.0),
            uptime_days,
            uptime_hours,
            self.cpu_brand.as_deref().unwrap_or("unknown"),
            self.cpu_cores.unwrap_or(0),
            self.board_serial.as_deref().unwrap_or("unknown"),
            self.product_uuid.as_deref().unwrap_or("unknown")
        )
    }
}

fn get_rootfs_uuid() -> Option<String> {
    // Get the real source of /
    let dev = std::process::Command::new("findmnt")
        .args(["-n", "-o", "SOURCE", "-T", "/"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })?;

    // Strip Btrfs subvolume suffix like /dev/sda2[/@]
    let dev_clean = dev.split('[').next().unwrap_or("").trim();

    if dev_clean.is_empty() {
        return None;
    }

    if !dev_clean.starts_with("/dev/") {
        // Likely ZFS
        return std::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "guid", &dev_clean])
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
                } else {
                    None
                }
            });
    }

    // blkid for UUID
    std::process::Command::new("blkid")
        .args(["-s", "UUID", "-o", "value", dev_clean])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                None
            }
        })
}

fn read_trimmed<P: AsRef<Path>>(path: P) -> std::io::Result<Option<String>> {
    fs::read_to_string(path)
        .map(|s| Some(s.trim().to_string()))
        .or_else(|e| {
            if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::NotFound) {
                Ok(None)
            } else {
                Err(e)
            }
        })
}
