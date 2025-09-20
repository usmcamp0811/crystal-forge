use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig};
use crate::queries::derivations::{
    clear_derivation_build_status, discover_and_insert_packages, update_derivation_build_status,
};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::process::Output;
use std::process::Stdio;
use std::sync::Mutex;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::{Duration, sleep};
use tokio::time::{Instant, interval};
use tracing::{debug, error, info, warn};

/// Add/remove to taste; this set covers AWS + MinIO/common S3 endpoints.
const CACHE_ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    // Attic-specific (optional – Attic mainly uses config files)
    "ATTIC_SERVER_URL",
    "ATTIC_TOKEN",
    // existing…
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

const DEFAULT_ATTIC_REMOTE: &str = "local";

// ⬇️ change OnceCell -> OnceLock
static ATTIC_LOGGED_REMOTES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn mark_attic_logged(remote: &str) {
    let set = ATTIC_LOGGED_REMOTES.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock().unwrap().insert(remote.to_string());
}

fn is_attic_logged(remote: &str) -> bool {
    let set = ATTIC_LOGGED_REMOTES.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock().unwrap().contains(remote)
}

fn clear_attic_logged(remote: &str) {
    let set = ATTIC_LOGGED_REMOTES.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock().unwrap().remove(remote);
}

fn debug_attic_environment() {
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

fn apply_cache_env_to_command(cmd: &mut Command) {
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

// Updated derivation model to match the new database schema
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct Derivation {
    pub id: i32,
    pub commit_id: Option<i32>,          // Changed from i32 to Option<i32>
    pub derivation_type: DerivationType, // "nixos" or "package"
    pub derivation_name: String, // display name (hostname for nixos, package name for packages)
    pub derivation_path: Option<String>, // populated post-build
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub evaluation_duration_ms: Option<i32>,
    pub error_message: Option<String>,
    // pub parent_derivation_id: Option<i32>, // for hierarchical relationships (packages → systems)
    pub pname: Option<String>,   // Nix package name (for packages)
    pub version: Option<String>, // package version (for packages)
    pub status_id: i32,          // foreign key to derivation_statuses table
    pub derivation_target: Option<String>,
    #[serde(default)]
    pub build_elapsed_seconds: Option<i32>,
    #[serde(default)]
    pub build_current_target: Option<String>,
    #[serde(default)]
    pub build_last_activity_seconds: Option<i32>,
    #[serde(default)]
    pub build_last_heartbeat: Option<DateTime<Utc>>,
}

// Enum for derivation types (updated from TargetType)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "lowercase")]
pub enum DerivationType {
    #[sqlx(rename = "nixos")]
    NixOS,
    #[sqlx(rename = "package")]
    Package,
}

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub main_derivation_path: String,
    pub dependency_derivation_paths: Vec<String>,
}

// SQLx requires this for deserialization - make it safe
impl From<String> for DerivationType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "nixos" => DerivationType::NixOS,
            "package" => DerivationType::Package,
            _ => {
                // Log the error but provide a default instead of panicking
                error!(
                    "Warning: Unknown DerivationType '{}', defaulting to NixOS",
                    s
                );
                DerivationType::NixOS
            }
        }
    }
}

impl ToString for DerivationType {
    fn to_string(&self) -> String {
        match self {
            DerivationType::NixOS => "nixos".into(),
            DerivationType::Package => "package".into(),
        }
    }
}

// Status information from the derivation_statuses table
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct DerivationStatus {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub is_terminal: bool,
    pub is_success: bool,
    pub display_order: i32,
}

impl Derivation {
    pub async fn push_to_cache_with_retry(
        &self,
        store_path: &str,
        cache_config: &CacheConfig,
        build_config: &BuildConfig,
    ) -> Result<()> {
        let mut attempts = 0;
        let max_attempts = cache_config.max_retries + 1;

        while attempts < max_attempts {
            match self
                .push_to_cache(store_path, cache_config, build_config)
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) if attempts < max_attempts - 1 => {
                    warn!(
                        "Cache push attempt {} failed: {}, retrying...",
                        attempts + 1,
                        e
                    );
                    sleep(Duration::from_secs(cache_config.retry_delay_seconds)).await;
                    attempts += 1;
                }
                Err(e) => return Err(e),
            }
        }

        unreachable!()
    }

    pub async fn summary(&self) -> Result<String> {
        let pool = CrystalForgeConfig::db_pool().await?;

        if let Some(commit_id) = self.commit_id {
            let commit = crate::queries::commits::get_commit_by_id(&pool, commit_id).await?;
            let flake = crate::queries::flakes::get_flake_by_id(&pool, commit.flake_id).await?;
            Ok(format!(
                "{}@{} ({})",
                flake.name, commit.git_commit_hash, self.derivation_name
            ))
        } else {
            // For packages without a commit (discovered during CVE scans)
            Ok(format!(
                "package {} ({})",
                self.derivation_name,
                self.pname.as_deref().unwrap_or("unknown")
            ))
        }
    }

    async fn run_print_out_paths(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
        // Check if systemd should be used
        if !build_config.should_use_systemd() {
            info!("📋 Systemd disabled in config, using direct execution for store path lookup");
            let mut direct = Command::new("nix");
            direct.args(["build", flake_target, "--print-out-paths"]);
            build_config.apply_to_command(&mut direct);
            return Ok(direct.output().await?);
        }

        let mut scoped = build_config.systemd_scoped_cmd_base();
        scoped.args([flake_target, "--print-out-paths"]);
        build_config.apply_to_command(&mut scoped);

        match scoped.output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(output)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Check for systemd-specific issues
                    if stderr.contains("Failed to connect to user scope bus")
                        || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
                        || stderr.contains("XDG_RUNTIME_DIR")
                        || stderr.contains("Failed to create scope")
                        || stderr.contains("systemd")
                    {
                        info!(
                            "📋 systemd-run not available for store path lookup, using direct execution"
                        );
                        let mut direct = Command::new("nix");
                        direct.args(["build", flake_target, "--print-out-paths"]);
                        build_config.apply_to_command(&mut direct);
                        Ok(direct.output().await?)
                    } else {
                        Ok(output)
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("📋 systemd-run not available for store path lookup");
                let mut direct = Command::new("nix");
                direct.args(["build", flake_target, "--print-out-paths"]);
                build_config.apply_to_command(&mut direct);
                Ok(direct.output().await?)
            }
            Err(e) => {
                warn!("⚠️ systemd-run failed for store path lookup: {}", e);
                let mut direct = Command::new("nix");
                direct.args(["build", flake_target, "--print-out-paths"]);
                build_config.apply_to_command(&mut direct);
                Ok(direct.output().await?)
            }
        }
    }

    async fn run_streaming_build(
        mut cmd: Command,
        drv_path: &str,
        derivation_id: i32,
        pool: &PgPool,
    ) -> Result<String> {
        let start_time = Instant::now();
        let mut heartbeat = interval(Duration::from_secs(30));
        heartbeat.tick().await; // Skip first immediate tick

        let mut child = cmd.spawn()?;

        // Take stdout and stderr for streaming
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_lines = Vec::new();
        let mut last_activity = Instant::now();
        let mut current_build_target = None::<String>;

        loop {
            tokio::select! {
                // Read stdout
                line_result = stdout_reader.next_line() => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();
                            debug!("nix stdout: {}", line);

                            // Check if this looks like an output path
                            if line.starts_with("/nix/store/") && !line.contains(".drv") {
                                output_lines.push(line);
                            }
                        }
                        None => break,
                    }
                }

                // Read stderr (where build logs go)
                line_result = stderr_reader.next_line() => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();

                            // Parse build status from nix output
                            if line.contains("building '") {
                                if let Some(start) = line.find("building '") {
                                    if let Some(end) = line[start + 10..].find("'") {
                                        current_build_target = Some(line[start + 10..start + 10 + end].to_string());
                                        info!("🔨 Started building: {}", current_build_target.as_ref().unwrap());
                                    }
                                }
                            } else if line.contains("built '") {
                                if let Some(start) = line.find("built '") {
                                    if let Some(end) = line[start + 7..].find("'") {
                                        let built_target = &line[start + 7..start + 7 + end];
                                        info!("✅ Completed building: {}", built_target);
                                        current_build_target = None;
                                    }
                                }
                            } else if line.contains("downloading '") || line.contains("fetching ") {
                                debug!("📥 {}", line);
                            } else if line.contains("error:") || line.contains("failed") {
                                error!("❌ Build error: {}", line);
                            }
                        }
                        None => break,
                    }
                }

                // Periodic heartbeat
                _ = heartbeat.tick() => {
                    let elapsed = start_time.elapsed();
                    let mins = elapsed.as_secs() / 60;
                    let secs = elapsed.as_secs() % 60;
                    let last_activity_secs = last_activity.elapsed().as_secs();

                    if let Some(ref target) = current_build_target {
                        info!("⏳ Still building {}: {}m {}s elapsed, last activity {}s ago",
                              target, mins, secs, last_activity_secs);
                    } else {
                        info!("⏳ Build in progress: {}m {}s elapsed, last activity {}s ago",
                              mins, secs, last_activity_secs);
                    }

                    // Database heartbeat update
                    let _ = crate::queries::derivations::update_derivation_build_status(
                        pool,
                        derivation_id,
                        elapsed.as_secs() as i32,
                        current_build_target.as_deref(),
                        last_activity_secs as i32
                    ).await;
                }
            }
        }

        // Wait for process to complete
        let status = child.wait().await?;

        if !status.success() {
            anyhow::bail!(
                "nix-store --realise failed with exit code: {}",
                status.code().unwrap_or(-1)
            );
        }

        // Find the output path
        let output_path = output_lines
            .into_iter()
            .find(|line| !line.contains(".drv"))
            .ok_or_else(|| anyhow::anyhow!("No output path found in nix-store output"))?;

        let elapsed = start_time.elapsed();
        info!(
            "✅ Built derivation {} -> {} (took {}m {}s)",
            drv_path,
            output_path,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60
        );

        // Clear build progress on completion
        let _ =
            crate::queries::derivations::clear_derivation_build_status(pool, derivation_id).await;

        Ok(output_path)
    }

    async fn run_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
        // Check if systemd should be used
        if !build_config.should_use_systemd() {
            info!("📋 Systemd disabled in config, using direct execution");
            return Self::run_direct_dry_run(flake_target, build_config).await;
        }

        // Try systemd-run scoped build first
        let mut scoped = build_config.systemd_scoped_cmd_base();
        scoped.args([flake_target, "--dry-run", "--print-out-paths", "--no-link"]);
        build_config.apply_to_command(&mut scoped);

        match scoped.output().await {
            Ok(output) => {
                if output.status.success() {
                    info!("✅ Systemd-scoped evaluation completed successfully");
                    Ok(output)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Check for systemd-specific issues that indicate fallback needed
                    if stderr.contains("Failed to connect to user scope bus")
                        || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
                        || stderr.contains("XDG_RUNTIME_DIR")
                        || stderr.contains("Failed to create scope")
                        || stderr.contains("systemd")
                    {
                        warn!(
                            "⚠️ Systemd scope creation failed (no user session), falling back to direct execution"
                        );
                        Self::run_direct_dry_run(flake_target, build_config).await
                    } else {
                        // Other failure - return the systemd error
                        Ok(output)
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("📋 systemd-run not available, using direct execution");
                Self::run_direct_dry_run(flake_target, build_config).await
            }
            Err(e) => {
                warn!("⚠️ systemd-run failed: {}, trying direct execution", e);
                Self::run_direct_dry_run(flake_target, build_config).await
            }
        }
    }

    async fn run_direct_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
        let mut direct = Command::new("nix");
        direct.args([
            "build",
            flake_target,
            "--dry-run",
            "--print-out-paths",
            "--no-link",
        ]);
        build_config.apply_to_command(&mut direct);
        Ok(direct.output().await?)
    }

    fn parse_derivation_paths(stderr: &str, flake_target: &str) -> Result<(String, Vec<String>)> {
        let mut derivation_paths = Vec::new();
        let mut collecting = false;

        for line in stderr.lines() {
            let t = line.trim();
            if t.starts_with("these ") && t.contains("derivations will be built:") {
                collecting = true;
                debug!("🔍 Found derivation list header: {}", t);
                continue;
            }
            if collecting {
                if t.starts_with("/nix/store/") && t.ends_with(".drv") {
                    derivation_paths.push(t.to_string());
                    debug!("🔍 Found derivation: {}", t);
                } else if t.is_empty() || t.starts_with("[{") {
                    collecting = false;
                    debug!("🔍 Stopped collecting derivations at: '{}'", t);
                }
            }
        }

        if derivation_paths.is_empty() {
            bail!("no-derivations");
        }

        let main = derivation_paths
            .iter()
            .find(|p| {
                if flake_target.contains("nixosConfigurations") {
                    p.contains("nixos-system")
                } else {
                    true
                }
            })
            .cloned()
            .ok_or_else(|| {
                error!("❌ Could not find main derivation. Available paths:");
                for path in &derivation_paths {
                    error!("  - {}", path);
                }
                anyhow!(
                    "Could not determine main derivation from {} candidates",
                    derivation_paths.len()
                )
            })?;

        let deps = derivation_paths
            .into_iter()
            .filter(|p| p != &main)
            .collect::<Vec<_>>();

        Ok((main, deps))
    }

    /// Phase 1: Evaluate and discover all derivation paths (dry-run only)
    /// This can run on any machine and doesn't require commit_id
    pub async fn dry_run_derivation_path(
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<EvaluationResult> {
        info!("🔍 Evaluating derivation paths for: {}", flake_target);

        // robust & deterministic: never scrape stderr, never depend on dry-run text
        let main_drv = eval_main_drv_path(flake_target, build_config).await?;
        let deps = list_immediate_input_drvs(&main_drv, build_config).await?;

        info!("🔍 main drv: {main_drv}");
        info!("🔍 {} immediate input drvs", deps.len());

        Ok(EvaluationResult {
            main_derivation_path: main_drv,
            dependency_derivation_paths: deps,
        })
    }

    /// Phase 2: Build a derivation from its .drv path
    /// This can run on any machine that has access to the required inputs
    pub async fn build_derivation_from_path(
        &self,
        pool: &PgPool,
        drv_path: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        info!("🔨 Building derivation: {}", drv_path);

        if !drv_path.ends_with(".drv") {
            anyhow::bail!("Expected .drv path, got: {}", drv_path);
        }

        // Check if systemd should be used
        if !build_config.should_use_systemd() {
            info!("📋 Systemd disabled in config, using direct execution for nix-store");
            return Self::run_direct_nix_store_streaming(
                &self,
                drv_path,
                build_config,
                self.id,
                pool,
            )
            .await;
        }

        // Try systemd-run scoped build first
        let mut scoped = Command::new("systemd-run");
        scoped.args(["--scope", "--collect", "--quiet"]);

        // Add systemd properties from config
        if let Some(ref memory_max) = build_config.systemd_memory_max {
            scoped.args(["--property", &format!("MemoryMax={}", memory_max)]);
        }
        if let Some(cpu_quota) = build_config.systemd_cpu_quota {
            scoped.args(["--property", &format!("CPUQuota={}%", cpu_quota)]);
        }
        if let Some(timeout_stop) = build_config.systemd_timeout_stop_sec {
            scoped.args(["--property", &format!("TimeoutStopSec={}", timeout_stop)]);
        }
        for property in &build_config.systemd_properties {
            scoped.args(["--property", property]);
        }

        scoped.args(["--", "nix-store", "--realise", drv_path]);
        scoped.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut scoped);

        match Self::run_streaming_build(scoped, drv_path, self.id, pool).await {
            Ok(output_path) => Ok(output_path),
            Err(e) => {
                let error_msg = e.to_string();
                // Check for systemd-specific issues that indicate fallback needed
                if error_msg.contains("Failed to connect to user scope bus")
                    || error_msg.contains("DBUS_SESSION_BUS_ADDRESS")
                    || error_msg.contains("XDG_RUNTIME_DIR")
                    || error_msg.contains("Failed to create scope")
                    || error_msg.contains("Interactive authentication required")
                    || error_msg.contains("systemd")
                {
                    warn!(
                        "⚠️ Systemd scope creation failed, falling back to direct execution: {}",
                        error_msg
                    );

                    Self::run_direct_nix_store_streaming(
                        &self,
                        drv_path,
                        build_config,
                        self.id,
                        pool,
                    )
                    .await
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn run_direct_nix_store_streaming(
        &self,
        drv_path: &str,
        build_config: &BuildConfig,
        derivation_id: i32,
        pool: &PgPool,
    ) -> Result<String> {
        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut cmd);

        Self::run_streaming_build(cmd, drv_path, derivation_id, pool).await
    }

    async fn run_direct_nix_store(drv_path: &str, build_config: &BuildConfig) -> Result<String> {
        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);

        // Add this line to apply auth configuration
        build_config.apply_to_command(&mut cmd);

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("❌ nix-store --realise failed: {}", stderr.trim());
            anyhow::bail!("nix-store --realise failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_path = stdout.trim();

        if output_path.is_empty() {
            anyhow::bail!("No output path returned from nix-store --realise");
        }

        info!("✅ Built derivation {} -> {}", drv_path, output_path);
        Ok(output_path.to_string())
    }

    /// High-level function that handles both NixOS and Package derivations
    pub async fn evaluate_and_build(
        &self,
        pool: &PgPool,
        full_build: bool,
        build_config: &BuildConfig,
    ) -> Result<String> {
        match self.derivation_type {
            DerivationType::NixOS => {
                // NixOS derivations need commit_id for flake evaluation
                let commit_id = self.commit_id.ok_or_else(|| {
                    anyhow::anyhow!("Cannot build NixOS derivation without commit_id")
                })?;

                // Use the provided pool instead of creating a new one
                let commit = crate::queries::commits::get_commit_by_id(pool, commit_id).await?;
                let flake = crate::queries::flakes::get_flake_by_id(pool, commit.flake_id).await?;
                let flake_target = self.build_flake_target_string(&flake, &commit).await?;

                if full_build {
                    // Phase 1: Evaluate
                    let eval_result =
                        Self::dry_run_derivation_path(&flake_target, build_config).await?;
                    // Insert dependencies into database
                    if !eval_result.dependency_derivation_paths.is_empty() {
                        crate::queries::derivations::discover_and_insert_packages(
                            pool, // Use provided pool
                            self.id,
                            &eval_result
                                .dependency_derivation_paths
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                        )
                        .await?;
                    }
                    // Phase 2: Build the main derivation
                    if eval_result.main_derivation_path.ends_with(".drv") {
                        Self::build_derivation_from_path(
                            &self,
                            &pool,
                            &eval_result.main_derivation_path,
                            build_config,
                        )
                        .await
                    } else {
                        // Already built, return the output path
                        Ok(eval_result.main_derivation_path)
                    }
                } else {
                    // Dry-run only
                    let eval_result =
                        Self::dry_run_derivation_path(&flake_target, build_config).await?;
                    // Insert dependencies into database
                    if !eval_result.dependency_derivation_paths.is_empty() {
                        crate::queries::derivations::discover_and_insert_packages(
                            pool, // Use provided pool
                            self.id,
                            &eval_result
                                .dependency_derivation_paths
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                        )
                        .await?;
                    }
                    Ok(eval_result.main_derivation_path)
                }
            }
            DerivationType::Package => {
                // Package derivations just need their .drv path built
                let drv_path = self
                    .derivation_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Package derivation missing derivation_path"))?;
                if full_build {
                    // Build the package from its .drv path
                    Self::build_derivation_from_path(&self, &pool, drv_path, build_config).await
                } else {
                    // For dry-run, just return the existing .drv path
                    Ok(drv_path.clone())
                }
            }
        }
    }

    /// Helper to build the flake target string
    async fn build_flake_target_string(
        &self,
        flake: &crate::models::flakes::Flake,
        commit: &crate::models::commits::Commit,
    ) -> Result<String> {
        let system = &self.derivation_name;
        // Always use a git flake ref pinned to the commit, even for local repos.
        let flake_ref = if Path::new(&flake.repo_url).exists() {
            let abs = std::fs::canonicalize(&flake.repo_url)
                .with_context(|| format!("canonicalize {}", flake.repo_url))?;
            format!(
                "git+file://{}?rev={}",
                abs.display(),
                commit.git_commit_hash
            )
        } else if flake.repo_url.starts_with("git+") {
            // ensure we pin a rev (append if not already present)
            if flake.repo_url.contains("?rev=") {
                flake.repo_url.clone()
            } else {
                format!("{}?rev={}", flake.repo_url, commit.git_commit_hash)
            }
        } else {
            // Check if URL already has query parameters to avoid malformed URLs
            let separator = if flake.repo_url.contains('?') {
                "&"
            } else {
                "?"
            };
            format!(
                "git+{}{separator}rev={}",
                flake.repo_url, commit.git_commit_hash
            )
        };
        Ok(format!(
            "{flake_ref}#nixosConfigurations.{system}.config.system.build.toplevel"
        ))
    }

    /// Resolve a .drv path to its output store path(s)
    pub async fn resolve_drv_to_store_path(drv_path: &str) -> Result<String> {
        if !drv_path.ends_with(".drv") {
            // Already a store path, return as-is
            return Ok(drv_path.to_string());
        }

        let output = Command::new("nix-store")
            .args(["--query", "--outputs", drv_path])
            .output()
            .await
            .context("Failed to execute nix-store --query --outputs")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("nix-store --query --outputs failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let store_paths: Vec<&str> = stdout.trim().lines().collect();

        if store_paths.is_empty() {
            anyhow::bail!("No output paths found for derivation: {}", drv_path);
        }

        // Return the first output path (typically there's only one)
        Ok(store_paths[0].to_string())
    }

    /// Push a store path to the configured cache. Includes robust Attic handling:
    /// - resolves .drv -> output path
    /// - ensures a fresh login every time
    /// - retries once on 401 Unauthorized by redoing login

    pub async fn push_to_cache(
        &self,
        path: &str,
        cache_config: &CacheConfig,
        build_config: &BuildConfig,
    ) -> Result<()> {
        use tokio::process::Command;

        if !cache_config.should_push(&self.derivation_name) {
            info!("⭐️ Skipping cache push for {}", self.derivation_name);
            return Ok(());
        }

        // Resolve .drv -> store path if needed
        let store_path = if path.ends_with(".drv") {
            info!("🔍 Resolving derivation path to store path: {}", path);
            Self::resolve_drv_to_store_path(path).await?
        } else {
            path.to_string()
        };

        // Get command and args from config
        let cache_cmd = match cache_config.cache_command(&store_path) {
            Some(cmd) => cmd,
            None => {
                warn!("⚠️ No cache push configuration found, skipping cache push");
                return Ok(());
            }
        };

        let mut effective_command = cache_cmd.command.clone();
        let mut effective_args = cache_cmd.args.clone();

        // --- Special handling for Attic -------------------------------------------------------
        if effective_command == "attic"
            && effective_args.first().map(|s| s.as_str()) == Some("push")
        {
            let endpoint = std::env::var("ATTIC_SERVER_URL")
                .context("ATTIC_SERVER_URL not set (e.g. http://atticCache:8080)")?;
            let token = std::env::var("ATTIC_TOKEN")
                .context("ATTIC_TOKEN not set (provide a token with push permission)")?;
            let remote = std::env::var("ATTIC_REMOTE_NAME").unwrap_or_else(|_| "local".to_string());

            // Ensure remote:repo format in arg[1]
            if effective_args.len() >= 2 && !effective_args[1].contains(':') {
                effective_args[1] = format!("{}:{}", remote, effective_args[1]);
            }

            // Ensure the store path is present (some configs might omit it)
            if !effective_args.iter().any(|a| a == &store_path) {
                effective_args.push(store_path.clone());
            }

            // Helpful: log environment presence and file-based config once
            debug_attic_environment();

            // One-time login (per-process), persisted under /var/lib/crystal-forge
            ensure_attic_login(&remote, &endpoint, &token).await?;

            info!(
                "📤 Pushing {} to cache... ({} {})",
                store_path,
                effective_command,
                effective_args.join(" ")
            );

            // Preflight: whoami
            {
                let mut whoami = tokio::process::Command::new("attic");
                whoami.arg("whoami");
                whoami.env("HOME", "/var/lib/crystal-forge");
                whoami.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                apply_cache_env_to_command(&mut whoami);
                if let Ok(out) = whoami.output().await {
                    let s = String::from_utf8_lossy(&out.stdout);
                    info!("attic whoami: {}", s.trim());
                }
            }

            // Preflight: repo visibility
            {
                let mut info_cmd = tokio::process::Command::new("attic");
                info_cmd.args([
                    "cache",
                    "info",
                    &effective_args[1], /* e.g. local:test */
                ]);
                info_cmd.env("HOME", "/var/lib/crystal-forge");
                info_cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                apply_cache_env_to_command(&mut info_cmd);
                if let Ok(out) = info_cmd.output().await {
                    if !out.status.success() {
                        warn!(
                            "Preflight 'attic cache info {}' failed: {}",
                            &effective_args[1],
                            String::from_utf8_lossy(&out.stderr).trim()
                        );
                    }
                }
            }

            // ---- First attempt (this was missing) ----
            let mut cmd = tokio::process::Command::new("attic");
            cmd.args(&effective_args);
            cmd.env("HOME", "/var/lib/crystal-forge");
            cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
            apply_cache_env_to_command(&mut cmd);

            let mut output = cmd.output().await.context("Failed to run 'attic push'")?;

            // ---- If unauthorized, redo login once and retry
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let trimmed = stderr.trim();
                if trimmed.contains("Unauthorized")
                    || trimmed.contains("401")
                    || trimmed.contains("invalid token")
                {
                    warn!("🔐 Attic push returned 401; clearing login cache and retrying once…");
                    clear_attic_logged(&remote);

                    // Re-login with current env
                    let endpoint = std::env::var("ATTIC_SERVER_URL")?;
                    let token = std::env::var("ATTIC_TOKEN")?;
                    ensure_attic_login(&remote, &endpoint, &token).await?;

                    // Retry push
                    let mut cmd2 = tokio::process::Command::new("attic");
                    cmd2.args(&effective_args);
                    cmd2.env("HOME", "/var/lib/crystal-forge");
                    cmd2.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                    apply_cache_env_to_command(&mut cmd2);
                    output = cmd2
                        .output()
                        .await
                        .context("Failed to run 'attic push' (retry)")?;
                }
            }

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("❌ attic (direct) failed: {}", stderr.trim());
                anyhow::bail!("attic failed (direct): {}", stderr.trim());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                info!("📤 attic output: {}", stdout.trim());
            }
            info!("✅ Successfully pushed {} to cache (attic)", store_path);
            return Ok(());
        }
        // --- End Attic special-case ----------------------------------------------------------

        // Non-Attic tools (e.g. `nix copy --to ...`)
        if build_config.should_use_systemd() {
            let mut scoped = Command::new("systemd-run");
            scoped.args(["--scope", "--collect", "--quiet"]);
            apply_systemd_props_for_scope(build_config, &mut scoped);
            apply_cache_env(&mut scoped);
            scoped
                .arg("--")
                .arg(&effective_command)
                .args(&effective_args);

            let output = scoped
                .output()
                .await
                .context("Failed to execute scoped cache command")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "❌ {} (scoped) failed: {}",
                    effective_command,
                    stderr.trim()
                );
                anyhow::bail!("{} failed (scoped): {}", effective_command, stderr.trim());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                info!("📤 {} output: {}", effective_command, stdout.trim());
            }
            info!("✅ Successfully pushed {} to cache (scoped)", store_path);
            return Ok(());
        }

        // Direct execution for non-Attic
        let mut cmd = Command::new(&effective_command);
        cmd.args(&effective_args);
        build_config.apply_to_command(&mut cmd);
        apply_cache_env_to_command(&mut cmd);

        let output = cmd
            .output()
            .await
            .context("Failed to execute cache command")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("❌ {} failed: {}", effective_command, stderr.trim());
            anyhow::bail!("{} failed: {}", effective_command, stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("📤 {} output: {}", effective_command, stdout.trim());
        }
        info!("✅ Successfully pushed {} to cache", store_path);
        Ok(())
    }

    /// Evaluates and optionally pushes to cache in one go
    pub async fn evaluate_and_push_to_cache(
        &self,
        pool: &PgPool,
        full_build: bool,
        build_config: &BuildConfig,
        cache_config: &CacheConfig,
    ) -> Result<String> {
        let store_path: String = self
            .evaluate_and_build(pool, full_build, build_config)
            .await?;

        // Only push to cache if we did a full build (not a dry-run)
        if full_build && cache_config.push_after_build {
            if let Err(e) = self
                .push_to_cache(&store_path, cache_config, build_config)
                .await
            {
                warn!("⚠️ Cache push failed but continuing: {}", e);
                // Don't fail the whole operation if cache push fails
            }
        }

        Ok(store_path)
    }
}

/// Parse derivation path to extract package information
pub fn parse_derivation_path(drv_path: &str) -> Option<PackageInfo> {
    // Derivation paths look like: /nix/store/hash-name-version.drv
    let filename = drv_path.split('/').last()?.strip_suffix(".drv")?;

    // Split by first dash to separate hash from name-version
    let (_, name_version) = filename.split_once('-')?;

    // Handle NixOS system derivations specifically
    if name_version.starts_with("nixos-system-") {
        // Pattern: nixos-system-HOSTNAME-VERSION
        // Example: nixos-system-aws-test-25.05.20250802.b6bab62
        let system_part = name_version.strip_prefix("nixos-system-")?;

        // Find the last dash to separate hostname from version
        if let Some((hostname, version)) = system_part.rsplit_once('-') {
            // Check if the last part looks like a version (contains dots or digits)
            if version.chars().any(|c| c.is_ascii_digit() || c == '.') {
                return Some(PackageInfo {
                    pname: Some(format!("nixos-system-{}", hostname)),
                    version: Some(version.to_string()),
                });
            }
        }

        // If we can't parse version, use the whole system part as pname
        return Some(PackageInfo {
            pname: Some(format!("{}", system_part)),
            version: None,
        });
    }

    // Handle regular packages
    // Try to split name and version
    // This is heuristic - Nix derivation naming isn't perfectly consistent
    if let Some((name, version)) = name_version.rsplit_once('-') {
        // Check if the last part looks like a version (starts with digit)
        if version.chars().next()?.is_ascii_digit() {
            return Some(PackageInfo {
                pname: Some(name.to_string()),
                version: Some(version.to_string()),
            });
        }
    }

    // If we can't parse version, use the whole name_version as pname
    Some(PackageInfo {
        pname: Some(name_version.to_string()),
        version: None,
    })
}

async fn eval_main_drv_path(flake_target: &str, build_config: &BuildConfig) -> Result<String> {
    // Example: "<flake>#…config.system.build.toplevel" -> ask Nix for ".drvPath"
    let mut cmd = Command::new("nix");
    cmd.args(["eval", "--raw", &format!("{flake_target}.drvPath")]);
    build_config.apply_to_command(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn list_immediate_input_drvs(
    drv_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<String>> {
    // `nix derivation show <drv>` yields {"<drv>": {"inputDrvs": {"<drv2>": ["out"], …}, …}}
    let mut cmd = Command::new("nix");
    cmd.args(["derivation", "show", drv_path]);
    build_config.apply_to_command(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "nix derivation show failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    let obj = v
        .as_object()
        .and_then(|m| m.get(drv_path))
        .and_then(|x| x.as_object())
        .ok_or_else(|| anyhow!("bad JSON from nix derivation show"))?;

    let deps = obj
        .get("inputDrvs")
        .and_then(|x| x.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_else(Vec::new);

    Ok(deps)
}

#[derive(Debug)]
pub struct PackageInfo {
    pub pname: Option<String>,
    pub version: Option<String>,
}

// For backward compatibility during migration
pub type EvaluationTarget = Derivation;
pub type TargetType = DerivationType;

fn apply_systemd_props_for_scope(build: &BuildConfig, cmd: &mut tokio::process::Command) {
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
fn apply_cache_env(scoped: &mut Command) {
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

/// Log into Attic so the remote is available to the client.
/// Always runs *directly* and writes config under /var/lib/crystal-forge.
async fn ensure_attic_login(remote: &str, endpoint: &str, token: &str) -> anyhow::Result<()> {
    if is_attic_logged(remote) {
        tracing::debug!(
            "attic: remote '{}' already initialized in this process",
            remote
        );
        return Ok(());
    }

    tracing::info!("🔐 Attic login for remote '{remote}' at {endpoint}");
    let mut cmd = tokio::process::Command::new("attic");
    cmd.args(["login", remote, endpoint, token]);
    // Ensure credentials are persisted under the crystal-forge account:
    cmd.env("HOME", "/var/lib/crystal-forge");
    cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");

    // If you also want AWS/S3 env available for any follow-up calls attic might make:
    apply_cache_env_to_command(&mut cmd);

    let out = cmd.output().await.context("failed to run 'attic login'")?;
    if !out.status.success() {
        let se = String::from_utf8_lossy(&out.stderr);
        // Treat "already exists/already configured" as success
        if se.contains("exist") || se.contains("Already") || se.contains("already") {
            tracing::info!("ℹ️ Attic remote '{remote}' already configured");
            mark_attic_logged(remote);
            return Ok(());
        }
        anyhow::bail!("attic login failed: {}", se.trim());
    }

    mark_attic_logged(remote);
    Ok(())
}
