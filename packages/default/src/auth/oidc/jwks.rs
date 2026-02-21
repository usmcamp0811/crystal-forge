//! JWKS (JSON Web Key Set) fetching and caching.

use anyhow::{Context, Result};
use openidconnect::core::CoreJsonWebKeySet;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// JWKS cache entry with expiration.
#[derive(Debug, Clone)]
struct CachedJwks {
    jwks: CoreJsonWebKeySet,
    fetched_at: SystemTime,
    ttl: Duration,
}

impl CachedJwks {
    fn new(jwks: CoreJsonWebKeySet, ttl: Duration) -> Self {
        Self {
            jwks,
            fetched_at: SystemTime::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.fetched_at
            .elapsed()
            .map(|elapsed| elapsed > self.ttl)
            .unwrap_or(true)
    }
}

/// JWKS fetcher with caching support.
///
/// Caches JWKS responses to avoid excessive requests to the OIDC provider.
/// Keys are automatically refreshed when the TTL expires.
#[derive(Debug, Clone)]
pub struct JwksCache {
    cache: Arc<RwLock<Option<CachedJwks>>>,
    jwks_uri: String,
    ttl: Duration,
}

impl JwksCache {
    /// Create a new JWKS cache with the given JWKS URI and TTL.
    ///
    /// # Arguments
    ///
    /// * `jwks_uri` - The JWKS endpoint URL from OIDC discovery
    /// * `ttl` - Time-to-live for cached keys (default: 1 hour)
    pub fn new(jwks_uri: String, ttl: Option<Duration>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            jwks_uri,
            ttl: ttl.unwrap_or(Duration::from_secs(3600)), // 1 hour default
        }
    }

    /// Fetch JWKS from the provider or return cached version if still valid.
    pub async fn fetch(&self) -> Result<CoreJsonWebKeySet> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if !cached.is_expired() {
                    tracing::debug!("Using cached JWKS (not expired)");
                    return Ok(cached.jwks.clone());
                }
                tracing::debug!("Cached JWKS expired, fetching fresh keys");
            }
        }

        // Cache miss or expired - fetch new keys
        let jwks = self.fetch_from_provider().await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedJwks::new(jwks.clone(), self.ttl));
        }

        Ok(jwks)
    }

    /// Fetch JWKS directly from the provider (bypasses cache).
    async fn fetch_from_provider(&self) -> Result<CoreJsonWebKeySet> {
        tracing::debug!("Fetching JWKS from {}", self.jwks_uri);

        let response = reqwest::get(&self.jwks_uri)
            .await
            .context("Failed to fetch JWKS from provider")?;

        if !response.status().is_success() {
            anyhow::bail!(
                "JWKS fetch failed with status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            );
        }

        let jwks: CoreJsonWebKeySet = response
            .json()
            .await
            .context("Failed to parse JWKS response")?;

        tracing::debug!("Successfully fetched JWKS with {} keys", jwks.keys().len());

        Ok(jwks)
    }

    /// Force refresh the JWKS cache (useful for key rotation).
    pub async fn refresh(&self) -> Result<CoreJsonWebKeySet> {
        tracing::info!("Forcing JWKS cache refresh");

        // Clear cache
        {
            let mut cache = self.cache.write().await;
            *cache = None;
        }

        // Fetch new keys
        self.fetch().await
    }

    /// Get the JWKS URI.
    pub fn jwks_uri(&self) -> &str {
        &self.jwks_uri
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_jwks_expiration() {
        let jwks = CoreJsonWebKeySet::new(vec![]);
        let cached = CachedJwks::new(jwks, Duration::from_secs(0));

        // Should expire immediately with 0 TTL
        std::thread::sleep(Duration::from_millis(10));
        assert!(cached.is_expired());
    }

    #[test]
    fn cached_jwks_not_expired() {
        let jwks = CoreJsonWebKeySet::new(vec![]);
        let cached = CachedJwks::new(jwks, Duration::from_secs(3600));

        // Should not be expired within 1 hour
        assert!(!cached.is_expired());
    }

    #[tokio::test]
    #[ignore] // Requires real JWKS endpoint
    async fn fetch_real_jwks() {
        // This would test against a real JWKS endpoint
        // Example: Google's JWKS, Auth0 test tenant, etc.
    }
}
