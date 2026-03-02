use crystal_forge::builder::api_client::BuilderApiClient;
use crystal_forge::builder::metrics::SystemMetrics;
use crystal_forge::builder::{run_build_loop, run_cache_push_loop, run_cve_scan_loop, get_gc_root_path};
use crystal_forge::config::CrystalForgeConfig;
use crystal_forge::derivations::Derivation;
use crystal_forge::models::builders::{BuildJob, ReportMetricsRequest};
use crystal_forge::queries::derivations::get_derivation_by_id;
use crystal_forge::server::memory_monitor_task;
use anyhow::Result;
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
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

                // Execute the build in a spawned task to allow concurrent builds
                let job_client = client.clone();
                let job_build_config = build_config.clone();
                let job_cache_config = cache_config.clone();
                
                tokio::spawn(async move {
                    execute_build_job(job, job_client, job_build_config, job_cache_config).await;
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
) {
    info!("🔨 Starting build for job #{} (derivation: {})", job.id, job.derivation_id);

    // We need database access to fetch the derivation
    // In API mode, we still need a database connection for build operations
    let pool = match crystal_forge::config::CrystalForgeConfig::db_pool().await {
        Ok(pool) => pool,
        Err(e) => {
            error!("❌ Failed to connect to database for job #{}: {}", job.id, e);
            if let Err(e2) = client.fail_job(job.id, &format!("Database connection failed: {}", e)).await {
                error!("❌ Failed to report job failure: {}", e2);
            }
            return;
        }
    };

    // Fetch the derivation from database
    let mut derivation = match get_derivation_by_id(&pool, job.derivation_id).await {
        Ok(deriv) => deriv,
        Err(e) => {
            error!("❌ Failed to fetch derivation {}: {}", job.derivation_id, e);
            if let Err(e2) = client.fail_job(job.id, &format!("Failed to fetch derivation: {}", e)).await {
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

    // Send initial log message
    let _ = client.append_logs(job.id, &format!("🔨 Starting build for {}\n", derivation.derivation_name)).await;

    // Create a log streaming task
    let (log_tx, mut log_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let log_client = client.clone();
    let log_job_id = job.id;
    
    // Spawn task to batch and send logs every 2 seconds
    let log_handle = tokio::spawn(async move {
        let mut buffer = String::new();
        let mut last_send = std::time::Instant::now();
        let send_interval = std::time::Duration::from_secs(2);
        
        loop {
            tokio::select! {
                Some(line) = log_rx.recv() => {
                    buffer.push_str(&line);
                    buffer.push('\n');
                    
                    // Send if buffer is large (>4KB) or enough time passed
                    if buffer.len() > 4096 || last_send.elapsed() >= send_interval {
                        if !buffer.is_empty() {
                            if let Err(e) = log_client.append_logs(log_job_id, &buffer).await {
                                warn!("Failed to send logs: {}", e);
                            }
                            buffer.clear();
                            last_send = std::time::Instant::now();
                        }
                    }
                }
                // Periodic flush
                _ = tokio::time::sleep(send_interval) => {
                    if !buffer.is_empty() {
                        if let Err(e) = log_client.append_logs(log_job_id, &buffer).await {
                            warn!("Failed to send logs: {}", e);
                        }
                        buffer.clear();
                        last_send = std::time::Instant::now();
            }
        }
    }
}

/// Build a derivation with real-time log streaming
async fn build_with_log_streaming(
    derivation: &mut Derivation,
    pool: &PgPool,
    build_config: &crystal_forge::config::BuildConfig,
    log_tx: tokio::sync::mpsc::UnboundedSender<String>,
) -> Result<String> {
    let drv_path = derivation.derivation_path.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Derivation missing derivation_path"))?;

    info!("🔨 Building with log streaming: {}", drv_path);

    let gc_root_path = get_gc_root_path(derivation.id).await;

    // Build the command (same as derivation.build())
    let mut cmd = if build_config.should_use_systemd() {
        let mut scoped = Command::new("systemd-run");
        scoped.args(["--scope", "--collect", "--quiet"]);
        // Apply systemd properties
        if let Some(cpu) = build_config.max_cpu_cores {
            scoped.args(["--property", &format!("CPUQuota={}%", cpu * 100)]);
        }
        if let Some(mem_mb) = build_config.max_memory_mb {
            scoped.args(["--property", &format!("MemoryMax={}M", mem_mb)]);
        }
        scoped.args([
            "--",
            "nix-store",
            "--realise",
            "--add-root",
            &gc_root_path,
            "--indirect",
            drv_path,
        ]);
        scoped
    } else {
        let mut direct = Command::new("nix-store");
        direct.args([
            "--realise",
            "--add-root",
            &gc_root_path,
            "--indirect",
            drv_path,
        ]);
        direct
    };

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    build_config.apply_to_command(&mut cmd);

    let mut child = cmd.spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn build process: {}", e))?;

    let stdout = child.stdout.take().expect("Failed to capture stdout");
    let stderr = child.stderr.take().expect("Failed to capture stderr");

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // Stream logs line by line
    loop {
        tokio::select! {
            line_result = stdout_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        info!("build: {}", line);
                        let _ = log_tx.send(line);
                    }
                    Ok(None) => break,
                    Err(e) => {
                        error!("Error reading stdout: {}", e);
                        break;
                    }
                }
            }
            line_result = stderr_reader.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        warn!("build stderr: {}", line);
                        let _ = log_tx.send(line);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("Error reading stderr: {}", e);
                    }
                }
            }
        }
    }

    let status = child.wait().await?;

    if !status.success() {
        return Err(anyhow::anyhow!("Build failed with status: {}", status));
    }

    // Parse output path from gc_root symlink
    let output_path = tokio::fs::read_link(&gc_root_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read GC root symlink: {}", e))?;
    
    Ok(output_path.to_string_lossy().to_string())
}
    });

    // Execute the build with timeout and log streaming
    let build_result = tokio::time::timeout(
        build_timeout,
        build_with_log_streaming(&mut derivation, &pool, &build_config, log_tx)
    ).await;
    
    // Stop log streaming
    log_handle.abort();

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
            let _ = client.append_logs(job.id, &format!("✅ Build completed successfully in {:.1}s\n", duration.as_secs_f64())).await;
            let _ = client.append_logs(job.id, &format!("   Output: {}\n", store_path)).await;

            // Update derivation with store_path for signing
            derivation.store_path = Some(store_path.clone());

            // Sign the derivation
            let _ = client.append_logs(job.id, "🔐 Signing derivation...\n").await;
            if let Err(e) = derivation.sign(&cache_config).await {
                warn!("⚠️ Signing failed for job #{}, continuing anyway: {}", job.id, e);
                let _ = client.append_logs(job.id, &format!("⚠️  Signing failed: {}\n", e)).await;
            } else {
                let _ = client.append_logs(job.id, "✅ Derivation signed\n").await;
            }

            // Create cache push job if configured
            if cache_config.push_after_build {
                let _ = client.append_logs(job.id, "📤 Queuing cache push job...\n").await;
                if let Some(ref store_path) = derivation.store_path {
                    if let Err(e) = crystal_forge::queries::cache_push::create_cache_push_job(
                        &pool,
                        derivation.id,
                        store_path,
                        cache_config.push_to.as_deref(),
                    ).await {
                        warn!("⚠️ Cache queue failed for job #{}, continuing anyway: {}", job.id, e);
                        let _ = client.append_logs(job.id, &format!("⚠️  Cache push queue failed: {}\n", e)).await;
                    } else {
                        let _ = client.append_logs(job.id, "✅ Cache push job queued\n").await;
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
                    ).await {
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
            let _ = client.append_logs(job.id, &format!("❌ Build failed after {:.1}s\n", duration.as_secs_f64())).await;
            let _ = client.append_logs(job.id, &format!("   Error: {}\n", e)).await;

            // Mark derivation as failed in database
            match pool.begin().await {
                Ok(mut tx) => {
                    if let Err(e2) = crystal_forge::queries::derivations::handle_derivation_failure(
                        &mut *tx,
                        &derivation,
                        "build",
                        &e,
                    ).await {
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
            let _ = client.append_logs(job.id, &format!("⏱️  {}\n", timeout_msg)).await;

            let timeout_error = anyhow::anyhow!(timeout_msg.clone());

            // Mark derivation as failed in database
            match pool.begin().await {
                Ok(mut tx) => {
                    if let Err(e) = crystal_forge::queries::derivations::handle_derivation_failure(
                        &mut *tx,
                        &derivation,
                        "build-timeout",
                        &timeout_error,
                    ).await {
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
}
