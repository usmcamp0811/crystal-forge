use crate::config;
use crate::db::insert_system_state;
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use sysinfo::System;
use systemstat::{Platform, System as StatSystem};

use reqwest::blocking::Client;
use std::fmt;
use std::io::{Error, ErrorKind};
use std::process::Command;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Struct holding system fingerprint components.
#[derive(Debug)]
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

/// Generates a system fingerprint with hardware and OS details.
///
/// # Returns
/// - `Ok(FingerprintParts)` with collected metadata
/// - `Err(std::io::Error)` if critical system reads fail
pub fn get_fingerprint() -> Result<FingerprintParts, Error> {
    let mut sys = System::new_all();
    let mut sysstat = StatSystem::new();
    sys.refresh_all();
    let hostname = System::host_name().unwrap_or_default();

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

    let rootfs_uuid = std::process::Command::new("sh")
        .arg("-c")
        .arg("findmnt -no SOURCE / | xargs blkid -s UUID -o value")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

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
