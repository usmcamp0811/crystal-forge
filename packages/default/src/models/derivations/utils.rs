use crate::models::config::BuildConfig;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tokio::process::Command;
use tracing::{debug, error, info};

/// Add/remove to taste; this set covers AWS + MinIO/common S3 endpoints.
pub const CACHE_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "ATTIC_SERVER_URL",
    "ATTIC_TOKEN",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "AWS_DEFAULT_REGION",
    "AWS_REGION",
    "AWS_ENDPOINT_URL",
    "AWS_ENDPOINT_URL_S3",
    "AWS_S3_ENDPOINT",
    "S3_ENDPOINT",
    "AWS_SHARED_CREDENTIALS_FILE",
    "AWS_CONFIG_FILE",
    "AWS_CA_BUNDLE",
    "SSL_CERT_FILE",
    "CURL_CA_BUNDLE",
    "NO_PROXY",
    "no_proxy",
    "NIX_CONFIG",
];

pub const DEFAULT_ATTIC_REMOTE: &str = "local";

// Track which Attic remotes have been logged in during this process
static ATTIC_LOGGED_REMOTES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub fn mark_attic_logged(remote: &str) {
    let set = ATTIC_LOGGED_REMOTES.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock().unwrap().insert(remote.to_string());
}

pub fn is_attic_logged(remote: &str) -> bool {
    let set = ATTIC_LOGGED_REMOTES.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock().unwrap().contains(remote)
}

pub fn clear_attic_logged(remote: &str) {
    let set = ATTIC_LOGGED_REMOTES.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock().unwrap().remove(remote);
}

pub fn debug_attic_environment() {
    debug!("=== Attic Environment Debug ===");
    debug!("HOME: {:?}", std::env::var("HOME"));
    debug!("XDG_CONFIG_HOME: {:?}", std::env::var("XDG_CONFIG_HOME"));
    debug!(
        "ATTIC_SERVER_URL: {:?}",
        std::env::var("ATTIC_SERVER_URL").map(|_| "[SET]")
    );
    debug!(
        "ATTIC_TOKEN: {:?}",
        std::env::var("ATTIC_TOKEN").map(|_| "[SET]")
    );
    debug!(
        "ATTIC_REMOTE_NAME: {:?}",
        std::env::var("ATTIC_REMOTE_NAME")
    );

    // Check if config file exists
    let config_path = "/var/lib/crystal-forge/.config/attic/config.toml";
    if std::path::Path::new(config_path).exists() {
        debug!("Attic config file exists at {}", config_path);
        match std::fs::read_to_string(config_path) {
            Ok(contents) => debug!("Config file contents: {}", contents),
            Err(e) => debug!("Cannot read config file: {}", e),
        }
    } else {
        debug!("Attic config file does not exist at {}", config_path);
    }
    debug!("=== End Attic Environment Debug ===");
}

pub fn apply_cache_env_to_command(cmd: &mut Command) {
    for &key in CACHE_ENV_ALLOWLIST {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }

    // Force the correct HOME and XDG_CONFIG_HOME for crystal-forge user
    cmd.env("HOME", "/var/lib/crystal-forge");
    cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");

    // Add Attic-specific environment variables if they exist
    if let Ok(val) = std::env::var("ATTIC_SERVER_URL") {
        cmd.env("ATTIC_SERVER_URL", val);
    }
    if let Ok(val) = std::env::var("ATTIC_TOKEN") {
        cmd.env("ATTIC_TOKEN", val);
    }
    if let Ok(val) = std::env::var("ATTIC_REMOTE_NAME") {
        cmd.env("ATTIC_REMOTE_NAME", val);
    }

    // If you set a custom S3 endpoint, disable IMDS by default
    let has_custom_endpoint = std::env::var_os("AWS_ENDPOINT_URL").is_some()
        || std::env::var_os("AWS_ENDPOINT_URL_S3").is_some()
        || std::env::var_os("AWS_S3_ENDPOINT").is_some()
        || std::env::var_os("S3_ENDPOINT").is_some();
    if has_custom_endpoint && std::env::var_os("AWS_EC2_METADATA_DISABLED").is_none() {
        cmd.env("AWS_EC2_METADATA_DISABLED", "true");
    }
}

pub fn apply_systemd_props_for_scope(build: &BuildConfig, cmd: &mut tokio::process::Command) {
    // resource-control props that are valid for scopes
    if let Some(ref memory_max) = build.systemd_memory_max {
        cmd.args(["--property", &format!("MemoryMax={}", memory_max)]);
    }
    if let Some(cpu_quota) = build.systemd_cpu_quota {
        cmd.args(["--property", &format!("CPUQuota={}%", cpu_quota)]);
    }
    if let Some(timeout_stop) = build.systemd_timeout_stop_sec {
        cmd.args(["--property", &format!("TimeoutStopSec={}", timeout_stop)]);
    }
    for p in &build.systemd_properties {
        // allow only resource-control-ish prefixes for scopes
        const OK: &[&str] = &[
            "Memory",
            "CPU",
            "Tasks",
            "IO",
            "Kill",
            "OOM",
            "Device",
            "IPAccounting",
        ];
        if OK.iter().any(|pre| p.starts_with(pre)) {
            cmd.args(["--property", p]);
        }
        // intentionally ignore service-only props like Environment=, Restart=, WorkingDirectory= …
    }
}

// Fixed apply_cache_env function - only use --setenv for systemd scopes
pub fn apply_cache_env(scoped: &mut Command) {
    for &key in CACHE_ENV_ALLOWLIST {
        if let Ok(val) = std::env::var(key) {
            // For systemd scopes, only use --setenv, not .env()
            // The .env() method affects the systemd-run process itself, not the scope
            scoped.arg("--setenv");
            scoped.arg(format!("{key}={val}"));
        }
    }

    // Force the correct HOME and XDG_CONFIG_HOME for crystal-forge user
    scoped.arg("--setenv");
    scoped.arg("HOME=/var/lib/crystal-forge");
    scoped.arg("--setenv");
    scoped.arg("XDG_CONFIG_HOME=/var/lib/crystal-forge/.config");

    // Add Attic-specific environment variables if they exist
    if let Ok(val) = std::env::var("ATTIC_SERVER_URL") {
        scoped.arg("--setenv");
        scoped.arg(format!("ATTIC_SERVER_URL={val}"));
    }
    if let Ok(val) = std::env::var("ATTIC_TOKEN") {
        scoped.arg("--setenv");
        scoped.arg(format!("ATTIC_TOKEN={val}"));
    }
    if let Ok(val) = std::env::var("ATTIC_REMOTE_NAME") {
        scoped.arg("--setenv");
        scoped.arg(format!("ATTIC_REMOTE_NAME={val}"));
    }

    // Handle AWS_EC2_METADATA_DISABLED specially
    let has_custom_endpoint = std::env::var_os("AWS_ENDPOINT_URL").is_some()
        || std::env::var_os("AWS_ENDPOINT_URL_S3").is_some()
        || std::env::var_os("AWS_S3_ENDPOINT").is_some()
        || std::env::var_os("S3_ENDPOINT").is_some();

    if has_custom_endpoint && std::env::var_os("AWS_EC2_METADATA_DISABLED").is_none() {
        scoped.arg("--setenv");
        scoped.arg("AWS_EC2_METADATA_DISABLED=true");
    }
}
