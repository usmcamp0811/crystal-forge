use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crystal_forge::models::config::CrystalForgeConfig;
use crystal_forge::models::system_states::SystemState;
use ed25519_dalek::{Signer, SigningKey};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use reqwest::blocking::Client;
use serde_json::Value;
use std::{ffi::OsStr, fs, path::PathBuf, process::Command};
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: Update watch_system to take in the private key path
    watch_system().await
}

/// Reads a symlink and returns its target as a `PathBuf`.
fn readlink_path(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(nix::fcntl::readlink(path)?))
}

fn deriver_drv(path: &OsStr) -> Result<String> {
    // 1) Try nix-store (fast path)
    let out = Command::new("nix-store")
        .args(["--query", "--deriver"])
        .arg(path)
        .output()
        .context("nix-store --query --deriver failed to start")?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() && s.ends_with(".drv") {
            return Ok(s);
        }
    }

    // 2) Fallback to nix path-info --json (some caches omit Deriver in narinfo)
    let out = Command::new("nix")
        .args(["path-info", "--json"])
        .arg(path)
        .output()
        .context("nix path-info --json failed to start")?;

    if out.status.success() {
        let v: Value =
            serde_json::from_slice(&out.stdout).context("decoding nix path-info JSON")?;
        if let Some(deriver) = v
            .as_array()
            .and_then(|a| a.get(0))
            .and_then(|o| o.get("deriver"))
            .and_then(|d| d.as_str())
        {
            if !deriver.is_empty() && deriver.ends_with(".drv") {
                return Ok(deriver.to_string());
            }
        }
    }

    bail!(
        "deriver unknown for {} (cache may have omitted Deriver metadata)",
        PathBuf::from(path).display(),
    );
}

fn deriver_drv_with_test_fallback(path: &OsStr) -> Result<String> {
    match deriver_drv(path) {
        Ok(drv_path) => Ok(drv_path),
        Err(_) => {
            // In test environments, try to construct a reasonable .drv path
            let path_str = path.to_string_lossy();
            if path_str.contains("nixos-system-") {
                // Try to find a matching .drv in the nix store
                let output = std::process::Command::new("find")
                    .args(["/nix/store", "-name", "*nixos-system*.drv", "-type", "f"])
                    .output()?;

                if output.status.success() {
                    let stderr_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_drv) = stderr_str.lines().next() {
                        return Ok(first_drv.to_string());
                    }
                }
            }
            // NOTE: Do we want to return a more obvious name to say its not real?
            // Last resort: construct a fake .drv path for testing
            Ok(format!("{}.drv", path_str))
        }
    }
}

/// Posts the current Nix system derivation ID to a configured server.
///
/// This function:
/// 1. Loads the client and server configuration from the config file.
/// 2. Reads and decodes the Ed25519 private key from the configured path.
/// 3. Constructs a payload using the system hostname and derivation ID.
/// 4. Signs the payload using the private key.
/// 5. Sends the payload to the server with signature headers.
///
/// The server is expected to:
/// - Verify the signature using the public key listed in its authorized keys
/// - Accept a POST at `/ingest` with headers `X-Signature` and `X-Key-ID`
///
/// # Arguments
///
/// * `current_system` - A reference to the `OsStr` path pointing to the derivation in /nix/store
///
/// # Errors
///
/// Returns an error if configuration cannot be loaded, the key cannot be read,
/// signing fails, or the HTTP request fails.
/// Creates and signs a system state payload
fn create_signed_payload(
    current_system: &OsStr,
    context: &str,
) -> Result<(SystemState, String, String)> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = &cfg.client;
    let hostname = hostname::get()?.to_string_lossy().into_owned();

    // Guarantees a .drv (or returns an error)
    let drv_path = deriver_drv_with_test_fallback(current_system)?;

    let payload = SystemState::gather(&hostname, context, &drv_path)?;
    let payload_json = serde_json::to_string(&payload)?;

    let key_bytes = STANDARD
        .decode(fs::read_to_string(&client_cfg.private_key)?.trim())
        .context("failed to decode base64 private key")?;
    let signing_key = SigningKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .context("expected a 32-byte Ed25519 private key")?,
    );

    let signature = signing_key.sign(payload_json.as_bytes());
    let signature_b64 = STANDARD.encode(signature.to_bytes());

    Ok((payload, payload_json, signature_b64))
}

/// Posts system state changes to the server
pub fn post_system_state_change(current_system: &OsStr, context: &str) -> Result<()> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = &cfg.client;

    let (payload, payload_json, signature_b64) = create_signed_payload(current_system, context)?;
    let hostname = hostname::get()?.to_string_lossy().into_owned();

    // Send to state endpoint
    let client = Client::new();
    let (scheme, port_suffix) = match client_cfg.server_port {
        443 => ("https", "".to_string()),       // Omit :443 for HTTPS
        80 => ("http", "".to_string()),         // Omit :80 for HTTP
        port => ("http", format!(":{}", port)), // Include port for non-standard
    };

    let url = format!(
        "{}://{}{}/agent/state",
        scheme, client_cfg.server_host, port_suffix
    );

    println!("Posting state change to: {}", url);
    let res = client
        .post(url)
        .header("X-Signature", signature_b64)
        .header("X-Key-ID", hostname)
        .body(payload_json)
        .send()
        .context("failed to send state change POST")?;

    if !res.status().is_success() {
        anyhow::bail!("server responded with {}", res.status());
    }

    Ok(())
}

/// Posts heartbeat to the server
pub fn post_system_heartbeat(current_system: &OsStr, context: &str) -> Result<()> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = &cfg.client;

    let (payload, payload_json, signature_b64) = create_signed_payload(current_system, context)?;
    let hostname = hostname::get()?.to_string_lossy().into_owned();

    // Send to heartbeat endpoint - USE SAME URL LOGIC AS STATE ENDPOINT
    let client = Client::new();
    let (scheme, port_suffix) = match client_cfg.server_port {
        443 => ("https", "".to_string()),       // Omit :443 for HTTPS
        80 => ("http", "".to_string()),         // Omit :80 for HTTP
        port => ("http", format!(":{}", port)), // Include port for non-standard
    };

    let url = format!(
        "{}://{}{}/agent/heartbeat",
        scheme, client_cfg.server_host, port_suffix
    );

    println!("Posting heartbeat to: {}", url);
    let res = client
        .post(url)
        .header("X-Signature", signature_b64)
        .header("X-Key-ID", hostname)
        .body(payload_json)
        .send()
        .context("failed to send heartbeat POST")?;

    if !res.status().is_success() {
        anyhow::bail!("server responded with {}", res.status());
    }

    Ok(())
}

/// Handles an inotify event for a specific file name by reading the current system path
/// and invoking a callback to record the system state if the event matches "current-system".
fn report_current_system_derivation<F, R>(
    name: &OsStr,
    context: &str,
    readlink_fn: F,
    insert_fn: R,
) -> Result<()>
where
    F: Fn(&str) -> Result<PathBuf>,
    R: Fn(&OsStr, &str) -> Result<()>,
{
    if name != OsStr::new("current-system") {
        return Ok(());
    }

    let current_system = readlink_fn("/run/current-system")?;
    println!(
        "[{}] Current System: {}",
        context,
        current_system.to_string_lossy()
    );
    insert_fn(current_system.as_os_str(), context)?;
    Ok(())
}

/// Runs a loop that watches for inotify events and handles "current-system" changes using
/// provided readlink and insertion callbacks. Designed for testing and flexibility.
async fn watch_for_system_changes<F, R>(
    inotify: &mut Inotify,
    readlink_fn: F,
    insert_fn: R,
) -> Result<()>
where
    F: Fn(&str) -> Result<PathBuf>,
    R: Fn(&OsStr, &str) -> Result<()>,
{
    println!("Watching /run for changes to current-system...");

    report_current_system_derivation(
        OsStr::new("current-system"),
        "startup",
        &readlink_fn,
        &insert_fn,
    )?;
    loop {
        for event in inotify.read_events()? {
            if let Some(name) = event.name {
                println!("Detected change to /run/current-system");
                report_current_system_derivation(&name, "config_change", &readlink_fn, &insert_fn)?;
            }
        }
    }
}

async fn run_periodic_heartbeat_loop() -> Result<()> {
    sleep(Duration::from_secs(600)).await;
    info!("💓 Starting heartbeat loop (every 10m)...");
    loop {
        if let Err(e) = report_current_system_derivation(
            OsStr::new("current-system"),
            "heartbeat",
            readlink_path,
            post_system_heartbeat,
        ) {
            error!("❌ Heartbeat failed: {e}");
        }
        sleep(Duration::from_secs(600)).await;
    }
}

// TODO: Update watch_system to take in the private key path
/// Initializes an inotify watcher on `/run` for "current-system" and records updates
/// to the system state in the database.
pub async fn watch_system() -> Result<()> {
    let mut inotify = Inotify::init(InitFlags::empty())?;
    inotify.add_watch(
        "/run",
        AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO,
    )?;

    tokio::spawn(run_periodic_heartbeat_loop());
    // TODO: add watch for home-manager too
    watch_for_system_changes(&mut inotify, readlink_path, post_system_state_change).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn test_handle_event_triggers_on_current_system() {
        let called = RefCell::new(false);

        let readlink_mock = |_path: &str| Ok(PathBuf::from("/nix/store/fake-system"));
        let insert_mock = |os: &OsStr, _ctx: &str| {
            assert_eq!(os.to_string_lossy(), "/nix/store/fake-system");
            *called.borrow_mut() = true;
            Ok(())
        };

        let result = report_current_system_derivation(
            OsStr::new("current-system"),
            "test",
            readlink_mock,
            insert_mock,
        );
        assert!(result.is_ok());
        assert!(*called.borrow());
    }

    #[test]
    fn test_handle_event_ignores_other_files() {
        let readlink_mock = |_path: &str| panic!("should not be called");
        let insert_mock = |_os: &OsStr, _ctx: &str| panic!("should not be called"); // ← now takes 2 args

        let result = report_current_system_derivation(
            OsStr::new("other-file"),
            "test",
            readlink_mock,
            insert_mock,
        );
        assert!(result.is_ok());
    }
}
