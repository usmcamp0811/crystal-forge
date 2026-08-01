use anyhow::Context;
use axum::Extension;
use axum::http::{
    HeaderValue, Method,
    header::{ACCEPT, CONTENT_TYPE, HeaderName},
};
use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};
use base64::{Engine as _, engine::general_purpose};
use crystal_forge::{
    auth::dev_mode::{
        ensure_bootstrap_oidc_admin_mapping, ensure_dev_users, ensure_local_bootstrap_admin,
    },
    config::{CrystalForgeConfig, db_pool, sync_systems_to_db, validate_db_connection},
    fixtures::seed_from_fixture,
    flake::commits::initialize_flake_commits,
    handlers::{
        agent::{deployment_failed, deployment_started, heartbeat, state},
        agent_request::CFState,
        api::{
            admin, auth_dev, auth_local, auth_oidc, auth_session, auth_status, auth_whoami,
            builders, caches, commits, compliance, config_health, cves, dashboard,
            deployment_policies, deployments, environments, flakes, hardening, navigation,
            scanning, setup_wizard, systems, user_preferences,
        },
        status,
        webhook::webhook_handler,
    },
    queries::attention::dedupe_open_occurrences,
    queries::cache_destinations::encrypt_plaintext_cache_secrets,
    queries::derivations::reset_non_terminal_derivations,
    queue::QueueNotifier,
    server::jobs::BackgroundJobRegistry,
    server::memory_monitor_task,
    server::spawn_background_tasks,
};
use ed25519_dalek::VerifyingKey;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

use tracing::{debug, error, info, warn};
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
    cfg.server.validate().map_err(anyhow::Error::msg)?;
    validate_db_connection().await?;

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

    if cfg.server.execution_mode.is_mock() {
        if !is_local_db_host(&cfg.database.host) {
            anyhow::bail!(
                "server.execution_mode=mock requires a local database host (localhost/127.0.0.1/::1)"
            );
        }

        warn!("⚠️  Running in MOCK execution mode (dev-only)");
        warn!("⚠️  Eval/build steps are simulated and must never be used in production");
    }

    debug!("======== INITIALIZING DATABASE ========");
    let pool = db_pool().await?;
    tokio::spawn(memory_monitor_task(pool.clone()));
    sqlx::migrate!("./migrations").run(&pool).await?;
    let encrypted_rows = encrypt_plaintext_cache_secrets(&pool).await?;
    if encrypted_rows > 0 {
        info!(
            "Encrypted cache secret fields at rest for {} existing destination(s)",
            encrypted_rows
        );
    }
    sync_systems_to_db(&cfg, &pool).await?;

    if let Ok(path) = std::env::var("FIXTURE_JSON_PATH") {
        if !path.is_empty() {
            info!("Fixture seeding enabled — loading from {}", path);
            if let Err(e) = seed_from_fixture(&pool, std::path::Path::new(&path)).await {
                warn!("Fixture seeding failed (continuing): {}", e);
            } else {
                info!("Fixture seeding complete");
            }
        }
    }

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

    if auth_mode == "local" {
        if let (Ok(username), Ok(password)) = (
            std::env::var("CRYSTAL_FORGE_LOCAL_BOOTSTRAP_USERNAME"),
            std::env::var("CRYSTAL_FORGE_LOCAL_BOOTSTRAP_PASSWORD"),
        ) {
            let email = std::env::var("CRYSTAL_FORGE_LOCAL_BOOTSTRAP_EMAIL")
                .unwrap_or_else(|_| "admin@crystal-forge.local".to_string());

            ensure_local_bootstrap_admin(&pool, &username, &email, &password)
                .await
                .context("Failed to initialize local bootstrap admin user")?;
        }
    }

    let background_pool = pool.clone();
    let deployment_pool = pool.clone();
    let flake_init_pool = pool.clone();
    // TODO: Update this to get the first N commits on the first time
    reset_non_terminal_derivations(&pool).await?;
    initialize_flake_commits(&flake_init_pool, &cfg.flakes.watched).await?;

    // Start HTTP server
    info!("Starting Crystal Forge Server...");
    let server_cfg = &cfg.server;
    info!("Host: 0.0.0.0");
    info!("Port: {}", server_cfg.port);

    // Create event-driven queue notifier
    let queue_notifier = Arc::new(QueueNotifier::new());
    info!("🔔 Initialized event-driven queue notification system");

    // Create the background job registry before CFState so the registry is
    // available on server state from the start.  Jobs register themselves
    // during spawn_background_tasks; the registry clone shares the same
    // inner Arc so both CFState and spawn see the same registered handles.
    let job_registry = BackgroundJobRegistry::new();
    let state = CFState::new(
        pool,
        server_cfg.clone(),
        queue_notifier.clone(),
        job_registry.clone(),
    );
    let state_arc = Arc::new(state.clone());

    // One-time (idempotent) repair for duplicate open occurrences — runs
    // SYNCHRONOUSLY before any background producer or the HTTP server starts
    // (round 12: previously this ran inside the spawned reconciliation loop,
    // racing with concurrent flake/eval/CVE producers that could immediately
    // recreate duplicates the repair had just resolved).
    //
    // Round 13: failure is fatal — the repair has been removed from the
    // periodic sweep and there is no retry path. A transient database error
    // would leave malformed attention state in place indefinitely.
    let repaired = dedupe_open_occurrences(&background_pool).await.context(
        "failed to repair attention occurrences at startup — required before producers start",
    )?;

    if repaired > 0 {
        tracing::warn!("🧹 Deduped {repaired} duplicate open attention occurrence(s) on startup");
    }

    spawn_background_tasks(
        cfg.clone(),
        background_pool,
        state_arc.clone(),
        queue_notifier.clone(),
        job_registry,
    );
    let mut app = Router::new()
        .route("/status", get(status::status))
        .route("/system_state", post(state::update))
        .route("/agent/heartbeat", post(heartbeat::log))
        .route("/agent/state", post(state::update))
        .route(
            "/agent/deployment-started",
            post(deployment_started::report),
        )
        .route("/agent/deployment-failed", post(deployment_failed::report))
        .route("/webhook", post(webhook_handler))
        // REST API v1
        .route(
            "/api/v1/dashboard/summary",
            get(dashboard::dashboard_summary),
        )
        .route(
            "/api/v1/cves/summary",
            get(dashboard::cve_dashboard_summary),
        )
        .route(
            "/api/v1/cves/vulnerabilities",
            get(dashboard::cve_dashboard_vulnerabilities),
        )
        .route(
            "/api/v1/cves/top-systems",
            get(dashboard::cve_dashboard_top_systems),
        )
        .route(
            "/api/v1/cves/scan-freshness",
            get(dashboard::cve_scan_freshness),
        )
        // Advanced CVE dashboard endpoints (TASK-322)
        .route(
            "/api/v1/navigation/badges",
            get(navigation::get_navigation_badges),
        )
        .route(
            "/api/v1/navigation/acknowledge",
            post(navigation::acknowledge_navigation_category),
        )
        .route(
            "/api/v1/user/preferences",
            get(user_preferences::get_preferences).patch(user_preferences::patch_preferences),
        )
        .route(
            "/api/v1/user/preferences/initialize",
            post(user_preferences::initialize_preferences),
        )
        .route("/api/v1/cves", get(cves::list_cves))
        .route("/api/v1/cves/grouped", get(cves::list_cves_grouped))
        .route("/api/v1/cves/stats", get(cves::get_fleet_stats))
        .route("/api/v1/cves/packages", get(cves::list_package_names))
        .route(
            "/api/v1/cves/rescan-fleet",
            post(cves::trigger_fleet_rescan),
        )
        .route("/api/v1/cves/export", get(cves::export_cves))
        .route("/api/v1/cves/:cve_id", get(cves::get_cve_detail))
        .route("/api/v1/cves/:cve_id/systems", get(cves::get_cve_systems))
        .route(
            "/api/v1/cves/:cve_id/justification",
            post(cves::save_justification).delete(cves::revoke_justification),
        )
        .route(
            "/api/v1/cves/:cve_id/justifications",
            get(cves::list_justifications),
        )
        .route("/api/v1/scanning/stats", get(scanning::get_scanning_stats))
        .route("/api/v1/scanning/queue", get(scanning::get_scanning_queue))
        .route(
            "/api/v1/scanning/systems",
            get(scanning::get_scanning_systems),
        )
        .route(
            "/api/v1/scanning/systems/:system_id/scans",
            get(scanning::get_scanning_system_scans),
        )
        .route(
            "/api/v1/scanning/activity",
            get(scanning::get_scanning_activity),
        )
        .route(
            "/api/v1/scanning/schedule",
            get(scanning::get_scanning_schedule).put(scanning::put_scanning_schedule),
        )
        .route(
            "/api/v1/scanning/deployed",
            get(scanning::get_scanning_deployed),
        )
        .route(
            "/api/v1/hardening/summary",
            get(hardening::hardening_fleet_summary),
        )
        .route(
            "/api/v1/hardening/top-services",
            get(hardening::hardening_top_services),
        )
        .route(
            "/api/v1/hardening/systems",
            get(hardening::hardening_system_postures),
        )
        .route(
            "/api/v1/systems",
            get(systems::list_systems).post(systems::create_system),
        )
        .route(
            "/api/v1/systems/:id",
            get(systems::get_system).patch(systems::update_system_handler),
        )
        .route("/api/v1/systems/:id/cves", get(systems::get_system_cves))
        .route(
            "/api/v1/systems/:id/cves/:cve_id/justification",
            put(systems::save_system_cve_justification),
        )
        .route(
            "/api/v1/systems/:id/cve-scan-eligibility",
            get(systems::get_system_cve_scan_eligibility),
        )
        .route(
            "/api/v1/systems/:id/cve-scan",
            post(systems::trigger_system_cve_scan),
        )
        .route(
            "/api/v1/systems/:id/hardening",
            get(hardening::get_system_hardening),
        )
        .route(
            "/api/v1/systems/:id/hardening/justifications",
            get(hardening::get_system_hardening_justifications),
        )
        .route(
            "/api/v1/systems/:id/hardening/:service_name/justification",
            put(hardening::save_hardening_justification),
        )
        .route(
            "/api/v1/systems/:id/hardening-scan-eligibility",
            get(hardening::get_system_hardening_scan_eligibility),
        )
        .route(
            "/api/v1/systems/:id/hardening-scan",
            post(hardening::trigger_system_hardening_scan_handler),
        )
        .route("/api/v1/systems/:id/sync", post(systems::sync_system))
        .route(
            "/api/v1/systems/:id/rollback",
            post(systems::rollback_system),
        )
        .route(
            "/api/v1/systems/:id/rollback-generation",
            post(systems::rollback_system_generation),
        )
        .route(
            "/api/v1/systems/:id/public-key",
            put(systems::update_system_public_key),
        )
        .route(
            "/api/v1/systems/:id/deactivate",
            post(systems::deactivate_system_handler),
        )
        .route("/api/v1/systems/:id/deploy", post(systems::deploy_system))
        .route(
            "/api/v1/systems/:id/commits",
            get(systems::get_system_commits),
        )
        .route(
            "/api/v1/systems/:id/generations",
            get(systems::get_system_generations),
        )
        .route(
            "/api/v1/systems/:id/verify-generation-closure",
            post(systems::verify_generation_closure),
        )
        .route(
            "/api/v1/systems/:id/deployment-status",
            get(systems::get_system_deployment_status),
        )
        .route(
            "/api/v1/systems/:id/history",
            get(systems::get_system_history),
        )
        .route(
            "/api/v1/systems/:id/agent-events",
            get(systems::get_system_agent_events),
        )
        .route(
            "/api/v1/environments",
            get(environments::list_environments).post(environments::create_environment),
        )
        .route(
            "/api/v1/environments/policies-map",
            get(environments::list_environment_policy_map_handler),
        )
        .route(
            "/api/v1/environments/:id",
            get(environments::get_environment)
                .patch(environments::update_environment_handler)
                .delete(environments::delete_environment_handler),
        )
        .route(
            "/api/v1/environments/:id/policies",
            get(environments::get_environment_with_policies_handler)
                .patch(environments::update_environment_policies_handler),
        )
        .route("/api/v1/policies", get(environments::list_policies_handler))
        .route(
            "/api/v1/compliance/bundles",
            get(compliance::list_compliance_bundles).post(compliance::create_compliance_bundle),
        )
        .route(
            "/api/v1/compliance/bundles/:id",
            // GET is intentionally absent: use GET /bundles/:id/systems instead.
            // Having GET here return the systems payload at both paths created a
            // misleading API contract (reviewer finding #3).
            put(compliance::update_compliance_bundle).delete(compliance::delete_compliance_bundle),
        )
        .route(
            "/api/v1/compliance/bundles/:id/systems",
            get(compliance::get_compliance_bundle_systems),
        )
        .route(
            "/api/v1/compliance/bundles/:id/systems/:system_id/evidence",
            get(compliance::get_compliance_system_evidence),
        )
        .route(
            "/api/v1/systems/:system_id/compliance",
            get(compliance::get_system_compliance_bundles),
        )
        // CF-XCCDF import/export and bundle version endpoints
        .route(
            "/api/v1/compliance/xccdf/preview",
            post(compliance::xccdf_preview),
        )
        .route(
            "/api/v1/compliance/xccdf/import",
            post(compliance::xccdf_import),
        )
        .route(
            "/api/v1/compliance/bundle-versions/:version_id/xccdf",
            get(compliance::export_bundle_xccdf),
        )
        // Policy interchange endpoints
        .route(
            "/api/v1/policies/interchange/export",
            post(compliance::policy_interchange_export),
        )
        // Deployment policies CRUD endpoints
        .route(
            "/api/v1/deployment-policies",
            get(deployment_policies::list_deployment_policies)
                .post(deployment_policies::create_deployment_policy),
        )
        .route(
            "/api/v1/deployment-policies/:id",
            get(deployment_policies::get_deployment_policy)
                .put(deployment_policies::update_deployment_policy)
                .delete(deployment_policies::delete_deployment_policy),
        )
        // Deployment policy workflow endpoints (approvals, rollout status)
        .route(
            "/api/v1/deployments/commit/:commit_id/approve",
            post(deployments::submit_commit_approval),
        )
        .route(
            "/api/v1/deployments/commit/:commit_id/approvals/:policy_id",
            get(deployments::get_commit_approval_status),
        )
        .route(
            "/api/v1/deployments/commit/:commit_id/rollout/:policy_id",
            get(deployments::get_commit_rollout_status),
        )
        .route("/api/v1/flakes", get(flakes::list_flakes))
        .route("/api/v1/flakes", post(flakes::create_flake))
        .route("/api/v1/flakes/sync", post(flakes::sync_all_flakes_handler))
        .route(
            "/api/v1/flakes/:id",
            patch(flakes::update_flake_handler).delete(flakes::delete_flake),
        )
        .route("/api/v1/flakes/:id/sync", post(flakes::sync_flake_handler))
        .route(
            "/api/v1/flakes/:id/configs/:config_name/cve-scan",
            post(flakes::trigger_flake_config_cve_scan),
        )
        .route(
            "/api/v1/flakes/:id/credentials",
            get(flakes::get_flake_credentials)
                .put(flakes::put_flake_credentials)
                .patch(flakes::patch_flake_credentials)
                .delete(flakes::delete_flake_credentials_handler),
        )
        .route(
            "/api/v1/flakes/:id/credentials/test",
            post(flakes::test_flake_credentials),
        )
        .route("/api/v1/flakes/:id/refresh", post(flakes::refresh_flake))
        .route(
            "/api/v1/flakes/:id/accept-rewrite",
            post(flakes::accept_flake_history_rewrite),
        )
        .route("/api/v1/flakes/timelines", get(flakes::get_flake_timelines))
        .route(
            "/api/v1/flakes/:id/commits/:hash/diff",
            get(flakes::get_commit_diff_handler),
        )
        .route("/api/v1/cve-scans/:id", get(systems::get_cve_scan_status))
        .route(
            "/api/v1/hardening-scans/:id",
            get(hardening::get_hardening_scan_status),
        )
        // Builder management (admin endpoints)
        .route(
            "/api/v1/builders",
            get(builders::list_builders).post(builders::create_builder),
        )
        .route(
            "/api/v1/builders/resolve-id",
            post(builders::resolve_builder_id),
        )
        .route(
            "/api/v1/builders/:id",
            get(builders::get_builder)
                .patch(builders::update_builder)
                .delete(builders::deactivate_builder),
        )
        .route(
            "/api/v1/builders/:id/permanent",
            delete(builders::delete_builder_permanently),
        )
        .route(
            "/api/v1/builders/:id/public-key",
            put(builders::update_builder_public_key),
        )
        .route(
            "/api/v1/builders/:id/regenerate-keypair",
            post(builders::regenerate_builder_keypair),
        )
        .route(
            "/api/v1/builders/:id/environments",
            patch(builders::update_builder_environments),
        )
        .route(
            "/api/v1/builders/:id/metrics",
            get(builders::get_builder_metrics),
        )
        .route("/api/v1/build-jobs", get(builders::list_build_queue))
        .route(
            "/api/v1/build-jobs/:id/cancel",
            post(builders::cancel_build_job),
        )
        .route(
            "/api/v1/build-jobs/:id/requeue",
            post(builders::requeue_build_job),
        )
        .route(
            "/api/v1/build-jobs/:id/force-cancel",
            post(builders::force_cancel_build_job),
        )
        .route(
            "/api/v1/build-jobs/recent",
            get(builders::list_recent_build_jobs),
        )
        .route(
            "/api/v1/build-jobs/:id/prioritize",
            post(builders::prioritize_build_job),
        )
        .route(
            "/api/v1/build-jobs/:id/move-up",
            post(builders::move_build_job_up),
        )
        .route(
            "/api/v1/build-jobs/:id/move-down",
            post(builders::move_build_job_down),
        )
        .route(
            "/api/v1/build-queue/reorder",
            post(builders::reorder_build_queue),
        )
        // Builder-authenticated endpoints
        .route(
            "/api/v1/builders/:id/session",
            post(builders::establish_builder_session),
        )
        .route(
            "/api/v1/builders/:id/heartbeat",
            post(builders::builder_heartbeat),
        )
        .route(
            "/api/v1/builders/:id/next-job",
            get(builders::get_next_job).post(builders::get_next_job),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/start",
            post(builders::start_job),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/progress",
            post(builders::build_progress),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/derivation-archive",
            get(builders::download_job_derivation_archive)
                .post(builders::download_job_derivation_archive_delta),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/derivation-manifest",
            get(builders::get_job_derivation_manifest),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/source-archive",
            get(builders::download_job_source_archive),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/publish-derivation-closure",
            post(builders::publish_job_derivation_closure),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/complete",
            post(builders::complete_job),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/fail",
            post(builders::fail_job),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/logs",
            post(builders::append_job_logs),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/finalize-cancelled",
            post(builders::finalize_cancelled_job),
        )
        .route(
            "/api/v1/builders/:id/jobs/:job_id/status",
            get(builders::get_job_status),
        )
        .route(
            "/api/v1/build-jobs/:job_id/logs/stream",
            get(builders::stream_build_logs),
        )
        .route("/api/v1/commits/eval-queue", get(commits::list_eval_queue))
        .route(
            "/api/v1/commits/eval-queue/reorder",
            post(commits::reorder_eval_queue),
        )
        .route(
            "/api/v1/commits/eval-history",
            get(commits::list_eval_history),
        )
        .route(
            "/api/v1/commits/:commit_id/eval/stream",
            get(commits::stream_eval_logs),
        )
        .route(
            "/api/v1/commits/:commit_id/eval/logs",
            get(commits::get_eval_logs_history),
        )
        .route(
            "/api/v1/commits/:commit_id/eval/policy-matrix",
            get(commits::get_eval_policy_matrix),
        )
        .route(
            "/api/v1/commits/:commit_id/eval/dependency-graph",
            get(commits::get_eval_dependency_graph),
        )
        .route(
            "/api/v1/commits/:commit_id/re-evaluate",
            post(commits::re_evaluate_commit),
        )
        .route(
            "/api/v1/commits/:commit_id/cancel-evaluation",
            post(commits::cancel_commit_evaluation),
        )
        .route(
            "/api/v1/commits/:commit_id/force-cancel-evaluation",
            post(commits::force_cancel_commit_evaluation),
        )
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
        .route("/api/v1/admin/server-info", get(admin::server_runtime_info))
        .route(
            "/api/v1/admin/classification-config",
            get(admin::get_classification_config).put(admin::update_classification_config),
        )
        .route(
            "/api/v1/admin/automatic-retry-policy",
            get(admin::get_automatic_retry_policy).put(admin::update_automatic_retry_policy),
        )
        .route(
            "/api/v1/admin/setup-progress",
            get(setup_wizard::get_setup_progress),
        )
        .route(
            "/api/v1/admin/setup-wizard/dismiss",
            post(setup_wizard::dismiss_setup_wizard),
        )
        .route(
            "/api/v1/admin/setup-wizard/agent-acknowledge",
            post(setup_wizard::acknowledge_agent_step),
        )
        .route(
            "/api/v1/admin/config-health",
            get(config_health::config_health),
        )
        // Cache management endpoints
        .route(
            "/api/v1/caches",
            get(caches::list_cache_destinations).post(caches::create_cache_destination),
        )
        .route(
            "/api/v1/caches/test-credentials",
            post(caches::test_cache_destination_credentials),
        )
        .route(
            "/api/v1/caches/:id",
            get(caches::get_cache_destination)
                .put(caches::update_cache_destination)
                .delete(caches::delete_cache_destination),
        )
        .route("/api/v1/cache-push-jobs", get(caches::list_cache_push_jobs))
        .route(
            "/api/v1/cache-push-jobs/:id",
            get(caches::get_cache_push_job),
        )
        .route(
            "/api/v1/cache-push-jobs/:id/retry",
            post(caches::retry_cache_push_job),
        )
        .route(
            "/api/v1/cache-push-jobs/:id/cancel",
            post(caches::cancel_cache_push_job),
        )
        .route(
            "/api/v1/cache-push-jobs/bulk-retry",
            post(caches::bulk_retry_cache_push_jobs),
        )
        .route(
            "/api/v1/cache-push-jobs/bulk-cancel",
            post(caches::bulk_cancel_cache_push_jobs),
        )
        // Cache environment assignment routes
        .route(
            "/api/v1/caches/:id/environments",
            get(caches::get_cache_environments_handler)
                .put(caches::assign_cache_environments_handler),
        )
        .route(
            "/api/v1/environments/:id/caches",
            get(caches::get_environment_caches_handler),
        )
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
            Method::PUT,
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

fn is_local_db_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
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
