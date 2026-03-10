use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Cache destination to environment assignment
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CacheDestinationEnvironment {
    pub cache_destination_id: i32,
    pub environment_id: i32,
    pub created_at: DateTime<Utc>,
}

/// Cache destination configuration stored in database
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CacheDestination {
    pub id: i32,
    pub name: String,
    pub cache_type: String, // 'S3', 'Attic', 'Http', 'Nix'

    // Common fields
    pub push_to: Option<String>,
    pub enabled: bool,
    pub signing_key_path: Option<String>,
    pub compression: Option<String>,

    // S3-specific
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,

    // Attic-specific
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,

    // Performance tuning
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,

    // Push behavior
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// DTO for creating a new cache destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCacheDestination {
    pub name: String,
    pub cache_type: String,
    pub push_to: Option<String>,
    pub enabled: Option<bool>,
    pub signing_key_path: Option<String>,
    pub compression: Option<String>,
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    // Environment assignments (empty = global cache)
    pub environment_ids: Option<Vec<i32>>,
}

/// DTO for updating an existing cache destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCacheDestination {
    pub name: Option<String>,
    pub cache_type: Option<String>,
    pub push_to: Option<String>,
    pub enabled: Option<bool>,
    pub signing_key_path: Option<String>,
    pub compression: Option<String>,
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    // Environment assignments (None = don't change, Some(vec) = update assignments)
    pub environment_ids: Option<Vec<i32>>,
}

impl CreateCacheDestination {
    /// Validate the cache destination based on cache type
    pub fn validate(&self) -> Result<(), String> {
        // Validate cache type
        match self.cache_type.as_str() {
            "S3" | "Attic" | "Http" | "Nix" => {}
            _ => {
                return Err(format!(
                    "Invalid cache_type: {}. Must be one of: S3, Attic, Http, Nix",
                    self.cache_type
                ))
            }
        }

        // Validate name is not empty
        if self.name.trim().is_empty() {
            return Err("Cache destination name cannot be empty".to_string());
        }

        // Type-specific validation
        match self.cache_type.as_str() {
            "Attic" => {
                if self.attic_cache_name.is_none()
                    || self
                        .attic_cache_name
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err("attic_cache_name is required for Attic cache type".to_string());
                }
            }
            "S3" | "Http" | "Nix" => {
                if self.push_to.is_none()
                    || self
                        .push_to
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err(format!(
                        "push_to URL is required for {} cache type",
                        self.cache_type
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_attic_requires_cache_name() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "Attic".to_string(),
            push_to: None,
            attic_cache_name: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            environment_ids: None,
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("attic_cache_name"));
    }

    #[test]
    fn test_validate_s3_requires_push_to() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "S3".to_string(),
            push_to: None,
            attic_cache_name: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            environment_ids: None,
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("push_to"));
    }

    #[test]
    fn test_validate_attic_succeeds_with_cache_name() {
        let create = CreateCacheDestination {
            name: "test-attic".to_string(),
            cache_type: "Attic".to_string(),
            push_to: None,
            attic_cache_name: Some("my-cache".to_string()),
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
        };

        let result = create.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_cache_type() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "InvalidType".to_string(),
            push_to: None,
            attic_cache_name: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid cache_type"));
    }

    #[test]
    fn test_validate_rejects_empty_name() {
        let create = CreateCacheDestination {
            name: "   ".to_string(),
            cache_type: "Nix".to_string(),
            push_to: Some("https://cache.example.com".to_string()),
            attic_cache_name: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("name cannot be empty"));
    }
}
