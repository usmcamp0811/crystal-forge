use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::fmt;
use std::fs;
use sysinfo::System;

use std::io::ErrorKind;
use tracing::debug;

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct SystemState {
    pub id: Option<i32>,
    pub hostname: String,
    pub derivation_path: Option<String>,
    pub context: String,
    pub os: Option<String>,
    pub kernel: Option<String>,
    pub memory_gb: Option<f64>,
    pub uptime_secs: Option<i64>,
    pub cpu_brand: Option<String>,
    pub cpu_cores: Option<i32>,
    pub board_serial: Option<String>,
    pub product_uuid: Option<String>,
    pub rootfs_uuid: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

impl SystemState {
    pub fn gather(hostname: &str, context: &str, derivation_path: &str) -> Result<Self> {
        let mut sys = System::new_all();
        sys.refresh_all();

        debug!("🔍 reading uptime_secs");
        let uptime_secs = System::uptime();

        debug!("🔍 reading os");
        let os = System::os_version().unwrap_or_else(|| "unknown".to_string());
        debug!("🔍 reading kernel");
        let kernel = System::kernel_version().unwrap_or_else(|| "unknown".to_string());

        debug!("🔍 reading memory_gb");
        let memory_gb = sys.total_memory() as f64 / 1024.0 / 1024.0;
        debug!("🔍 reading cpu_brand");
        let cpu_brand = sys
            .cpus()
            .get(0)
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let cpu_cores = sys.cpus().len();

        debug!("🔍 reading board_serial");
        let board_serial = fs::read_to_string("/sys/class/dmi/id/board_serial")
            .map(|s| Some(s.trim().to_string()))
            .or_else(|e| {
                eprintln!("[fingerprint] board_serial read error: {:?}", e);
                if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::NotFound) {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;

        debug!("🔍 reading product_uuid");
        let product_uuid = fs::read_to_string("/sys/class/dmi/id/product_uuid")
            .map(|s| Some(s.trim().to_string()))
            .or_else(|e| {
                eprintln!("[fingerprint] product_uuid read error: {:?}", e);
                if matches!(e.kind(), ErrorKind::PermissionDenied | ErrorKind::NotFound) {
                    Ok(None)
                } else {
                    Err(e)
                }
            })?;

        debug!("🔍 reading rootfs_uuid");
        let rootfs_uuid = get_rootfs_uuid();

        Ok(SystemState {
            id: None,
            timestamp: None,
            hostname: hostname.to_string(),
            derivation_path: Some(derivation_path.to_string()),
            context: context.to_string(),
            os: Some(os),
            kernel: Some(kernel),
            memory_gb: Some(memory_gb),
            uptime_secs: Some(uptime_secs as i64),
            cpu_brand: Some(cpu_brand),
            cpu_cores: Some(cpu_cores as i32),
            board_serial,
            product_uuid,
            rootfs_uuid,
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
