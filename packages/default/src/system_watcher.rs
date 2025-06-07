use crate::config;
use crate::sys_fingerprint::{FingerprintParts, get_fingerprint};
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use std::cell::RefCell;

use reqwest::blocking::Client;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Reads a symlink and returns its target as a `PathBuf`.
fn readlink_path(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(nix::fcntl::readlink(path)?))
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
pub fn post_system_state(current_system: &OsStr) -> Result<()> {
    let cfg = config::load_config()?;
    let client_cfg = cfg.client;
    // TODO: Add MAC Address & Hardware fingerprint along with hostname
    let hostname = hostname::get()?.to_string_lossy().into_owned();
    let system_hash = Path::new(current_system)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| current_system.to_string_lossy().into_owned());

    let fingerprint = get_fingerprint()?;
    println!("{:#?}", fingerprint);

    // Construct payload: hostname:system_hash:fingerprint
    let payload = format!("{hostname}:{system_hash}:{fingerprint}");

    // Load and decode private key from file
    let key_bytes = STANDARD
        .decode(fs::read_to_string(&client_cfg.private_key)?.trim())
        .context("failed to decode base64 private key")?;

    let signing_key = SigningKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .context("expected a 32-byte Ed25519 private key")?,
    );
    let signature = signing_key.sign(payload.as_bytes());

    let signature_b64 = STANDARD.encode(signature.to_bytes());

    // Send HTTP POST with signed payload
    let client = Client::new();
    let url = format!(
        "http://{}:{}/current-system",
        client_cfg.server_host, client_cfg.server_port
    );

    println!("Server: {}", url);

    let res = client
        .post(url)
        .header("X-Signature", signature_b64)
        .header("X-Key-ID", hostname)
        .body(payload)
        .send()
        .context("failed to send POST")?;

    if !res.status().is_success() {
        anyhow::bail!("server responded with {}", res.status());
    }

    Ok(())
}

/// Handles an inotify event for a specific file name by reading the current system path
/// and invoking a callback to record the system state if the event matches "current-system".
fn handle_event<F, R>(name: &OsStr, readlink_fn: F, insert_fn: R) -> Result<()>
where
    F: Fn(&str) -> Result<PathBuf>,
    R: Fn(&OsStr) -> Result<()>,
{
    if name != OsStr::new("current-system") {
        return Ok(());
    }

    let current_system = readlink_fn("/run/current-system")?;
    println!("Current System: {}", current_system.to_string_lossy());
    insert_fn(current_system.as_os_str())?;

    Ok(())
}

/// Runs a loop that watches for inotify events and handles "current-system" changes using
/// provided readlink and insertion callbacks. Designed for testing and flexibility.
fn watch_system_loop<F, R>(inotify: &mut Inotify, readlink_fn: F, insert_fn: R) -> Result<()>
where
    F: Fn(&str) -> Result<PathBuf>,
    R: Fn(&OsStr) -> Result<()>,
{
    println!("Watching /run for changes to current-system...");
    let init = OsStr::new("current-system");
    handle_event(init, &readlink_fn, &insert_fn)?;
    loop {
        for event in inotify.read_events()? {
            if let Some(name) = event.name {
                println!("Detected change to /run/current-system");
                handle_event(&name, &readlink_fn, &insert_fn)?;
            }
        }
    }
}

// TODO: Update watch_system to take in the private key path
/// Initializes an inotify watcher on `/run` for "current-system" and records updates
/// to the system state in the database.
pub fn watch_system() -> Result<()> {
    let mut inotify = Inotify::init(InitFlags::empty())?;
    inotify.add_watch(
        "/run",
        AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO,
    )?;

    // TODO: add watch for home-manager too
    watch_system_loop(&mut inotify, readlink_path, post_system_state)
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
        let insert_mock = |os: &OsStr| {
            assert_eq!(os.to_string_lossy(), "/nix/store/fake-system");
            *called.borrow_mut() = true;
            Ok(())
        };

        let result = handle_event(OsStr::new("current-system"), readlink_mock, insert_mock);
        assert!(result.is_ok());
        assert!(*called.borrow());
    }

    #[test]
    fn test_handle_event_ignores_other_files() {
        let readlink_mock = |_path: &str| panic!("should not be called");
        let insert_mock = |_os: &OsStr| panic!("should not be called");

        let result = handle_event(OsStr::new("other-file"), readlink_mock, insert_mock);
        assert!(result.is_ok());
    }
}
