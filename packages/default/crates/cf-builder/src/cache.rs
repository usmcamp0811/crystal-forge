//! Cache configuration utilities for the builder.
//!
//! Provides conversions from builder cache push configs (from cf-protocol)
//! to local CacheConfig (from cf-config).

use cf_config::config::{CacheConfig, CacheType};
use cf_protocol::builder::BuilderCachePushConfig;

/// Converts server-supplied push settings into the builder's local configuration.
///
/// Dispatched upload caps override local limits; absent caps retain the legacy
/// local fallback. Niks3 caps have a minimum of one and do not use `attic_jobs`.
/// Local settings supply other scheduling limits and legacy signing defaults. Niks3
/// endpoints and credentials come only from the authorized job configuration;
/// missing values never select local credentials or another cache backend.
pub fn builder_cache_to_config(
    push: &BuilderCachePushConfig,
    fallback: &CacheConfig,
) -> CacheConfig {
    let cache_type = match &push.cache_type {
        cf_protocol::cache::CacheType::S3 => CacheType::S3,
        cf_protocol::cache::CacheType::Attic => CacheType::Attic,
        cf_protocol::cache::CacheType::Http => CacheType::Http,
        cf_protocol::cache::CacheType::Nix => CacheType::Nix,
        cf_protocol::cache::CacheType::Niks3 => CacheType::Niks3,
    };
    let is_niks3 = matches!(cache_type, CacheType::Niks3);
    let parallel_uploads = push.parallel_uploads.unwrap_or(fallback.parallel_uploads);
    CacheConfig {
        cache_type,
        push_to: push.push_to.clone(),
        push_after_build: push.push_after_build,
        signing_key: if is_niks3 {
            push.signing_key.clone()
        } else {
            push.signing_key
                .clone()
                .or_else(|| fallback.signing_key.clone())
        },
        compression: push.compression.clone(),
        push_filter: None,
        parallel_uploads: if is_niks3 {
            parallel_uploads.max(1)
        } else {
            parallel_uploads
        },
        s3_region: if is_niks3 {
            None
        } else {
            push.s3_region.clone()
        },
        s3_profile: if is_niks3 {
            None
        } else {
            push.s3_profile.clone()
        },
        s3_access_key_id: if is_niks3 {
            None
        } else {
            push.s3_access_key_id.clone()
        },
        s3_secret_access_key: if is_niks3 {
            None
        } else {
            push.s3_secret_access_key.clone()
        },
        s3_session_token: if is_niks3 {
            None
        } else {
            push.s3_session_token.clone()
        },
        s3_endpoint_url: if is_niks3 {
            None
        } else {
            push.s3_endpoint_url.clone()
        },
        attic_token: if is_niks3 {
            None
        } else {
            push.attic_token.clone()
        },
        attic_cache_name: push.attic_cache_name.clone(),
        attic_public_key: push.attic_public_key.clone(),
        attic_ignore_upstream_cache_filter: push.attic_ignore_upstream_cache_filter,
        attic_jobs: if push.attic_jobs == 0 {
            fallback.attic_jobs
        } else {
            push.attic_jobs
        },
        niks3_server_url: push.niks3_server_url.clone(),
        niks3_write_auth: push.niks3_write_auth.clone(),
        niks3_public_keys: Vec::new(),
        niks3_read_auth: Default::default(),
        max_retries: push.max_retries,
        retry_delay_seconds: push.retry_delay_seconds,
        poll_interval: fallback.poll_interval,
        push_timeout_seconds: push.push_timeout_seconds,
        force_repush: push.force_repush,
        require_sigs: push.require_sigs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_protocol::cache::{CacheReadAuth, Niks3WriteAuth};

    #[test]
    fn niks3_upload_cap_uses_job_setting_or_legacy_fallback_not_attic_jobs() {
        let mut fallback = CacheConfig {
            parallel_uploads: 3,
            attic_jobs: 99,
            ..CacheConfig::default()
        };
        let mut push = BuilderCachePushConfig::disabled();
        push.cache_type = CacheType::Niks3;
        push.attic_jobs = 91;
        for (supplied, expected) in [(None, 3), (Some(7), 7), (Some(1), 1), (Some(0), 1)] {
            push.parallel_uploads = supplied;
            assert_eq!(
                builder_cache_to_config(&push, &fallback).parallel_uploads,
                expected
            );
        }
        push.parallel_uploads = None;
        fallback.parallel_uploads = 0;
        assert_eq!(
            builder_cache_to_config(&push, &fallback).parallel_uploads,
            1
        );
    }

    #[test]
    fn niks3_job_settings_do_not_fall_back_to_local_credentials() {
        let fallback = CacheConfig {
            niks3_server_url: Some("https://local-write.example.org".into()),
            niks3_write_auth: Some(Niks3WriteAuth::Token {
                token: "local-secret".into(),
            }),
            signing_key: Some("local-signing-key".into()),
            parallel_uploads: 3,
            ..CacheConfig::default()
        };
        let mut push = BuilderCachePushConfig::disabled();
        push.cache_type = CacheType::Niks3;
        push.push_after_build = true;
        push.push_to = Some("https://read.example.org".into());
        push.s3_secret_access_key = Some("unrelated-secret".into());
        let missing = builder_cache_to_config(&push, &fallback);
        assert!(missing.niks3_server_url.is_none());
        assert!(missing.niks3_write_auth.is_none());
        assert!(missing.signing_key.is_none());
        assert!(missing.s3_secret_access_key.is_none());

        push.niks3_server_url = Some("https://job-write.example.org".into());
        push.niks3_write_auth = Some(Niks3WriteAuth::Token {
            token: "job-secret".into(),
        });
        let mapped = builder_cache_to_config(&push, &fallback);
        assert_eq!(mapped.niks3_server_url, push.niks3_server_url);
        assert_eq!(mapped.niks3_write_auth, push.niks3_write_auth);
        assert_eq!(mapped.push_to, push.push_to);
        assert_eq!(mapped.parallel_uploads, 3);
        assert_eq!(mapped.niks3_read_auth, CacheReadAuth::None);
        assert!(mapped.niks3_public_keys.is_empty());
    }
}
