use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::fmt::Debug;
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

type SystemEvalQueue = Arc<Mutex<VecDeque<Job>>>;

#[derive(Debug, Clone)]
struct SystemEvalJob {
    flake_name: String,
    flake_url: String,
    commit: String,
    system_name: Option<String>,
    enqueued_at: chrono::DateTime<chrono::Utc>,
}

impl SystemEvalJob {
    /// Returns the derivation output path (hash) for a specific NixOS system from a given flake.
    ///
    /// If the flake is a local path, the given commit will be checked out via `git`.
    ///
    /// # Arguments
    /// * `system` - The nixosConfigurations.<system> to target
    /// * `flake_url` - Local path or remote git URL
    /// * `commit` - Git commit to use if applicable
    ///
    /// # Returns
    /// * A string like `/nix/store/<hash>-toplevel`
    pub async fn get_system_derivation(
        system: &str,
        flake_url: &str,
        commit: &str,
    ) -> std::result::Result<String, anyhow::Error> {
        info!(
            "🔍 Determining derivation for system: {system}, flake: {flake_url}, commit: {commit}"
        );

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
