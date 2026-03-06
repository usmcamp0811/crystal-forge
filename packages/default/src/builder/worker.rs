//! Build worker implementation for the builder module.
//!
//! This module contains the core build worker loop that claims derivations,
//! executes builds, and manages build lifecycle (completion, failure, timeouts).

use super::status::update_worker_status;
use crate::config::{BuildConfig, CacheConfig};
use crate::derivations::Derivation;
use crate::log::WorkerState;
use crate::queries::build_reservations;
use crate::queries::cache_push::create_cache_push_job;
use crate::queries::derivations::{handle_derivation_failure, mark_target_build_complete};
use anyhow::Result;
use sqlx::PgPool;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Main build worker loop
///
/// CRITICAL IMPROVEMENTS:
/// 1. Timeout protection prevents workers from getting stuck for hours
/// 2. Helper functions for task description and status updates
/// 3. Better error handling and logging
pub(super) async fn build_worker(
    worker_id: usize,
    worker_uuid: String,
    pool: PgPool,
    build_config: BuildConfig,
    cache_config: CacheConfig,
    use_mock_build: bool,
) {
    update_worker_status(
        worker_id,
        WorkerState::Working,
        Some("claiming work".to_string()),
    );

    info!("Worker {} ({}) started", worker_id, worker_uuid);

    // Spawn heartbeat task for this worker
    let heartbeat_pool = pool.clone();
    let heartbeat_uuid = worker_uuid.clone();
    tokio::spawn(async move {
        worker_heartbeat_loop(heartbeat_uuid, heartbeat_pool).await;
    });

    // Get the build timeout from config (with a reasonable maximum)
    // This is CRITICAL to prevent workers from getting stuck for hours
    let build_timeout = std::cmp::min(
        build_config.timeout,
        std::time::Duration::from_secs(7200), // Max 2 hours
    );

    info!(
        "Worker {} configured with {:.1}s timeout",
        worker_id,
        build_timeout.as_secs_f64()
    );

    loop {
        update_worker_status(
            worker_id,
            WorkerState::Working,
            Some("claiming work".to_string()),
        );

        match build_reservations::claim_next_derivation(&pool, &worker_uuid).await {
            Ok(Some(mut derivation)) => {
                info!(
                    "✅ Worker {} CLAIMED derivation {}",
                    worker_id, derivation.derivation_name
                );

                // ADD THIS DEBUG LINE
                info!(
                    "🔨 Worker {} STARTING BUILD for {}",
                    worker_id, derivation.derivation_name
                );

                // Build task description using helper function (no embedded SQL)
                let task_description = derivation.derivation_name.clone();
                // let task_description = build_task_description(&pool, &derivation).await;

                update_worker_status(
                    worker_id,
                    WorkerState::Working,
                    Some(task_description.clone()),
                );

                info!(
                    "Worker {} claimed: {} (type: {:?}, cf_agent: {:?}, attempt: {})",
                    worker_id,
                    task_description,
                    derivation.derivation_type,
                    derivation.cf_agent_enabled,
                    derivation.attempt_count
                );

                let start = std::time::Instant::now();

                info!(
                    "🔨 Worker {} STARTING BUILD for {}",
                    worker_id, derivation.derivation_name
                );
                info!("  → Step 1: About to call derivation.build()");

                let build_result = if use_mock_build {
                    tokio::time::timeout(build_timeout, run_mock_legacy_build(&derivation)).await
                } else {
                    tokio::time::timeout(build_timeout, derivation.build(&pool, &build_config))
                        .await
                };

                info!("  → Step 2: derivation.build() returned");

                match build_result {
                    // Build succeeded within timeout
                    Ok(Ok(store_path)) => {
                        let duration = start.elapsed();
                        info!(
                            "✅ worker {} completed {} in {:.1}s: {}",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            store_path
                        );

                        // update derivation with store_path for signing
                        derivation.store_path = Some(store_path.clone());

                        if !use_mock_build {
                            // sign before cache push
                            if let Err(e) = derivation.sign(&cache_config).await {
                                warn!(
                                    "⚠️ signing failed for {}, continuing anyway: {}",
                                    task_description, e
                                );
                                // non-fatal - we can still push to cache unsigned
                            }

                            // TODO: Include the name of the server that built the derivation
                            if let Some(ref store_path) = derivation.store_path {
                                if let Err(e) = create_cache_push_job(
                                    &pool,
                                    derivation.id,
                                    store_path, // &String coerces to &str
                                    cache_config.push_to.as_deref(), // Option<String> -> Option<&str>
                                )
                                .await
                                {
                                    warn!(
                                        "⚠️ cache queue failed for {}, continuing anyway: {}",
                                        task_description, e
                                    );
                                }
                            } else {
                                warn!(
                                    "⚠️ skipping cache queue for {}: missing store_path on derivation {}",
                                    task_description, derivation.id
                                );
                            }
                        }

                        if let Err(e) = mark_build_complete_and_release(
                            &pool,
                            &worker_uuid,
                            derivation.id,
                            &store_path,
                        )
                        .await
                        {
                            error!("failed to mark build complete: {}", e);
                        }
                    }

                    // Build failed within timeout
                    Ok(Err(e)) => {
                        let duration = start.elapsed();
                        error!(
                            "❌ Worker {} build failed for {} after {:.1}s: {}",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            e
                        );

                        if let Err(e2) =
                            mark_build_failed_and_release(&pool, &worker_uuid, &derivation, &e)
                                .await
                        {
                            error!("Failed to mark build failed: {}", e2);
                        }
                    }

                    // Build TIMED OUT - this is the fix for stuck workers!
                    Err(_timeout) => {
                        let duration = start.elapsed();
                        let timeout_error = anyhow::anyhow!(
                            "Build timed out after {:.1}s (limit: {:.1}s)",
                            duration.as_secs_f64(),
                            build_timeout.as_secs_f64()
                        );

                        error!(
                            "⏱️  Worker {} build TIMEOUT for {} after {:.1}s (limit: {:.1}s)",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            build_timeout.as_secs_f64()
                        );

                        if let Err(e2) = mark_build_failed_and_release(
                            &pool,
                            &worker_uuid,
                            &derivation,
                            &timeout_error,
                        )
                        .await
                        {
                            error!("Failed to mark build timeout: {}", e2);
                        }
                    }
                }
            }

            // No work available - idle
            Ok(None) => {
                update_worker_status(worker_id, WorkerState::Idle, None);
                debug!("Worker {} idle, no work available", worker_id);
                sleep(std::time::Duration::from_secs(5)).await;
            }

            // Error claiming work
            Err(e) => {
                error!("Worker {} error claiming work: {}", worker_id, e);
                sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }
}

async fn run_mock_legacy_build(derivation: &Derivation) -> Result<String> {
    info!(
        "🧪 MOCK MODE: simulating legacy builder build for {}",
        derivation.derivation_name
    );

    for step in [
        "Resolving derivation graph",
        "Preparing build sandbox",
        "Building outputs",
        "Finalizing store path",
    ] {
        info!("🧪 MOCK BUILD [{}]: {}", derivation.derivation_name, step);
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    derivation.id.hash(&mut hasher);
    derivation.derivation_name.hash(&mut hasher);
    let short_hash = format!("{:016x}", hasher.finish());
    let sanitized = derivation
        .derivation_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();

    Ok(format!("/nix/store/{}-{}", short_hash, sanitized))
}

/// Mark build complete and release reservation
async fn mark_build_complete_and_release(
    pool: &PgPool,
    worker_uuid: &str,
    derivation_id: i32,
    store_path: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Delete reservation
    build_reservations::delete_reservation(&mut *tx, worker_uuid, derivation_id).await?;

    // Mark complete
    mark_target_build_complete(&mut *tx, derivation_id, store_path).await?;

    tx.commit().await?;

    // Create GC root to prevent cleanup before cache push
    if let Err(e) = create_gc_root(store_path, derivation_id).await {
        warn!("Failed to create GC root for {}: {}", store_path, e);
    }

    Ok(())
}

/// Mark build failed and release reservation
async fn mark_build_failed_and_release(
    pool: &PgPool,
    worker_uuid: &str,
    derivation: &Derivation,
    error: &anyhow::Error,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Delete reservation
    build_reservations::delete_reservation(&mut *tx, worker_uuid, derivation.id).await?;

    // Mark failed
    handle_derivation_failure(&mut *tx, derivation, "build", error).await?;

    tx.commit().await?;
    Ok(())
}

/// Worker heartbeat loop - updates reservation heartbeat every 30 seconds
async fn worker_heartbeat_loop(worker_uuid: String, pool: PgPool) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        match build_reservations::update_heartbeat(&pool, &worker_uuid).await {
            Ok(count) if count > 0 => {
                debug!(
                    "Worker {} heartbeat updated ({} reservations)",
                    worker_uuid, count
                );
            }
            Err(e) => {
                error!("Worker {} heartbeat failed: {}", worker_uuid, e);
            }
            _ => {}
        }
    }
}

pub async fn get_gc_root_path(derivation_id: i32) -> String {
    // Try environment variable first, then fall back to /var/cache, then temp dir
    let gc_root_dir = std::env::var("CRYSTAL_FORGE_GC_ROOT_DIR").unwrap_or_else(|_| {
        // Try /var/cache first
        if std::path::Path::new("/var/cache/crystal-forge").exists()
            || std::fs::create_dir_all("/var/cache/crystal-forge/gc-roots").is_ok()
        {
            "/var/cache/crystal-forge/gc-roots".to_string()
        } else {
            // Fall back to temp directory
            format!("{}/crystal-forge/gc-roots", std::env::temp_dir().display())
        }
    });

    // Create the directory if it doesn't exist
    if let Err(e) = tokio::fs::create_dir_all(&gc_root_dir).await {
        warn!("Failed to create GC root directory {}: {}", gc_root_dir, e);
        // Use temp dir as last resort
        let temp_gc_dir = format!("{}/crystal-forge/gc-roots", std::env::temp_dir().display());
        tokio::fs::create_dir_all(&temp_gc_dir)
            .await
            .expect("failed to create GC root directory in temp");
        return format!("{}/derivation-{}", temp_gc_dir, derivation_id);
    }

    format!("{}/derivation-{}", gc_root_dir, derivation_id)
}

/// Create a GC root to prevent garbage collection until cache push
pub async fn create_gc_root(store_path: &str, derivation_id: i32) -> Result<()> {
    let gc_root_path = get_gc_root_path(derivation_id).await;

    // Create symlink to store path
    if let Err(e) = tokio::fs::symlink(store_path, &gc_root_path).await {
        // Ignore if already exists
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(e.into());
        }
    }

    debug!("Created GC root: {} -> {}", gc_root_path, store_path);
    Ok(())
}

/// Remove GC root after successful cache push
pub async fn remove_gc_root(derivation_id: i32) -> Result<()> {
    let gc_root_path = get_gc_root_path(derivation_id).await;

    if let Err(e) = tokio::fs::remove_file(&gc_root_path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to remove GC root {}: {}", gc_root_path, e);
        }
    } else {
        debug!("Removed GC root: {}", gc_root_path);
    }

    Ok(())
}
