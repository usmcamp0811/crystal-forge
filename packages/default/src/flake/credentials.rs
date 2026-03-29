//! Credential injection for flake access.
//!
//! This module resolves per-flake credentials from the database and materialises them
//! into environment variables or temporary files so that `git` and `nix` commands can
//! access private repositories transparently.
//!
//! # Supported auth types
//!
//! | `auth_type`        | Mechanism                                                     |
//! |--------------------|---------------------------------------------------------------|
//! | `pat`              | netrc entry (`machine <host> login <username> password <tok>`) |
//! | `username_password`| same netrc entry                                              |
//! | `ssh_key`          | temp private-key file + `GIT_SSH_COMMAND`                     |
//!
//! # Security
//!
//! * Temp files are created with mode 0o600 and removed when the guard is dropped.
//! * The secret is never logged at any level.
//! * netrc files are written to a per-invocation tmpdir so concurrent evaluations
//!   cannot step on each other.

use anyhow::Result;
use sqlx::PgPool;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;
use tracing::{debug, info};

use crate::models::flake_credentials::{FlakeCredential, FlakeCredentialAuthType};
use crate::queries::flake_credentials::get_flake_credential;

/// An active credential environment for a flake command.
///
/// Holds any temp files that are needed. These are deleted when the guard is dropped.
/// Pass a reference to [`apply_to_git_command`] / [`apply_to_nix_command`] to wire
/// the credentials into a [`tokio::process::Command`].
pub struct FlakeCredentialEnv {
    /// If set, the path of the temp netrc file.
    netrc_path: Option<PathBuf>,
    /// If set, the path of the temp SSH private-key file.
    ssh_key_path: Option<PathBuf>,
    /// The SSH username to use (default "git"); reserved for future SSH config generation.
    #[allow(dead_code)]
    ssh_username: String,
    /// Strict host checking disabled flag for SSH.
    strict_host_check_disabled: bool,
    /// Keeps the TempDir alive so files are cleaned up on drop.
    _tmpdir: Option<TempDir>,
}

impl FlakeCredentialEnv {
    /// Load credentials from the DB for the given `flake_id`.
    ///
    /// Returns `None` if the flake has no credentials stored (public repo).
    pub async fn load(pool: &PgPool, flake_id: i32) -> Result<Option<Self>> {
        let credential = get_flake_credential(pool, flake_id).await?;
        match credential {
            Some(c) => Ok(Some(Self::materialise(c)?)),
            None => Ok(None),
        }
    }

    /// Returns `true` when this env has actual credentials to inject.
    pub fn has_credentials(&self) -> bool {
        self.netrc_path.is_some() || self.ssh_key_path.is_some()
    }

    /// Apply credential environment variables to a `git` command.
    ///
    /// Sets:
    /// - `GIT_CONFIG_NOSYSTEM=1` — prevents accidental system-level config leaks
    /// - `GIT_TERMINAL_PROMPT=0` — disables interactive credential prompts
    /// - `NETRC` / `GIT_SSH_COMMAND` — credentials-specific vars
    pub fn apply_to_git_command(&self, cmd: &mut tokio::process::Command) {
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.env("GIT_CONFIG_NOSYSTEM", "1");

        if let Some(netrc) = &self.netrc_path {
            debug!("Injecting netrc for git command: {:?}", netrc);
            // Git honours $HOME/.netrc, but we use GIT_CONFIG_COUNT to override the
            // credential helper so we can point at our temp file instead.
            cmd.env("GIT_CONFIG_COUNT", "1");
            cmd.env("GIT_CONFIG_KEY_0", "credential.helper");
            cmd.env(
                "GIT_CONFIG_VALUE_0",
                format!(
                    "store --file {}",
                    netrc.to_string_lossy()
                ),
            );
            // Also set NETRC directly so libcurl-backed git transports pick it up.
            cmd.env("NETRC", netrc);
        }

        if let Some(ssh_key) = &self.ssh_key_path {
            debug!("Injecting SSH key for git command");
            let strict = if self.strict_host_check_disabled {
                "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
            } else {
                ""
            };
            let ssh_cmd = format!(
                "ssh -i {} -o BatchMode=yes {}",
                ssh_key.to_string_lossy(),
                strict,
            );
            cmd.env("GIT_SSH_COMMAND", ssh_cmd.trim());
        }
    }

    /// Apply credential environment variables to a Nix/nix-eval-jobs command.
    ///
    /// For HTTPS PAT, Nix reads `$NETRC` (or `netrc-file` in nix.conf) to authenticate
    /// against fetchers.  For SSH, it reads `$GIT_SSH_COMMAND`.
    pub fn apply_to_nix_command(&self, cmd: &mut tokio::process::Command) {
        // Disable interactive prompts — Nix evaluations must never block on stdin.
        cmd.env("GIT_TERMINAL_PROMPT", "0");

        if let Some(netrc) = &self.netrc_path {
            debug!("Injecting netrc for nix command: {:?}", netrc);
            cmd.env("NETRC", netrc);
        }

        if let Some(ssh_key) = &self.ssh_key_path {
            debug!("Injecting SSH key for nix command");
            let strict = if self.strict_host_check_disabled {
                "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
            } else {
                ""
            };
            let ssh_cmd = format!(
                "ssh -i {} -o BatchMode=yes {}",
                ssh_key.to_string_lossy(),
                strict,
            );
            cmd.env("GIT_SSH_COMMAND", ssh_cmd.trim());
        }
    }

    // ── internals ──────────────────────────────────────────────────────────────

    fn materialise(credential: FlakeCredential) -> Result<Self> {
        let auth_type = credential.auth_type.parse::<FlakeCredentialAuthType>()
            .unwrap_or(FlakeCredentialAuthType::Pat);

        let tmpdir = tempfile::tempdir()?;
        let mut netrc_path: Option<PathBuf> = None;
        let mut ssh_key_path: Option<PathBuf> = None;
        let ssh_username = credential
            .ssh_username
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("git")
            .to_string();

        match auth_type {
            FlakeCredentialAuthType::Pat | FlakeCredentialAuthType::UsernamePassword => {
                if let Some(ref secret) = credential.secret_encrypted {
                    let secret = secret.trim();
                    if !secret.is_empty() {
                        let username = credential
                            .username
                            .as_deref()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or(if matches!(auth_type, FlakeCredentialAuthType::Pat) {
                                "oauth2"
                            } else {
                                "user"
                            });

                        // Write a netrc file with a catch-all machine entry.
                        // Nix/git will use the first matching entry.
                        let netrc_file = tmpdir.path().join("netrc");
                        let netrc_content = format!(
                            "default login {username} password {secret}\n"
                        );
                        write_secret_file(&netrc_file, &netrc_content)?;
                        info!("Prepared netrc credential for flake {}", credential.flake_id);
                        netrc_path = Some(netrc_file);
                    }
                }
            }
            FlakeCredentialAuthType::SshKey => {
                if let Some(ref secret) = credential.secret_encrypted {
                    let secret = secret.trim();
                    if !secret.is_empty() {
                        let key_file = tmpdir.path().join("id_ed25519");
                        // Ensure trailing newline for OpenSSH
                        let key_content = if secret.ends_with('\n') {
                            secret.to_string()
                        } else {
                            format!("{secret}\n")
                        };
                        write_secret_file(&key_file, &key_content)?;
                        info!("Prepared SSH key for flake {}", credential.flake_id);
                        ssh_key_path = Some(key_file);
                    }
                }
            }
        }

        Ok(Self {
            netrc_path,
            ssh_key_path,
            ssh_username,
            strict_host_check_disabled: true, // safe default for CI/server context
            _tmpdir: Some(tmpdir),
        })
    }
}

/// Write `content` to `path` with mode 0o600 (owner read/write only).
fn write_secret_file(path: &PathBuf, content: &str) -> Result<()> {
    let mut file = fs::File::create(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    file.write_all(content.as_bytes())?;
    Ok(())
}
