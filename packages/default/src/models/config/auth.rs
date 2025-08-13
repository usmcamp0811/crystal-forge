use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct AuthConfig {
    /// Path to SSH private key for Git authentication
    pub ssh_key_path: Option<PathBuf>,

    /// Path to SSH known_hosts file  
    pub ssh_known_hosts_path: Option<PathBuf>,

    /// Path to .netrc file for HTTPS Git authentication
    pub netrc_path: Option<PathBuf>,

    /// Whether to disable strict host key checking for SSH
    pub ssh_disable_strict_host_checking: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            ssh_key_path: Some(PathBuf::from("/var/lib/crystal-forge/.ssh/id_ed25519")),
            ssh_known_hosts_path: Some(PathBuf::from("/var/lib/crystal-forge/.ssh/known_hosts")),
            netrc_path: Some(PathBuf::from("/var/lib/crystal-forge/.netrc")),
            ssh_disable_strict_host_checking: false,
        }
    }
}
