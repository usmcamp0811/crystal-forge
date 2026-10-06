//! API handlers for builder management and work queue operations.
//!
//! This module provides two sets of endpoints:
//! 1. Builder Management (Admin-only): CRUD operations for builders
//! 2. Builder Work Queue (Builder-authenticated): Job polling and status updates

use axum::{
    Json,
    body::Body,
    extract::{
        ConnectInfo, Path, Query, State,
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
use url::Url;
use uuid::Uuid;

use crate::handlers::agent_request::CFState;
use crate::handlers::api::auth_session::require_csrf;
use crate::handlers::api::rbac::{
    authenticated_user_roles, has_admin_role, has_operator_or_admin_role, has_viewer_or_above_role,
    require_admin, require_operator_or_admin, require_viewer_or_above,
};
use crate::handlers::builder_request::{
    authenticate_builder_request, authenticate_builder_request_allow_inactive,
};
use crate::models::builders::{
    AppendLogsRequest, BuildJob, Builder, BuilderCachePushConfig, BuilderCreatedResponse,
    BuilderMetrics, BuilderSummary, BuilderWithEnvironments, CreateBuilderRequest,
    EstablishBuilderSessionRequest, EstablishBuilderSessionResponse, EvaluatorFingerprint,
    KeypairRegeneratedResponse, NextJobConflictReason, NextJobConflictResponse, NextJobRequest,
    RemoteBuildExecutionStrategy, ReportMetricsRequest, ResolveBuilderIdRequest,
    ResolveBuilderIdResponse, SourceInputDeliveryMode, UpdateBuilderEnvironmentsRequest,
    UpdateBuilderPublicKeyRequest, UpdateBuilderRequest,
    VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION, VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
    VerifiedSourceIdentity,
};
use crate::models::cache_destination::CacheDestination;
use crate::models::public_key::PublicKey;
use crate::queries::builders;

const NIX_STORE_EXPORT_ARG_BYTES_LIMIT: usize = 128 * 1024;
const ATTIC_PUSH_PATH_CHUNK_SIZE: usize = 200;
const BUILDER_SESSION_STALE_TIMEOUT_SECS: i64 = 60;
// A full signed closure import uses the same five-minute bound as CVE cache
// materialization. Failed verification keeps the build claim recoverable.
const CACHE_PUBLICATION_VERIFY_TIMEOUT_SECS: u64 = 300;

/// Verifies confidential transport through an explicitly trusted direct peer.
///
/// The proxy must overwrite `X-Forwarded-Proto`. Missing peer information,
/// duplicate headers, and protocol chains fail closed. The HTTPS opt-in alone
/// does not establish trust in a client-supplied header.
pub(crate) fn builder_https_verified_by_trusted_proxy(
    server_config: &crate::config::ServerConfig,
    headers: &HeaderMap,
    peer: Option<std::net::SocketAddr>,
) -> bool {
    BuilderHttpsEvidence::from_request(server_config, headers, peer).verified()
}

/// Records only credential-free evidence for the direct-peer HTTPS decision.
///
/// The decision and denial diagnostics use the same snapshot. Header contents,
/// CIDR strings, URLs, and request bodies must never enter this type.
#[derive(Debug, PartialEq, Eq)]
struct BuilderHttpsEvidence {
    direct_peer_ip: Option<std::net::IpAddr>,
    trust_forwarded_builder_https: bool,
    peer_cidr_match: bool,
    x_forwarded_proto_count: usize,
    exact_https: bool,
}

impl BuilderHttpsEvidence {
    fn from_request(
        server_config: &crate::config::ServerConfig,
        headers: &HeaderMap,
        peer: Option<std::net::SocketAddr>,
    ) -> Self {
        let direct_peer_ip = peer.map(|peer| peer.ip());
        let peer_cidr_match = direct_peer_ip.is_some_and(|ip| {
            server_config.trusted_proxy_cidrs.iter().any(|cidr| {
                let Some((network, prefix)) = cidr.split_once('/') else {
                    return false;
                };
                let (Ok(network), Ok(prefix)) =
                    (network.parse::<std::net::IpAddr>(), prefix.parse::<u8>())
                else {
                    return false;
                };
                match (ip, network) {
                    (std::net::IpAddr::V4(ip), std::net::IpAddr::V4(network)) if prefix <= 32 => {
                        let mask = u32::MAX.checked_shl(u32::from(32 - prefix)).unwrap_or(0);
                        u32::from(ip) & mask == u32::from(network) & mask
                    }
                    (std::net::IpAddr::V6(ip), std::net::IpAddr::V6(network)) if prefix <= 128 => {
                        let mask = u128::MAX.checked_shl(u32::from(128 - prefix)).unwrap_or(0);
                        u128::from(ip) & mask == u128::from(network) & mask
                    }
                    _ => false,
                }
            })
        });
        let values = headers.get_all("x-forwarded-proto");
        let x_forwarded_proto_count = values.iter().count();
        let exact_https = x_forwarded_proto_count == 1
            && values
                .iter()
                .next()
                .is_some_and(|value| value.as_bytes() == b"https");
        Self {
            direct_peer_ip,
            trust_forwarded_builder_https: server_config.trust_forwarded_builder_https,
            peer_cidr_match,
            x_forwarded_proto_count,
            exact_https,
        }
    }

    fn verified(&self) -> bool {
        self.trust_forwarded_builder_https && self.peer_cidr_match && self.exact_https
    }
}

fn build_log_append_status_allowed(status: &str) -> bool {
    matches!(status, "queued" | "building" | "cancelling")
}

fn cache_push_config_contains_credentials(config: &BuilderCachePushConfig) -> bool {
    config.niks3_write_auth.is_some()
        || config.attic_token.is_some()
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
    crate::queries::cache_push::eligible_cache_destinations_for_derivation(pool, derivation)
        .await
        .map_err(|e| {
            tracing::warn!(
                derivation_id = derivation.id,
                "failed to resolve canonical cache destinations: {e}"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

fn cache_type_from_destination(value: &str) -> Result<cf_protocol::cache::CacheType, StatusCode> {
    match value {
        "S3" => Ok(cf_protocol::cache::CacheType::S3),
        "Attic" => Ok(cf_protocol::cache::CacheType::Attic),
        "Http" => Ok(cf_protocol::cache::CacheType::Http),
        "Nix" => Ok(cf_protocol::cache::CacheType::Nix),
        "Niks3" => Ok(cf_protocol::cache::CacheType::Niks3),
        _ => Err(StatusCode::CONFLICT),
    }
}

fn builder_cache_push_config_from_destination(
    destination: &CacheDestination,
) -> Result<BuilderCachePushConfig, StatusCode> {
    let cache_type = cache_type_from_destination(&destination.cache_type)?;
    if cache_type == cf_protocol::cache::CacheType::Niks3 {
        let (url, keys, _) = destination
            .read_config()
            .map_err(|_| StatusCode::CONFLICT)?;
        return Ok(BuilderCachePushConfig {
            cache_destination_id: Some(destination.id),
            cache_type,
            push_to: Some(url),
            push_after_build: true,
            signing_key: destination.signing_key_path.clone(),
            niks3_server_url: destination.niks3_server_url.clone(),
            niks3_write_auth: Some(
                destination
                    .niks3_write_auth()
                    .map_err(|_| StatusCode::CONFLICT)?,
            ),
            attic_public_key: Some(keys.join(" ")),
            parallel_uploads: Some(destination.parallel_uploads.unwrap_or(1).max(1) as u32),
            max_retries: destination.max_retries.unwrap_or(3).max(0) as u32,
            retry_delay_seconds: destination.retry_delay_seconds.unwrap_or(5).max(0) as u64,
            push_timeout_seconds: destination.push_timeout_seconds.unwrap_or(3600).max(1) as u64,
            ..BuilderCachePushConfig::disabled()
        });
    }
    Ok(BuilderCachePushConfig {
        cache_destination_id: Some(destination.id),
        parallel_uploads: Some(destination.parallel_uploads.unwrap_or(1).max(1) as u32),
        niks3_server_url: None,
        niks3_write_auth: None,
        cache_type,
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
    })
}

async fn builder_cache_push_config_for_derivation(
    pool: &sqlx::PgPool,
    derivation: &crate::derivations::Derivation,
) -> Result<BuilderCachePushConfig, StatusCode> {
    let destinations = resolve_cache_destinations_for_derivation(pool, derivation).await?;

    destinations
        .first()
        .map(builder_cache_push_config_from_destination)
        .unwrap_or_else(|| Ok(BuilderCachePushConfig::disabled()))
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
        repo_url: credential_free_repo_url(&flake.repo_url)?,
        commit_hash: commit.git_commit_hash,
        flake_target: source_flake_target_for_derivation(derivation),
        mirror_id: Some(mirror_id),
        mirror_path: None,
        worktree_path: None,
        lock_hash: None,
        archive_url: None,
        archive_sha256: None,
        immutable_source: None,
    }))
}

fn credential_free_repo_url(repo_url: &str) -> anyhow::Result<String> {
    let Ok(mut parsed) = Url::parse(repo_url) else {
        let without_suffix = repo_url.split(['?', '#']).next().unwrap_or_default();
        return Ok(without_suffix
            .rsplit_once('@')
            .map_or(without_suffix, |(_, location)| location)
            .to_string());
    };
    parsed
        .set_username("")
        .map_err(|_| anyhow::anyhow!("repository URL username cannot be removed"))?;
    parsed
        .set_password(None)
        .map_err(|_| anyhow::anyhow!("repository URL password cannot be removed"))?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NixEvaluatorIdentity {
    nix_version: String,
    evaluator_system: String,
}

fn parse_nix_eval_jobs_identity(output: &str) -> Option<NixEvaluatorIdentity> {
    output.lines().find_map(|line| {
        let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
        let extra = value.get("extraValue")?;
        Some(NixEvaluatorIdentity {
            nix_version: extra.get("nixVersion")?.as_str()?.to_string(),
            evaluator_system: extra.get("evaluatorSystem")?.as_str()?.to_string(),
        })
    })
}

fn parse_nix_version(output: &str) -> Option<String> {
    output
        .trim()
        .strip_prefix("nix (Nix) ")
        .filter(|version| !version.is_empty())
        .map(str::to_string)
}

async fn probe_nix_version(program: &std::path::Path) -> anyhow::Result<String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(program)
            .kill_on_drop(true)
            .arg("--version")
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("nix --version probe timed out"))??;
    if !output.status.success() {
        anyhow::bail!("nix --version probe failed");
    }
    parse_nix_version(String::from_utf8_lossy(&output.stdout).as_ref())
        .ok_or_else(|| anyhow::anyhow!("nix --version returned an invalid version"))
}

async fn executing_nix_version() -> anyhow::Result<String> {
    static VERSION: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    VERSION
        .get_or_try_init(|| probe_nix_version(std::path::Path::new("nix")))
        .await
        .cloned()
}

async fn executing_nix_evaluator_identity() -> anyhow::Result<NixEvaluatorIdentity> {
    static IDENTITY: tokio::sync::OnceCell<NixEvaluatorIdentity> =
        tokio::sync::OnceCell::const_new();
    IDENTITY
        .get_or_try_init(|| async {
            let output = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                tokio::process::Command::new("nix-eval-jobs")
                    .kill_on_drop(true)
                    .args([
                        "--expr",
                        "{ probe = builtins.derivation { name = \"crystal-forge-evaluator-probe\"; system = builtins.currentSystem; builder = \"/bin/sh\"; }; }",
                        "--workers",
                        "1",
                        "--meta",
                        "--apply",
                        "_: { nixVersion = builtins.nixVersion; evaluatorSystem = builtins.currentSystem; }",
                        "--option",
                        "pure-eval",
                        "false",
                        "--option",
                        "allow-import-from-derivation",
                        "true",
                    ])
                    .output(),
            )
            .await
            .map_err(|_| anyhow::anyhow!("nix-eval-jobs version probe timed out"))??;
            if !output.status.success() {
                anyhow::bail!("nix-eval-jobs version probe failed");
            }
            parse_nix_eval_jobs_identity(String::from_utf8_lossy(&output.stdout).trim())
                .ok_or_else(|| anyhow::anyhow!("nix-eval-jobs returned no evaluator identity"))
        })
        .await
        .cloned()
}

fn evaluator_fingerprint(
    executing_nix_version: &str,
    identity: NixEvaluatorIdentity,
) -> anyhow::Result<EvaluatorFingerprint> {
    // INVARIANT: The fingerprint describes the Nix library linked into the
    // authoritative evaluator, but only when the server's Nix CLI matches it.
    if executing_nix_version != identity.nix_version {
        anyhow::bail!(
            "executing Nix version {executing_nix_version} does not match nix-eval-jobs linked Nix version {}",
            identity.nix_version
        );
    }
    Ok(EvaluatorFingerprint {
        contract_version: VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
        nix_version: identity.nix_version,
        evaluator_system: identity.evaluator_system,
        pure_eval: true,
        lockfile_mutation_allowed: false,
        allow_import_from_derivation: true,
        source_materialization_schema_version: VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
    })
}

async fn current_evaluator_fingerprint() -> anyhow::Result<EvaluatorFingerprint> {
    let (nix_version, identity) =
        tokio::try_join!(executing_nix_version(), executing_nix_evaluator_identity())?;
    evaluator_fingerprint(&nix_version, identity)
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

fn source_archive_contract_is_authorized(
    execution_strategy: RemoteBuildExecutionStrategy,
    delivery: SourceInputDeliveryMode,
) -> bool {
    execution_strategy == RemoteBuildExecutionStrategy::SourceReEvaluateVerified
        && delivery == SourceInputDeliveryMode::ServerBundledArchive
}

fn next_job_conflict(reason: NextJobConflictReason) -> Response {
    (
        StatusCode::CONFLICT,
        Json(NextJobConflictResponse { reason }),
    )
        .into_response()
}

/// Returns a preclaim conflict when the selected cache cannot be decoded.
///
/// Only Niks3 requires the additive capability. Disabled publication and
/// existing cache types preserve their legacy dispatch behavior.
pub(crate) fn cache_type_conflict(
    request: &NextJobRequest,
    cache: &BuilderCachePushConfig,
) -> Option<NextJobConflictReason> {
    (cache.cache_type == cf_protocol::cache::CacheType::Niks3 && !request.capabilities.niks3_cache)
        .then_some(NextJobConflictReason::UnsupportedCacheType)
}

fn execution_strategy_conflict(
    request: &NextJobRequest,
    execution_strategy: RemoteBuildExecutionStrategy,
) -> Option<NextJobConflictReason> {
    (!request
        .supported_execution_strategies
        .contains(&execution_strategy))
    .then_some(NextJobConflictReason::UnsupportedExecutionStrategy)
}

fn evaluator_conflict(
    request: &NextJobRequest,
    authoritative: &EvaluatorFingerprint,
) -> Option<NextJobConflictReason> {
    (!verified_source_evaluator_is_compatible(request, authoritative))
        .then_some(NextJobConflictReason::IncompatibleEvaluator)
}

fn source_delivery_conflict(
    execution_strategy: RemoteBuildExecutionStrategy,
    delivery: SourceInputDeliveryMode,
) -> Option<NextJobConflictReason> {
    (!source_archive_contract_is_authorized(execution_strategy, delivery))
        .then_some(NextJobConflictReason::IncompatibleSourceDelivery)
}

pub(crate) fn verified_source_evaluator_is_compatible(
    request: &NextJobRequest,
    authoritative: &EvaluatorFingerprint,
) -> bool {
    request
        .supported_evaluator_contract_versions
        .contains(&authoritative.contract_version)
        && request.evaluator.as_ref() == Some(authoritative)
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

fn parse_next_job_request(body: &[u8]) -> Result<NextJobRequest, StatusCode> {
    if body.is_empty() {
        return Ok(legacy_next_job_request());
    }

    serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)
}

fn legacy_next_job_request() -> NextJobRequest {
    NextJobRequest {
        capabilities: Default::default(),
        protocol_version: 1,
        supported_execution_strategies: vec![RemoteBuildExecutionStrategy::ServerDerivation],
        supported_evaluator_contract_versions: Vec::new(),
        evaluator: None,
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
    if destination.cache_type == "Attic"
        && let (Some(endpoint), Some(cache)) = (
            destination.push_to.as_deref(),
            destination.attic_cache_name.as_deref(),
        )
        && let Ok(urls) = cf_config::resolve_attic_urls(endpoint, cache)
    {
        // Match the login and read consumers without changing persisted URLs.
        // The CLI still requires its remote profile to be initialized.
        command.env("ATTIC_SERVER_URL", urls.server_url.as_str());
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
    root: &str,
) -> Result<bool, StatusCode> {
    let cache_type = cache_type_from_destination(&destination.cache_type)?;
    if cache_type == cf_protocol::cache::CacheType::Niks3 {
        if !archive_paths.iter().any(|path| path == root) {
            return Err(StatusCode::CONFLICT);
        }
        for chunk in archive_paths.chunks(ATTIC_PUSH_PATH_CHUNK_SIZE) {
            sign_derivation_requisites_for_cache(destination, chunk).await?;
        }
        let auth = destination
            .niks3_write_auth()
            .map_err(|_| StatusCode::CONFLICT)?;
        // PERFORMANCE: Niks3 traverses the root's closure itself. Passing only
        // the authoritative root avoids ARG_MAX for large requisite manifests.
        let prepared = cf_config::cache_credentials::PreparedNiks3Push::new(
            destination
                .niks3_server_url
                .as_deref()
                .ok_or(StatusCode::CONFLICT)?,
            &auth,
            destination.parallel_uploads.unwrap_or(1).max(1) as u32,
            root,
        )
        .map_err(|_| StatusCode::CONFLICT)?;
        let deadline = std::time::Duration::from_secs(
            destination.push_timeout_seconds.unwrap_or(3600).max(1) as u64,
        );
        return run_niks3_input_owner(prepared, deadline, |event| {
            // SECURITY: Fixed fields identify an operation, never its arguments,
            // credential filenames, store paths, URLs or upstream diagnostics.
            tracing::info!(
                target: "crystal_forge::niks3_input_owner",
                operation = %event.operation,
                phase = event.phase,
                child_pid = event.child_pid.unwrap_or(0),
                child_reaped = event.child_reaped,
                cleanup_attempted = event.cleanup_attempted,
                outcome = event.outcome,
                "Niks3 input owner lifecycle"
            );
        })
        .await;
    }
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

// Completion acknowledges resource destruction, not successful directory
// removal: TempDir::drop can ignore filesystem errors. The final cleanup audit
// must still check the original roots after this acknowledgment.
struct Niks3InputOwnerEvent {
    operation: Uuid,
    phase: &'static str,
    child_pid: Option<u32>,
    child_reaped: bool,
    cleanup_attempted: bool,
    outcome: &'static str,
}

// CONCURRENCY: The handoff transfers child and prepared credentials together.
// Dropping the caller afterward detaches the owned task without changing upload
// cancellation policy.
// Child startup and the start acknowledgment precede the first await and task
// handoff, so a queued owner cannot hold resources without a visible start.
// It emits exactly one start and one completion on controlled return paths.
// Completion follows child wait/reap and explicit credential-owner drop. A
// failed reap is recorded as such and cannot establish a quiescent boundary.
async fn run_niks3_input_owner(
    prepared: cf_config::cache_credentials::PreparedNiks3Push,
    deadline: std::time::Duration,
    mut observe: impl FnMut(Niks3InputOwnerEvent) + Send + 'static,
) -> Result<bool, StatusCode> {
    let operation = Uuid::new_v4();
    let credential_directory = prepared
        .args
        .windows(2)
        .find(|pair| matches!(pair[0].as_str(), "--auth-token-path" | "--client-key"))
        .and_then(|pair| std::path::Path::new(&pair[1]).parent());
    let spawned = credential_directory.map(|directory| {
        let mut command = Command::new(&prepared.command);
        crate::derivations::utils::apply_niks3_env_to_command(&mut command, directory);
        command
            .args(&prepared.args)
            .kill_on_drop(true)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    });
    let child_pid = spawned
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(tokio::process::Child::id);
    // Preserve the deadline relative to child startup even if the owned task
    // must wait for a scheduler slot after this synchronous resource handoff.
    let expires = tokio::time::Instant::now() + deadline;
    observe(Niks3InputOwnerEvent {
        operation,
        phase: "started",
        child_pid,
        child_reaped: false,
        cleanup_attempted: false,
        outcome: "running",
    });
    tokio::spawn(async move {
        let (result, child_reaped, outcome) = match spawned {
            Some(Ok(mut child)) => {
                let completion = match tokio::time::timeout_at(expires, child.wait()).await {
                    Ok(Ok(status)) if status.success() => (Ok(true), true, "success"),
                    Ok(Ok(_)) => (Err(StatusCode::INTERNAL_SERVER_ERROR), true, "exit_failure"),
                    Ok(Err(_)) => {
                        let _ = child.kill().await;
                        let reaped = child.wait().await.is_ok();
                        (
                            Err(StatusCode::INTERNAL_SERVER_ERROR),
                            reaped,
                            "wait_failure",
                        )
                    }
                    Err(_) => {
                        let _ = child.kill().await;
                        let reaped = child.wait().await.is_ok();
                        (Err(StatusCode::CONFLICT), reaped, "timeout")
                    }
                };
                drop(child);
                completion
            }
            Some(Err(_)) => (
                Err(StatusCode::INTERNAL_SERVER_ERROR),
                false,
                "spawn_failure",
            ),
            None => (Err(StatusCode::CONFLICT), false, "configuration_failure"),
        };
        drop(prepared);
        observe(Niks3InputOwnerEvent {
            operation,
            phase: "completed",
            child_pid,
            child_reaped,
            cleanup_attempted: true,
            outcome,
        });
        result
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

#[cfg(test)]
mod niks3_input_owner_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    use std::time::{Duration, Instant};

    // Keep both FIFO ends open before spawning a child. O_NONBLOCK bounds every
    // read/write syscall; the parent endpoints prevent blocking opens or EOF
    // from making a missing child look like a completed handshake.
    fn handshake_fifo(path: &std::path::Path) -> std::io::Result<std::fs::File> {
        nix::unistd::mkfifo(
            path,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )?;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
    }

    fn poll_fifo(
        fifo: &std::fs::File,
        events: libc::c_short,
        until: Instant,
        phase: &'static str,
    ) -> std::io::Result<()> {
        loop {
            let remaining = until
                .checked_duration_since(Instant::now())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, phase))?;
            let milliseconds = remaining
                .as_millis()
                .saturating_add(1)
                .min(i32::MAX as u128);
            let mut descriptor = libc::pollfd {
                fd: fifo.as_raw_fd(),
                events,
                revents: 0,
            };
            // SAFETY: The borrowed File keeps its descriptor open. The initialized
            // stack pollfd is exclusively borrowed for one synchronous poll call.
            let result = unsafe { libc::poll(&mut descriptor, 1, milliseconds as i32) };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    // EINTR must not restart the original relative timeout.
                    continue;
                }
                return Err(error);
            }
            if result == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, phase));
            }
            if descriptor.revents & (libc::POLLERR | libc::POLLNVAL | libc::POLLHUP) != 0 {
                return Err(std::io::Error::other(phase));
            }
            if descriptor.revents & events != 0 {
                return Ok(());
            }
        }
    }

    fn ready_pid(fifo: &mut std::fs::File, until: Instant) -> std::io::Result<u32> {
        // A u32 PID needs at most ten decimal digits plus newline. Reject an
        // oversized or malformed marker without printing any received bytes.
        let mut digits = Vec::with_capacity(10);
        loop {
            poll_fifo(fifo, libc::POLLIN, until, "child readiness deadline")?;
            let mut byte = [0];
            match fifo.read(&mut byte) {
                Ok(1) if byte[0] == b'\n' => {
                    return std::str::from_utf8(&digits)
                        .ok()
                        .and_then(|line| line.parse::<u32>().ok())
                        .filter(|pid| *pid != 0)
                        .ok_or_else(|| std::io::Error::other("invalid child readiness marker"));
                }
                Ok(1) if byte[0].is_ascii_digit() && digits.len() < 10 => digits.push(byte[0]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                _ => return Err(std::io::Error::other("invalid child readiness marker")),
            }
        }
    }

    fn release_child(
        fifo: &mut std::fs::File,
        code: &[u8; 2],
        until: Instant,
    ) -> std::io::Result<()> {
        let mut written = 0;
        while written < code.len() {
            poll_fifo(fifo, libc::POLLOUT, until, "child release deadline")?;
            match fifo.write(&code[written..]) {
                Ok(0) => return Err(std::io::Error::other("child release made no progress")),
                Ok(count) => written += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    #[test]
    fn fifo_deadlines_cover_missing_readiness_and_absent_release_reader() {
        let fixture = tempfile::tempdir().unwrap();
        let mut ready = handshake_fifo(&fixture.path().join("ready")).unwrap();
        let mut release = handshake_fifo(&fixture.path().join("release")).unwrap();
        let bound = Duration::from_millis(30);
        assert_eq!(
            ready_pid(&mut ready, Instant::now() + bound)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
        // No child has opened a reader. The tiny release is still nonblocking
        // because the parent holds RDWR; readiness/owner checks identify absence.
        release_child(&mut release, b"0\n", Instant::now() + bound).unwrap();
        // With no external reader, exhaust the FIFO capacity and prove that a
        // blocked release fails at its own poll deadline rather than hanging.
        let fill_until = Instant::now() + Duration::from_secs(1);
        loop {
            assert!(Instant::now() < fill_until, "FIFO capacity-fill deadline");
            match release.write(&[0; 4096]) {
                Ok(count) => assert!(count > 0),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                _ => panic!("FIFO capacity-fill failed"),
            }
        }
        assert_eq!(
            release_child(&mut release, b"0\n", Instant::now() + bound)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
    }

    // FIFO handshakes prove that the child has inspected its live credentials.
    // The completion callback, not filesystem polling, is the cleanup boundary.
    #[tokio::test]
    async fn niks3_input_owner_acknowledges_reap_and_cleanup_after_detach() {
        for scenario in [
            "success",
            "exit_failure",
            "timeout",
            "detach",
            "spawn_failure",
            "configuration_failure",
        ] {
            let fixture = tempfile::tempdir().unwrap();
            let ready_path = fixture.path().join("ready");
            let release_path = fixture.path().join("release");
            let mut ready = handshake_fifo(&ready_path).unwrap();
            let mut release = handshake_fifo(&release_path).unwrap();
            let program = fixture.path().join("niks3");
            std::fs::write(&program, format!(
                "#!/bin/sh\nset -eu\ntest -f \"$HOME/token\"\nprintf '%s\\n' \"$$\" > '{}'\nIFS= read -r code < '{}'\ntest -f \"$HOME/token\"\nexit \"$code\"\n",
                ready_path.display(), release_path.display(),
            )).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut prepared = cf_config::cache_credentials::PreparedNiks3Push::new(
                "https://input.example",
                &cf_protocol::cache::Niks3WriteAuth::Token {
                    token: "synthetic-input-fixture".into(),
                },
                1,
                "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-input.drv",
            )
            .unwrap();
            let credentials = std::path::PathBuf::from(
                &prepared.args[prepared
                    .args
                    .iter()
                    .position(|arg| arg == "--auth-token-path")
                    .unwrap()
                    + 1],
            );
            prepared.command = if scenario == "spawn_failure" {
                fixture.path().join("absent").display().to_string()
            } else {
                program.display().to_string()
            };
            if scenario == "configuration_failure" {
                prepared.args.clear();
            }
            let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
            let observed_credentials = credentials.clone();
            let deadline = if scenario == "timeout" {
                Duration::from_secs(5)
            } else {
                Duration::from_secs(10)
            };
            let caller = tokio::spawn(run_niks3_input_owner(prepared, deadline, move |event| {
                if event.phase == "started" {
                    assert!(observed_credentials.exists());
                } else {
                    assert!(!observed_credentials.exists());
                    assert!(!observed_credentials.parent().unwrap().exists());
                    if let Some(pid) = event.child_pid {
                        assert!(event.child_reaped);
                        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
                    }
                }
                sender.send((event, Instant::now())).unwrap();
            }));
            let (started, started_at) =
                tokio::time::timeout(Duration::from_secs(5), receiver.recv())
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(started.phase, "started");
            let pid = if matches!(scenario, "spawn_failure" | "configuration_failure") {
                None
            } else {
                let pid = ready_pid(&mut ready, started_at + Duration::from_secs(2))
                    .unwrap_or_else(|error| panic!("{scenario}: readiness phase failed: {error}"));
                assert_eq!(Some(pid), started.child_pid);
                let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                    .unwrap_or_else(|_| panic!("{scenario}: ready child no longer exists"));
                let state = stat
                    .rsplit_once(')')
                    .unwrap()
                    .1
                    .split_whitespace()
                    .next()
                    .unwrap();
                assert!(
                    !matches!(state, "Z" | "X"),
                    "{scenario}: ready child is not alive"
                );
                assert!(credentials.exists());
                if scenario == "detach" {
                    caller.abort();
                }
                if scenario != "timeout" {
                    let code = if scenario == "exit_failure" {
                        b"7\n"
                    } else {
                        b"0\n"
                    };
                    release_child(&mut release, code, Instant::now() + Duration::from_secs(1))
                        .unwrap_or_else(|error| {
                            panic!("{scenario}: release phase failed: {error}")
                        });
                }
                Some(pid)
            };
            // Include the remaining configured operation deadline and two seconds
            // for reap/drop/ack scheduling. This is a bound, not a readiness sleep.
            let remaining = (started_at + deadline + Duration::from_secs(2))
                .saturating_duration_since(Instant::now());
            let (completed, _) = tokio::time::timeout(remaining, receiver.recv())
                .await
                .unwrap_or_else(|_| panic!("{scenario}: owner completion deadline"))
                .unwrap_or_else(|| panic!("{scenario}: owner ended without acknowledgment"));
            assert_eq!(completed.operation, started.operation);
            assert_eq!(completed.phase, "completed");
            assert!(completed.cleanup_attempted);
            assert!(!credentials.exists());
            if let Some(pid) = pid {
                assert!(completed.child_reaped);
                assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
            }
            assert_eq!(
                completed.outcome,
                if scenario == "detach" {
                    "success"
                } else {
                    scenario
                }
            );
            if scenario != "detach" {
                let result = tokio::time::timeout(Duration::from_secs(2), caller)
                    .await
                    .expect("caller result deadline")
                    .unwrap();
                assert_eq!(result.is_ok(), scenario == "success");
            }
            assert!(
                tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                    .await
                    .expect("owner event-channel closure deadline")
                    .is_none(),
                "owner emitted more than two events"
            );
        }
    }
}

async fn push_derivation_requisites_to_assigned_cache(
    pool: &sqlx::PgPool,
    derivation: &crate::derivations::Derivation,
    archive_paths: &[String],
    job: &BuildJob,
) -> Result<bool, StatusCode> {
    let destinations = resolve_cache_destinations_for_derivation(pool, derivation).await?;

    if destinations.is_empty() {
        if job.cache_dispatch_recorded_at.is_some() && job.dispatched_cache_destination_id.is_some()
        {
            return Err(StatusCode::CONFLICT);
        }
        tracing::debug!(
            derivation_id = derivation.id,
            derivation_name = %derivation.derivation_name,
            "no assigned or global cache destination configured for derivation closure publish"
        );
        return Ok(false);
    }

    // SECURITY: Input closure publication uses the same immutable selection
    // as output publication. Reordering eligible caches must not redirect it.
    let destination = if job.cache_dispatch_recorded_at.is_some() {
        let Some(selected) = job.dispatched_cache_destination_id else {
            return Ok(false);
        };
        destinations
            .iter()
            .find(|destination| destination.id == selected)
            .ok_or(StatusCode::CONFLICT)?
    } else {
        let first = &destinations[0];
        if first.cache_type == "Niks3" {
            return Err(StatusCode::CONFLICT);
        }
        first
    };
    let root = derivation
        .derivation_path
        .as_deref()
        .ok_or(StatusCode::CONFLICT)?;
    push_derivation_requisites_to_cache_destination(destination, archive_paths, root).await
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

    let capabilities_recorded =
        crate::queries::cve_scan_leases::record_session_cve_capabilities(
        &state.pool,
        builder_id,
        session_id,
        request.capabilities,
    )
    .await
    .map_err(|error| {
        tracing::error!(builder_id = %builder_id, %error, "failed to persist builder CVE capabilities");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to establish builder capabilities".to_string(),
        )
    })?;
    if !capabilities_recorded {
        return Err((
            StatusCode::GONE,
            "Builder session is no longer active".to_string(),
        ));
    }

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

    let capabilities_recorded =
        crate::queries::cve_scan_leases::record_session_cve_capabilities(
        &state.pool,
        builder_id,
        request.session_id,
        request.capabilities,
    )
    .await
    .map_err(|error| {
        tracing::error!(builder_id = %builder_id, %error, "failed to persist builder CVE capabilities");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to establish builder capabilities".to_string(),
        )
    })?;
    if !capabilities_recorded {
        return Err((
            StatusCode::GONE,
            "Builder session is no longer active".to_string(),
        ));
    }

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

/// Reports the active attempt selected by a manual requeue request.
#[derive(Debug, Serialize)]
pub struct RequeueBuildJobResponse {
    /// Active build attempt identity.
    pub attempt_id: Uuid,
    /// Immutable lineage attempt number.
    pub attempt_number: i32,
    /// Active attempt status.
    pub status: String,
    /// `created` when this request inserted the attempt, otherwise `reused`.
    pub outcome: &'static str,
}

/// POST /api/v1/build-jobs/:id/requeue - Requeues a terminal build job.
///
/// The endpoint requires operator or administrator authorization and a valid
/// CSRF token. It returns the new or existing active attempt so retries are
/// idempotent from the caller's perspective.
pub async fn requeue_build_job(
    State(state): State<CFState>,
    Path(job_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Response {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "forbidden",
                "message": "Operator or admin access required"
            })),
        )
            .into_response();
    };
    if !has_operator_or_admin_role(&roles) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "forbidden",
                "message": "Operator or admin access required"
            })),
        )
            .into_response();
    }

    if let Err(response) = require_csrf(&headers) {
        return response;
    }

    match builders::requeue_build_job_as_new_attempt(
        &state.pool,
        &job_id,
        user_id,
        has_admin_role(&roles),
    )
    .await
    {
        Ok(result) => {
            let outcome = match result.disposition {
                builders::RequeueBuildJobDisposition::Created => "created",
                builders::RequeueBuildJobDisposition::Reused => "reused",
            };
            Json(RequeueBuildJobResponse {
                attempt_id: result.attempt.id,
                attempt_number: result.attempt.attempt_number,
                status: result.attempt.status,
                outcome,
            })
            .into_response()
        }
        Err(builders::RequeueBuildJobError::NotFoundOrHidden) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "message": "Build job not found"
            })),
        )
            .into_response(),
        Err(builders::RequeueBuildJobError::LifecycleConflict { status }) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "build_job_not_terminal",
                "message": "Build job is not in a terminal state",
                "status": status
            })),
        )
            .into_response(),
        Err(builders::RequeueBuildJobError::EvaluatorContractObsolete { commit_id }) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "evaluator_contract_obsolete",
                "message": "Re-evaluate this exact revision before retrying the build",
                "commit_id": commit_id,
                "action": "re_evaluate_commit"
            })),
        )
            .into_response(),
        Err(builders::RequeueBuildJobError::Internal(error)) => {
            tracing::error!(job_id = %job_id, error = %error, "failed to requeue build attempt");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "requeue_failed",
                    "message": "Failed to create or reuse a build attempt"
                })),
            )
                .into_response()
        }
    }
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
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };
    if !has_viewer_or_above_role(&roles) {
        return Err(StatusCode::FORBIDDEN);
    }
    let visibility_user_id = (!has_admin_role(&roles)).then_some(user_id);

    // Clamp per-request limit to prevent unbounded result sets and overflow.
    params.limit = params.limit.max(1).min(crate::api::models::LIMIT_MAX);
    params.page = params.page.max(1);
    if (params.page - 1).checked_mul(params.limit).is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let result = crate::queries::dashboard::list_build_queue_paginated(
        &state.pool,
        &params,
        visibility_user_id,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list build queue: {e:#}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(result))
}

/// Returns one exact visible build attempt without scanning paginated lists.
///
/// # Errors
///
/// Returns `403 Forbidden` when the caller is unauthenticated or lacks Viewer
/// access. Returns `404 Not Found` for both missing attempts and attempts hidden
/// by the caller's environment scope. Returns `500 Internal Server Error` when
/// the database query fails.
pub async fn get_build_attempt(
    State(state): State<CFState>,
    Path(attempt_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<crate::api::models::BuildAttemptLookupResponse>, StatusCode> {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };
    if !has_viewer_or_above_role(&roles) {
        return Err(StatusCode::FORBIDDEN);
    }
    let visibility_user_id = (!has_admin_role(&roles)).then_some(user_id);

    let attempt = crate::queries::dashboard::get_build_attempt_by_id(
        &state.pool,
        attempt_id,
        visibility_user_id,
    )
    .await
    .map_err(|error| {
        tracing::error!(%attempt_id, "Failed to load exact build attempt: {error:#}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    // SECURITY: Hidden and missing attempts are both 404 so this endpoint does
    // not disclose build identity across environment boundaries.
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(attempt))
}

/// GET /api/v1/build-jobs/recent - Recent completed/failed builds (viewer+)
pub async fn list_recent_build_jobs(
    State(state): State<CFState>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<crate::api::models::BuildQueuePageResponse>, StatusCode> {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };
    if !has_viewer_or_above_role(&roles) {
        return Err(StatusCode::FORBIDDEN);
    }
    let visibility_user_id = (!has_admin_role(&roles)).then_some(user_id);

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

    let items = crate::queries::dashboard::fetch_recent_build_history(
        &state.pool,
        &query,
        visibility_user_id,
    )
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

    if let Some(session_id) = verified.builder_session_id
        && !crate::queries::cve_scan_leases::record_session_cve_capabilities(
            &state.pool,
            builder_id,
            session_id,
            metrics.capabilities.clone(),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::GONE);
    }

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

/// Claims one server-authorized CVE scan lease for an API builder.
///
/// A direct post-build request can claim only the successful build named in the
/// request. Background claims yield no work while build work is queued or while
/// this builder owns an active build. Incapable builders also receive no work.
///
/// # Errors
///
/// Returns an HTTP error for invalid authentication, builder/session mismatch,
/// malformed input, or persistence failure.
pub async fn claim_cve_scan(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::CveScanClaimResponse>, StatusCode> {
    let path = format!("/api/v1/builders/{builder_id}/cve-scans/claim");
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let request: crate::models::builders::CveScanClaimRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    if verified.builder_session_id != Some(request.builder_session_id) {
        return Err(StatusCode::GONE);
    }
    if !request.capabilities.supports_current_cve_schema() {
        return Ok(Json(crate::models::builders::CveScanClaimResponse {
            claim: None,
        }));
    }
    let claim = crate::queries::cve_scan_leases::claim_remote_cve_scan(
        &state.pool,
        builder_id,
        request.builder_session_id,
        request.completed_build_job_id,
    )
    .await
    .map_err(|error| {
        if error.to_string().contains("session_mismatch")
            || error.to_string().contains("builder_inactive")
        {
            StatusCode::GONE
        } else {
            tracing::error!(builder_id = %builder_id, %error, "remote CVE claim failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    Ok(Json(crate::models::builders::CveScanClaimResponse {
        claim,
    }))
}

/// Renews one exact remote CVE scan execution lease.
///
/// # Errors
///
/// Returns `403 Forbidden` for mismatched lease ownership, `410 Gone` for an
/// expired or superseded lease, or another HTTP error for invalid
/// authentication, malformed input, or persistence failure.
pub async fn heartbeat_cve_scan(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::CveScanHeartbeatResponse>, StatusCode> {
    let path = format!("/api/v1/builders/{builder_id}/cve-scans/heartbeat");
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let request: crate::models::builders::CveScanHeartbeatRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    if request.lease.builder_id != builder_id
        || verified.builder_session_id != Some(request.lease.builder_session_id)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let Some(lease_expires_at) = crate::queries::cve_scan_leases::heartbeat_remote_cve_scan(
        &state.pool,
        request.lease,
        request.entries_collected,
        request.observations_collected,
        &request.diagnostics,
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::GONE);
    };
    Ok(Json(crate::models::builders::CveScanHeartbeatResponse {
        lease_expires_at,
        revocation_requested: false,
    }))
}

/// Completes one remote CVE lease after server-side canonical validation.
///
/// The server returns `422 Unprocessable Entity` without changing the lease for
/// invalid evidence. A same-digest retry succeeds idempotently. A different
/// digest for the same completed execution returns `409 Conflict`.
///
/// # Errors
///
/// Returns an HTTP error for invalid authentication, mismatched ownership,
/// malformed or semantically invalid evidence, stale execution, digest
/// conflict, or persistence failure.
pub async fn complete_cve_scan(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::CveScanCompleteResponse>, StatusCode> {
    let path = format!("/api/v1/builders/{builder_id}/cve-scans/complete");
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let request: crate::models::builders::CveScanCompleteRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    if request.lease.builder_id != builder_id
        || verified.builder_session_id != Some(request.lease.builder_session_id)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    use crate::queries::cve_scan_leases::RemoteCompletion;
    match crate::queries::cve_scan_leases::complete_remote_cve_scan(&state.pool, request)
        .await
        .map_err(|error| {
            tracing::error!(builder_id = %builder_id, %error, "remote CVE completion failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
        RemoteCompletion::Completed(digest) => {
            Ok(Json(crate::models::builders::CveScanCompleteResponse {
                result_digest_sha256: digest,
                already_completed: false,
            }))
        }
        RemoteCompletion::AlreadyCompleted(digest) => {
            Ok(Json(crate::models::builders::CveScanCompleteResponse {
                result_digest_sha256: digest,
                already_completed: true,
            }))
        }
        RemoteCompletion::Invalid(_) | RemoteCompletion::DigestMismatch => {
            Err(StatusCode::UNPROCESSABLE_ENTITY)
        }
        RemoteCompletion::DigestConflict => Err(StatusCode::CONFLICT),
        RemoteCompletion::Stale => Err(StatusCode::GONE),
    }
}

/// Reports a remote scanner failure without changing build or cache outcome.
///
/// # Errors
///
/// Returns `410 Gone` for a stale execution or another HTTP error for invalid
/// authentication, mismatched ownership, malformed input, or persistence
/// failure.
pub async fn fail_cve_scan(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::CveScanFailResponse>, StatusCode> {
    let path = format!("/api/v1/builders/{builder_id}/cve-scans/fail");
    let verified =
        authenticate_builder_request(&headers, body.clone(), "POST", &path, &state.pool).await?;
    if verified.builder_id != builder_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let request: crate::models::builders::CveScanFailRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    if request.lease.builder_id != builder_id
        || verified.builder_session_id != Some(request.lease.builder_session_id)
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let requeued = crate::queries::cve_scan_leases::fail_remote_cve_scan(
        &state.pool,
        request.lease,
        request.failure_class,
        &request.error_message,
        &request.diagnostics,
    )
    .await
    .map_err(|error| {
        if error.to_string().contains("stale_cve_scan_execution") {
            StatusCode::GONE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    Ok(Json(crate::models::builders::CveScanFailResponse {
        requeued,
    }))
}
/// GET/POST /api/v1/builders/:id/next-job - Get next job for builder
///
/// This endpoint implements the load-based job assignment logic:
/// 1. Filter jobs by builder's environment assignments (or all if no assignments)
/// 2. Check builder's current concurrency limit
/// 3. Return highest-priority queued job if available
/// Niks3 dispatch requires capability advertisement in the signed poll. Cache
/// preflight precedes the exact-candidate claim, so incompatible builders never
/// claim work or receive cache credentials. Legacy cache types remain eligible.
/// Credential-bearing cache settings require an allowlisted direct proxy peer
/// with the HTTPS opt-in and one proxy-overwritten `X-Forwarded-Proto: https`.
///
/// # Errors
/// Returns an authentication or authorization status for invalid builders and
/// sessions, `NOT_FOUND` when no job is available, or a conflict/error status
/// when dispatch preflight fails. Secret delivery fails closed without verified
/// confidential transport.
pub async fn get_next_job(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    peer: Option<ConnectInfo<std::net::SocketAddr>>,
    method: Method,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
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
    if let Some(reason) = execution_strategy_conflict(&next_job_request, execution_strategy) {
        tracing::warn!(
            builder_id = %builder_id,
            ?execution_strategy,
            supported = ?next_job_request.supported_execution_strategies,
            "builder does not support configured remote execution strategy; returning 409 Conflict"
        );
        return Ok(next_job_conflict(reason));
    }
    let evaluator_fingerprint =
        if execution_strategy == RemoteBuildExecutionStrategy::SourceReEvaluateVerified {
            let authoritative = current_evaluator_fingerprint().await.map_err(|error| {
                tracing::error!(
                    builder_id = %builder_id,
                    "failed to identify authoritative Nix evaluator: {error:#}"
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;
            // SECURITY: Capability equality is checked before queue lookup or claim.
            // A mismatch is builder-specific and must not mutate shared job state.
            if let Some(reason) = evaluator_conflict(&next_job_request, &authoritative) {
                tracing::warn!(
                    builder_id = %builder_id,
                    builder_evaluator = ?next_job_request.evaluator,
                    authoritative_evaluator = ?authoritative,
                    "builder evaluator is incompatible with verified-source work"
                );
                return Ok(next_job_conflict(reason));
            }
            Some(authoritative)
        } else {
            None
        };

    // Get builder's environment assignments (empty = wildcard)
    let environment_ids = builders::get_builder_environment_ids(&state.pool, &builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let preflight_source = if execution_strategy
        == RemoteBuildExecutionStrategy::SourceReEvaluateVerified
    {
        if let Some(reason) =
            source_delivery_conflict(execution_strategy, state.server_config.source_delivery_mode)
        {
            tracing::warn!(
                builder_id = %builder_id,
                ?state.server_config.source_delivery_mode,
                "verified-source contract version 1 requires canonical server artifact delivery"
            );
            return Ok(next_job_conflict(reason));
        }
        let (candidate, published) = loop {
            let Some(candidate) = builders::peek_next_verified_source_job(
                    &state.pool,
                    &environment_ids,
                )
                .await
                .map_err(|error| {
                    tracing::error!(builder_id = %builder_id, "failed to select source preflight candidate: {error:#}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
                else {
                    return Err(StatusCode::NOT_FOUND);
                };
            match crate::flake::verified_source::lookup_published_source(
                &state.server_config.source_archive_root,
                &candidate.repo_url,
                &candidate.commit_hash,
            )
            .await
            {
                Ok(published) => break (candidate, published),
                Err(error)
                    if matches!(
                        error.class,
                        crate::flake::verified_source::MaterializationFailureClass::NotPublished
                            | crate::flake::verified_source::MaterializationFailureClass::UnsupportedObjectFormat
                            | crate::flake::verified_source::MaterializationFailureClass::Deterministic
                    ) =>
                {
                    // Pre-contract jobs cannot satisfy the signed source and
                    // evaluator identity. Retire each stale queue head so a
                    // compatible job behind it can be selected in this poll.
                    let retired = builders::mark_queued_verified_source_job_obsolete(
                        &state.pool,
                        &candidate.job_id,
                        &format!("contract-v1 source publication is unusable: {error}"),
                    )
                    .await
                    .map_err(|transition_error| {
                        tracing::error!(
                            job_id = %candidate.job_id,
                            "failed to retire obsolete verified-source job: {transition_error:#}"
                        );
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                    tracing::warn!(
                        job_id = %candidate.job_id,
                        class = ?error.class,
                        retired,
                        "retired obsolete verified-source authority before claim"
                    );
                }
                Err(error) => {
                    let status = match error.class {
                        crate::flake::verified_source::MaterializationFailureClass::Transient => {
                            StatusCode::SERVICE_UNAVAILABLE
                        }
                        crate::flake::verified_source::MaterializationFailureClass::Cancelled => {
                            return Ok(next_job_conflict(
                                NextJobConflictReason::SourceMaterializationCancelled,
                            ));
                        }
                        crate::flake::verified_source::MaterializationFailureClass::NotPublished
                        | crate::flake::verified_source::MaterializationFailureClass::UnsupportedObjectFormat
                        | crate::flake::verified_source::MaterializationFailureClass::Deterministic => {
                            StatusCode::UNPROCESSABLE_ENTITY
                        }
                    };
                    tracing::warn!(
                        job_id = %candidate.job_id,
                        class = ?error.class,
                        "canonical source is not ready for dispatch: {error}"
                    );
                    return Err(status);
                }
            }
        };
        Some((candidate, published))
    } else {
        None
    };
    let candidate = if let Some((candidate, _)) = preflight_source.as_ref() {
        builders::get_build_job_by_id(&state.pool, &candidate.job_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        builders::peek_next_server_derivation_job(&state.pool, &environment_ids)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    }
    .ok_or(StatusCode::NOT_FOUND)?;
    let derivation =
        crate::queries::derivations::get_derivation_by_id(&state.pool, candidate.derivation_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let cache_push = builder_cache_push_config_for_derivation(&state.pool, &derivation).await?;
    if let Some(reason) = cache_type_conflict(&next_job_request, &cache_push) {
        return Ok(next_job_conflict(reason));
    }
    // CONCURRENCY: Claim only the cache-preflighted candidate and retain its
    // selected configuration. A lost candidate must not substitute a job or
    // resolve a newly selected Niks3 cache after this capability check.
    let preflight_job_id = Some(&candidate.id);

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
        preflight_job_id,
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

    // Convenience alias for the session ID used in every dispatch-failure call.
    let session_id = verified.builder_session_id.as_ref();

    // Reload build metadata after claim, retaining only the cache configuration
    // that passed capability preflight. Source authority remains current.
    let derivation =
        match crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
        {
            Ok(derivation) => derivation,
            Err(e) => {
                let status = fail_claimed_job_at_dispatch(
                    &state.pool,
                    &job.id,
                    &builder_id,
                    session_id,
                    "derivation_load",
                    DispatchFailureClass::Transient,
                    &format!("failed to load derivation {}: {e}", job.derivation_id),
                )
                .await;
                return Err(status);
            }
        };

    let mut source = match verified_source_identity_for_derivation(&state.pool, &derivation).await {
        Ok(source) => source,
        Err(e) => {
            let status = fail_claimed_job_at_dispatch(
                &state.pool,
                &job.id,
                &builder_id,
                session_id,
                "source_identity",
                DispatchFailureClass::Transient,
                &format!("failed to assemble verified source identity: {e}"),
            )
            .await;
            return Err(status);
        }
    };
    let expected_drv_path = derivation.derivation_path.clone();
    let source_input_delivery = match execution_strategy {
        RemoteBuildExecutionStrategy::ServerDerivation => SourceInputDeliveryMode::None,
        RemoteBuildExecutionStrategy::SourceReEvaluateVerified => {
            state.server_config.source_delivery_mode
        }
    };

    if execution_strategy == RemoteBuildExecutionStrategy::SourceReEvaluateVerified {
        let Some(source_mut) = source.as_mut() else {
            let status = fail_claimed_job_at_dispatch(
                &state.pool,
                &job.id,
                &builder_id,
                session_id,
                "source_materialization",
                DispatchFailureClass::Deterministic,
                "verified-source job has no source identity",
            )
            .await;
            return Err(status);
        };
        let Some((candidate, published)) = preflight_source else {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        };
        if candidate.job_id != job.id
            || candidate.commit_hash != source_mut.commit_hash
            || credential_free_repo_url(&candidate.repo_url)
                .ok()
                .as_deref()
                != Some(source_mut.repo_url.as_str())
        {
            let status = fail_claimed_job_at_dispatch(
                &state.pool,
                &job.id,
                &builder_id,
                session_id,
                "source_identity",
                DispatchFailureClass::Deterministic,
                "claimed source identity differs from its dispatch preflight",
            )
            .await;
            return Err(status);
        }
        source_mut.archive_url = Some(format!(
            "/api/v1/builders/{}/jobs/{}/source-archive",
            builder_id, job.id
        ));
        source_mut.archive_sha256 = Some(published.identity.artifact_sha256.clone());
        source_mut.lock_hash = Some(published.identity.lock_hash.clone());
        source_mut.immutable_source = Some(published.identity);
    }

    if execution_strategy == RemoteBuildExecutionStrategy::SourceReEvaluateVerified
        && (source.is_none() || expected_drv_path.is_none())
    {
        // Permanent data invariant violation: SourceReEvaluateVerified jobs
        // must have both source identity and derivation_path. Classify as
        // Deterministic so the job does not endlessly cycle through the queue.
        let status = fail_claimed_job_at_dispatch(
            &state.pool,
            &job.id,
            &builder_id,
            session_id,
            "manifest_validation",
            DispatchFailureClass::Deterministic,
            &format!(
                "SourceReEvaluateVerified job missing required metadata: \
                 has_source={} has_expected_drv_path={}",
                source.is_some(),
                expected_drv_path.is_some(),
            ),
        )
        .await;
        return Err(status);
    }

    let cache_push = Some(cache_push);
    let https_evidence =
        BuilderHttpsEvidence::from_request(&state.server_config, &headers, peer.map(|p| p.0));

    if cache_push
        .as_ref()
        .is_some_and(cache_push_config_contains_credentials)
        && !https_evidence.verified()
    {
        tracing::warn!(
            job_id = %job.id,
            derivation_id = derivation.id,
            builder_id = %builder_id,
            direct_peer_ip = ?https_evidence.direct_peer_ip,
            trust_forwarded_builder_https = https_evidence.trust_forwarded_builder_https,
            peer_cidr_match = https_evidence.peer_cidr_match,
            x_forwarded_proto_count = https_evidence.x_forwarded_proto_count,
            exact_https = https_evidence.exact_https,
            "refusing to send cache push credentials: connection is not verified HTTPS"
        );
        // This is a transient configuration mismatch (server config / TLS termination),
        // not a data problem with the job itself.
        let status = fail_claimed_job_at_dispatch(
            &state.pool,
            &job.id,
            &builder_id,
            session_id,
            "cache_config",
            DispatchFailureClass::Transient,
            "cache push credentials refused: builder connection is not verified HTTPS; configure services.crystal-forge.server.trust_forwarded_builder_https and services.crystal-forge.server.trustedProxyCidrs for the actual direct backend proxy peer, and have that proxy overwrite X-Forwarded-Proto with exactly one https value",
        )
        .await;
        return Err(status);
    }

    // SECURITY: Completion must compare with the selection actually dispatched
    // on this claim, not merely with any destination currently eligible for it.
    let job = match builders::record_job_cache_dispatch(
        &state.pool,
        &job.id,
        &builder_id,
        session_id,
        cache_push
            .as_ref()
            .and_then(|cache| cache.cache_destination_id),
    )
    .await
    {
        Ok(job) => job,
        Err(_) => {
            let status = fail_claimed_job_at_dispatch(
                &state.pool,
                &job.id,
                &builder_id,
                session_id,
                "cache_identity",
                DispatchFailureClass::Transient,
                "failed to persist selected cache destination for dispatch",
            )
            .await;
            return Err(status);
        }
    };

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
        evaluator: evaluator_fingerprint,
        cache_push,
    };

    Ok(Json(crate::models::builders::NextJobResponse {
        job: job.into(),
        derivation: payload,
    })
    .into_response())
}

/// Classification of a post-claim dispatch failure.
///
/// Used to drive the retry/backoff decision in `fail_claimed_job_at_dispatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DispatchFailureClass {
    /// Infrastructure failure that may resolve on the next poll cycle.
    /// Examples: source mirror fetch failure, temporary cache unreachable.
    /// The job is sent through `mark_job_failed_with_retry` as `Transient`
    /// so it re-enters the queue with the configured backoff instead of
    /// immediately becoming claimable again.
    Transient,
    /// The job's stored data is permanently inconsistent with the required
    /// dispatch contract. Examples: missing derivation_path for a
    /// SourceReEvaluateVerified job, missing source identity metadata.
    /// The job is marked failed with `Deterministic` class, which prevents
    /// automatic retry under `transient_only = true` policy and triggers a
    /// build-attention record.
    Deterministic,
}

/// Unified post-claim dispatch failure handler.
///
/// Replaces the old `requeue_claimed_job_after_manifest_error` which
/// immediately requeued the job with `available_at = NOW()` (no backoff,
/// no retry counting, no session guard). That caused a high-priority job
/// whose source mirror commit is missing to be re-claimed and re-failed
/// every few seconds, stalling all builders.
///
/// This helper:
/// - Calls `mark_job_failed_with_retry` with the appropriate `RetryFailureClass`.
/// - Uses the configured automatic retry policy backoff (`available_at = NOW()
///   + backoff_seconds`).
/// - Clears `builder_id` / `builder_session_id` atomically on the failed row.
/// - Opens a build-attention record when the retry budget is exhausted.
/// - Returns `StatusCode::NOT_FOUND` so the builder's polling loop treats
///   this iteration as "no work" and retries after its normal interval.
async fn fail_claimed_job_at_dispatch(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
    stage: &str,
    failure_class: DispatchFailureClass,
    message: &str,
) -> StatusCode {
    use crate::models::retry_policy::RetryFailureClass;

    let retry_class = match failure_class {
        DispatchFailureClass::Transient => RetryFailureClass::Transient,
        DispatchFailureClass::Deterministic => RetryFailureClass::Deterministic,
    };

    let full_message = format!("[dispatch:{stage}] {message}");

    match crate::queries::builders::mark_job_failed_with_retry(
        pool,
        job_id,
        builder_id,
        builder_session_id,
        Some(&full_message),
        retry_class,
    )
    .await
    {
        Ok(transition) => {
            if transition.retry_job.is_some() {
                tracing::warn!(
                    job_id = %job_id,
                    builder_id = %builder_id,
                    %stage,
                    ?failure_class,
                    "dispatch failure — job re-queued with backoff: {message}"
                );
            } else {
                tracing::error!(
                    job_id = %job_id,
                    builder_id = %builder_id,
                    %stage,
                    ?failure_class,
                    "dispatch failure — retry budget exhausted, job permanently failed: {message}"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                job_id = %job_id,
                builder_id = %builder_id,
                %stage,
                "failed to record dispatch failure via mark_job_failed_with_retry: {e:#}"
            );
        }
    }

    // Return NOT_FOUND rather than INTERNAL_SERVER_ERROR. The builder
    // treats 404 as "no work this cycle"; 500 is logged as an error by the
    // builder and may trigger escalation. The dispatch failure has already
    // been persisted server-side with full context.
    StatusCode::NOT_FOUND
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
/// Streams the canonical tracked-tree artifact used by authoritative evaluation.
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

    // SECURITY: Reject unsupported global modes before querying by job ID. The
    // route must not disclose whether a source-bearing job exists in those modes.
    if !source_archive_contract_is_authorized(
        state.server_config.remote_build_execution_strategy,
        state.server_config.source_delivery_mode,
    ) {
        return Err(StatusCode::FORBIDDEN);
    }
    let builder_session_id = verified
        .builder_session_id
        .as_ref()
        .ok_or(StatusCode::FORBIDDEN)?;
    let job = builders::get_authorized_source_archive_job(
        &state.pool,
        &job_id,
        &builder_id,
        builder_session_id,
    )
    .await
    .map_err(|e| {
        tracing::error!(job_id = %job_id, "failed to authorize source archive job: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let derivation =
        crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
            .map_err(|error| {
                tracing::error!(job_id = %job_id, "failed to load source derivation: {error:#}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if derivation.derivation_path.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let commit_id = derivation.commit_id.ok_or(StatusCode::NOT_FOUND)?;
    let commit = crate::queries::commits::get_commit_by_id(&state.pool, commit_id)
        .await
        .map_err(|error| {
            tracing::error!(job_id = %job_id, "failed to load source commit: {error:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let flake = crate::queries::flakes::get_flake_by_id(&state.pool, commit.flake_id)
        .await
        .map_err(|error| {
            tracing::error!(job_id = %job_id, "failed to load source flake: {error:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let published = crate::flake::verified_source::lookup_published_source(
        &state.server_config.source_archive_root,
        &flake.repo_url,
        &commit.git_commit_hash,
    )
    .await
    .map_err(|error| {
        tracing::warn!(job_id = %job_id, class = ?error.class, "canonical source is unavailable: {error}");
        StatusCode::NOT_FOUND
    })?;
    let archive_path = published.artifact_path;

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
        .header("Content-Type", "application/x-tar")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}.tar\"", job_id),
        );
    if let Some(size) = file_size {
        resp_builder = resp_builder.header("Content-Length", size.to_string());
    }
    resp_builder.body(Body::from_stream(stream)).map_err(|e| {
        tracing::error!(job_id = %job_id, "failed to build source archive response: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// Publishes the authorized derivation closure to the dispatched cache.
///
/// POST /api/v1/builders/:id/jobs/:job_id/publish-derivation-closure
///
/// Rechecks current eligibility without selecting an alternate cache for a
/// recorded claim. Legacy claims can use the first eligible non-Niks3 cache.
/// API builders can fetch the closure through Nix substituters or use the
/// authenticated archive endpoint when cache publication is unavailable.
///
/// # Errors
/// Returns an authentication/authorization status for invalid ownership, a
/// conflict for missing Niks3 dispatch identity or an ineligible selection,
/// `NOT_FOUND` when publication is disabled, or an error for Nix/process failure.
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

    match push_derivation_requisites_to_assigned_cache(
        &state.pool,
        &derivation,
        &archive_paths,
        &job,
    )
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

/// Reports build completion while preserving legacy reference-only clients.
#[derive(Debug, Deserialize)]
pub struct CompleteJobRequest {
    /// Identifies the enabled, derivation-eligible destination actually used.
    /// Niks3 publication requires this identity; references are insufficient.
    #[serde(default)]
    pub cache_destination_id: Option<i32>,
    /// Reports the built output path, or omits it for legacy completion.
    #[serde(default)]
    pub output_path: Option<String>,
    /// Indicates that the builder actually published the output.
    #[serde(default)]
    pub cache_pushed: bool,
    /// Provides a legacy destination name, URL, or Attic cache reference.
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
    job: &BuildJob,
    derivation: &crate::derivations::Derivation,
) -> Result<Option<crate::models::cache_destination::CacheDestination>, StatusCode> {
    if !request.cache_pushed {
        return Ok(None);
    }

    let reported = request
        .cache_reference
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if reported.is_none() && request.cache_destination_id.is_none() {
        tracing::warn!(
            job_id = %job.id,
            "builder reported cache_pushed without cache_reference"
        );
        return Err(StatusCode::CONFLICT);
    }

    let destinations = resolve_cache_destinations_for_derivation(&state.pool, derivation).await?;

    let Some(destination) = destinations.iter().find(|destination| {
        completion_matches_destination(
            request,
            destination,
            job.dispatched_cache_destination_id,
            job.cache_dispatch_recorded_at.is_some(),
        )
    }) else {
        tracing::warn!(
            job_id = %job.id,
            "builder reported cache push to a cache that does not match any active server cache destination"
        );
        return Err(StatusCode::CONFLICT);
    };

    Ok(Some(destination.clone()))
}

// SECURITY: An ID never falls back to a reference on mismatch. Niks3 requires
// an ID because its independent write and read URLs are not an identity.
fn completion_matches_destination(
    request: &CompleteJobRequest,
    destination: &CacheDestination,
    dispatched_id: Option<i32>,
    dispatch_recorded: bool,
) -> bool {
    destination.enabled
        && cache_type_from_destination(&destination.cache_type).is_ok()
        && (!dispatch_recorded || dispatched_id == Some(destination.id))
        && (destination.cache_type != "Niks3"
            || (dispatch_recorded
                && dispatched_id == Some(destination.id)
                && request.cache_destination_id == Some(destination.id)))
        && match request.cache_destination_id {
            Some(id) => id == destination.id,
            None => {
                destination.cache_type != "Niks3"
                    && request.cache_reference.as_deref().is_some_and(|reference| {
                        cache_reference_matches_destination(reference, destination)
                    })
            }
        }
}

async fn verify_store_path_available_from_cache(
    destination: &crate::models::cache_destination::CacheDestination,
    store_path: &str,
    derivation_id: i32,
    job_id: Uuid,
) -> Result<(), StatusCode> {
    let status = probe_cache_read(
        destination.clone(),
        store_path.to_owned(),
        std::ffi::OsString::from("nix"),
        // Full closure import is bounded separately from a narinfo-only probe.
        std::time::Duration::from_secs(CACHE_PUBLICATION_VERIFY_TIMEOUT_SECS),
    )
    .await?;
    if !status.success() {
        tracing::warn!(derivation_id, job_id = %job_id,
            "builder reported cache_pushed, but server publication verification failed");
        return Err(StatusCode::CONFLICT);
    }
    Ok(())
}

async fn probe_cache_read(
    destination: CacheDestination,
    store_path: String,
    program: std::ffi::OsString,
    deadline: std::time::Duration,
) -> Result<std::process::ExitStatus, StatusCode> {
    if !cf_protocol::builder::is_canonical_nix_store_path(&store_path, false) {
        return Err(StatusCode::CONFLICT);
    }
    let (url, keys, auth) = destination
        .read_config()
        .map_err(|_| StatusCode::CONFLICT)?;
    // CONCURRENCY: Dropping the HTTP future signals the process owner to kill
    // and reap before deleting read credentials or the isolated local store.
    let (cancel_guard, mut cancelled) = tokio::sync::oneshot::channel::<()>();
    let result = tokio::spawn(async move {
        let expires = tokio::time::Instant::now() + deadline;
        let prepared = if destination.cache_type == "Niks3" {
            Some(
                cf_config::cache_credentials::PreparedCacheRead::new(&url, &keys, &auth)
                    .map_err(|_| StatusCode::CONFLICT)?,
            )
        } else {
            None
        };
        let temporary_store = if prepared.is_some() {
            use std::os::unix::fs::PermissionsExt;
            Some(
                tempfile::Builder::new()
                    .prefix("cf-cache-verify-")
                    .permissions(std::fs::Permissions::from_mode(0o700))
                    .tempdir()
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            )
        } else {
            None
        };
        let outcome = async {
            let mut command = Command::new(&program);
            command.args([
                "path-info",
                "--store",
                prepared.as_ref().map_or(url.as_str(), |p| p.url.as_str()),
                &store_path,
            ]);
            if let (Some(read), Some(root)) = (&prepared, &temporary_store) {
                configure_niks3_verification_read(&mut command, read, root.path());
            } else {
                command.args(["--option", "extra-trusted-public-keys", &keys.join(" ")]);
                apply_cache_destination_env(&mut command, &destination);
            }
            let status = wait_cache_verification_command(command, expires, &mut cancelled).await?;
            if !status.success() {
                return Ok(status);
            }
            if let (Some(read), Some(root)) = (&prepared, &temporary_store) {
                // SECURITY: path-info does not validate signatures.
                // Nix 2.34.8 CmdCopy imports the closure with CheckSigs.
                // LocalStore rejects untrusted signatures and NAR hashes.
                // A fresh root prevents valid local paths from skipping
                // import. Explicit keys exclude machine trust settings.
                // Sources (Nix 2.34.8): src/nix/copy.cc,
                // src/libstore/store-api.cc, src/libstore/local-store.cc.
                let store_url = format!(
                    "local?{}",
                    url::form_urlencoded::Serializer::new(String::new())
                        .append_pair(
                            "root",
                            root.path()
                                .to_str()
                                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                        )
                        .append_pair("require-sigs", "true")
                        .finish()
                );
                let mut copy = Command::new(&program);
                copy.args(["copy", "--from", &read.url, "--to", &store_url, &store_path]);
                configure_niks3_verification_read(&mut copy, read, root.path());
                let status = wait_cache_verification_command(copy, expires, &mut cancelled).await?;
                // INVARIANT: Successful copy must materialize the requested
                // root, not only another realised path. Inspect the imported
                // object itself without following a store-path symlink.
                let relative_path = store_path.strip_prefix('/').ok_or(StatusCode::CONFLICT)?;
                if status.success()
                    && tokio::fs::symlink_metadata(root.path().join(relative_path))
                        .await
                        .is_err()
                {
                    return Err(StatusCode::CONFLICT);
                }
                return Ok(status);
            }
            Ok(status)
        }
        .await;
        drop(prepared);
        if let Some(root) = temporary_store {
            dispose_verification_store(root).await?;
        }
        outcome
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    drop(cancel_guard);
    result
}

// SECURITY: Imported store directories are read-only. After the child is
// reaped, make only this owner-only root's directories writable for deletion.
// Never follow imported symlinks, which can refer outside the isolated store.
async fn dispose_verification_store(root: tempfile::TempDir) -> Result<(), StatusCode> {
    tokio::task::spawn_blocking(move || {
        fn writable_directories(path: &std::path::Path) -> std::io::Result<()> {
            use std::os::unix::fs::PermissionsExt;
            if !std::fs::symlink_metadata(path)?.is_dir() {
                return Ok(());
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
            for entry in std::fs::read_dir(path)? {
                writable_directories(&entry?.path())?;
            }
            Ok(())
        }
        writable_directories(root.path()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        root.close().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

// SECURITY: Do not inherit ambient write auth, Nix user/system trust keys or
// narinfo metadata. A task-owned cache directory and --refresh ensure each
// verification observes the selected remote publication instead of old cache
// metadata. The isolated store retains logical /nix/store path identities.
fn configure_niks3_verification_read(
    command: &mut Command,
    read: &cf_config::cache_credentials::PreparedCacheRead,
    root: &std::path::Path,
) {
    command.env_clear();
    for key in ["PATH", "SSL_CERT_FILE", "SSL_CERT_DIR", "NIX_SSL_CERT_FILE"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("HOME", root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("NIX_CONF_DIR", root.join("config"))
        .env("NIX_USER_CONF_FILES", "/dev/null");
    command.args([
        "--extra-experimental-features",
        "nix-command",
        "--refresh",
        "--option",
        "trusted-public-keys",
        &read.trusted_public_keys,
        "--option",
        "extra-trusted-public-keys",
        "",
        "--option",
        "require-sigs",
        "true",
        "--option",
        "substituters",
        "",
        "--option",
        "narinfo-cache-positive-ttl",
        "0",
        "--option",
        "narinfo-cache-negative-ttl",
        "0",
    ]);
    if let Some(ca) = &read.ca_certificate_path {
        command.env("NIX_SSL_CERT_FILE", ca);
    }
}

async fn wait_cache_verification_command(
    mut command: Command,
    expires: tokio::time::Instant,
    cancelled: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<std::process::ExitStatus, StatusCode> {
    use crate::vulnix::process_group::{ScannerProcessGroup, isolate};
    if tokio::time::Instant::now() >= expires
        || !matches!(
            cancelled.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        )
    {
        return Err(StatusCode::CONFLICT);
    }
    command
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    isolate(&mut command);
    let child = command
        .spawn()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut child = ScannerProcessGroup::new(child, "cache publication verification")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tokio::select! {
        status = child.wait() => match status {
            Ok(status) => { child.disarm(); Ok(status) },
            Err(_) => { child.terminate().await; Err(StatusCode::INTERNAL_SERVER_ERROR) },
        },
        _ = tokio::time::sleep_until(expires) => {
            child.terminate().await;
            Err(StatusCode::CONFLICT)
        },
        _ = cancelled => {
            child.terminate().await;
            Err(StatusCode::CONFLICT)
        },
    }
}

// CONCURRENCY: Do not poll a success transition until external publication
// verification finishes. Failure or request cancellation preserves the claim's
// building state for retry/recovery. The transaction rechecks identity/output.
async fn complete_after_verification<T>(
    verification: impl std::future::Future<Output = Result<(), StatusCode>>,
    completion: impl std::future::Future<Output = Result<T, StatusCode>>,
) -> Result<T, StatusCode> {
    verification.await?;
    completion.await
}

/// POST /api/v1/builders/:id/jobs/:job_id/complete - Mark job as complete
///
/// In addition to closing the build job, the server performs the derivation
/// completion (store path + status) and queues a cache-push job. This keeps all
/// database writes server-side so API builders never need a DB connection.
/// Reported publication is verified once against authoritative output before
/// success. Niks3 requires the persisted dispatch identity and a fresh,
/// signature-verified remote closure. The success transaction rechecks output,
/// destination eligibility and publication configuration, and records publication
/// atomically. A failed probe leaves the claim recoverable without success.
/// Admission-time CVE provenance and policy-enabled automatic hardening admission
/// commit with build success. Idempotent retries retain the original admission.
/// After commit, confirmed publication releases the derivation's GC root with
/// best-effort cleanup. Failed verification retains the root for recovery.
///
/// # Errors
/// Returns an authentication/authorization status for invalid ownership, a
/// conflict for changed identity, output or unverifiable publication, or an
/// internal error for database/process failures.
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
            cache_destination_id: None,
            output_path: None,
            cache_pushed: false,
            cache_reference: None,
        }
    } else {
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?
    };

    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if job.builder_id != Some(builder_id) || job.builder_session_id != verified.builder_session_id {
        return Err(StatusCode::FORBIDDEN);
    }
    if !matches!(job.status.as_str(), "building" | "success") {
        return Err(StatusCode::CONFLICT);
    }
    let derivation =
        crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let reported_cache_destination =
        validated_reported_cache_destination(&state, &request, &job, &derivation).await?;
    let (evaluated_output, persisted_output): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT expected_store_path, store_path FROM derivations WHERE id = $1")
            .bind(job.derivation_id)
            .fetch_one(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let require_authoritative = if let Some(destination) = reported_cache_destination.as_ref() {
        destination.cache_type == "Niks3"
    } else if let Some(destination_id) = job.dispatched_cache_destination_id {
        crate::queries::cache_destinations::get_cache_destination(&state.pool, destination_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .is_some_and(|destination| destination.cache_type == "Niks3")
    } else {
        false
    };
    // COMPATIBILITY: Older caches can report a canonical builder output when
    // evaluation did not record one. Existing server authority still wins;
    // Niks3 always requires it. Request-less legacy publication uses known data.
    let requested_output = request.output_path.as_deref().or_else(|| {
        reported_cache_destination
            .as_ref()
            .and(evaluated_output.as_deref().or(persisted_output.as_deref()))
    });
    let output = builders::validated_completion_output(
        &job.status,
        requested_output,
        evaluated_output.as_deref(),
        persisted_output.as_deref(),
        require_authoritative,
    )
    .map_err(|_| StatusCode::CONFLICT)?;
    let publication = if let Some(destination) = reported_cache_destination.as_ref() {
        Some(
            builders::capture_cache_publication_configuration(
                &state.pool,
                job.derivation_id,
                destination,
            )
            .await
            .map_err(|_| StatusCode::CONFLICT)?,
        )
    } else {
        None
    };
    let verification = async {
        if let Some(destination) = reported_cache_destination.as_ref() {
            verify_store_path_available_from_cache(
                destination,
                output.as_deref().ok_or(StatusCode::CONFLICT)?,
                job.derivation_id,
                job_id,
            )
            .await?;
        }
        Ok(())
    };

    // Perform atomic completion (job + derivation update in one transaction).
    // Idempotent: if the job is already 'success' with matching builder+session,
    // this is a safe no-op. The returned bool indicates whether this was a new
    // completion (true) or an idempotent retry (false).
    // The completion policy carries deployment configuration into the
    // transaction after publication verification. Automatic hardening admission
    // uses the handler's configuration, not a query-layer configuration read.
    let completion = async {
        builders::complete_preverified_job_atomic_with_policy(
            &state.pool,
            &job_id,
            &builder_id,
            verified.builder_session_id.as_ref(),
            output.as_deref(),
            publication.as_ref(),
            builders::BuildCompletionPolicy {
                auto_hardening_scans: state.server_config.auto_hardening_scans,
            },
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
        })
    };
    let (completed_job, is_new) = complete_after_verification(verification, completion).await?;

    if publication.is_some() {
        // INVARIANT: Retain the recovery root until verified publication and
        // success commit together. Cleanup remains best-effort and retryable.
        if let Err(error) = crate::builder::remove_gc_root(completed_job.derivation_id).await {
            tracing::warn!(derivation_id = completed_job.derivation_id, %error,
                "failed to remove GC root after verified builder publication");
        }
    }

    if !request.cache_pushed
        && is_new
        && job.dispatched_cache_destination_id.is_none()
        && output.is_some()
    {
        // COMPATIBILITY: Only unbound legacy/static work needs post-commit
        // resolution. Recorded IDs were queued atomically above. The shared
        // helper requires unambiguous eligibility and never chooses a first row.
        let database_present: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM cache_destinations)")
                .fetch_one(&state.pool)
                .await
                .unwrap_or(true);
        if job.cache_dispatch_recorded_at.is_none() || !database_present {
            let static_config = crate::config::CrystalForgeConfig::load()
                .map(|config| config.get_cache_config().clone())
                .unwrap_or_default();
            if crate::queries::cache_push::enqueue_cache_push_for_derivation(
                &state.pool,
                completed_job.derivation_id,
                &static_config,
            )
            .await
            .is_err()
            {
                tracing::warn!(derivation_id = completed_job.derivation_id, job_id = %job_id,
                    "failed to queue unbound legacy publication with canonical destination policy");
            }
        }
    }

    cleanup_build_log_channel(&state, job_id).await;

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

    if request.failure_phase.as_deref() == Some("evaluator_incompatible")
        && state.server_config.remote_build_execution_strategy
            == RemoteBuildExecutionStrategy::SourceReEvaluateVerified
    {
        // SECURITY: Only a verified-source claim can encounter an evaluator
        // mismatch. Do not let a builder bypass retry accounting for another
        // execution strategy by selecting this failure-phase string.
        builders::release_job_for_incompatible_evaluator(
            &state.pool,
            &job_id,
            &builder_id,
            verified.builder_session_id.as_ref(),
            request.error_message.as_deref(),
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                builder_id = %builder_id,
                job_id = %job_id,
                "rejected evaluator-incompatible release: {error:#}"
            );
            StatusCode::CONFLICT
        })?;
        cleanup_build_log_channel(&state, job_id).await;
        return Ok(StatusCode::OK);
    }

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
#[path = "builders_proxy_dispatch_tests.rs"]
mod proxy_dispatch_tests;

#[cfg(test)]
mod tests {
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn niks3_builder_and_agent_selection_share_assigned_first_policy(pool: sqlx::PgPool) {
        let environment: uuid::Uuid = sqlx::query_scalar("INSERT INTO environments (name, description, is_active) VALUES ('builder-selection', 'test', TRUE) RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let repo = "https://example.com/cache-selection.git";
        let hash = "2".repeat(40);
        crate::queries::flakes::insert_flake(&pool, "cache-selection", repo, "main", "all_configs")
            .await
            .unwrap();
        crate::queries::commits::insert_commit_with_metadata(
            &pool,
            &hash,
            repo,
            chrono::Utc::now(),
            Some("selection fixture"),
            Some("test"),
        )
        .await
        .unwrap();
        let commit = crate::queries::commits::get_commit_by_hash(&pool, &hash)
            .await
            .unwrap();
        sqlx::query("INSERT INTO systems (hostname, system_configuration_name, environment_id, public_key, derivation, flake_id) VALUES ('selection-host', 'selection-config', $1, $2, '/nix/store/current', $3)")
            .bind(environment).bind("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=").bind(commit.flake_id).execute(&pool).await.unwrap();
        let selected: i32 = sqlx::query_scalar("INSERT INTO cache_destinations (name, cache_type, enabled, push_to, niks3_server_url, niks3_write_auth_mode, niks3_auth_token, niks3_public_keys, niks3_read_auth_mode) VALUES ('z-assigned-niks3', 'Niks3', TRUE, 'https://selected.example', 'https://write.example', 'token', 'write-token', ARRAY['cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='], 'none') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)")
            .bind(selected).bind(environment).execute(&pool).await.unwrap();
        let global: i32 = sqlx::query_scalar("INSERT INTO cache_destinations (name, cache_type, enabled, push_to) VALUES ('a-global', 'Nix', TRUE, 'https://fallback.example') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let derivation = crate::test_utils::builders::DerivationBuilder::new()
            .commit_id(Some(commit.id))
            .name("selection-config")
            .build();
        for expected in [selected, global] {
            let builder = super::resolve_cache_destinations_for_derivation(&pool, &derivation)
                .await
                .unwrap();
            let agent =
                crate::queries::cache_destinations::eligible_cache_destinations_for_environment(
                    &pool,
                    Some(environment),
                )
                .await
                .unwrap();
            assert_eq!(
                builder.iter().map(|d| d.id).collect::<Vec<_>>(),
                vec![expected]
            );
            assert_eq!(
                agent.iter().map(|d| d.id).collect::<Vec<_>>(),
                vec![expected]
            );
            sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
                .bind(selected)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
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
    use super::evaluator_conflict;
    use super::execution_strategy_conflict;
    use super::fallback_job_status_request_for_invalid_details;
    use super::format_failure_message;
    use super::map_create_builder_error;
    use super::next_job_request_for_method;
    use super::parse_derivation_requisites;
    use super::parse_job_status_request;
    use super::parse_next_job_request;
    use super::persisted_build_log_frames;
    use super::retry_failure_class;
    use super::source_delivery_conflict;
    use super::source_flake_target_for_derivation;
    use super::verify_builder_resolve_request;
    use crate::builder::api_client::BuilderApiClient;
    use crate::derivations::{Derivation, DerivationType};
    use crate::models::builders::{
        Builder, BuilderStatus, NextJobConflictReason, NextJobConflictResponse, NextJobRequest,
        RemoteBuildExecutionStrategy, ResolveBuilderIdRequest, SourceInputDeliveryMode,
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
            capabilities: Default::default(),
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
        assert!(request.evaluator.is_none());
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
        assert!(request.evaluator.is_none());
    }

    #[test]
    fn unsupported_execution_strategy_has_discriminating_preclaim_reason() {
        let request = NextJobRequest {
            capabilities: Default::default(),
            protocol_version: 2,
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
            supported_evaluator_contract_versions: Vec::new(),
            evaluator: None,
        };

        assert_eq!(
            execution_strategy_conflict(&request, RemoteBuildExecutionStrategy::ServerDerivation,),
            Some(NextJobConflictReason::UnsupportedExecutionStrategy)
        );
    }

    #[tokio::test]
    async fn niks3_preclaim_capability_gate_preserves_legacy_cache_dispatch() {
        use cf_protocol::cache::CacheType;
        let mut request = super::legacy_next_job_request();
        let mut cache = super::BuilderCachePushConfig::disabled();
        for cache_type in [
            CacheType::Nix,
            CacheType::Attic,
            CacheType::S3,
            CacheType::Http,
        ] {
            cache.cache_type = cache_type;
            assert_eq!(super::cache_type_conflict(&request, &cache), None);
        }
        cache.cache_type = CacheType::Niks3;
        // Even a no-push config contains an enum older builders cannot decode.
        assert!(!cache.push_after_build);
        let reason = super::cache_type_conflict(&request, &cache).unwrap();
        assert_eq!(reason, NextJobConflictReason::UnsupportedCacheType);
        let response = super::next_job_conflict(reason);
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"reason":"unsupported_cache_type"})
        );
        request.capabilities.niks3_cache = true;
        assert!(!request.capabilities.supports_current_cve_schema());
        assert_eq!(super::cache_type_conflict(&request, &cache), None);
    }

    #[tokio::test]
    async fn next_job_conflict_response_contains_machine_readable_reason() {
        let response =
            super::next_job_conflict(NextJobConflictReason::UnsupportedExecutionStrategy);
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("conflict response body should be readable");
        let conflict: NextJobConflictResponse =
            serde_json::from_slice(&body).expect("conflict response should be JSON");

        assert_eq!(
            conflict.reason,
            NextJobConflictReason::UnsupportedExecutionStrategy
        );
    }

    #[test]
    fn evaluator_compatibility_has_discriminating_preclaim_reason() {
        let authoritative = super::EvaluatorFingerprint {
            contract_version: super::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
            nix_version: "2.34.5".to_string(),
            evaluator_system: "x86_64-linux".to_string(),
            pure_eval: true,
            lockfile_mutation_allowed: false,
            allow_import_from_derivation: true,
            source_materialization_schema_version:
                super::VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
        };
        let mut request = NextJobRequest {
            capabilities: Default::default(),
            protocol_version: 2,
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
            supported_evaluator_contract_versions: vec![authoritative.contract_version],
            evaluator: Some(authoritative.clone()),
        };

        assert_eq!(evaluator_conflict(&request, &authoritative), None);

        request.evaluator = Some(super::EvaluatorFingerprint {
            nix_version: "2.33.0".to_string(),
            ..authoritative.clone()
        });

        assert_eq!(
            evaluator_conflict(&request, &authoritative),
            Some(NextJobConflictReason::IncompatibleEvaluator)
        );
    }

    #[test]
    fn incompatible_source_delivery_has_discriminating_preclaim_reason() {
        assert_eq!(
            source_delivery_conflict(
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
                SourceInputDeliveryMode::LocalGitWorktree,
            ),
            Some(NextJobConflictReason::IncompatibleSourceDelivery)
        );
        assert_eq!(
            source_delivery_conflict(
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
                SourceInputDeliveryMode::ServerBundledArchive,
            ),
            None
        );
    }

    #[test]
    fn next_job_body_accepts_explicit_verified_source_capability() {
        let body = serde_json::to_vec(&NextJobRequest {
            capabilities: Default::default(),
            protocol_version: 2,
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::ServerDerivation,
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
            supported_evaluator_contract_versions: vec![
                super::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
            ],
            evaluator: Some(super::EvaluatorFingerprint {
                contract_version: super::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
                nix_version: "2.34.5".to_string(),
                evaluator_system: "x86_64-linux".to_string(),
                pure_eval: true,
                lockfile_mutation_allowed: false,
                allow_import_from_derivation: true,
                source_materialization_schema_version:
                    super::VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
            }),
        })
        .expect("request should serialize");

        let request = parse_next_job_request(&body).expect("request should parse");

        assert_eq!(request.protocol_version, 2);
        assert!(
            request
                .supported_execution_strategies
                .contains(&RemoteBuildExecutionStrategy::SourceReEvaluateVerified)
        );
        assert!(request.evaluator.is_some());
    }

    #[test]
    fn verified_source_preclaim_requires_exact_evaluator_capability() {
        let authoritative = super::EvaluatorFingerprint {
            contract_version: super::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION,
            nix_version: "2.34.5".to_string(),
            evaluator_system: "x86_64-linux".to_string(),
            pure_eval: true,
            lockfile_mutation_allowed: false,
            allow_import_from_derivation: true,
            source_materialization_schema_version:
                super::VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION,
        };
        let mut request = NextJobRequest {
            capabilities: Default::default(),
            protocol_version: 2,
            supported_execution_strategies: vec![
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            ],
            supported_evaluator_contract_versions: vec![authoritative.contract_version],
            evaluator: Some(authoritative.clone()),
        };
        assert!(super::verified_source_evaluator_is_compatible(
            &request,
            &authoritative
        ));

        for mismatch in [
            ("nix_version", "2.34.4"),
            ("evaluator_system", "aarch64-linux"),
        ] {
            let mut candidate = authoritative.clone();
            match mismatch.0 {
                "nix_version" => candidate.nix_version = mismatch.1.to_string(),
                "evaluator_system" => candidate.evaluator_system = mismatch.1.to_string(),
                _ => unreachable!(),
            }
            request.evaluator = Some(candidate);
            assert!(!super::verified_source_evaluator_is_compatible(
                &request,
                &authoritative
            ));
        }

        for candidate in [
            super::EvaluatorFingerprint {
                contract_version: 0,
                ..authoritative.clone()
            },
            super::EvaluatorFingerprint {
                pure_eval: false,
                ..authoritative.clone()
            },
            super::EvaluatorFingerprint {
                lockfile_mutation_allowed: true,
                ..authoritative.clone()
            },
            super::EvaluatorFingerprint {
                allow_import_from_derivation: false,
                ..authoritative.clone()
            },
            super::EvaluatorFingerprint {
                source_materialization_schema_version: 0,
                ..authoritative.clone()
            },
        ] {
            request.evaluator = Some(candidate);
            assert!(!super::verified_source_evaluator_is_compatible(
                &request,
                &authoritative
            ));
        }

        request.evaluator = None;
        assert!(!super::verified_source_evaluator_is_compatible(
            &request,
            &authoritative
        ));
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
            capabilities: Default::default(),
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
        cfg.trusted_proxy_cidrs = vec!["127.0.0.1/32".into()];
        cfg
    }

    #[test]
    fn credential_check_blocked_when_flag_false_even_with_https_header() {
        let cfg = server_config_with_trust(false);
        let headers = make_headers_with("x-forwarded-proto", "https");
        assert!(
            !builder_https_verified_by_trusted_proxy(
                &cfg,
                &headers,
                Some("127.0.0.1:443".parse().unwrap())
            ),
            "must not trust forwarded headers when flag is off"
        );
    }

    #[test]
    fn credential_check_blocked_when_flag_true_but_no_header() {
        let cfg = server_config_with_trust(true);
        let headers = HeaderMap::new();
        assert!(
            !builder_https_verified_by_trusted_proxy(
                &cfg,
                &headers,
                Some("127.0.0.1:443".parse().unwrap())
            ),
            "must not pass when flag is on but no forwarded-proto header present"
        );
    }

    #[test]
    fn credential_check_blocked_when_flag_true_but_header_says_http() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("x-forwarded-proto", "http");
        assert!(
            !builder_https_verified_by_trusted_proxy(
                &cfg,
                &headers,
                Some("127.0.0.1:443".parse().unwrap())
            ),
            "must not pass when forwarded-proto says http"
        );
    }

    #[test]
    fn credential_check_passes_when_flag_true_and_x_forwarded_proto_https() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("x-forwarded-proto", "https");
        assert!(
            builder_https_verified_by_trusted_proxy(
                &cfg,
                &headers,
                Some("127.0.0.1:443".parse().unwrap())
            ),
            "must pass when flag is on and x-forwarded-proto asserts https"
        );
    }

    #[test]
    fn credential_check_rejects_unsupported_forwarded_proto() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("forwarded", "for=1.2.3.4;proto=https");
        assert!(
            !builder_https_verified_by_trusted_proxy(
                &cfg,
                &headers,
                Some("127.0.0.1:443".parse().unwrap())
            ),
            "only the proxy-overwritten x-forwarded-proto header is supported"
        );
    }

    #[test]
    fn credential_check_rejects_unsupported_forwarded_ssl() {
        let cfg = server_config_with_trust(true);
        let headers = make_headers_with("x-forwarded-ssl", "on");
        assert!(
            !builder_https_verified_by_trusted_proxy(
                &cfg,
                &headers,
                Some("127.0.0.1:443".parse().unwrap())
            ),
            "only the proxy-overwritten x-forwarded-proto header is supported"
        );
    }

    #[test]
    fn niks3_confidential_transport_rejects_spoofing_and_ambiguous_headers() {
        let cfg = server_config_with_trust(true);
        let mut headers = make_headers_with("x-forwarded-proto", "https");
        assert!(!builder_https_verified_by_trusted_proxy(
            &cfg, &headers, None
        ));
        assert!(!builder_https_verified_by_trusted_proxy(
            &cfg,
            &headers,
            Some("192.0.2.1:443".parse().unwrap())
        ));
        let peer = Some("127.0.0.1:443".parse().unwrap());
        headers.append("x-forwarded-proto", "https".parse().unwrap());
        assert!(!builder_https_verified_by_trusted_proxy(
            &cfg, &headers, peer
        ));
        headers.insert("x-forwarded-proto", "https,http".parse().unwrap());
        assert!(!builder_https_verified_by_trusted_proxy(
            &cfg, &headers, peer
        ));
        let mut cfg = cfg;
        cfg.trusted_proxy_cidrs.clear();
        assert!(!builder_https_verified_by_trusted_proxy(
            &cfg,
            &make_headers_with("x-forwarded-proto", "https"),
            peer
        ));
    }

    #[test]
    fn attic_requisite_env_uses_shared_server_base() {
        use crate::models::cache_destination::CacheDestination;
        use std::ffi::OsStr;

        for input in [
            "https://cache.example/proxy/",
            "https://cache.example/proxy/campground",
            "https://cache.example/proxy/campground/nix-cache-info",
        ] {
            let destination = CacheDestination {
                cache_type: "Attic".into(),
                push_to: Some(input.into()),
                attic_cache_name: Some("local:campground".into()),
                ..Default::default()
            };
            let mut command = tokio::process::Command::new("attic");
            super::apply_cache_destination_env(&mut command, &destination);
            let endpoint = command
                .as_std()
                .get_envs()
                .find(|(key, _)| *key == OsStr::new("ATTIC_SERVER_URL"))
                .and_then(|(_, value)| value);
            assert_eq!(endpoint, Some(OsStr::new("https://cache.example/proxy/")));
        }
        for (kind, input) in [
            ("Nix", "https://cache.example"),
            ("Attic", "https://cache.example/#fragment"),
        ] {
            let destination = CacheDestination {
                cache_type: kind.into(),
                push_to: Some(input.into()),
                attic_cache_name: Some("campground".into()),
                ..Default::default()
            };
            let mut command = tokio::process::Command::new("attic");
            super::apply_cache_destination_env(&mut command, &destination);
            assert!(
                !command
                    .as_std()
                    .get_envs()
                    .any(|(key, _)| key == OsStr::new("ATTIC_SERVER_URL"))
            );
        }
    }

    #[test]
    fn niks3_builder_config_excludes_read_and_aws_credentials_and_requires_identity() {
        use crate::models::cache_destination::CacheDestination;
        let mut destination = CacheDestination {
            id: 42,
            enabled: true,
            name: "niks3".into(),
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example".into()),
            niks3_server_url: Some("https://write.example".into()),
            niks3_public_keys: vec![
                crate::models::cache_destination::nix_public_key_fixture("one"),
                crate::models::cache_destination::nix_public_key_fixture("two"),
            ],
            parallel_uploads: Some(7),
            attic_jobs: Some(91),
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("write-token".into()),
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("read-key".into()),
            s3_access_key_id: Some("aws-id".into()),
            s3_secret_access_key: Some("aws-secret".into()),
            ..Default::default()
        };
        let config = super::builder_cache_push_config_from_destination(&destination).unwrap();
        assert_eq!(config.cache_destination_id, Some(42));
        assert_eq!(config.parallel_uploads, Some(7));
        assert_ne!(config.attic_jobs, 91);
        assert!(super::cache_push_config_contains_credentials(&config));
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("write-token"));
        for secret in ["BEGIN CERTIFICATE", "read-key", "aws-id", "aws-secret"] {
            assert!(!json.contains(secret));
        }
        let mut request: super::CompleteJobRequest = serde_json::from_value(serde_json::json!({
            "cache_pushed":true, "cache_reference":"niks3"
        }))
        .unwrap();
        assert!(!super::completion_matches_destination(
            &request,
            &destination,
            Some(42),
            true
        ));
        request.cache_destination_id = Some(42);
        assert!(!super::completion_matches_destination(
            &request,
            &destination,
            Some(43),
            true
        ));
        assert!(!super::completion_matches_destination(
            &request,
            &destination,
            None,
            false
        ));
        assert!(super::completion_matches_destination(
            &request,
            &destination,
            Some(42),
            true
        ));
        request.cache_destination_id = Some(43);
        assert!(!super::completion_matches_destination(
            &request,
            &destination,
            Some(42),
            true
        ));
        destination.enabled = false;
        request.cache_destination_id = Some(42);
        assert!(!super::completion_matches_destination(
            &request,
            &destination,
            Some(42),
            true
        ));
        destination.enabled = true;
        destination.niks3_write_auth_mode = Some("mtls".into());
        destination.niks3_auth_token = None;
        destination.niks3_write_client_cert =
            Some(crate::security::cache_secrets::TEST_CERTIFICATE.into());
        destination.niks3_write_client_key = Some("write-key".into());
        let config = super::builder_cache_push_config_from_destination(&destination).unwrap();
        assert!(super::cache_push_config_contains_credentials(&config));
        destination.cache_type = "Nix".into();
        request.cache_destination_id = None;
        assert!(super::completion_matches_destination(
            &request,
            &destination,
            None,
            false
        ));
        destination.cache_type = "unknown".into();
        assert!(super::builder_cache_push_config_from_destination(&destination).is_err());
    }

    #[tokio::test]
    async fn niks3_completion_probe_uses_independent_read_plane() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("nix");
        std::fs::write(
            dir.path().join("expected-ca"),
            crate::security::cache_secrets::TEST_CERTIFICATE,
        )
        .unwrap();
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
set -eu
case "$1" in
path-info) test "$2" = --store; shift 4 ;;
copy)
    test "$2" = --from
    case "$3" in https://read.example/*tls-certificate=*tls-private-key=*) ;; *) exit 11;; esac
    test "$4" = --to
    case "$5" in 'local?root='*'&require-sigs=true') ;; *) exit 12;; esac
    test ! -e "$HOME/nix/var/nix/db/db.sqlite"
    mkdir -p "$HOME/nix/store"
    touch "$HOME/${{6#/}}"
    shift 6 ;;
*) exit 13 ;;
esac
refresh=false; keys=false; signatures=false
while test "$#" -gt 0; do
    case "$1" in
    --refresh) refresh=true; shift ;;
    --extra-experimental-features) test "$2" = nix-command; shift 2 ;;
    --option)
        case "$2" in
        trusted-public-keys) test "$3" = '{}'; keys=true ;;
        require-sigs) test "$3" = true; signatures=true ;;
        extra-trusted-public-keys|substituters) test -z "$3" ;;
        narinfo-cache-positive-ttl|narinfo-cache-negative-ttl) test "$3" = 0 ;;
        *) exit 14 ;;
        esac
        shift 3 ;;
    *) exit 15 ;;
    esac
done
test "$refresh" = true; test "$keys" = true; test "$signatures" = true
test "$(cat "$NIX_SSL_CERT_FILE")" = "$(cat '{}')"
test -z "${{AWS_SECRET_ACCESS_KEY:-}}"
test -z "${{NIKS3_AUTH_TOKEN_FILE:-}}"
printf '%s' "${{NIX_SSL_CERT_FILE%/*}}" > '{}'
"#,
                [
                    crate::models::cache_destination::nix_public_key_fixture("one"),
                    crate::models::cache_destination::nix_public_key_fixture("two")
                ]
                .join(" "),
                dir.path().join("expected-ca").display(),
                dir.path().join("credentials").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let destination = crate::models::cache_destination::CacheDestination {
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example".into()),
            niks3_server_url: Some("https://write.example".into()),
            niks3_public_keys: vec![
                crate::models::cache_destination::nix_public_key_fixture("one"),
                crate::models::cache_destination::nix_public_key_fixture("two"),
            ],
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("read-key".into()),
            niks3_read_ca_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            niks3_auth_token: Some("write-token".into()),
            s3_secret_access_key: Some("aws-secret".into()),
            ..Default::default()
        };
        assert!(
            super::probe_cache_read(
                destination,
                "/nix/store/output".into(),
                program.into_os_string(),
                std::time::Duration::from_secs(5)
            )
            .await
            .unwrap()
            .success()
        );
        let directory = std::fs::read_to_string(dir.path().join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[tokio::test]
    async fn niks3_probe_failure_never_polls_success_transition() {
        use std::os::unix::fs::PermissionsExt;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("nix");
        for failure in ["missing", "forged", "missing-output"] {
            std::fs::write(
                &program,
                format!(
                    r#"#!/bin/sh
set -eu
if test "$1" = path-info; then exit 0; fi
test "$1" = copy
test "$4" = --to
case "$5" in 'local?root='*'&require-sigs=true') ;; *) exit 10;; esac
test ! -e "$HOME/nix/var/nix/db/db.sqlite"
printf '%s' '{}' > '{}'
printf 'private diagnostic: untrusted {} signature' >&2
if test '{}' = missing-output; then exit 0; fi
exit 17
"#,
                    failure,
                    dir.path().join("copy-failure").display(),
                    failure,
                    failure
                ),
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            let destination = crate::models::cache_destination::CacheDestination {
                cache_type: "Niks3".into(),
                push_to: Some("https://read.example".into()),
                niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
                    "one",
                )],
                niks3_read_auth_mode: Some("none".into()),
                ..Default::default()
            };
            let transitions = AtomicUsize::new(0);
            let result = super::complete_after_verification(
                async {
                    let status = super::probe_cache_read(
                        destination,
                        "/nix/store/output".into(),
                        program.clone().into_os_string(),
                        std::time::Duration::from_secs(5),
                    )
                    .await?;
                    if status.success() {
                        Ok(())
                    } else {
                        Err(StatusCode::CONFLICT)
                    }
                },
                async {
                    transitions.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await;
            assert_eq!(result, Err(StatusCode::CONFLICT));
            assert_eq!(transitions.load(Ordering::SeqCst), 0);
            assert_eq!(
                std::fs::read_to_string(dir.path().join("copy-failure")).unwrap(),
                failure
            );
        }
        let events = std::sync::Mutex::new(Vec::new());
        super::complete_after_verification(
            async {
                events.lock().unwrap().push("probe");
                Ok(())
            },
            async {
                events.lock().unwrap().push("success");
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(*events.lock().unwrap(), ["probe", "success"]);
    }

    #[tokio::test]
    async fn niks3_probe_timeout_and_cancellation_reap_before_cleanup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("nix");
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
set -eu
printf '%s' "$HOME" > '{}'
printf '%s' "$NIX_SSL_CERT_FILE" > '{}'
exec sleep 30
"#,
                dir.path().join("root").display(),
                dir.path().join("ca").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let destination = crate::models::cache_destination::CacheDestination {
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example".into()),
            niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
                "one",
            )],
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("read-key".into()),
            niks3_read_ca_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            ..Default::default()
        };
        assert_eq!(
            super::probe_cache_read(
                destination.clone(),
                "/nix/store/output".into(),
                program.clone().into_os_string(),
                std::time::Duration::from_millis(200)
            )
            .await
            .unwrap_err(),
            StatusCode::CONFLICT
        );
        let root = std::fs::read_to_string(dir.path().join("root")).unwrap();
        let ca = std::fs::read_to_string(dir.path().join("ca")).unwrap();
        assert!(!std::path::Path::new(&root).exists());
        assert!(!std::path::Path::new(&ca).exists());
        std::fs::remove_file(dir.path().join("root")).unwrap();
        std::fs::remove_file(dir.path().join("ca")).unwrap();
        let operation = tokio::spawn(super::probe_cache_read(
            destination,
            "/nix/store/output".into(),
            program.into_os_string(),
            std::time::Duration::from_secs(5),
        ));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !dir.path().join("ca").exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let root = std::fs::read_to_string(dir.path().join("root")).unwrap();
        let ca = std::fs::read_to_string(dir.path().join("ca")).unwrap();
        operation.abort();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while std::path::Path::new(&root).exists() || std::path::Path::new(&ca).exists() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn niks3_verification_store_cleanup_preserves_external_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let external = tempfile::tempdir().unwrap();
        std::fs::write(external.path().join("keep"), b"external").unwrap();
        std::fs::set_permissions(external.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_owned();
        let immutable = root.path().join("nix/store/output");
        std::fs::create_dir_all(&immutable).unwrap();
        std::fs::write(immutable.join("file"), b"copied").unwrap();
        std::os::unix::fs::symlink(external.path(), immutable.join("outside")).unwrap();
        std::fs::set_permissions(&immutable, std::fs::Permissions::from_mode(0o555)).unwrap();
        super::dispose_verification_store(root).await.unwrap();
        assert!(!root_path.exists());
        assert_eq!(
            std::fs::metadata(external.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o500
        );
        assert!(external.path().join("keep").exists());
        std::fs::set_permissions(external.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[tokio::test]
    async fn niks3_pinned_nix_accepts_isolated_store_and_refresh_flags() {
        use super::Command;
        let version = Command::new("nix").arg("--version").output().await.unwrap();
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8_lossy(&version.stdout).trim(),
            "nix (Nix) 2.34.8"
        );
        let root = tempfile::tempdir().unwrap();
        let store_url = format!(
            "local?{}",
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("root", root.path().to_str().unwrap())
                .append_pair("require-sigs", "true")
                .finish()
        );
        let read = cf_config::cache_credentials::PreparedCacheRead::new(
            "https://read.example",
            &["cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()],
            &cf_protocol::cache::CacheReadAuth::None,
        )
        .unwrap();
        let mut ping = Command::new("nix");
        ping.args(["store", "ping", "--store", &store_url]);
        super::configure_niks3_verification_read(&mut ping, &read, root.path());
        let output = ping.output().await.unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(root.path().join("nix/var/nix/db/db.sqlite").exists());
        let source = tempfile::tempdir().unwrap();
        let mut copy = Command::new("nix");
        copy.args([
            "copy",
            "--from",
            &format!("file://{}", source.path().display()),
            "--to",
            &store_url,
            "/nix/store/00000000000000000000000000000000-verification-missing",
        ]);
        super::configure_niks3_verification_read(&mut copy, &read, root.path());
        let output = copy.output().await.unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("unrecognised flag") && !stderr.contains("unknown setting"),
            "{stderr}"
        );
        assert!(
            stderr.contains("is not valid")
                || stderr.contains("does not exist")
                || stderr.contains("not found")
                || stderr.contains("no substituter"),
            "{stderr}"
        );
        super::dispose_verification_store(root).await.unwrap();
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
    fn source_archive_route_accepts_only_contract_v1_mode() {
        for rejected in [
            SourceInputDeliveryMode::None,
            SourceInputDeliveryMode::LocalGitWorktree,
            SourceInputDeliveryMode::BuilderFetchPublicInputs,
        ] {
            assert!(!super::source_archive_contract_is_authorized(
                RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
                rejected,
            ));
        }
        assert!(!super::source_archive_contract_is_authorized(
            RemoteBuildExecutionStrategy::ServerDerivation,
            SourceInputDeliveryMode::ServerBundledArchive,
        ));
        assert!(super::source_archive_contract_is_authorized(
            RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            SourceInputDeliveryMode::ServerBundledArchive,
        ));
    }

    #[test]
    fn verified_source_manifest_removes_repository_url_credentials() {
        let sanitized = super::credential_free_repo_url(
            "https://deploy-token:secret@example.com/team/repo.git?access_token=secret#fragment",
        )
        .expect("credential URL should be sanitizable");

        assert_eq!(sanitized, "https://example.com/team/repo.git");
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("deploy-token"));

        let scp_style = super::credential_free_repo_url(
            "deploy-token@example.com:team/repo.git?access_token=secret#fragment",
        )
        .expect("SCP-style repository URL should be sanitizable");
        assert_eq!(scp_style, "example.com:team/repo.git");
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

    #[tokio::test]
    async fn evaluator_fingerprint_reports_executing_nix_and_pure_contract() {
        let tempdir = tempfile::tempdir().expect("Nix probe test tempdir should create");
        let nix = tempdir.path().join("nix");
        std::fs::write(&nix, "#!/bin/sh\nprintf 'nix (Nix) 2.34.5\\n'\n")
            .expect("fake Nix executable should write");
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&nix)
            .expect("fake Nix executable should stat")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&nix, permissions)
            .expect("fake Nix executable should be executable");

        let nix_version = super::probe_nix_version(&nix)
            .await
            .expect("controlled Nix version should be probeable");
        let identity = super::parse_nix_eval_jobs_identity(
            r#"{"attr":"probe","extraValue":{"nixVersion":"2.34.5","evaluatorSystem":"x86_64-linux"}}"#,
        )
        .expect("controlled evaluator output should contain its identity");
        let fingerprint = super::evaluator_fingerprint(&nix_version, identity)
            .expect("matching Nix identities should produce a fingerprint");

        assert_eq!(fingerprint.nix_version, "2.34.5");
        assert_eq!(fingerprint.evaluator_system, "x86_64-linux");
        assert!(fingerprint.pure_eval);
        assert!(!fingerprint.lockfile_mutation_allowed);
        assert!(fingerprint.allow_import_from_derivation);
        assert_eq!(
            fingerprint.contract_version,
            super::VERIFIED_SOURCE_EVALUATOR_CONTRACT_VERSION
        );
        assert_eq!(
            fingerprint.source_materialization_schema_version,
            super::VERIFIED_SOURCE_MATERIALIZATION_SCHEMA_VERSION
        );

        let mismatched_identity = super::NixEvaluatorIdentity {
            nix_version: "2.34.4".to_string(),
            evaluator_system: "x86_64-linux".to_string(),
        };
        assert!(super::evaluator_fingerprint(&nix_version, mismatched_identity).is_err());
    }
}
