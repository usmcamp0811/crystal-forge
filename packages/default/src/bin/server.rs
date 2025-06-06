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
use ed25519_dalek::{Signature, VerifyingKey};
use std::{collections::HashMap, fs};
use tokio::net::TcpListener;

/// Holds the loaded public keys for authorized agents.
#[derive(Clone)]
struct AppState {
    /// Map of agent identifiers to their Ed25519 verifying keys.
    authorized_keys: HashMap<String, Result<VerifyingKey, _>>,
}

/// Entry point for the server. Initializes configuration, tracing, and starts the Axum HTTP server.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load and validate config

    let cfg = config::load_config()?;
    let db_url = cfg.database.to_url();

    config::validate_db_connection(&db_url).await?;

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
fn parse_authorized_keys(b64_keys: &[String]) -> anyhow::Result<HashMap<String, VerifyingKey>> {
    let mut map = HashMap::new();

    for (i, b64) in b64_keys.iter().enumerate() {
        let bytes = base64::decode(b64.trim())
            .with_context(|| format!("Invalid base64 key at index {}", i))?;

        let key = VerifyingKey::from_bytes(bytes.as_slice().try_into()?)
            .context(format!("Invalid base64 key at index {}", i))?;
        map.insert(format!("agent-{i}"), key);
    }

    Ok(map)
}

/// HTTP handler for `/current-system` POST requests.
///
/// This endpoint:
/// 1. Extracts the `X-Key-ID` and `X-Signature` headers.
/// 2. Verifies the signature against the request body using the key from authorized keys.
/// 3. If valid, prints the payload; otherwise returns an error status.
///
/// # Arguments
///
/// * `State(state)` - Shared application state (authorized keys)
/// * `headers` - HTTP headers from the request
/// * `body` - Raw request body (e.g., hostname:system_hash)
///
/// # Returns
///
/// * `200 OK` if the signature is valid
/// * `401 Unauthorized` if key is missing or signature is invalid
/// * `400 Bad Request` if input formatting is invalid
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

    // Convert to Signature type
    let bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let signature = match Signature::from_bytes(&bytes) {
        Ok(sig) => sig,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    // Look up the key
    let key = match state.authorized_keys.get(key_id) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    // Verify signature
    if key.verify(&body, &signature).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    // Success
    println!(
        "✅ accepted from {key_id}: {:?}",
        String::from_utf8_lossy(&body)
    );
    StatusCode::OK
}
