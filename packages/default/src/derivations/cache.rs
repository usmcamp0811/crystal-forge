use super::Derivation;
use super::utils::*;
use crate::config::{BuildConfig, CacheConfig};
use anyhow::bail;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
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
            // Timeout per attempt
            // For large systems (40GB+), increase push_timeout_seconds to 3600 (1 hour) or more
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
                        error!("Terminal cache push error, not retrying: {}", e);
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
            info!("Skipping cache push for {}", self.derivation_name);
            return Ok(());
        }

        // Resolve .drv -> store path if needed
        let store_path = if path.ends_with(".drv") {
            info!("Resolving derivation path to store path: {}", path);
            Self::resolve_drv_to_store_path(path).await?
        } else {
            path.to_string()
        };

        // Get command and args from config
        let cache_cmd = match cache_config.cache_command(&store_path) {
            Some(cmd) => cmd,
            None => {
                warn!("No cache push configuration found, skipping cache push");
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

            // Helpful: log environment presence and file-based config once
            debug_attic_environment();

            // One-time login (per-process), persisted under /var/lib/crystal-forge
            ensure_attic_login(&remote, &endpoint, &token).await?;

            info!(
                "Pushing {} to cache... ({} {})",
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

            // ---- First attempt (streaming) ----
            let mut cmd = tokio::process::Command::new("attic");
            cmd.args(&effective_args);
            cmd.arg("-vv"); // Add verbose output for streaming
            cmd.env("HOME", "/var/lib/crystal-forge");
            cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
            apply_cache_env_to_command(&mut cmd);

            let success = run_cache_command_streaming(cmd, "attic push (first attempt)").await?;

            if !success {
                // Re-run to get error details for retry logic
                let mut cmd_check = tokio::process::Command::new("attic");
                cmd_check.args(&effective_args);
                cmd_check.env("HOME", "/var/lib/crystal-forge");
                cmd_check.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                apply_cache_env_to_command(&mut cmd_check);
                let output = cmd_check
                    .output()
                    .await
                    .context("Failed to run 'attic push'")?;
                let stderr = String::from_utf8_lossy(&output.stderr);
                let trimmed = stderr.trim();

                // ---- If unauthorized, redo login once and retry
                if trimmed.contains("Unauthorized")
                    || trimmed.contains("401")
                    || trimmed.contains("invalid token")
                {
                    warn!("Attic push returned 401; clearing login cache and retrying once...");
                    clear_attic_logged(&remote);

                    // Re-login with current env
                    let endpoint = std::env::var("ATTIC_SERVER_URL")?;
                    let token = std::env::var("ATTIC_TOKEN")?;
                    ensure_attic_login(&remote, &endpoint, &token).await?;

                    // Retry push with streaming
                    let mut cmd2 = tokio::process::Command::new("attic");
                    cmd2.args(&effective_args);
                    cmd2.arg("-vv"); // Add verbose output for streaming
                    cmd2.env("HOME", "/var/lib/crystal-forge");
                    cmd2.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                    apply_cache_env_to_command(&mut cmd2);
                    let retry_success =
                        run_cache_command_streaming(cmd2, "attic push (retry after 401)").await?;
                    if retry_success {
                        info!(
                            "Successfully pushed {} to cache (attic, after retry)",
                            store_path
                        );
                        return Ok(());
                    }
                }

                // If we get here, there was an error
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    error!("attic (direct) failed: {}", stderr.trim());
                    anyhow::bail!("attic failed (direct): {}", stderr.trim());
                }
            }

            info!("Successfully pushed {} to cache (attic)", store_path);
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

            // Add verbosity for nix commands
            if effective_command == "nix" {
                scoped.arg("-v");
            }

            let success =
                run_cache_command_streaming(scoped, &format!("{} (scoped)", effective_command))
                    .await?;
            if !success {
                anyhow::bail!("{} failed (scoped)", effective_command);
            }

            info!("Successfully pushed {} to cache (scoped)", store_path);
            return Ok(());
        }

        // Direct execution for non-Attic
        let mut cmd = Command::new(&effective_command);
        cmd.args(&effective_args);

        // Add verbosity for nix commands
        if effective_command == "nix" {
            cmd.arg("-v");
        }

        build_config.apply_to_command(&mut cmd);
        apply_cache_env_to_command(&mut cmd);

        let success = run_cache_command_streaming(cmd, &effective_command).await?;
        if !success {
            anyhow::bail!("{} failed", effective_command);
        }

        info!("Successfully pushed {} to cache", store_path);
        Ok(())
    }
}

/// Run a command and stream its output to debug logs
async fn run_cache_command_streaming(
    mut cmd: tokio::process::Command,
    command_name: &str,
) -> Result<bool> {
    info!("  → Spawning cache command: {}", command_name);

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("Failed to spawn cache command")?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // No per-read timeout! Large cache pushes (40GB+) can take a long time between outputs
    // We rely on the overall timeout in push_to_cache_with_retry instead
    loop {
        tokio::select! {
            line_result = stdout_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        info!("cache stdout: {}", line);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        error!("Error reading cache stdout: {}", e);
                        break;
                    }
                }
            }

            line_result = stderr_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        debug!("cache stderr: {}", line);
                    }
                    Ok(None) => {},
                    Err(e) => {
                        error!("Error reading cache stderr: {}", e);
                    }
                }
            }
        }
    }

    let status = child.wait().await?;
    Ok(status.success())
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

    tracing::info!("Attic login for remote '{remote}' at {endpoint}");
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
            tracing::info!("Attic remote '{remote}' already configured");
            mark_attic_logged(remote);
            return Ok(());
        }
        anyhow::bail!("attic login failed: {}", se.trim());
    }

    mark_attic_logged(remote);
    Ok(())
}
