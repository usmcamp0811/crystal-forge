use crate::build::Derivation;
use crate::derivations::utils::*;
use anyhow::bail;
use anyhow::{Context, Result};
use cf_config::cache_credentials::PreparedNiks3Push;
use cf_config::config::CacheType;
use cf_config::config::{BuildConfig, CacheConfig};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

fn attic_streaming_push_args(effective_args: &[String]) -> Vec<String> {
    effective_args.to_vec()
}

fn shell_quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }

    if arg.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'=')
    }) {
        return arg.to_string();
    }

    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn format_command_for_log(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().map(|arg| shell_quote_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Derivation {
    /// Pushes a store path with the configured attempt deadline and retry policy.
    ///
    /// Niks3 credentials remain owned until the child exits, including when the
    /// attempt times out or the caller cancels the future.
    ///
    /// # Errors
    ///
    /// Returns the final push error or an exhausted-attempt timeout error.
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

            // Niks3 owns its deadline so timeout cleanup finishes before retry.
            let result = if matches!(cache_config.cache_type, CacheType::Niks3) {
                Ok(self
                    .push_to_cache(store_path, cache_config, build_config)
                    .await)
            } else {
                tokio::time::timeout(
                    timeout_duration,
                    self.push_to_cache(store_path, cache_config, build_config),
                )
                .await
            };
            match result {
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

    /// Pushes a store path, resolving a derivation to its output when necessary.
    ///
    /// Niks3 uses one CLI process with file-based write credentials and
    /// `parallel_uploads` concurrency. Its output is suppressed because it can
    /// contain presigned URLs. Attic retries authorization once after login.
    /// Disabled or filtered pushes return success without running a command.
    ///
    /// Attic login preserves ambient endpoint and token precedence. The chosen
    /// endpoint is resolved to the canonical server base before login.
    ///
    /// # Errors
    ///
    /// Returns an error for resolution, invalid Niks3 or Attic configuration,
    /// missing Attic credentials, process execution, timeout, or failed
    /// publication.
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

        if matches!(cache_config.cache_type, CacheType::Niks3) {
            return run_niks3_push(&store_path, cache_config).await;
        }

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
            ensure_attic_client_available().await?;

            let endpoint = resolve_attic_login_endpoint(
                cache_config,
                std::env::var("ATTIC_SERVER_URL").ok().as_deref(),
            )?;
            let token = std::env::var("ATTIC_TOKEN")
                .ok()
                .or_else(|| cache_config.attic_token.clone())
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
            let attic_push_command = format_command_for_log("attic", &effective_args);

            // Preflight: whoami
            {
                let mut whoami = tokio::process::Command::new("attic");
                whoami.arg("whoami");
                whoami.env("HOME", "/var/lib/crystal-forge");
                whoami.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                apply_cache_env_to_command(&mut whoami);
                apply_cache_config_env_to_command(&mut whoami, cache_config);
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
                apply_cache_config_env_to_command(&mut info_cmd, cache_config);
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
            cmd.args(attic_streaming_push_args(&effective_args));
            cmd.env("HOME", "/var/lib/crystal-forge");
            cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
            apply_cache_env_to_command(&mut cmd);
            apply_cache_config_env_to_command(&mut cmd, cache_config);

            let success = run_cache_command_streaming(cmd, "attic push (first attempt)").await?;

            if !success {
                // Re-run to get error details for retry logic
                let mut cmd_check = tokio::process::Command::new("attic");
                cmd_check.args(&effective_args);
                cmd_check.env("HOME", "/var/lib/crystal-forge");
                cmd_check.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                apply_cache_env_to_command(&mut cmd_check);
                apply_cache_config_env_to_command(&mut cmd_check, cache_config);
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

                    ensure_attic_login(&remote, &endpoint, &token).await?;

                    // Retry push with streaming
                    let mut cmd2 = tokio::process::Command::new("attic");
                    cmd2.args(attic_streaming_push_args(&effective_args));
                    cmd2.env("HOME", "/var/lib/crystal-forge");
                    cmd2.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
                    apply_cache_env_to_command(&mut cmd2);
                    apply_cache_config_env_to_command(&mut cmd2, cache_config);
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
                    anyhow::bail!(
                        "attic failed (direct): command: {}\n{}",
                        attic_push_command,
                        stderr.trim()
                    );
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
            apply_cache_config_env_for_scope(&mut scoped, cache_config);
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
        apply_cache_config_env_to_command(&mut cmd, cache_config);

        let success = run_cache_command_streaming(cmd, &effective_command).await?;
        if !success {
            anyhow::bail!("{} failed", effective_command);
        }

        info!("Successfully pushed {} to cache", store_path);
        Ok(())
    }
}

/// Runs one Niks3 upload without exposing credentials or presigned URLs.
async fn run_niks3_push(store_path: &str, cache: &CacheConfig) -> Result<()> {
    let server_url = cache
        .niks3_server_url
        .as_deref()
        .context("Niks3 push requires niks3_server_url")?;
    let auth = cache
        .niks3_write_auth
        .as_ref()
        .context("Niks3 push requires niks3_write_auth")?;
    let prepared = PreparedNiks3Push::new(server_url, auth, cache.parallel_uploads, store_path)
        .map_err(|_| {
            anyhow::anyhow!("Failed to prepare Niks3 push credentials or configuration")
        })?;
    run_prepared_niks3_push(prepared, Duration::from_secs(cache.push_timeout_seconds)).await
}

async fn run_prepared_niks3_push(prepared: PreparedNiks3Push, deadline: Duration) -> Result<()> {
    let (cancel, cancelled) = tokio::sync::oneshot::channel::<()>();

    // CONCURRENCY: This owner outlives cancellation of the calling future. The
    // dropped sender requests kill/reap before protected files are removed.
    let owner = tokio::spawn(async move {
        let mut command = tokio::process::Command::new(&prepared.command);
        command
            .args(&prepared.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // The shared helper owns all auth files in one protected directory.
        let credential_directory = prepared
            .args
            .windows(2)
            .find(|pair| matches!(pair[0].as_str(), "--auth-token-path" | "--client-key"))
            .and_then(|pair| std::path::Path::new(&pair[1]).parent())
            .context("Niks3 preparation did not supply a credential directory")?;
        apply_niks3_env_to_command(&mut command, credential_directory);
        let mut child = command.spawn().context("Failed to spawn Niks3 push")?;
        let result = tokio::select! {
            result = tokio::time::timeout(deadline, child.wait()) => match result {
                Ok(Ok(status)) if status.success() => Ok(()),
                Ok(Ok(status)) => Err(anyhow::anyhow!("Niks3 push failed with {status}; output suppressed")),
                Ok(Err(error)) => Err(anyhow::Error::new(error).context("Failed to wait for Niks3 push")),
                Err(_) => Err(anyhow::anyhow!("Niks3 push timed out after {}s", deadline.as_secs())),
            },
            _ = cancelled => Err(anyhow::anyhow!("Niks3 push cancelled")),
        };
        if child.id().is_some() {
            child
                .kill()
                .await
                .context("Failed to kill and reap Niks3 push")?;
        }
        // SECURITY: Keep temporary credentials until exit or confirmed kill.
        drop(prepared);
        result
    });
    let result = owner.await.context("Niks3 push owner failed")?;
    drop(cancel);
    result
}

#[cfg(test)]
mod tests {
    use super::{attic_streaming_push_args, format_command_for_log, run_prepared_niks3_push};
    use cf_config::cache_credentials::PreparedNiks3Push;
    use cf_protocol::cache::Niks3WriteAuth;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::Duration;

    // Use the shared preparation helper; these tests cover process ownership
    // and output suppression rather than reimplementing credential creation.
    fn fake_niks3(body: &str) -> (tempfile::TempDir, PreparedNiks3Push, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("niks3");
        std::fs::write(&script, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut prepared = PreparedNiks3Push::new(
            "https://write.example.org",
            &Niks3WriteAuth::Token {
                token: "synthetic-private-token".into(),
            },
            3,
            "/nix/store/00000000000000000000000000000000-output",
        )
        .unwrap();
        let credential = PathBuf::from(
            &prepared.args[prepared
                .args
                .iter()
                .position(|arg| arg == "--auth-token-path")
                .unwrap()
                + 1],
        );
        prepared.command = script.to_str().unwrap().into();
        (directory, prepared, credential)
    }

    #[tokio::test]
    async fn niks3_process_receives_flags_without_token_and_cleans_credentials() {
        let (_directory, prepared, credential) = fake_niks3(
            r#"
test "$1" = push
test "$2" = --server-url
test "$3" = https://write.example.org/
test "$4" = --max-concurrent-uploads
test "$5" = 3
test "$6" = --auth-token-path
test -f "$7"
test "$HOME/token" = "$7"
test "$XDG_CONFIG_HOME" = "$HOME"
test "$8" = --
test "$9" = /nix/store/00000000000000000000000000000000-output
case "$*" in *synthetic-private-token*) exit 42;; esac
test -z "${AWS_ACCESS_KEY_ID-}${AWS_SECRET_ACCESS_KEY-}${GARAGE_SECRET_KEY-}${ATTIC_TOKEN-}${NIKS3_AUTH_TOKEN_FILE-}"
"#,
        );
        run_prepared_niks3_push(prepared, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(!credential.exists());
        assert!(!credential.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn niks3_mtls_uses_isolated_home_and_file_credentials() {
        let (directory, token_prepared, _) = fake_niks3(
            r#"
test "$6" = --client-cert
test -f "$7"
test "$8" = --client-key
test -f "$9"
test "$HOME/client-key.pem" = "$9"
test "$XDG_CONFIG_HOME" = "$HOME"
test ! -e "$HOME/token"
test -z "${NIKS3_AUTH_TOKEN_FILE-}"
case "$*" in *synthetic-private-key*) exit 42;; esac
"#,
        );
        drop(token_prepared);
        let mut prepared = PreparedNiks3Push::new(
            "https://write.example.org",
            &Niks3WriteAuth::Mtls {
                client_certificate: "synthetic-certificate".into(),
                client_private_key: "synthetic-private-key".into(),
                ca_certificate: None,
            },
            3,
            "/nix/store/00000000000000000000000000000000-output",
        )
        .unwrap();
        prepared.command = directory.path().join("niks3").to_str().unwrap().into();
        let key = PathBuf::from(
            &prepared.args[prepared
                .args
                .iter()
                .position(|arg| arg == "--client-key")
                .unwrap()
                + 1],
        );
        run_prepared_niks3_push(prepared, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(!key.exists());
    }

    #[tokio::test]
    async fn niks3_failure_suppresses_sensitive_child_output() {
        let (_directory, prepared, credential) = fake_niks3(
            r#"
printf '%s\n' 'https://object.example.org/?presigned=synthetic-secret'
printf '%s\n' 'synthetic-private-token' >&2
exit 23
"#,
        );
        let error = run_prepared_niks3_push(prepared, Duration::from_secs(5))
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("23"));
        assert!(message.contains("output suppressed"));
        assert!(!message.contains("synthetic"));
        assert!(!message.contains("https://"));
        assert!(!credential.exists());
    }

    #[tokio::test]
    async fn niks3_timeout_reaps_child_before_credential_cleanup() {
        let (directory, prepared, credential) =
            fake_niks3("printf '%s' \"$$\" > \"${0%/*}/pid\"\nexec sleep 30");
        let error = run_prepared_niks3_push(prepared, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(!credential.exists());
        let pid = std::fs::read_to_string(directory.path().join("pid")).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }

    #[tokio::test]
    async fn niks3_cancellation_keeps_owner_until_child_cleanup() {
        let (directory, prepared, credential) =
            fake_niks3("printf '%s' \"$$\" > \"${0%/*}/pid\"\nexec sleep 30");
        let task = tokio::spawn(run_prepared_niks3_push(prepared, Duration::from_secs(30)));
        // Wait until the owner has started, then cancel only the caller.
        tokio::time::timeout(Duration::from_secs(5), async {
            while !directory.path().join("pid").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid = std::fs::read_to_string(directory.path().join("pid")).unwrap();
        assert!(credential.exists());
        task.abort();
        let _ = task.await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while credential.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        drop(directory);
    }

    #[test]
    fn attic_streaming_push_args_do_not_add_verbose_flags() {
        let args = vec![
            "push".to_string(),
            "local:campground".to_string(),
            "/nix/store/example".to_string(),
        ];

        let push_args = attic_streaming_push_args(&args);

        assert_eq!(push_args, args);
        assert!(!push_args.iter().any(|arg| arg == "-v" || arg == "-vv"));
    }

    #[test]
    fn attic_push_command_for_logs_is_copy_pasteable_without_verbose_flags() {
        let args = vec![
            "push".to_string(),
            "local:campground".to_string(),
            "/nix/store/example path".to_string(),
        ];

        assert_eq!(
            format_command_for_log("attic", &args),
            "attic push local:campground '/nix/store/example path'"
        );
    }
}

/// Run a command and stream its output to debug logs
async fn run_cache_command_streaming(
    mut cmd: tokio::process::Command,
    command_name: &str,
) -> Result<bool> {
    info!("  → Spawning cache command: {}", command_name);

    cmd.kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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

// COMPATIBILITY: Ambient ATTIC_SERVER_URL still wins over configured push_to.
// Canonicalize that chosen endpoint before login; do not change push references.
fn resolve_attic_login_endpoint(
    cache: &CacheConfig,
    endpoint_override: Option<&str>,
) -> Result<String> {
    let endpoint = endpoint_override
        .or(cache.push_to.as_deref().map(str::trim))
        .context("Attic server endpoint is missing")?;
    let urls = cf_config::attic_urls::resolve_attic_urls(
        endpoint,
        cache.attic_cache_name.as_deref().unwrap_or_default().trim(),
    )?;
    Ok(urls.server_url.to_string())
}

/// Logs into Attic with a canonical server base under the service account.
///
/// Remote-only memoization and already-configured acceptance preserve existing
/// behavior; neither proves that persisted credentials match this request.
async fn ensure_attic_login(remote: &str, endpoint: &str, token: &str) -> anyhow::Result<()> {
    ensure_attic_login_with_program(remote, endpoint, token, std::ffi::OsStr::new("attic")).await
}

async fn ensure_attic_login_with_program(
    remote: &str,
    endpoint: &str,
    token: &str,
    program: &std::ffi::OsStr,
) -> anyhow::Result<()> {
    if is_attic_logged(remote) {
        tracing::debug!(
            "attic: remote '{}' already initialized in this process",
            remote
        );
        return Ok(());
    }

    tracing::info!("Attic login for remote '{remote}'");
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(["login", remote, endpoint, token]);
    // Ensure credentials are persisted under the crystal-forge account:
    cmd.env("HOME", "/var/lib/crystal-forge");
    cmd.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");

    // If you also want AWS/S3 env available for any follow-up calls attic might make:
    apply_cache_env_to_command(&mut cmd);

    let out = cmd.output().await.with_context(|| {
        "failed to run 'attic login'; attic client must be available in the builder service PATH"
    })?;
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

async fn ensure_attic_client_available() -> anyhow::Result<()> {
    let output = tokio::process::Command::new("attic")
        .arg("--version")
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => anyhow::bail!(
            "attic client is present but failed to execute: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => anyhow::bail!(
            "attic client is not available in PATH; API builders receiving server-supplied \
             Attic cache-push config must run with attic-client available (current NixOS \
             module builds add it to crystal-forge-builder.service PATH)"
        ),
        Err(e) => Err(e).context("failed to probe attic client availability"),
    }
}

#[cfg(test)]
mod attic_url_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn attic_login_cli_receives_canonical_base_and_existing_token_argument() {
        for (index, (input, expected)) in [
            ("http://cache.example:8080", "http://cache.example:8080/"),
            ("https://cache.example/", "https://cache.example/"),
            ("https://cache.example/team", "https://cache.example/"),
            ("attic://cache.example/team", "https://cache.example/"),
            (
                "https://cache.example/team/nix-cache-info",
                "https://cache.example/",
            ),
            (
                "https://cache.example/prefix/team/?view=one",
                "https://cache.example/prefix/?view=one",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let config = CacheConfig {
                cache_type: CacheType::Attic,
                push_to: Some(input.into()),
                attic_cache_name: Some("configured-remote:team".into()),
                ..Default::default()
            };
            let endpoint = resolve_attic_login_endpoint(&config, None).unwrap();
            assert_eq!(endpoint, expected);
            assert_eq!(config.push_to.as_deref(), Some(input));
            assert_eq!(
                config.attic_cache_name.as_deref(),
                Some("configured-remote:team")
            );
            let remote = format!("builder-url-test-{index}");
            let directory = tempfile::tempdir().unwrap();
            let script = directory.path().join("attic");
            std::fs::write(&script, format!(
                "#!/bin/sh\nset -eu\ntest \"$#\" = 4\ntest \"$1\" = login\ntest \"$2\" = '{remote}'\ntest \"$3\" = '{expected}'\ntest \"$4\" = synthetic-attic-cli-token\ntest \"$HOME\" = /var/lib/crystal-forge\ntest \"$XDG_CONFIG_HOME\" = /var/lib/crystal-forge/.config\n"
            )).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
            clear_attic_logged(&remote);
            ensure_attic_login_with_program(
                &remote,
                &endpoint,
                "synthetic-attic-cli-token",
                script.as_os_str(),
            )
            .await
            .unwrap();
            assert!(is_attic_logged(&remote));
            clear_attic_logged(&remote);
        }
    }

    #[test]
    fn attic_login_preserves_ambient_endpoint_precedence_and_fails_closed() {
        let config = CacheConfig {
            cache_type: CacheType::Attic,
            push_to: Some("https://configured.example/team".into()),
            attic_cache_name: Some("team".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_attic_login_endpoint(
                &config,
                Some("http://override.example/prefix/team/nix-cache-info?view=one")
            )
            .unwrap(),
            "http://override.example/prefix/?view=one"
        );
        assert!(resolve_attic_login_endpoint(&config, Some("invalid")).is_err());
        assert!(resolve_attic_login_endpoint(&config, Some("")).is_err());
    }
}
