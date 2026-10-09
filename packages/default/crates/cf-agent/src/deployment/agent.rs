use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cf_config::config::{CacheType, CrystalForgeConfig, deployment::DeploymentConfig};
use cf_protocol::agent::{LogResponse, RuntimeCacheConfig};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
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
    started_at: Instant,
}

#[derive(Debug, Serialize)]
struct DeploymentStartedReport<'a> {
    hostname: &'a str,
    target_store_path: &'a str,
}

#[derive(Debug, Serialize)]
struct DeploymentFailedReport<'a> {
    hostname: &'a str,
    target_store_path: &'a str,
    error: &'a str,
}

impl AgentDeploymentManager {
    /// Creates a deployment manager with local fallback cache settings.
    pub fn new(config: DeploymentConfig) -> Self {
        Self {
            config,
            current_target: None,
            deployment_lock: Arc::new(Semaphore::new(1)),
            runtime_caches: Vec::new(),
            started_at: Instant::now(),
        }
    }

    fn effective_runtime_cache(&self) -> Result<Option<RuntimeCacheConfig>> {
        if let Some(cache) = self.runtime_caches.first() {
            if !matches!(
                cache.cache_type.as_str(),
                "Attic" | "S3" | "Http" | "Nix" | "Niks3"
            ) {
                anyhow::bail!("Unknown runtime cache type");
            }
            return Ok(Some(clone_runtime_cache(cache)));
        }

        // INVARIANT: Static deployment settings cannot express Niks3 auth.
        // Do not infer a public cache when runtime credentials are unavailable.
        if self.config.cache_type == CacheType::Niks3 {
            anyhow::bail!("Niks3 deployment requires server-provided read settings");
        }
        Ok(self
            .config
            .cache_url
            .as_ref()
            .map(|cache_url| RuntimeCacheConfig {
                cache_url: cache_url.clone(),
                cache_type: format!("{:?}", self.config.cache_type),
                cache_public_key: self.config.cache_public_key.clone(),
                attic_cache_name: self.config.attic_cache_name.clone(),
                cache_public_keys: self.config.cache_public_key.iter().cloned().collect(),
                read_auth: cf_protocol::cache::CacheReadAuth::None,
            }))
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

    /// Updates read-only runtime caches and applies an authorized desired target.
    ///
    /// # Errors
    /// Returns an error if the current system cannot be read. Deployment failures
    /// are returned as [`DeploymentResult::Failed`] after best-effort reporting.
    pub async fn process_heartbeat_response(
        &mut self,
        response: LogResponse,
    ) -> Result<(DeploymentResult, Option<u64>)> {
        debug!("Processing heartbeat response");

        self.runtime_caches = response.runtime_caches;
        let heartbeat_interval_secs = response.heartbeat_interval_secs;

        let Some(desired_target) = response.desired_target else {
            debug!("No desired target in heartbeat response");
            return Ok((
                DeploymentResult::NoDeploymentNeeded,
                heartbeat_interval_secs,
            ));
        };

        info!("Received desired target: {}", desired_target);

        let uptime = self.started_at.elapsed();
        if uptime < self.config.post_agent_start_deployment_delay {
            info!(
                "Deferring deployment for {} until post-agent-start delay expires ({:?} remaining)",
                desired_target,
                self.config.post_agent_start_deployment_delay - uptime
            );
            return Ok((
                DeploymentResult::NoDeploymentNeeded,
                heartbeat_interval_secs,
            ));
        }

        // Always check the actual running system, not just cached state
        // This handles agent restarts, manual switches, and detached deployments
        let actual_current = self.get_current_system()?;

        if actual_current == desired_target {
            debug!("Already on target (verified via /run/current-system), skipping deployment");
            self.current_target = Some(desired_target.to_string());
            return Ok((DeploymentResult::AlreadyOnTarget, heartbeat_interval_secs));
        }

        debug!("Current system: {}", actual_current);
        debug!("Desired system: {}", desired_target);

        match self.execute_deployment(&desired_target).await {
            Ok(result) => {
                info!("Deployment completed successfully");
                self.current_target = Some(desired_target.to_string());
                Ok((result, heartbeat_interval_secs))
            }
            Err(e) => {
                error!("Deployment failed: {:#}", e);
                let error_message = format!("{:#}", e);
                self.report_deployment_failed_best_effort(&desired_target, &error_message)
                    .await;
                Ok((
                    DeploymentResult::Failed {
                        error: error_message,
                        desired_target: desired_target.to_string(),
                    },
                    heartbeat_interval_secs,
                ))
            }
        }
    }

    async fn execute_deployment(&self, target: &str) -> Result<DeploymentResult> {
        let _permit = self.deployment_lock.acquire().await?;

        info!("Starting deployment execution for: {}", target);

        let is_store_path = target.starts_with("/nix/store/");

        let effective_cache = self.effective_runtime_cache()?;

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
            let Some(cache) = effective_cache else {
                anyhow::bail!("Store path deployment requested without effective cache config");
            };
            self.deploy_store_path_from_cache(target, &cache).await?
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
        cache: &RuntimeCacheConfig,
    ) -> Result<DeploymentResult> {
        info!("Deploying store path from cache: {}", store_path);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        // Step 1: Copy from cache with retry logic
        info!("Starting cache copy with retry logic...");
        self.copy_from_cache_with_retry(cache, store_path).await?;

        // Step 2: Activate the configuration using systemd-run
        info!("Activating configuration via systemd-run...");
        self.activate_configuration(store_path, &unit_name).await?;

        info!("Deployment detached to systemd unit: {}", unit_name);
        Ok(DeploymentResult::Started { unit_name })
    }

    async fn copy_from_cache_with_retry(
        &self,
        cache: &RuntimeCacheConfig,
        store_path: &str,
    ) -> Result<()> {
        const MAX_RETRIES: u32 = 3;
        const BASE_RETRY_DELAY: Duration = Duration::from_secs(5);

        for attempt in 1..=MAX_RETRIES {
            // Progressive retry strategies:
            // Attempt 1: normal copy
            // Attempt 2: add --refresh to bypass stale cache metadata
            // Attempt 3: clear local nix cache directory, then retry
            let use_refresh = attempt == 2;

            match self.copy_from_cache(cache, store_path, use_refresh).await {
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
        cache: &RuntimeCacheConfig,
        store_path: &str,
        refresh: bool,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command as TokioCommand;

        let copy_timeout = self.config.deployment_timeout_minutes * 60;
        if cache.cache_type == "Niks3"
            || !matches!(cache.read_auth, cf_protocol::cache::CacheReadAuth::None)
        {
            return copy_authenticated_cache(
                clone_runtime_cache(cache),
                store_path.to_owned(),
                refresh,
                Duration::from_secs(copy_timeout),
                std::ffi::OsString::from("nix"),
            )
            .await;
        }
        let cache_url = &cache.cache_url;
        let keys = if cache.cache_public_keys.is_empty() {
            cache.cache_public_key.iter().cloned().collect::<Vec<_>>()
        } else {
            cache.cache_public_keys.clone()
        };
        let joined_keys = keys.join(" ");
        let cache_public_key = (!joined_keys.is_empty()).then_some(joined_keys.as_str());

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
        if cache.cache_type == "Attic" {
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
                "extra-trusted-public-keys".to_string(),
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
        use cf_config::config::deployment::DeploymentStrategy;
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

        self.report_deployment_started_best_effort(store_path).await;

        self.activate_via_systemd(store_path, unit_name, action)
            .await?;
        Ok(())
    }

    async fn report_deployment_started_best_effort(&self, store_path: &str) {
        if let Err(error) = self.report_deployment_started(store_path).await {
            debug!(
                target_store_path = %store_path,
                ?error,
                "Failed to report deployment-started; continuing deployment"
            );
        }
    }

    async fn report_deployment_started(&self, store_path: &str) -> Result<()> {
        let cfg = CrystalForgeConfig::load()?;
        let client_cfg = &cfg.client;
        let hostname = hostname::get()?.to_string_lossy().into_owned();
        let payload = DeploymentStartedReport {
            hostname: &hostname,
            target_store_path: store_path,
        };
        let payload_json = serde_json::to_string(&payload)?;

        let key_bytes = STANDARD
            .decode(fs::read_to_string(&client_cfg.private_key)?.trim())
            .context("failed to decode base64 private key")?;
        let signing_key = SigningKey::from_bytes(
            key_bytes
                .as_slice()
                .try_into()
                .context("expected a 32-byte Ed25519 private key")?,
        );
        let signature = signing_key.sign(payload_json.as_bytes());
        let signature_b64 = STANDARD.encode(signature.to_bytes());

        let (scheme, port_suffix) = match client_cfg.server_port {
            443 => ("https", "".to_string()),
            80 => ("http", "".to_string()),
            port => ("http", format!(":{}", port)),
        };
        let url = format!(
            "{}://{}{}/agent/deployment-started",
            scheme, client_cfg.server_host, port_suffix
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let response = client
            .post(url)
            .header("X-Signature", signature_b64)
            .header("X-Key-ID", hostname)
            .body(payload_json)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("server responded with {}", response.status());
        }

        Ok(())
    }

    async fn report_deployment_failed_best_effort(&self, store_path: &str, error: &str) {
        if let Err(report_error) = self.report_deployment_failed(store_path, error).await {
            debug!(
                target_store_path = %store_path,
                ?report_error,
                "Failed to report deployment-failed; deployment failure already logged locally"
            );
        }
    }

    async fn report_deployment_failed(&self, store_path: &str, error: &str) -> Result<()> {
        let cfg = CrystalForgeConfig::load()?;
        let client_cfg = &cfg.client;
        let hostname = hostname::get()?.to_string_lossy().into_owned();
        let payload = DeploymentFailedReport {
            hostname: &hostname,
            target_store_path: store_path,
            error,
        };
        let payload_json = serde_json::to_string(&payload)?;

        let key_bytes = STANDARD
            .decode(fs::read_to_string(&client_cfg.private_key)?.trim())
            .context("failed to decode base64 private key")?;
        let signing_key = SigningKey::from_bytes(
            key_bytes
                .as_slice()
                .try_into()
                .context("expected a 32-byte Ed25519 private key")?,
        );
        let signature = signing_key.sign(payload_json.as_bytes());
        let signature_b64 = STANDARD.encode(signature.to_bytes());

        let (scheme, port_suffix) = match client_cfg.server_port {
            443 => ("https", "".to_string()),
            80 => ("http", "".to_string()),
            port => ("http", format!(":{}", port)),
        };
        let url = format!(
            "{}://{}{}/agent/deployment-failed",
            scheme, client_cfg.server_host, port_suffix
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let response = client
            .post(url)
            .header("X-Signature", signature_b64)
            .header("X-Key-ID", hostname)
            .body(payload_json)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("server responded with {}", response.status());
        }

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

    /// Updates the manager's remembered target without activating a system.
    pub fn update_current_target(&mut self, target: Option<String>) {
        self.current_target = target;
    }
}

fn clone_runtime_cache(cache: &RuntimeCacheConfig) -> RuntimeCacheConfig {
    RuntimeCacheConfig {
        cache_type: cache.cache_type.clone(),
        cache_url: cache.cache_url.clone(),
        cache_public_key: cache.cache_public_key.clone(),
        attic_cache_name: cache.attic_cache_name.clone(),
        cache_public_keys: cache.cache_public_keys.clone(),
        read_auth: cache.read_auth.clone(),
    }
}

// CONCURRENCY: The detached task owns read credentials until child exit or
// timeout kill/reap, even when the deployment future is dropped. Child output
// is discarded because TLS diagnostics may contain private configuration.
async fn copy_authenticated_cache(
    cache: RuntimeCacheConfig,
    store_path: String,
    refresh: bool,
    timeout: Duration,
    program: std::ffi::OsString,
) -> Result<()> {
    tokio::spawn(async move {
        if cache.cache_type == "Niks3" && cache.cache_public_keys.is_empty() {
            anyhow::bail!("Niks3 reads require signing keys");
        }
        let features = if matches!(
            cache.read_auth,
            cf_protocol::cache::CacheReadAuth::Basic { .. }
        ) {
            probe_nix_read_features(&program).await?
        } else {
            cf_config::cache_credentials::NixReadFeatures::default()
        };
        let read = cf_config::cache_credentials::PreparedCacheRead::new_with_nix_features(
            &cache.cache_url,
            &cache.cache_public_keys,
            &cache.read_auth,
            &features,
        )?;
        let mut command = tokio::process::Command::new(program);
        // SECURITY: Only read auth is available to this child. Ambient write
        // tokens, AWS credentials, and Nix access-token config are excluded.
        command.env_clear();
        for key in ["PATH", "NIX_REMOTE", "SSL_CERT_FILE", "SSL_CERT_DIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.args([
            "copy",
            "--from",
            &read.url,
            "--option",
            "extra-trusted-public-keys",
            &read.trusted_public_keys,
            "--option",
            "require-sigs",
            "true",
            &store_path,
        ]);
        if refresh {
            command.arg("--refresh");
        }
        read.apply_to_nix_command(command.as_std_mut());
        let mut child = command
            .kill_on_drop(true)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to spawn authenticated cache copy")?;
        let result = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => {
                if status.success() {
                    Ok(())
                } else {
                    anyhow::bail!("Authenticated cache copy failed with status {status}")
                }
            }
            Ok(Err(error)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(error).context("Failed to wait for authenticated cache copy")
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                anyhow::bail!("Authenticated cache copy timed out");
            }
        };
        drop(read);
        result
    })
    .await
    .context("Authenticated cache copy owner failed")?
}

/// Probes the read executable for native netrc authority protection.
///
/// Captures settings privately, with a ten-second deadline and a 2 MiB stdout
/// limit. Callers must use the same executable and PATH for the subsequent read.
/// Probe diagnostics are never included in errors because settings can contain
/// unrelated credentials. No credential files are created by this operation.
///
/// # Errors
/// Returns a static error on spawn, timeout, excessive output, nonzero exit,
/// or malformed settings. A valid unpatched runtime returns absent support.
///
/// # Examples
/// ```no_run
/// # async fn example() -> anyhow::Result<()> {
/// use cf_agent::deployment::agent::probe_nix_read_features;
/// let program = std::ffi::OsStr::new("nix");
/// let features = probe_nix_read_features(program).await?;
/// if !features.supports_netrc_authority() {
///     anyhow::bail!("Basic reads require the native authority guard");
/// }
/// # Ok(())
/// # }
/// ```
pub async fn probe_nix_read_features(
    program: &std::ffi::OsStr,
) -> Result<cf_config::cache_credentials::NixReadFeatures> {
    use tokio::io::AsyncReadExt;
    // SECURITY: Bound both captured streams; never expose settings or stderr.
    const MAX_SETTINGS: u64 = 2 * 1024 * 1024;
    let mut command = tokio::process::Command::new(program);
    command.args([
        "--extra-experimental-features",
        "nix-command",
        "config",
        "show",
        "--json",
    ]);
    let mut child = command
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| anyhow::anyhow!("Nix runtime feature probe failed"))?;
    let stdout = child.stdout.take().context("Missing Nix probe stdout")?;
    let stderr = child.stderr.take().context("Missing Nix probe stderr")?;
    let capture = async {
        let mut settings = Vec::new();
        let mut diagnostics = Vec::new();
        let mut stdout = stdout.take(MAX_SETTINGS + 1);
        let mut stderr = stderr.take(64 * 1024 + 1);
        let (out, err) = tokio::join!(
            stdout.read_to_end(&mut settings),
            stderr.read_to_end(&mut diagnostics),
        );
        if out.is_err()
            || err.is_err()
            || settings.len() > MAX_SETTINGS as usize
            || diagnostics.len() > 64 * 1024
        {
            anyhow::bail!("Nix runtime feature probe exceeded capture limits");
        }
        let status = child
            .wait()
            .await
            .map_err(|_| anyhow::anyhow!("Nix runtime feature probe failed"))?;
        anyhow::ensure!(status.success(), "Nix runtime feature probe failed");
        cf_config::cache_credentials::NixReadFeatures::from_settings_json(&settings)
    };
    match tokio::time::timeout(Duration::from_secs(10), capture).await {
        Ok(Ok(features)) => Ok(features),
        _ => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            anyhow::bail!("Nix runtime feature probe failed");
        }
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

#[cfg(test)]
mod niks3_tests {
    use super::*;
    use cf_protocol::cache::CacheReadAuth;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(script: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("cf-agent-read-test-{}", rand::random::<u64>()));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            let program = path.join("nix");
            std::fs::write(&program, script.replace("FIXTURE", path.to_str().unwrap())).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn program(&self) -> std::ffi::OsString {
            self.0.join("nix").into_os_string()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn cache(private: bool) -> RuntimeCacheConfig {
        RuntimeCacheConfig {
            cache_type: "Niks3".into(),
            cache_url: "https://read.example".into(),
            cache_public_key: None,
            attic_cache_name: None,
            cache_public_keys: vec!["one:key".into(), "two:key".into()],
            read_auth: if private {
                CacheReadAuth::Mtls {
                    client_certificate: "read-cert".into(),
                    client_private_key: "read-key".into(),
                    ca_certificate: Some("read-ca".into()),
                }
            } else {
                CacheReadAuth::None
            },
        }
    }
    const PRIVATE_SCRIPT: &str = r#"#!/bin/sh
set -eu
test "$1" = copy
test "$2" = --from
test "$6" = 'one:key two:key'
test "$9" = true
case "$3" in https://read.example/*tls-certificate=*tls-private-key=*) ;; *) exit 11;; esac
directory=${NIX_SSL_CERT_FILE%/*}
test "$(cat "$directory/client-cert.pem")" = read-cert
test "$(cat "$directory/client-key.pem")" = read-key
test "$(cat "$NIX_SSL_CERT_FILE")" = read-ca
test "$(stat -c %a "$directory")" = 700
test "$(stat -c %a "$directory/client-key.pem")" = 600
test -z "${AWS_SECRET_ACCESS_KEY:-}"
test -z "${NIKS3_AUTH_TOKEN_FILE:-}"
test -z "${ATTIC_TOKEN:-}"
printf '%s' "$directory" > FIXTURE/credentials
"#;

    #[tokio::test]
    async fn niks3_basic_read_applies_guard_paths_and_signatures() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
set -eu
if test "$1" = --extra-experimental-features; then
    test "$2" = nix-command; test "$3" = config; test "$4" = show; test "$5" = --json
    printf '%s' '{"cf-netrc-authority":{"value":""}}'
    exit 0
fi
test "$1" = copy; test "$3" = https://read.example/
case "$*" in *private-user*|*private-password*) exit 24 ;; esac
shift 10
netrc=; authority=false; sigs=false; keys=false
while test "$#" -gt 0; do
    test "$1" = --option
    case "$2" in
    netrc-file) netrc=$3 ;;
    cf-netrc-authority) test "$3" = https://read.example; authority=true ;;
    require-sigs) test "$3" = true; sigs=true ;;
    extra-trusted-public-keys) test "$3" = 'one:key two:key'; keys=true ;;
    *) exit 11 ;;
    esac
    shift 3
done
test "$authority" = true; test "$sigs" = true; test "$keys" = true
test -f "$netrc"; test "$(stat -c %a "$netrc")" = 600
test "$(stat -c %a "${netrc%/*}")" = 700
test -z "${NIX_CONFIG:-}"; test -z "${NIKS3_AUTH_TOKEN_FILE:-}"
printf '%s' "${netrc%/*}" > FIXTURE/credentials
"#,
        );
        let mut basic = cache(false);
        basic.read_auth = CacheReadAuth::Basic {
            username: "private-user".into(),
            password: " private-password ".into(),
        };
        copy_authenticated_cache(
            basic,
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap();
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[tokio::test]
    async fn niks3_basic_read_unpatched_runtime_never_copies() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
set -eu
test "$1" = --extra-experimental-features
printf '%s' '{"require-sigs":{"value":true}}'
"#,
        );
        let mut basic = cache(false);
        basic.read_auth = CacheReadAuth::Basic {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        assert!(
            copy_authenticated_cache(
                basic,
                "/nix/store/output".into(),
                false,
                Duration::from_secs(5),
                fixture.program()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn niks3_private_read_owns_files_and_ca_until_exit() {
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\nsleep 0.05\ntest -f \"$directory/client-key.pem\"\n"
        ));
        copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            true,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap();
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[tokio::test]
    async fn niks3_public_read_preserves_multiple_keys_and_signatures() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
set -eu
test "$3" = https://read.example/
test "$6" = 'one:key two:key'
test "$9" = true
test -z "${NIX_SSL_CERT_FILE:-}"
test -z "${AWS_SECRET_ACCESS_KEY:-}"
"#,
        );
        copy_authenticated_cache(
            cache(false),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn niks3_read_failures_suppress_child_secrets_and_clean_up() {
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\nprintf 'read-key write-token' >&2\nexit 17\n"
        ));
        let error = copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(!error.contains("read-key"));
        assert!(!error.contains("write-token"));
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
        let mut insecure = cache(true);
        insecure.cache_url = "http://read.example".into();
        assert!(
            copy_authenticated_cache(
                insecure,
                "/nix/store/output".into(),
                false,
                Duration::from_secs(5),
                fixture.program()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn niks3_read_cancellation_retains_credentials_through_child_exit() {
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\nsleep 0.2\ntest -f \"$directory/client-key.pem\"\ntouch FIXTURE/exited\n"
        ));
        let operation = tokio::spawn(copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !fixture.0.join("credentials").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        operation.abort();
        assert!(std::path::Path::new(&directory).exists());
        tokio::time::timeout(Duration::from_secs(5), async {
            while std::path::Path::new(&directory).exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(fixture.0.join("exited").exists());
    }

    #[tokio::test]
    async fn niks3_read_timeout_kills_and_reaps_before_credential_cleanup() {
        let fixture = Fixture::new(&format!("{PRIVATE_SCRIPT}\nexec sleep 30\n"));
        assert!(
            copy_authenticated_cache(
                cache(true),
                "/nix/store/output".into(),
                false,
                Duration::from_millis(100),
                fixture.program()
            )
            .await
            .is_err()
        );
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[test]
    fn niks3_runtime_unknown_types_and_missing_static_auth_fail_closed() {
        let mut manager = AgentDeploymentManager::new(DeploymentConfig::default());
        let mut unknown = cache(false);
        unknown.cache_type = "unknown".into();
        manager.runtime_caches = vec![unknown];
        assert!(manager.effective_runtime_cache().is_err());
        manager.runtime_caches.clear();
        manager.config.cache_type = CacheType::Niks3;
        manager.config.cache_url = Some("https://read.example".into());
        assert!(manager.effective_runtime_cache().is_err());
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
