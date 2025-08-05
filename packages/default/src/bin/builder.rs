use crystal_forge::builder::{run_build_loop, run_cve_scan_loop};
use crystal_forge::models::config::CrystalForgeConfig;
use crystal_forge::server::memory_monitor_task;
use tokio::signal;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()) // uses RUST_LOG
        .init();

    // Load and validate config
    let cfg = CrystalForgeConfig::load()?;
    CrystalForgeConfig::validate_db_connection().await?;

    info!("Starting Crystal Forge Builder...");

    let pool = CrystalForgeConfig::db_pool().await?;
    tokio::spawn(memory_monitor_task(pool.clone()));
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Spawn both loops
    let build_handle = tokio::spawn(run_build_loop(pool.clone()));
    let cve_scan_handle = tokio::spawn(run_cve_scan_loop(pool.clone()));

    info!("✅ Both build and CVE scan loops started");

    // Wait for either task to complete (they shouldn't under normal circumstances)

    // After spawning tasks...
    info!("✅ Both build and CVE scan loops started");

    // Wait for shutdown signal instead of task completion
    // After spawning tasks...
    info!("✅ Both build and CVE scan loops started");

    // Wait for shutdown signal instead of task completion
    tokio::select! {
        result = build_handle => {
            match result {
                Ok(Ok(())) => {
                    tracing::error!("Build loop completed normally (this shouldn't happen)");
                }
                Ok(Err(e)) => {
                    tracing::error!("Build loop returned error: {}", e);
                }
                Err(e) => {
                    tracing::error!("Build loop panicked: {}", e);
                }
            }
        }
        result = cve_scan_handle => {
            match result {
                Ok(Ok(())) => {
                    tracing::error!("CVE scan loop completed normally (this shouldn't happen)");
                }
                Ok(Err(e)) => {
                    tracing::error!("CVE scan loop returned error: {}", e);
                }
                Err(e) => {
                    tracing::error!("CVE scan loop panicked: {}", e);
                }
            }
        }
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
            return Ok(());
        }
    }

    tracing::error!("One of the loops exited - this indicates a bug in the loop implementation");

    info!("Shutting down Crystal Forge Builder...");

    Ok(())
}
