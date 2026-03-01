use crystal_forge::builder::api_client::BuilderApiClient;
use crystal_forge::builder::metrics::SystemMetrics;
use crystal_forge::builder::{run_build_loop, run_cache_push_loop, run_cve_scan_loop};
use crystal_forge::config::CrystalForgeConfig;
use crystal_forge::models::builders::ReportMetricsRequest;
use crystal_forge::server::memory_monitor_task;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = CrystalForgeConfig::load()?;
    let builder_config = cfg.get_builder_config();

    // Check if API mode is enabled and configured
    if builder_config.is_api_mode_ready() {
        info!("🌐 Starting Crystal Forge Builder in API mode...");
        return run_api_mode(&cfg).await;
    }

    // Fall back to legacy direct-database mode
    info!("💾 Starting Crystal Forge Builder in legacy database mode...");
    CrystalForgeConfig::validate_db_connection().await?;

    let pool = CrystalForgeConfig::db_pool().await?;

    tokio::spawn(memory_monitor_task(pool.clone()));
    sqlx::migrate!("./migrations").run(&pool).await?;

    let cache_config = &cfg.cache;

    let build_handle = tokio::spawn(run_build_loop(pool.clone()));
    let cve_scan_handle = tokio::spawn(run_cve_scan_loop(pool.clone()));

    if cache_config.push_after_build {
        let cache_handle = tokio::spawn(run_cache_push_loop(pool.clone()));
        info!("✅ Build, CVE scan, and cache push loops started");

        tokio::select! {
            result = build_handle => {
                error!("Build loop exited unexpectedly: {:?}", result);
            }
            result = cve_scan_handle => {
                error!("CVE scan loop exited unexpectedly: {:?}", result);
            }
            result = cache_handle => {
                error!("Cache push loop exited unexpectedly: {:?}", result);
            }
            _ = signal::ctrl_c() => {
                info!("Received shutdown signal");
            }
        }
    } else {
        info!("📤 Cache push disabled in configuration");
        info!("✅ Build and CVE scan loops started");

        tokio::select! {
            result = build_handle => {
                error!("Build loop exited unexpectedly: {:?}", result);
            }
            result = cve_scan_handle => {
                error!("CVE scan loop exited unexpectedly: {:?}", result);
            }
            _ = signal::ctrl_c() => {
                info!("Received shutdown signal");
            }
        }
    }

    info!("Shutting down Crystal Forge Builder...");
    Ok(())
}

/// Run builder in API mode (communicates with server via API instead of direct DB)
async fn run_api_mode(cfg: &CrystalForgeConfig) -> anyhow::Result<()> {
    let builder_config = cfg.get_builder_config();
    let build_config = cfg.get_build_config();
    let cache_config = cfg.get_cache_config();

    info!("Initializing API client...");
    let api_client = BuilderApiClient::new(builder_config).await?;

    let builder_id = builder_config.require_builder_id()?;
    info!("✅ Builder ID: {}", builder_id);
    info!(
        "✅ Derived Public Key (base64): {}",
        api_client.public_key_base64()
    );
    info!("✅ Server URL: {}", builder_config.require_server_url()?);
    info!("✅ Poll interval: {:?}", builder_config.poll_interval);
    info!(
        "✅ Heartbeat interval: {:?}",
        builder_config.heartbeat_interval
    );

    // Spawn heartbeat task
    let heartbeat_client = api_client.clone();
    let heartbeat_interval = builder_config.heartbeat_interval;
    tokio::spawn(async move {
        run_heartbeat_loop(heartbeat_client, heartbeat_interval).await;
    });

    // Spawn job polling loop
    let poll_client = api_client.clone();
    let poll_interval = builder_config.poll_interval;
    let max_concurrent = builder_config.max_concurrent_jobs.unwrap_or(1);

    info!(
        "🔨 Starting job polling loop (max concurrent: {})...",
        max_concurrent
    );

    tokio::select! {
        result = run_api_job_loop(poll_client, poll_interval, max_concurrent, build_config.clone(), cache_config.clone()) => {
            error!("Job loop exited unexpectedly: {:?}", result);
        }
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
    }

    info!("Shutting down API mode builder...");
    Ok(())
}

/// Heartbeat loop - sends metrics to server periodically
async fn run_heartbeat_loop(client: BuilderApiClient, interval: std::time::Duration) {
    let mut ticker = tokio::time::interval(interval);

    loop {
        ticker.tick().await;

        // Collect system metrics
        let system_metrics = SystemMetrics::collect(0).await; // TODO: track actual active jobs

        let memory_total_mb = system_metrics.memory_total_mb;
        let memory_used_mb = system_metrics.memory_used_mb;

        let metrics = ReportMetricsRequest {
            cpu_usage_percent: system_metrics.cpu_usage_percent.unwrap_or(0.0),
            memory_usage_mb: memory_used_mb.unwrap_or(0),
            system_cpu_usage_percent: system_metrics.cpu_usage_percent,
            system_memory_total_mb: memory_total_mb,
            system_memory_used_mb: memory_used_mb,
        };

        if let Err(e) = client.send_heartbeat(&metrics).await {
            error!("❌ Failed to send heartbeat: {}", e);
        }
    }
}

/// Job polling loop - requests work from server and executes it
async fn run_api_job_loop(
    client: BuilderApiClient,
    poll_interval: std::time::Duration,
    max_concurrent: i32,
    build_config: crystal_forge::config::BuildConfig,
    cache_config: crystal_forge::config::CacheConfig,
) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(poll_interval);

    loop {
        ticker.tick().await;

        // TODO: Check current active job count against max_concurrent
        // For now, we'll keep it simple and process one job at a time

        match client.get_next_job().await {
            Ok(Some(job)) => {
                info!(
                    "📦 Received job #{} (derivation: {})",
                    job.id, job.derivation_id
                );

                // Start the job
                if let Err(e) = client.start_job(job.id).await {
                    error!("❌ Failed to start job #{}: {}", job.id, e);
                    continue;
                }

                // TODO: Actually build the derivation
                // This is a placeholder - in a real implementation, we would:
                // 1. Fetch derivation details from the job
                // 2. Build it using the existing derivation.build() logic
                // 3. Stream logs during the build
                // 4. Complete or fail based on the result

                warn!("⚠️  Job execution not yet implemented in API mode");

                // For now, just fail it
                if let Err(e) = client
                    .fail_job(job.id, "API mode job execution not yet implemented")
                    .await
                {
                    error!("❌ Failed to fail job #{}: {}", job.id, e);
                }
            }
            Ok(None) => {
                // No jobs available, continue polling
            }
            Err(e) => {
                error!("❌ Failed to get next job: {}", e);
            }
        }
    }
}
