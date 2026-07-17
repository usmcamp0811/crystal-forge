//! Background job registry for the Crystal Forge server.
//!
//! This module defines a lightweight `BackgroundJob` abstraction that every
//! long-running server background task registers with.  The registry is
//! intentionally simple: it holds shared-state handles so that the future
//! Admin → Background Jobs tab (TASK-336.5) can expose real job metadata
//! (status, last-run, next-run, interval) and runtime controls
//! (enable/disable, run-now) without a heavyweight scheduler.
//!
//! ## Design goals
//!
//! - No dynamic dispatch / trait objects required for the common case.
//! - Each job carries its own `Arc<BackgroundJobState>` which is cloned into
//!   both the background task and the registry so both sides see live state.
//! - `run_now_tx` is a `tokio::sync::watch::Sender<bool>` — the background
//!   task selects on it and starts an immediate cycle when it flips to `true`.
//!   After handling the signal the task resets the channel to `false`.
//! - The registry is stored on `CFState` (or wherever the server state lives)
//!   so HTTP handlers can reach it.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{RwLock, watch};
use tokio::time::Duration;
use tracing::info;

/// Live runtime state for a single background job.
#[derive(Debug, Default)]
pub struct BackgroundJobState {
    pub enabled: RwLock<bool>,
    pub last_run_at: RwLock<Option<DateTime<Utc>>>,
    pub next_run_at: RwLock<Option<DateTime<Utc>>>,
    pub is_running: RwLock<bool>,
}

impl BackgroundJobState {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: RwLock::new(enabled),
            last_run_at: RwLock::new(None),
            next_run_at: RwLock::new(None),
            is_running: RwLock::new(false),
        }
    }
}

/// A handle to a registered background job.
///
/// Clone this and store it both in the registry and in the background task
/// so both sides share the same `Arc<BackgroundJobState>`.
#[derive(Debug, Clone)]
pub struct BackgroundJobHandle {
    /// Human-readable identifier (e.g. `"cve_scan"`).
    pub id: &'static str,
    /// Human-readable display name.
    pub name: &'static str,
    /// Configured polling / rescan interval (informational; the task uses this directly).
    pub interval: Duration,
    /// Shared mutable state visible to both the task and HTTP handlers.
    pub state: Arc<BackgroundJobState>,
    /// Send `true` to trigger an immediate run cycle; the task resets to `false`.
    pub run_now_tx: watch::Sender<bool>,
}

impl BackgroundJobHandle {
    /// Create a new handle.  Pass a clone of the returned `watch::Receiver` to
    /// the background task so it can listen for run-now signals.
    pub fn new(
        id: &'static str,
        name: &'static str,
        interval: Duration,
        enabled: bool,
    ) -> (Self, watch::Receiver<bool>) {
        let (run_now_tx, run_now_rx) = watch::channel(false);
        let state = Arc::new(BackgroundJobState::new(enabled));
        let handle = Self {
            id,
            name,
            interval,
            state,
            run_now_tx,
        };
        (handle, run_now_rx)
    }

    /// Trigger an immediate run cycle.
    pub fn trigger_run_now(&self) {
        let _ = self.run_now_tx.send(true);
        info!("🔔 Background job '{}' triggered for immediate run", self.id);
    }

    /// Enable or disable the job.
    pub async fn set_enabled(&self, enabled: bool) {
        *self.state.enabled.write().await = enabled;
        info!(
            "⚙️  Background job '{}' {}",
            self.id,
            if enabled { "enabled" } else { "disabled" }
        );
    }
}

/// JSON-serialisable snapshot of a job for the Admin API (TASK-336.5).
#[derive(Debug, Serialize)]
pub struct BackgroundJobStatus {
    pub id: &'static str,
    pub name: &'static str,
    pub interval_seconds: u64,
    pub enabled: bool,
    pub is_running: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}

impl BackgroundJobHandle {
    /// Snapshot current state for API serialisation.
    pub async fn status(&self) -> BackgroundJobStatus {
        let state = &self.state;
        BackgroundJobStatus {
            id: self.id,
            name: self.name,
            interval_seconds: self.interval.as_secs(),
            enabled: *state.enabled.read().await,
            is_running: *state.is_running.read().await,
            last_run_at: *state.last_run_at.read().await,
            next_run_at: *state.next_run_at.read().await,
        }
    }
}

/// The server-wide registry of all registered background jobs.
///
/// Stored on server state so HTTP handlers and the future Admin Background Jobs
/// API (TASK-336.5) can reach all registered jobs without knowing their types.
#[derive(Debug, Default, Clone)]
pub struct BackgroundJobRegistry {
    jobs: Arc<RwLock<Vec<BackgroundJobHandle>>>,
}

impl BackgroundJobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a job handle.  Called once per job during server startup.
    pub async fn register(&self, handle: BackgroundJobHandle) {
        info!(
            "📋 Registered background job '{}' (interval: {}s, enabled: {})",
            handle.id,
            handle.interval.as_secs(),
            *handle.state.enabled.read().await,
        );
        self.jobs.write().await.push(handle);
    }

    /// Return a snapshot of all job statuses (for the Admin API).
    pub async fn all_statuses(&self) -> Vec<BackgroundJobStatus> {
        let jobs = self.jobs.read().await;
        let mut out = Vec::with_capacity(jobs.len());
        for job in jobs.iter() {
            out.push(job.status().await);
        }
        out
    }

    /// Find a job by id and return a clone of its handle.
    pub async fn find(&self, id: &str) -> Option<BackgroundJobHandle> {
        self.jobs
            .read()
            .await
            .iter()
            .find(|j| j.id == id)
            .cloned()
    }
}
