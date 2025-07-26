use serde::Deserialize;

/// Configuration for nix cache pushing and distribution
#[derive(Clone, Debug, Deserialize)]
pub struct CacheConfig {
    /// Cache URI to push to (e.g., "s3://bucket", "https://cache.example.com")
    pub push_to: Option<String>,

    /// Automatically push builds to cache after successful completion
    #[serde(default)]
    pub push_after_build: bool,

    /// Path to private signing key for cache signatures
    pub signing_key: Option<String>,

    /// Compression method for cache uploads
    pub compression: Option<String>,

    /// Only push builds for these systems/targets
    pub push_filter: Option<Vec<String>>,

    /// Maximum parallel uploads to cache
    #[serde(default = "CacheConfig::default_parallel_uploads")]
    pub parallel_uploads: u32,
}

impl CacheConfig {
    fn default_parallel_uploads() -> u32 {
        4
    }

    /// Check if a target should be pushed to cache based on filters
    pub fn should_push(&self, target_name: &str) -> bool {
        if !self.push_after_build {
            return false;
        }

        match &self.push_filter {
            Some(filters) => filters.iter().any(|filter| target_name.contains(filter)),
            None => true, // No filter means push everything
        }
    }

    /// Get nix copy command args for pushing to cache
    pub fn copy_command_args(&self, store_path: &str) -> Option<Vec<String>> {
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
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            push_filter: None,
            parallel_uploads: 4,
        }
    }
}
