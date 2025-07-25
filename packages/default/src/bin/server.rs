use anyhow::Context;
use axum::{Router, routing::post};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::flake::eval::list_nixos_configurations_from_commit;
use crystal_forge::handlers::agent::heartbeat;
use crystal_forge::handlers::agent::state;
use crystal_forge::handlers::agent_request::CFState;
use crystal_forge::handlers::webhook::webhook_handler;
use crystal_forge::models::config::CrystalForgeConfig;
use crystal_forge::queries::commits::get_commits_pending_evaluation;
use crystal_forge::queries::evaluation_targets::reset_non_complete_targets;
use crystal_forge::queries::evaluation_targets::{
    get_pending_targets, increment_evaluation_target_attempt_count, insert_evaluation_target,
    update_evaluation_target_path,
};
use crystal_forge::queries::flakes::insert_flake;
use crystal_forge::server::spawn_server_background_tasks;
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use tokio::net::TcpListener;
use tracing::{debug, info};
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
    sqlx::migrate!("./migrations").run(&pool).await?;
    cfg.sync_systems_to_db(&pool).await?;
    let background_pool = pool.clone();
    reset_non_complete_targets(&pool);
    spawn_server_background_tasks(background_pool);

    // Start HTTP server
    info!("Starting Crystal Forge Server...");
    let server_cfg = cfg
        .server
        .as_ref()
        .expect("missing [server] section in config");
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
