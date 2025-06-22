use anyhow::Context;
use axum::{Router, routing::post};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::flake::eval::list_nixos_configurations_from_commit;
use crystal_forge::handlers::current_system::{CFState, handle_current_system};
use crystal_forge::handlers::webhook::webhook_handler;
use crystal_forge::queries::commits::get_commits_pending_evaluation;
use crystal_forge::queries::evaluation_targets::{
    get_pending_targets, insert_evaluation_target, update_evaluation_target_path,
};
use crystal_forge::queries::flakes::insert_flake;
use crystal_forge::{config, db::get_db_client};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()) // uses RUST_LOG
        .init();
    println!("Crystal Forge: Starting...");
    // Load and validate config
    let cfg = config::load_config()?;
    config::debug_print_config(&cfg);
    let db_url = cfg
        .database
        .as_ref()
        .expect("missing [database] section in config")
        .to_url();
    config::validate_db_connection(&db_url).await?;

    info!("======== INITIALIZING DATABASE ========");
    let pool = get_db_client().await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Insert any statically watched flakes into the database
    if let Some(watched) = &cfg.flakes {
        for (name, repo_url) in &watched.watched {
            insert_flake(&pool, name, repo_url).await?;
        }
    }

    // add systems to evaluation target table w/o eval
    tokio::spawn(async move {
        let pool = get_db_client().await.expect("db");
        loop {
            match get_commits_pending_evaluation(&pool).await {
                Ok(pending_commits) => {
                    for commit in pending_commits {
                        let target_type = "nixos";
                        match list_nixos_configurations_from_commit(&pool, &commit).await {
                            Ok(nixos_targets) => {
                                for target_name in nixos_targets {
                                    if let Err(e) = insert_evaluation_target(
                                        &pool,
                                        &commit,
                                        &target_name,
                                        target_type,
                                    )
                                    .await
                                    {
                                        tracing::error!(
                                            "❌ Failed to insert evaluation target for {}: {}",
                                            target_name,
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "❌ Failed to list nixos configurations for commit {}: {}",
                                    commit.git_commit_hash,
                                    e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("❌ Failed to get pending targets: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    // add evaluation target derivation paths to table
    tokio::spawn(async move {
        let pool = get_db_client().await.expect("db");
        loop {
            match get_pending_targets(&pool).await {
                Ok(pending_targets) => {
                    for mut target in pending_targets {
                        match target.resolve_derivation_path().await {
                            Ok(path) => {
                                match update_evaluation_target_path(&pool, &target, &path).await {
                                    Ok(updated) => tracing::info!("✅ Updated: {:?}", updated),
                                    Err(e) => tracing::error!("❌ Failed to update path: {e}"),
                                }
                            }
                            Err(e) => {
                                tracing::error!("❌ Failed to resolve derivation path: {e}");
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("❌ Failed to get pending targets: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });

    // Start logs and diagnostics
    info!("Starting Crystal Forge Server...");
    let server_cfg = cfg
        .server
        .as_ref()
        .expect("missing [server] section in config");
    info!("Host: 0.0.0.0");
    info!("Port: {}", server_cfg.port);

    // Decode and parse all authorized public keys
    let authorized_keys = parse_authorized_keys(&server_cfg.authorized_keys)?;
    let pool = get_db_client().await?;
    let state = CFState::new(pool, authorized_keys);

    // Define application routes and state
    let app = Router::new()
        .route("/current-system", post(handle_current_system))
        .route("/webhook", post(webhook_handler))
        .with_state(state);

    // Bind TCP listener and start serving
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
