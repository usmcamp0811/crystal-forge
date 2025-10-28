use super::Derivation;
use super::utils::*;
use crate::models::config::{BuildConfig, CacheConfig};
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::path::Path;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

impl Derivation {
    pub async fn push_to_cache_with_retry(
        &self,
        store_path: &str,
        cache_config: &CacheConfig,
        build_config: &BuildConfig,
    ) -> Result<()> {
        let mut attempts = 0;
        let max_attempts = cache_config.max_retries + 1;
        let base_delay = cache_config.retry_delay_seconds;

        while attempts < max_attempts {
            // Add timeout wrapper - default 10 minutes per attempt
            let timeout_duration = Duration::from_secs(cache_config.push_timeout_seconds);

            match tokio::time::timeout(
                timeout_duration,
                self.push_to_cache(store_path, cache_config, build_config),
            )
            .await
            {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(e)) if attempts < max_attempts - 1 => {
                    let err_msg = e.to_string();
                    // Terminal errors - don't retry
                    if err_msg.contains("SSL connect error")
                        || err_msg.contains("certificate verify failed")
                        || err_msg.contains("Name or service not known")
                        || err_msg.contains("no substituter that can build it")
                        || err_msg.contains("don't know how to build these paths")
                    {
                        error!("❌ Terminal cache push error, not retrying: {}", e);
                        return Err(e);
                    }
                    // Exponential backoff: 5s, 10s, 20s, 40s, 80s
                    let delay_secs = base_delay * (2_u64.pow(attempts as u32));
                    warn!(
                        "Cache push attempt {} failed: {}, retrying in {}s...",
                        attempts + 1,
                        e,
                        delay_secs
                    );
                    sleep(Duration::from_secs(delay_secs)).await;
                    attempts += 1;
                }
                Ok(Err(e)) => return Err(e),
                Err(_timeout) => {
                    if attempts < max_attempts - 1 {
                        let delay_secs = base_delay * (2_u64.pow(attempts as u32));
                        warn!(
                            "Cache push attempt {} timed out after {}s, retrying in {}s...",
                            attempts + 1,
                            timeout_duration.as_secs(),
                            delay_secs
                        );
                        sleep(Duration::from_secs(delay_secs)).await;
                        attempts += 1;
                    } else {
                        return Err(anyhow::anyhow!(
                            "Cache push timed out after {} attempts ({}s each)",
                            max_attempts,
                            timeout_duration.as_secs()
                        ));
                    }
                }
            }
        }
        unreachable!()
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

        // --- Ensure a GC root exists during push (prevents GC races) --------------------------
        let roots_dir = std::path::Path::new("/var/lib/crystal-forge/gcroots");
        // Safe to drop if your service guarantees this exists:
        let _ = tokio::fs::create_dir_all(roots_dir).await;

        let push_root = roots_dir.join(format!(
            "push-{}.root",
            store_path.trim_start_matches('/').replace("/", "_")
        ));
        let push_root_str = push_root.to_string_lossy().to_string();

        // Best-effort: create an indirect root on the output we’re about to push
        // nix-store --add-root <link> --indirect <store_path>
        {
            let mut addroot = tokio::process::Command::new("nix-store");
            addroot.args(["--add-root", &push_root_str, "--indirect", &store_path]);
            let _ = addroot.status().await;
        }

        // Get command and args from config
        let cache_cmd = match cache_config.cache_command(&store_path) {
            Some(cmd) => cmd,
            None => {
                warn!("⚠️ No cache push configuration found, skipping cache push");
                // Clean up the temporary root since we didn't push
                let _ = tokio::fs::remove_file(&push_root).await;
                return Ok(());
            }
        };

        let effective_command = cache_cmd.command.clone();
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

            debug_attic_environment();
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
                info_cmd.args(["cache", "info", &effective_args[1]]); // e.g. local:repo
                info_cmd.kill_on_drop(true);
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

            // ---- First attempt ----
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
                    let endpoint = std::env::var("ATTIC_SERVER_URL")?;
                    let token = std::env::var("ATTIC_TOKEN")?;
                    ensure_attic_login(&remote, &endpoint, &token).await?;

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
                // Clean up the temporary root even on failure (we’ll re-root on next attempt)
                let _ = tokio::fs::remove_file(&push_root).await;
                anyhow::bail!("attic failed (direct): {}", stderr.trim());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                info!("📤 attic output: {}", stdout.trim());
            }
            info!("✅ Successfully pushed {} to cache (attic)", store_path);

            // Remove GC root after successful push
            let _ = tokio::fs::remove_file(&push_root).await;

            return Ok(());
        }
        // --- End Attic special-case ------------------------------------------------------------

        // Non-Attic tools (e.g. `nix copy --to ...`)
        if build_config.should_use_systemd() {
            let mut scoped = Command::new("systemd-run");
            scoped.args(["--scope", "--collect", "--quiet"]);
            apply_systemd_props_for_scope(build_config, &mut scoped);
            apply_cache_env(&mut scoped);
            scoped
                .arg("--")
                .arg(&effective_command)
                .args(&effective_args)
                .kill_on_drop(true);

            let mut child = scoped
                .spawn()
                .context("Failed to spawn scoped cache command")?;
            let output = child
                .wait_with_output()
                .await
                .context("Failed to wait for scoped cache command")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "❌ {} (scoped) failed: {}",
                    effective_command,
                    stderr.trim()
                );
                let _ = tokio::fs::remove_file(&push_root).await;
                anyhow::bail!("{} failed (scoped): {}", effective_command, stderr.trim());
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                info!("📤 {} output: {}", effective_command, stdout.trim());
            }
            info!("✅ Successfully pushed {} to cache (scoped)", store_path);

            let _ = tokio::fs::remove_file(&push_root).await;
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
            let _ = tokio::fs::remove_file(&push_root).await;
            anyhow::bail!("{} failed: {}", effective_command, stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("📤 {} output: {}", effective_command, stdout.trim());
        }
        info!("✅ Successfully pushed {} to cache", store_path);

        // Remove the GC root after successful push
        let _ = tokio::fs::remove_file(&push_root).await;

        Ok(())
    }

    // Evaluates and optionally pushes to cache in one go
    // pub async fn evaluate_and_push_to_cache(
    //     &mut self,
    //     pool: &PgPool,
    //     build_config: &BuildConfig,
    //     cache_config: &CacheConfig,
    // ) -> Result<String> {
    //     let store_path: String = self.evaluate_and_build(pool, build_config).await?;
    //
    //     // Only push to cache if we did a full build (not a dry-run)
    //     if cache_config.push_after_build {
    //         if let Err(e) = self
    //             .push_to_cache(&store_path, cache_config, build_config)
    //             .await
    //         {
    //             warn!("⚠️ Cache push failed but continuing: {}", e);
    //             // Don't fail the whole operation if cache push fails
    //         }
    //     }
    //
    //     Ok(store_path)
    // }
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
