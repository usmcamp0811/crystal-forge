// src/cache/config.rs
use crate::models::config::duration_serde;
use serde::Deserialize;
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct CacheConfig {
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
    // Attic-specific
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
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
    #[serde(default = "CacheConfig::default_push_timeout_seconds")]
    pub push_timeout_seconds: u64,
    #[serde(default)]
    pub force_repush: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub enum CacheType {
    S3,
    Attic,
    Http,
    #[default]
    Nix,
}

#[derive(Debug, Clone)]
pub struct CachePushJob {
    pub derivation_id: i32,
    pub derivation_name: String,
    pub store_path: String,
}

#[derive(Debug, Clone)]
pub struct CacheCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl CacheConfig {
    fn default_parallel_uploads() -> u32 {
        4
    }

    fn default_poll_interval() -> Duration {
        Duration::from_secs(30)
    }

    fn default_push_timeout_seconds() -> u64 {
        600 // 10 minutes
    }

    /// Optional signing step. If `signing_key` is set, run this BEFORE `cache_command`.
    /// Equivalent to: nix store sign --recursive --key-file <key> <store_path>
    pub fn sign_command(&self, store_path: &str) -> Option<CacheCommand> {
        let key_path = self.signing_key.as_ref()?;
        Some(CacheCommand {
            command: "nix".to_string(),
            args: vec![
                "store".to_string(),
                "sign".to_string(),
                "--key-file".to_string(),
                key_path.clone(),
                store_path.to_string(),
            ],
        })
    }

    /// Returns the command and arguments for cache operations (the COPY step).
    pub fn cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        match self.cache_type {
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

        let mut args = vec!["push".to_string()];
        if self.force_repush {
            args.push("--force".to_string());
        }
        args.extend([cache_name.clone(), store_path.to_string()]);

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

        args.extend(["--parallel".to_string(), self.parallel_uploads.to_string()]);
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
        args.extend(["--parallel".to_string(), self.parallel_uploads.to_string()]);
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
            cache_type: CacheType::Nix,
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            push_filter: None,
            parallel_uploads: Self::default_parallel_uploads(),
            s3_region: None,
            s3_profile: None,
            attic_token: None,
            attic_cache_name: None,
            max_retries: 3,
            retry_delay_seconds: 5,
            poll_interval: Self::default_poll_interval(),
            push_timeout_seconds: Self::default_push_timeout_seconds(),
            force_repush: false,
        }
    }
}
