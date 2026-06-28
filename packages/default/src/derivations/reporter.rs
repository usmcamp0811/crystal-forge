//! Build progress reporting and cancellation abstraction.
//!
//! [`BuildReporter`] decouples the streaming build loop in [`crate::derivations::build`]
//! from any specific transport. The server/local worker reports progress and checks
//! cancellation directly against the database via [`PgPoolReporter`]. Remote API-mode
//! builders implement this trait against the server API (WebSocket primary, HTTP
//! fallback) so they never need a database connection.

use anyhow::Result;
use axum::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// Progress snapshot emitted periodically during a streaming build.
#[derive(Debug, Clone)]
pub struct BuildProgress {
    /// Database id of the derivation being built.
    pub derivation_id: i32,
    /// Seconds elapsed since the build started.
    pub elapsed_seconds: i32,
    /// Most recent build target line (e.g. `building '/nix/store/...drv'`).
    pub current_target: Option<String>,
    /// Seconds since the last build output line was observed.
    pub last_activity_seconds: i32,
}

/// Abstraction over the two DB-coupled operations performed inside the streaming
/// build loop: periodic progress reporting and cancellation checks.
///
/// Implementations must be cheap to call repeatedly and must never panic; transient
/// transport errors should be surfaced as `Err` (for progress) or `false`/`Err`
/// (for cancellation) and treated as non-fatal by the build loop.
#[async_trait]
pub trait BuildReporter: Send + Sync {
    /// Report periodic build progress. Non-fatal on error.
    async fn report_progress(&self, progress: &BuildProgress) -> Result<()>;

    /// Returns `true` if the build job has been marked for cancellation.
    ///
    /// `job_id` is the `build_jobs.id`; `None` means there is no cancellable job
    /// context and the build should never be considered cancelled.
    async fn is_cancelled(&self, job_id: Option<Uuid>) -> Result<bool>;
}

/// Database-backed reporter used by the in-process/server worker.
///
/// Reports progress via `update_build_heartbeat` and checks cancellation by reading
/// the build job status directly from the database.
#[derive(Clone)]
pub struct PgPoolReporter {
    pool: PgPool,
}

impl PgPoolReporter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Borrow the underlying pool (useful for callers that still need it).
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl BuildReporter for PgPoolReporter {
    async fn report_progress(&self, progress: &BuildProgress) -> Result<()> {
        crate::queries::derivations::update_build_heartbeat(
            &self.pool,
            progress.derivation_id,
            progress.elapsed_seconds,
            progress.current_target.as_deref(),
            progress.last_activity_seconds,
        )
        .await
    }

    async fn is_cancelled(&self, job_id: Option<Uuid>) -> Result<bool> {
        let Some(job_id) = job_id else {
            return Ok(false);
        };

        let status = crate::queries::builders::get_build_job_status(&self.pool, &job_id).await?;
        Ok(matches!(status.as_deref(), Some("cancelling")))
    }
}
