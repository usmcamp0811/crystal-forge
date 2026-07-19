use crate::config::CrystalForgeConfig;
use crate::queries::cve_scans::{
    create_cve_scan, get_active_scan_for_derivation, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::get_derivation_by_id;
use crate::vulnix::vulnix_runner::VulnixRunner;
use sqlx::PgPool;
use tracing::error;
use uuid::Uuid;

/// Domain errors for CVE scan operations.
///
/// Handlers match on this enum rather than inspecting error message strings,
/// so the API contract is stable even if log/display text changes.
#[derive(Debug)]
pub enum CveScanError {
    /// vulnix is not installed or not reachable on this node.
    VulnixUnavailable,
    /// Any other unexpected failure (DB error, spawn error, etc.).
    Internal(anyhow::Error),
}

impl std::fmt::Display for CveScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CveScanError::VulnixUnavailable => {
                write!(
                    f,
                    "vulnix is not available on this node; immediate scan cannot start"
                )
            }
            CveScanError::Internal(err) => write!(f, "{err:#}"),
        }
    }
}

impl From<anyhow::Error> for CveScanError {
    fn from(err: anyhow::Error) -> Self {
        CveScanError::Internal(err)
    }
}

pub async fn trigger_immediate_cve_scan(
    pool: PgPool,
    derivation_id: i32,
) -> Result<Uuid, CveScanError> {
    if let Some(existing_scan_id) = get_active_scan_for_derivation(&pool, derivation_id)
        .await
        .map_err(CveScanError::Internal)?
    {
        return Ok(existing_scan_id);
    }

    if !VulnixRunner::check_vulnix_available().await {
        return Err(CveScanError::VulnixUnavailable);
    }

    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();
    let scan_id = create_cve_scan(&pool, derivation_id, "vulnix", vulnix_version.clone())
        .await
        .map_err(CveScanError::Internal)?;

    let spawn_pool = pool.clone();
    tokio::spawn(async move {
        let result: anyhow::Result<()> = async {
            let derivation = get_derivation_by_id(&spawn_pool, derivation_id).await?;
            let cfg = CrystalForgeConfig::load().unwrap_or_else(|_| CrystalForgeConfig::default());
            let vulnix_config = cfg.get_vulnix_config();
            let runner = VulnixRunner::with_config(vulnix_config);

            mark_scan_in_progress(&spawn_pool, scan_id).await?;
            let started = std::time::Instant::now();

            match runner
                .scan_derivation(&spawn_pool, derivation_id, vulnix_version)
                .await
            {
                Ok(vulnix_entries) => {
                    let scan_duration_ms = Some(started.elapsed().as_millis() as i32);
                    save_scan_results(&spawn_pool, scan_id, &vulnix_entries, scan_duration_ms)
                        .await?;
                }
                Err(err) => {
                    mark_cve_scan_failed(&spawn_pool, &derivation, &err.to_string()).await?;
                }
            }

            Ok(())
        }
        .await;

        if let Err(err) = result {
            error!("Immediate CVE scan task failed for derivation {derivation_id}: {err:#}");
        }
    });

    Ok(scan_id)
}
