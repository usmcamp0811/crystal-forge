//! API handlers for builder management and work queue operations.
//!
//! This module provides two sets of endpoints:
//! 1. Builder Management (Admin-only): CRUD operations for builders
//! 2. Builder Work Queue (Builder-authenticated): Job polling and status updates

use axum::{
    Json,
    body::Body,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose};
use bytes::Bytes;
use ed25519_dalek::{Signature, Verifier};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::process::Command;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{
    require_admin, require_operator_or_admin, require_viewer_or_above,
};
use crate::handlers::builder_request::{
    authenticate_builder_request, authenticate_builder_request_allow_inactive,
};
use crate::models::builders::{
    AppendLogsRequest, BuildJob, Builder, BuilderCachePushConfig, BuilderCreatedResponse,
    BuilderMetrics, BuilderSummary, BuilderWithEnvironments, CreateBuilderRequest,
    EstablishBuilderSessionRequest, EstablishBuilderSessionResponse, EvaluatorFingerprint,
    KeypairRegeneratedResponse, NextJobRequest, RemoteBuildExecutionStrategy, ReportMetricsRequest,
    ResolveBuilderIdRequest, ResolveBuilderIdResponse, SourceInputDeliveryMode,
    UpdateBuilderEnvironmentsRequest, UpdateBuilderPublicKeyRequest, UpdateBuilderRequest,
    VerifiedSourceIdentity,
};
use crate::models::cache_destination::CacheDestination;
use crate::models::public_key::PublicKey;
use crate::queries::builders;

const NIX_STORE_EXPORT_ARG_BYTES_LIMIT: usize = 128 * 1024;
const ATTIC_PUSH_PATH_CHUNK_SIZE: usize = 200;
const BUILDER_SESSION_STALE_TIMEOUT_SECS: i64 = 60;

// Per-mirror mutex map: prevents concurrent git clone/fetch into the same bare
// mirror when multiple jobs for the same repo are claimed at the same time.
// The lock scope covers clone/fetch AND archive generation so a reader never
// opens a partially-written mirror.
static MIRROR_LOCKS: std::sync::OnceLock<
    dashmap::DashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>,
> = std::sync::OnceLock::new();

fn mirror_lock(mirror_id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    let map = MIRROR_LOCKS.get_or_init(dashmap::DashMap::new);
    map.entry(mirror_id.to_string())
        .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Returns true only when the server is explicitly configured to trust
/// forwarded-proto headers from its reverse proxy AND those headers assert
/// HTTPS for the current request.
///
/// The two-layer check prevents a builder from spoofing the header over a
/// direct plaintext connection:
///   1. `trust_forwarded_builder_https` must be `true` in `[server]` config —
///      the operator opts in by confirming their proxy strips/rewrites these
///      headers before forwarding.
///   2. At least one of the standard forwarded-proto headers in the request
///      must assert "https".
///
/// When `trust_forwarded_builder_https` is `false` (the default) this always
/// returns `false`, so credential-bearing cache config is never sent.
fn builder_https_verified_by_trusted_proxy(
    server_config: &crate::config::ServerConfig,
    headers: &HeaderMap,
) -> bool {
    if !server_config.trust_forwarded_builder_https {
        return false;
    }
    forwarded_header_asserts_https(headers)
}

fn build_log_append_status_allowed(status: &str) -> bool {
    matches!(status, "queued" | "building" | "cancelling")
}

fn forwarded_header_asserts_https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("https"))
        || headers
            .get("forwarded")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| forwarded_field_has_https(v))
        || headers
            .get("x-forwarded-ssl")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("on"))
        || headers
            .get("x-url-scheme")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

fn forwarded_field_has_https(value: &str) -> bool {
    value
        .split(',')
        .flat_map(|part| part.split(';'))
        .map(str::trim)
        .any(|part| {
            let Some((name, val)) = part.split_once('=') else {
                return false;
            };
            name.eq_ignore_ascii_case("proto")
                && val.trim_matches('"').eq_ignore_ascii_case("https")
        })
}

fn cache_push_config_contains_credentials(config: &BuilderCachePushConfig) -> bool {
    config.attic_token.is_some()
        || config.s3_access_key_id.is_some()
        || config.s3_secret_access_key.is_some()
        || config.s3_session_token.is_some()
}

fn parse_derivation_requisites(stdout: &[u8], drv_path: &str) -> Vec<String> {
    let mut paths = Vec::new();

    for line in String::from_utf8_lossy(stdout).lines() {
        let path = line.trim();
        if path.is_empty() || paths.iter().any(|existing| existing == path) {
            continue;
        }
        paths.push(path.to_string());
    }

    if !paths.iter().any(|path| path == drv_path) {
        paths.insert(0, drv_path.to_string());
    }

    paths
}

fn chunk_derivation_archive_paths(paths: &[String], max_arg_bytes: usize) -> Vec<&[String]> {
    if paths.is_empty() {
        return Vec::new();
    }

    let max_arg_bytes = max_arg_bytes.max(1);
    let mut chunks = Vec::new();
    let mut chunk_start = 0;
    let mut chunk_arg_bytes = 0;

    for (index, path) in paths.iter().enumerate() {
        // Account for the path plus one separator byte. This is conservative
        // enough for argv/env overhead while keeping chunks comfortably below
        // Linux ARG_MAX.
        let path_arg_bytes = path.len() + 1;
        if index > chunk_start && chunk_arg_bytes + path_arg_bytes > max_arg_bytes {
            chunks.push(&paths[chunk_start..index]);
            chunk_start = index;
            chunk_arg_bytes = 0;
        }

        chunk_arg_bytes += path_arg_bytes;
    }

    chunks.push(&paths[chunk_start..]);
    chunks
}

/// Minimal syntactic sanity check for a Nix store path. The real authorization
/// check is set membership in the server-computed manifest; this only rejects
/// obviously malformed input early.
fn looks_like_store_path(path: &str) -> bool {
    path.starts_with("/nix/store/") && !path.contains('\0')
}

/// Compute the authorized requisite manifest for a `.drv` path.
///
/// Runs `nix-store --query --requisites <path>` and returns the resulting
/// store paths sorted and deduplicated. The `.drv` itself is always included.
async fn nix_store_requisites(drv_path: &str) -> Result<Vec<String>, String> {
    let output = Command::new("nix-store")
        .arg("--query")
        .arg("--requisites")
        .arg(drv_path)
        .output()
        .await
        .map_err(|e| format!("failed to run nix-store --query --requisites: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Sanitize: only first line of stderr in the error string.
        let first_line = stderr.lines().next().unwrap_or("unknown error");
        return Err(format!(
            "nix-store --query --requisites failed: {first_line}"
        ));
    }

    let mut paths = parse_derivation_requisites(&output.stdout, drv_path);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Validate that every requested path is a member of the authorized manifest.
///
/// Returns the deduplicated, validated path list on success. Any path outside
/// the authorized set is a hard authorization failure (403) — the server must
/// never export store paths just because a builder asked for them.
fn validate_requested_paths(
    authorized_manifest: &[String],
    requested_paths: &[String],
) -> Result<Vec<String>, StatusCode> {
    use std::collections::HashSet;

    let authorized: HashSet<&str> = authorized_manifest.iter().map(String::as_str).collect();

    let mut seen: HashSet<&str> = HashSet::new();
    let mut validated = Vec::new();

    for path in requested_paths {
        let path = path.trim();
        if path.is_empty() || !looks_like_store_path(path) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if !authorized.contains(path) {
            // Do NOT log the full requested list (could be huge); the caller
            // logs builder/job identifiers.
            return Err(StatusCode::FORBIDDEN);
        }
        if seen.insert(path) {
            validated.push(path.to_string());
        }
    }

    Ok(validated)
}

async fn resolve_cache_destinations_for_derivation(
    pool: &sqlx::PgPool,
    derivation: &crate::derivations::Derivation,
) -> Result<Vec<crate::models::cache_destination::CacheDestination>, StatusCode> {
    let environment_id = match derivation.commit_id {
        Some(commit_id) => sqlx::query_scalar::<_, uuid::Uuid>(
            r#"
            SELECT s.environment_id
            FROM systems s
            JOIN commits c ON c.flake_id = s.flake_id
            WHERE c.id = $1
              AND s.environment_id IS NOT NULL
              AND s.is_active = TRUE
              AND (
                    s.hostname = $2
                    OR NULLIF(s.system_configuration_name, '') = $2
                  )
            ORDER BY CASE
                WHEN NULLIF(s.system_configuration_name, '') = $2 THEN 0
                ELSE 1
            END
            LIMIT 1
            "#,
        )
        .bind(commit_id)
        .bind(&derivation.derivation_name)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::warn!(
                derivation_id = derivation.id,
                derivation_name = %derivation.derivation_name,
                "failed to resolve derivation environment for cache selection: {e}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?,
        None => None,
    };

    let mut destinations = if let Some(environment_id) = environment_id {
        let assigned = crate::queries::cache_destinations::filter_caches_by_environment(
            pool,
            Some(environment_id),
        )
        .await
        .map_err(|e| {
            tracing::warn!(
                derivation_id = derivation.id,
                environment_id = %environment_id,
                "failed to load environment cache destinations for derivation closure publish: {e}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .into_iter()
        .filter(|destination| destination.enabled)
        .collect::<Vec<_>>();

        if assigned.is_empty() {
            crate::queries::cache_destinations::get_global_caches(pool)
                .await
                .map_err(|e| {
                    tracing::warn!(
                        derivation_id = derivation.id,
                        environment_id = %environment_id,
                        "failed to load global cache destinations for derivation closure publish: {e}"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
        } else {
            assigned
        }
    } else {
        crate::queries::cache_destinations::get_global_caches(pool)
            .await
            .map_err(|e| {
                tracing::warn!(
                    derivation_id = derivation.id,
                    "failed to load global cache destinations for derivation closure publish: {e}"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };

    destinations.retain(|destination| destination.enabled);
    Ok(destinations)
}

fn cache_type_from_destination(value: &str) -> cf_protocol::cache::CacheType {
    match value {
        "S3" => cf_protocol::cache::CacheType::S3,
        "Attic" => cf_protocol::cache::CacheType::Attic,
        "Http" => cf_protocol::cache::CacheType::Http,
        _ => cf_protocol::cache::CacheType::Nix,
    }
}

fn builder_cache_push_config_from_destination(
    destination: &CacheDestination,
) -> BuilderCachePushConfig {
    BuilderCachePushConfig {
        cache_type: cache_type_from_destination(&destination.cache_type),
        push_to: destination.push_to.clone(),
        push_after_build: true,
        signing_key: destination.signing_key_path.clone(),
        compression: destination.compression.clone(),
        s3_region: destination.s3_region.clone(),
        s3_profile: destination.s3_profile.clone(),
        s3_access_key_id: destination.s3_access_key_id.clone(),
        s3_secret_access_key: destination.s3_secret_access_key.clone(),
        s3_session_token: destination.s3_session_token.clone(),
        s3_endpoint_url: destination.s3_endpoint_url.clone(),
        attic_token: destination.attic_token.clone(),
        attic_cache_name: destination.attic_cache_name.clone(),
        attic_public_key: destination.attic_public_key.clone(),
        attic_ignore_upstream_cache_filter: destination
            .attic_ignore_upstream_cache_filter
            .unwrap_or(true),
        attic_jobs: destination
            .attic_jobs
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(5),
        max_retries: destination
            .max_retries
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(3),
        retry_delay_seconds: destination
            .retry_delay_seconds
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(5),
        push_timeout_seconds: destination
            .push_timeout_seconds
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_else(crate::config::CacheConfig::default_push_timeout_seconds),
        force_repush: destination.force_repush.unwrap_or(false),
        require_sigs: destination.require_sigs.unwrap_or(true),
    }
}

async fn builder_cache_push_config_for_derivation(
    pool: &sqlx::PgPool,
    derivation: &crate::derivations::Derivation,
) -> Result<BuilderCachePushConfig, StatusCode> {
    let destinations = resolve_cache_destinations_for_derivation(pool, derivation).await?;

    Ok(destinations
        .first()
        .map(builder_cache_push_config_from_destination)
        .unwrap_or_else(BuilderCachePushConfig::disabled))
}

async fn verified_source_identity_for_derivation(
    pool: &sqlx::PgPool,
    derivation: &crate::derivations::Derivation,
) -> anyhow::Result<Option<VerifiedSourceIdentity>> {
    let Some(commit_id) = derivation.commit_id else {
        return Ok(None);
    };
    let commit = crate::queries::commits::get_commit_by_id(pool, commit_id).await?;

    let flake = crate::queries::flakes::get_flake_by_id(pool, commit.flake_id).await?;

    let mirror_id = source_mirror_id(&flake.repo_url);

    Ok(Some(VerifiedSourceIdentity {
        repo_url: flake.repo_url,
        commit_hash: commit.git_commit_hash,
        flake_target: source_flake_target_for_derivation(derivation),
        mirror_id: Some(mirror_id),
        mirror_path: None,
        worktree_path: None,
        lock_hash: None,
        archive_url: None,
        archive_sha256: None,
    }))
}

fn current_evaluator_fingerprint() -> EvaluatorFingerprint {
    EvaluatorFingerprint {
        nix_version: std::env::var("NIX_VERSION").unwrap_or_else(|_| "unknown".to_string()),
        pure_eval: true,
        lockfile_mutation_allowed: false,
    }
}

fn source_flake_target_for_derivation(derivation: &crate::derivations::Derivation) -> String {
    let target = derivation
        .derivation_target
        .as_deref()
        .and_then(|target| target.split_once('#').map(|(_, attr)| attr.to_string()))
        .or_else(|| derivation.derivation_target.clone())
        .unwrap_or_else(|| {
            format!(
                "nixosConfigurations.{}.config.system.build.toplevel",
                derivation.derivation_name
            )
        });

    if matches!(
        derivation.derivation_type,
        crate::derivations::DerivationType::NixOS
    ) && target.starts_with("nixosConfigurations.")
        && !target.contains(".config.system.build.toplevel")
    {
        format!("{target}.config.system.build.toplevel")
    } else {
        target
    }
}

fn source_mirror_id(repo_url: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(repo_url.as_bytes());
    let short = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("repo-{short}")
}

/// Compute the path for the server's cached bare mirror of a repo.
fn server_mirror_path(archive_root: &std::path::Path, repo_url: &str) -> std::path::PathBuf {
    archive_root
        .join("mirrors")
        .join(format!("{}.git", source_mirror_id(repo_url)))
}

/// Ensure the server's bare mirror for `repo_url` contains `commit_hash`.
///
/// Clones the repo bare if the mirror does not exist, or fetches if the commit
/// is not present. Mirrors the builder's `ensure_mirror_has_commit` logic but
/// runs server-side so the server can serve archive tarballs to remote builders.
///
/// `creds` is the optional per-flake credential environment (SSH key / netrc).
/// When `None`, the git commands run without credential injection (public repos only).
async fn ensure_server_mirror_has_commit(
    mirror_path: &std::path::Path,
    repo_url: &str,
    commit_hash: &str,
    creds: Option<&crate::flake::credentials::FlakeCredentialEnv>,
) -> Result<(), StatusCode> {
    let source_err = |msg: String| {
        tracing::error!("server source mirror error: {msg}");
        StatusCode::INTERNAL_SERVER_ERROR
    };

    if !mirror_path.exists() {
        let temp_suffix = format!(".tmp-{}-{}", std::process::id(), uuid::Uuid::new_v4());
        let temp_mirror = mirror_path.with_extension(temp_suffix);

        if let Some(parent) = mirror_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                source_err(format!(
                    "failed to create mirror parent {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let _ = tokio::fs::remove_dir_all(&temp_mirror).await;

        let mut clone_cmd = tokio::process::Command::new("git");
        clone_cmd.kill_on_drop(true);
        clone_cmd
            .arg("clone")
            .arg("--bare")
            .arg(repo_url)
            .arg(&temp_mirror);
        if let Some(c) = creds {
            c.apply_to_git_command(&mut clone_cmd);
        }

        let output = clone_cmd
            .output()
            .await
            .map_err(|e| source_err(format!("failed to spawn git clone --bare: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(source_err(format!(
                "git clone --bare failed for {repo_url}: {stderr}"
            )));
        }

        tokio::fs::rename(&temp_mirror, mirror_path)
            .await
            .map_err(|e| {
                source_err(format!(
                    "failed to install cloned source mirror {} -> {}: {e}",
                    temp_mirror.display(),
                    mirror_path.display()
                ))
            })?;

        tracing::info!("Server source mirror cloned at {}", mirror_path.display());
    }

    // Check if commit is already present.
    let has_commit = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(mirror_path)
        .arg("cat-file")
        .arg("-e")
        .arg(format!("{commit_hash}^{{commit}}"))
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);

    if has_commit {
        return Ok(());
    }

    tracing::info!(
        "Fetching authorized commit {} into server source mirror {}",
        commit_hash,
        mirror_path.display()
    );

    let mut fetch_cmd = tokio::process::Command::new("git");
    fetch_cmd.kill_on_drop(true);
    fetch_cmd
        .arg("--git-dir")
        .arg(mirror_path)
        .arg("fetch")
        .arg("--prune")
        .arg(repo_url)
        .arg("+refs/*:refs/*");
    if let Some(c) = creds {
        c.apply_to_git_command(&mut fetch_cmd);
    }

    let output = fetch_cmd
        .output()
        .await
        .map_err(|e| source_err(format!("failed to spawn git fetch: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(source_err(format!(
            "git fetch failed for {repo_url}: {stderr}"
        )));
    }

    let has_commit_after = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(mirror_path)
        .arg("cat-file")
        .arg("-e")
        .arg(format!("{commit_hash}^{{commit}}"))
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);

    if has_commit_after {
        Ok(())
    } else {
        Err(source_err(format!(
            "commit {commit_hash} not found in server mirror for {repo_url} after fetch"
        )))
    }
}

/// Generate a gzipped tar archive of the bare mirror at the archive path.
///
/// Returns the SHA-256 hex digest of the archive.
/// Generate a gzipped tar archive of the bare mirror at `archive_path`.
///
/// Writes to a `.tmp` file first and renames atomically on success so that
/// concurrent readers or partial downloads never see a half-written archive.
///
/// Returns the SHA-256 hex digest of the completed archive.
///
/// Callers MUST hold the per-mirror lock (via `mirror_lock()`) before calling
/// this function to prevent concurrent mutation of the same mirror.
async fn generate_source_archive(
    mirror_path: &std::path::Path,
    archive_path: &std::path::Path,
) -> Result<String, StatusCode> {
    let source_err = |msg: String| {
        tracing::error!("source archive generation error: {msg}");
        StatusCode::INTERNAL_SERVER_ERROR
    };

    if let Some(parent) = archive_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            source_err(format!(
                "failed to create archive parent {}: {e}",
                parent.display()
            ))
        })?;
    }

    // Write to a temp file then rename atomically so readers never see a
    // partially-written archive. Include the PID to avoid cross-process
    // collision if the server is restarted mid-generation.
    // Build the temp path by appending a suffix to the full archive path string
    // rather than using .with_extension(), which strips only the last component
    // and produces a double extension like ".tar.tar.gz.tmp" for ".tar.gz" paths.
    let tmp_archive = {
        let mut s = archive_path.as_os_str().to_owned();
        s.push(format!(".tmp.{}", std::process::id()));
        std::path::PathBuf::from(s)
    };
    let _ = tokio::fs::remove_file(&tmp_archive).await;

    // Tar the mirror directory. Since mirror_path is like .../<mirror_id>.git,
    // we tar from the parent directory with the basename so extraction produces
    // the correct directory layout.
    let mirror_parent = mirror_path
        .parent()
        .ok_or_else(|| source_err("mirror path has no parent".to_string()))?;
    let mirror_name = mirror_path
        .file_name()
        .ok_or_else(|| source_err("mirror path has no file name".to_string()))?;

    let output = tokio::process::Command::new("tar")
        .kill_on_drop(true)
        .arg("-czf")
        .arg(&tmp_archive)
        .arg("-C")
        .arg(mirror_parent)
        .arg(mirror_name)
        .output()
        .await
        .map_err(|e| source_err(format!("failed to spawn tar: {e}")))?;

    if !output.status.success() {
        let _ = tokio::fs::remove_file(&tmp_archive).await;
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(source_err(format!("tar archive creation failed: {stderr}")));
    }

    // Compute SHA256 of the archive before it becomes visible to readers.
    let hash_output = tokio::process::Command::new("sha256sum")
        .arg(&tmp_archive)
        .output()
        .await
        .map_err(|e| source_err(format!("failed to run sha256sum: {e}")))?;

    if !hash_output.status.success() {
        let _ = tokio::fs::remove_file(&tmp_archive).await;
        let stderr = String::from_utf8_lossy(&hash_output.stderr)
            .trim()
            .to_string();
        return Err(source_err(format!("sha256sum failed: {stderr}")));
    }

    let stdout = String::from_utf8_lossy(&hash_output.stdout);
    let hash = stdout
        .split_whitespace()
        .next()
        .ok_or_else(|| source_err("sha256sum produced no output".to_string()))?
        .to_string();

    // Atomic rename: makes the archive visible to readers only when fully written.
    tokio::fs::rename(&tmp_archive, archive_path)
        .await
        .map_err(|e| {
            source_err(format!(
                "failed to atomically install source archive {} → {}: {e}",
                tmp_archive.display(),
                archive_path.display()
            ))
        })?;

    tracing::info!(
        "Source archive generated at {} (sha256: {})",
        archive_path.display(),
        hash
    );

    Ok(hash)
}

/// Best-effort cleanup of the job-scoped source archive after job completion/failure.
///
/// The archive is stored under `archives/jobs/<job_id>.tar.gz` so cleanup only
/// ever removes the archive for this specific job. Errors are logged but not
/// propagated — archive cleanup must not block job finalization.
async fn cleanup_source_archive(_pool: &PgPool, archive_root: &std::path::Path, job_id: Uuid) {
    // Job-scoped path: each job has its own archive so concurrent jobs for the
    // same repo+commit cannot interfere with each other's downloads.
    let archive_path = job_scoped_archive_path(archive_root, job_id);

    match tokio::fs::remove_file(&archive_path).await {
        Ok(()) => {
            tracing::debug!(
                job_id = %job_id,
                archive_path = %archive_path.display(),
                "Cleaned up job-scoped source archive"
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(
                job_id = %job_id,
                archive_path = %archive_path.display(),
                "Failed to clean up source archive: {e}"
            );
        }
    }
}

/// Compute the job-scoped archive path.
///
/// Archives are stored per-job rather than per-repo+commit to avoid concurrent
/// job races where one job's cleanup deletes an archive another job is still
/// downloading.
fn job_scoped_archive_path(archive_root: &std::path::Path, job_id: Uuid) -> std::path::PathBuf {
    archive_root
        .join("archives")
        .join("jobs")
        .join(format!("{job_id}.tar.gz"))
}

fn parse_next_job_request(body: &[u8]) -> Result<NextJobRequest, StatusCode> {
    if body.is_empty() {
        return Ok(legacy_next_job_request());
    }

    serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)
}

fn legacy_next_job_request() -> NextJobRequest {
    NextJobRequest {
        protocol_version: 1,
        supported_execution_strategies: vec![RemoteBuildExecutionStrategy::ServerDerivation],
    }
}

fn next_job_request_for_method(method: &Method, body: &[u8]) -> Result<NextJobRequest, StatusCode> {
    if *method == Method::GET {
        return Ok(legacy_next_job_request());
    }

    parse_next_job_request(body)
}

fn apply_cache_destination_env(
    command: &mut Command,
    destination: &crate::models::cache_destination::CacheDestination,
) {
    if let Some(value) = destination.s3_access_key_id.as_deref() {
        command.env("AWS_ACCESS_KEY_ID", value);
    }
    if let Some(value) = destination.s3_secret_access_key.as_deref() {
        command.env("AWS_SECRET_ACCESS_KEY", value);
    }
    if let Some(value) = destination.s3_session_token.as_deref() {
        command.env("AWS_SESSION_TOKEN", value);
    }
    if let Some(value) = destination.s3_region.as_deref() {
        command.env("AWS_REGION", value);
        command.env("AWS_DEFAULT_REGION", value);
    }
    if let Some(value) = destination.s3_profile.as_deref() {
        command.env("AWS_PROFILE", value);
    }
    if let Some(value) = destination.s3_endpoint_url.as_deref() {
        command.env("AWS_ENDPOINT_URL", value);
        command.env("AWS_ENDPOINT_URL_S3", value);
    }
    if let Some(value) = destination.attic_token.as_deref() {
        command.env("ATTIC_TOKEN", value);
    }
}

async fn sign_derivation_requisites_for_cache(
    destination: &crate::models::cache_destination::CacheDestination,
    chunk: &[String],
) -> Result<(), StatusCode> {
    let Some(signing_key_path) = destination.signing_key_path.as_deref() else {
        return Ok(());
    };

    let output = Command::new("nix")
        .arg("store")
        .arg("sign")
        .arg("--recursive")
        .arg("--key-file")
        .arg(signing_key_path)
        .args(chunk)
        .output()
        .await
        .map_err(|e| {
            tracing::warn!(
                cache_destination = %destination.name,
                "failed to run nix store sign for derivation closure: {e}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            cache_destination = %destination.name,
            stderr = %stderr,
            "nix store sign failed while publishing derivation closure"
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(())
}

async fn push_derivation_requisites_to_cache_destination(
    destination: &crate::models::cache_destination::CacheDestination,
    archive_paths: &[String],
) -> Result<bool, StatusCode> {
    let remote = std::env::var("ATTIC_REMOTE_NAME").unwrap_or_else(|_| "local".to_string());

    for (chunk_index, chunk) in archive_paths.chunks(ATTIC_PUSH_PATH_CHUNK_SIZE).enumerate() {
        sign_derivation_requisites_for_cache(destination, chunk).await?;

        let mut command = if destination.cache_type.eq_ignore_ascii_case("Attic") {
            let Some(cache_name) = destination.attic_cache_name.as_deref() else {
                tracing::warn!(
                    cache_destination = %destination.name,
                    "assigned Attic cache destination is missing attic_cache_name"
                );
                return Ok(false);
            };
            let cache_ref = if cache_name.contains(':') {
                cache_name.to_string()
            } else {
                format!("{remote}:{cache_name}")
            };
            let attic_jobs = destination.attic_jobs.unwrap_or(5).max(1).to_string();
            let mut command = Command::new("attic");
            command.arg("push").arg(&cache_ref).args(chunk);

            if destination
                .attic_ignore_upstream_cache_filter
                .unwrap_or(true)
            {
                command.arg("--ignore-upstream-cache-filter");
            }

            command.arg("--jobs").arg(attic_jobs);
            command
        } else {
            let Some(push_to) = destination.push_to.as_deref() else {
                tracing::warn!(
                    cache_destination = %destination.name,
                    cache_type = %destination.cache_type,
                    "assigned cache destination is missing push_to"
                );
                return Ok(false);
            };
            let mut command = Command::new("nix");
            command.arg("copy").arg("--to").arg(push_to);

            if destination.force_repush.unwrap_or(false) {
                command.arg("--refresh");
            }
            if let Some(compression) = destination.compression.as_deref() {
                command.arg("--compression").arg(compression);
            }

            command.args(chunk);
            command
        };

        command.env("HOME", "/var/lib/crystal-forge");
        command.env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config");
        apply_cache_destination_env(&mut command, destination);

        tracing::info!(
            cache_destination = %destination.name,
            cache_type = %destination.cache_type,
            chunk_index,
            chunk_path_count = chunk.len(),
            "publishing derivation requisite closure chunk to assigned cache"
        );

        let output = command.output().await.map_err(|e| {
            tracing::warn!(
                cache_destination = %destination.name,
                cache_type = %destination.cache_type,
                chunk_index,
                "failed to run cache publish for derivation closure: {e}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(
                cache_destination = %destination.name,
                cache_type = %destination.cache_type,
                chunk_index,
                stderr = %stderr,
                "cache publish failed while publishing derivation closure"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(true)
}

async fn push_derivation_requisites_to_assigned_cache(
    pool: &sqlx::PgPool,
    derivation: &crate::derivations::Derivation,
    archive_paths: &[String],
) -> Result<bool, StatusCode> {
    let destinations = resolve_cache_destinations_for_derivation(pool, derivation).await?;

    if destinations.is_empty() {
        tracing::debug!(
            derivation_id = derivation.id,
            derivation_name = %derivation.derivation_name,
            "no assigned or global cache destination configured for derivation closure publish"
        );
        return Ok(false);
    }

    let destination = &destinations[0];
    push_derivation_requisites_to_cache_destination(destination, archive_paths).await
}

// =============================================================================
// BUILDER MANAGEMENT ENDPOINTS (Admin-only)
// =============================================================================

fn canonical_signature_payload(method: &str, path: &str, timestamp: &str, body: &[u8]) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(method.len() + path.len() + timestamp.len() + body.len() + 3);
    payload.extend_from_slice(method.as_bytes());
    payload.push(b'\n');
    payload.extend_from_slice(path.as_bytes());
    payload.push(b'\n');
    payload.extend_from_slice(timestamp.as_bytes());
    payload.push(b'\n');
    payload.extend_from_slice(body);
    payload
}

/// POST /api/v1/builders/resolve-id - Resolve a registered builder ID by public key.
///
/// This bootstrap endpoint lets a newly deployed builder start with only its local
/// private key and server URL. The operator registers the derived public key in
/// the UI, then the builder signs this request with the matching private key to
/// discover its server-assigned UUID.
pub async fn resolve_builder_id(
    State(state): State<CFState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ResolveBuilderIdResponse>, (StatusCode, String)> {
    let (request, public_key) = verify_builder_resolve_request(&headers, &body)?;

    let builder = builders::get_builder_by_public_key(&state.pool, &public_key)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to resolve builder by public key");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to resolve builder".to_string(),
            )
        })?;
    let builder_id = builder_id_for_resolved_builder(builder)?;

    tracing::debug!(
        builder_id = %builder_id,
        public_key = %request.public_key,
        "resolved builder ID from public key"
    );

    let Some(session_id) = request.session_id else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Builder session_id is required".to_string(),
        ));
    };

    let recovered_jobs = builders::establish_builder_session(
        &state.pool,
        &builder_id,
        &session_id,
        BUILDER_SESSION_STALE_TIMEOUT_SECS,
        "builder startup recovery",
    )
    .await
    .map_err(|e| {
        tracing::error!(
            builder_id = %builder_id,
            error = %e,
            "failed to establish builder session during startup"
        );
        let message = e.to_string();
        if message.contains("active_builder_session_exists") {
            return (StatusCode::CONFLICT, message);
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to establish builder session".to_string(),
        )
    })?;

    if !recovered_jobs.is_empty() {
        tracing::warn!(
            builder_id = %builder_id,
            recovered_jobs = recovered_jobs.len(),
            "re-queued builder-assigned building jobs during builder startup"
        );
    }

    Ok(Json(ResolveBuilderIdResponse {
        builder_id,
        session_id: Some(session_id),
    }))
}

/// POST /api/v1/builders/:id/session - Establish a process/session for a configured builder ID.
pub async fn establish_builder_session(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<EstablishBuilderSessionResponse>, (StatusCode, String)> {
    let path = format!("/api/v1/builders/{}/session", builder_id);
    let verified = authenticate_builder_request_allow_inactive(
        &headers,
        body.clone(),
        "POST",
        &path,
        &state.pool,
    )
    .await
    .map_err(|status| (status, "Builder authentication failed".to_string()))?;

    if verified.builder_id != builder_id {
        return Err((StatusCode::FORBIDDEN, "Builder ID mismatch".to_string()));
    }

    let request: EstablishBuilderSessionRequest = serde_json::from_slice(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid builder session request".to_string(),
        )
    })?;

    if verified.builder_session_id != Some(request.session_id) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Builder session header does not match request body".to_string(),
        ));
    }

    let recovered_jobs = builders::establish_builder_session(
        &state.pool,
        &builder_id,
        &request.session_id,
        BUILDER_SESSION_STALE_TIMEOUT_SECS,
        "builder startup recovery",
    )
    .await
    .map_err(|e| {
        let message = e.to_string();
        if message.contains("active_builder_session_exists") {
            (StatusCode::CONFLICT, message)
        } else {
            tracing::error!(builder_id = %builder_id, error = %e, "failed to establish builder session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to establish builder session".to_string(),
            )
        }
    })?;

    Ok(Json(EstablishBuilderSessionResponse {
        builder_id,
        session_id: request.session_id,
        recovered_jobs: recovered_jobs.len(),
    }))
}

fn builder_owns_job_session(
    job: &BuildJob,
    builder_id: Uuid,
    builder_session_id: Option<Uuid>,
) -> bool {
    job.builder_id == Some(builder_id)
        && match job.builder_session_id {
            Some(job_session_id) => builder_session_id == Some(job_session_id),
            None => true,
        }
}

fn builder_id_for_resolved_builder(builder: Option<Builder>) -> Result<Uuid, (StatusCode, String)> {
    let builder = builder.ok_or((
        StatusCode::NOT_FOUND,
        "Builder public key is not registered".to_string(),
    ))?;

    if !builder.enabled {
        return Err((
            StatusCode::FORBIDDEN,
            "Builder is registered but disabled".to_string(),
        ));
    }

    Ok(builder.id)
}

fn verify_builder_resolve_request(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(ResolveBuilderIdRequest, PublicKey), (StatusCode, String)> {
    let request: ResolveBuilderIdRequest = serde_json::from_slice(body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid resolve builder request".to_string(),
        )
    })?;

    let public_key = PublicKey::from_base64(&request.public_key, "builder").map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid builder public key".to_string(),
        )
    })?;

    let timestamp_str = headers
        .get("X-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "Missing X-Timestamp header".to_string(),
        ))?;
    let request_timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_str)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid timestamp".to_string()))?
        .with_timezone(&chrono::Utc);
    let now = chrono::Utc::now();
    const FRESHNESS_WINDOW_SECS: i64 = 5 * 60;
    if (now - request_timestamp).num_seconds().abs() > FRESHNESS_WINDOW_SECS {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Builder resolve timestamp outside freshness window".to_string(),
        ));
    }

    let signature_header = headers
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            "Missing X-Signature header".to_string(),
        ))?;
    let signature_bytes = general_purpose::STANDARD
        .decode(signature_header)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid signature".to_string()))?;
    let signature_array: [u8; 64] = signature_bytes.try_into().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid signature length".to_string(),
        )
    })?;
    let signature = Signature::from_bytes(&signature_array);

    let path = "/api/v1/builders/resolve-id";
    let signed_payload = canonical_signature_payload("POST", path, timestamp_str, body);
    public_key
        .verifying_key()
        .verify(&signed_payload, &signature)
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "Builder resolve signature verification failed".to_string(),
            )
        })?;

    Ok((request, public_key))
}
/// POST /api/v1/builders - Create a new builder (admin-only)
///
/// If `public_key` is not provided in request, server generates a proper Ed25519 keypair.
/// Response includes the private key ONLY if generated server-side.
///
/// WARNING: Save the private key immediately - it's shown only once!
pub async fn create_builder(
    State(state): State<CFState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CreateBuilderRequest>,
) -> Result<Json<BuilderCreatedResponse>, (StatusCode, String)> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    };

    // Validate request fields (input sanitization)
    if request.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Builder name cannot be empty".to_string(),
        ));
    }
    if request.name.len() > 255 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Builder name too long (max 255 characters)".to_string(),
        ));
    }

    validate_builder_arch(&request.arch)?;

    // Validate public key if provided (prevent DoS via oversized input)
    if let Some(ref pk) = request.public_key {
        if pk.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Public key cannot be empty".to_string(),
            ));
        }
        if pk.len() > 1000 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Public key too long (max 1000 characters)".to_string(),
            ));
        }
    }

    // Create builder (may generate keypair server-side)
    // PublicKey::from_base64() will validate:
    // - Base64 decoding
    // - Exactly 32 bytes (Ed25519 requirement)
    // - Valid Ed25519 curve point
    let (builder, private_key_option) = builders::create_builder(&state.pool, &request)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create builder: {}", e);
            map_create_builder_error(&e)
        })?;

    // Get environment IDs for response
    let assigned_environment_ids = request.environment_ids.clone();

    Ok(Json(BuilderCreatedResponse {
        builder,
        private_key: private_key_option,
        assigned_environment_ids,
    }))
}

fn map_create_builder_error(error: &anyhow::Error) -> (StatusCode, String) {
    let message = error.to_string();

    if message.contains("Invalid public key")
        || message.contains("must be exactly 32 bytes")
        || message.contains("Failed to decode base64")
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("Invalid public key: {}", error),
        );
    }

    if message.contains("builders_name_key")
        || (message.contains("duplicate key value violates unique constraint")
            && message.contains("builders"))
    {
        return (
            StatusCode::CONFLICT,
            "Builder name already exists".to_string(),
        );
    }

    if message.contains("builder_environment_assignments_environment_id_fkey")
        || (message.contains("violates foreign key constraint")
            && message.contains("environment_id"))
    {
        return (
            StatusCode::BAD_REQUEST,
            "One or more selected environments do not exist".to_string(),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to create builder".to_string(),
    )
}

fn validate_builder_arch(arch: &str) -> Result<(), (StatusCode, String)> {
    let valid_arches = [
        "x86_64-linux",
        "aarch64-linux",
        "aarch64-darwin",
        "x86_64-darwin",
    ];
    if !valid_arches.contains(&arch) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid architecture. Must be one of: {}",
                valid_arches.join(", ")
            ),
        ));
    }

    Ok(())
}

/// GET /api/v1/builders - List all builders (admin-only)
pub async fn list_builders(
    State(state): State<CFState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<BuilderSummary>>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // List builders
    let builders_list = builders::list_builders(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builders_list))
}

/// GET /api/v1/builders/:id - Get builder details (admin-only)
pub async fn get_builder(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<BuilderWithEnvironments>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Get builder with environments
    let builder = builders::get_builder_with_environments(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(builder))
}

/// PATCH /api/v1/builders/:id - Update builder config (admin-only)
pub async fn update_builder(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateBuilderRequest>,
) -> Result<Json<Builder>, (StatusCode, String)> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    };

    if let Some(ref arch) = request.arch {
        validate_builder_arch(arch)?;
    }

    // Update builder
    let builder = builders::update_builder(&state.pool, &builder_id, &request)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update builder".to_string(),
            )
        })?;

    Ok(Json(builder))
}

/// DELETE /api/v1/builders/:id - Deactivate builder (admin-only)
pub async fn deactivate_builder(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Builder>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Deactivate builder
    let builder = builders::deactivate_builder(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// DELETE /api/v1/builders/:id/permanent - Permanently delete builder (admin-only)
pub async fn delete_builder_permanently(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    builders::delete_builder(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/v1/builders/:id/public-key - Update builder public key (admin-only)
pub async fn update_builder_public_key(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateBuilderPublicKeyRequest>,
) -> Result<Json<Builder>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Get builder name for validation
    let existing = builders::get_builder_by_id(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Update public key
    let builder = builders::update_builder_public_key(
        &state.pool,
        &builder_id,
        &request.public_key,
        &existing.name,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(builder))
}

/// POST /api/v1/builders/:id/regenerate-keypair - Generate new Ed25519 keypair for builder (admin-only)
///
/// Generates a cryptographically correct Ed25519 keypair and updates the builder's public key.
/// Returns the new private key ONCE - save it immediately, it won't be shown again!
pub async fn regenerate_builder_keypair(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<KeypairRegeneratedResponse>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Check builder exists
    let existing = builders::get_builder_by_id(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Generate new Ed25519 keypair
    let (public_key_base64, private_key_base64) =
        builders::generate_ed25519_keypair().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Update builder's public key
    builders::update_builder_public_key(
        &state.pool,
        &builder_id,
        &public_key_base64,
        &existing.name,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Return keypair (private key shown ONLY ONCE)
    Ok(Json(KeypairRegeneratedResponse {
        public_key: public_key_base64,
        private_key: private_key_base64,
    }))
}

/// POST /api/v1/build-jobs/:id/prioritize - Move queued build job to front (operator/admin)
pub async fn prioritize_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let Some(_operator_or_admin) = require_operator_or_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    builders::prioritize_build_job(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

/// POST /api/v1/build-jobs/:id/move-up - Move queued build job one position earlier (operator/admin)
pub async fn move_build_job_up(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let Some(_operator_or_admin) = require_operator_or_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    builders::move_build_job_up(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

/// POST /api/v1/build-jobs/:id/move-down - Move queued build job one position later (operator/admin)
pub async fn move_build_job_down(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let Some(_operator_or_admin) = require_operator_or_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    builders::move_build_job_down(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(StatusCode::OK)
}

/// Request body for bulk queue reorder
#[derive(Debug, Deserialize)]
pub struct ReorderBuildQueueRequest {
    pub ordered_job_ids: Vec<Uuid>,
}

/// POST /api/v1/build-queue/reorder - Reorder entire build queue (operator/admin)
pub async fn reorder_build_queue(
    State(state): State<CFState>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ReorderBuildQueueRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let Some(_operator_or_admin) = require_operator_or_admin(&state.pool, &headers).await else {
        return Err((
            StatusCode::FORBIDDEN,
            "Operator or admin access required".to_string(),
        ));
    };

    builders::reorder_build_queue(&state.pool, &request.ordered_job_ids)
        .await
        .map_err(|e| {
            let message = e.to_string();
            tracing::error!("Failed to reorder build queue: {}", message);
            (StatusCode::BAD_REQUEST, message)
        })?;

    Ok(StatusCode::OK)
}

/// POST /api/v1/build-jobs/:id/cancel - Cancel/stop a build job (admin-only)
pub async fn cancel_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<BuildJob>, (StatusCode, String)> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    };

    builders::cancel_build_job(&state.pool, &job_id)
        .await
        .map(Json)
        .map_err(|e| {
            let message = e.to_string();
            if message.to_lowercase().contains("not found") {
                (StatusCode::NOT_FOUND, message)
            } else {
                (StatusCode::BAD_REQUEST, message)
            }
        })
}

/// POST /api/v1/build-jobs/:id/requeue - Requeue a terminal build job (operator/admin)
pub async fn requeue_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<BuildJob>, (StatusCode, String)> {
    let Some(_operator_or_admin) = require_operator_or_admin(&state.pool, &headers).await else {
        return Err((
            StatusCode::FORBIDDEN,
            "Operator or admin access required".to_string(),
        ));
    };

    builders::requeue_build_job_as_new_attempt(&state.pool, &job_id)
        .await
        .map(Json)
        .map_err(|e| {
            let message = e.to_string();
            if message.to_lowercase().contains("not found") {
                (StatusCode::NOT_FOUND, message)
            } else {
                (StatusCode::BAD_REQUEST, message)
            }
        })
}

/// POST /api/v1/build-jobs/:id/force-cancel - Force-cancel a stuck build job (admin-only)
///
/// Use this when a build is stuck in 'cancelling' state and needs immediate termination.
/// Unlike regular cancel, this immediately transitions to 'cancelled' without waiting
/// for builder confirmation.
pub async fn force_cancel_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<BuildJob>, (StatusCode, String)> {
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err((StatusCode::FORBIDDEN, "Admin access required".to_string()));
    };

    builders::force_cancel_build_job(&state.pool, &job_id)
        .await
        .map(Json)
        .map_err(|e| {
            let message = e.to_string();
            if message.to_lowercase().contains("not found") {
                (StatusCode::NOT_FOUND, message)
            } else {
                (StatusCode::BAD_REQUEST, message)
            }
        })
}

/// POST /api/v1/builders/:id/jobs/:job_id/finalize-cancelled
/// Builder-authenticated. Called after the builder has stopped the nix process.
pub async fn finalize_cancelled_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let path = format!(
        "/api/v1/builders/{}/jobs/{}/finalize-cancelled",
        builder_id, job_id
    );
    let verified = authenticate_builder_request(&headers, body, "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    builders::finalize_cancelled_job(
        &state.pool,
        &job_id,
        &builder_id,
        verified.builder_session_id.as_ref(),
    )
    .await
    .map_err(|err| {
        tracing::warn!(
            builder_id = %builder_id,
            job_id = %job_id,
            error = %err,
            "Rejected finalize-cancelled transition due to lease/state mismatch"
        );
        StatusCode::CONFLICT
    })?;

    cleanup_build_log_channel(&state, job_id).await;
    if state.server_config.source_delivery_mode == SourceInputDeliveryMode::ServerBundledArchive {
        cleanup_source_archive(
            &state.pool,
            &state.server_config.source_archive_root,
            job_id,
        )
        .await;
    }
    Ok(StatusCode::OK)
}

/// GET /api/v1/builders/:id/jobs/:job_id/status - Poll job status (builder-authenticated)
pub async fn get_job_status(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let path = format!("/api/v1/builders/{}/jobs/{}/status", builder_id, job_id);
    let verified =
        authenticate_builder_request(&headers, Bytes::new(), "GET", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let status = builders::get_build_job_status(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({ "status": status })))
}

/// GET /api/v1/build-jobs - Paginated build queue with filtering (viewer+)
///
/// Query parameters (all optional):
/// - `page` (default 1), `limit` (default 50)
/// - `status`: comma-separated statuses to include (queued, building, success, failed)
/// - `commit_hash`: prefix match on git commit hash
/// - `flake_name`: partial match on flake name
/// - `config_name`: partial match on system hostname / config name
/// - `queued_after`, `queued_before`: ISO-8601 timestamps bounding queued_at
pub async fn list_build_queue(
    State(state): State<CFState>,
    headers: HeaderMap,
    Query(mut params): Query<crate::api::models::BuildQueueParams>,
) -> Result<Json<crate::api::models::BuildQueuePageResponse>, StatusCode> {
    let Some(_viewer) = require_viewer_or_above(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Clamp per-request limit to prevent unbounded result sets and overflow.
    params.limit = params.limit.max(1).min(crate::api::models::LIMIT_MAX);
    params.page = params.page.max(1);
    if (params.page - 1).checked_mul(params.limit).is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let result = crate::queries::dashboard::list_build_queue_paginated(&state.pool, &params)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list build queue: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(result))
}

/// GET /api/v1/build-jobs/recent - Recent completed/failed builds (viewer+)
pub async fn list_recent_build_jobs(
    State(state): State<CFState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<crate::api::models::BuildQueuePageResponse>, StatusCode> {
    let Some(_viewer) = require_viewer_or_above(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100)
        .max(1)
        .min(crate::api::models::LIMIT_MAX);

    let query = crate::api::models::BuildQueueParams {
        page: 1,
        limit,
        status: params.get("status").cloned(),
        commit_hash: params.get("commit_hash").cloned(),
        flake_name: params
            .get("flake_name")
            .or_else(|| params.get("flake"))
            .cloned(),
        config_name: params.get("config_name").cloned(),
        queued_after: params
            .get("queued_after")
            .and_then(|value| value.parse().ok()),
        queued_before: params
            .get("queued_before")
            .and_then(|value| value.parse().ok()),
        search: params.get("search").cloned(),
        latest_only: params
            .get("latest_only")
            .and_then(|value| value.parse().ok())
            .unwrap_or(false),
    };

    let items = crate::queries::dashboard::fetch_recent_build_history(&state.pool, &query)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(items))
}

/// PATCH /api/v1/builders/:id/environments - Update environment assignments (admin-only)
pub async fn update_builder_environments(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(request): Json<UpdateBuilderEnvironmentsRequest>,
) -> Result<StatusCode, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Update environments
    builders::update_builder_environments(&state.pool, &builder_id, &request.environment_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/builders/:id/metrics - Get builder metrics (admin-only)
pub async fn get_builder_metrics(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<BuilderMetrics>>, StatusCode> {
    // Verify admin authorization
    let Some(_admin_user) = require_admin(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    // Get recent metrics (last 100 data points)
    let metrics = builders::get_builder_metrics(&state.pool, &builder_id, 100)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(metrics))
}

// =============================================================================
// BUILDER WORK QUEUE ENDPOINTS (Builder-authenticated)
// =============================================================================

#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub status: String,
    pub message: String,
}

/// POST /api/v1/builders/:id/heartbeat - Builder heartbeat with metrics
pub async fn builder_heartbeat(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<HeartbeatResponse>, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/heartbeat", builder_id);
    let verified = authenticate_builder_request_allow_inactive(
        &headers,
        body.clone(),
        "POST",
        &path,
        &state.pool,
    )
    .await?;

    // Verify the builder_id in the path matches the authenticated builder
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse metrics from body
    let metrics: ReportMetricsRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Update heartbeat timestamp (marks builder as active)
    builders::update_builder_heartbeat(
        &state.pool,
        &builder_id,
        verified.builder_session_id.as_ref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Record metrics
    builders::record_builder_metrics(&state.pool, &builder_id, &metrics)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(HeartbeatResponse {
        status: "ok".to_string(),
        message: "Heartbeat recorded".to_string(),
    }))
}
/// GET/POST /api/v1/builders/:id/next-job - Get next job for builder
///
/// This endpoint implements the load-based job assignment logic:
/// 1. Filter jobs by builder's environment assignments (or all if no assignments)
/// 2. Check builder's current concurrency limit
/// 3. Return highest-priority queued job if available
pub async fn get_next_job(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::NextJobResponse>, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/next-job", builder_id);
    let verified =
        authenticate_builder_request(&headers, body.clone(), method.as_str(), &path, &state.pool)
            .await?;

    // Verify the builder_id matches
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Get builder to check max_concurrent_jobs
    let builder = builders::get_builder_by_id(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder.enabled {
        return Err(StatusCode::NOT_FOUND);
    }

    let next_job_request = next_job_request_for_method(&method, &body)?;
    let execution_strategy = state.server_config.remote_build_execution_strategy;
    if !next_job_request
        .supported_execution_strategies
        .contains(&execution_strategy)
    {
        tracing::warn!(
            builder_id = %builder_id,
            ?execution_strategy,
            supported = ?next_job_request.supported_execution_strategies,
            "builder does not support configured remote execution strategy; returning 409 Conflict"
        );
        return Err(StatusCode::CONFLICT);
    }

    // Get builder's environment assignments (empty = wildcard)
    let environment_ids = builders::get_builder_environment_ids(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // TASK-147: Atomically claim next job with race-free concurrency enforcement
    // This single transaction ensures count check + job assignment are atomic,
    // preventing multiple builders from exceeding their max_concurrent_jobs limit
    let job = builders::claim_next_job_atomic(
        &state.pool,
        &builder_id,
        builder.max_concurrent_jobs,
        &environment_ids,
        execution_strategy,
        verified.builder_session_id.as_ref(),
    )
    .await
    .map_err(|e| {
        if e.to_string().contains("builder_session_mismatch") {
            tracing::warn!(
                builder_id = %builder_id,
                error = %e,
                "rejected next-job claim from superseded builder session (410 Gone)"
            );
            StatusCode::GONE
        } else {
            tracing::error!("Failed to claim job atomically: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    let Some(job) = job else {
        // Either no jobs available OR builder at capacity.
        // Return 404 NOT_FOUND so builder knows to wait.
        return Err(StatusCode::NOT_FOUND);
    };

    // Embed the derivation build payload so the remote builder needs no DB access.
    let derivation =
        match crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
        {
            Ok(derivation) => derivation,
            Err(e) => {
                tracing::error!(
                    "Failed to load derivation {} for claimed job {}: {}",
                    job.derivation_id,
                    job.id,
                    e
                );
                requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
                    .await
                    .map_err(|err| {
                        tracing::error!(
                            job_id = %job.id,
                            "failed to requeue job after derivation load error: {err}"
                        );
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

    let mut source = match verified_source_identity_for_derivation(&state.pool, &derivation).await {
        Ok(source) => source,
        Err(e) => {
            tracing::error!(
                job_id = %job.id,
                derivation_id = derivation.id,
                "failed to assemble verified source identity; requeueing claimed job: {e}"
            );
            requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
                .await
                .map_err(|err| {
                    tracing::error!(
                        job_id = %job.id,
                        "failed to requeue job after source identity assembly error: {err}"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let expected_drv_path = derivation.derivation_path.clone();
    let source_input_delivery = match execution_strategy {
        RemoteBuildExecutionStrategy::ServerDerivation => SourceInputDeliveryMode::None,
        RemoteBuildExecutionStrategy::SourceReEvaluateVerified => {
            state.server_config.source_delivery_mode
        }
    };

    let mut source_archive_generated = false;

    // If ServerBundledArchive is selected, generate the source archive now.
    if source_input_delivery == SourceInputDeliveryMode::ServerBundledArchive {
        if let Some(ref mut source_mut) = source {
            let mirror_path = server_mirror_path(
                &state.server_config.source_archive_root,
                &source_mut.repo_url,
            );
            let mirror_id = source_mirror_id(&source_mut.repo_url);

            // Job-scoped archive path: one archive file per claimed job so
            // concurrent jobs for the same repo+commit don't interfere.
            let archive_path =
                job_scoped_archive_path(&state.server_config.source_archive_root, job.id);

            // Load per-flake credentials so the server-side mirror clone/fetch
            // can authenticate against private repositories.
            let flake_creds = if let Some(commit_id) = derivation.commit_id {
                match crate::queries::commits::get_commit_by_id(&state.pool, commit_id).await {
                    Ok(commit) => crate::flake::credentials::FlakeCredentialEnv::load(
                        &state.pool,
                        commit.flake_id,
                    )
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!(
                            job_id = %job.id,
                            flake_id = commit.flake_id,
                            "failed to load flake credentials for server mirror: {e}"
                        );
                        None
                    }),
                    Err(e) => {
                        tracing::warn!(
                            job_id = %job.id,
                            commit_id,
                            "failed to load commit for credential lookup: {e}"
                        );
                        None
                    }
                }
            } else {
                None
            };

            // Acquire the per-mirror lock before any git clone/fetch or archive
            // generation. This ensures concurrent jobs for the same repository
            // don't corrupt the shared bare mirror.
            let _mirror_guard = mirror_lock(&mirror_id).lock_owned().await;

            if ensure_server_mirror_has_commit(
                &mirror_path,
                &source_mut.repo_url,
                &source_mut.commit_hash,
                flake_creds.as_ref(),
            )
            .await
            .is_err()
            {
                requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
                    .await
                    .ok();
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }

            match generate_source_archive(&mirror_path, &archive_path).await {
                Ok(sha256) => {
                    source_archive_generated = true;
                    source_mut.archive_url = Some(format!(
                        "/api/v1/builders/{}/jobs/{}/source-archive",
                        builder_id, job.id
                    ));
                    source_mut.archive_sha256 = Some(sha256);
                }
                Err(status) => {
                    requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
                        .await
                        .ok();
                    return Err(status);
                }
            }
        } else {
            // Source is None but delivery is ServerBundledArchive — bail.
            tracing::error!(
                job_id = %job.id,
                derivation_id = derivation.id,
                "ServerBundledArchive selected but source identity is missing; requeueing"
            );
            requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
                .await
                .map_err(|e| {
                    tracing::error!(
                        job_id = %job.id,
                        "failed to requeue job after source identity assembly error: {e}"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    if execution_strategy == RemoteBuildExecutionStrategy::SourceReEvaluateVerified
        && (source.is_none() || expected_drv_path.is_none())
    {
        tracing::error!(
            job_id = %job.id,
            derivation_id = derivation.id,
            has_source = source.is_some(),
            has_expected_drv_path = expected_drv_path.is_some(),
            "claimed source-verified job is missing required manifest metadata; requeueing"
        );
        if source_archive_generated {
            cleanup_source_archive(
                &state.pool,
                &state.server_config.source_archive_root,
                job.id,
            )
            .await;
        }
        requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
            .await
            .map_err(|e| {
                tracing::error!(
                    job_id = %job.id,
                    "failed to requeue job after manifest assembly error: {e}"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let cache_push = match builder_cache_push_config_for_derivation(&state.pool, &derivation).await
    {
        Ok(cache_push) => Some(cache_push),
        Err(status) => {
            tracing::error!(
                job_id = %job.id,
                derivation_id = derivation.id,
                "failed to assemble builder cache-push config; requeueing claimed job"
            );
            if source_archive_generated {
                cleanup_source_archive(
                    &state.pool,
                    &state.server_config.source_archive_root,
                    job.id,
                )
                .await;
            }
            requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
                .await
                .map_err(|e| {
                    tracing::error!(
                        job_id = %job.id,
                        "failed to requeue job after cache-push config assembly error: {e}"
                    );
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
            return Err(status);
        }
    };

    if cache_push
        .as_ref()
        .is_some_and(cache_push_config_contains_credentials)
        && !builder_https_verified_by_trusted_proxy(&state.server_config, &headers)
    {
        tracing::error!(
            job_id = %job.id,
            derivation_id = derivation.id,
            builder_id = %builder_id,
            trust_forwarded = state.server_config.trust_forwarded_builder_https,
            "refusing to send cache push credentials: server.trust_forwarded_builder_https \
             is false or forwarded-proto header does not assert HTTPS"
        );
        if source_archive_generated {
            cleanup_source_archive(
                &state.pool,
                &state.server_config.source_archive_root,
                job.id,
            )
            .await;
        }
        requeue_claimed_job_after_manifest_error(&state.pool, &job.id, &builder_id)
            .await
            .map_err(|e| {
                tracing::error!(
                    job_id = %job.id,
                    "failed to requeue job after builder credential transport rejection: {e}"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        return Err(StatusCode::UPGRADE_REQUIRED);
    }

    let payload = crate::models::builders::BuildJobDerivation {
        id: derivation.id,
        derivation_name: derivation.derivation_name.clone(),
        derivation_type: match derivation.derivation_type {
            crate::derivations::DerivationType::NixOS => "nixos".to_string(),
            crate::derivations::DerivationType::Package => "package".to_string(),
        },
        derivation_path: derivation.derivation_path.clone(),
        store_path: derivation.store_path.clone(),
        execution_strategy,
        source,
        source_input_delivery,
        expected_drv_path,
        evaluator: Some(current_evaluator_fingerprint()),
        cache_push,
    };

    Ok(Json(crate::models::builders::NextJobResponse {
        job: job.into(),
        derivation: payload,
    }))
}

async fn requeue_claimed_job_after_manifest_error(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE build_jobs
        SET status = 'queued',
            builder_id = NULL,
            started_at = NULL,
            updated_at = NOW()
        WHERE id = $1
          AND builder_id = $2
          AND status = 'building'
        "#,
    )
    .bind(job_id)
    .bind(builder_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// POST /api/v1/builders/:id/jobs/:job_id/progress - Build progress heartbeat
///
/// HTTP fallback for the WebSocket progress frame. Updates the derivation's
/// build heartbeat/progress fields so the UI can show live build status.
pub async fn build_progress(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let path = format!("/api/v1/builders/{}/jobs/{}/progress", builder_id, job_id);
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let request: crate::models::builders::BuildProgressRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    crate::queries::derivations::update_build_heartbeat(
        &state.pool,
        request.derivation_id,
        request.elapsed_seconds,
        request.current_target.as_deref(),
        request.last_activity_seconds,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
pub struct JobStatusRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub failure_phase: Option<String>,
    #[serde(default)]
    pub failure_class: Option<crate::models::builders::BuildFailureClass>,
    pub error_message: Option<String>,
}

fn parse_job_status_request(body: &[u8]) -> Result<JobStatusRequest, serde_json::Error> {
    if body.is_empty() {
        return Ok(JobStatusRequest {
            status: None,
            failure_phase: None,
            failure_class: None,
            error_message: None,
        });
    }

    serde_json::from_slice(body)
}

fn fallback_job_status_request_for_invalid_details() -> JobStatusRequest {
    JobStatusRequest {
        status: None,
        failure_phase: Some("build".to_string()),
        failure_class: None,
        error_message: Some("builder reported failure with invalid failure details".to_string()),
    }
}

fn format_failure_message(request: &JobStatusRequest) -> Option<String> {
    let message = request.error_message.clone()?;
    match request.failure_phase.as_deref() {
        Some(phase) if !phase.trim().is_empty() => Some(format!("[{phase}] {message}")),
        _ => Some(message),
    }
}

fn retry_failure_class(
    request: &JobStatusRequest,
) -> crate::models::retry_policy::RetryFailureClass {
    use crate::models::builders::BuildFailureClass;
    use crate::models::retry_policy::RetryFailureClass;

    if request.failure_phase.as_deref() == Some("derivation_mismatch") {
        return RetryFailureClass::DerivationMismatch;
    }

    match request.failure_class {
        Some(BuildFailureClass::Transient) => RetryFailureClass::Transient,
        Some(BuildFailureClass::Deterministic) => RetryFailureClass::Deterministic,
        Some(BuildFailureClass::Authorization) => RetryFailureClass::Authorization,
        Some(BuildFailureClass::Cancelled) => RetryFailureClass::Cancelled,
        Some(BuildFailureClass::Unknown) | None => RetryFailureClass::Unknown,
    }
}

/// GET /api/v1/builders/:id/jobs/:job_id/derivation-archive
///
/// Streams a Nix archive for the claimed job's `.drv` closure. Remote API
/// builders import this before realizing server-evaluated derivations so they
/// do not require a shared Nix store with the server.
pub async fn download_job_derivation_archive(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let path = format!(
        "/api/v1/builders/{}/jobs/{}/derivation-archive",
        builder_id, job_id
    );
    let verified = authenticate_builder_request(&headers, body, "GET", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, "failed to load build job for derivation archive: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id)
        || job.status != "building"
    {
        return Err(StatusCode::FORBIDDEN);
    }

    let derivation = crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, derivation_id = job.derivation_id, "failed to load derivation for archive: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let Some(drv_path) = derivation.derivation_path.as_deref() else {
        return Err(StatusCode::NOT_FOUND);
    };

    if !drv_path.ends_with(".drv") {
        tracing::warn!(job_id = %job_id, drv_path, "refusing to export non-.drv path");
        return Err(StatusCode::BAD_REQUEST);
    }

    let validity_output = Command::new("nix-store")
        .arg("--check-validity")
        .arg(drv_path)
        .output()
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, drv_path, "failed to run nix-store --check-validity: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !validity_output.status.success() {
        let stderr = String::from_utf8_lossy(&validity_output.stderr);
        tracing::error!(job_id = %job_id, drv_path, stderr = %stderr, "derivation path is not valid in server store; evaluated drvs must be rooted before API builders can import them");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let requisites_output = Command::new("nix-store")
        .arg("--query")
        .arg("--requisites")
        .arg(drv_path)
        .output()
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, drv_path, "failed to run nix-store --query --requisites: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !requisites_output.status.success() {
        let stderr = String::from_utf8_lossy(&requisites_output.stderr);
        tracing::error!(job_id = %job_id, drv_path, stderr = %stderr, "nix-store --query --requisites failed");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let archive_paths = parse_derivation_requisites(&requisites_output.stdout, drv_path);
    tracing::debug!(
        job_id = %job_id,
        drv_path,
        path_count = archive_paths.len(),
        "exporting derivation requisites archive (full closure)"
    );

    stream_nix_export_response(archive_paths, job_id, drv_path.to_string())
}

/// Splices multiple `nix-store --export` streams into one valid stream.
///
/// The Nix export stream format is a sequence of `[1u64][path entry]` records
/// followed by a single `[0u64]` end-of-stream terminator. Concatenating the
/// raw stdout of several `nix-store --export` invocations therefore produces
/// an INVALID stream: `nix-store --import` stops at the first chunk's
/// terminator and silently discards everything after it, leaving the closure
/// incomplete on the builder.
///
/// This helper holds back the trailing 8 bytes of each chunk's stream. For
/// every chunk except the last, the held-back terminator is verified to be
/// the 8-byte zero marker and dropped; the final chunk keeps its terminator
/// so the spliced stream ends correctly.
struct ExportStreamSplicer {
    tail: Vec<u8>,
}

impl ExportStreamSplicer {
    const TERMINATOR_LEN: usize = 8;

    fn new() -> Self {
        Self { tail: Vec::new() }
    }

    /// Accept the next bytes of the current chunk's stdout. Returns the bytes
    /// that are safe to forward now — everything except the last 8 bytes seen
    /// so far (which may turn out to be the stream terminator).
    fn push(&mut self, incoming: &[u8]) -> Vec<u8> {
        let mut combined = std::mem::take(&mut self.tail);
        combined.extend_from_slice(incoming);
        if combined.len() <= Self::TERMINATOR_LEN {
            self.tail = combined;
            return Vec::new();
        }
        let split = combined.len() - Self::TERMINATOR_LEN;
        self.tail = combined.split_off(split);
        combined
    }

    /// Finish the current chunk. When `emit_terminator` is true (final chunk)
    /// the held-back bytes are returned for forwarding. Otherwise the
    /// held-back bytes MUST be the 8-byte zero terminator, which is dropped so
    /// the next chunk's records continue the stream seamlessly.
    fn finish(&mut self, emit_terminator: bool) -> Result<Option<Vec<u8>>, String> {
        let tail = std::mem::take(&mut self.tail);
        if emit_terminator {
            return Ok(if tail.is_empty() { None } else { Some(tail) });
        }
        if tail.len() != Self::TERMINATOR_LEN || tail.iter().any(|b| *b != 0) {
            return Err(format!(
                "unexpected nix-store --export stream tail ({} bytes, expected 8-byte zero terminator)",
                tail.len()
            ));
        }
        Ok(None)
    }
}

/// Build a streaming HTTP response of `nix-store --export <paths>`.
///
/// True process-stdout streaming: spawns each nix-store --export chunk with
/// Stdio::piped(), wraps stdout in a ReaderStream, and forwards bytes directly
/// into the HTTP response channel without ever materialising output.stdout as
/// a Vec<u8>. Per-chunk memory overhead is bounded by the HTTP buffer size.
/// stderr is drained concurrently with a bounded 64 KiB tail so a noisy child
/// cannot deadlock the pipe or OOM the server.
///
/// Multiple export chunks are spliced into a single valid import stream via
/// [`ExportStreamSplicer`] — each intermediate chunk's end-of-stream
/// terminator is stripped so `nix-store --import` on the builder consumes the
/// entire multi-chunk archive instead of stopping at the first terminator.
fn stream_nix_export_response(
    archive_paths: Vec<String>,
    job_id: Uuid,
    drv_path: String,
) -> Result<Response, StatusCode> {
    let archive_chunks: Vec<Vec<String>> =
        chunk_derivation_archive_paths(&archive_paths, NIX_STORE_EXPORT_ARG_BYTES_LIMIT)
            .into_iter()
            .map(|chunk| chunk.to_vec())
            .collect();

    let chunk_count = archive_chunks.len();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(4);
    let job_id_copy = job_id;
    let drv_path_owned = drv_path;
    let path_count = archive_paths.len();

    tokio::spawn(async move {
        for (chunk_index, archive_chunk) in archive_chunks.iter().enumerate() {
            // Spawn with Stdio::piped() so stdout is a stream, not a buffer.
            let mut child = match Command::new("nix-store")
                .arg("--export")
                .args(archive_chunk)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(
                        job_id = %job_id_copy,
                        drv_path = %drv_path_owned,
                        chunk_index,
                        chunk_count,
                        "failed to spawn nix-store --export: {e}"
                    );
                    let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                    return;
                }
            };

            // Drain stderr concurrently with stdout so that a child that writes
            // enough stderr doesn't fill the OS pipe buffer and block, which
            // would stall stdout and deadlock the whole stream.
            // Keep only the last 64 KiB so a noisy process cannot OOM the server.
            const STDERR_TAIL_BYTES: usize = 64 * 1024;
            let stderr_pipe = child.stderr.take();
            let stderr_task: tokio::task::JoinHandle<String> = tokio::spawn(async move {
                let mut buf: Vec<u8> = Vec::new();
                if let Some(mut pipe) = stderr_pipe {
                    use tokio::io::AsyncReadExt;
                    let mut tmp = [0u8; 8192];
                    while let Ok(n) = pipe.read(&mut tmp).await {
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if buf.len() > STDERR_TAIL_BYTES {
                            let drain = buf.len() - STDERR_TAIL_BYTES;
                            buf.drain(..drain);
                        }
                    }
                }
                String::from_utf8_lossy(&buf).into_owned()
            });

            // Stream stdout bytes into the response channel as they arrive.
            // The splicer holds back each chunk's trailing 8-byte terminator so
            // that intermediate chunks join into one valid import stream; only
            // the final chunk's terminator is forwarded.
            let is_last_chunk = chunk_index == chunk_count - 1;
            let mut splicer = ExportStreamSplicer::new();
            if let Some(stdout) = child.stdout.take() {
                let mut reader = ReaderStream::new(stdout);
                loop {
                    use futures::StreamExt;
                    match reader.next().await {
                        Some(Ok(chunk)) if !chunk.is_empty() => {
                            let forward = splicer.push(&chunk);
                            if !forward.is_empty()
                                && tx.send(Ok(bytes::Bytes::from(forward))).await.is_err()
                            {
                                tracing::debug!(
                                    job_id = %job_id_copy,
                                    "derivation archive stream cancelled by client"
                                );
                                let _ = child.kill().await;
                                return;
                            }
                        }
                        Some(Ok(_)) => {} // empty chunk, skip
                        Some(Err(e)) => {
                            tracing::error!(
                                job_id = %job_id_copy,
                                drv_path = %drv_path_owned,
                                chunk_index,
                                "error reading nix-store --export stdout: {e}"
                            );
                            let _ = tx.send(Err(e)).await;
                            return;
                        }
                        None => break, // stdout EOF
                    }
                }
            }

            // Chunk stdout complete: emit or verify-and-drop the held-back
            // stream terminator depending on whether this is the final chunk.
            match splicer.finish(is_last_chunk) {
                Ok(Some(tail)) => {
                    if tx.send(Ok(bytes::Bytes::from(tail))).await.is_err() {
                        tracing::debug!(
                            job_id = %job_id_copy,
                            "derivation archive stream cancelled by client at tail"
                        );
                        let _ = child.kill().await;
                        return;
                    }
                }
                Ok(None) => {}
                Err(msg) => {
                    tracing::error!(
                        job_id = %job_id_copy,
                        drv_path = %drv_path_owned,
                        chunk_index,
                        chunk_count,
                        "export stream splice failed: {msg}"
                    );
                    let _ = tx.send(Err(std::io::Error::other(msg))).await;
                    return;
                }
            }

            // Wait for exit status; stderr is already drained by the task above.
            let stderr = stderr_task.await.unwrap_or_default();
            let status = child.wait().await;
            match status {
                Ok(s) if s.success() => {}
                Ok(_) => {
                    tracing::error!(
                        job_id = %job_id_copy,
                        drv_path = %drv_path_owned,
                        path_count,
                        chunk_index,
                        chunk_count,
                        stderr = %stderr,
                        "nix-store --export chunk failed"
                    );
                    let _ = tx
                        .send(Err(std::io::Error::other(format!(
                            "nix-store --export failed: {stderr}"
                        ))))
                        .await;
                    return;
                }
                Err(e) => {
                    tracing::error!(
                        job_id = %job_id_copy,
                        "failed to wait for nix-store --export: {e}"
                    );
                    let _ = tx.send(Err(std::io::Error::other(e.to_string()))).await;
                    return;
                }
            }
        }
        tracing::debug!(
            job_id = %job_id_copy,
            drv_path = %drv_path_owned,
            chunk_count,
            "derivation archive streaming complete"
        );
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/x-nix-archive")
        .body(Body::from_stream(stream))
        .map_err(|e| {
            tracing::error!(job_id = %job_id, "failed to build derivation archive response: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// Resolve and authorize the job's persisted `.drv` path for archive/manifest
/// endpoints. Verifies the job belongs to the builder+session and is currently
/// `building`, then loads the derivation path from persisted state — never
/// from client input.
async fn authorized_job_drv_path(
    state: &CFState,
    builder_id: Uuid,
    job_id: Uuid,
    builder_session_id: Option<Uuid>,
) -> Result<String, StatusCode> {
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, "failed to load build job: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, builder_session_id) || job.status != "building" {
        return Err(StatusCode::FORBIDDEN);
    }

    let derivation =
        crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
            .map_err(|e| {
                tracing::error!(job_id = %job_id, derivation_id = job.derivation_id, "failed to load derivation: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let Some(drv_path) = derivation.derivation_path else {
        return Err(StatusCode::NOT_FOUND);
    };

    if !drv_path.ends_with(".drv") {
        tracing::warn!(job_id = %job_id, drv_path = %drv_path, "refusing to serve non-.drv path");
        return Err(StatusCode::BAD_REQUEST);
    }

    Ok(drv_path)
}

/// GET /api/v1/builders/:id/jobs/:job_id/derivation-manifest
///
/// Returns the authorized requisite path manifest for the claimed job's
/// server-evaluated `.drv`. The builder uses this to compute which paths it
/// is missing locally and then requests only those via the POST delta archive
/// endpoint. The manifest is computed from persisted job state — the builder
/// cannot influence which drv is used.
pub async fn get_job_derivation_manifest(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::DerivationManifestResponse>, StatusCode> {
    let path = format!(
        "/api/v1/builders/{}/jobs/{}/derivation-manifest",
        builder_id, job_id
    );
    let verified = authenticate_builder_request(&headers, body, "GET", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let drv_path =
        authorized_job_drv_path(&state, builder_id, job_id, verified.builder_session_id).await?;

    let paths = nix_store_requisites(&drv_path).await.map_err(|e| {
        tracing::error!(job_id = %job_id, drv_path = %drv_path, "manifest requisites failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::debug!(
        job_id = %job_id,
        drv_path = %drv_path,
        path_count = paths.len(),
        "serving derivation manifest"
    );

    Ok(Json(crate::models::builders::DerivationManifestResponse {
        job_id,
        drv_path,
        paths,
    }))
}

/// POST /api/v1/builders/:id/jobs/:job_id/derivation-archive
///
/// Delta archive: the builder posts the subset of the authorized manifest it
/// is missing locally, and the server streams `nix-store --export` for exactly
/// those paths. Every requested path is validated against the server-computed
/// manifest — a request for any path outside the authorized set is rejected
/// with 403 and nothing is exported.
pub async fn download_job_derivation_archive_delta(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let path = format!(
        "/api/v1/builders/{}/jobs/{}/derivation-archive",
        builder_id, job_id
    );
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let request: crate::models::builders::DerivationArchiveRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    let drv_path =
        authorized_job_drv_path(&state, builder_id, job_id, verified.builder_session_id).await?;

    // Empty request: nothing to export.
    if request.paths.is_empty() {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .map_err(|e| {
                tracing::error!(job_id = %job_id, "failed to build empty delta response: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            });
    }

    // Compute the authorized manifest server-side and validate the requested
    // subset. Unauthorized paths are a hard 403 — never export arbitrary paths.
    let authorized_manifest = nix_store_requisites(&drv_path).await.map_err(|e| {
        tracing::error!(job_id = %job_id, drv_path = %drv_path, "delta manifest requisites failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let validated =
        validate_requested_paths(&authorized_manifest, &request.paths).map_err(|status| {
            if status == StatusCode::FORBIDDEN {
                tracing::warn!(
                    builder_id = %builder_id,
                    job_id = %job_id,
                    requested_count = request.paths.len(),
                    "builder requested store path outside authorized manifest"
                );
            }
            status
        })?;

    tracing::debug!(
        job_id = %job_id,
        drv_path = %drv_path,
        requested_count = request.paths.len(),
        validated_count = validated.len(),
        manifest_count = authorized_manifest.len(),
        "exporting delta derivation archive"
    );

    stream_nix_export_response(validated, job_id, drv_path)
}

/// GET /api/v1/builders/:id/jobs/:job_id/source-archive
///
/// Streams a gzipped tar archive of the bare Git mirror for the job's source
/// repository, containing the authorized commit. Remote API builders in
/// ServerBundledArchive mode download this archive instead of cloning the repo
/// directly.
pub async fn download_job_source_archive(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let path = format!(
        "/api/v1/builders/{}/jobs/{}/source-archive",
        builder_id, job_id
    );
    let verified = authenticate_builder_request(&headers, body, "GET", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, "failed to load build job for source archive: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id)
        || job.status != "building"
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Job-scoped archive path: one archive per claimed job, so this builder
    // gets exactly the archive generated for its job and not one shared with
    // (and potentially deleted by) another concurrent job.
    let archive_path = job_scoped_archive_path(&state.server_config.source_archive_root, job_id);

    // Stream the archive file rather than reading it fully into RAM.
    let file = tokio::fs::File::open(&archive_path).await.map_err(|e| {
        tracing::error!(
            job_id = %job_id,
            archive_path = %archive_path.display(),
            "failed to open source archive for streaming: {e}"
        );
        StatusCode::NOT_FOUND
    })?;
    let file_size = file.metadata().await.ok().map(|m| m.len());
    let stream = ReaderStream::new(file);

    let mut resp_builder = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/gzip")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}.tar.gz\"", job_id),
        );
    if let Some(size) = file_size {
        resp_builder = resp_builder.header("Content-Length", size.to_string());
    }
    resp_builder.body(Body::from_stream(stream)).map_err(|e| {
        tracing::error!(job_id = %job_id, "failed to build source archive response: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// POST /api/v1/builders/:id/jobs/:job_id/publish-derivation-closure
///
/// Publishes the evaluated derivation requisite closure to the configured Attic
/// cache so API builders can fetch it through normal Nix substituters instead
/// of downloading a large archive through the Crystal Forge server.
pub async fn publish_job_derivation_closure(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let path = format!(
        "/api/v1/builders/{}/jobs/{}/publish-derivation-closure",
        builder_id, job_id
    );
    let verified = authenticate_builder_request(&headers, body, "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, "failed to load build job for derivation closure publish: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id)
        || job.status != "building"
    {
        return Err(StatusCode::FORBIDDEN);
    }

    let derivation = crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, derivation_id = job.derivation_id, "failed to load derivation for closure publish: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let Some(drv_path) = derivation.derivation_path.as_deref() else {
        return Err(StatusCode::NOT_FOUND);
    };

    if !drv_path.ends_with(".drv") {
        tracing::warn!(job_id = %job_id, drv_path, "refusing to publish non-.drv path");
        return Err(StatusCode::BAD_REQUEST);
    }

    let validity_output = Command::new("nix-store")
        .arg("--check-validity")
        .arg(drv_path)
        .output()
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, drv_path, "failed to run nix-store --check-validity before closure publish: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !validity_output.status.success() {
        let stderr = String::from_utf8_lossy(&validity_output.stderr);
        tracing::error!(job_id = %job_id, drv_path, stderr = %stderr, "derivation path is not valid in server store; cannot publish closure");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let requisites_output = Command::new("nix-store")
        .arg("--query")
        .arg("--requisites")
        .arg(drv_path)
        .output()
        .await
        .map_err(|e| {
            tracing::error!(job_id = %job_id, drv_path, "failed to run nix-store --query --requisites before closure publish: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !requisites_output.status.success() {
        let stderr = String::from_utf8_lossy(&requisites_output.stderr);
        tracing::error!(job_id = %job_id, drv_path, stderr = %stderr, "nix-store --query --requisites failed before closure publish");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let archive_paths = parse_derivation_requisites(&requisites_output.stdout, drv_path);
    tracing::info!(
        job_id = %job_id,
        drv_path,
        path_count = archive_paths.len(),
        "publishing derivation requisite closure to cache"
    );

    match push_derivation_requisites_to_assigned_cache(&state.pool, &derivation, &archive_paths)
        .await?
    {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /api/v1/builders/:id/jobs/:job_id/start - Mark job as started
///
/// Note: This is a no-op since get_next_job already marks the job as building.
/// Kept for API consistency and future extensibility.
pub async fn start_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/start", builder_id, job_id);
    let verified = authenticate_builder_request(&headers, body, "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Verify the job exists and is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Job already marked as building by get_next_job
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
pub struct CompleteJobRequest {
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub cache_pushed: bool,
    #[serde(default)]
    pub cache_reference: Option<String>,
}

fn cache_reference_matches_destination(
    reported: &str,
    destination: &crate::models::cache_destination::CacheDestination,
) -> bool {
    let reported = reported.trim();
    if reported.is_empty() {
        return false;
    }

    destination.name == reported
        || destination.push_to.as_deref() == Some(reported)
        || destination.attic_cache_name.as_deref() == Some(reported)
}

async fn validated_reported_cache_destination(
    state: &CFState,
    request: &CompleteJobRequest,
    builder_id: Uuid,
    job_id: Uuid,
) -> Result<Option<crate::models::cache_destination::CacheDestination>, StatusCode> {
    if !request.cache_pushed {
        return Ok(None);
    }

    let Some(reported) = request
        .cache_reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        tracing::warn!(
            builder_id = %builder_id,
            job_id = %job_id,
            "builder reported cache_pushed without cache_reference"
        );
        return Err(StatusCode::CONFLICT);
    };

    let destinations =
        crate::queries::cache_destinations::list_cache_destinations(&state.pool, true)
            .await
            .map_err(|e| {
                tracing::warn!(
                    builder_id = %builder_id,
                    job_id = %job_id,
                    error = %e,
                    "failed to load cache destinations while validating builder cache push"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let Some(destination) = destinations
        .iter()
        .find(|destination| cache_reference_matches_destination(reported, destination))
    else {
        tracing::warn!(
            builder_id = %builder_id,
            job_id = %job_id,
            reported_cache = reported,
            "builder reported cache push to a cache that does not match any active server cache destination"
        );
        return Err(StatusCode::CONFLICT);
    };

    Ok(Some(destination.clone()))
}

async fn verify_store_path_available_from_cache(
    destination: &crate::models::cache_destination::CacheDestination,
    store_path: &str,
    derivation_id: i32,
    job_id: Uuid,
) -> Result<(), StatusCode> {
    let Some(cache_url) = destination
        .push_to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        tracing::warn!(
            derivation_id,
            job_id = %job_id,
            cache_destination = %destination.name,
            "cannot verify builder cache push because destination has no push_to/substituter URL"
        );
        return Err(StatusCode::CONFLICT);
    };

    let mut command = Command::new("nix");
    command.args(["path-info", "--store", cache_url, store_path]);

    if let Some(public_key) = destination
        .attic_public_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        command.env(
            "NIX_CONFIG",
            format!("extra-substituters = {cache_url}\nextra-trusted-public-keys = {public_key}\n"),
        );
    }

    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await
        .map_err(|_| {
            tracing::warn!(
                derivation_id,
                job_id = %job_id,
                cache_destination = %destination.name,
                cache_url,
                store_path,
                "timed out verifying builder cache push"
            );
            StatusCode::CONFLICT
        })?
        .map_err(|e| {
            tracing::warn!(
                derivation_id,
                job_id = %job_id,
                cache_destination = %destination.name,
                cache_url,
                store_path,
                error = %e,
                "failed to run cache availability probe"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !output.status.success() {
        tracing::warn!(
            derivation_id,
            job_id = %job_id,
            cache_destination = %destination.name,
            cache_url,
            store_path,
            stderr = %String::from_utf8_lossy(&output.stderr).trim(),
            "builder reported cache_pushed, but server cache probe could not find store path"
        );
        return Err(StatusCode::CONFLICT);
    }

    Ok(())
}

async fn record_builder_confirmed_cache_push(
    state: &CFState,
    derivation_id: i32,
    job_id: Uuid,
    cache_destination: &crate::models::cache_destination::CacheDestination,
) -> Result<(), StatusCode> {
    let derivation = crate::queries::derivations::get_derivation_by_id(&state.pool, derivation_id)
        .await
        .map_err(|e| {
            tracing::warn!(
                derivation_id,
                job_id = %job_id,
                error = %e,
                "failed to load derivation while recording builder-confirmed cache push"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let Some(store_path) = derivation
        .store_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        tracing::warn!(
            derivation_id,
            job_id = %job_id,
            "builder reported cache_pushed but derivation has no persisted store_path"
        );
        return Err(StatusCode::CONFLICT);
    };

    verify_store_path_available_from_cache(cache_destination, store_path, derivation_id, job_id)
        .await?;

    let cache_job_id = crate::queries::cache_push::create_cache_push_job(
        &state.pool,
        derivation_id,
        store_path,
        Some(cache_destination.name.as_str()),
    )
    .await
    .map_err(|e| {
        tracing::warn!(
            derivation_id,
            job_id = %job_id,
            error = %e,
            "failed to create/reuse cache push job for builder-confirmed cache push"
        );
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    crate::queries::cache_push::mark_cache_push_completed(&state.pool, cache_job_id, None, None)
        .await
        .map_err(|e| {
            tracing::warn!(
                derivation_id,
                job_id = %job_id,
                cache_job_id,
                error = %e,
                "failed to mark builder-confirmed cache push completed"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(())
}

/// POST /api/v1/builders/:id/jobs/:job_id/complete - Mark job as complete
///
/// In addition to closing the build job, the server performs the derivation
/// completion (store path + status) and queues a cache-push job. This keeps all
/// database writes server-side so API builders never need a DB connection.
pub async fn complete_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/complete", builder_id, job_id);
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Output path is optional for backwards compatibility but expected from API builders.
    let request: CompleteJobRequest = if body.is_empty() {
        CompleteJobRequest {
            output_path: None,
            cache_pushed: false,
            cache_reference: None,
        }
    } else {
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?
    };

    let reported_cache_destination =
        validated_reported_cache_destination(&state, &request, builder_id, job_id).await?;

    // Perform atomic completion (job + derivation update in one transaction).
    // Idempotent: if the job is already 'success' with matching builder+session,
    // this is a safe no-op. The returned bool indicates whether this was a new
    // completion (true) or an idempotent retry (false).
    let (completed_job, is_new) = builders::complete_job_atomic(
        &state.pool,
        &job_id,
        &builder_id,
        verified.builder_session_id.as_ref(),
        request.output_path.as_deref(),
    )
    .await
    .map_err(|err| {
        tracing::warn!(
            builder_id = %builder_id,
            job_id = %job_id,
            error = %err,
            "Rejected complete transition due to lease/state mismatch"
        );
        StatusCode::CONFLICT
    })?;

    if request.cache_pushed {
        let Some(cache_destination) = reported_cache_destination.as_ref() else {
            tracing::warn!(
                builder_id = %builder_id,
                job_id = %job_id,
                "builder reported cache_pushed but no validated cache destination is available"
            );
            return Err(StatusCode::CONFLICT);
        };
        record_builder_confirmed_cache_push(
            &state,
            completed_job.derivation_id,
            job_id,
            cache_destination,
        )
        .await?;
    } else if is_new {
        // Old builder compatibility: builders that do not report builder-side cache
        // push still create a pending server-side cache-push row on first completion.
        if let Some(ref store_path) = request.output_path {
            let cache_destination =
                crate::queries::cache_destinations::list_cache_destinations(&state.pool, true)
                    .await
                    .ok()
                    .and_then(|dests| dests.into_iter().next().map(|d| d.name));

            if let Err(e) = crate::queries::cache_push::create_cache_push_job(
                &state.pool,
                completed_job.derivation_id,
                store_path,
                cache_destination.as_deref(),
            )
            .await
            {
                tracing::warn!(
                    "Failed to queue cache push for derivation {} (job {}): {}",
                    completed_job.derivation_id,
                    job_id,
                    e
                );
            }
        }
    }

    cleanup_build_log_channel(&state, job_id).await;

    // Best-effort source archive cleanup (for ServerBundledArchive jobs).
    if state.server_config.source_delivery_mode == SourceInputDeliveryMode::ServerBundledArchive {
        cleanup_source_archive(
            &state.pool,
            &state.server_config.source_archive_root,
            job_id,
        )
        .await;
    }

    Ok(StatusCode::OK)
}

/// POST /api/v1/builders/:id/jobs/:job_id/fail - Mark job as failed
///
/// Terminally records this attempt and may schedule a policy-eligible child attempt.
pub async fn fail_job(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/fail", builder_id, job_id);
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;

    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Parse failure details. Once the builder request is authenticated, a bad
    // details payload must not keep a known-failed build stuck in `building`.
    let request = parse_job_status_request(&body).unwrap_or_else(|e| {
        tracing::warn!(
            builder_id = %builder_id,
            job_id = %job_id,
            error = %e,
            "builder fail request contained invalid JSON body; failing job with fallback message"
        );
        fallback_job_status_request_for_invalid_details()
    });

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    let failure_message = format_failure_message(&request);

    // Mark job as failed with retry logic
    let updated_job = builders::mark_job_failed_with_retry(
        &state.pool,
        &job_id,
        &builder_id,
        verified.builder_session_id.as_ref(),
        failure_message.as_deref(),
        retry_failure_class(&request),
    )
    .await
    .map_err(|err| {
        tracing::warn!(
            builder_id = %builder_id,
            job_id = %job_id,
            error = %err,
            "Rejected fail transition due to lease/state mismatch"
        );
        StatusCode::CONFLICT
    })?;

    cleanup_build_log_channel(&state, job_id).await;

    // Return 200 when a child was scheduled, 202 when no retry is eligible.
    if updated_job.retry_job.is_some() {
        if state.server_config.source_delivery_mode == SourceInputDeliveryMode::ServerBundledArchive
        {
            cleanup_source_archive(
                &state.pool,
                &state.server_config.source_archive_root,
                job_id,
            )
            .await;
        }
        Ok(StatusCode::OK) // Job re-queued for retry
    } else {
        // No retry was scheduled: record the derivation-level failure server-side so
        // API builders never touch the database directly.
        match crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
        {
            Ok(derivation) => {
                let err = anyhow::anyhow!(
                    failure_message
                        .clone()
                        .unwrap_or_else(|| "build failed".to_string())
                );
                if let Err(e) = crate::queries::derivations::handle_derivation_failure(
                    &state.pool,
                    &derivation,
                    "build",
                    &err,
                )
                .await
                {
                    tracing::error!(
                        "Failed to record derivation {} failure for job {}: {}",
                        job.derivation_id,
                        job_id,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load derivation {} to record failure for job {}: {}",
                    job.derivation_id,
                    job_id,
                    e
                );
            }
        }

        // Best-effort source archive cleanup (for ServerBundledArchive jobs).
        if state.server_config.source_delivery_mode == SourceInputDeliveryMode::ServerBundledArchive
        {
            cleanup_source_archive(
                &state.pool,
                &state.server_config.source_archive_root,
                job_id,
            )
            .await;
        }

        Ok(StatusCode::ACCEPTED) // Job permanently failed
    }
}

/// POST /api/v1/builders/:id/jobs/:job_id/logs - Append build logs
pub async fn append_job_logs(
    State(state): State<CFState>,
    Path((builder_id, job_id)): Path<(Uuid, Uuid)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    let max_chunk_bytes = state.server_config.max_build_log_chunk_mb * 1024 * 1024;
    let max_total_bytes = state.server_config.max_build_log_size_mb * 1024 * 1024;

    // Enforce per-request payload size limit before parsing JSON.
    if body.len() > max_chunk_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Log payload too large: {} bytes exceeds {} byte limit",
                body.len(),
                max_chunk_bytes
            ),
        ));
    }

    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/jobs/{}/logs", builder_id, job_id);
    let verified = authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool)
        .await
        .map_err(|status| {
            (
                status,
                "Builder authentication failed for log append request".to_string(),
            )
        })?;

    if verified.builder_id != builder_id {
        return Err((
            StatusCode::FORBIDDEN,
            "Builder ID mismatch in log append request".to_string(),
        ));
    }

    // Parse log content
    let request: AppendLogsRequest = serde_json::from_slice(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid log append payload: expected JSON with 'logs' string field".to_string(),
        )
    })?;

    if request.logs.len() > max_chunk_bytes {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "Log chunk too large: {} bytes exceeds {} byte limit",
                request.logs.len(),
                max_chunk_bytes
            ),
        ));
    }

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load build job for log append".to_string(),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Build job not found for log append".to_string(),
        ))?;

    if !builder_owns_job_session(&job, builder_id, verified.builder_session_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Builder cannot append logs for a job assigned to another builder".to_string(),
        ));
    }

    // Only active or cancelling jobs may receive log appends.  Final messages
    // emitted while the builder is shutting down are accepted in `cancelling`,
    // but terminal statuses remain closed.
    if !build_log_append_status_allowed(&job.status) {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot append logs for terminal job in '{}' status; only 'queued', 'building', and 'cancelling' are allowed",
                job.status
            ),
        ));
    }

    // Append logs with per-job size cap enforcement.
    builders::append_job_logs_with_limits_for_builder(
        &state.pool,
        &job_id,
        &builder_id,
        verified.builder_session_id.as_ref(),
        &request.logs,
        max_total_bytes,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("log_size_limit_exceeded") {
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("Total job logs would exceed {} byte limit", max_total_bytes),
            )
        } else if msg.contains("invalid_job_status") {
            (
                StatusCode::CONFLICT,
                "Cannot append logs for job in current status".to_string(),
            )
        } else if msg.contains("job_not_found") {
            (
                StatusCode::NOT_FOUND,
                "Build job not found for log append".to_string(),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to append logs due to internal server error".to_string(),
            )
        }
    })?;

    if let Some(tx) = get_or_create_build_log_channel(&state, job_id).await {
        let log_msg = BuildStreamMessage::Log {
            message: request.logs.clone(),
        };
        record_build_stream_message(&state, job_id, &log_msg).await;
        let _ = broadcast_build_stream_message(&tx, &log_msg);
    }

    Ok((
        StatusCode::ACCEPTED,
        format!(
            "Log chunk accepted ({} bytes). Max per-chunk: {} bytes, max total per job: {} bytes",
            request.logs.len(),
            max_chunk_bytes,
            max_total_bytes
        ),
    ))
}

// =============================================================================
// WEBSOCKET LOG STREAMING
// =============================================================================

const BUILD_LOG_WS_CHANNEL_BUFFER: usize = 1024;
const MAX_BUILD_LOG_WS_CHANNELS: usize = 2048;
const BUILD_LOG_HISTORY_BUFFER: usize = 4000;
const PERSISTED_BUILD_LOG_REPLAY_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BuildStreamMessage {
    Log {
        message: String,
    },
    Metrics {
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
        timestamp: String,
    },
    /// Builder -> server: live build progress (replaces DB heartbeat polling).
    Progress {
        derivation_id: i32,
        elapsed_seconds: i32,
        current_target: Option<String>,
        last_activity_seconds: i32,
    },
    /// Server -> builder: the operator requested cancellation; stop the build.
    CancelRequested,
    Error {
        message: String,
    },
}

enum BuildLogStreamPrincipal {
    Viewer,
    Builder {
        builder_id: Uuid,
        builder_session_id: Option<Uuid>,
    },
}

/// WebSocket endpoint for real-time build log streaming
/// GET /api/v1/build-jobs/:job_id/logs/stream
///
/// This endpoint allows clients (UI or builders) to stream logs in real-time.
/// Builders send log lines, UI clients receive them.
///
/// Message Format:
/// - Text messages from builder -> stored as logs in database
/// - Text messages to clients -> broadcast log lines
/// - JSON messages -> system metrics (CPU/RAM usage)
pub async fn stream_build_logs(
    ws: WebSocketUpgrade,
    Path(job_id): Path<Uuid>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let principal = match authorize_build_log_stream(&state, &headers, job_id).await {
        Ok(principal) => principal,
        Err(status) => return status.into_response(),
    };

    ws.on_upgrade(move |socket| handle_log_stream(socket, job_id, state, principal))
}

async fn authorize_build_log_stream(
    state: &CFState,
    headers: &HeaderMap,
    job_id: Uuid,
) -> Result<BuildLogStreamPrincipal, StatusCode> {
    if require_viewer_or_above(&state.pool, headers)
        .await
        .is_some()
    {
        return Ok(BuildLogStreamPrincipal::Viewer);
    }

    let path = format!("/api/v1/build-jobs/{}/logs/stream", job_id);
    let verified = authenticate_builder_request(headers, Bytes::new(), "GET", &path, &state.pool)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !builder_owns_job_session(&job, verified.builder_id, verified.builder_session_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(BuildLogStreamPrincipal::Builder {
        builder_id: verified.builder_id,
        builder_session_id: verified.builder_session_id,
    })
}

async fn handle_log_stream(
    mut socket: WebSocket,
    job_id: Uuid,
    state: CFState,
    principal: BuildLogStreamPrincipal,
) {
    tracing::info!("WebSocket connection established for job {}", job_id);

    let Some(tx) = get_or_create_build_log_channel(&state, job_id).await else {
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1013,
                reason: "Server overloaded".into(),
            })))
            .await;
        return;
    };

    match principal {
        BuildLogStreamPrincipal::Viewer => {
            let mut rx = tx.subscribe();
            if !replay_initial_build_log_history(&mut socket, &state, job_id, true).await {
                return;
            }

            while let Ok(frame) = rx.recv().await {
                if let Err(e) = socket.send(Message::Text(frame)).await {
                    tracing::debug!(
                        "Viewer build-log websocket closed for job {}: {}",
                        job_id,
                        e
                    );
                    break;
                }
            }
        }
        BuildLogStreamPrincipal::Builder {
            builder_id,
            builder_session_id,
        } => {
            if !replay_initial_build_log_history(&mut socket, &state, job_id, false).await {
                return;
            }

            let max_chunk_bytes = state.server_config.max_build_log_chunk_mb * 1024 * 1024;
            let max_total_bytes = state.server_config.max_build_log_size_mb * 1024 * 1024;

            while let Some(msg) = socket.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if text.len() > max_chunk_bytes {
                            let error = BuildStreamMessage::Error {
                                message: format!(
                                    "stream frame too large: {} bytes exceeds {}",
                                    text.len(),
                                    max_chunk_bytes
                                ),
                            };
                            let _ = send_build_stream_message(&mut socket, &error).await;
                            break;
                        }

                        let parsed = match serde_json::from_str::<BuildStreamMessage>(&text) {
                            Ok(message) => message,
                            Err(_) => {
                                let error = BuildStreamMessage::Error {
                                    message:
                                        "invalid websocket payload; expected typed JSON message"
                                            .to_string(),
                                };
                                let _ = send_build_stream_message(&mut socket, &error).await;
                                break;
                            }
                        };

                        match parsed {
                            BuildStreamMessage::Log { message } => {
                                if let Err(e) = builders::append_job_logs_with_limits_for_builder(
                                    &state.pool,
                                    &job_id,
                                    &builder_id,
                                    builder_session_id.as_ref(),
                                    &message,
                                    max_total_bytes,
                                )
                                .await
                                {
                                    tracing::error!(
                                        "Failed to append log over WS for job {} from builder {}: {}",
                                        job_id,
                                        builder_id,
                                        e
                                    );
                                    let error = BuildStreamMessage::Error {
                                        message: "failed to persist log frame".to_string(),
                                    };
                                    let _ = send_build_stream_message(&mut socket, &error).await;
                                    break;
                                }

                                let log_msg = BuildStreamMessage::Log { message };
                                record_build_stream_message(&state, job_id, &log_msg).await;
                                let _ = broadcast_build_stream_message(&tx, &log_msg);
                            }
                            BuildStreamMessage::Metrics {
                                cpu_percent,
                                ram_used_mb,
                                ram_total_mb,
                                timestamp,
                            } => {
                                let metrics_msg = BuildStreamMessage::Metrics {
                                    cpu_percent,
                                    ram_used_mb,
                                    ram_total_mb,
                                    timestamp,
                                };
                                record_build_stream_message(&state, job_id, &metrics_msg).await;
                                let _ = broadcast_build_stream_message(&tx, &metrics_msg);
                            }
                            BuildStreamMessage::Progress {
                                derivation_id,
                                elapsed_seconds,
                                current_target,
                                last_activity_seconds,
                            } => {
                                if let Err(e) = crate::queries::derivations::update_build_heartbeat(
                                    &state.pool,
                                    derivation_id,
                                    elapsed_seconds,
                                    current_target.as_deref(),
                                    last_activity_seconds,
                                )
                                .await
                                {
                                    tracing::warn!(
                                        "Failed to persist WS build progress for job {} (builder {}): {}",
                                        job_id,
                                        builder_id,
                                        e
                                    );
                                }
                            }
                            BuildStreamMessage::CancelRequested => {
                                let error = BuildStreamMessage::Error {
                                    message: "builders cannot send cancel frames".to_string(),
                                };
                                let _ = send_build_stream_message(&mut socket, &error).await;
                                break;
                            }
                            BuildStreamMessage::Error { .. } => {
                                let error = BuildStreamMessage::Error {
                                    message: "clients cannot send error frames".to_string(),
                                };
                                let _ = send_build_stream_message(&mut socket, &error).await;
                                break;
                            }
                        }
                    }
                    Ok(Message::Ping(data)) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(e) => {
                        tracing::debug!(
                            "Builder build-log websocket error for job {} (builder {}): {}",
                            job_id,
                            builder_id,
                            e
                        );
                        break;
                    }
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed for job {}", job_id);
}

async fn replay_initial_build_log_history(
    socket: &mut WebSocket,
    state: &CFState,
    job_id: Uuid,
    include_persisted_logs: bool,
) -> bool {
    for frame in initial_build_log_history_snapshot(state, job_id, include_persisted_logs).await {
        if let Err(e) = socket.send(Message::Text(frame.into())).await {
            tracing::debug!(
                "Failed to replay build log history to websocket for job {}: {}",
                job_id,
                e
            );
            return false;
        }
    }

    true
}

async fn initial_build_log_history_snapshot(
    state: &CFState,
    job_id: Uuid,
    include_persisted_logs: bool,
) -> Vec<String> {
    let in_memory_snapshot = {
        let history = state.build_log_history.lock().await;
        history.get(&job_id).cloned().unwrap_or_default()
    };

    if !in_memory_snapshot.is_empty() {
        return in_memory_snapshot;
    }

    if !include_persisted_logs {
        return Vec::new();
    }

    match builders::get_build_job_by_id(&state.pool, &job_id).await {
        Ok(Some(job)) => persisted_build_log_frames(job.logs.as_deref()),
        Ok(None) => Vec::new(),
        Err(e) => {
            tracing::warn!(
                "Failed to load persisted build logs for websocket replay on job {}: {}",
                job_id,
                e
            );
            Vec::new()
        }
    }
}

fn persisted_build_log_frames(logs: Option<&str>) -> Vec<String> {
    let Some(logs) = logs.filter(|logs| !logs.is_empty()) else {
        return Vec::new();
    };

    split_utf8_chunks(logs, PERSISTED_BUILD_LOG_REPLAY_CHUNK_BYTES)
        .filter_map(|chunk| {
            serde_json::to_string(&BuildStreamMessage::Log {
                message: chunk.to_string(),
            })
            .ok()
        })
        .collect()
}

fn split_utf8_chunks(input: &str, max_chunk_bytes: usize) -> impl Iterator<Item = &str> {
    let max_chunk_bytes = max_chunk_bytes.max(1);
    let mut start = 0;

    std::iter::from_fn(move || {
        if start >= input.len() {
            return None;
        }

        let mut end = (start + max_chunk_bytes).min(input.len());
        while end > start && !input.is_char_boundary(end) {
            end -= 1;
        }

        if end == start {
            end = input[start..]
                .char_indices()
                .nth(1)
                .map(|(idx, _)| start + idx)
                .unwrap_or(input.len());
        }

        let chunk = &input[start..end];
        start = end;
        Some(chunk)
    })
}

fn broadcast_build_stream_message(
    tx: &tokio::sync::broadcast::Sender<String>,
    msg: &BuildStreamMessage,
) -> Result<(), serde_json::Error> {
    let json = serde_json::to_string(msg)?;
    let _ = tx.send(json);
    Ok(())
}

async fn send_build_stream_message(
    socket: &mut WebSocket,
    msg: &BuildStreamMessage,
) -> Result<(), ()> {
    let json = match serde_json::to_string(msg) {
        Ok(json) => json,
        Err(_) => return Err(()),
    };
    socket.send(Message::Text(json)).await.map_err(|_| ())
}

async fn get_or_create_build_log_channel(
    state: &CFState,
    job_id: Uuid,
) -> Option<tokio::sync::broadcast::Sender<String>> {
    let mut channels = state.build_log_channels.lock().await;
    if let Some(tx) = channels.get(&job_id) {
        return Some(tx.clone());
    }

    if channels.len() >= MAX_BUILD_LOG_WS_CHANNELS {
        return None;
    }

    let (tx, _rx) = tokio::sync::broadcast::channel(BUILD_LOG_WS_CHANNEL_BUFFER);
    channels.insert(job_id, tx.clone());
    Some(tx)
}

async fn cleanup_build_log_channel(state: &CFState, job_id: Uuid) {
    let mut channels = state.build_log_channels.lock().await;
    channels.remove(&job_id);
    drop(channels);

    let mut history = state.build_log_history.lock().await;
    history.remove(&job_id);
}

async fn record_build_stream_message(state: &CFState, job_id: Uuid, msg: &BuildStreamMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        let mut history = state.build_log_history.lock().await;
        let entry = history.entry(job_id).or_default();
        entry.push(json);
        if entry.len() > BUILD_LOG_HISTORY_BUFFER {
            let overflow = entry.len() - BUILD_LOG_HISTORY_BUFFER;
            entry.drain(0..overflow);
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
    use base64::engine::{Engine, general_purpose};
    use chrono::{Duration, Utc};
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use uuid::Uuid;

    use super::BuildStreamMessage;
    use super::builder_https_verified_by_trusted_proxy;
    use super::builder_id_for_resolved_builder;
    use super::canonical_signature_payload;
    use super::chunk_derivation_archive_paths;
    use super::fallback_job_status_request_for_invalid_details;
    use super::format_failure_message;
    use super::map_create_builder_error;
    use super::next_job_request_for_method;
    use super::parse_derivation_requisites;
    use super::parse_job_status_request;
    use super::parse_next_job_request;
    use super::persisted_build_log_frames;
    use super::retry_failure_class;
    use super::source_flake_target_for_derivation;
    use super::verify_builder_resolve_request;
    use crate::builder::api_client::BuilderApiClient;
    use crate::derivations::{Derivation, DerivationType};
    use crate::models::builders::{
        Builder, BuilderStatus, NextJobRequest, RemoteBuildExecutionStrategy,
        ResolveBuilderIdRequest,
    };
    use crate::models::public_key::PublicKey;

    fn signed_resolve_request(
        signing_key: &SigningKey,
        timestamp: String,
    ) -> (HeaderMap, Vec<u8>, String) {
        let public_key_base64 =
            general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());
        let body = serde_json::to_vec(&ResolveBuilderIdRequest {
            public_key: public_key_base64.clone(),
            session_id: Some(Uuid::new_v4()),
        })
        .expect("resolve request should serialize");
        let payload =
            canonical_signature_payload("POST", "/api/v1/builders/resolve-id", &timestamp, &body);
        let signature = signing_key.sign(&payload);

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Timestamp",
            HeaderValue::from_str(&timestamp).expect("valid timestamp header"),
        );
        headers.insert(
            "X-Signature",
            HeaderValue::from_str(&general_purpose::STANDARD.encode(signature.to_bytes()))
                .expect("valid signature header"),
        );

        (headers, body, public_key_base64)
    }

    fn test_builder(public_key_base64: &str, enabled: bool) -> Builder {
        let now = Utc::now();
        Builder {
            id: Uuid::new_v4(),
            name: "bootstrap-builder".to_string(),
            host: Some("bootstrap-builder.test".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: PublicKey::from_base64(public_key_base64, "builder")
                .expect("test public key should parse"),
            public_key_fingerprint: String::new(),
            status: BuilderStatus::Inactive,
            max_cpu_cores: Some(4),
            max_memory_mb: Some(8192),
            max_concurrent_jobs: 1,
            enabled,
            current_session_id: None,
            current_session_started_at: None,
            last_heartbeat_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn test_derivation(
        derivation_type: DerivationType,
        name: &str,
        target: Option<&str>,
    ) -> Derivation {
        Derivation {
            id: 1,
            commit_id: Some(1),
            derivation_type,
            derivation_name: name.to_string(),
            derivation_path: None,
            scheduled_at: None,
            completed_at: None,
            started_at: None,
            attempt_count: 0,
            evaluation_duration_ms: None,
            error_message: None,
            pname: None,
            version: None,
            status_id: 1,
            derivation_target: target.map(str::to_string),
            build_elapsed_seconds: None,
            build_current_target: None,
            build_last_activity_seconds: None,
            build_last_heartbeat: None,
            cf_agent_enabled: None,
            store_path: None,
        }
    }

    #[test]
    fn derivation_archive_requisites_include_inputs_and_requested_drv() {
        let drv_path = "/nix/store/top-system.drv";
        let stdout = b"/nix/store/input-boot-json.drv\n/nix/store/source-path\n/nix/store/input-boot-json.drv\n";

        let paths = parse_derivation_requisites(stdout, drv_path);

        assert_eq!(paths[0], drv_path);
        assert!(paths.contains(&"/nix/store/input-boot-json.drv".to_string()));
        assert!(paths.contains(&"/nix/store/source-path".to_string()));
        assert_eq!(
            paths
                .iter()
                .filter(|path| path.as_str() == "/nix/store/input-boot-json.drv")
                .count(),
            1
        );
    }

    #[test]
    fn derivation_archive_paths_are_chunked_under_argument_limit() {
        let paths = vec![
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-first.drv".to_string(),
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-second.drv".to_string(),
            "/nix/store/cccccccccccccccccccccccccccccccc-third.drv".to_string(),
        ];

        let chunks = chunk_derivation_archive_paths(&paths, 80);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &paths[0..1]);
        assert_eq!(chunks[1], &paths[1..2]);
        assert_eq!(chunks[2], &paths[2..3]);
    }

    #[test]
    fn derivation_archive_chunking_keeps_small_sets_together() {
        let paths = vec![
            "/nix/store/a.drv".to_string(),
            "/nix/store/b.drv".to_string(),
            "/nix/store/c.drv".to_string(),
        ];

        let chunks = chunk_derivation_archive_paths(&paths, 1024);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], paths.as_slice());
    }

    #[test]
    fn empty_next_job_body_defaults_to_legacy_server_derivation_only() {
        let request = parse_next_job_request(b"").expect("empty request is legacy-compatible");

        assert_eq!(request.protocol_version, 1);
        assert_eq!(
            request.supported_execution_strategies,
            vec![RemoteBuildExecutionStrategy::ServerDerivation]
        );
    }

    #[test]
    fn legacy_get_next_job_request_defaults_to_protocol_v1_server_derivation_only() {
        let request = next_job_request_for_method(&Method::GET, b"")
            .expect("legacy GET request should be accepted");

        assert_eq!(request.protocol_version, 1);
        assert_eq!(
            request.supported_execution_strategies,
            vec![RemoteBuildExecutionStrategy::ServerDerivation]
        );
    }

    #[test]
    fn next_job_body_accepts_explicit_verified_source_capability() {
        let body = serde_json::to_vec(&NextJobRequest {
            protocol_version: 2,
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::ServerDerivation,
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
        })
        .expect("request should serialize");

        let request = parse_next_job_request(&body).expect("request should parse");

        assert_eq!(request.protocol_version, 2);
        assert!(
            request
                .supported_execution_strategies
                .contains(&RemoteBuildExecutionStrategy::SourceReEvaluateVerified)
        );
    }

    #[test]
    fn verified_source_target_expands_nixos_configuration_to_toplevel() {
        let derivation = test_derivation(
            DerivationType::NixOS,
            "webb",
            Some("nixosConfigurations.webb"),
        );

        assert_eq!(
            source_flake_target_for_derivation(&derivation),
            "nixosConfigurations.webb.config.system.build.toplevel"
        );
    }

    #[test]
    fn verified_source_target_preserves_full_nixos_toplevel_target() {
        let derivation = test_derivation(
            DerivationType::NixOS,
            "webb",
            Some(
                "git+ssh://git@example.invalid/repo#nixosConfigurations.webb.config.system.build.toplevel",
            ),
        );

        assert_eq!(
            source_flake_target_for_derivation(&derivation),
            "nixosConfigurations.webb.config.system.build.toplevel"
        );
    }

    #[test]
    fn build_stream_requires_explicit_type_discriminator() {
        let ambiguous_metrics_json =
            r#"{"cpu_percent":10.0,"ram_used_mb":100,"ram_total_mb":200,"timestamp":"t"}"#;

        let parsed = serde_json::from_str::<BuildStreamMessage>(ambiguous_metrics_json);
        assert!(
            parsed.is_err(),
            "untagged JSON should not be accepted as a valid stream frame"
        );
    }

    #[test]
    fn persisted_build_logs_replay_as_typed_log_frames() {
        let frames = persisted_build_log_frames(Some("line 1\nline 2\n"));

        assert_eq!(frames.len(), 1);
        let parsed = serde_json::from_str::<BuildStreamMessage>(&frames[0])
            .expect("persisted log frame should deserialize");
        assert!(matches!(
            parsed,
            BuildStreamMessage::Log { message } if message == "line 1\nline 2\n"
        ));
    }

    #[test]
    fn persisted_build_logs_replay_without_splitting_multibyte_chars() {
        let frames = persisted_build_log_frames(Some("é".repeat(40_000).as_str()));

        assert!(frames.len() > 1);
        let replayed = frames
            .iter()
            .map(|frame| {
                match serde_json::from_str::<BuildStreamMessage>(frame)
                    .expect("persisted log frame should deserialize")
                {
                    BuildStreamMessage::Log { message } => message,
                    _ => panic!("persisted replay should only create log frames"),
                }
            })
            .collect::<String>();

        assert_eq!(replayed, "é".repeat(40_000));
    }

    #[test]
    fn job_status_request_accepts_failure_body_without_status() {
        let parsed = parse_job_status_request(br#"{"error_message":"nix build failed"}"#)
            .expect("failure body without status should remain accepted");

        assert_eq!(parsed.status, None);
        assert_eq!(parsed.failure_phase, None);
        assert_eq!(parsed.failure_class, None);
        assert_eq!(parsed.error_message.as_deref(), Some("nix build failed"));
    }

    #[test]
    fn job_status_request_accepts_failure_phase() {
        let parsed = parse_job_status_request(
            br#"{"failure_phase":"derivation_mismatch","error_message":"drv mismatch"}"#,
        )
        .expect("failure body with phase should parse");

        assert_eq!(parsed.status, None);
        assert_eq!(parsed.failure_phase.as_deref(), Some("derivation_mismatch"));
        assert_eq!(
            format_failure_message(&parsed).as_deref(),
            Some("[derivation_mismatch] drv mismatch")
        );
    }

    #[test]
    fn job_status_request_accepts_additive_failure_class() {
        let parsed = parse_job_status_request(
            br#"{"failure_phase":"source_fetch","failure_class":"transient","error_message":"timeout"}"#,
        )
        .expect("classified failure should parse");

        assert_eq!(
            retry_failure_class(&parsed),
            crate::models::retry_policy::RetryFailureClass::Transient
        );
    }

    #[test]
    fn derivation_mismatch_is_never_retryable_even_if_misclassified() {
        let parsed = parse_job_status_request(
            br#"{"failure_phase":"derivation_mismatch","failure_class":"transient"}"#,
        )
        .expect("classified failure should parse");

        assert_eq!(
            retry_failure_class(&parsed),
            crate::models::retry_policy::RetryFailureClass::DerivationMismatch
        );
    }

    #[test]
    fn job_status_request_accepts_empty_failure_body() {
        let parsed = parse_job_status_request(b"")
            .expect("empty failure body should still allow job failure reporting");

        assert_eq!(parsed.status, None);
        assert_eq!(parsed.failure_phase, None);
        assert_eq!(parsed.error_message, None);
    }

    #[test]
    fn invalid_job_status_details_fallback_preserves_failure_signal() {
        let parsed = parse_job_status_request(b"not json");
        assert!(parsed.is_err());

        let fallback = fallback_job_status_request_for_invalid_details();

        assert_eq!(fallback.status, None);
        assert_eq!(fallback.failure_phase.as_deref(), Some("build"));
        assert_eq!(
            fallback.error_message.as_deref(),
            Some("builder reported failure with invalid failure details")
        );
    }

    #[test]
    fn resolve_builder_request_accepts_client_canonical_payload() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let timestamp = Utc::now().to_rfc3339();
        let (headers, body, public_key_base64) = signed_resolve_request(&signing_key, timestamp);

        let (request, _) = verify_builder_resolve_request(&headers, &body)
            .expect("signed bootstrap request should verify");

        assert_eq!(request.public_key, public_key_base64);
    }

    #[test]
    fn resolve_builder_request_accepts_client_generated_bootstrap_signature() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let public_key_base64 =
            general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());
        let body = serde_json::to_vec(&ResolveBuilderIdRequest {
            public_key: public_key_base64.clone(),
            session_id: Some(Uuid::new_v4()),
        })
        .expect("resolve request should serialize");

        let (signature, timestamp) = BuilderApiClient::sign_bootstrap_request(
            &signing_key,
            "POST",
            "/api/v1/builders/resolve-id",
            &body,
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Timestamp",
            HeaderValue::from_str(&timestamp).expect("valid timestamp header"),
        );
        headers.insert(
            "X-Signature",
            HeaderValue::from_str(&signature).expect("valid signature header"),
        );

        let (request, _) = verify_builder_resolve_request(&headers, &body)
            .expect("server verifier should accept client-generated bootstrap signature");

        assert_eq!(request.public_key, public_key_base64);
    }

    #[test]
    fn resolve_builder_request_rejects_tampered_body_bytes() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let timestamp = Utc::now().to_rfc3339();
        let (headers, mut body, _) = signed_resolve_request(&signing_key, timestamp);
        body.push(b' ');

        let (status, message) = verify_builder_resolve_request(&headers, &body)
            .expect_err("body-byte tampering should invalidate signature");

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(message.contains("signature verification failed"));
    }

    #[test]
    fn resolve_builder_request_rejects_expired_timestamp() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let expired_timestamp = (Utc::now() - Duration::minutes(10)).to_rfc3339();
        let (headers, body, _) = signed_resolve_request(&signing_key, expired_timestamp);

        let (status, message) = verify_builder_resolve_request(&headers, &body)
            .expect_err("expired bootstrap timestamp should be rejected");

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(message.contains("freshness window"));
    }

    #[test]
    fn resolve_builder_request_rejects_invalid_signature() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let other_signing_key = SigningKey::generate(&mut OsRng);
        let timestamp = Utc::now().to_rfc3339();
        let (mut headers, body, _) = signed_resolve_request(&signing_key, timestamp.clone());
        let wrong_payload =
            canonical_signature_payload("POST", "/api/v1/builders/resolve-id", &timestamp, &body);
        let wrong_signature = other_signing_key.sign(&wrong_payload);
        headers.insert(
            "X-Signature",
            HeaderValue::from_str(&general_purpose::STANDARD.encode(wrong_signature.to_bytes()))
                .expect("valid signature header"),
        );

        let (status, message) = verify_builder_resolve_request(&headers, &body)
            .expect_err("signature from another key should be rejected");

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(message.contains("signature verification failed"));
    }

    #[test]
    fn resolve_registered_builder_returns_uuid_for_enabled_builder() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let public_key_base64 =
            general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());
        let builder = test_builder(&public_key_base64, true);
        let expected_id = builder.id;

        let resolved_id = builder_id_for_resolved_builder(Some(builder))
            .expect("enabled registered builder should resolve");

        assert_eq!(resolved_id, expected_id);
    }

    #[test]
    fn resolve_registered_builder_returns_404_for_unregistered_key() {
        let (status, message) = builder_id_for_resolved_builder(None)
            .expect_err("missing builder should return not found");

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(message.contains("not registered"));
    }

    #[test]
    fn resolve_registered_builder_returns_403_for_disabled_builder() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let public_key_base64 =
            general_purpose::STANDARD.encode(signing_key.verifying_key().to_bytes());
        let builder = test_builder(&public_key_base64, false);

        let (status, message) = builder_id_for_resolved_builder(Some(builder))
            .expect_err("disabled builder should be forbidden");

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(message.contains("disabled"));
    }

    #[test]
    fn create_builder_duplicate_name_maps_to_conflict() {
        let error = anyhow!("duplicate key value violates unique constraint \"builders_name_key\"");
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body, "Builder name already exists");
    }

    #[test]
    fn create_builder_invalid_environment_maps_to_bad_request() {
        let error = anyhow!(
            "insert or update on table \"builder_environment_assignments\" violates foreign key constraint \"builder_environment_assignments_environment_id_fkey\"",
        );
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, "One or more selected environments do not exist");
    }

    #[test]
    fn create_builder_invalid_public_key_maps_to_bad_request() {
        let error = anyhow!("Invalid public key format");
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.starts_with("Invalid public key:"));
    }

    #[test]
    fn create_builder_unexpected_error_maps_to_internal_server_error() {
        let error = anyhow!("database connection timeout");
        let (status, body) = map_create_builder_error(&error);

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, "Failed to create builder");
    }

    #[test]
    fn build_log_append_status_allows_cancelling_but_rejects_terminal() {
        for status in ["queued", "building", "cancelling"] {
            assert!(
                super::build_log_append_status_allowed(status),
                "{status} should accept builder log appends"
            );
        }

        for status in ["cancelled", "failed", "success"] {
            assert!(
                !super::build_log_append_status_allowed(status),
                "{status} should reject builder log appends"
            );
        }
    }

    // ── builder_https_verified_by_trusted_proxy tests ──────────────────────

    fn make_headers_with(key: &str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::header::HeaderName::from_bytes(key.as_bytes()).unwrap(),
            axum::http::header::HeaderValue::from_str(value).unwrap(),
        );
        h
    }

    fn server_config_with_trust(trust: bool) -> crate::config::ServerConfig {
        let mut cfg = crate::config::ServerConfig::default();
        cfg.trust_forwarded_builder_https = trust;
        cfg
    }

    #[test]
    fn credential_check_blocked_when_flag_false_even_with_https_header() {
        let cfg = server_config_with_trust(false);
        let headers = make_headers_with("x-forwarded-proto", "https");
        assert!(
            !builder_https_verified_by_trusted_proxy(&cfg, &headers),
            "must not trust forwarded headers when flag is off"
        );
    }

    #[test]
    fn credential_check_blocked_when_flag_true_but_no_header() {
        let cfg = server_config_with_trust(true);
        let headers = HeaderMap::new();
        assert!(
            !builder_https_verified_by_trusted_proxy(&cfg, &headers),
            "must not pass when flag is on but no forwarded-proto header present"
        );
    }

    #[test]
    fn credential_check_blocked_when_flag_true_but_header_says_http() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("x-forwarded-proto", "http");
        assert!(
            !builder_https_verified_by_trusted_proxy(&cfg, &headers),
            "must not pass when forwarded-proto says http"
        );
    }

    #[test]
    fn credential_check_passes_when_flag_true_and_x_forwarded_proto_https() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("x-forwarded-proto", "https");
        assert!(
            builder_https_verified_by_trusted_proxy(&cfg, &headers),
            "must pass when flag is on and x-forwarded-proto asserts https"
        );
    }

    #[test]
    fn credential_check_passes_when_flag_true_and_forwarded_proto_https() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("forwarded", "for=1.2.3.4;proto=https");
        assert!(
            builder_https_verified_by_trusted_proxy(&cfg, &headers),
            "must pass when flag is on and Forwarded field asserts proto=https"
        );
    }

    #[test]
    fn credential_check_passes_when_flag_true_and_x_forwarded_ssl_on() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("x-forwarded-ssl", "on");
        assert!(
            builder_https_verified_by_trusted_proxy(&cfg, &headers),
            "must pass when flag is on and x-forwarded-ssl: on"
        );
    }

    // ── ServerBundledArchive / source mirror tests ─────────────────────────

    #[test]
    fn source_mirror_id_is_deterministic() {
        let id1 = super::source_mirror_id("https://github.com/example/repo.git");
        let id2 = super::source_mirror_id("https://github.com/example/repo.git");
        assert_eq!(id1, id2);
        assert!(id1.starts_with("repo-"));
    }

    #[test]
    fn source_mirror_id_varies_by_url() {
        let id1 = super::source_mirror_id("https://github.com/example/repo-a.git");
        let id2 = super::source_mirror_id("https://github.com/example/repo-b.git");
        assert_ne!(id1, id2);
    }

    #[test]
    fn server_mirror_path_contains_mirror_id() {
        let archive_root = std::path::PathBuf::from("/var/lib/crystal-forge/source-archives");
        let path = super::server_mirror_path(&archive_root, "https://github.com/example/repo.git");
        let mirror_id = super::source_mirror_id("https://github.com/example/repo.git");
        assert_eq!(
            path,
            archive_root
                .join("mirrors")
                .join(format!("{mirror_id}.git"))
        );
    }

    #[test]
    fn source_archive_url_format_matches_download_endpoint() {
        // The archive_url set in get_next_job must be parseable as an API path
        // that the builder can GET as an authenticated request.
        let builder_id = uuid::Uuid::new_v4();
        let job_id = uuid::Uuid::new_v4();
        let url = format!(
            "/api/v1/builders/{}/jobs/{}/source-archive",
            builder_id, job_id
        );
        assert!(url.contains(&builder_id.to_string()));
        assert!(url.contains(&job_id.to_string()));
        assert!(url.ends_with("/source-archive"));
    }

    #[test]
    fn chunk_derivation_archive_paths_respects_arg_limit() {
        let paths: Vec<String> = (0..100)
            .map(|i| {
                format!(
                    "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa{:04}-path-{}",
                    i, i
                )
            })
            .collect();
        let chunks = super::chunk_derivation_archive_paths(&paths, 512);
        // Each chunk must not exceed the byte limit
        for chunk in &chunks {
            let total: usize = chunk.iter().map(|p| p.len() + 1).sum(); // +1 for space separator
            assert!(
                total <= 512,
                "chunk total arg bytes {total} exceeds 512 limit"
            );
        }
        // All paths must appear exactly once
        let all: Vec<_> = chunks.iter().flat_map(|c| c.iter()).collect();
        assert_eq!(all.len(), paths.len());
    }

    // ── ExportStreamSplicer: multi-chunk export stream splicing ─────────────

    /// Simulate a single-chunk export stream: records then 8-byte terminator.
    /// The final chunk must pass its terminator through untouched.
    #[test]
    fn export_splicer_single_chunk_passes_terminator_through() {
        let mut splicer = super::ExportStreamSplicer::new();
        // "records" payload followed by the 8-byte zero terminator
        let mut stream = b"RECORDS-PAYLOAD".to_vec();
        stream.extend_from_slice(&[0u8; 8]);

        let forwarded = splicer.push(&stream);
        let tail = splicer
            .finish(true)
            .expect("final chunk finish must succeed");

        let mut result = forwarded;
        if let Some(t) = tail {
            result.extend_from_slice(&t);
        }
        assert_eq!(
            result, stream,
            "single-chunk stream must be forwarded byte-identical"
        );
    }

    /// Two chunks: the first chunk's terminator must be stripped, the second's
    /// kept, producing one valid continuous stream.
    #[test]
    fn export_splicer_strips_intermediate_terminator() {
        let mut chunk1 = b"CHUNK-ONE-RECORDS".to_vec();
        chunk1.extend_from_slice(&[0u8; 8]);
        let mut chunk2 = b"CHUNK-TWO-RECORDS".to_vec();
        chunk2.extend_from_slice(&[0u8; 8]);

        let mut spliced: Vec<u8> = Vec::new();

        // Chunk 1 (intermediate): terminator must be verified and dropped.
        let mut splicer = super::ExportStreamSplicer::new();
        spliced.extend_from_slice(&splicer.push(&chunk1));
        let tail = splicer
            .finish(false)
            .expect("intermediate chunk with zero terminator must succeed");
        assert!(tail.is_none(), "intermediate terminator must be dropped");

        // Chunk 2 (final): terminator must be forwarded.
        let mut splicer = super::ExportStreamSplicer::new();
        spliced.extend_from_slice(&splicer.push(&chunk2));
        if let Some(t) = splicer.finish(true).expect("final chunk must succeed") {
            spliced.extend_from_slice(&t);
        }

        let mut expected = b"CHUNK-ONE-RECORDS".to_vec();
        expected.extend_from_slice(b"CHUNK-TWO-RECORDS");
        expected.extend_from_slice(&[0u8; 8]);
        assert_eq!(
            spliced, expected,
            "spliced stream must contain both chunks' records and exactly one terminator"
        );
    }

    /// A nonzero tail on an intermediate chunk indicates a malformed or
    /// truncated export stream — must be a hard error, not silently spliced.
    #[test]
    fn export_splicer_rejects_nonzero_intermediate_tail() {
        let mut chunk = b"RECORDS".to_vec();
        chunk.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]); // last byte nonzero

        let mut splicer = super::ExportStreamSplicer::new();
        let _ = splicer.push(&chunk);
        assert!(
            splicer.finish(false).is_err(),
            "nonzero tail must be rejected for intermediate chunks"
        );
    }

    /// Bytes arriving in small increments (smaller than the 8-byte holdback)
    /// must still be spliced correctly.
    #[test]
    fn export_splicer_handles_tiny_reads() {
        let mut stream = b"AB".to_vec();
        stream.extend_from_slice(&[0u8; 8]);

        let mut splicer = super::ExportStreamSplicer::new();
        let mut forwarded: Vec<u8> = Vec::new();
        // Feed one byte at a time.
        for b in &stream {
            forwarded.extend_from_slice(&splicer.push(&[*b]));
        }
        let tail = splicer.finish(false).expect("zero terminator expected");
        assert!(tail.is_none());
        assert_eq!(forwarded, b"AB", "only the records may be forwarded");
    }

    #[test]
    fn source_archive_path_is_job_scoped() {
        // Archives are job-scoped so concurrent jobs for the same repo+commit
        // don't race to delete each other's archive during cleanup.
        let archive_root = std::path::PathBuf::from("/var/lib/crystal-forge/source-archives");
        let job_a = uuid::Uuid::new_v4();
        let job_b = uuid::Uuid::new_v4();

        let path_a = super::job_scoped_archive_path(&archive_root, job_a);
        let path_b = super::job_scoped_archive_path(&archive_root, job_b);

        // Two different jobs produce different paths even for the same repo+commit.
        assert_ne!(path_a, path_b);
        assert!(path_a.to_str().unwrap().ends_with(".tar.gz"));
        assert!(path_a.to_str().unwrap().contains(&job_a.to_string()));
        assert!(path_b.to_str().unwrap().contains(&job_b.to_string()));

        // Both paths are deterministic.
        assert_eq!(path_a, super::job_scoped_archive_path(&archive_root, job_a));
    }

    #[test]
    fn job_scoped_archive_cleanup_only_removes_one_job() {
        // Prove that cleanup_source_archive uses the job-scoped path by
        // checking the path helper returns unique files per job.
        let root = std::path::PathBuf::from("/var/lib/cf/archives");
        let j1 = uuid::Uuid::new_v4();
        let j2 = uuid::Uuid::new_v4();
        let p1 = super::job_scoped_archive_path(&root, j1);
        let p2 = super::job_scoped_archive_path(&root, j2);
        assert_ne!(p1, p2, "different jobs must have different archive paths");
    }

    // ── delta derivation transport: requested-path validation ──────────────

    fn manifest_fixture() -> Vec<String> {
        vec![
            "/nix/store/aaaa-one.drv".to_string(),
            "/nix/store/bbbb-two".to_string(),
            "/nix/store/cccc-three.drv".to_string(),
        ]
    }

    #[test]
    fn validate_requested_paths_accepts_authorized_subset() {
        let manifest = manifest_fixture();
        let requested = vec![
            "/nix/store/aaaa-one.drv".to_string(),
            "/nix/store/cccc-three.drv".to_string(),
        ];
        let validated = super::validate_requested_paths(&manifest, &requested)
            .expect("authorized subset must validate");
        assert_eq!(validated, requested);
    }

    #[test]
    fn validate_requested_paths_rejects_path_outside_manifest_with_403() {
        let manifest = manifest_fixture();
        let requested = vec![
            "/nix/store/aaaa-one.drv".to_string(),
            "/nix/store/evil-not-in-manifest".to_string(),
        ];
        let err = super::validate_requested_paths(&manifest, &requested)
            .expect_err("path outside manifest must be rejected");
        assert_eq!(err, StatusCode::FORBIDDEN);
    }

    #[test]
    fn validate_requested_paths_rejects_non_store_path() {
        let manifest = manifest_fixture();
        let requested = vec!["/etc/passwd".to_string()];
        let err = super::validate_requested_paths(&manifest, &requested)
            .expect_err("non-store path must be rejected");
        assert_eq!(err, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_requested_paths_rejects_empty_string_path() {
        let manifest = manifest_fixture();
        let requested = vec!["".to_string()];
        let err = super::validate_requested_paths(&manifest, &requested)
            .expect_err("empty path must be rejected");
        assert_eq!(err, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_requested_paths_deduplicates() {
        let manifest = manifest_fixture();
        let requested = vec![
            "/nix/store/aaaa-one.drv".to_string(),
            "/nix/store/aaaa-one.drv".to_string(),
            "/nix/store/bbbb-two".to_string(),
        ];
        let validated = super::validate_requested_paths(&manifest, &requested)
            .expect("duplicated authorized paths must validate");
        assert_eq!(
            validated,
            vec![
                "/nix/store/aaaa-one.drv".to_string(),
                "/nix/store/bbbb-two".to_string(),
            ]
        );
    }

    #[test]
    fn validate_requested_paths_allows_empty_request() {
        let manifest = manifest_fixture();
        let validated =
            super::validate_requested_paths(&manifest, &[]).expect("empty request list is allowed");
        assert!(validated.is_empty());
    }

    #[test]
    fn looks_like_store_path_rules() {
        assert!(super::looks_like_store_path("/nix/store/abc-foo.drv"));
        assert!(!super::looks_like_store_path("/etc/passwd"));
        assert!(!super::looks_like_store_path("nix/store/abc"));
        assert!(!super::looks_like_store_path("/nix/store/abc\0evil"));
    }
}
