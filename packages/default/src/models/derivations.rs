use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig};
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
    pub parent_derivation_id: Option<i32>, // for hierarchical relationships (packages → systems)
    pub pname: Option<String>,             // Nix package name (for packages)
    pub version: Option<String>,           // package version (for packages)
    pub status_id: i32,                    // foreign key to derivation_statuses table
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
        let commit_hash = &commit.git_commit_hash;
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
        }
        cmd.arg("--json");

        // Apply build configuration
        build_config.apply_to_command(&mut cmd);

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("❌ nix build failed: {}", stderr.trim());
            anyhow::bail!("nix build failed: {}", stderr.trim());
        }

        let parsed: Value = serde_json::from_slice(&output.stdout)?;
        let hash = parsed
            .get(0)
            .and_then(|v| v["outputs"]["out"].as_str())
            .context("❌ Missing derivation output in nix build output")?
            .to_string();

        info!(
            "✅ {} path for {system}: {hash}",
            if full_build {
                "Built output"
            } else {
                "Derivation"
            }
        );
        Ok(hash)
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

// For backward compatibility during migration
pub type EvaluationTarget = Derivation;
pub type TargetType = DerivationType;
