use crystal_forge::builder::spawn_background_tasks;
use crystal_forge::models::config::CrystalForgeConfig;
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
    let builder_cfg = cfg
        .build
        .as_ref()
        .expect("missing [build] section in config");

    let pool = CrystalForgeConfig::db_pool().await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let builder_pool = pool.clone();
    spawn_background_tasks(builder_pool);

    Ok(())
}
