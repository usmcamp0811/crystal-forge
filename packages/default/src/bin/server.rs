use anyhow::Context;

use axum::{
    Router,
    body::Bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::config;
use crystal_forge::db::insert_system_state;
use ed25519_dalek::Verifier;
use ed25519_dalek::{Signature, VerifyingKey};
use std::ffi::OsStr;
use std::{collections::HashMap, fs};
use tokio::net::TcpListener;

/// Holds the loaded public keys for authorized agents.
#[derive(Clone)]
struct AppState {
    /// Map of agent identifiers to their Ed25519 verifying keys.
    authorized_keys: HashMap<String, VerifyingKey>,
}

/// Entry point for the server. Initializes configuration, tracing, and starts the Axum HTTP server.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load and validate config

    let cfg = config::load_config()?;
    let db_url = cfg.database.to_url();

    config::validate_db_connection(&db_url).await?;

    println!("Starting Crystal Forge Server...");
    println!("Host: {}", "0.0.0.0");
    println!("Port: {}", cfg.server.port);

    // Set up tracing/logging
    tracing_subscriber::fmt::init();

    // Load authorized public keys for agent verification
    let authorized_keys = parse_authorized_keys(&cfg.server.authorized_keys)?;
    let state = AppState {
        authorized_keys: authorized_keys,
    };

    // Define routes and shared state
    let app = Router::new()
        .route("/current-system", post(handle_current_system))
        .with_state(state);

    // Start server
    let listener = TcpListener::bind(("0.0.0.0", cfg.server.port)).await?;
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
/// 2. Verifies the signature of the request body using the corresponding Ed25519 public key.
/// 3. Parses the request body, which is expected to be in the format: `<hostname>:<system_hash>:<fingerprint>`.
/// 4. Logs and stores the verified system state in the database.
///
/// # Request
///
/// Headers:
/// - `X-Key-ID`: The identifier for the public key to verify the signature
/// - `X-Signature`: The base64-encoded Ed25519 signature of the request body
///
/// Body:
/// - A UTF-8 encoded string with format: `hostname:system_hash:fingerprint`
///
/// # Response
///
/// - `200 OK`: If the signature is valid and the data is successfully stored
/// - `400 Bad Request`: If the signature is malformed or the payload is invalid
/// - `401 Unauthorized`: If the key ID is missing or the signature is invalid
/// - `500 Internal Server Error`: If insertion into the database fails
async fn handle_current_system(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Extract the Key ID
    let key_id = match headers.get("X-Key-ID") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    // Extract the base64 signature
    let sig = match headers.get("X-Signature") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    // Decode base64 signature
    let signature_bytes = match general_purpose::STANDARD.decode(sig) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let signature = Signature::from_bytes(&bytes);

    // Look up the key
    let key = match state.authorized_keys.get(key_id) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    // Verify signature
    if key.verify(&body, &signature).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    // Decode and split payload
    let payload = String::from_utf8_lossy(&body);
    let parts: Vec<&str> = payload.split(':').collect();

    if parts.len() != 3 {
        return StatusCode::BAD_REQUEST;
    }

    let hostname = parts[0];
    let system_hash: &OsStr = OsStr::new(parts[1]);
    let fingerprint = parts[2];

    println!(
        "✅ accepted from {key_id}: hostname={hostname}, hash={}, fingerprint={fingerprint}",
        system_hash.to_string_lossy()
    );

    // insert into db (adapt insert function accordingly)
    if let Err(e) = insert_system_state(hostname, system_hash, fingerprint).await {
        eprintln!("❌ failed to insert into DB: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}
