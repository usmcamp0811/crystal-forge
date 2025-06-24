use crate::db::get_db_client;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use std::path::Path;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::task;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info};

// Basically just derivations / outputs of a flake / aka System derivations
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct EvaluationTarget {
    pub id: i32,
    pub commit_id: i32,
    pub target_type: TargetType,         // e.g. "nixos", "home", "app"
    pub target_name: String,             // e.g. system or profile name
    pub derivation_path: Option<String>, // populated post-build
    pub build_timestamp: Option<DateTime<Utc>>, // nullable until built
    pub scheduled_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "text")]
#[sqlx(rename_all = "lowercase")]
pub enum TargetType {
    NixOS,
    HomeManager,
}

impl From<String> for TargetType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "nixos" => TargetType::NixOS,
            "homemanager" => TargetType::HomeManager,
            _ => panic!("Invalid TargetType: {}", s),
        }
    }
}

impl ToString for TargetType {
    fn to_string(&self) -> String {
        match self {
            TargetType::NixOS => "nixos".into(),
            TargetType::HomeManager => "homemanager".into(),
        }
    }
}

impl EvaluationTarget {
    pub async fn summary(&self) -> Result<String> {
        let pool = get_db_client().await?;
        let commit = crate::queries::commits::get_commit_by_id(&pool, self.commit_id).await?;

        let flake = crate::queries::flakes::get_flake_by_id(&pool, commit.flake_id).await?;
        Ok(format!(
            "{}@{} ({})",
            flake.name, commit.git_commit_hash, self.target_name
        ))
    }

    pub async fn resolve_derivation_path(&mut self) -> Result<String> {
        let name = self.target_name.clone();
        let kind = self.target_type.clone();

        println!(
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
                    sleep(Duration::from_secs(5)).await;
                    if *rx.borrow_and_update() {
                        break;
                    }
                    println!(
                        "⏳ Still evaluating {} '{}'",
                        kind.to_string().to_lowercase(),
                        name
                    );
                }
            }
        });

        let hash = match self.target_type {
            TargetType::NixOS => self.evaluate_nixos_system().await?,
            TargetType::HomeManager => anyhow::bail!("Home Manager evaluation not implemented yet"),
        };

        self.derivation_path = Some(hash.clone());

        let _ = tx.send(true);
        let _ = ticker.await;

        Ok(hash)
    }

    /// Returns the derivation output path (hash) for a specific NixOS system from a given flake.
    /// # Returns
    /// * A string like `/nix/store/<hash>-toplevel`
    pub async fn evaluate_nixos_system(&self) -> Result<String> {
        let summary = self.summary().await?;
        info!("🔍 Determining derivation for {summary}");

        let pool = get_db_client().await?;
        let commit = crate::queries::commits::get_commit_by_id(&pool, self.commit_id).await?;
        let flake = crate::queries::flakes::get_flake_by_id(&pool, commit.flake_id).await?;

        let flake_url = &flake.repo_url;
        let commit_hash = &commit.git_commit_hash;
        let system = &self.target_name;

        let is_path = Path::new(flake_url).exists();
        debug!("📁 Is local path: {is_path}");

        if is_path {
            debug!("📌 Running 'git checkout {:?}' in {}", commit, flake_url);
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
                flake_url, commit.git_commit_hash, self.target_name
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
