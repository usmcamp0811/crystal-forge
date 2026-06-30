//! API handlers for builder management and work queue operations.
//!
//! This module provides two sets of endpoints:
//! 1. Builder Management (Admin-only): CRUD operations for builders
//! 2. Builder Work Queue (Builder-authenticated): Job polling and status updates

use axum::{
    body::Body,
    Json,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose};
use bytes::Bytes;
use ed25519_dalek::{Signature, Verifier};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tokio::process::Command;

use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{
    require_admin, require_operator_or_admin, require_viewer_or_above,
};
use crate::handlers::builder_request::{
    authenticate_builder_request, authenticate_builder_request_allow_inactive,
};
use crate::models::builders::{
    AppendLogsRequest, BuildJob, Builder, BuilderCreatedResponse, BuilderMetrics, BuilderSummary,
    BuilderWithEnvironments, CreateBuilderRequest, KeypairRegeneratedResponse,
    ReportMetricsRequest, ResolveBuilderIdRequest, ResolveBuilderIdResponse,
    UpdateBuilderEnvironmentsRequest, UpdateBuilderPublicKeyRequest, UpdateBuilderRequest,
};
use crate::models::public_key::PublicKey;
use crate::queries::builders;

const NIX_STORE_EXPORT_ARG_BYTES_LIMIT: usize = 128 * 1024;
const ATTIC_PUSH_PATH_CHUNK_SIZE: usize = 200;

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

/// POST /api/v1/builders/resolve-id - Resolve builder ID from signed public key proof.
pub async fn resolve_builder_id(
    State(state): State<CFState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ResolveBuilderIdResponse>, (StatusCode, String)> {
    let request: ResolveBuilderIdRequest = serde_json::from_slice(&body).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid resolve-id request".to_string(),
        )
    })?;

    let public_key = PublicKey::from_base64(&request.public_key, "builder").map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid builder public key".to_string(),
        )
    })?;

    let builder = builders::get_builder_by_public_key(&state.pool, &public_key)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to resolve builder by public key");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to resolve builder".to_string(),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Unknown builder public key".to_string(),
            )
        })?;

    let timestamp_str = headers
        .get("X-Timestamp")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Missing X-Timestamp header".to_string(),
            )
        })?;

    let request_timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_str)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid X-Timestamp header".to_string(),
            )
        })?
        .with_timezone(&chrono::Utc);
    let now = chrono::Utc::now();
    const FRESHNESS_WINDOW_SECS: i64 = 5 * 60;
    if (now - request_timestamp).num_seconds().abs() > FRESHNESS_WINDOW_SECS {
        tracing::warn!(builder_id = %builder.id, "builder resolve-id rejected: stale timestamp");
        return Err((StatusCode::UNAUTHORIZED, "Stale timestamp".to_string()));
    }

    let signature_header = headers
        .get("X-Signature")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Missing X-Signature header".to_string(),
            )
        })?;

    let signature_bytes = general_purpose::STANDARD
        .decode(signature_header)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid X-Signature header".to_string(),
            )
        })?;
    let signature_array: [u8; 64] = signature_bytes.try_into().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid X-Signature length".to_string(),
        )
    })?;
    let signature = Signature::from_bytes(&signature_array);

    let signed_payload =
        canonical_signature_payload("POST", "/api/v1/builders/resolve-id", timestamp_str, &body);
    if builder
        .public_key
        .verifying_key()
        .verify(&signed_payload, &signature)
        .is_err()
    {
        tracing::warn!(builder_id = %builder.id, "builder resolve-id rejected: invalid signature");
        return Err((StatusCode::UNAUTHORIZED, "Invalid signature".to_string()));
    }

    Ok(Json(ResolveBuilderIdResponse {
        builder_id: builder.id,
    }))
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
    let valid_arches = ["x86_64-linux", "aarch64-linux", "aarch64-darwin", "x86_64-darwin"];
    if !valid_arches.contains(&arch) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid architecture. Must be one of: {}", valid_arches.join(", ")),
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

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    builders::finalize_cancelled_job(&state.pool, &job_id, &builder_id)
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
/// - `page` (default 1), `limit` (default 50, max 200)
/// - `status`: comma-separated statuses to include (queued, building, success, failed)
/// - `commit_hash`: prefix match on git commit hash
/// - `flake_name`: partial match on flake name
/// - `config_name`: partial match on system hostname / config name
/// - `queued_after`, `queued_before`: ISO-8601 timestamps bounding queued_at
pub async fn list_build_queue(
    State(state): State<CFState>,
    headers: HeaderMap,
    Query(params): Query<crate::api::models::BuildQueueParams>,
) -> Result<Json<crate::api::models::BuildQueuePageResponse>, StatusCode> {
    let Some(_viewer) = require_viewer_or_above(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

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
) -> Result<Json<Vec<crate::api::models::BuildQueueItem>>, StatusCode> {
    let Some(_viewer) = require_viewer_or_above(&state.pool, &headers).await else {
        return Err(StatusCode::FORBIDDEN);
    };

    let items = crate::queries::dashboard::fetch_recent_build_history(&state.pool, 100)
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
    builders::update_builder_heartbeat(&state.pool, &builder_id)
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
/// GET /api/v1/builders/:id/next-job - Get next job for builder
///
/// This endpoint implements the load-based job assignment logic:
/// 1. Filter jobs by builder's environment assignments (or all if no assignments)
/// 2. Check builder's current concurrency limit
/// 3. Return highest-priority queued job if available
pub async fn get_next_job(
    State(state): State<CFState>,
    Path(builder_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<crate::models::builders::NextJobResponse>, StatusCode> {
    // Authenticate builder request with replay resistance
    let path = format!("/api/v1/builders/{}/next-job", builder_id);
    let verified = authenticate_builder_request(&headers, body, "GET", &path, &state.pool).await?;

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
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to claim job atomically: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let Some(job) = job else {
        // Either no jobs available OR builder at capacity.
        // Return 404 NOT_FOUND so builder knows to wait.
        return Err(StatusCode::NOT_FOUND);
    };

    // Embed the derivation build payload so the remote builder needs no DB access.
    let derivation = crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to load derivation {} for claimed job {}: {}",
                job.derivation_id,
                job.id,
                e
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let payload = crate::models::builders::BuildJobDerivation {
        id: derivation.id,
        derivation_name: derivation.derivation_name.clone(),
        derivation_type: match derivation.derivation_type {
            crate::derivations::DerivationType::NixOS => "nixos".to_string(),
            crate::derivations::DerivationType::Package => "package".to_string(),
        },
        derivation_path: derivation.derivation_path.clone(),
        store_path: derivation.store_path.clone(),
    };

    Ok(Json(crate::models::builders::NextJobResponse {
        job,
        derivation: payload,
    }))
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

    if job.builder_id != Some(builder_id) {
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
    pub status: String,
    pub error_message: Option<String>,
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

    if job.builder_id != Some(builder_id) || job.status != "building" {
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
        "exporting derivation requisites archive"
    );

    let archive_chunks =
        chunk_derivation_archive_paths(&archive_paths, NIX_STORE_EXPORT_ARG_BYTES_LIMIT);
    let mut archive = Vec::new();

    for (chunk_index, archive_chunk) in archive_chunks.iter().enumerate() {
        let output = Command::new("nix-store")
            .arg("--export")
            .args(*archive_chunk)
            .output()
            .await
            .map_err(|e| {
                tracing::error!(
                    job_id = %job_id,
                    drv_path,
                    chunk_index,
                    chunk_count = archive_chunks.len(),
                    chunk_path_count = archive_chunk.len(),
                    "failed to run nix-store --export chunk: {e}"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!(
                job_id = %job_id,
                drv_path,
                path_count = archive_paths.len(),
                chunk_index,
                chunk_count = archive_chunks.len(),
                chunk_path_count = archive_chunk.len(),
                stderr = %stderr,
                "nix-store --export chunk failed"
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        archive.extend_from_slice(&output.stdout);
    }

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/x-nix-archive")
        .body(Body::from(archive))
        .map_err(|e| {
            tracing::error!(job_id = %job_id, "failed to build derivation archive response: {e}");
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

    if job.builder_id != Some(builder_id) || job.status != "building" {
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

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Job already marked as building by get_next_job
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
pub struct CompleteJobRequest {
    #[serde(default)]
    pub output_path: Option<String>,
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
        CompleteJobRequest { output_path: None }
    } else {
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?
    };

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Perform derivation completion + cache-push queueing server-side.
    if let Some(ref store_path) = request.output_path {
        if let Err(e) = crate::queries::derivations::mark_target_build_complete(
            &state.pool,
            job.derivation_id,
            store_path,
        )
        .await
        {
            tracing::error!(
                "Failed to mark derivation {} complete for job {}: {}",
                job.derivation_id,
                job_id,
                e
            );
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        let cache_destination = crate::queries::cache_destinations::list_cache_destinations(
            &state.pool,
            true,
        )
        .await
        .ok()
        .and_then(|dests| dests.into_iter().next().map(|d| d.name));

        if let Err(e) = crate::queries::cache_push::create_cache_push_job(
            &state.pool,
            job.derivation_id,
            store_path,
            cache_destination.as_deref(),
        )
        .await
        {
            tracing::warn!(
                "Failed to queue cache push for derivation {} (job {}): {}",
                job.derivation_id,
                job_id,
                e
            );
        }
    }

    // Mark job as complete
    builders::mark_job_complete(&state.pool, &job_id, &builder_id)
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

    cleanup_build_log_channel(&state, job_id).await;

    Ok(StatusCode::OK)
}

/// POST /api/v1/builders/:id/jobs/:job_id/fail - Mark job as failed
///
/// Implements retry logic:
/// - If retry_count < max_retries: increment retry_count, re-queue job, reduce priority
/// - If retry_count >= max_retries: mark as permanently failed
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

    // Parse failure details
    let request: JobStatusRequest =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Verify the job is assigned to this builder
    let job = builders::get_build_job_by_id(&state.pool, &job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if job.builder_id != Some(builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Mark job as failed with retry logic
    let updated_job = builders::mark_job_failed_with_retry(
        &state.pool,
        &job_id,
        &builder_id,
        request.error_message.as_deref(),
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

    // Return 200 for re-queued jobs, 202 for permanently failed jobs
    if updated_job.status == "queued" {
        Ok(StatusCode::OK) // Job re-queued for retry
    } else {
        // Permanent failure: record the derivation-level failure server-side so
        // API builders never touch the database directly.
        match crate::queries::derivations::get_derivation_by_id(&state.pool, job.derivation_id)
            .await
        {
            Ok(derivation) => {
                let err = anyhow::anyhow!(
                    request
                        .error_message
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

        cleanup_build_log_channel(&state, job_id).await;
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

    if job.builder_id != Some(builder_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Builder cannot append logs for a job assigned to another builder".to_string(),
        ));
    }

    // Only queued/building jobs may receive log appends.
    if job.status != "queued" && job.status != "building" {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot append logs for job in '{}' status; only 'queued' and 'building' are allowed",
                job.status
            ),
        ));
    }

    // Append logs with per-job size cap enforcement.
    builders::append_job_logs_with_limits(&state.pool, &job_id, &request.logs, max_total_bytes)
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
    Builder(Uuid),
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

    if job.builder_id != Some(verified.builder_id) {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(BuildLogStreamPrincipal::Builder(verified.builder_id))
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

    let history_snapshot = {
        let history = state.build_log_history.lock().await;
        history.get(&job_id).cloned().unwrap_or_default()
    };

    for frame in history_snapshot {
        if let Err(e) = socket.send(Message::Text(frame.into())).await {
            tracing::debug!(
                "Failed to replay build log history to websocket for job {}: {}",
                job_id,
                e
            );
            return;
        }
    }

    match principal {
        BuildLogStreamPrincipal::Viewer => {
            let mut rx = tx.subscribe();
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
        BuildLogStreamPrincipal::Builder(builder_id) => {
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
                                if let Err(e) = builders::append_job_logs_with_limits(
                                    &state.pool,
                                    &job_id,
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
    use axum::http::StatusCode;

    use super::BuildStreamMessage;
    use super::chunk_derivation_archive_paths;
    use super::map_create_builder_error;
    use super::parse_derivation_requisites;

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
}
