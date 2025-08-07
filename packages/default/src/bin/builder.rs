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
    tokio::select! {
        result = build_handle => {
            tracing::error!("Build loop exited unexpectedly: {:?}", result);
        }
        result = cve_scan_handle => {
            tracing::error!("CVE scan loop exited unexpectedly: {:?}", result);
        }
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    info!("Shutting down Crystal Forge Builder...");

    Ok(())
}
