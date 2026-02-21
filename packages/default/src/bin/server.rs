use anyhow::Context;
use axum::Extension;
use axum::{
    Router,
    routing::{delete, get, post},
};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::{
    auth::dev_mode::ensure_dev_users,
    config::CrystalForgeConfig,
    flake::commits::initialize_flake_commits,
    handlers::{
        agent::{heartbeat, state},
        agent_request::CFState,
        api::{auth_dev, auth_oidc, auth_session, dashboard, flakes},
        status,
        webhook::webhook_handler,
    },
    queries::derivations::reset_non_terminal_derivations,
    server::memory_monitor_task,
    server::spawn_background_tasks,
};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;

use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[cfg(feature = "embedded-ui")]
use crystal_forge::handlers::ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()) // uses RUST_LOG
        .init();

    println!("Crystal Forge: Starting...");

    // Load and validate config
    let cfg = CrystalForgeConfig::load()?;
    CrystalForgeConfig::validate_db_connection().await?;

    // Validate auth mode and apply production guard
    let auth_mode = &cfg.server.auth_mode;
    if auth_mode == "dev" {
        #[cfg(not(debug_assertions))]
        {
            anyhow::bail!(
                "AUTH_MODE=dev is not allowed in release builds. \
                 Use AUTH_MODE=oidc for production deployments."
            );
        }

        #[cfg(debug_assertions)]
        {
            warn!("⚠️  Running in development auth mode (AUTH_MODE=dev)");
            warn!("⚠️  This mode is insecure and should NEVER be used in production");
        }
    }

    debug!("======== INITIALIZING DATABASE ========");
    let pool = CrystalForgeConfig::db_pool().await?;
    tokio::spawn(memory_monitor_task(pool.clone()));
    sqlx::migrate!("./migrations").run(&pool).await?;
    cfg.sync_systems_to_db(&pool).await?;

    // Initialize dev mode fixtures if AUTH_MODE=dev
    if auth_mode == "dev" {
        info!("Initializing development auth fixtures...");
        ensure_dev_users(&pool)
            .await
            .context("Failed to initialize dev auth fixtures")?;
    }
    let background_pool = pool.clone();
    let deployment_pool = pool.clone();
    let flake_init_pool = pool.clone();
    // TODO: Update this to get the first N commits on the first time
    reset_non_terminal_derivations(&pool).await?;
    initialize_flake_commits(&flake_init_pool, &cfg.flakes.watched).await?;
    spawn_background_tasks(cfg.clone(), background_pool);

    // Start HTTP server
    info!("Starting Crystal Forge Server...");
    let server_cfg = &cfg.server;
    info!("Host: 0.0.0.0");
    info!("Port: {}", server_cfg.port);

    let state = CFState::new(pool);
    let mut app = Router::new()
        .route("/status", get(status::status))
        .route("/system_state", post(state::update))
        .route("/agent/heartbeat", post(heartbeat::log))
        .route("/agent/state", post(state::update))
        .route("/webhook", post(webhook_handler))
        // REST API v1
        .route(
            "/api/v1/dashboard/summary",
            get(dashboard::dashboard_summary),
        )
        .route("/api/v1/flakes", get(flakes::list_flakes))
        .route("/api/v1/flakes", post(flakes::create_flake))
        .route("/api/v1/flakes/:id", delete(flakes::delete_flake));

    // Dev-mode auth routes (only available when AUTH_MODE=dev)
    if auth_mode == "dev" {
        info!("Registering development auth endpoints at /api/auth/dev/*");
        app = app.route("/api/auth/dev/login", post(auth_dev::dev_login));
    } else if auth_mode == "oidc" {
        info!("Registering OIDC auth endpoints at /api/auth/oidc/*");
        let oidc_config = crystal_forge::config::OidcConfig::from_env()
            .context("Failed to load OIDC configuration from environment")?;
        let oidc_state = Arc::new(auth_oidc::OidcClientState::new(oidc_config).await?);

        let oidc_router = Router::new()
            .route("/api/auth/oidc/login", get(auth_oidc::oidc_login))
            .route("/api/auth/oidc/callback", get(auth_oidc::oidc_callback))
            .layer(Extension(oidc_state));

        app = app
            .merge(oidc_router)
            .route("/api/auth/logout", post(auth_session::logout));
    }

    #[cfg(feature = "embedded-ui")]
    {
        app = app.fallback(get(ui::serve_ui));
    }

    let app = app.with_state(state);

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
