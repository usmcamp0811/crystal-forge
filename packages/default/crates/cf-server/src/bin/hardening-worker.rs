use crystal_forge::config::{CrystalForgeConfig, db_pool, validate_db_connection};
use crystal_forge::services::hardening_scans::run_hardening_scan_queue;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = CrystalForgeConfig::load()?;
    cfg.server.validate().map_err(anyhow::Error::msg)?;
    validate_db_connection().await?;
    let pool = db_pool().await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Automatic admission policy is configuration-owned. The worker reads it
    // once here and passes it explicitly into the queue loop, which gates the
    // bounded backfill. Manually requested scans run regardless of this value.
    let auto_hardening_scans = cfg.server.auto_hardening_scans;

    info!(
        auto_hardening_scans,
        "Starting Crystal Forge hardening worker"
    );
    run_hardening_scan_queue(pool, auto_hardening_scans).await;
    Ok(())
}
