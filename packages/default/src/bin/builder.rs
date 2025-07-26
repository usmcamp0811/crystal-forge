use anyhow::Context;
use axum::{Router, routing::post};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::builder::spawn_background_tasks;
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

    // Load and validate config
    let cfg = CrystalForgeConfig::load()?;
    CrystalForgeConfig::validate_db_connection().await?;
    info!("Starting Crystal Forge Builder...");
    let builder_cfg = cfg
        .build
        .as_ref()
        .expect("missing [build] section in config");

    let pool = CrystalForgeConfig::db_pool().await?;
    let builder_pool = pool.clone();
    reset_non_complete_targets(&pool);
    spawn_background_tasks(builder_pool);

    // Start HTTP server

    Ok(())
}
