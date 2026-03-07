use crystal_forge::builder::api_client::BuilderApiClient;
use crystal_forge::builder::metrics::SystemMetrics;
use crystal_forge::builder::{run_build_loop, run_cache_push_loop, run_cve_scan_loop};
use crystal_forge::config::CrystalForgeConfig;
use crystal_forge::models::builders::{BuildJob, ReportMetricsRequest};
use crystal_forge::queries::derivations::get_derivation_by_id;
use crystal_forge::server::memory_monitor_task;
use std::hash::{Hash, Hasher};
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = CrystalForgeConfig::load()?;
    cfg.server.validate().map_err(anyhow::Error::msg)?;
    let builder_config = cfg.get_builder_config();

    if cfg.server.execution_mode.is_mock() {
        if !is_local_db_host(&cfg.database.host) {
            anyhow::bail!(
                "server.execution_mode=mock requires a local database host (localhost/127.0.0.1/::1)"
            );
        }

        warn!("⚠️  Builder running in MOCK execution mode (dev-only)");
    }

    // Check if API mode is enabled and configured
    if builder_config.is_api_mode_ready() {
        info!("🌐 Starting Crystal Forge Builder in API mode...");
        return run_api_mode(&cfg).await;
    }

    if cfg.server.execution_mode.is_mock() {
        anyhow::bail!(
            "server.execution_mode=mock requires builder API mode. Set builder.enable_api_mode=true with builder_id/private_key_path/server_url"
        );
    }

    // Fall back to legacy direct-database mode (deprecated)
    warn!(
        "⚠️  Starting builder in deprecated legacy direct-database mode. Migrate to builder API mode."
    );
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
        result = run_api_job_loop(
            poll_client,
            poll_interval,
            build_config.clone(),
            cache_config.clone(),
            cfg.server.execution_mode,
        ) => {
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
    build_config: crystal_forge::config::BuildConfig,
    cache_config: crystal_forge::config::CacheConfig,
    execution_mode: crystal_forge::config::ExecutionMode,
) -> anyhow::Result<()> {
    // Create DB pool once and share across all jobs
    let pool = crystal_forge::config::CrystalForgeConfig::db_pool().await?;
    let mut ticker = tokio::time::interval(poll_interval);

    // Limit concurrent builds to max_concurrent_jobs
    let max_concurrent = build_config.max_concurrent_derivations;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrent));
    info!(
        "🔨 Starting job polling loop (max concurrent: {})...",
        max_concurrent
    );

    loop {
        ticker.tick().await;

        // Check if we have capacity for another build
        if semaphore.available_permits() == 0 {
            continue; // All slots busy, skip polling
        }

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

                // Acquire semaphore permit for this build
                let permit = semaphore.clone().acquire_owned().await.unwrap();

                // Execute the build in a spawned task to allow concurrent builds
                let job_client = client.clone();
                let job_build_config = build_config.clone();
                let job_cache_config = cache_config.clone();
                let job_pool = pool.clone();

                tokio::spawn(async move {
                    execute_build_job(
                        job,
                        job_client,
                        job_build_config,
                        job_cache_config,
                        job_pool,
                        execution_mode,
                    )
                    .await;
                    drop(permit); // Release semaphore when build completes
                });
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

/// Execute a build job: fetch derivation, build it, report results
async fn execute_build_job(
    job: BuildJob,
    client: BuilderApiClient,
    build_config: crystal_forge::config::BuildConfig,
    cache_config: crystal_forge::config::CacheConfig,
    pool: sqlx::PgPool,
    execution_mode: crystal_forge::config::ExecutionMode,
) {
    info!(
        "🔨 Starting build for job #{} (derivation: {})",
        job.id, job.derivation_id
    );

    // Fetch the derivation from database
    let mut derivation = match get_derivation_by_id(&pool, job.derivation_id).await {
        Ok(deriv) => deriv,
        Err(e) => {
            error!("❌ Failed to fetch derivation {}: {}", job.derivation_id, e);
            if let Err(e2) = client
                .fail_job(job.id, &format!("Failed to fetch derivation: {}", e))
                .await
            {
                error!("❌ Failed to report job failure: {}", e2);
            }
            return;
        }
    };

    info!("📦 Building derivation: {}", derivation.derivation_name);

    // Get build timeout from config
    let build_timeout = std::cmp::min(
        build_config.timeout,
        std::time::Duration::from_secs(7200), // Max 2 hours
    );

    let start = std::time::Instant::now();

    // Try to connect WebSocket for real-time log streaming
    let ws_stream = match client.create_log_stream(&job.id).await {
        Ok(stream) => {
            info!("📡 WebSocket connected for real-time logs");
            Some(stream)
        }
        Err(e) => {
            warn!(
                "⚠️  WebSocket connection failed, using HTTP fallback: {}",
                e
            );
            None
        }
    };

    // Wrap WebSocket in Arc<Mutex> for sharing between tasks
    let mut ws_shared = ws_stream.map(|ws| std::sync::Arc::new(tokio::sync::Mutex::new(ws)));

    // Send initial log message (via WebSocket if available, otherwise HTTP)
    let initial_log = format!("🔨 Starting build for {}\n", derivation.derivation_name);
    send_log_with_fallback(&client, job.id, &mut ws_shared, &initial_log).await;

    // Spawn metrics reporting task if WebSocket is available
    let metrics_task = if let Some(ref ws) = ws_shared {
        use sysinfo::System;
        let ws_for_metrics = ws.clone();

        Some(tokio::spawn(async move {
            let mut sys = System::new_all();
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));

            loop {
                interval.tick().await;
                sys.refresh_cpu_all();
                sys.refresh_memory();

                let cpu_percent = sys.global_cpu_usage();
                let ram_used_mb = sys.used_memory() / 1024 / 1024;
                let ram_total_mb = sys.total_memory() / 1024 / 1024;

                let mut ws = ws_for_metrics.lock().await;
                if let Err(e) = crystal_forge::builder::api_client::BuilderApiClient::send_metrics(
                    &mut *ws,
                    cpu_percent,
                    ram_used_mb,
                    ram_total_mb,
                )
                .await
                {
                    tracing::warn!("Failed to send metrics: {}", e);
                    break;
                }
            }
        }))
    } else {
        None
    };

    // Execute the build with timeout, or deterministic mock execution in dev mode.
    let build_result = if execution_mode.is_mock() {
        let mock_result = run_mock_build(&mut derivation, &client, job.id, &mut ws_shared).await;
        Ok(mock_result)
    } else {
        tokio::time::timeout(build_timeout, derivation.build(&pool, &build_config)).await
    };

    match build_result {
        // Build succeeded within timeout
        Ok(Ok(store_path)) => {
            let duration = start.elapsed();
            info!(
                "✅ Job #{} completed in {:.1}s: {}",
                job.id,
                duration.as_secs_f64(),
                store_path
            );

            // Send success log
            let success_msg = format!(
                "✅ Build completed successfully in {:.1}s\n",
                duration.as_secs_f64()
            );
            let output_msg = format!("   Output: {}\n", store_path);

            send_log_with_fallback(&client, job.id, &mut ws_shared, &success_msg).await;
            send_log_with_fallback(&client, job.id, &mut ws_shared, &output_msg).await;

            // Update derivation with store_path for signing
            derivation.store_path = Some(store_path.clone());

            if execution_mode.is_mock() {
                let _ = client
                    .append_logs(
                        job.id,
                        "🧪 MOCK MODE: skipping signing and cache push for synthetic artifacts\n",
                    )
                    .await;
            } else {
                // Sign the derivation
                let _ = client
                    .append_logs(job.id, "🔐 Signing derivation...\n")
                    .await;
                if let Err(e) = derivation.sign(&cache_config).await {
                    warn!(
                        "⚠️ Signing failed for job #{}, continuing anyway: {}",
                        job.id, e
                    );
                    let _ = client
                        .append_logs(job.id, &format!("⚠️  Signing failed: {}\n", e))
                        .await;
                } else {
                    let _ = client.append_logs(job.id, "✅ Derivation signed\n").await;
                }

                // Create cache push job if configured
                if cache_config.push_after_build {
                    let _ = client
                        .append_logs(job.id, "📤 Queuing cache push job...\n")
                        .await;
                    if let Some(ref store_path) = derivation.store_path {
                        if let Err(e) = crystal_forge::queries::cache_push::create_cache_push_job(
                            &pool,
                            derivation.id,
                            store_path,
                            cache_config.push_to.as_deref(),
                        )
                        .await
                        {
                            warn!(
                                "⚠️ Cache queue failed for job #{}, continuing anyway: {}",
                                job.id, e
                            );
                            let _ = client
                                .append_logs(
                                    job.id,
                                    &format!("⚠️  Cache push queue failed: {}\n", e),
                                )
                                .await;
                        } else {
                            let _ = client
                                .append_logs(job.id, "✅ Cache push job queued\n")
                                .await;
                        }
                    }
                }
            }

            // Mark derivation as complete in database
            match pool.begin().await {
                Ok(mut tx) => {
                    if let Err(e) = crystal_forge::queries::derivations::mark_target_build_complete(
                        &mut *tx,
                        derivation.id,
                        &store_path,
                    )
                    .await
                    {
                        error!("❌ Failed to mark derivation complete: {}", e);
                    } else if let Err(e) = tx.commit().await {
                        error!("❌ Failed to commit transaction: {}", e);
                    }
                }
                Err(e) => {
                    error!("❌ Failed to begin transaction: {}", e);
                }
            }

            // Report success to server
            if let Err(e) = client.complete_job(job.id, &store_path).await {
                error!("❌ Failed to report job completion: {}", e);
            }
        }

        // Build failed within timeout
        Ok(Err(e)) => {
            let duration = start.elapsed();
            error!(
                "❌ Job #{} build failed after {:.1}s: {}",
                job.id,
                duration.as_secs_f64(),
                e
            );

            // Send failure log
            send_log_with_fallback(
                &client,
                job.id,
                &mut ws_shared,
                &format!("❌ Build failed after {:.1}s\n", duration.as_secs_f64()),
            )
            .await;
            send_log_with_fallback(
                &client,
                job.id,
                &mut ws_shared,
                &format!("   Error: {}\n", e),
            )
            .await;

            // Mark derivation as failed in database
            match pool.begin().await {
                Ok(mut tx) => {
                    if let Err(e2) = crystal_forge::queries::derivations::handle_derivation_failure(
                        &mut *tx,
                        &derivation,
                        "build",
                        &e,
                    )
                    .await
                    {
                        error!("❌ Failed to mark derivation failed: {}", e2);
                    } else if let Err(e2) = tx.commit().await {
                        error!("❌ Failed to commit transaction: {}", e2);
                    }
                }
                Err(e2) => {
                    error!("❌ Failed to begin transaction: {}", e2);
                }
            }

            // Report failure to server
            if let Err(e2) = client.fail_job(job.id, &e.to_string()).await {
                error!("❌ Failed to report job failure: {}", e2);
            }
        }

        // Build timed out
        Err(_timeout) => {
            let duration = start.elapsed();
            let timeout_msg = format!(
                "Build timed out after {:.1}s (limit: {:.1}s)",
                duration.as_secs_f64(),
                build_timeout.as_secs_f64()
            );

            error!("⏱️ Job #{}: {}", job.id, timeout_msg);

            // Send timeout log
            send_log_with_fallback(
                &client,
                job.id,
                &mut ws_shared,
                &format!("⏱️  {}\n", timeout_msg),
            )
            .await;

            let timeout_error = anyhow::anyhow!(timeout_msg.clone());

            // Mark derivation as failed in database
            match pool.begin().await {
                Ok(mut tx) => {
                    if let Err(e) = crystal_forge::queries::derivations::handle_derivation_failure(
                        &mut *tx,
                        &derivation,
                        "build-timeout",
                        &timeout_error,
                    )
                    .await
                    {
                        error!("❌ Failed to mark derivation timeout: {}", e);
                    } else if let Err(e) = tx.commit().await {
                        error!("❌ Failed to commit transaction: {}", e);
                    }
                }
                Err(e) => {
                    error!("❌ Failed to begin transaction: {}", e);
                }
            }

            // Report timeout failure to server
            if let Err(e) = client.fail_job(job.id, &timeout_msg).await {
                error!("❌ Failed to report job timeout: {}", e);
            }
        }
    }

    // Clean up metrics task if it was spawned
    if let Some(task) = metrics_task {
        task.abort();
    }
}

async fn run_mock_build(
    derivation: &mut crystal_forge::derivations::Derivation,
    client: &BuilderApiClient,
    job_id: uuid::Uuid,
    ws_shared: &mut Option<
        std::sync::Arc<
            tokio::sync::Mutex<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
            >,
        >,
    >,
) -> anyhow::Result<String> {
    send_log_with_fallback(
        client,
        job_id,
        ws_shared,
        "🧪 MOCK MODE: simulating build\n",
    )
    .await;
    send_log_with_fallback(
        client,
        job_id,
        ws_shared,
        "⏳ [10%] Reserving local sandbox...\n",
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    send_log_with_fallback(
        client,
        job_id,
        ws_shared,
        "🔨 [35%] Resolving derivation graph...\n",
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    send_log_with_fallback(
        client,
        job_id,
        ws_shared,
        "⚙️  [65%] Building outputs...\n",
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    send_log_with_fallback(
        client,
        job_id,
        ws_shared,
        "📦 [90%] Finalizing store path...\n",
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    if should_mock_build_fail(&derivation.derivation_name) {
        send_log_with_fallback(
            client,
            job_id,
            ws_shared,
            "❌ MOCK build failed intentionally for UI validation\n",
        )
        .await;
        anyhow::bail!(
            "MOCK build failure for {}",
            derivation.derivation_name
        );
    }

    let store_path = mock_store_path(job_id, derivation.id, &derivation.derivation_name);

    send_log_with_fallback(
        client,
        job_id,
        ws_shared,
        &format!("✅ MOCK build complete: {}\n", store_path),
    )
    .await;

    Ok(store_path)
}

fn mock_store_path(job_id: uuid::Uuid, derivation_id: i32, derivation_name: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{}:{}", job_id, derivation_id).hash(&mut hasher);
    let short_hash = format!("{:016x}", hasher.finish());
    let sanitized = derivation_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("/nix/store/{}-{}", short_hash, sanitized)
}

fn should_mock_build_fail(derivation_name: &str) -> bool {
    derivation_name.contains("-control-0")
}

async fn send_log_with_fallback(
    client: &BuilderApiClient,
    job_id: uuid::Uuid,
    ws_shared: &mut Option<
        std::sync::Arc<
            tokio::sync::Mutex<
                tokio_tungstenite::WebSocketStream<
                    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
                >,
            >,
        >,
    >,
    message: &str,
) {
    if let Some(ws) = ws_shared.as_ref() {
        let mut ws_lock = ws.lock().await;
        let sent_ok = crystal_forge::builder::api_client::BuilderApiClient::send_log_line(
            &mut *ws_lock,
            message,
        )
        .await
        .is_ok();
        drop(ws_lock);

        if sent_ok {
            return;
        }

        warn!(
            "WebSocket log send failed for job {}, falling back to HTTP append",
            job_id
        );
        *ws_shared = None;
    }

    let _ = client.append_logs(job_id, message).await;
}

fn is_local_db_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[cfg(test)]
mod tests {
    use super::{mock_store_path, should_mock_build_fail};

    #[test]
    fn mock_store_path_is_deterministic_and_sanitized() {
        let job_id = uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555")
            .expect("uuid should parse");

        let one = mock_store_path(job_id, 7, "web-ui/main@amd64");
        let two = mock_store_path(job_id, 7, "web-ui/main@amd64");
        assert_eq!(one, two);
        assert!(one.starts_with("/nix/store/"));
        assert!(one.ends_with("-web-ui-main-amd64"));
    }

    #[test]
    fn mock_build_fail_pattern_is_deterministic() {
        assert!(should_mock_build_fail("myflake-control-0"));
        assert!(!should_mock_build_fail("myflake-worker-0"));
    }
}
