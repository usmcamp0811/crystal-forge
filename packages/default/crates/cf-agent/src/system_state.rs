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
    let network_interfaces = get_network_interfaces().ok().map(serde_json::Value::String);
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
    current_system_generation_info_at("/nix/var/nix/profiles/system", current_store_path)
}

fn current_system_generation_info_at(
    profile_path: impl AsRef<Path>,
    current_store_path: impl AsRef<Path>,
) -> (Option<i32>, Option<bool>) {
    let profile_path = profile_path.as_ref();
    let current_store_path = current_store_path.as_ref();
    let profile_link_target = match fs::read_link(profile_path) {
        Ok(path) => path,
        Err(_) => return (None, None),
    };

    let generation = profile_link_target
        .file_name()
        .and_then(|name| parse_generation_from_profile_link_name(name.to_string_lossy().as_ref()));
    let matches_current = nixos_system_paths_are_equivalent(profile_path, current_store_path);

    (generation, matches_current)
}

fn nixos_system_paths_are_equivalent(
    profile_path: impl AsRef<Path>,
    current_store_path: impl AsRef<Path>,
) -> Option<bool> {
    let profile_path = profile_path.as_ref();
    let current_store_path = current_store_path.as_ref();
    let profile_root = fs::canonicalize(profile_path).ok()?;
    let current_root = fs::canonicalize(current_store_path).ok()?;
    if profile_root == current_root {
        return Some(true);
    }

    // INVARIANT: A deploy-rs activatable wrapper is equivalent only when
    // multiple stable children resolve to the running NixOS toplevel's exact
    // children. Wrapper names and activation markers do not prove identity.
    const IDENTITY_SENTINELS: [&str; 3] = ["init", "system", "bin/switch-to-configuration"];
    let sentinels_match = IDENTITY_SENTINELS.iter().all(|relative_path| {
        let profile_sentinel = fs::canonicalize(profile_root.join(relative_path));
        let current_sentinel = fs::canonicalize(current_root.join(relative_path));
        matches!(
            (profile_sentinel, current_sentinel),
            (Ok(profile), Ok(current)) if profile == current
        )
    });
    Some(sentinels_match)
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
    use super::{
        current_system_generation_info_at, nixos_system_paths_are_equivalent,
        parse_generation_from_profile_link_name,
    };
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct FilesystemFixture {
        root: PathBuf,
    }

    impl FilesystemFixture {
        fn new() -> Self {
            let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "cf-agent-system-state-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&root).expect("create system-state fixture");
            Self { root }
        }

        fn nixos_system(&self, name: &str, identity: &str) -> PathBuf {
            let root = self.root.join(name);
            fs::create_dir_all(root.join("bin")).expect("create NixOS fixture directories");
            for relative_path in ["init", "system", "bin/switch-to-configuration"] {
                fs::write(
                    root.join(relative_path),
                    format!("{identity}:{relative_path}"),
                )
                .expect("write NixOS identity sentinel");
            }
            root
        }

        fn activatable_wrapper(&self, name: &str, system: &Path) -> PathBuf {
            let wrapper = self.root.join(name);
            fs::create_dir_all(wrapper.join("bin")).expect("create wrapper directories");
            for relative_path in ["init", "system", "bin/switch-to-configuration"] {
                symlink(system.join(relative_path), wrapper.join(relative_path))
                    .expect("link wrapper identity sentinel");
            }
            wrapper
        }
    }

    impl Drop for FilesystemFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("remove system-state fixture");
        }
    }

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

    #[test]
    fn direct_profile_target_matches_current_system() {
        let fixture = FilesystemFixture::new();
        let system = fixture.nixos_system("nixos-system-webb", "webb");
        let generation_link = fixture.root.join("system-3079-link");
        let profile = fixture.root.join("system");
        symlink(&system, &generation_link).expect("link generation to system");
        symlink("system-3079-link", &profile).expect("link profile to generation");

        assert_eq!(
            current_system_generation_info_at(&profile, &system),
            (Some(3079), Some(true))
        );
    }

    #[test]
    fn activatable_generation_matches_through_stable_sentinels() {
        let fixture = FilesystemFixture::new();
        let system = fixture.nixos_system("nixos-system-webb", "webb");
        let wrapper = fixture.activatable_wrapper("activatable-nixos-system-webb", &system);
        let generation_link = fixture.root.join("system-3079-link");
        let profile = fixture.root.join("system");
        symlink(&wrapper, &generation_link).expect("link generation to wrapper");
        symlink("system-3079-link", &profile).expect("link profile to generation");

        assert_eq!(
            current_system_generation_info_at(&profile, &system),
            (Some(3079), Some(true))
        );
    }

    #[test]
    fn unrelated_nixos_systems_do_not_match() {
        let fixture = FilesystemFixture::new();
        let first = fixture.nixos_system("nixos-system-first", "first");
        let second = fixture.nixos_system("nixos-system-second", "second");

        assert_eq!(
            nixos_system_paths_are_equivalent(&first, &second),
            Some(false)
        );
    }

    #[test]
    fn activatable_name_without_filesystem_identity_does_not_match() {
        let fixture = FilesystemFixture::new();
        let system = fixture.nixos_system("nixos-system-webb", "webb");
        let name_only_wrapper = fixture.nixos_system("activatable-nixos-system-webb", "different");

        assert_eq!(
            nixos_system_paths_are_equivalent(&name_only_wrapper, &system),
            Some(false)
        );
    }

    #[test]
    fn missing_profile_preserves_unknown_generation_and_identity() {
        let fixture = FilesystemFixture::new();
        let system = fixture.nixos_system("nixos-system-webb", "webb");

        assert_eq!(
            current_system_generation_info_at(fixture.root.join("missing-profile"), system),
            (None, None)
        );
    }
}
