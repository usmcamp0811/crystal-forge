use crate::config::CrystalForgeConfig;
use crate::queries::cve_scans::{
    CreateCveScanOutcome, acknowledge_revoked_cve_scan_execution, create_cve_scan,
    get_active_scan_for_derivation, heartbeat_cve_scan_execution,
    mark_cve_scan_failed_by_id_for_execution, mark_cve_scan_failed_for_execution,
    save_scan_results_for_execution,
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
    let scan_claim = create_cve_scan(&pool, derivation_id, "vulnix", vulnix_version.clone())
        .await
        .map_err(CveScanError::Internal)?;
    let claim = match scan_claim {
        CreateCveScanOutcome::Created(claim) => claim,
        CreateCveScanOutcome::Existing(scan_id) => return Ok(scan_id),
    };
    let scan_id = claim.scan_id;

    let spawn_pool = pool.clone();
    tokio::spawn(async move {
        let execution = async {
            let derivation = get_derivation_by_id(&spawn_pool, derivation_id).await?;
            let cfg = CrystalForgeConfig::load().unwrap_or_else(|_| CrystalForgeConfig::default());
            let vulnix_config = cfg.get_vulnix_config();
            let runner = VulnixRunner::with_config(vulnix_config);

            let started = std::time::Instant::now();

            match runner
                .scan_derivation(&spawn_pool, derivation_id, vulnix_version)
                .await
            {
                Ok(vulnix_entries) => {
                    let scan_duration_ms = Some(started.elapsed().as_millis() as i32);
                    if let Err(err) = save_scan_results_for_execution(
                        &spawn_pool,
                        scan_id,
                        &vulnix_entries,
                        scan_duration_ms,
                        claim.execution_id,
                    )
                    .await
                    {
                        return Err(err);
                    }
                }
                Err(err) => {
                    mark_cve_scan_failed_for_execution(
                        &spawn_pool,
                        scan_id,
                        &derivation,
                        &err.to_string(),
                        claim.execution_id,
                    )
                    .await?;
                }
            }

            Ok(())
        };

        let mut execution = Box::pin(execution);
        let start = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut heartbeat = tokio::time::interval_at(start, std::time::Duration::from_secs(30));
        const HEARTBEAT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
        let result = loop {
            tokio::select! {
                biased;
                result = &mut execution => break result,
                _ = heartbeat.tick() => {
                    match tokio::time::timeout(
                        HEARTBEAT_QUERY_TIMEOUT,
                        heartbeat_cve_scan_execution(
                            &spawn_pool,
                            scan_id,
                            claim.execution_id,
                        ),
                    )
                    .await
                    {
                        Ok(Ok(true)) => {}
                        Ok(Ok(false)) => {
                            drop(execution);
                            if acknowledge_revoked_cve_scan_execution(
                                &spawn_pool,
                                scan_id,
                                claim.execution_id,
                            )
                            .await
                            .unwrap_or(false)
                            {
                                return;
                            }
                            break Err(anyhow::anyhow!(
                                "CVE scan {scan_id} lost execution ownership"
                            ));
                        }
                        Ok(Err(err)) => {
                            drop(execution);
                            break Err(err.context(
                                "Failed to refresh immediate CVE scan execution heartbeat",
                            ));
                        }
                        Err(_) => {
                            drop(execution);
                            break Err(anyhow::anyhow!(
                                "Timed out refreshing immediate CVE scan {scan_id} execution heartbeat"
                            ));
                        }
                    }
                }
            }
        };

        if let Err(err) = result {
            if let Err(mark_err) = mark_cve_scan_failed_by_id_for_execution(
                &spawn_pool,
                scan_id,
                derivation_id,
                &err.to_string(),
                claim.execution_id,
            )
            .await
            {
                error!(
                    "Failed to mark immediate CVE scan {scan_id} as failed after setup error: {mark_err:#}"
                );
            }
            error!("Immediate CVE scan task failed for derivation {derivation_id}: {err:#}");
        }
    });

    Ok(scan_id)
}
