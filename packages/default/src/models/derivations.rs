use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig};
use crate::queries::derivations::discover_and_insert_packages;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use std::path::Path;
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

// SQLx requires this for deserialization - make it safe
impl From<String> for DerivationType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "nixos" => DerivationType::NixOS,
            "package" => DerivationType::Package,
            _ => {
                // Log the error but provide a default instead of panicking
                eprintln!(
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

    pub async fn resolve_derivation_path(&mut self, pool: &PgPool) -> Result<String> {
        // Only NixOS derivations should resolve paths, packages are discovered
        if self.derivation_type == DerivationType::Package {
            return Err(anyhow::anyhow!("Package derivations don't resolve paths"));
        }

        if self.commit_id.is_none() {
            return Err(anyhow::anyhow!(
                "Cannot resolve path for derivation without commit"
            ));
        }

        let name = self.derivation_name.clone();
        let kind = self.derivation_type.clone();

        info!(
            "🔍 Starting evaluation of {} target '{}'",
            kind.to_string().to_lowercase(),
            name
        );

        let (tx, mut rx) = watch::channel(false);

        let ticker = tokio::spawn({
            let name = name.clone();
            let kind = kind.clone();
            async move {
                loop {
                    sleep(Duration::from_secs(60)).await;
                    if *rx.borrow_and_update() {
                        break;
                    }
                    info!(
                        "⏳ Still evaluating {} '{}'",
                        kind.to_string().to_lowercase(),
                        name
                    );
                }
            }
        });

        let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
            warn!("Failed to load Crystal Forge config: {}, using defaults", e);
            CrystalForgeConfig::default()
        });
        let build_config = cfg.get_build_config();

        let hash = match self.derivation_type {
            DerivationType::NixOS => {
                self.evaluate_nixos_system_with_build(false, &build_config)
                    .await?
            }
            DerivationType::Package => {
                anyhow::bail!("Package derivation evaluation not implemented yet")
            }
        };

        self.derivation_path = Some(hash.clone());

        let _ = tx.send(true);
        let _ = ticker.await;

        Ok(hash)
    }

    pub async fn evaluate_nixos_system_with_build(
        &self,
        full_build: bool,
        build_config: &BuildConfig,
    ) -> Result<String> {
        // Require commit_id for NixOS builds
        let commit_id = self
            .commit_id
            .ok_or_else(|| anyhow::anyhow!("Cannot build NixOS derivation without commit_id"))?;

        let summary = self.summary().await?;
        info!("🔍 Determining derivation for {summary}");
        let pool = CrystalForgeConfig::db_pool().await?;
        let commit = crate::queries::commits::get_commit_by_id(&pool, commit_id).await?;
        let flake = crate::queries::flakes::get_flake_by_id(&pool, commit.flake_id).await?;
        let flake_url = &flake.repo_url;
        let system = &self.derivation_name;
        let is_path = Path::new(flake_url).exists();
        debug!("📁 Is local path: {is_path}");

        if is_path {
            info!("📌 Running 'git checkout {:?}' in {}", commit, flake_url);
            let status = Command::new("git")
                .args(["-C", flake_url, "checkout", &commit.git_commit_hash])
                .status()
                .await?;
            if !status.success() {
                anyhow::bail!(
                    "❌ git checkout failed for path {} at rev {}",
                    flake_url,
                    commit
                );
            }
        }

        let flake_target = if is_path {
            format!("{flake_url}#nixosConfigurations.{system}.config.system.build.toplevel")
        } else if flake_url.starts_with("git+") {
            format!(
                "{flake_url}?rev={}#nixosConfigurations.{system}.config.system.build.toplevel",
                commit.git_commit_hash
            )
        } else {
            format!(
                "git+{0}?rev={1}#nixosConfigurations.{2}.config.system.build.toplevel",
                flake_url, commit.git_commit_hash, self.derivation_name
            )
        };

        let build_mode = if full_build { "full build" } else { "dry-run" };
        info!("🔨 Building flake target: {flake_target} ({})", build_mode);

        let mut cmd = Command::new("nix");
        cmd.args(["build", &flake_target]);

        if !full_build {
            cmd.arg("--dry-run");
            cmd.arg("--print-out-paths");
            // NOTE: Don't add --json for dry-run, it conflicts with --dry-run output format
        } else {
            cmd.arg("--json");
        }

        // Apply build configuration
        build_config.apply_to_command(&mut cmd);

        let output = cmd.output().await?;

        // Always capture both stdout and stderr for debugging
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            error!("❌ nix build failed: {}", stderr.trim());
            anyhow::bail!("nix build failed: {}", stderr.trim());
        }

        let main_derivation_path = if full_build {
            // Full build: parse JSON output
            let parsed: Value = serde_json::from_slice(&output.stdout)?;
            parsed
                .get(0)
                .and_then(|v| v["outputs"]["out"].as_str())
                .context("❌ Missing derivation output in nix build output")?
                .to_string()
        } else {
            // Dry run: parse text output for derivation paths
            info!("🔍 Full dry-run stdout:\n{}", stdout);
            info!("🔍 Full dry-run stderr:\n{}", stderr);

            // Parse the stderr output: collect all derivation paths (Nix outputs the build list to stderr)
            let mut derivation_paths = Vec::new();
            let mut collecting_derivations = false;

            for line in stderr.lines() {
                let trimmed = line.trim();

                // Look for the start of derivation list
                if trimmed.starts_with("these ") && trimmed.contains("derivations will be built:") {
                    collecting_derivations = true;
                    debug!("🔍 Found derivation list header: {}", trimmed);
                    continue;
                }

                // If we're collecting and find a derivation path
                if collecting_derivations {
                    if trimmed.starts_with("/nix/store/") && trimmed.ends_with(".drv") {
                        derivation_paths.push(trimmed);
                        debug!("🔍 Found derivation: {}", trimmed);
                    } else if trimmed.is_empty() || trimmed.starts_with("[{") {
                        // Stop when we hit empty line or JSON section
                        collecting_derivations = false;
                        debug!("🔍 Stopped collecting derivations at: '{}'", trimmed);
                    }
                }
            }

            info!("🔍 Found {} derivation paths", derivation_paths.len());

            if derivation_paths.is_empty() {
                // If no derivations found, this means everything is already built/cached
                // We need to get the actual store path instead
                info!("📦 No new derivations needed - everything already available");

                // Run without --dry-run but still with --print-out-paths to get the store path
                let mut store_cmd = Command::new("nix");
                store_cmd.args(["build", &flake_target, "--print-out-paths"]);
                build_config.apply_to_command(&mut store_cmd);

                let store_output = store_cmd.output().await?;
                if !store_output.status.success() {
                    let store_stderr = String::from_utf8_lossy(&store_output.stderr);
                    error!(
                        "❌ nix build for store path failed: {}",
                        store_stderr.trim()
                    );
                    anyhow::bail!("nix build for store path failed: {}", store_stderr.trim());
                }

                let store_stdout = String::from_utf8_lossy(&store_output.stdout);
                let store_path = store_stdout.trim();

                if store_path.is_empty() {
                    anyhow::bail!("No store path returned from nix build");
                }

                info!("✅ Using existing store path: {}", store_path);
                return Ok(store_path.to_string());
            }

            // Look for the main system derivation
            let main_path = derivation_paths
                .iter()
                .find(|path| {
                    let contains_nixos_system = path.contains("nixos-system");
                    let contains_system_name = path.contains(&self.derivation_name);
                    debug!(
                        "🔍 Checking path {}: nixos-system={}, system-name={}",
                        path, contains_nixos_system, contains_system_name
                    );
                    contains_nixos_system || contains_system_name
                })
                .ok_or_else(|| {
                    error!("❌ Could not find main system derivation. Available paths:");
                    for path in &derivation_paths {
                        error!("  - {}", path);
                    }
                    anyhow::anyhow!("Could not find main system derivation in dry-run output")
                })?;

            info!("🔍 Selected main derivation: {}", main_path);

            // Insert discovered packages into the database
            if !derivation_paths.is_empty() {
                info!(
                    "🔍 Discovering packages from {} derivations",
                    derivation_paths.len()
                );
                crate::queries::derivations::discover_and_insert_packages(
                    &pool,
                    self.id,
                    &derivation_paths,
                )
                .await?;
            }

            main_path.to_string()
        };

        info!(
            "✅ {} path for {system}: {main_derivation_path}",
            if full_build {
                "Built output"
            } else {
                "Derivation"
            }
        );

        Ok(main_derivation_path)
    }

    /// Pushes a built derivation to the configured cache
    pub async fn push_to_cache(&self, store_path: &str, cache_config: &CacheConfig) -> Result<()> {
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
        full_build: bool,
        build_config: &BuildConfig,
        cache_config: &CacheConfig,
    ) -> Result<String> {
        let store_path: String = self
            .evaluate_nixos_system_with_build(full_build, build_config)
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
