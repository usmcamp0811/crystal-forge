use cf_config::config::{BuildConfig, CacheConfig};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use tokio::process::Command;
use tracing::{debug, info};

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
        // match std::fs::read_to_string(config_path) {
        //     Ok(contents) => debug!("Config file contents: {}", contents),
        //     Err(e) => debug!("Cannot read config file: {}", e),
        // }
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

pub fn apply_cache_config_env_to_command(cmd: &mut Command, cache: &CacheConfig) {
    if let Some(ref v) = cache.s3_access_key_id {
        cmd.env("AWS_ACCESS_KEY_ID", v);
    }
    if let Some(ref v) = cache.s3_secret_access_key {
        cmd.env("AWS_SECRET_ACCESS_KEY", v);
    }
    if let Some(ref v) = cache.s3_session_token {
        cmd.env("AWS_SESSION_TOKEN", v);
    }
    if let Some(ref v) = cache.s3_region {
        cmd.env("AWS_REGION", v);
        cmd.env("AWS_DEFAULT_REGION", v);
    }
    if let Some(ref v) = cache.s3_profile {
        cmd.env("AWS_PROFILE", v);
    }
    if let Some(ref v) = cache.s3_endpoint_url {
        cmd.env("AWS_ENDPOINT_URL", v);
        cmd.env("AWS_ENDPOINT_URL_S3", v);
    }
    if let Some(ref v) = cache.attic_token {
        cmd.env("ATTIC_TOKEN", v);
    }
    if let Some(v) = attic_server_url_from_cache_config(cache) {
        cmd.env("ATTIC_SERVER_URL", v);
    }
}

pub fn attic_server_url_from_cache_config(cache: &CacheConfig) -> Option<String> {
    let raw = cache.push_to.as_deref()?.trim();
    if raw.is_empty() {
        return None;
    }

    if let Some(rest) = raw.strip_prefix("attic://") {
        let host = rest.split('/').next()?.trim();
        if host.is_empty() {
            return None;
        }
        return Some(format!("https://{host}"));
    }

    if !(raw.starts_with("http://") || raw.starts_with("https://")) {
        return None;
    }

    let Some(cache_name) = cache
        .attic_cache_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Some(raw.trim_end_matches('/').to_string());
    };

    let trimmed = raw.trim_end_matches('/');
    let suffix = format!("/{cache_name}");
    Some(trimmed.strip_suffix(&suffix).unwrap_or(trimmed).to_string())
}

pub fn apply_cache_config_env_for_scope(scoped: &mut Command, cache: &CacheConfig) {
    if let Some(ref v) = cache.s3_access_key_id {
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_ACCESS_KEY_ID={v}"));
    }
    if let Some(ref v) = cache.s3_secret_access_key {
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_SECRET_ACCESS_KEY={v}"));
    }
    if let Some(ref v) = cache.s3_session_token {
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_SESSION_TOKEN={v}"));
    }
    if let Some(ref v) = cache.s3_region {
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_REGION={v}"));
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_DEFAULT_REGION={v}"));
    }
    if let Some(ref v) = cache.s3_profile {
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_PROFILE={v}"));
    }
    if let Some(ref v) = cache.s3_endpoint_url {
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_ENDPOINT_URL={v}"));
        scoped.arg("--setenv");
        scoped.arg(format!("AWS_ENDPOINT_URL_S3={v}"));
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
    info!(
        "🌍 Environment vars: AWS_ACCESS_KEY_ID={}, AWS_SECRET_ACCESS_KEY={}",
        std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default(), // Empty string if not set
        if std::env::var("AWS_SECRET_ACCESS_KEY").is_ok() {
            "***SET***"
        } else {
            "NOT SET"
        }
    );
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

// ============================================================================
// Flake reference building helpers
// ============================================================================

/// Normalize a Git repository URL into a proper Nix flake reference URL.
///
/// Nix's `builtins.getFlake` only accepts properly-formed flake references.
/// SCP-style SSH URLs like `git@github.com:org/repo.git` are not valid flake
/// references — they must be converted to `git+ssh://git@github.com/org/repo.git`.
///
/// This function handles:
/// - SCP-style `user@host:path`   → `git+ssh://user@host/path`
/// - `ssh://user@host/path`       → `git+ssh://user@host/path`
/// - `https://host/path`          → `git+https://host/path` (pass through)
/// - Already has `git+`           → pass through unchanged
/// - `github:owner/repo`          → `git+github:owner/repo` (Nix shorthand, pass through)
/// - Bare `host:path`             → `git+ssh://git@host/path`
pub fn normalize_flake_git_url(repo_url: &str) -> String {
    // Already has git+ prefix — Nix will handle it directly.
    if repo_url.starts_with("git+") {
        return repo_url.to_string();
    }

    // github:/gitlab:/sourcehut: shorthands or other scheme:// URLs — just add git+
    if repo_url.contains("://") && !repo_url.starts_with("ssh://") {
        return format!("git+{repo_url}");
    }

    // SCP-style: git@host:path
    // Convert to proper git+ssh://git@host/path form.
    if let Some(rest) = repo_url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("git+ssh://git@{host}/{path}");
        }
        // No colon, e.g. just "git@host/path" — prefix with git+ssh://
        return format!("git+ssh://{repo_url}");
    }

    // ssh:// scheme (without git@ prefix, e.g. ssh://git@host/path)
    if repo_url.starts_with("ssh://") {
        return format!("git+{repo_url}");
    }

    // http:// or https://
    if repo_url.starts_with("http://") || repo_url.starts_with("https://") {
        return format!("git+{repo_url}");
    }

    // Bare host:path  (no user@ prefix, but looks like scp-style).
    // Only match when the host contains a '.' to distinguish from
    // Nix shorthands like github:owner/repo, gitlab:owner/repo, etc.
    if let Some((host, path)) = repo_url.split_once(':') {
        if host.contains('.') && !host.contains('/') && !path.contains("//") {
            return format!("git+ssh://git@{host}/{path}");
        }
    }

    // Fallback: just prefix with git+
    format!("git+{repo_url}")
}

/// Build the base flake reference (git+url?rev=hash)
pub fn build_flake_reference(repo_url: &str, commit_hash: &str) -> String {
    // First normalize the URL so it's a valid Nix flake ref
    let normalized = normalize_flake_git_url(repo_url);

    if normalized.contains("?rev=") {
        normalized
    } else {
        let separator = if normalized.contains('?') { "&" } else { "?" };
        format!("{normalized}{separator}rev={commit_hash}")
    }
}

/// Build flake target for agent deployment (nixos-rebuild compatible)
pub fn build_agent_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    debug!("Making Deployment Target for {system_name} ==> {flake_ref}#{system_name}");
    format!("{flake_ref}#{system_name}")
}

/// Build flake target for evaluation (nix path-info compatible)
pub fn build_evaluation_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    format!("{flake_ref}#nixosConfigurations.{system_name}.config.system.build.toplevel")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_scp_ssh_url_no_dot_git() {
        // git@host:path without .git
        assert_eq!(
            normalize_flake_git_url("git@github.com:ATALLC/nix-config"),
            "git+ssh://git@github.com/ATALLC/nix-config"
        );
    }

    #[test]
    fn test_normalize_scp_ssh_url_with_dot_git() {
        // git@host:path.git
        assert_eq!(
            normalize_flake_git_url("git@github.com:ATALLC/nix-config.git"),
            "git+ssh://git@github.com/ATALLC/nix-config.git"
        );
    }

    #[test]
    fn test_normalize_shorthand_github() {
        // github: shorthand (Nix native)
        assert_eq!(
            normalize_flake_git_url("github:nixos/nixpkgs"),
            "git+github:nixos/nixpkgs"
        );
    }

    #[test]
    fn test_normalize_https_url_without_dot_git() {
        assert_eq!(
            normalize_flake_git_url("https://github.com/nixos/nixpkgs"),
            "git+https://github.com/nixos/nixpkgs"
        );
    }

    #[test]
    fn test_normalize_https_url_with_dot_git() {
        assert_eq!(
            normalize_flake_git_url("https://github.com/nixos/nixpkgs.git"),
            "git+https://github.com/nixos/nixpkgs.git"
        );
    }

    #[test]
    fn test_normalize_ssh_scheme_url() {
        assert_eq!(
            normalize_flake_git_url("ssh://git@github.com/nixos/nixpkgs"),
            "git+ssh://git@github.com/nixos/nixpkgs"
        );
    }

    #[test]
    fn test_normalize_already_has_git_plus() {
        assert_eq!(
            normalize_flake_git_url("git+https://github.com/nixos/nixpkgs"),
            "git+https://github.com/nixos/nixpkgs"
        );
    }

    #[test]
    fn test_normalize_already_has_git_plus_ssh() {
        assert_eq!(
            normalize_flake_git_url("git+ssh://git@github.com/nixos/nixpkgs"),
            "git+ssh://git@github.com/nixos/nixpkgs"
        );
    }

    #[test]
    fn test_normalize_bare_host_colon_path() {
        // Bare host:path without git@ prefix
        assert_eq!(
            normalize_flake_git_url("github.com:ATALLC/nix-config"),
            "git+ssh://git@github.com/ATALLC/nix-config"
        );
    }

    #[test]
    fn test_normalize_scp_url_with_query() {
        assert_eq!(
            normalize_flake_git_url("git@github.com:ATALLC/nix-config?ref=main"),
            "git+ssh://git@github.com/ATALLC/nix-config?ref=main"
        );
    }

    #[test]
    fn test_build_flake_reference_scp_url() {
        let result = build_flake_reference("git@github.com:ATALLC/nix-config.git", "abc123");
        assert_eq!(
            result,
            "git+ssh://git@github.com/ATALLC/nix-config.git?rev=abc123"
        );
    }

    #[test]
    fn test_build_flake_reference_https_url() {
        let result = build_flake_reference("https://github.com/ATALLC/nix-config", "abc123");
        assert_eq!(
            result,
            "git+https://github.com/ATALLC/nix-config?rev=abc123"
        );
    }

    #[test]
    fn test_build_flake_reference_already_git_plus() {
        let result = build_flake_reference(
            "git+https://github.com/ATALLC/nix-config?rev=def456",
            "abc123",
        );
        // Already has ?rev= so it's returned as-is
        assert_eq!(
            result,
            "git+https://github.com/ATALLC/nix-config?rev=def456"
        );
    }

    #[test]
    fn test_build_flake_reference_shorthand() {
        let result = build_flake_reference("github:nixos/nixpkgs", "abc123");
        assert_eq!(result, "git+github:nixos/nixpkgs?rev=abc123");
    }
}
