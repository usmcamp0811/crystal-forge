use crate::config::{CacheType, deployment::DeploymentConfig};
use crate::handlers::agent::heartbeat::{LogResponse, RuntimeCacheConfig};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

// Note: This module requires readlink_path() to be in scope
// readlink_path should be imported from the agent module where it's defined

/// Result of a deployment operation
#[derive(Debug, Clone)]
pub enum DeploymentResult {
    NoDeploymentNeeded,
    AlreadyOnTarget,
    SuccessFromCache {
        cache_url: String,
    },
    SuccessLocalBuild,
    Started {
        unit_name: String,
    },
    Failed {
        error: String,
        desired_target: String,
    },
}

impl DeploymentResult {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            DeploymentResult::NoDeploymentNeeded
                | DeploymentResult::AlreadyOnTarget
                | DeploymentResult::SuccessFromCache { .. }
                | DeploymentResult::SuccessLocalBuild
                | DeploymentResult::Started { .. }
        )
    }

    pub fn description(&self) -> String {
        match self {
            DeploymentResult::NoDeploymentNeeded => "No deployment needed".to_string(),
            DeploymentResult::AlreadyOnTarget => "Already on target".to_string(),
            DeploymentResult::SuccessFromCache { cache_url } => {
                format!("Successfully deployed from cache: {}", cache_url)
            }
            DeploymentResult::SuccessLocalBuild => {
                "Successfully deployed with local build".to_string()
            }
            DeploymentResult::Started { unit_name } => {
                format!("Deployment started in unit: {}", unit_name)
            }
            DeploymentResult::Failed {
                error,
                desired_target,
            } => {
                format!("Deployment failed for {}: {}", desired_target, error)
            }
        }
    }

    pub fn change_reason(&self) -> &'static str {
        match self {
            DeploymentResult::SuccessFromCache { .. }
            | DeploymentResult::SuccessLocalBuild
            | DeploymentResult::Started { .. } => "cf_deployment",
            _ => "heartbeat",
        }
    }
}

/// Agent deployment manager handles applying deployments from server
pub struct AgentDeploymentManager {
    config: DeploymentConfig,
    current_target: Option<String>,
    deployment_lock: Arc<Semaphore>,
    runtime_caches: Vec<RuntimeCacheConfig>,
}

impl AgentDeploymentManager {
    pub fn new(config: DeploymentConfig) -> Self {
        Self {
            config,
            current_target: None,
            deployment_lock: Arc::new(Semaphore::new(1)),
            runtime_caches: Vec::new(),
        }
    }

    fn effective_runtime_cache(&self) -> Option<(String, CacheType, Option<String>, Option<String>)> {
        if let Some(cache) = self.runtime_caches.first() {
            let cache_type = match cache.cache_type.as_str() {
                "Attic" => CacheType::Attic,
                "S3" => CacheType::S3,
                "Http" => CacheType::Http,
                _ => CacheType::Nix,
            };
            return Some((
                cache.cache_url.clone(),
                cache_type,
                cache.cache_public_key.clone(),
                cache.attic_cache_name.clone(),
            ));
        }

        self.config.cache_url.as_ref().map(|cache_url| {
            (
                cache_url.clone(),
                self.config.cache_type.clone(),
                self.config.cache_public_key.clone(),
                self.config.attic_cache_name.clone(),
            )
        })
    }

    /// Read the actual current system from /run/current-system
    fn get_current_system(&self) -> Result<String> {
        let target = readlink_path("/run/current-system")
            .context("Failed to read /run/current-system symlink")?;

        let target_str = target
            .to_str()
            .context("Current system path is not valid UTF-8")?
            .to_string();

        Ok(target_str)
    }

    pub async fn process_heartbeat_response(
        &mut self,
        response: LogResponse,
    ) -> Result<DeploymentResult> {
        debug!("Processing heartbeat response");

        self.runtime_caches = response.runtime_caches;

        let Some(desired_target) = response.desired_target else {
            debug!("No desired target in heartbeat response");
            return Ok(DeploymentResult::NoDeploymentNeeded);
        };

        info!("Received desired target: {}", desired_target);

        // Always check the actual running system, not just cached state
        // This handles agent restarts, manual switches, and detached deployments
        let actual_current = self.get_current_system()?;

        if actual_current == desired_target {
            debug!("Already on target (verified via /run/current-system), skipping deployment");
            self.current_target = Some(desired_target.to_string());
            return Ok(DeploymentResult::AlreadyOnTarget);
        }

        debug!("Current system: {}", actual_current);
        debug!("Desired system: {}", desired_target);

        match self.execute_deployment(&desired_target).await {
            Ok(result) => {
                info!("Deployment completed successfully");
                self.current_target = Some(desired_target.to_string());
                Ok(result)
            }
            Err(e) => {
                error!("Deployment failed: {:#}", e);
                Ok(DeploymentResult::Failed {
                    error: e.to_string(),
                    desired_target: desired_target.to_string(),
                })
            }
        }
    }

    async fn execute_deployment(&self, target: &str) -> Result<DeploymentResult> {
        let _permit = self.deployment_lock.acquire().await?;

        info!("Starting deployment execution for: {}", target);

        let is_store_path = target.starts_with("/nix/store/");

        let effective_cache = self.effective_runtime_cache();

        // Store paths REQUIRE cache to be configured
        if is_store_path && effective_cache.is_none() {
            anyhow::bail!(
                "Cannot deploy store path without cache configured. Target: {}",
                target
            );
        }

        let start_time = std::time::Instant::now();

        let result = if is_store_path {
            // Store paths: deploy from cache
            let Some((cache_url, cache_type, cache_public_key, attic_cache_name)) = effective_cache else {
                anyhow::bail!("Store path deployment requested without effective cache config");
            };
            self.deploy_store_path_from_cache(
                target,
                &cache_url,
                &cache_type,
                cache_public_key.as_deref(),
                attic_cache_name.as_deref(),
            )
            .await?
        } else {
            anyhow::bail!(
                "This is not a store path we don't know how to handle it! Target: {}",
                target
            );
        };
        let duration = start_time.elapsed();
        info!(
            "Deployment completed in {:.2} seconds",
            duration.as_secs_f64()
        );

        Ok(result)
    }

    async fn deploy_store_path_from_cache(
        &self,
        store_path: &str,
        cache_url: &str,
        cache_type: &CacheType,
        cache_public_key: Option<&str>,
        attic_cache_name: Option<&str>,
    ) -> Result<DeploymentResult> {
        info!("Deploying store path from cache: {}", store_path);
        info!("Cache type: {:?}", cache_type);
        info!("Cache URL: {}", cache_url);
        if let Some(pk) = cache_public_key {
            info!("Using runtime cache public key: {}", pk);
        }
        if let Some(name) = attic_cache_name {
            info!("Using runtime attic cache name: {}", name);
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        // For all cache types, use cache_url directly
        let binary_cache_url = cache_url.to_string();

        // Step 1: Copy from cache with retry logic
        info!("Starting cache copy with retry logic...");
        self.copy_from_cache_with_retry(&binary_cache_url, store_path, cache_type, cache_public_key)
            .await?;

        // Step 2: Activate the configuration using systemd-run
        info!("Activating configuration via systemd-run...");
        self.activate_configuration(store_path, &unit_name).await?;

        info!("Deployment detached to systemd unit: {}", unit_name);
        Ok(DeploymentResult::Started { unit_name })
    }

    async fn copy_from_cache_with_retry(
        &self,
        cache_url: &str,
        store_path: &str,
        cache_type: &CacheType,
        cache_public_key: Option<&str>,
    ) -> Result<()> {
        const MAX_RETRIES: u32 = 3;
        const BASE_RETRY_DELAY: Duration = Duration::from_secs(5);

        for attempt in 1..=MAX_RETRIES {
            // Progressive retry strategies:
            // Attempt 1: normal copy
            // Attempt 2: add --refresh to bypass stale cache metadata
            // Attempt 3: clear local nix cache directory, then retry
            let use_refresh = attempt == 2;

            match self
                .copy_from_cache(cache_url, store_path, use_refresh, cache_type, cache_public_key)
                .await
            {
                Ok(()) => {
                    info!(
                        "Successfully copied {} from cache on attempt {}",
                        store_path, attempt
                    );
                    return Ok(());
                }
                Err(e) if attempt < MAX_RETRIES => {
                    let retry_delay = BASE_RETRY_DELAY.mul_f64(2_f64.powi((attempt - 1) as i32));
                    warn!(
                        "Cache copy attempt {} failed: {}. Retrying in {:.1}s...",
                        attempt,
                        e,
                        retry_delay.as_secs_f64()
                    );

                    // After second failure, clear nix cache before third attempt
                    if attempt == 2 {
                        if let Err(cache_err) = self.clear_nix_cache().await {
                            warn!("Failed to clear nix cache: {}", cache_err);
                        }
                    }

                    tokio::time::sleep(retry_delay).await;
                }
                Err(e) => {
                    error!("Cache copy failed after {} attempts: {}", MAX_RETRIES, e);
                    return Err(e).context(format!(
                        "Failed to copy {} from cache after {} retries",
                        store_path, MAX_RETRIES
                    ));
                }
            }
        }

        Err(anyhow::anyhow!(
            "Cache copy exhausted all {} retries",
            MAX_RETRIES
        ))
    }

    async fn clear_nix_cache(&self) -> Result<()> {
        // Try to determine the cache directory intelligently
        let cache_dir = if let Ok(home) = std::env::var("HOME") {
            format!("{}/.cache/nix", home)
        } else {
            // Fallback to common service user location
            "/var/lib/crystal-forge-agent/.cache/nix".to_string()
        };

        info!("Attempting to clear nix cache directory: {}", cache_dir);

        if std::path::Path::new(&cache_dir).exists() {
            tokio::fs::remove_dir_all(&cache_dir)
                .await
                .context(format!(
                    "Failed to remove nix cache directory: {}",
                    cache_dir
                ))?;
            info!("Successfully cleared nix cache directory: {}", cache_dir);
        } else {
            debug!("Nix cache directory does not exist: {}", cache_dir);
        }

        Ok(())
    }

    async fn copy_from_cache(
        &self,
        cache_url: &str,
        store_path: &str,
        refresh: bool,
        cache_type: &CacheType,
        cache_public_key: Option<&str>,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command as TokioCommand;

        let copy_timeout = self.config.deployment_timeout_minutes * 60;

        let mut copy_args = vec![
            "copy".to_string(),
            "--from".to_string(),
            cache_url.to_string(),
        ];

        // Add --refresh flag to bypass stale local cache metadata
        if refresh {
            info!("Using --refresh flag to bypass stale local cache metadata");
            copy_args.push("--refresh".to_string());
        }

        // Disable HTTP/2 for Attic to avoid framing errors
        if matches!(cache_type, CacheType::Attic) {
            debug!("Disabling HTTP/2 for Attic cache");
            copy_args.extend(vec![
                "--option".to_string(),
                "http2".to_string(),
                "false".to_string(),
            ]);
        }

        if let Some(public_key) = cache_public_key {
            copy_args.extend(vec![
                "--option".to_string(),
                "trusted-public-keys".to_string(),
                public_key.to_string(),
            ]);
        }

        copy_args.push(store_path.to_string());

        debug!(
            "Executing: nix {}",
            shell_join(&copy_args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        );

        let copy_result = tokio::time::timeout(
            Duration::from_secs(copy_timeout),
            async {
                let mut child = TokioCommand::new("nix")
                    .args(&copy_args)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .context("Failed to spawn nix copy command")?;

                let stdout = child
                    .stdout
                    .take()
                    .context("Failed to capture stdout from nix copy")?;
                let stderr = child
                    .stderr
                    .take()
                    .context("Failed to capture stderr from nix copy")?;

                let mut stdout_reader = BufReader::new(stdout).lines();
                let mut stderr_reader = BufReader::new(stderr).lines();

                let start = std::time::Instant::now();
                let mut last_output = std::time::Instant::now();
                let mut progress_interval = tokio::time::interval(Duration::from_secs(30));
                let mut error_buffer = String::new();

                loop {
                    tokio::select! {
                        line_result = stdout_reader.next_line() => {
                            match line_result {
                                Ok(Some(line)) => {
                                    last_output = std::time::Instant::now();
                                    info!("nix copy stdout: {}", line);
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    error!("Error reading stdout: {}", e);
                                    break;
                                }
                            }
                        }

                        line_result = stderr_reader.next_line() => {
                            match line_result {
                                Ok(Some(line)) => {
                                    last_output = std::time::Instant::now();
                                    debug!("nix copy stderr: {}", line);
                                    // Capture error lines for better error reporting
                                    if line.contains("error") {
                                        error_buffer.push_str(&line);
                                        error_buffer.push('\n');
                                    }
                                }
                                Ok(None) => {},
                                Err(e) => {
                                    error!("Error reading stderr: {}", e);
                                }
                            }
                        }

                        _ = progress_interval.tick() => {
                            let elapsed = start.elapsed().as_secs();
                            let idle_time = last_output.elapsed().as_secs();
                            let hours = elapsed / 3600;
                            let minutes = (elapsed % 3600) / 60;
                            let seconds = elapsed % 60;
                            info!(
                                "Still copying {} from cache... ({}h {}m {}s elapsed, {}s since last output)",
                                store_path, hours, minutes, seconds, idle_time
                            );
                        }
                    }
                }

                let status = child.wait().await?;
                if !status.success() {
                    let error_msg = if !error_buffer.is_empty() {
                        format!("nix copy failed: {}", error_buffer)
                    } else {
                        format!("nix copy failed with exit code {:?}", status.code())
                    };
                    anyhow::bail!(error_msg);
                }

                Ok::<(), anyhow::Error>(())
            },
        )
        .await;

        match copy_result {
            Ok(Ok(())) => {
                info!("Successfully copied {} from cache", store_path);
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_timeout) => {
                anyhow::bail!(
                    "Cache copy timed out after {} seconds ({}h {}m). Cache may be slow or unreachable. Consider increasing deployment_timeout_minutes.",
                    copy_timeout,
                    copy_timeout / 3600,
                    (copy_timeout % 3600) / 60
                );
            }
        }
    }

    async fn activate_configuration(&self, store_path: &str, unit_name: &str) -> Result<()> {
        let switch_script = format!("{}/bin/switch-to-configuration", store_path);

        // Verify the script exists
        if !std::path::Path::new(&switch_script).exists() {
            anyhow::bail!(
                "switch-to-configuration script not found at: {}. Store path may not be available.",
                switch_script
            );
        }

        // Step 1: Always create generation (for both strategies)
        info!("Creating new NixOS generation...");
        self.create_generation(store_path).await?;
        self.verify_generation_created(store_path).await?;

        // Step 2: Activate based on strategy
        use crate::config::deployment::DeploymentStrategy;
        let action = match self.config.strategy {
            DeploymentStrategy::ImmediatePersist => {
                info!("Using immediate_persist strategy: activating now");
                "switch"
            }
            DeploymentStrategy::BootOnly => {
                info!("Using boot_only strategy: will activate on next boot");
                "boot"
            }
        };

        self.activate_via_systemd(store_path, unit_name, action)
            .await?;
        Ok(())
    }

    /// Create a new NixOS generation
    async fn create_generation(&self, store_path: &str) -> Result<()> {
        let profile_path = "/nix/var/nix/profiles/system";

        debug!(
            "Creating generation: nix-env --profile {} --set {}",
            profile_path, store_path
        );

        let output = Command::new("nix-env")
            .args(&["--profile", profile_path, "--set", store_path])
            .output()
            .context("Failed to execute nix-env")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create generation: {}", stderr);
        }

        info!("✅ Generation created successfully");
        Ok(())
    }

    /// Verify that generation was created correctly with bounded retry for convergence
    async fn verify_generation_created(&self, store_path: &str) -> Result<()> {
        let profile_path = "/nix/var/nix/profiles/system";
        let current_system_path = "/run/current-system";

        // Canonicalize the expected store path once to ensure consistent comparison
        // (handles case where store_path could theoretically contain symlinks or relative components)
        let store_path_owned = store_path.to_string();
        let store_path_canonical =
            tokio::task::spawn_blocking(move || Self::resolve_symlink(&store_path_owned))
                .await
                .context("Task panicked while resolving target store path")??;

        // Retry configuration: up to 20 attempts with 500ms between = 10 seconds max
        const MAX_ATTEMPTS: u32 = 20;
        const RETRY_DELAY: Duration = Duration::from_millis(500);

        for attempt in 1..=MAX_ATTEMPTS {
            // Resolve the actual store paths (follow symlinks completely)
            // Use spawn_blocking to avoid blocking Tokio worker threads with sync fs calls
            let profile_path_owned = profile_path.to_string();
            let profile_resolved =
                tokio::task::spawn_blocking(move || Self::resolve_symlink(&profile_path_owned))
                    .await
                    .context("Task panicked while resolving profile symlink")??;

            let current_system_path_owned = current_system_path.to_string();
            let current_resolved = tokio::task::spawn_blocking(move || {
                Self::resolve_symlink(&current_system_path_owned)
            })
            .await
            .context("Task panicked while resolving current-system symlink")??;

            debug!(
                "Verification attempt {}/{}: profile={}, current_system={}, desired={}",
                attempt, MAX_ATTEMPTS, profile_resolved, current_resolved, store_path_canonical
            );

            // Check if either the profile or current-system points to the desired target
            let profile_matches = profile_resolved == store_path_canonical;
            let current_matches = current_resolved == store_path_canonical;

            if profile_matches || current_matches {
                let which = if profile_matches && current_matches {
                    "both profile and /run/current-system"
                } else if profile_matches {
                    "profile (generation created)"
                } else {
                    "/run/current-system (live system)"
                };
                info!(
                    "✅ Generation verified: {} converged to {}",
                    which, store_path_canonical
                );
                return Ok(());
            }

            // Check if we're in a transient activatable state
            let is_activatable = profile_resolved.contains("-activatable-nixos-system-")
                || current_resolved.contains("-activatable-nixos-system-");

            if is_activatable {
                debug!(
                    "System in transient activatable state, continuing to wait for convergence..."
                );
            } else if attempt == MAX_ATTEMPTS {
                // Final attempt failed and we're not in activatable state
                anyhow::bail!(
                    "Generation verification failed: system did not converge to desired target within {} seconds. \
                     Profile resolved to: {}, /run/current-system resolved to: {}, expected: {}",
                    (MAX_ATTEMPTS as f64 * RETRY_DELAY.as_secs_f64()),
                    profile_resolved,
                    current_resolved,
                    store_path_canonical
                );
            }

            // Wait before next attempt (unless this was the last attempt)
            if attempt < MAX_ATTEMPTS {
                tokio::time::sleep(RETRY_DELAY).await;
            }
        }

        // Should be unreachable due to bail in loop, but satisfy compiler
        anyhow::bail!("Verification loop exited unexpectedly")
    }

    /// Resolve a symlink to its final target (equivalent to readlink -f)
    fn resolve_symlink(path: &str) -> Result<String> {
        let path_buf = std::fs::canonicalize(path)
            .with_context(|| format!("Failed to canonicalize path: {}", path))?;

        let resolved = path_buf
            .to_str()
            .context("Resolved path is not valid UTF-8")?
            .to_string();

        Ok(resolved)
    }

    /// Activate configuration via systemd-run
    async fn activate_via_systemd(
        &self,
        store_path: &str,
        unit_name: &str,
        action: &str,
    ) -> Result<()> {
        let switch_script = format!("{}/bin/switch-to-configuration", store_path);

        let run_args = [
            "--unit",
            unit_name,
            "--no-block",
            "--same-dir",
            "--collect",
            "--",
            &switch_script,
            action, // "switch" or "boot"
        ];

        debug!("Executing: systemd-run {}", shell_join(&run_args));

        let output = Command::new("systemd-run")
            .args(&run_args)
            .output()
            .context("Failed to spawn systemd-run")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "systemd-run failed: stdout={}, stderr={}",
                stdout.trim(),
                stderr.trim()
            );
        }

        Ok(())
    }

    pub fn update_current_target(&mut self, target: Option<String>) {
        self.current_target = target;
    }
}

fn shell_quote(s: &str) -> String {
    // Simple POSIX single-quote: ' -> '\''  (ends, escaped quote, resumes)
    if s.is_empty() {
        return "''".to_string();
    }
    if s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_./:@".contains(&b))
    {
        // Fast path: no quoting needed for common arg chars
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    }
}

fn shell_join(args: &[&str]) -> String {
    args.iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a symlink and returns its target as a `PathBuf`.
pub fn readlink_path(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(nix::fcntl::readlink(path)?))
}
