use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig};
use crate::queries::derivations::discover_and_insert_packages;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use std::path::Path;
use std::process::Output;
use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

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
    pub async fn evaluate_derivation_path(
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<EvaluationResult> {
        info!("🔍 Evaluating derivation paths for: {}", flake_target);

        let output = Self::run_dry_run(flake_target, build_config).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            error!("❌ nix build dry-run failed: {}", stderr.trim());

            // Provide more context for common failures
            if stderr.contains("out of memory") {
                bail!(
                    "nix build failed due to insufficient memory: {}",
                    stderr.trim()
                );
            } else if stderr.contains("timeout") {
                bail!("nix build timed out: {}", stderr.trim());
            } else {
                bail!("nix build dry-run failed: {}", stderr.trim());
            }
        }

        debug!("🔍 Dry-run stdout:\n{}", stdout);
        debug!("🔍 Dry-run stderr:\n{}", stderr);

        match Self::parse_derivation_paths(&stderr, flake_target) {
            Ok((main, deps)) => {
                info!("🔍 Main derivation: {}", main);
                info!("🔍 Found {} dependency derivations", deps.len());
                Ok(EvaluationResult {
                    main_derivation_path: main,
                    dependency_derivation_paths: deps,
                })
            }
            Err(e) if e.to_string() == "no-derivations" => {
                info!("📦 No new derivations needed - everything already available");
                let store_output = Self::run_print_out_paths(flake_target, build_config).await?;
                if !store_output.status.success() {
                    let se = String::from_utf8_lossy(&store_output.stderr);
                    bail!("nix build for store path failed: {}", se.trim());
                }
                let store_stdout = String::from_utf8_lossy(&store_output.stdout);
                let store_path = store_stdout.trim().to_string();

                Ok(EvaluationResult {
                    main_derivation_path: store_path,
                    dependency_derivation_paths: Vec::new(),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Phase 2: Build a derivation from its .drv path
    /// This can run on any machine that has access to the required inputs
    pub async fn build_derivation_from_path(
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
            return Self::run_direct_nix_store(drv_path, build_config).await;
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

        match scoped.output().await {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let output_path = stdout.trim();

                    if output_path.is_empty() {
                        anyhow::bail!("No output path returned from nix-store --realise");
                    }

                    info!("✅ Built derivation {} -> {}", drv_path, output_path);
                    Ok(output_path.to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Check for systemd-specific issues that indicate fallback needed
                    if stderr.contains("Failed to connect to user scope bus")
                        || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
                        || stderr.contains("XDG_RUNTIME_DIR")
                        || stderr.contains("Failed to create scope")
                        || stderr.contains("Interactive authentication required")
                        || stderr.contains("systemd")
                    {
                        warn!(
                            "⚠️ Systemd scope creation failed, falling back to direct execution: {}",
                            stderr.trim()
                        );
                        Self::run_direct_nix_store(drv_path, build_config).await
                    } else {
                        // Other failure - return the systemd error
                        error!("❌ nix-store --realise failed: {}", stderr.trim());
                        anyhow::bail!("nix-store --realise failed: {}", stderr.trim());
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("📋 systemd-run not available for nix-store, using direct execution");
                Self::run_direct_nix_store(drv_path, build_config).await
            }
            Err(e) => {
                warn!(
                    "⚠️ systemd-run failed for nix-store: {}, trying direct execution",
                    e
                );
                Self::run_direct_nix_store(drv_path, build_config).await
            }
        }
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
                let flake_target = self.build_flake_target(&flake, &commit).await?;

                if full_build {
                    // Phase 1: Evaluate
                    let eval_result =
                        Self::evaluate_derivation_path(&flake_target, build_config).await?;
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
                        Self::evaluate_derivation_path(&flake_target, build_config).await?;
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
                    Self::build_derivation_from_path(drv_path, build_config).await
                } else {
                    // For dry-run, just return the existing .drv path
                    Ok(drv_path.clone())
                }
            }
        }
    }

    /// Helper to build the flake target string
    async fn build_flake_target(
        &self,
        flake: &crate::models::flakes::Flake,
        commit: &crate::models::commits::Commit,
    ) -> Result<String> {
        let flake_url = &flake.repo_url;
        let system = &self.derivation_name;
        let is_path = Path::new(flake_url).exists();

        if is_path {
            // For local paths, checkout first
            info!(
                "📌 Running 'git checkout {}' in {}",
                commit.git_commit_hash, flake_url
            );
            let status = tokio::process::Command::new("git")
                .args(["-C", flake_url, "checkout", &commit.git_commit_hash])
                .status()
                .await?;
            if !status.success() {
                anyhow::bail!(
                    "❌ git checkout failed for path {} at rev {}",
                    flake_url,
                    commit.git_commit_hash
                );
            }
            Ok(format!(
                "{flake_url}#nixosConfigurations.{system}.config.system.build.toplevel"
            ))
        } else if flake_url.starts_with("git+") {
            Ok(format!(
                "{flake_url}?rev={}#nixosConfigurations.{system}.config.system.build.toplevel",
                commit.git_commit_hash
            ))
        } else {
            Ok(format!(
                "git+{0}?rev={1}#nixosConfigurations.{2}.config.system.build.toplevel",
                flake_url, commit.git_commit_hash, self.derivation_name
            ))
        }
    }
    /// Pushes a built derivation to the configured cache
    pub async fn push_to_cache(
        &self,
        store_path: &str,
        cache_config: &CacheConfig,
        build_config: &BuildConfig,
    ) -> Result<()> {
        // Check if we should push this target
        if !cache_config.should_push(&self.derivation_name) {
            info!(
                "⏭️ Skipping cache push for {} (filtered out)",
                self.derivation_name
            );
            return Ok(());
        }

        // Get the nix copy command arguments
        let args = match cache_config.copy_command_args(store_path) {
            Some(args) => args,
            None => {
                warn!("⚠️ No cache push configuration found, skipping cache push");
                return Ok(());
            }
        };

        info!("📤 Pushing {} to cache...", store_path);
        info!("🔧 nix copy command: nix {}", args.join(" "));

        let mut cmd = Command::new("nix");
        cmd.args(&args);

        build_config.apply_to_command(&mut cmd);

        let output = cmd
            .output()
            .await
            .context("Failed to execute nix copy command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("❌ nix copy failed: {}", stderr.trim());
            anyhow::bail!("nix copy failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("📤 nix copy output: {}", stdout.trim());
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
            if let Err(e) = self.push_to_cache(&store_path, cache_config).await {
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

#[derive(Debug)]
pub struct PackageInfo {
    pub pname: Option<String>,
    pub version: Option<String>,
}

// For backward compatibility during migration
pub type EvaluationTarget = Derivation;
pub type TargetType = DerivationType;
