use super::duration_serde;
pub use cf_protocol::cache::CacheType;
use cf_protocol::cache::{CacheReadAuth, Niks3WriteAuth};
use serde::Deserialize;
use std::time::Duration;

/// Configures publication and read-plane trust for one cache destination.
///
/// Niks3 write credentials and read credentials are independent. Consumers must
/// use the prepared credential helpers to own files until child processes exit.
#[derive(Clone, Debug, Deserialize)]
pub struct CacheConfig {
    /// Niks3 write API URL, distinct from the read/substituter URL.
    #[serde(default)]
    pub niks3_server_url: Option<String>,
    /// Authentication for Niks3 writes over HTTPS.
    #[serde(default)]
    pub niks3_write_auth: Option<Niks3WriteAuth>,
    /// Signing keys trusted for Niks3 cache reads.
    #[serde(default)]
    pub niks3_public_keys: Vec<String>,
    /// Independent read-plane authentication; absent means public reads.
    #[serde(default)]
    pub niks3_read_auth: CacheReadAuth,
    #[serde(default)]
    pub cache_type: CacheType,
    pub push_to: Option<String>,
    #[serde(default)]
    pub push_after_build: bool,
    pub signing_key: Option<String>,
    pub compression: Option<String>,
    pub push_filter: Option<Vec<String>>,
    #[serde(default = "CacheConfig::default_parallel_uploads")]
    pub parallel_uploads: u32,
    // S3-specific
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_session_token: Option<String>,
    pub s3_endpoint_url: Option<String>,
    // Attic-specific
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    pub attic_ignore_upstream_cache_filter: bool,
    pub attic_jobs: u32,
    // Retry configuration
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_delay_seconds: u64,
    #[serde(
        default = "CacheConfig::default_poll_interval",
        with = "duration_serde"
    )]
    pub poll_interval: Duration,
    /// Timeout for each cache push attempt in seconds (default: 3600 = 1 hour)
    #[serde(default = "CacheConfig::default_push_timeout_seconds")]
    pub push_timeout_seconds: u64,
    #[serde(default)]
    pub force_repush: bool,
    pub require_sigs: bool,
}

#[derive(Debug, Clone)]
pub struct CacheCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl CacheConfig {
    fn default_parallel_uploads() -> u32 {
        1
    }

    fn default_poll_interval() -> Duration {
        Duration::from_secs(30)
    }

    pub fn default_push_timeout_seconds() -> u64 {
        3600 // 1 hour
    }

    /// Optional signing step. If `signing_key` is set, run this BEFORE `cache_command`.
    pub fn sign_command(&self, store_path: &str) -> Option<CacheCommand> {
        let key_path = self.signing_key.as_ref()?;
        Some(CacheCommand {
            command: "nix".to_string(),
            args: vec![
                "store".to_string(),
                "sign".to_string(),
                "--recursive".to_string(),
                "--key-file".to_string(),
                key_path.clone(),
                store_path.to_string(),
            ],
        })
    }

    /// Returns the command and arguments for cache operations.
    ///
    /// Returns `None` for Niks3: a generic command cannot own authentication
    /// files. Use [`crate::cache_credentials::PreparedNiks3Push`] instead.
    pub fn cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        match self.cache_type {
            CacheType::Niks3 => None,
            CacheType::S3 => self.s3_cache_command(store_path),
            CacheType::Attic => self.attic_cache_command(store_path),
            CacheType::Http | CacheType::Nix => self.nix_cache_command(store_path),
        }
    }

    /// Legacy: still returns args only.
    pub fn copy_command_args(&self, store_path: &str) -> Option<Vec<String>> {
        self.cache_command(store_path).map(|cmd| cmd.args)
    }

    fn attic_cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        let cache_name = self.attic_cache_name.as_ref()?;
        let mut args = vec![
            "push".to_string(),
            cache_name.clone(),
            store_path.to_string(),
        ];
        if self.attic_ignore_upstream_cache_filter {
            args.push("--ignore-upstream-cache-filter".to_string());
        }
        args.extend(["--jobs".to_string(), self.attic_jobs.to_string()]);
        Some(CacheCommand {
            command: "attic".to_string(),
            args,
        })
    }

    fn s3_cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        let push_to = self.push_to.as_ref()?;
        let mut args = vec!["copy".to_string(), "--to".to_string(), push_to.clone()];
        if self.force_repush {
            args.push("--refresh".to_string());
        }
        if let Some(compression) = &self.compression {
            args.extend(["--compression".to_string(), compression.clone()]);
        }
        args.push(store_path.to_string());
        Some(CacheCommand {
            command: "nix".to_string(),
            args,
        })
    }

    fn nix_cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        let push_to = self.push_to.as_ref()?;
        let mut args = vec!["copy".to_string(), "--to".to_string(), push_to.clone()];
        if self.force_repush {
            args.push("--refresh".to_string());
        }
        if let Some(compression) = &self.compression {
            args.extend(["--compression".to_string(), compression.clone()]);
        }
        args.push(store_path.to_string());
        Some(CacheCommand {
            command: "nix".to_string(),
            args,
        })
    }

    pub fn should_push(&self, target_name: &str) -> bool {
        if !self.push_after_build {
            return false;
        }
        match &self.push_filter {
            Some(filters) => filters.iter().any(|filter| target_name.contains(filter)),
            None => true,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            niks3_server_url: None,
            niks3_write_auth: None,
            niks3_public_keys: Vec::new(),
            niks3_read_auth: CacheReadAuth::None,
            cache_type: CacheType::Nix,
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            push_filter: None,
            parallel_uploads: Self::default_parallel_uploads(),
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_cache_name: None,
            attic_public_key: None,
            attic_ignore_upstream_cache_filter: true,
            attic_jobs: 5,
            max_retries: 3,
            retry_delay_seconds: 5,
            poll_interval: Self::default_poll_interval(),
            push_timeout_seconds: Self::default_push_timeout_seconds(),
            force_repush: false,
            require_sigs: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_cache_configuration_defaults_niks3_fields() {
        let config: CacheConfig = serde_json::from_value(serde_json::json!({
            "attic_ignore_upstream_cache_filter":true, "attic_jobs":5, "require_sigs":true
        }))
        .unwrap();
        assert!(config.niks3_server_url.is_none());
        assert!(config.niks3_write_auth.is_none());
        assert!(config.niks3_public_keys.is_empty());
        assert_eq!(config.niks3_read_auth, CacheReadAuth::None);
        let default = CacheConfig::default();
        assert!(default.niks3_server_url.is_none());
        assert!(default.niks3_write_auth.is_none());
        assert!(default.niks3_public_keys.is_empty());
        assert_eq!(default.niks3_read_auth, CacheReadAuth::None);
    }

    #[test]
    fn niks3_config_requires_prepared_command_and_redacts_auth() {
        let config: CacheConfig = serde_json::from_value(serde_json::json!({
            "cache_type":"Niks3", "push_to":"https://read.example",
            "niks3_server_url":"https://write.example",
            "niks3_write_auth":{"kind":"token", "token":"secret-token"},
            "niks3_public_keys":["one:key", "two:key"],
            "niks3_read_auth":{"kind":"mtls", "client_certificate":"secret-cert",
                "client_private_key":"secret-key", "ca_certificate":"secret-ca"},
            "attic_ignore_upstream_cache_filter":true, "attic_jobs":5, "require_sigs":true
        }))
        .unwrap();
        assert_eq!(config.cache_type, CacheType::Niks3);
        assert_eq!(config.niks3_public_keys, ["one:key", "two:key"]);
        assert!(config.cache_command("/nix/store/abc-output").is_none());
        assert!(config.copy_command_args("/nix/store/abc-output").is_none());
        for secret in ["secret-token", "secret-key", "secret-cert", "secret-ca"] {
            assert!(!format!("{config:?}").contains(secret));
        }
    }
}
