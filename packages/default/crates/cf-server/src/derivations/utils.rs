use crate::config::{BuildConfig, CacheConfig};
use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, info, warn};

const DEPENDENCY_BUILD_PLAN_COMMAND_TIMEOUT_SECS: u64 = 120;

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
    let normalized = normalize_flake_git_url(repo_url);
    let (without_fragment, fragment) = normalized
        .split_once('#')
        .map_or((normalized.as_str(), None), |(base, fragment)| {
            (base, Some(fragment))
        });
    let (base, query) = without_fragment
        .split_once('?')
        .map_or((without_fragment, ""), |(base, query)| (base, query));
    let mut parameters = query
        .split('&')
        .filter(|parameter| !parameter.is_empty())
        .filter(|parameter| parameter.split_once('=').map_or(*parameter, |(key, _)| key) != "rev")
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    parameters.push(format!("rev={commit_hash}"));
    let mut reference = format!("{base}?{}", parameters.join("&"));
    if let Some(fragment) = fragment {
        reference.push('#');
        reference.push_str(fragment);
    }
    reference
}

pub fn flake_reference_revision(flake_ref: &str) -> Option<&str> {
    let query = flake_ref.split_once('?')?.1.split('#').next()?;
    query.split('&').find_map(|parameter| {
        let (key, value) = parameter.split_once('=')?;
        (key == "rev" && !value.is_empty()).then_some(value)
    })
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
// Derivation closure and build status helpers
// ============================================================================

/// Get all derivations in a closure with their build status
pub async fn get_complete_closure(
    derivation_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<(String, bool)>> {
    // Return (drv_path, is_built)
    let output = Command::new("nix")
        .args(["path-info", "--derivation", "--recursive", derivation_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get closure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let drv_paths: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .filter(|line| !line.is_empty() && *line != derivation_path)
        .map(|s| s.to_string())
        .collect();

    // Check which ones are already built in the local store
    let mut closure = Vec::new();
    for drv_path in drv_paths {
        let is_built = check_if_built(&drv_path).await.unwrap_or(false);
        closure.push((drv_path, is_built));
    }

    Ok(closure)
}

/// Check if a derivation is already built in the Nix store
pub async fn check_if_built(drv_path: &str) -> Result<bool> {
    // Check if all outputs of this derivation exist in the store
    let output = Command::new("nix")
        .args(["path-info", "--json", drv_path])
        .output()
        .await?;

    Ok(output.status.success())
}

/// Get the store path from a .drv path
pub async fn get_store_path_from_drv(drv_path: &str) -> Result<String> {
    let output = Command::new("nix-store")
        .args(["--query", "--outputs", drv_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("Failed to get store path for {}", drv_path);
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Enhanced version that gets closure with cache status in one pass
pub async fn get_complete_closure_with_cache_status(
    derivation_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<(String, String, bool)>> {
    // Returns (drv_path, store_path, is_built)

    // Get all derivations in closure
    let output = Command::new("nix")
        .args(["path-info", "--derivation", "--recursive", derivation_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get closure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let drv_paths: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .filter(|line| !line.is_empty() && *line != derivation_path)
        .map(|s| s.to_string())
        .collect();

    info!(
        "🔍 Checking build status for {} derivations...",
        drv_paths.len()
    );

    // Check status in batches for better performance
    let mut closure = Vec::new();
    for drv_path in drv_paths {
        let (store_path, is_built) = match get_store_path_and_build_status(&drv_path).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Failed to check status for {}: {}", drv_path, e);
                (String::new(), false)
            }
        };
        closure.push((drv_path, store_path, is_built));
    }

    Ok(closure)
}

/// Get store path and check if it's built in one call
pub async fn get_store_path_and_build_status(drv_path: &str) -> Result<(String, bool)> {
    // First get the store path
    let output = Command::new("nix-store")
        .args(["--query", "--outputs", drv_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("Failed to get store path for {}", drv_path);
    }

    let store_path = String::from_utf8(output.stdout)?.trim().to_string();

    // Check if it exists
    let is_built = Command::new("nix")
        .args(["path-info", &store_path])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    Ok((store_path, is_built))
}

/// Counts dependency derivations and the subset that Nix would build.
///
/// The dependency total contains only `.drv` requisites and excludes the exact
/// top-level system derivation. The build count contains only derivations from
/// the build section of `nix-store --realise --dry-run`; fetched paths do not
/// contribute. The dry run uses the supplied [`BuildConfig`], so substitute and
/// offline settings match the real build.
///
/// # Errors
///
/// Returns an error when either Nix command fails, times out, emits non-UTF-8
/// output, or reports a malformed or internally inconsistent build section.
/// No fallback count is returned.
pub async fn calculate_dependency_build_plan(
    drv_path: &str,
    build_config: &BuildConfig,
) -> Result<DependencyBuildPlan> {
    info!("📦 Calculating dependency build plan for {}", drv_path);
    let command_timeout = Duration::from_secs(DEPENDENCY_BUILD_PLAN_COMMAND_TIMEOUT_SECS);
    let req_out = timeout(
        command_timeout,
        Command::new("nix-store")
            .args(["--query", "--requisites", drv_path])
            .output(),
    )
    .await
    .map_err(|_| {
        anyhow!(
            "nix-store --query --requisites timed out after {}s for {}",
            DEPENDENCY_BUILD_PLAN_COMMAND_TIMEOUT_SECS,
            drv_path
        )
    })??;

    if !req_out.status.success() {
        anyhow::bail!(
            "nix-store --query --requisites failed: {}",
            String::from_utf8_lossy(&req_out.stderr)
        );
    }

    let dependency_derivation_count = parse_dependency_requisites(&req_out.stdout, drv_path)?;

    let mut command = dependency_build_plan_command(drv_path, build_config);
    let plan_out = timeout(command_timeout, command.output())
        .await
        .map_err(|_| {
            anyhow!(
                "nix-store --realise --dry-run timed out after {}s for {}",
                DEPENDENCY_BUILD_PLAN_COMMAND_TIMEOUT_SECS,
                drv_path
            )
        })??;

    let dependency_build_count = interpret_dependency_build_plan_output(
        plan_out.status.success(),
        &plan_out.stderr,
        drv_path,
    )?;
    Ok(DependencyBuildPlan {
        dependency_derivation_count,
        dependency_build_count,
    })
}

/// Contains dependency counts calculated for one evaluated NixOS system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyBuildPlan {
    /// Number of `.drv` requisites excluding the exact top-level system derivation.
    pub dependency_derivation_count: i32,
    /// Number of dependency derivations that Nix reports it would build.
    pub dependency_build_count: i32,
}

fn dependency_build_plan_command(drv_path: &str, build_config: &BuildConfig) -> Command {
    let mut command = Command::new("nix-store");
    command.args(["--realise", "--dry-run", drv_path]);
    // COMPATIBILITY: The legacy dry-run interface is human-readable. A fixed
    // locale keeps the documented singular and plural section headers stable.
    command.env("LC_ALL", "C");
    build_config.apply_to_command(&mut command);
    command
}

fn parse_dependency_requisites(output: &[u8], top_level_drv: &str) -> Result<i32> {
    let requisites = String::from_utf8(output.to_vec())?;
    let count = requisites
        .lines()
        .map(str::trim)
        .filter(|path| path.ends_with(".drv") && *path != top_level_drv)
        .collect::<HashSet<_>>()
        .len();
    i32::try_from(count).map_err(|_| anyhow!("dependency derivation count exceeds i32"))
}

fn parse_dependency_build_plan(output: &[u8], top_level_drv: &str) -> Result<i32> {
    let output = String::from_utf8(output.to_vec())?;
    let mut expected = None;
    let mut in_build_section = false;
    let mut reported = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line == "this derivation will be built:" {
            if expected.replace(1).is_some() {
                anyhow::bail!("dry-run output contains multiple build sections");
            }
            in_build_section = true;
            continue;
        }
        if let Some(value) = line
            .strip_prefix("these ")
            .and_then(|value| value.strip_suffix(" derivations will be built:"))
        {
            let count = value
                .parse::<usize>()
                .map_err(|_| anyhow!("malformed dry-run build count: {line}"))?;
            if expected.replace(count).is_some() {
                anyhow::bail!("dry-run output contains multiple build sections");
            }
            in_build_section = true;
            continue;
        }
        if line.ends_with(':')
            && (line.contains(" path will be fetched") || line.contains(" paths will be fetched"))
        {
            in_build_section = false;
            continue;
        }
        if line.contains("will be built") {
            anyhow::bail!("malformed dry-run build header: {line}");
        }
        if in_build_section && line.starts_with("/nix/store/") {
            if !line.ends_with(".drv") {
                anyhow::bail!("non-derivation path in dry-run build section: {line}");
            }
            reported.push(line);
        }
    }

    let Some(expected) = expected else {
        // A successful dry run emits no build section when realization is a no-op.
        return Ok(0);
    };
    if reported.len() != expected {
        anyhow::bail!(
            "dry-run build count mismatch: header reports {}, section contains {}",
            expected,
            reported.len()
        );
    }

    let count = reported
        .into_iter()
        .filter(|path| *path != top_level_drv)
        .collect::<HashSet<_>>()
        .len();
    i32::try_from(count).map_err(|_| anyhow!("dependency build count exceeds i32"))
}

fn interpret_dependency_build_plan_output(
    succeeded: bool,
    stderr: &[u8],
    top_level_drv: &str,
) -> Result<i32> {
    if !succeeded {
        anyhow::bail!(
            "nix-store --realise --dry-run failed: {}",
            String::from_utf8_lossy(stderr)
        );
    }

    parse_dependency_build_plan(stderr, top_level_drv)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const TOP_LEVEL_DRV: &str = "/nix/store/aaaaaaaa-system.drv";

    #[test]
    fn dependency_requisites_count_only_unique_dependency_derivations() {
        let output = format!(
            "/nix/store/source.tar.gz\n/nix/store/bbbbbbbb-one.drv\n{TOP_LEVEL_DRV}\n/nix/store/config.json\n/nix/store/bbbbbbbb-one.drv\n/nix/store/cccccccc-two.drv\n"
        );
        assert_eq!(
            parse_dependency_requisites(output.as_bytes(), TOP_LEVEL_DRV).unwrap(),
            2
        );
    }

    #[test]
    fn dependency_build_plan_parses_plural_and_excludes_fetched_paths_and_top_level() {
        let output = format!(
            "these 3 derivations will be built:\n  /nix/store/bbbbbbbb-one.drv\n  {TOP_LEVEL_DRV}\n  /nix/store/cccccccc-two.drv\nthese 2 paths will be fetched (1.0 MiB download, 2.0 MiB unpacked):\n  /nix/store/dddddddd-fetched\n  /nix/store/eeeeeeee-fetched.drv\n"
        );
        assert_eq!(
            parse_dependency_build_plan(output.as_bytes(), TOP_LEVEL_DRV).unwrap(),
            2
        );
    }

    #[test]
    fn dependency_build_plan_parses_singular() {
        let output = b"this derivation will be built:\n  /nix/store/bbbbbbbb-one.drv\n";
        assert_eq!(
            parse_dependency_build_plan(output, TOP_LEVEL_DRV).unwrap(),
            1
        );
    }

    #[test]
    fn dependency_build_plan_accepts_successful_no_op() {
        assert_eq!(parse_dependency_build_plan(b"", TOP_LEVEL_DRV).unwrap(), 0);
    }

    #[test]
    fn dependency_build_plan_rejects_mismatch_and_malformed_paths() {
        let mismatch = b"these 2 derivations will be built:\n  /nix/store/bbbbbbbb-one.drv\n";
        assert!(parse_dependency_build_plan(mismatch, TOP_LEVEL_DRV).is_err());

        let malformed = b"this derivation will be built:\n  /nix/store/bbbbbbbb-output\n";
        assert!(parse_dependency_build_plan(malformed, TOP_LEVEL_DRV).is_err());

        let malformed_header = b"some derivations will be built:\n";
        assert!(parse_dependency_build_plan(malformed_header, TOP_LEVEL_DRV).is_err());
    }

    #[test]
    fn dependency_build_plan_rejects_failed_nix_command() {
        let error = interpret_dependency_build_plan_output(
            false,
            b"error: cannot contact configured substituter",
            TOP_LEVEL_DRV,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("nix-store --realise --dry-run failed"));
    }

    #[test]
    fn dependency_build_plan_command_applies_build_configuration() {
        let config = BuildConfig {
            use_substitutes: false,
            offline: true,
            max_jobs: 7,
            cores_per_job: 3,
            ..BuildConfig::default()
        };
        let command = dependency_build_plan_command(TOP_LEVEL_DRV, &config);
        let arguments = command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments.windows(2).any(|args| args == ["--max-jobs", "7"]));
        assert!(arguments.windows(2).any(|args| args == ["--cores", "3"]));
        assert!(arguments.iter().any(|arg| arg == "--no-substitute"));
        assert!(arguments.iter().any(|arg| arg == "--offline"));
        assert_eq!(
            command
                .as_std()
                .get_envs()
                .find(|(name, _)| *name == "LC_ALL")
                .and_then(|(_, value)| value)
                .map(|value| value.to_string_lossy().into_owned()),
            Some("C".to_string())
        );
    }

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
        assert_eq!(
            result,
            "git+https://github.com/ATALLC/nix-config?rev=abc123"
        );
    }

    #[test]
    fn test_build_flake_reference_replaces_rev_and_preserves_other_query_parameters() {
        let result = build_flake_reference(
            "git+https://github.com/ATALLC/nix-config?ref=main&rev=def456&shallow=1",
            "abc123",
        );
        assert_eq!(
            result,
            "git+https://github.com/ATALLC/nix-config?ref=main&shallow=1&rev=abc123"
        );
        assert_eq!(flake_reference_revision(&result), Some("abc123"));
    }

    #[test]
    fn test_build_flake_reference_shorthand() {
        let result = build_flake_reference("github:nixos/nixpkgs", "abc123");
        assert_eq!(result, "git+github:nixos/nixpkgs?rev=abc123");
    }
}
