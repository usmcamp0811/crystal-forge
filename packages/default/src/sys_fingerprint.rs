use crate::config;
use crate::db::insert_system_state;
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Error, ErrorKind};
use std::process::Command;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use sysinfo::System;
use systemstat::System as StatSystem;

/// Struct holding system fingerprint components.
#[derive(Debug, Serialize, Deserialize)]
pub struct FingerprintParts {
    /// Operating system version (e.g., "NixOS 24.05").
    pub os: String,
    /// Kernel version (e.g., "6.1.38").
    pub kernel: String,
    /// Total system memory in gigabytes.
    pub memory_gb: f64,
    /// Uptime in seconds.
    pub uptime_secs: u64,
    /// Brand of the first CPU (e.g., "AMD Ryzen 7 5800X").
    pub cpu_brand: String,
    /// Number of logical CPU cores.
    pub cpu_cores: usize,
    /// Motherboard serial number, if available.
    pub board_serial: Option<String>,
    /// Product UUID from DMI data, if available.
    pub product_uuid: Option<String>,
    /// UUID of the root filesystem, if retrievable.
    pub rootfs_uuid: Option<String>,
}

impl fmt::Display for FingerprintParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "os={}, kernel={}, memory_gb={:.2}, uptime_secs={}, cpu_brand={}, cpu_cores={}, \
             board_serial={:?}, product_uuid={:?}, rootfs_uuid={:?}",
            self.os,
            self.kernel,
            self.memory_gb,
            self.uptime_secs,
            self.cpu_brand,
            self.cpu_cores,
            self.board_serial,
            self.product_uuid,
            self.rootfs_uuid
        )
    }
}

fn get_rootfs_uuid() -> Option<String> {
    // Get the device or mount info for root
    let df_output = std::process::Command::new("findmnt")
        .args(["-no", "SOURCE", "/"])
        .output()
        .ok()?;

    if !df_output.status.success() {
        return None;
    }

    let dev = String::from_utf8_lossy(&df_output.stdout)
        .trim()
        .to_string();

    // ZFS: If the source is a dataset name (not a /dev path)
    if !dev.starts_with("/dev/") {
        return std::process::Command::new("zfs")
            .args(["get", "-H", "-o", "value", "guid", &dev])
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

    // Otherwise try blkid UUID
    std::process::Command::new("blkid")
        .args(["-s", "UUID", "-o", "value", &dev])
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

/// Generates a system fingerprint with hardware and OS details.
///
/// # Returns
/// - `Ok(FingerprintParts)` with collected metadata
/// - `Err(std::io::Error)` if critical system reads fail
pub fn get_fingerprint() -> Result<FingerprintParts, Error> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let uptime_secs = System::uptime();
    let os = System::os_version().unwrap_or_default();
    let kernel = System::kernel_version().unwrap_or_default();
    let memory_gb = sys.total_memory() as f64 / 1024.0 / 1024.0;
    let cpu_brand = sys
        .cpus()
        .get(0)
        .map(|c| c.brand().to_string())
        .unwrap_or_default();
    let cpu_cores = sys.cpus().len();

    let board_serial = fs::read_to_string("/sys/class/dmi/id/board_serial")
        .map(|s| Some(s.trim().to_string()))
        .or_else(|e| {
            if e.kind() == ErrorKind::PermissionDenied {
                Ok(None)
            } else {
                Err(e)
            }
        })?;

    let product_uuid = fs::read_to_string("/sys/class/dmi/id/product_uuid")
        .map(|s| Some(s.trim().to_string()))
        .or_else(|e| {
            if e.kind() == ErrorKind::PermissionDenied {
                Ok(None)
            } else {
                Err(e)
            }
        })?;

    let rootfs_uuid = get_rootfs_uuid();
    Ok(FingerprintParts {
        os,
        kernel,
        memory_gb,
        uptime_secs,
        cpu_brand,
        cpu_cores,
        board_serial,
        product_uuid,
        rootfs_uuid,
    })
}
