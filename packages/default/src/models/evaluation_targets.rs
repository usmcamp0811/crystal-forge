use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// Evaluation Targets that have not been evaluated and thus we don't know the derivation path for
#[derive(Debug)]
pub struct PendingTarget {
    pub flake_name: String,
    pub repo_url: String,
    pub commit_hash: String,
    pub target: EvaluationTarget,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct EvaluationTarget {
    pub id: i32,
    pub commit_id: i32,
    pub target_type: String,                    // e.g. "nixos", "home", "app"
    pub target_name: String,                    // e.g. system or profile name
    pub derivation_path: Option<String>,        // populated post-build
    pub build_timestamp: Option<DateTime<Utc>>, // nullable until built
}

#[derive(Debug, Serialize, Deserialize)]
enum TargetType {
    NixOS,
    HomeManager,
}

impl EvaluationTarget {
    pub fn summary(&self) -> String {
        pool = get_db_client();
        let commit = crate::queries::commits::get_commit_by_id(pool, self.commit_id).await?;
        let flake = crate::queries::flakes::get_flake_by_id(pool, commit.flake_id).await?;
        format!(
            "{}@{} ({})",
            flake.name,
            commit.git_commit_hash,
            self.target_name.as_deref().unwrap_or("unknown")
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
