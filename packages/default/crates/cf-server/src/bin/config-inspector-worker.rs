use crystal_forge::config::{CrystalForgeConfig, db_pool, validate_db_connection};
use crystal_forge::services::config_inspections::{
    run_config_inspection_queue, should_run_config_inspection_worker,
};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = CrystalForgeConfig::load()?;
    cfg.server.validate().map_err(anyhow::Error::msg)?;
    if !should_run_config_inspection_worker(cfg.server.execution_mode.is_mock()) {
        info!("Config Inspector worker is disabled in mock execution mode");
        return Ok(());
    }

    validate_db_connection().await?;
    let pool = db_pool().await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    info!("Starting Crystal Forge Config Inspector worker");
    run_config_inspection_queue(pool).await;
    Ok(())
}
