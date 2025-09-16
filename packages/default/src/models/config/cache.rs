use serde::Deserialize;
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
    pub poll_interval: Duration, // Add this line
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

impl CacheConfig {
    fn default_parallel_uploads() -> u32 {
        4
    }

    fn default_poll_interval() -> Duration {
        Duration::from_secs(30)
    }

    pub fn copy_command_args(&self, store_path: &str) -> Option<Vec<String>> {
        match self.cache_type {
            CacheType::S3 => self.s3_copy_args(store_path),
            CacheType::Attic => self.attic_copy_args(store_path),
            CacheType::Http | CacheType::Nix => self.nix_copy_args(store_path),
        }
    }

    fn attic_copy_args(&self, store_path: &str) -> Option<Vec<String>> {
        let cache_name = self.attic_cache_name.as_ref()?;
        Some(vec![
            "attic".to_string(),
            "push".to_string(),
            cache_name.clone(),
            store_path.to_string(),
        ])
    }

    fn s3_copy_args(&self, store_path: &str) -> Option<Vec<String>> {
        let push_to = self.push_to.as_ref()?;
        let mut args = vec![
            "copy".to_string(),
            "--to".to_string(),
            push_to.clone(),
            store_path.to_string(),
        ];

        if let Some(key_path) = &self.signing_key {
            args.extend(["--sign-key".to_string(), key_path.clone()]);
        }

        args.extend(["--parallel".to_string(), self.parallel_uploads.to_string()]);
        Some(args)
    }

    fn nix_copy_args(&self, store_path: &str) -> Option<Vec<String>> {
        let push_to = self.push_to.as_ref()?;
        let mut args = vec![
            "copy".to_string(),
            "--to".to_string(),
            push_to.clone(),
            store_path.to_string(),
        ];

        if let Some(key_path) = &self.signing_key {
            args.extend(["--sign-key".to_string(), key_path.clone()]);
        }

        if let Some(compression) = &self.compression {
            args.extend(["--compression".to_string(), compression.clone()]);
        }

        args.extend(["--parallel".to_string(), self.parallel_uploads.to_string()]);
        Some(args)
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
            cache_type: CacheType::Nix, // Use Nix, not None
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            push_filter: None, // This is Option<Vec<String>>, not Vec<String>
            parallel_uploads: Self::default_parallel_uploads(),
            s3_region: None, // This is Option<String>, not String
            s3_profile: None,
            attic_token: None,
            attic_cache_name: None,
            max_retries: 3,
            retry_delay_seconds: 5,
            poll_interval: Self::default_poll_interval(),
        }
    }
}
