use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone)]
pub struct WorkerStatus {
    pub worker_id: usize,
    pub current_task: Option<String>,
    pub started_at: Option<std::time::Instant>,
    pub state: WorkerState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorkerState {
    Idle,
    Working,
    Sleeping,
}

// Global status tracker using OnceLock
pub static BUILD_WORKER_STATUS: OnceLock<Arc<RwLock<Vec<WorkerStatus>>>> = OnceLock::new();
pub static CVE_SCAN_STATUS: OnceLock<Arc<RwLock<Option<WorkerStatus>>>> = OnceLock::new();
pub static CACHE_PUSH_STATUS: OnceLock<Arc<RwLock<Option<WorkerStatus>>>> = OnceLock::new();
pub static DRY_RUN_WORKER_STATUS: OnceLock<Arc<RwLock<Vec<WorkerStatus>>>> = OnceLock::new();

pub fn get_dry_run_status() -> &'static Arc<RwLock<Vec<WorkerStatus>>> {
    DRY_RUN_WORKER_STATUS.get_or_init(|| Arc::new(RwLock::new(Vec::new())))
}

pub fn get_build_status() -> &'static Arc<RwLock<Vec<WorkerStatus>>> {
    BUILD_WORKER_STATUS.get_or_init(|| Arc::new(RwLock::new(Vec::new())))
}

pub fn get_cve_status() -> &'static Arc<RwLock<Option<WorkerStatus>>> {
    CVE_SCAN_STATUS.get_or_init(|| Arc::new(RwLock::new(None)))
}

pub fn get_cache_status() -> &'static Arc<RwLock<Option<WorkerStatus>>> {
    CACHE_PUSH_STATUS.get_or_init(|| Arc::new(RwLock::new(None)))
}

pub async fn log_builder_worker_status() {
    let build_workers = get_build_status().read().await;
    let dry_run_workers = get_dry_run_status().read().await;
    let cve_status = get_cve_status().read().await;
    let cache_status = get_cache_status().read().await;

    info!("=== Worker Status ===");

    // Dry run workers
    info!("Dry Run Workers ({} total):", dry_run_workers.len());
    for worker in dry_run_workers.iter() {
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
