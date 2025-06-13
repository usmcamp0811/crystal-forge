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
use crystal_forge::flake_watcher::{get_nixos_configurations, stream_derivations};
use crystal_forge::system_watcher::SystemPayload;
use crystal_forge::webhook_handler::BoxedHandler;
use crystal_forge::webhook_handler::webhook_handler;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use std::ffi::OsStr;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::{collections::HashMap, fs};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Holds the loaded public keys for authorized agents.
#[derive(Clone)]
struct CFState {
    /// Map of agent identifiers to their Ed25519 verifying keys.
    authorized_keys: HashMap<String, VerifyingKey>,
}

/// Entry point for the server. Initializes configuration, tracing, and starts the Axum HTTP server.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load and validate config

    let cfg = config::load_config()?;

    // TODO: clean this up.. maybe I dunno
    let db_url = cfg
        .database
        .as_ref()
        .expect("missing [database] section in config")
        .to_url();

    config::validate_db_connection(&db_url).await?;

    // ensure DB table exists (idempotent)
    init_db().await?;

    // always insert the flakes from the config
    // it has a unique constraint so we are fine
    if let Some(watched) = &cfg.flakes {
        for (name, repo_url) in &watched.watched {
            insert_flake(name, repo_url).await?;
        }
    }

    println!("Starting Crystal Forge Server...");
    println!("Host: {}", "0.0.0.0");

    let server_cfg = cfg
        .server
        .as_ref()
        .expect("missing [server] section in config");

    println!("Port: {}", server_cfg.port);

    // Set up tracing/logging
    tracing_subscriber::fmt::init();

    // Load authorized public keys for agent verification
    let authorized_keys = parse_authorized_keys(&server_cfg.authorized_keys)?;
    let state = CFState {
        authorized_keys: authorized_keys,
    };

    let get_configs_boxed =
        |repo_url: String| Box::pin(get_nixos_configurations(repo_url)) as Pin<Box<_>>;

    let insert_derivation_hash_boxed = |a: String, b: String, c: String, d: String| {
        Box::pin(async move { insert_derivation_hash(&a, &b, &c, &d).await }) as Pin<Box<_>>
    };

    let stream_derivations_boxed =
        |systems: Vec<String>, flake_url: &str, handler: Arc<Mutex<BoxedHandler>>| {
            let flake_url = flake_url.to_string();
            Box::pin(async move {
                let _ = stream_derivations(systems, &flake_url, handler).await;
            }) as Pin<Box<dyn Future<Output = ()> + Send>>
        };

    let insert_commit_boxed = |commit: &str, repo: &str| {
        let commit = commit.to_owned();
        let repo = repo.to_owned();
        Box::pin(async move { insert_commit(&commit, &repo).await })
            as Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
    };

    let webhook_handler_wrap = {
        let get_configs_boxed = get_configs_boxed.clone();
        let insert_commit_boxed = insert_commit_boxed.clone();
        let stream_derivations_boxed = stream_derivations_boxed.clone();
        let insert_derivation_hash_boxed = insert_derivation_hash_boxed.clone();

        move |Json(payload): Json<Value>| {
            let get_configs_boxed = get_configs_boxed.clone();
            let insert_commit_boxed = insert_commit_boxed.clone();
            let stream_derivations_boxed = stream_derivations_boxed.clone();
            let insert_derivation_hash_boxed = insert_derivation_hash_boxed.clone();

            async move {
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
    // Define routes and shared state
    let app = Router::new()
        .route("/current-system", post(handle_current_system))
        .route("/webhook", post(webhook_handler_wrap))
        .with_state(state);

    // Start server
    let listener = TcpListener::bind(("0.0.0.0", server_cfg.port)).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Parses a list of base64-encoded Ed25519 public keys into a map of agent identifiers.
///
/// Each key is expected to be a valid 32-byte Ed25519 public key encoded in base64 format.
/// The returned map assigns default IDs in the format `agent-0`, `agent-1`, etc.
///
/// # Arguments
///
/// * `b64_keys` - A slice of strings, each containing a base64-encoded public key
///
/// # Returns
///
/// * `Ok(HashMap<String, VerifyingKey>)` - A map of agent names to their parsed verifying keys
/// * `Err(anyhow::Error)` - If any key is invalid base64 or not a valid Ed25519 key
///
/// # Example
///
/// ```toml
/// authorized_keys = [
///   "Base64EncodedKey1",
///   "Base64EncodedKey2"
/// ]
/// ```
///
/// ```rust
/// let map = parse_authorized_keys(&config.server.authorized_keys)?;
/// let key = map.get("agent-0").unwrap();
/// ```

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

/// HTTP handler for `/current-system` POST requests.
///
/// This endpoint:
/// 1. Extracts the `X-Key-ID` and `X-Signature` headers.
/// 2. Verifies the signature of the raw JSON request body using the corresponding Ed25519 public key.
/// 3. Parses the request body as JSON into a `SystemPayload` struct.
/// 4. Logs and stores the verified system state in the database.
///
/// # Request
///
/// Headers:
/// - `X-Key-ID`: The identifier for the public key to verify the signature
/// - `X-Signature`: The base64-encoded Ed25519 signature of the request body
///
/// Body:
/// - A JSON object:
///   {
///     "hostname": "my-host",
///     "system_hash": "abc123",
///     "context": "startup",
///     "fingerprint": { /* hardware & OS info */ }
///   }
///
/// # Response
///
/// - `200 OK`: If the signature is valid and the data is successfully stored
/// - `400 Bad Request`: If the signature or payload is invalid
/// - `401 Unauthorized`: If the key ID is missing or the signature verification fails
/// - `500 Internal Server Error`: If insertion into the database fails
async fn handle_current_system(
    State(state): State<CFState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let key_id = match headers.get("X-Key-ID") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    let sig = match headers.get("X-Signature") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    let signature_bytes = match general_purpose::STANDARD.decode(sig) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let signature = Signature::from_bytes(&bytes);
    let key = match state.authorized_keys.get(key_id) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    if key.verify(&body, &signature).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    // Parse JSON
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
