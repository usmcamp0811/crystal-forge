use anyhow::Context;
use axum::Extension;
use axum::http::{
    HeaderValue, Method,
    header::{ACCEPT, CONTENT_TYPE, HeaderName},
};
use axum::{
    Router,
    routing::{delete, get, patch, post},
};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::{
    auth::dev_mode::{ensure_bootstrap_oidc_admin_mapping, ensure_dev_users},
    config::CrystalForgeConfig,
    flake::commits::initialize_flake_commits,
    handlers::{
        agent::{heartbeat, state},
        agent_request::CFState,
        api::{
            admin, auth_dev, auth_local, auth_oidc, auth_session, auth_status, auth_whoami,
            dashboard, flakes, systems,
        },
        status,
        webhook::webhook_handler,
    },
    queries::derivations::reset_non_terminal_derivations,
    server::memory_monitor_task,
    server::spawn_background_tasks,
};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

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

    // Bootstrap OIDC admin group mapping if configured (for oidc mode)
    if auth_mode == "oidc" {
        ensure_bootstrap_oidc_admin_mapping(&pool)
            .await
            .context("Failed to bootstrap OIDC admin mapping")?;
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

    let state = CFState::new(pool, server_cfg.clone());
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
        .route("/api/v1/systems", get(systems::list_systems).post(systems::create_system))
        .route("/api/v1/systems/:id", get(systems::get_system))
        .route("/api/v1/systems/:id/sync", post(systems::sync_system))
        .route(
            "/api/v1/systems/:id/rollback",
            post(systems::rollback_system),
        )
        .route("/api/v1/flakes", get(flakes::list_flakes))
        .route("/api/v1/flakes", post(flakes::create_flake))
        .route("/api/v1/flakes/:id", delete(flakes::delete_flake))
        .route("/api/v1/admin/users", get(admin::list_users))
        .route("/api/v1/admin/users", post(admin::create_user))
        .route("/api/v1/admin/users/:id", patch(admin::update_user))
        .route("/api/v1/admin/users/:id", delete(admin::delete_user))
        .route(
            "/api/v1/admin/oidc-mappings",
            get(admin::list_oidc_mappings),
        )
        .route(
            "/api/v1/admin/oidc-mappings",
            post(admin::upsert_oidc_mapping),
        )
        .route(
            "/api/v1/admin/oidc-mappings/:id",
            delete(admin::delete_oidc_mapping),
        )
        .route("/api/v1/admin/audit-events", get(admin::list_audit_events))
        // Auth context endpoint (publicly accessible)
        .route("/api/auth/whoami", get(auth_whoami::whoami))
        // Setup status endpoint (publicly accessible)
        .route("/api/auth/setup-status", get(auth_status::setup_status))
        // Logout is valid for any mode that issues cookie sessions.
        .route("/api/auth/logout", post(auth_session::logout));

    // Dev-mode auth routes (only available when AUTH_MODE=dev)
    if auth_mode == "dev" {
        info!("Registering development auth endpoints at /api/auth/dev/*");
        app = app.route("/api/auth/dev/login", post(auth_dev::dev_login));
    } else if auth_mode == "local" {
        info!("Registering local auth endpoints at /api/auth/local/*");
        app = app
            .route("/api/auth/local/login", post(auth_local::login))
            .route("/api/auth/local/register", post(auth_local::register));
    } else if auth_mode == "oidc" {
        info!("Registering OIDC auth endpoints at /api/auth/oidc/*");
        match crystal_forge::config::OidcConfig::from_env() {
            Ok(oidc_config) => {
                let oidc_state = Arc::new(auth_oidc::OidcClientState::new(oidc_config).await?);

                let oidc_router = Router::new()
                    .route("/api/auth/oidc/login", get(auth_oidc::oidc_login))
                    .route("/api/auth/oidc/callback", get(auth_oidc::oidc_callback))
                    .layer(Extension(oidc_state));

                app = app.merge(oidc_router);
            }
            Err(err) => {
                // AUTH_MODE defaults to `oidc` if unset. In environments that do not
                // configure OIDC (e.g., certain VM tests), keep server startup working
                // unless oidc mode was explicitly requested.
                if std::env::var("AUTH_MODE").as_deref() == Ok("oidc") {
                    return Err(err).context("Failed to load OIDC configuration from environment");
                }

                warn!(
                    "AUTH_MODE resolved to oidc but OIDC env is incomplete; skipping OIDC route registration: {}",
                    err
                );
            }
        }
    }

    #[cfg(feature = "embedded-ui")]
    {
        app = app.fallback(get(ui::serve_ui));
    }

    // Add CORS layer for development (allows frontend dev server to talk to backend)
    // In production, the UI is served from the same origin, so this is permissive for dev
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("http://localhost:8080"),
            HeaderValue::from_static("http://127.0.0.1:8080"),
            HeaderValue::from_static("http://localhost:8081"),
            HeaderValue::from_static("http://127.0.0.1:8081"),
            HeaderValue::from_static("http://localhost:8000"),
            HeaderValue::from_static("http://127.0.0.1:8000"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            ACCEPT,
            CONTENT_TYPE,
            HeaderName::from_static("x-csrf-token"),
        ])
        .allow_credentials(true);

    let app = app.layer(cors).with_state(state);

    let listener = TcpListener::bind(("0.0.0.0", server_cfg.port)).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

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
