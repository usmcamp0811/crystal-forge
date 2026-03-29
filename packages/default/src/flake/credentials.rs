//! Credential injection for flake access.
//!
//! This module resolves per-flake credentials from the database and materializes them
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
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::{debug, info};
use url::Url;

use crate::models::flake_credentials::{FlakeCredential, FlakeCredentialAuthType};
use crate::queries::flake_credentials::get_flake_credential;
use crate::queries::flakes::get_flake_by_id;

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
    /// If set, the path of the temp known_hosts file.
    ssh_known_hosts_path: Option<PathBuf>,
    /// The SSH username to use (default "git").
    ssh_username: String,
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
            Some(c) => {
                let flake = get_flake_by_id(pool, flake_id).await?;
                Ok(Some(Self::materialise(c, &flake.repo_url)?))
            }
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
            cmd.env("GIT_CONFIG_COUNT", "1");
            cmd.env("GIT_CONFIG_KEY_0", "credential.helper");
            cmd.env(
                "GIT_CONFIG_VALUE_0",
                format!("store --file {}", netrc.to_string_lossy()),
            );
            cmd.env("NETRC", netrc);
        }

        if let Some(ssh_key) = &self.ssh_key_path {
            debug!("Injecting SSH key for git command");
            let ssh_cmd = build_ssh_command(
                ssh_key,
                self.ssh_known_hosts_path.as_deref(),
                &self.ssh_username,
            );
            cmd.env("GIT_SSH_COMMAND", ssh_cmd);
        }
    }

    /// Apply credential environment variables to a Nix/nix-eval-jobs command.
    ///
    /// For HTTPS PAT, Nix reads `$NETRC` (or `netrc-file` in nix.conf) to authenticate
    /// against fetchers. For SSH, it reads `$GIT_SSH_COMMAND`.
    pub fn apply_to_nix_command(&self, cmd: &mut tokio::process::Command) {
        cmd.env("GIT_TERMINAL_PROMPT", "0");

        if let Some(netrc) = &self.netrc_path {
            debug!("Injecting netrc for nix command: {:?}", netrc);
            cmd.env("NETRC", netrc);
        }

        if let Some(ssh_key) = &self.ssh_key_path {
            debug!("Injecting SSH key for nix command");
            let ssh_cmd = build_ssh_command(
                ssh_key,
                self.ssh_known_hosts_path.as_deref(),
                &self.ssh_username,
            );
            cmd.env("GIT_SSH_COMMAND", ssh_cmd);
        }
    }

    fn materialise(credential: FlakeCredential, repo_url: &str) -> Result<Self> {
        let auth_type = credential
            .auth_type
            .parse::<FlakeCredentialAuthType>()
            .unwrap_or(FlakeCredentialAuthType::Pat);

        let tmpdir = tempfile::tempdir()?;
        let mut netrc_path: Option<PathBuf> = None;
        let mut ssh_key_path: Option<PathBuf> = None;
        let mut ssh_known_hosts_path: Option<PathBuf> = None;
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

                        let repo_host = extract_repo_host(repo_url).ok_or_else(|| {
                            anyhow::anyhow!(
                                "failed to determine repository host for flake {} from repo_url '{}'",
                                credential.flake_id,
                                repo_url
                            )
                        })?;

                        let netrc_file = tmpdir.path().join("netrc");
                        let netrc_content =
                            format!("machine {repo_host} login {username} password {secret}\n");
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
                        let key_content = if secret.ends_with('\n') {
                            secret.to_string()
                        } else {
                            format!("{secret}\n")
                        };
                        write_secret_file(&key_file, &key_content)?;

                        let known_hosts_file = tmpdir.path().join("known_hosts");
                        write_secret_file(&known_hosts_file, "")?;

                        info!("Prepared SSH key for flake {}", credential.flake_id);
                        ssh_key_path = Some(key_file);
                        ssh_known_hosts_path = Some(known_hosts_file);
                    }
                }
            }
        }

        Ok(Self {
            netrc_path,
            ssh_key_path,
            ssh_known_hosts_path,
            ssh_username,
            _tmpdir: Some(tmpdir),
        })
    }
}

fn build_ssh_command(ssh_key: &Path, known_hosts: Option<&Path>, username: &str) -> String {
    let mut command = format!(
        "ssh -i {} -o BatchMode=yes -o StrictHostKeyChecking=accept-new",
        ssh_key.to_string_lossy()
    );

    if let Some(path) = known_hosts {
        command.push_str(&format!(" -o UserKnownHostsFile={}", path.to_string_lossy()));
    }

    if !username.trim().is_empty() {
        command.push_str(&format!(" -l {}", username.trim()));
    }

    command
}

fn extract_repo_host(repo_url: &str) -> Option<String> {
    let trimmed = repo_url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let normalized = trimmed.strip_prefix("git+").unwrap_or(trimmed);

    if let Ok(parsed) = Url::parse(normalized) {
        if let Some(host) = parsed.host_str() {
            return Some(host.to_string());
        }
    }

    if !normalized.contains("://") {
        let host_part = normalized
            .rsplit_once('@')
            .map_or(normalized, |(_, rhs)| rhs);
        if let Some((host, _)) = host_part.split_once(':') {
            if !host.trim().is_empty() {
                return Some(host.trim().to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod credential_env_tests {
    use super::*;
    use crate::models::flake_credentials::FlakeCredential;
    use chrono::Utc;

    fn sample_pat_credential() -> FlakeCredential {
        FlakeCredential {
            id: 1,
            flake_id: 42,
            auth_type: "pat".to_string(),
            username: Some("x-access-token".to_string()),
            secret_encrypted: Some("ghp_test_token".to_string()),
            ssh_username: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn apply_to_nix_command_exposes_netrc_for_pat() {
        let env = FlakeCredentialEnv::materialise(
            sample_pat_credential(),
            "https://github.com/example/private-repo.git",
        )
        .expect("credential materialization should succeed");

        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "test -n \"$NETRC\" && test -f \"$NETRC\""]);
        env.apply_to_nix_command(&mut cmd);

        let output = cmd.output().await.expect("command should execute");
        assert!(
            output.status.success(),
            "expected NETRC to be set and existing for nix command"
        );
    }
}

fn write_secret_file(path: &PathBuf, content: &str) -> Result<()> {
    let mut file = fs::File::create(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_ssh_command, extract_repo_host};
    use std::path::Path;

    #[test]
    fn extracts_host_from_https_repo_url() {
        assert_eq!(
            extract_repo_host("https://github.com/org/repo.git"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn extracts_host_from_scp_repo_url() {
        assert_eq!(
            extract_repo_host("git@github.com:org/repo.git"),
            Some("github.com".to_string())
        );
    }

    #[test]
    fn extracts_host_from_git_prefixed_repo_url() {
        assert_eq!(
            extract_repo_host("git+ssh://git@gitlab.com/org/repo"),
            Some("gitlab.com".to_string())
        );
    }

    #[test]
    fn ssh_command_enables_known_hosts_and_username() {
        let cmd = build_ssh_command(
            Path::new("/tmp/id_ed25519"),
            Some(Path::new("/tmp/known_hosts")),
            "deploy",
        );

        assert!(cmd.contains("StrictHostKeyChecking=accept-new"));
        assert!(cmd.contains("UserKnownHostsFile=/tmp/known_hosts"));
        assert!(cmd.contains(" -l deploy"));
        assert!(!cmd.contains("StrictHostKeyChecking=no"));
    }

    #[test]
    fn ssh_command_omits_login_when_username_is_blank() {
        let cmd = build_ssh_command(Path::new("/tmp/id_ed25519"), None, "   ");
        assert!(!cmd.contains(" -l "));
    }

    #[test]
    fn returns_none_for_non_parseable_repo_host() {
        assert_eq!(extract_repo_host("not-a-repo"), None);
    }
}
