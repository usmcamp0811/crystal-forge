/// Main entry point for the Crystal Forge server.
/// Sets up database, loads config, inserts watched flakes, initializes routes,
/// and starts the Axum HTTP server.
use anyhow::{Context, Result};
use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::flake_watcher::get_system_derivation;
use crystal_forge::{
    config,
    db::{
        init_db, insert_commit, insert_derivation_hash, insert_flake, insert_system_name,
        insert_system_state,
    },
    flake_watcher::{get_nixos_configurations_at_commit, stream_derivations},
    system_watcher::SystemPayload,
    webhook_handler::{BoxedHandler, webhook_handler},
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::EnvFilter;

/// Shared server state containing authorized signing keys for current-system auth
#[derive(Clone)]
struct CFState {
    authorized_keys: HashMap<String, VerifyingKey>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()) // uses RUST_LOG
        .init();
    // Load and validate config
    let cfg = config::load_config()?;
    let db_url = cfg
        .database
        .as_ref()
        .expect("missing [database] section in config")
        .to_url();
    config::validate_db_connection(&db_url).await?;
    init_db().await?;

    // Insert any statically watched flakes into the database
    if let Some(watched) = &cfg.flakes {
        for (name, repo_url) in &watched.watched {
            insert_flake(name, repo_url).await?;
        }
    }

    // maybe we got a webhook trigger and for some reason something shit the bed and we were unable
    // to get the systems in the commit; we don't want to just go the rest of our lives never knowing
    // so we should at startup always go see if we can do better.
    tokio::spawn(async {
        if let Ok(entries) = crystal_forge::db::get_commits_without_systems().await {
            for (commit, flake_url, flake_name) in entries {
                tracing::info!("📦 queued eval: {flake_name} {flake_url} {commit}");
                let result: std::result::Result<Vec<String>, anyhow::Error> =
                    get_nixos_configurations_at_commit(&flake_url, &commit).await;

                match result {
                    Ok(configs) => {
                        for system_name in &configs {
                            if let Err(e) =
                                insert_system_name(&commit, &flake_url, system_name).await
                            {
                                tracing::error!(
                                    "❌ Failed to insert system name {system_name}: {e:?}"
                                );
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ get_nixos_configurations_at_commit failed: {e}");
                    }
                }
            }
        }
    });

    // Check if we have any systems that dont have derivation_hash values.
    // This is meant to be a catch all in case we crash or otherwise restart
    // during the processing of a webhook.
    tokio::spawn(async {
        if let Ok(entries) = crystal_forge::db::get_systems_to_eval().await {
            for (flake_name, flake_url, commit, system_name) in entries {
                tracing::info!("📦 queued eval: {flake_name} {flake_url} {commit} {system_name}");
                let result: std::result::Result<String, anyhow::Error> =
                    get_system_derivation(&system_name, &flake_url, &commit).await;
                match result {
                    Ok(hash) => {
                        if let Err(e) =
                            insert_derivation_hash(&system_name, &hash, &flake_url, &commit).await
                        {
                            tracing::error!("❌ insert_derivation_hash failed: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("❌ get_system_derivation failed: {e}");
                    }
                }
            }
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
    let state = CFState { authorized_keys };

    // Wrap database insert for derivation hash

    let insert_derivation_hash_boxed =
        |_commit: String, _repo: String, _system: String, _hash: String| {
            Box::pin(async move { Ok(()) })
                as Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        };

    // Wrap commit insert
    let insert_commit_boxed = |commit: &str, repo: &str| {
        let commit = commit.to_owned();
        let repo = repo.to_owned();
        Box::pin(async move { insert_commit(&commit, &repo).await })
            as Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
    };

    let insert_system_name_boxed = |commit_hash: String, repo_url: String, system_name: String| {
        Box::pin(async move { insert_system_name(&commit_hash, &repo_url, &system_name).await })
            as Pin<Box<_>>
    };
    /// Handler function for the `/webhook` route. Extracts payload,
    /// builds config lookup closure, and invokes main `webhook_handler`.
    let webhook_handler_wrap = {
        let insert_commit_boxed = insert_commit_boxed.clone();
        let insert_derivation_hash_boxed = insert_derivation_hash_boxed.clone();

        move |Json(payload): Json<Value>| {
            let insert_commit_boxed = insert_commit_boxed.clone();
            let insert_derivation_hash_boxed = insert_derivation_hash_boxed.clone();

            async move {
                // Extract repo URL and commit hash
                let Some(repo_url) = payload
                    .pointer("/repository/clone_url")
                    .or_else(|| payload.pointer("/project/web_url"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                else {
                    return StatusCode::BAD_REQUEST;
                };

                let Some(commit_hash) = payload
                    .pointer("/after")
                    .or_else(|| payload.pointer("/checkout_sha"))
                    .and_then(|v| v.as_str())
                    .map(String::from)
                else {
                    return StatusCode::BAD_REQUEST;
                };

                // ✅ define stream_derivations_boxed now that commit_hash is available
                let insert_system_fn = Arc::new(insert_system_name_boxed.clone());

                let stream_derivations_boxed = {
                    let commit = commit_hash.clone();
                    move |systems: Vec<String>,
                          flake_url: &str,
                          handler: Arc<Mutex<BoxedHandler>>| {
                        let flake_url = flake_url.to_string();
                        let commit = commit.clone();
                        let insert_system_fn = insert_system_fn.clone();
                        Box::pin(async move {
                            stream_derivations(
                                systems,
                                &flake_url,
                                &commit,
                                insert_system_fn,
                                handler,
                            )
                            .await
                        })
                            as Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
                    }
                };

                let get_configs_boxed = {
                    let commit_hash = commit_hash.clone();
                    Box::new(move |repo_url: String| {
                        let commit = commit_hash.clone();
                        let repo = repo_url.clone();
                        Box::pin(
                            async move { get_nixos_configurations_at_commit(&repo, &commit).await },
                        )
                            as Pin<
                                Box<dyn Future<Output = Result<Vec<String>, anyhow::Error>> + Send>,
                            >
                    })
                };

                webhook_handler(
                    payload,
                    insert_commit_boxed,
                    get_configs_boxed,
                    stream_derivations_boxed,
                    insert_derivation_hash_boxed,
                )
                .await
            }
        }
    };

    // Define application routes and state
    let app = Router::new()
        .route("/current-system", post(handle_current_system))
        .route("/webhook", post(webhook_handler_wrap))
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

        let key_bytes: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .context("Failed to convert to [u8; 32]")?;

        let key = VerifyingKey::from_bytes(&key_bytes)
            .context(format!("Invalid public key for ID '{}'", key_id))?;

        map.insert(key_id.clone(), key);
    }

    Ok(map)
}

/// Handles the `/current-system` POST route.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
async fn handle_current_system(
    State(state): State<CFState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Extract and validate key ID and signature from headers
    let key_id = match headers.get("X-Key-ID") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    let sig = match headers.get("X-Signature") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    // Decode and validate signature format
    let signature_bytes = match general_purpose::STANDARD.decode(sig) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let signature = Signature::from_bytes(&bytes);

    // Lookup key for signature verification
    let key = match state.authorized_keys.get(key_id) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    if key.verify(&body, &signature).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    // Deserialize JSON payload
    let payload: SystemPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "❌ JSON deserialization failed: {e}\nBody:\n{}",
                String::from_utf8_lossy(&body)
            );
            return StatusCode::BAD_REQUEST;
        }
    };

    info!(
        "✅ accepted from {key_id}: hostname={}, hash={}, context={}, fingerprint={:?}",
        payload.hostname, payload.system_hash, payload.context, payload.fingerprint
    );

    // Insert system state into DB
    if let Err(e) = insert_system_state(
        &payload.hostname,
        &payload.context,
        &payload.system_hash,
        &payload.fingerprint,
    )
    .await
    {
        eprintln!("❌ failed to insert into DB: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}
