//! Cache configuration utilities for the builder.
//!
//! Provides conversions from builder cache push configs (from cf-protocol)
//! to local CacheConfig (from cf-config).

use cf_config::config::{CacheConfig, CacheType};
use cf_protocol::builder::BuilderCachePushConfig;

/// Convert a `BuilderCachePushConfig` (delivered from the server) into a
/// local `CacheConfig`, filling missing fields from a local fallback.
pub fn builder_cache_to_config(
    push: &BuilderCachePushConfig,
    fallback: &CacheConfig,
) -> CacheConfig {
    let cache_type = match &push.cache_type {
        cf_protocol::cache::CacheType::S3 => CacheType::S3,
        cf_protocol::cache::CacheType::Attic => CacheType::Attic,
        cf_protocol::cache::CacheType::Http => CacheType::Http,
        cf_protocol::cache::CacheType::Nix => CacheType::Nix,
    };
    CacheConfig {
        cache_type,
        push_to: push.push_to.clone(),
        push_after_build: push.push_after_build,
        signing_key: push
            .signing_key
            .clone()
            .or_else(|| fallback.signing_key.clone()),
        compression: push.compression.clone(),
        push_filter: None,
        parallel_uploads: fallback.parallel_uploads,
        s3_region: push.s3_region.clone(),
        s3_profile: push.s3_profile.clone(),
        s3_access_key_id: push.s3_access_key_id.clone(),
        s3_secret_access_key: push.s3_secret_access_key.clone(),
        s3_session_token: push.s3_session_token.clone(),
        s3_endpoint_url: push.s3_endpoint_url.clone(),
        attic_token: push.attic_token.clone(),
        attic_cache_name: push.attic_cache_name.clone(),
        attic_ignore_upstream_cache_filter: push.attic_ignore_upstream_cache_filter,
        attic_jobs: if push.attic_jobs == 0 {
            fallback.attic_jobs
        } else {
            push.attic_jobs
        },
        max_retries: push.max_retries,
        retry_delay_seconds: push.retry_delay_seconds,
        poll_interval: fallback.poll_interval,
        push_timeout_seconds: push.push_timeout_seconds,
        force_repush: push.force_repush,
        require_sigs: push.require_sigs,
    }
}
