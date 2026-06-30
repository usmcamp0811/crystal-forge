use crystal_forge::builder::api_client::{ApiBuildReporter, BuilderApiClient};
use crystal_forge::builder::metrics::SystemMetrics;
use crystal_forge::config::CrystalForgeConfig;
use crystal_forge::derivations::build::{BuildCancelledError, LogSink};
use crystal_forge::models::builders::{BuildJobDerivation, NextJobResponse, ReportMetricsRequest};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::AsyncWriteExt;
use tokio::signal;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// Channel capacity for log streaming. Provides backpressure when the forwarding
/// task cannot keep up with build output.
const LOG_CHANNEL_CAPACITY: usize = 64;
const LOG_DRAIN_GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(2);

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

    // API mode is the only supported mode. If private_key_path and server_url
    // are not set, fail immediately with a clear error — there is no DB fallback.
    if !builder_config.is_api_mode_ready() {
        anyhow::bail!(
            "Builder requires API mode configuration. \
             Set CRYSTAL_FORGE__BUILDER__PRIVATE_KEY_PATH and \
             CRYSTAL_FORGE__BUILDER__SERVER_URL (or equivalent TOML keys). \
             Legacy direct-database mode has been removed."
        );
    }

    info!("🌐 Starting Crystal Forge Builder in API mode...");
    run_api_mode(&cfg).await
}

/// Run builder in API mode (communicates with server via API instead of direct DB)
async fn run_api_mode(cfg: &CrystalForgeConfig) -> anyhow::Result<()> {
    let builder_config = cfg.get_builder_config();
    let build_config = cfg.get_build_config();
    let cache_config = cfg.get_cache_config();

    info!("Initializing API client...");
    let api_client = BuilderApiClient::new(builder_config).await?;

    info!("✅ Builder ID: {}", api_client.builder_id());
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

    // Cache push and CVE scanning are handled via the server API in API mode and
    // are queued/performed server-side. Remote builders do not open a database
    // connection for these loops.
    info!("📤 Cache push queued server-side on job completion (no builder DB pool)");
    info!("🔍 CVE scanning handled server-side (no builder DB pool)");

    // Spawn job polling loop
    let poll_client = api_client.clone();
    let poll_interval = builder_config.poll_interval;
    let max_concurrent = builder_config
        .max_concurrent_jobs
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(1);

    info!(
        "🔨 Starting job polling loop (max concurrent: {})...",
        max_concurrent
    );

    tokio::select! {
        result = run_api_job_loop(
            poll_client,
            poll_interval,
            max_concurrent,
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
    max_concurrent: usize,
    build_config: crystal_forge::config::BuildConfig,
    cache_config: crystal_forge::config::CacheConfig,
    execution_mode: crystal_forge::config::ExecutionMode,
) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(poll_interval);

    // Limit concurrent builds to builder.max_concurrent_jobs
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
            Ok(Some(NextJobResponse { job, derivation })) => {
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
                let job_id = job.id;

                tokio::spawn(async move {
                    execute_build_job(
                        job_id,
                        derivation,
                        job_client,
                        job_build_config,
                        job_cache_config,
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

/// Execute a build job entirely over the API: build the supplied derivation,
/// stream logs, report progress/cancellation, and report results. No DB access.
async fn execute_build_job(
    job_id: uuid::Uuid,
    derivation_payload: BuildJobDerivation,
    client: BuilderApiClient,
    build_config: crystal_forge::config::BuildConfig,
    cache_config: crystal_forge::config::CacheConfig,
    execution_mode: crystal_forge::config::ExecutionMode,
) {
    info!(
        "🔨 Starting build for job #{} (derivation: {})",
        job_id, derivation_payload.id
    );

    // Build a Derivation from the API payload — no database read required.
    let mut derivation =
        crystal_forge::derivations::Derivation::from_build_payload(&derivation_payload);

    // API-backed reporter: progress + cancel checks over HTTP (no DB pool).
    let reporter = ApiBuildReporter::new(client.clone(), job_id);

    info!("📦 Building derivation: {}", derivation.derivation_name);

    // Respect the configured build timeout for remote API builders. Nix itself
    // receives build_config.timeout; the wrapper gets a small cleanup buffer so
    // it can observe/report Nix's timeout rather than racing it.
    let build_timeout = build_config.process_timeout();

    let start = std::time::Instant::now();

    // Try to connect WebSocket for real-time log streaming
    let ws_stream = match client.create_log_stream(&job_id).await {
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
    send_log_with_fallback(&client, job_id, &mut ws_shared, &initial_log).await;

    if let Some(drv_path) = derivation.derivation_path.as_deref() {
        if let Err(e) = ensure_derivation_available(&client, job_id, drv_path).await {
            let message = format!(
                "[crystal-forge] failed to import derivation archive for {}: {}\n",
                drv_path, e
            );
            send_log_with_fallback(&client, job_id, &mut ws_shared, &message).await;
            error!("❌ Job #{} cannot start build: {}", job_id, e);
            if let Err(report_err) = client.fail_job(job_id, &e.to_string()).await {
                error!(
                    "❌ Failed to report job #{} derivation import failure: {}",
                    job_id, report_err
                );
            }
            return;
        }
    }

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

    // Set up bounded channel for log streaming with backpressure.
    // The forwarding task receives batched log content and sends it to ws/http.
    let (log_tx, mut log_rx) = mpsc::channel::<String>(LOG_CHANNEL_CAPACITY);
    let dropped_log_batches = Arc::new(AtomicUsize::new(0));

    // Clone references for the forwarding task
    let fwd_client = client.clone();
    let fwd_job_id = job_id;
    let fwd_ws = ws_shared.clone();

    // Spawn log forwarding task - receives batched content from channel
    let log_forward_task = tokio::spawn(async move {
        let mut ws_local = fwd_ws;
        while let Some(batch) = log_rx.recv().await {
            send_log_with_fallback(&fwd_client, fwd_job_id, &mut ws_local, &batch).await;
        }
    });

    // Execute the build with timeout, or deterministic mock execution in dev mode.
    let build_result = if execution_mode.is_mock() {
        // Mock path: drop log_tx immediately since mock build doesn't use it.
        // This ensures the forwarding task can exit cleanly.
        drop(log_tx);
        let mock_result = run_mock_build(&mut derivation, &client, job_id, &mut ws_shared).await;
        Ok(mock_result)
    } else {
        // Real path: create log_sink that sends to channel using try_send for backpressure.
        // The sink is created inline so it's dropped when the build completes.
        let log_sink: LogSink = {
            let tx = log_tx;
            let dropped_counter = dropped_log_batches.clone();
            Arc::new(move |batch: String| {
                // Use try_send to avoid blocking the build if the channel is full.
                // If full, the batch is dropped (acceptable for high-throughput builds).
                if let Err(e) = tx.try_send(batch) {
                    if matches!(e, mpsc::error::TrySendError::Full(_)) {
                        dropped_counter.fetch_add(1, Ordering::Relaxed);
                        tracing::debug!("Log channel full, dropping batch");
                    }
                }
            })
        };

        tokio::time::timeout(
            build_timeout,
            derivation.build_with_log_sink(&reporter, &build_config, Some(job_id), Some(log_sink)),
        )
        .await
    };

    // Wait for forwarding task to drain remaining messages, but do not block
    // job completion indefinitely under heavy log pressure.
    let mut log_forward_task = log_forward_task;
    match tokio::time::timeout(LOG_DRAIN_GRACE_PERIOD, &mut log_forward_task).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            warn!("log forwarding task failed for job {}: {}", job_id, e);
        }
        Err(_) => {
            warn!(
                "log drain grace period exceeded for job {}, aborting forwarder",
                job_id
            );
            log_forward_task.abort();
            send_log_with_fallback(
                &client,
                job_id,
                &mut ws_shared,
                "[crystal-forge] warning: log drain timed out; tail output may be truncated\n",
            )
            .await;
        }
    }

    let dropped_count = dropped_log_batches.load(Ordering::Relaxed);
    if dropped_count > 0 {
        let notice = format!(
            "[crystal-forge] warning: dropped {} buffered log batch(es) due to backpressure\n",
            dropped_count
        );
        send_log_with_fallback(&client, job_id, &mut ws_shared, &notice).await;
    }

    match build_result {
        // Build succeeded within timeout
        Ok(Ok(store_path)) => {
            let duration = start.elapsed();
            info!(
                "✅ Job #{} completed in {:.1}s: {}",
                job_id,
                duration.as_secs_f64(),
                store_path
            );

            // Send success log
            let success_msg = format!(
                "✅ Build completed successfully in {:.1}s\n",
                duration.as_secs_f64()
            );
            let output_msg = format!("   Output: {}\n", store_path);

            send_log_with_fallback(&client, job_id, &mut ws_shared, &success_msg).await;
            send_log_with_fallback(&client, job_id, &mut ws_shared, &output_msg).await;

            // Update derivation with store_path for signing
            derivation.store_path = Some(store_path.clone());

            if execution_mode.is_mock() {
                let _ = client
                    .append_logs(
                        job_id,
                        "🧪 MOCK MODE: skipping signing and cache push for synthetic artifacts\n",
                    )
                    .await;
            } else {
                // Sign the derivation locally before reporting completion.
                let _ = client
                    .append_logs(job_id, "🔐 Signing derivation...\n")
                    .await;
                if let Err(e) = derivation.sign(&cache_config).await {
                    warn!(
                        "⚠️ Signing failed for job #{}, continuing anyway: {}",
                        job_id, e
                    );
                    let _ = client
                        .append_logs(job_id, &format!("⚠️  Signing failed: {}\n", e))
                        .await;
                } else {
                    let _ = client.append_logs(job_id, "✅ Derivation signed\n").await;
                }
            }

            // Report success to the server. The server marks the derivation
            // complete and queues the cache-push job (no builder DB access).
            if let Err(e) = client.complete_job(job_id, &store_path).await {
                error!("❌ Failed to report job #{} completion: {}", job_id, e);
            }
        }

        // Build was cancelled by an operator (server set status to 'cancelling')
        Ok(Err(ref e)) if e.downcast_ref::<BuildCancelledError>().is_some() => {
            info!("🛑 Job #{} cancelled by operator — finalizing", job_id);

            // Append cancellation notice to build log
            send_log_with_fallback(
                &client,
                job_id,
                &mut ws_shared,
                "[crystal-forge] Build cancelled by operator — nix process stopped\n",
            )
            .await;

            // Call finalize-cancelled so the server sets completed_at and closes
            // the job cleanly.  If this fails we log and move on — the job will
            // remain in 'cancelling' until the next reconciliation.
            if let Err(e) = client.finalize_cancelled_job(job_id).await {
                error!("❌ Failed to finalize cancelled job #{}: {}", job_id, e);
            }
        }

        // Build failed within timeout
        Ok(Err(e)) => {
            let duration = start.elapsed();
            error!(
                "❌ Job #{} build failed after {:.1}s: {}",
                job_id,
                duration.as_secs_f64(),
                e
            );

            // Send failure log
            send_log_with_fallback(
                &client,
                job_id,
                &mut ws_shared,
                &format!("❌ Build failed after {:.1}s\n", duration.as_secs_f64()),
            )
            .await;
            send_log_with_fallback(
                &client,
                job_id,
                &mut ws_shared,
                &format!("   Error: {}\n", e),
            )
            .await;

            // Report failure to the server, which records the derivation-level
            // failure server-side (no builder DB access).
            if let Err(report_err) = client.fail_job(job_id, &e.to_string()).await {
                error!(
                    "❌ Failed to report job #{} failure: {}",
                    job_id, report_err
                );
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

            error!("⏱️ Job #{}: {}", job_id, timeout_msg);

            // Send timeout log
            send_log_with_fallback(
                &client,
                job_id,
                &mut ws_shared,
                &format!("⏱️  {}\n", timeout_msg),
            )
            .await;

            // Report timeout failure to the server (server records derivation failure).
            if let Err(report_err) = client.fail_job(job_id, &timeout_msg).await {
                error!(
                    "❌ Failed to report job #{} timeout failure: {}",
                    job_id, report_err
                );
            }
        }
    }

    // Clean up metrics task if it was spawned
    if let Some(task) = metrics_task {
        task.abort();
    }
}

async fn ensure_derivation_available(
    client: &BuilderApiClient,
    job_id: uuid::Uuid,
    drv_path: &str,
) -> anyhow::Result<()> {
    if Path::new(drv_path).exists() {
        return Ok(());
    }

    info!(
        "📤 Derivation {} missing locally; asking server to publish closure to cache",
        drv_path
    );

    match client.publish_derivation_closure(job_id).await {
        Ok(()) => {
            info!(
                "✅ Server published derivation closure for {}; continuing with Nix substituters",
                drv_path
            );
            return Ok(());
        }
        Err(e) => {
            warn!(
                "⚠️  Server cache publish unavailable for {}; falling back to archive download: {}",
                drv_path, e
            );
        }
    }

    info!(
        "📥 Derivation {} missing locally; downloading archive from server",
        drv_path
    );
    let archive = client.download_derivation_archive(job_id).await?;

    let mut child = tokio::process::Command::new("nix-store")
        .arg("--import")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn nix-store --import: {e}"))?;

    let Some(mut stdin) = child.stdin.take() else {
        anyhow::bail!("failed to open stdin for nix-store --import");
    };
    stdin.write_all(&archive).await?;
    drop(stdin);

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nix-store --import failed: {stderr}");
    }

    if !Path::new(drv_path).exists() {
        anyhow::bail!("import completed but derivation path is still missing: {drv_path}");
    }

    info!("✅ Imported derivation archive for {}", drv_path);
    Ok(())
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

    send_log_with_fallback(client, job_id, ws_shared, "⚙️  [65%] Building outputs...\n").await;
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
        anyhow::bail!("MOCK build failure for {}", derivation.derivation_name);
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
