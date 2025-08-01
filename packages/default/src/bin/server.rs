use anyhow::Context;
use axum::{Router, routing::post};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::{
    flake::commits::initialize_flake_commits,
    handlers::{
        agent::{heartbeat, state},
        agent_request::CFState,
        webhook::webhook_handler,
    },
    models::config::CrystalForgeConfig,
    queries::{
        commits::get_commits_pending_evaluation, evaluation_targets::reset_non_terminal_targets,
    },
    server::memory_monitor_task,
    server::spawn_background_tasks,
};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use tokio::net::TcpListener;

use tokio::time::{Duration, interval};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()) // uses RUST_LOG
        .init();

    println!("Crystal Forge: Starting...");

    // Load and validate config
    let cfg = CrystalForgeConfig::load()?;
    CrystalForgeConfig::validate_db_connection().await?;

    debug!("======== INITIALIZING DATABASE ========");
    let pool = CrystalForgeConfig::db_pool().await?;
    tokio::spawn(memory_monitor_task(pool.clone()));
    sqlx::migrate!("./migrations").run(&pool).await?;
    cfg.sync_systems_to_db(&pool).await?;
    let background_pool = pool.clone();
    let flake_init_pool = pool.clone();
    // TODO: Update this to get the first N commits on the first time
    initialize_flake_commits(&flake_init_pool, &cfg.flakes.watched).await?;
    reset_non_terminal_targets(&pool);
    spawn_background_tasks(cfg.clone(), background_pool);

    // Start HTTP server
    info!("Starting Crystal Forge Server...");
    let server_cfg = &cfg.server;
    info!("Host: 0.0.0.0");
    info!("Port: {}", server_cfg.port);

    let state = CFState::new(pool);
    let app = Router::new()
        .route("/system_state", post(state::update))
        .route("/agent/heartbeat", post(heartbeat::log))
        .route("/agent/state", post(state::update))
        .route("/webhook", post(webhook_handler))
        .with_state(state);

    let listener = TcpListener::bind(("0.0.0.0", server_cfg.port)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Parses base64-encoded public keys from config and converts them to `VerifyingKey`s.
fn parse_authorized_keys(
    b64_keys: &HashMap<String, String>,
) -> anyhow::Result<HashMap<String, VerifyingKey>> {
    let mut map = HashMap::new();

    for (key_id, b64) in b64_keys {
        let bytes = general_purpose::STANDARD
            .decode(b64.trim())
            .with_context(|| format!("Invalid base64 key for ID '{}'", key_id))?;

        if bytes.len() != 32 {
            anyhow::bail!("Key ID '{}' is not 32 bytes (got {})", key_id, bytes.len());
        }

        let key_bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .expect("already checked length == 32");

        let key = VerifyingKey::from_bytes(&key_bytes)
            .context(format!("Invalid public key for ID '{}'", key_id))?;

        map.insert(key_id.clone(), key);
    }

    Ok(map)
}
