use crystal_forge::builder::{run_build_loop, run_cache_push_loop, run_cve_scan_loop};
use crystal_forge::models::config::CrystalForgeConfig;
use crystal_forge::server::memory_monitor_task;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = CrystalForgeConfig::load()?;
    CrystalForgeConfig::validate_db_connection().await?;

    info!("Starting Crystal Forge Builder...");
    let pool = CrystalForgeConfig::db_pool().await?;

    tokio::spawn(memory_monitor_task(pool.clone()));
    sqlx::migrate!("./migrations").run(&pool).await?;

    let cache_config = &cfg.cache;

    let build_handle = tokio::spawn(run_build_loop(pool.clone()));
    let cve_scan_handle = tokio::spawn(run_cve_scan_loop(pool.clone()));

    if cache_config.push_after_build {
        let cache_handle = tokio::spawn(run_cache_push_loop(pool.clone()));
        info!("✅ Build, CVE scan, and cache push loops started");

        tokio::select! {
            result = build_handle => {
                error!("Build loop exited unexpectedly: {:?}", result);
            }
            result = cve_scan_handle => {
                error!("CVE scan loop exited unexpectedly: {:?}", result);
            }
            result = cache_handle => {
                error!("Cache push loop exited unexpectedly: {:?}", result);
            }
            _ = signal::ctrl_c() => {
                info!("Received shutdown signal");
            }
        }
    } else {
        info!("📤 Cache push disabled in configuration");
        info!("✅ Build and CVE scan loops started");

        tokio::select! {
            result = build_handle => {
                error!("Build loop exited unexpectedly: {:?}", result);
            }
            result = cve_scan_handle => {
                error!("CVE scan loop exited unexpectedly: {:?}", result);
            }
            _ = signal::ctrl_c() => {
                info!("Received shutdown signal");
            }
        }
    }

    info!("Shutting down Crystal Forge Builder...");
    Ok(())
}
