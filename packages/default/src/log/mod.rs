use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig, VulnixConfig};
use crate::models::derivations::{Derivation, DerivationType};
use crate::queries::cache_push::{
    create_cache_push_job, get_derivations_needing_cache_push_for_dest,
    get_pending_cache_push_jobs, mark_cache_push_completed, mark_cache_push_failed,
    mark_cache_push_in_progress, mark_derivation_cache_pushed,
};
use crate::queries::commits::get_commits_pending_evaluation;
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::{
    EvaluationStatus, claim_next_derivation, get_derivations_ready_for_build,
    mark_target_build_in_progress, mark_target_failed, update_derivation_status,
};
use crate::queries::derivations::{
    discover_and_queue_all_transitive_dependencies, handle_derivation_failure,
    mark_target_build_complete,
};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use anyhow::bail;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::fs;
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
struct WorkerStatus {
    worker_id: usize,
    current_task: Option<String>,
    started_at: Option<std::time::Instant>,
    state: WorkerState,
}

#[derive(Debug, Clone)]
enum WorkerState {
    Idle,
    Working,
    Sleeping,
}

// Global status tracker using OnceLock
static BUILD_WORKER_STATUS: OnceLock<Arc<RwLock<Vec<WorkerStatus>>>> = OnceLock::new();
static CVE_SCAN_STATUS: OnceLock<Arc<RwLock<Option<WorkerStatus>>>> = OnceLock::new();
static CACHE_PUSH_STATUS: OnceLock<Arc<RwLock<Option<WorkerStatus>>>> = OnceLock::new();

fn get_build_status() -> &'static Arc<RwLock<Vec<WorkerStatus>>> {
    BUILD_WORKER_STATUS.get_or_init(|| Arc::new(RwLock::new(Vec::new())))
}

fn get_cve_status() -> &'static Arc<RwLock<Option<WorkerStatus>>> {
    CVE_SCAN_STATUS.get_or_init(|| Arc::new(RwLock::new(None)))
}

fn get_cache_status() -> &'static Arc<RwLock<Option<WorkerStatus>>> {
    CACHE_PUSH_STATUS.get_or_init(|| Arc::new(RwLock::new(None)))
}

pub async fn log_builder_worker_status() {
    let build_workers = get_build_status().read().await;
    let cve_status = get_cve_status().read().await;
    let cache_status = get_cache_status().read().await;

    info!("=== Worker Status ===");

    // Build workers
    info!("Build Workers ({} total):", build_workers.len());
    for worker in build_workers.iter() {
        match &worker.current_task {
            Some(task) => {
                let elapsed = worker
                    .started_at
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                info!(
                    "  Worker {}: {:?} - {} ({}s)",
                    worker.worker_id, worker.state, task, elapsed
                );
            }
            None => {
                info!("  Worker {}: {:?}", worker.worker_id, worker.state);
            }
        }
    }

    // CVE scan
    if let Some(status) = cve_status.as_ref() {
        match &status.current_task {
            Some(task) => {
                let elapsed = status
                    .started_at
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                info!("CVE Scanner: {:?} - {} ({}s)", status.state, task, elapsed);
            }
            None => {
                info!("CVE Scanner: {:?}", status.state);
            }
        }
    }

    // Cache push
    if let Some(status) = cache_status.as_ref() {
        match &status.current_task {
            Some(task) => {
                let elapsed = status
                    .started_at
                    .map(|t| t.elapsed().as_secs())
                    .unwrap_or(0);
                info!("Cache Push: {:?} - {} ({}s)", status.state, task, elapsed);
            }
            None => {
                info!("Cache Push: {:?}", status.state);
            }
        }
    }
}
