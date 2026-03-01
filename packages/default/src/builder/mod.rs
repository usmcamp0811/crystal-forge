//! Builder module for continuous build processing.
//!
//! This module orchestrates the build system with separate workers for:
//! - Build execution
//! - CVE scanning
//! - Binary cache pushing
//! - Build reservation cleanup
//!
//! ## Architecture
//!
//! The module is organized into focused submodules:
//! - `worker` - Core build worker implementation
//! - `cve_worker` - CVE scanning with vulnix
//! - `cache_worker` - Binary cache push operations
//! - `status` - Worker status tracking and task descriptions
//! - `reservation` - Stale reservation cleanup
//! - `error` - Error types (currently uses anyhow)
//!
//! ## Usage
//!
//! ```no_run
//! use crystal_forge::builder::run_build_loop;
//! use sqlx::PgPool;
//!
//! #[tokio::main]
//! async fn main() {
//!     let pool = PgPool::connect("postgresql://localhost/crystal_forge").await.unwrap();
//!     run_build_loop(pool).await;
//! }
//! ```

pub mod api_client;
pub mod metrics;

mod cache_worker;
mod cve_worker;
mod error;
mod reservation;
mod status;
mod worker;

// Re-export public functions
pub use cache_worker::{process_cache_pushes, run_cache_push_loop, run_cache_push_workers};
pub use cve_worker::run_cve_scan_loop;
pub use worker::{create_gc_root, get_gc_root_path, remove_gc_root};

use crate::config::CrystalForgeConfig;
use crate::log::{WorkerState, WorkerStatus, get_build_status};
use sqlx::PgPool;
use tracing::{info, warn};

/// Runs the continuous build loop with multiple workers.
///
/// This is the main entry point for the build system. It:
/// 1. Initializes worker status tracking
/// 2. Spawns a reservation cleanup background task
/// 3. Spawns N build workers (configured via BuildConfig)
/// 4. Waits for all workers to complete (runs indefinitely)
///
/// # Configuration
///
/// Worker count and timeouts are controlled by `BuildConfig`:
/// - `max_concurrent_derivations` - number of parallel build workers
/// - `timeout` - maximum build time per derivation
///
/// # Example
///
/// ```no_run
/// use crystal_forge::builder::run_build_loop;
/// use sqlx::PgPool;
///
/// #[tokio::main]
/// async fn main() {
///     let pool = PgPool::connect("postgresql://localhost/crystal_forge").await.unwrap();
///     run_build_loop(pool).await;
/// }
/// ```
pub async fn run_build_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
    let cache_config = cfg.get_cache_config();
    let num_workers = build_config.max_concurrent_derivations;

    info!("🏗 Starting {} continuous build workers...", num_workers);

    // Get hostname for worker IDs
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    // Pre-initialize worker status tracking BEFORE spawning workers
    {
        let mut statuses = get_build_status().write().await;
        for worker_id in 0..num_workers {
            statuses.push(WorkerStatus {
                worker_id,
                current_task: None,
                started_at: None,
                state: WorkerState::Idle,
            });
        }
    }

    // Spawn stale reservation cleanup task
    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        reservation::run_reservation_cleanup_loop(cleanup_pool).await;
    });

    // Spawn worker pool
    let mut handles = Vec::new();
    for worker_id in 0..num_workers {
        let pool = pool.clone();
        let build_config = build_config.clone();
        let cache_config = cache_config.clone();
        let worker_uuid = format!("{}-worker-{}", hostname, worker_id);

        let handle = tokio::spawn(async move {
            worker::build_worker(worker_id, worker_uuid, pool, build_config, cache_config).await;
        });
        handles.push(handle);
    }

    // Wait for all workers
    for handle in handles {
        let _ = handle.await;
    }
}
