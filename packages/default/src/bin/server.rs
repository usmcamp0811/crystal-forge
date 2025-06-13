/// Main entry point for the Crystal Forge server.
/// Sets up database, loads config, inserts watched flakes, initializes routes,
/// and starts the Axum HTTP server.
use anyhow::Context;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::config;
use crystal_forge::db::{
    init_db, insert_commit, insert_derivation_hash, insert_flake, insert_system_state,
};
use crystal_forge::flake_watcher::{get_nixos_configurations_at_commit, stream_derivations};
use crystal_forge::system_watcher::SystemPayload;
use crystal_forge::webhook_handler::{BoxedHandler, webhook_handler};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use std::{collections::HashMap, ffi::OsStr, fs, future::Future, pin::Pin, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};

/// Shared server state containing authorized signing keys for current-system auth
#[derive(Clone)]
struct CFState {
    authorized_keys: HashMap<String, VerifyingKey>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    // Start logs and diagnostics
    println!("Starting Crystal Forge Server...");
    let server_cfg = cfg
        .server
        .as_ref()
        .expect("missing [server] section in config");
    println!("Host: 0.0.0.0");
    println!("Port: {}", server_cfg.port);
    tracing_subscriber::fmt::init();

    // Decode and parse all authorized public keys
    let authorized_keys = parse_authorized_keys(&server_cfg.authorized_keys)?;
    let state = CFState { authorized_keys };

    // Wrap database insert for derivation hash
    let insert_derivation_hash_boxed = |a: String, b: String, c: String, d: String| {
        Box::pin(async move { insert_derivation_hash(&a, &b, &c, &d).await }) as Pin<Box<_>>
    };

    // Wrap derivation streamer with boxed output
    let stream_derivations_boxed =
        |systems: Vec<String>, flake_url: &str, handler: Arc<Mutex<BoxedHandler>>| {
            let flake_url = flake_url.to_string();
            Box::pin(async move {
                let _ = stream_derivations(systems, &flake_url, handler).await;
            }) as Pin<Box<dyn Future<Output = ()> + Send>>
        };

    // Wrap commit insert
    let insert_commit_boxed = |commit: &str, repo: &str| {
        let commit = commit.to_owned();
        let repo = repo.to_owned();
        Box::pin(async move { insert_commit(&commit, &repo).await })
            as Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
    };

    /// Handler function for the `/webhook` route. Extracts payload,
    /// builds config lookup closure, and invokes main `webhook_handler`.
    let webhook_handler_wrap = {
        let insert_commit_boxed = insert_commit_boxed.clone();
        let stream_derivations_boxed = stream_derivations_boxed.clone();
        let insert_derivation_hash_boxed = insert_derivation_hash_boxed.clone();

        move |Json(payload): Json<Value>| {
            let insert_commit_boxed = insert_commit_boxed.clone();
            let stream_derivations_boxed = stream_derivations_boxed.clone();
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

                // Wrap configuration fetcher with commit context
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

    println!(
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
