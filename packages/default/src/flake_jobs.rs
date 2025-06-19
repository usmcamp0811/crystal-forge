use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

type SystemEvalQueue = Arc<Mutex<VecDeque<Job>>>;

struct Flake {
    flake_name: String,
    flake_url: String,
    commit: String,
    systems: Vec<FlakeSystem>,
    commit_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct FlakeTarget {
    flake_name: String,
    flake_url: String,
    commit: String,
    target_name: Option<String>,
    target_type: TargetType, // ← new field
    enqueued_at: DateTime<Utc>,
    derivation_path: Option<String>,
}

enum TargetType {
    NixOS,
    HomeManager,
    // Future: Package, App, etc.
}

impl FlakeTarget {
    pub fn summary(&self) -> String {
        format!(
            "{}@{} ({})",
            self.flake_name,
            self.commit,
            self.system_name.as_deref().unwrap_or("unknown")
        )
    }
    pub async fn resolve_derivation_path(&mut self) -> Result<()> {
        let hash = match self.target_type {
            TargetType::NixOS => self.evaluate_nixos_system().await?,
            TargetType::HomeManager => {
                anyhow::bail!("Home Manager evaluation not implemented yet");
            }
        };
        self.derivation_path = Some(hash);
        Ok(())
    }
    /// Returns the derivation output path (hash) for a specific NixOS system from a given flake.
    /// # Returns
    /// * A string like `/nix/store/<hash>-toplevel`
    pub async fn evaluate_nixos_system(&self) -> Result<String> {
        info!("🔍 Determining derivation for {}", self.summary());

        let is_path = Path::new(flake_url).exists();
        debug!("📁 Is local path: {is_path}");

        if is_path {
            debug!("📌 Running 'git checkout {commit}' in {flake_url}");
            let status = Command::new("git")
                .args(["-C", flake_url, "checkout", commit])
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
                "{flake_url}?rev={commit}#nixosConfigurations.{system}.config.system.build.toplevel"
            )
        } else {
            format!(
                "git+{flake_url}?rev={commit}#nixosConfigurations.{system}.config.system.build.toplevel"
            )
        };

        debug!("🔨 Building flake target: {flake_target} (dry-run)");

        let output = Command::new("nix")
            .args(["build", &flake_target, "--dry-run", "--json"])
            .output()
            .await?;

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

        info!("✅ Derivation path for {system}: {hash}");
        Ok(hash)
    }
}
