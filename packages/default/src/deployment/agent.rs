use crate::handlers::agent::heartbeat::LogResponse;
use crate::models::config::{CacheType, deployment::DeploymentConfig};
use anyhow::{Context, Result};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

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
}

impl AgentDeploymentManager {
    pub fn new(config: DeploymentConfig) -> Self {
        Self {
            config,
            current_target: None,
            deployment_lock: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn process_heartbeat_response(
        &mut self,
        response: LogResponse,
    ) -> Result<DeploymentResult> {
        debug!("Processing heartbeat response");

        let Some(desired_target) = response.desired_target else {
            debug!("No desired target in heartbeat response");
            return Ok(DeploymentResult::NoDeploymentNeeded);
        };

        info!("Received desired target: {}", desired_target);

        if let Some(ref current) = self.current_target {
            if current == &desired_target {
                debug!("Already on target, skipping deployment");
                return Ok(DeploymentResult::AlreadyOnTarget);
            }
        }

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

        // Store paths REQUIRE cache to be configured
        if is_store_path && self.config.cache_url.is_none() {
            anyhow::bail!(
                "Cannot deploy store path without cache configured. Target: {}",
                target
            );
        }

        let start_time = std::time::Instant::now();

        let result = if is_store_path {
            // Store paths: deploy from cache
            let cache_url = self.config.cache_url.as_ref().unwrap(); // Safe because we checked above
            self.deploy_store_path_from_cache(target, cache_url).await?
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
    ) -> Result<DeploymentResult> {
        info!("Deploying store path from cache: {}", store_path);
        info!("Cache type: {:?}", self.config.cache_type);
        info!("Cache URL: {}", cache_url);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        // For all cache types, use cache_url directly
        let binary_cache_url = cache_url.to_string();

        // Step 1: Copy from cache with retry logic
        info!("Starting cache copy with retry logic...");
        self.copy_from_cache_with_retry(&binary_cache_url, store_path)
            .await?;

        // Step 2: Activate the configuration using systemd-run
        info!("Activating configuration via systemd-run...");
        self.activate_configuration(store_path, &unit_name).await?;

        info!("Deployment detached to systemd unit: {}", unit_name);
        Ok(DeploymentResult::Started { unit_name })
    }

    async fn copy_from_cache_with_retry(&self, cache_url: &str, store_path: &str) -> Result<()> {
        const MAX_RETRIES: u32 = 3;
        const BASE_RETRY_DELAY: Duration = Duration::from_secs(5);

        for attempt in 1..=MAX_RETRIES {
            // Progressive retry strategies:
            // Attempt 1: normal copy
            // Attempt 2: add --refresh to bypass stale cache metadata
            // Attempt 3: clear local nix cache directory, then retry
            let use_refresh = attempt == 2;

            match self
                .copy_from_cache(cache_url, store_path, use_refresh)
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
        if matches!(self.config.cache_type, CacheType::Attic) {
            debug!("Disabling HTTP/2 for Attic cache");
            copy_args.extend(vec![
                "--option".to_string(),
                "http2".to_string(),
                "false".to_string(),
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

        let run_args = [
            "--unit",
            unit_name,
            "--no-block",
            "--same-dir",
            "--collect",
            "--",
            &switch_script,
            "switch",
        ];

        debug!("Executing: systemd-run {}", shell_join(&run_args));

        let output = Command::new("systemd-run")
            .args(&run_args)
            .output()
            .context("Failed to spawn systemd-run process")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("systemd-run failed stdout: {}", stdout);
            error!("systemd-run failed stderr: {}", stderr);
            anyhow::bail!(
                "systemd-run failed with exit code {:?}\nstdout: {}\nstderr: {}",
                output.status.code(),
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
