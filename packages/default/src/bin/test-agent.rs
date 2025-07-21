use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use clap::Parser;
use crystal_forge::models::config::CrystalForgeConfig;
use crystal_forge::models::system_states::SystemState;
use ed25519_dalek::{Signer, SigningKey};
use reqwest::blocking::Client;
use std::fs;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Hostname for the test agent
    #[arg(short, long)]
    hostname: String,

    /// Change reason (startup, config_change, heartbeat)
    #[arg(short = 'r', long)]
    change_reason: String,

    /// System derivation path
    #[arg(short, long)]
    derivation: String,

    /// Override timestamp (ISO 8601 format, e.g., 2025-07-12T10:30:00Z)
    #[arg(short, long)]
    timestamp: Option<String>,

    /// Server host (default: from config)
    #[arg(long)]
    server_host: Option<String>,

    /// Server port (default: from config)
    #[arg(long)]
    server_port: Option<u16>,

    /// Private key override (base64 encoded)
    #[arg(long)]
    private_key: Option<String>,

    /// OS version override
    #[arg(long)]
    os: Option<String>,

    /// Kernel version override
    #[arg(long)]
    kernel: Option<String>,

    /// Memory GB override
    #[arg(long)]
    memory_gb: Option<f64>,

    /// CPU brand override
    #[arg(long)]
    cpu_brand: Option<String>,

    /// CPU cores override
    #[arg(long)]
    cpu_cores: Option<i32>,
}

fn create_test_payload(args: &Args) -> Result<(SystemState, String, String)> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = cfg.client.expect("client config is required for agent");

    // Parse timestamp if provided
    let timestamp_override = if let Some(ts_str) = &args.timestamp {
        Some(DateTime::parse_from_rfc3339(ts_str)?.with_timezone(&Utc))
    } else {
        None
    };

    // Create SystemState from arguments
    let payload = SystemState::gather_from_args(
        &args.hostname,
        &args.change_reason,
        &args.derivation,
        timestamp_override,
        args.os.as_deref(),
        args.kernel.as_deref(),
        args.memory_gb,
        args.cpu_brand.as_deref(),
        args.cpu_cores,
    )?;

    // Serialize payload
    let payload_json = serde_json::to_string(&payload)?;

    // Load private key (use override or config)
    let private_key_content = if let Some(key) = &args.private_key {
        key.clone()
    } else {
        fs::read_to_string(&client_cfg.private_key)?
            .trim()
            .to_string()
    };

    let key_bytes = STANDARD
        .decode(&private_key_content)
        .context("failed to decode base64 private key")?;
    let signing_key = SigningKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .context("expected a 32-byte Ed25519 private key")?,
    );

    // Sign the payload
    let signature = signing_key.sign(payload_json.as_bytes());
    let signature_b64 = STANDARD.encode(signature.to_bytes());

    Ok((payload, payload_json, signature_b64))
}

fn send_test_request(args: &Args) -> Result<()> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = cfg.client.expect("client config is required for agent");

    let (payload, payload_json, signature_b64) = create_test_payload(args)?;

    // Determine endpoint based on change_reason
    let endpoint = match args.change_reason.as_str() {
        "startup" | "config_change" => "state",
        "heartbeat" => "heartbeat",
        _ => "state", // Default fallback
    };

    // Use overrides or config for server details
    let server_host = args
        .server_host
        .as_deref()
        .unwrap_or(&client_cfg.server_host);
    let server_port = args.server_port.unwrap_or(client_cfg.server_port);

    let client = Client::new();
    let url = format!("http://{}:{}/agent/{}", server_host, server_port, endpoint);

    println!("Sending {} request to: {}", args.change_reason, url);
    println!("Hostname: {}", args.hostname);
    println!("Derivation: {}", args.derivation);
    if let Some(ts) = &args.timestamp {
        println!("Timestamp: {}", ts);
    }

    let res = client
        .post(url)
        .header("X-Signature", signature_b64)
        .header("X-Key-ID", &args.hostname)
        .body(payload_json)
        .send()
        .context("failed to send POST request")?;

    if res.status().is_success() {
        println!("✅ Success: {}", res.status());
    } else {
        println!("❌ Failed: {}", res.status());
        if let Ok(body) = res.text() {
            println!("Response: {}", body);
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    send_test_request(&args)
}
