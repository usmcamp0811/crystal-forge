use crystal_forge::builder::api_client::{ApiBuildReporter, BuilderApiClient};
use crystal_forge::builder::metrics::SystemMetrics;
use crystal_forge::config::{CacheConfig, CacheType, CrystalForgeConfig};
use crystal_forge::derivations::build::{BuildCancelledError, LogSink};
use crystal_forge::models::builders::{
    BuildFailurePhase, BuildJobDerivation, NextJobResponse, RemoteBuildExecutionStrategy,
    ReportMetricsRequest, SourceInputDeliveryMode, VerifiedSourceIdentity,
};
#[allow(deprecated)]
use nix::fcntl::{FlockArg, flock};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// Tracks the active pre-build phase for accurate timeout failure reporting.
/// - `PRE_BUILD_SOURCE_FETCH`: source-fetch operations (lock, clone, fetch, worktree)
/// - `PRE_BUILD_EVALUATION`: nix eval phase
const PRE_BUILD_SOURCE_FETCH: u8 = 0;
const PRE_BUILD_EVALUATION: u8 = 1;

use tokio::signal;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// Channel capacity for log streaming. Provides backpressure when the forwarding
/// task cannot keep up with build output.
const LOG_CHANNEL_CAPACITY: usize = 64;
const LOG_DRAIN_GRACE_PERIOD: std::time::Duration = std::time::Duration::from_secs(2);

fn required_cache_push_reference(
    cache_config: &CacheConfig,
    target_name: &str,
    store_path: &str,
) -> anyhow::Result<String> {
    if !cache_config.should_push(target_name) {
        anyhow::bail!(
            "cache push is disabled or filtered out for target {target_name}; refusing to report a deployable build"
        );
    }

    if cache_config.cache_command(store_path).is_none() {
        anyhow::bail!(
            "cache push is enabled for {target_name}, but no cache push command can be built from builder configuration"
        );
    }

    match cache_config.cache_type {
        CacheType::Attic => cache_config
            .attic_cache_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("Attic cache push requires attic_cache_name")),
        CacheType::S3 | CacheType::Http | CacheType::Nix => cache_config
            .push_to
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("cache push requires push_to")),
    }
}

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

    let builder_id = api_client.builder_id();
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

    // Remote API builders must push successful outputs from the builder host,
    // because the built closure may not exist in the server's local store.
    info!("📤 Cache push performed builder-side after successful builds");
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
            RemoteBuildRuntime {
                supported_execution_strategies: builder_config
                    .supported_execution_strategies
                    .clone(),
                source_mirror_root: builder_config.source_mirror_root.clone(),
                source_worktree_root: builder_config.source_worktree_root.clone(),
                cleanup_source_worktrees: builder_config.cleanup_source_worktrees,
            },
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

/// Builder-side configuration for remote build strategy selection and verified
/// source workspace management.
#[derive(Debug, Clone)]
struct RemoteBuildRuntime {
    supported_execution_strategies: Vec<RemoteBuildExecutionStrategy>,
    source_mirror_root: PathBuf,
    source_worktree_root: PathBuf,
    cleanup_source_worktrees: bool,
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
    remote_runtime: RemoteBuildRuntime,
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

                if !remote_runtime
                    .supported_execution_strategies
                    .contains(&derivation.execution_strategy)
                {
                    let message = format!(
                        "builder does not support execution strategy {:?}",
                        derivation.execution_strategy
                    );
                    error!("❌ Job #{} rejected: {}", job.id, message);
                    if let Err(report_err) = client
                        .fail_job_with_phase(job.id, BuildFailurePhase::Build, &message)
                        .await
                    {
                        error!(
                            "❌ Failed to report job #{} unsupported strategy: {}",
                            job.id, report_err
                        );
                    }
                    continue;
                }

                // Acquire semaphore permit for this build
                let permit = semaphore.clone().acquire_owned().await.unwrap();

                // Execute the build in a spawned task to allow concurrent builds
                let job_client = client.clone();
                let job_build_config = build_config.clone();
                let job_cache_config = cache_config.clone();
                let job_remote_runtime = remote_runtime.clone();
                let job_id = job.id;

                tokio::spawn(async move {
                    execute_build_job(
                        job_id,
                        derivation,
                        job_client,
                        job_build_config,
                        job_cache_config,
                        execution_mode,
                        job_remote_runtime,
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
#[derive(Debug)]
struct PreBuildFailure {
    phase: BuildFailurePhase,
    message: String,
}

#[derive(Debug)]
enum VerificationOutcome {
    Completed(Result<String, PreBuildFailure>),
    Cancelled,
}

async fn wait_for_pre_build_verification<F, C, CFuture>(
    verification_future: F,
    build_timeout: std::time::Duration,
    pre_build_phase: &AtomicU8,
    mut cancellation_requested: C,
) -> VerificationOutcome
where
    F: std::future::Future<Output = Result<String, PreBuildFailure>>,
    C: FnMut() -> CFuture,
    CFuture: std::future::Future<Output = bool>,
{
    tokio::pin!(verification_future);
    let timeout = tokio::time::sleep(build_timeout);
    tokio::pin!(timeout);
    let mut cancel_poll = tokio::time::interval(std::time::Duration::from_secs(5));

    loop {
        tokio::select! {
            result = &mut verification_future => break VerificationOutcome::Completed(result),
            _ = &mut timeout => {
                let phase = match pre_build_phase.load(Ordering::SeqCst) {
                    PRE_BUILD_EVALUATION => BuildFailurePhase::Evaluation,
                    _ => BuildFailurePhase::SourceFetch,
                };
                break VerificationOutcome::Completed(Err(PreBuildFailure {
                    phase,
                    message: format!(
                        "verified source pre-build {} timed out after {:?}",
                        if phase == BuildFailurePhase::Evaluation {
                            "evaluation"
                        } else {
                            "source-fetch"
                        },
                        build_timeout
                    ),
                }));
            }
            _ = cancel_poll.tick() => {
                if cancellation_requested().await {
                    break VerificationOutcome::Cancelled;
                }
            }
        }
    }
}

fn expected_drv_path(payload: &BuildJobDerivation) -> Result<&str, PreBuildFailure> {
    payload
        .expected_drv_path
        .as_deref()
        .or(payload.derivation_path.as_deref())
        .filter(|drv| drv.ends_with(".drv"))
        .ok_or_else(|| PreBuildFailure {
            phase: BuildFailurePhase::Evaluation,
            message: "verified source job is missing expected .drvPath".to_string(),
        })
}

fn source_flake_ref(
    source: &VerifiedSourceIdentity,
    delivery: SourceInputDeliveryMode,
    mirror_root: &Path,
    worktree_root: &Path,
) -> Result<String, PreBuildFailure> {
    if delivery == SourceInputDeliveryMode::LocalGitWorktree {
        return source_workspace_paths(source, mirror_root, worktree_root)
            .map(|(_, worktree_path)| worktree_path)
            .map(|path| path.to_string_lossy().to_string());
    }

    if let Some(archive_url) = source.archive_url.as_deref() {
        return Ok(archive_url
            .strip_prefix("file://")
            .unwrap_or(archive_url)
            .to_string());
    }

    match delivery {
        SourceInputDeliveryMode::BuilderFetchPublicInputs => Ok(format!(
            "git+{}?rev={}",
            source.repo_url, source.commit_hash
        )),
        SourceInputDeliveryMode::LocalGitWorktree => {
            unreachable!("handled before archive fallback")
        }
        SourceInputDeliveryMode::ServerBundledArchive => Err(PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: "server-bundled source delivery selected but archive_url is missing"
                .to_string(),
        }),
        SourceInputDeliveryMode::None => Err(PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: "verified source job is missing source delivery metadata".to_string(),
        }),
    }
}

fn source_workspace_paths(
    source: &VerifiedSourceIdentity,
    mirror_root: &Path,
    worktree_root: &Path,
) -> Result<(PathBuf, PathBuf), PreBuildFailure> {
    source_workspace_paths_for_job(source, mirror_root, worktree_root, None)
}

fn source_workspace_paths_for_job(
    source: &VerifiedSourceIdentity,
    mirror_root: &Path,
    worktree_root: &Path,
    job_id: Option<uuid::Uuid>,
) -> Result<(PathBuf, PathBuf), PreBuildFailure> {
    if let Some(path) = source.worktree_path.as_deref() {
        let mirror_path = source
            .mirror_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| mirror_root.join("external-source.git"));
        return Ok((mirror_path, PathBuf::from(path)));
    }

    let mirror_id = source
        .mirror_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: "local git worktree delivery is missing mirror_id".to_string(),
        })?;

    let mirror_path = source
        .mirror_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| mirror_root.join(format!("{mirror_id}.git")));

    let worktree_path = match job_id {
        Some(job_id) => worktree_root
            .join(mirror_id)
            .join(&source.commit_hash)
            .join(job_id.to_string()),
        None => worktree_root.join(mirror_id).join(&source.commit_hash),
    };

    Ok((mirror_path, worktree_path))
}

struct MirrorLock {
    _file: File,
}

struct TempPathCleanup {
    path: PathBuf,
    armed: bool,
}

impl TempPathCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempPathCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

async fn acquire_mirror_lock(mirror_path: &Path) -> Result<MirrorLock, PreBuildFailure> {
    let lock_path = mirror_path.with_extension("git.lock");
    if let Some(parent) = lock_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| PreBuildFailure {
                phase: BuildFailurePhase::SourceFetch,
                message: format!(
                    "failed to create source mirror lock parent {}: {e}",
                    parent.display()
                ),
            })?;
    }

    let file = File::options()
        .create(true)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!(
                "failed to open source mirror lock {}: {e}",
                lock_path.display()
            ),
        })?;

    loop {
        #[allow(deprecated)]
        match flock(file.as_raw_fd(), FlockArg::LockExclusiveNonblock) {
            Ok(()) => return Ok(MirrorLock { _file: file }),
            Err(e) if e == nix::errno::Errno::EWOULDBLOCK || e == nix::errno::Errno::EAGAIN => {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Err(e) => {
                return Err(PreBuildFailure {
                    phase: BuildFailurePhase::SourceFetch,
                    message: format!(
                        "failed to acquire source mirror lock {}: {e}",
                        lock_path.display()
                    ),
                });
            }
        }
    }
}

/// Ensure a bare mirror exists at `mirror_path` and contains `commit_hash`.
///
/// If the mirror is missing it is created (`git clone --bare`). If the requested
/// commit is not present, the mirror is fetched. This lets a fresh builder
/// populate its local source copy directly from the repository URL without a
/// pre-seeded mirror, while still building from the exact authorized commit.
async fn ensure_mirror_has_commit(
    mirror_path: &Path,
    repo_url: &str,
    commit_hash: &str,
) -> Result<(), PreBuildFailure> {
    let _lock = acquire_mirror_lock(mirror_path).await?;
    let source_fetch = |message: String| PreBuildFailure {
        phase: BuildFailurePhase::SourceFetch,
        message,
    };

    if !mirror_path.exists() {
        let temp_mirror_path = mirror_path.with_extension(format!(
            "git.tmp-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        info!(
            "🪞 Source mirror missing; cloning {} into {}",
            repo_url,
            temp_mirror_path.display()
        );
        if let Some(parent) = mirror_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                source_fetch(format!(
                    "failed to create mirror parent {}: {e}",
                    parent.display()
                ))
            })?;
        }

        let _ = tokio::fs::remove_dir_all(&temp_mirror_path).await;
        let mut temp_mirror_cleanup = TempPathCleanup::new(temp_mirror_path.clone());

        let output = tokio::process::Command::new("git")
            .kill_on_drop(true)
            .arg("clone")
            .arg("--bare")
            .arg(repo_url)
            .arg(&temp_mirror_path)
            .output()
            .await
            .map_err(|e| source_fetch(format!("failed to spawn git clone --bare: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(source_fetch(format!(
                "git clone --bare failed for {repo_url}: {stderr}"
            )));
        }

        tokio::fs::rename(&temp_mirror_path, mirror_path)
            .await
            .map_err(|e| {
                source_fetch(format!(
                    "failed to install cloned source mirror {} -> {}: {e}",
                    temp_mirror_path.display(),
                    mirror_path.display()
                ))
            })?;
        temp_mirror_cleanup.disarm();

        info!("✅ Source mirror cloned at {}", mirror_path.display());
    }

    // If the commit is already present, no fetch is required.
    let has_commit = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(mirror_path)
        .arg("cat-file")
        .arg("-e")
        .arg(format!("{commit_hash}^{{commit}}"))
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);

    if has_commit {
        info!(
            "✅ Source mirror {} already has commit {}",
            mirror_path.display(),
            commit_hash
        );
        return Ok(());
    }

    info!(
        "🔄 Fetching authorized commit {} into source mirror {}",
        commit_hash,
        mirror_path.display()
    );

    let output = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(mirror_path)
        .arg("fetch")
        .arg("--prune")
        .arg(repo_url)
        .arg("+refs/*:refs/*")
        .output()
        .await
        .map_err(|e| source_fetch(format!("failed to spawn git fetch: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(source_fetch(format!(
            "git fetch failed for {repo_url}: {stderr}"
        )));
    }

    let has_commit_after = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(mirror_path)
        .arg("cat-file")
        .arg("-e")
        .arg(format!("{commit_hash}^{{commit}}"))
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);

    if has_commit_after {
        info!(
            "✅ Source mirror {} fetched commit {}",
            mirror_path.display(),
            commit_hash
        );
        Ok(())
    } else {
        Err(source_fetch(format!(
            "commit {commit_hash} not found in mirror for {repo_url} after fetch"
        )))
    }
}

async fn ensure_source_worktree(
    source: &VerifiedSourceIdentity,
    mirror_root: &Path,
    worktree_root: &Path,
    job_id: uuid::Uuid,
) -> Result<PathBuf, PreBuildFailure> {
    let (mirror_path, worktree_path) =
        source_workspace_paths_for_job(source, mirror_root, worktree_root, Some(job_id))?;

    if worktree_path.exists() {
        info!("🌳 Reusing source worktree {}", worktree_path.display());
        verify_worktree_head(&worktree_path, &source.commit_hash).await?;
        return Ok(worktree_path);
    }

    ensure_mirror_has_commit(&mirror_path, &source.repo_url, &source.commit_hash).await?;
    info!(
        "🌳 Creating detached source worktree {} at commit {}",
        worktree_path.display(),
        source.commit_hash
    );

    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| PreBuildFailure {
                phase: BuildFailurePhase::SourceFetch,
                message: format!(
                    "failed to create source worktree parent {}: {e}",
                    parent.display()
                ),
            })?;
    }

    let output = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(&mirror_path)
        .arg("worktree")
        .arg("add")
        .arg("--detach")
        .arg(&worktree_path)
        .arg(&source.commit_hash)
        .output()
        .await
        .map_err(|e| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!("failed to spawn git worktree add: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!("git worktree add failed: {stderr}"),
        });
    }

    verify_worktree_head(&worktree_path, &source.commit_hash).await?;
    info!("✅ Source worktree ready at {}", worktree_path.display());
    Ok(worktree_path)
}

/// Create a detached source worktree from an already-populated mirror.
///
/// Unlike [`ensure_source_worktree`], this does NOT call
/// [`ensure_mirror_has_commit`] — the caller is responsible for ensuring the
/// mirror already contains the authorized commit (e.g. by extracting a
/// server-provided archive). Used for `ServerBundledArchive` delivery.
///
/// `mirror_path` must be the job-scoped bare mirror directory
/// (`mirror_root/server-bundled/<job_id>/<mirror_id>.git`). The worktree is
/// placed at `worktree_root/<mirror_id>/<commit>/<job_id>` so each job has
/// an isolated worktree that does not race with other concurrent jobs.
async fn ensure_source_worktree_from_mirror(
    source: &VerifiedSourceIdentity,
    mirror_path: &Path,
    worktree_root: &Path,
    job_id: uuid::Uuid,
) -> Result<PathBuf, PreBuildFailure> {
    // Derive the worktree path — job-scoped under <worktree_root>/<mirror_id>/<commit>/<job_id>.
    let mirror_id = source
        .mirror_id
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: "ServerBundledArchive delivery is missing mirror_id for worktree path"
                .to_string(),
        })?;
    let worktree_path = worktree_root
        .join(mirror_id)
        .join(&source.commit_hash)
        .join(job_id.to_string());

    if worktree_path.exists() {
        info!("🌳 Reusing source worktree {}", worktree_path.display());
        verify_worktree_head(&worktree_path, &source.commit_hash).await?;
        return Ok(worktree_path);
    }

    info!(
        "🌳 Creating detached source worktree {} at commit {} (from pre-populated mirror)",
        worktree_path.display(),
        source.commit_hash
    );

    if let Some(parent) = worktree_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| PreBuildFailure {
                phase: BuildFailurePhase::SourceFetch,
                message: format!(
                    "failed to create source worktree parent {}: {e}",
                    parent.display()
                ),
            })?;
    }

    let output = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("--git-dir")
        .arg(&mirror_path)
        .arg("worktree")
        .arg("add")
        .arg("--detach")
        .arg(&worktree_path)
        .arg(&source.commit_hash)
        .output()
        .await
        .map_err(|e| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!("failed to spawn git worktree add: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!("git worktree add failed: {stderr}"),
        });
    }

    verify_worktree_head(&worktree_path, &source.commit_hash).await?;
    info!("✅ Source worktree ready at {}", worktree_path.display());
    Ok(worktree_path)
}

async fn verify_worktree_head(
    worktree_path: &Path,
    expected_commit: &str,
) -> Result<(), PreBuildFailure> {
    let output = tokio::process::Command::new("git")
        .kill_on_drop(true)
        .arg("-C")
        .arg(worktree_path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .await
        .map_err(|e| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!(
                "failed to inspect source worktree {}: {e}",
                worktree_path.display()
            ),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!(
                "git rev-parse failed for {}: {stderr}",
                worktree_path.display()
            ),
        });
    }

    let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if actual == expected_commit {
        Ok(())
    } else {
        Err(PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: format!(
                "source worktree {} is at {}, expected {}",
                worktree_path.display(),
                actual,
                expected_commit
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CleanupSourceWorktree {
    mirror_path: PathBuf,
    worktree_path: PathBuf,
    /// For ServerBundledArchive jobs: the job-scoped mirror *directory*
    /// (`mirror_root/server-bundled/<job_id>/`) to remove after the worktree
    /// is detached.  `None` for LocalGitWorktree jobs where the mirror is
    /// shared across jobs.
    job_mirror_dir: Option<PathBuf>,
}

fn cleanup_candidate_worktree(
    payload: &BuildJobDerivation,
    mirror_root: &Path,
    worktree_root: &Path,
    job_id: uuid::Uuid,
) -> Option<CleanupSourceWorktree> {
    if payload.execution_strategy != RemoteBuildExecutionStrategy::SourceReEvaluateVerified {
        return None;
    }

    let source = payload.source.as_ref()?;

    match payload.source_input_delivery {
        SourceInputDeliveryMode::LocalGitWorktree => {
            let (mirror_path, worktree_path) =
                source_workspace_paths_for_job(source, mirror_root, worktree_root, Some(job_id))
                    .ok()?;
            if worktree_path.starts_with(worktree_root) {
                Some(CleanupSourceWorktree {
                    mirror_path,
                    worktree_path,
                    job_mirror_dir: None,
                })
            } else {
                None
            }
        }
        SourceInputDeliveryMode::ServerBundledArchive => {
            // Job-scoped mirror: mirror_root/server-bundled/<job_id>/<mirror_id>.git
            let mirror_id = source.mirror_id.as_deref().filter(|v| !v.is_empty())?;
            let job_mirror_dir = mirror_root.join("server-bundled").join(job_id.to_string());
            let mirror_path = job_mirror_dir.join(format!("{mirror_id}.git"));
            // Worktree is shared-layout: worktree_root/<mirror_id>/<commit>/<job_id>
            let worktree_path = worktree_root
                .join(mirror_id)
                .join(&source.commit_hash)
                .join(job_id.to_string());
            if worktree_path.starts_with(worktree_root) {
                Some(CleanupSourceWorktree {
                    mirror_path,
                    worktree_path,
                    job_mirror_dir: Some(job_mirror_dir),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

async fn cleanup_source_worktree(cleanup: &CleanupSourceWorktree) {
    let path = cleanup.worktree_path.as_path();

    if path.exists() {
        let status = tokio::process::Command::new("git")
            .arg("--git-dir")
            .arg(&cleanup.mirror_path)
            .arg("worktree")
            .arg("remove")
            .arg("--force")
            .arg(path)
            .status()
            .await;

        match status {
            Ok(status) if status.success() => {
                info!("🧹 Removed source worktree {}", path.display());
            }
            Ok(status) => {
                warn!(
                    "source worktree cleanup command exited with status {} for {}",
                    status,
                    path.display()
                );
            }
            Err(e) => {
                warn!(
                    "failed to run source worktree cleanup for {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    // For ServerBundledArchive jobs, also remove the job-scoped mirror directory.
    // This dir (mirror_root/server-bundled/<job_id>/) was created exclusively for
    // this job so it is safe to delete in full after the worktree is gone.
    if let Some(ref job_mirror_dir) = cleanup.job_mirror_dir {
        if job_mirror_dir.exists() {
            match tokio::fs::remove_dir_all(job_mirror_dir).await {
                Ok(()) => {
                    info!(
                        "🧹 Removed job-scoped source mirror dir {}",
                        job_mirror_dir.display()
                    );
                }
                Err(e) => {
                    warn!(
                        "failed to remove job-scoped source mirror dir {}: {}",
                        job_mirror_dir.display(),
                        e
                    );
                }
            }
        }
    }
}

fn drv_path_eval_attr(source_ref: &str, flake_target: &str) -> String {
    let attr = flake_target
        .strip_suffix(".drvPath")
        .unwrap_or(flake_target);
    format!("{source_ref}#{attr}.drvPath")
}

fn verify_drv_identity(expected: &str, actual: &str) -> Result<(), PreBuildFailure> {
    if expected == actual {
        Ok(())
    } else {
        Err(PreBuildFailure {
            phase: BuildFailurePhase::DerivationMismatch,
            message: format!(
                "builder evaluated drvPath {actual}, expected server-authorized drvPath {expected}"
            ),
        })
    }
}

async fn evaluate_verified_source_drv(
    source: &VerifiedSourceIdentity,
    delivery: SourceInputDeliveryMode,
    mirror_root: &Path,
    worktree_root: &Path,
    job_id: uuid::Uuid,
    pre_build_phase: &AtomicU8,
    client: Option<&crystal_forge::builder::api_client::BuilderApiClient>,
) -> Result<String, PreBuildFailure> {
    let source_ref = if delivery == SourceInputDeliveryMode::ServerBundledArchive {
        // ServerBundledArchive: download the source archive from the server API,
        // extract it to the mirror path, then create a worktree from the mirror.
        let source_err = |msg: String| PreBuildFailure {
            phase: BuildFailurePhase::SourceFetch,
            message: msg,
        };

        let client = client.ok_or_else(|| {
            source_err(
                "ServerBundledArchive requires an API client for archive download".to_string(),
            )
        })?;

        // Determine the mirror_id for path construction.
        let mirror_id = source
            .mirror_id
            .as_deref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                source_err("ServerBundledArchive delivery is missing mirror_id".to_string())
            })?;

        // Job-scoped mirror path: each job extracts into its own directory so
        // concurrent jobs for the same repo never race on the same bare mirror.
        // Layout: mirror_root/server-bundled/<job_id>/<mirror_id>.git
        let job_mirror_dir = mirror_root.join("server-bundled").join(job_id.to_string());
        let mirror_path = job_mirror_dir.join(format!("{mirror_id}.git"));

        // Stream archive to a temp file, verifying SHA-256 incrementally.
        // This avoids buffering the entire archive in RAM.
        info!(
            "📦 Streaming source archive for job {} to temp file...",
            job_id
        );
        let tmp_archive = client
            .stream_source_archive_to_tempfile(
                job_id,
                source.archive_sha256.as_deref(),
                &job_mirror_dir,
            )
            .await
            .map_err(|e| source_err(format!("failed to stream source archive: {e}")))?;

        // Create the job-scoped mirror directory for extraction.
        tokio::fs::create_dir_all(&job_mirror_dir)
            .await
            .map_err(|e| {
                source_err(format!(
                    "failed to create job mirror directory {}: {e}",
                    job_mirror_dir.display()
                ))
            })?;

        let output = tokio::process::Command::new("tar")
            .kill_on_drop(true)
            .arg("-xzf")
            .arg(&tmp_archive)
            .arg("-C")
            .arg(&job_mirror_dir)
            .output()
            .await
            .map_err(|e| source_err(format!("failed to spawn tar extraction: {e}")))?;

        // Clean up the temp archive file regardless of extraction success.
        let _ = tokio::fs::remove_file(&tmp_archive).await;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            // Clean up the job mirror dir on failure to avoid stale state.
            let _ = tokio::fs::remove_dir_all(&job_mirror_dir).await;
            return Err(source_err(format!("tar extraction failed: {stderr}")));
        }

        info!(
            "✅ Source archive extracted to job-scoped mirror at {}",
            mirror_path.display()
        );

        // Create a worktree from the job-scoped extracted mirror.
        // ensure_source_worktree_from_mirror takes the mirror_path directly;
        // the worktree is placed at worktree_root/<mirror_id>/<commit>/<job_id>.
        ensure_source_worktree_from_mirror(source, &mirror_path, worktree_root, job_id)
            .await?
            .to_string_lossy()
            .to_string()
    } else if delivery == SourceInputDeliveryMode::LocalGitWorktree {
        ensure_source_worktree(source, mirror_root, worktree_root, job_id)
            .await?
            .to_string_lossy()
            .to_string()
    } else {
        source_flake_ref(source, delivery, mirror_root, worktree_root)?
    };
    let eval_attr = drv_path_eval_attr(&source_ref, &source.flake_target);
    info!("🔎 Evaluating verified source drvPath: {}", eval_attr);

    // Transition phase tracker to evaluation phase so timeouts are reported correctly.
    pre_build_phase.store(PRE_BUILD_EVALUATION, Ordering::SeqCst);

    let output = tokio::process::Command::new("nix")
        .kill_on_drop(true)
        .arg("eval")
        .arg("--raw")
        .arg("--no-write-lock-file")
        .arg("--option")
        .arg("allow-import-from-derivation")
        .arg("false")
        .arg(&eval_attr)
        .output()
        .await
        .map_err(|e| PreBuildFailure {
            phase: BuildFailurePhase::Evaluation,
            message: format!("failed to spawn nix eval for {eval_attr}: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(PreBuildFailure {
            phase: BuildFailurePhase::Evaluation,
            message: format!("nix eval failed for {eval_attr}: {stderr}"),
        });
    }

    let drv_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    info!("✅ Builder evaluated verified source drvPath: {}", drv_path);
    Ok(drv_path)
}

async fn verify_source_build_plan(
    payload: &BuildJobDerivation,
    mirror_root: &Path,
    worktree_root: &Path,
    job_id: uuid::Uuid,
    pre_build_phase: &AtomicU8,
    client: Option<&crystal_forge::builder::api_client::BuilderApiClient>,
) -> Result<String, PreBuildFailure> {
    let expected = expected_drv_path(payload)?.to_string();
    let source = payload.source.as_ref().ok_or_else(|| PreBuildFailure {
        phase: BuildFailurePhase::SourceFetch,
        message: "verified source job is missing immutable source identity".to_string(),
    })?;
    let actual = evaluate_verified_source_drv(
        source,
        payload.source_input_delivery,
        mirror_root,
        worktree_root,
        job_id,
        pre_build_phase,
        client,
    )
    .await?;
    verify_drv_identity(&expected, &actual)?;
    Ok(actual)
}

async fn execute_build_job(
    job_id: uuid::Uuid,
    mut derivation_payload: BuildJobDerivation,
    client: BuilderApiClient,
    build_config: crystal_forge::config::BuildConfig,
    local_cache_config: crystal_forge::config::CacheConfig,
    execution_mode: crystal_forge::config::ExecutionMode,
    remote_runtime: RemoteBuildRuntime,
) {
    info!(
        "🔨 Starting build for job #{} (derivation: {})",
        job_id, derivation_payload.id
    );

    let cache_config = derivation_payload
        .cache_push
        .as_ref()
        .map(|cache_push| cache_push.to_cache_config(&local_cache_config))
        .unwrap_or(local_cache_config);

    let source_mirror_root = remote_runtime.source_mirror_root.as_path();
    let source_worktree_root = remote_runtime.source_worktree_root.as_path();
    let build_timeout = build_config.process_timeout();

    let cleanup_worktree = remote_runtime
        .cleanup_source_worktrees
        .then(|| {
            cleanup_candidate_worktree(
                &derivation_payload,
                source_mirror_root,
                source_worktree_root,
                job_id,
            )
        })
        .flatten();

    if derivation_payload.execution_strategy
        == RemoteBuildExecutionStrategy::SourceReEvaluateVerified
    {
        info!(
            "🔐 Verifying source build plan before build (timeout: {:?})",
            build_timeout
        );
        // Track the active sub-phase so timeouts are reported with the correct
        // failure phase (SourceFetch vs Evaluation).
        let pre_build_phase = Arc::new(AtomicU8::new(PRE_BUILD_SOURCE_FETCH));
        let verification = {
            let verification_future = verify_source_build_plan(
                &derivation_payload,
                source_mirror_root,
                source_worktree_root,
                job_id,
                &pre_build_phase,
                Some(&client),
            );
            wait_for_pre_build_verification(verification_future, build_timeout, &pre_build_phase, || {
                let client = client.clone();
                async move {
                    match client.get_job_status(job_id).await {
                        Ok(Some(status)) if status == "cancelling" => true,
                        Ok(_) => false,
                        Err(err) => {
                            warn!(
                                "⚠️ Failed to poll cancellation during verified source pre-build phase for job #{}: {}",
                                job_id, err
                            );
                            false
                        }
                    }
                }
            })
            .await
        };

        match verification {
            VerificationOutcome::Completed(Ok(verified_drv_path)) => {
                info!(
                    "✅ Verified source re-evaluation matched server drvPath: {}",
                    verified_drv_path
                );
                derivation_payload.derivation_path = Some(verified_drv_path);
            }
            VerificationOutcome::Completed(Err(failure)) => {
                error!(
                    "❌ Job #{} failed before build during {}: {}",
                    job_id, failure.phase, failure.message
                );
                if let Err(report_err) = client
                    .fail_job_with_phase(job_id, failure.phase, &failure.message)
                    .await
                {
                    error!(
                        "❌ Failed to report job #{} pre-build failure: {}",
                        job_id, report_err
                    );
                }
                if let Some(cleanup) = cleanup_worktree.as_ref() {
                    cleanup_source_worktree(cleanup).await;
                }
                return;
            }
            VerificationOutcome::Cancelled => {
                warn!(
                    "🛑 Job #{} cancelled during verified source pre-build phase",
                    job_id
                );
                if let Err(err) = client.finalize_cancelled_job(job_id).await {
                    error!("❌ Failed to finalize cancelled job #{}: {}", job_id, err);
                }
                if let Some(cleanup) = cleanup_worktree.as_ref() {
                    cleanup_source_worktree(cleanup).await;
                }
                return;
            }
        }
    }

    // Build a Derivation from the API payload — no database read required.
    let mut derivation =
        crystal_forge::derivations::Derivation::from_build_payload(&derivation_payload);

    // API-backed reporter: progress + cancel checks over HTTP (no DB pool).
    let reporter = ApiBuildReporter::new(client.clone(), job_id);

    info!("📦 Building derivation: {}", derivation.derivation_name);

    // Respect the configured build timeout for remote API builders. Nix itself
    // receives build_config.timeout; the wrapper gets a small cleanup buffer so
    // it can observe/report Nix's timeout rather than racing it.
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
            if let Err(report_err) = client
                .fail_job_with_phase(
                    job_id,
                    BuildFailurePhase::PathMaterialization,
                    &e.to_string(),
                )
                .await
            {
                error!(
                    "❌ Failed to report job #{} derivation import failure: {}",
                    job_id, report_err
                );
            }
            if let Some(cleanup) = cleanup_worktree.as_ref() {
                cleanup_source_worktree(cleanup).await;
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

            if !execution_mode.is_mock() {
                let _cache_reference = match required_cache_push_reference(
                    &cache_config,
                    &derivation.derivation_name,
                    &store_path,
                ) {
                    Ok(reference) => reference,
                    Err(e) => {
                        error!(
                            "❌ Cache push is not configured for job #{}: {:#}",
                            job_id, e
                        );
                        let _ = client
                            .append_logs(
                                job_id,
                                &format!("❌ Cache push is not configured: {:#}\n", e),
                            )
                            .await;
                        if let Err(report_error) = client
                            .fail_job(
                                job_id,
                                &format!("Cache push is required but not configured: {:#}", e),
                            )
                            .await
                        {
                            error!(
                                "❌ Failed to report cache-configuration failure for job #{}: {}",
                                job_id, report_error
                            );
                        }
                        return;
                    }
                };

                let _ = client
                    .append_logs(job_id, "📤 Pushing build output to cache...\n")
                    .await;

                match derivation
                    .push_to_cache_with_retry(&store_path, &cache_config, &build_config)
                    .await
                {
                    Ok(()) => {
                        let _ = client
                            .append_logs(job_id, "✅ Build output pushed to cache\n")
                            .await;
                    }
                    Err(e) => {
                        error!("❌ Cache push failed for job #{}: {:#}", job_id, e);
                        let _ = client
                            .append_logs(job_id, &format!("❌ Cache push failed: {:#}\n", e))
                            .await;
                        if let Err(report_error) = client
                            .fail_job(
                                job_id,
                                &format!("Cache push failed after successful build: {:#}", e),
                            )
                            .await
                        {
                            error!(
                                "❌ Failed to report cache-push failure for job #{}: {}",
                                job_id, report_error
                            );
                        }
                        return;
                    }
                }
            }

            // Report success to the server after the builder-side cache push.
            // The server records completion and may create a cache-push audit row,
            // but remote deployability depends on the builder push above.
            let cache_reference = if execution_mode.is_mock() {
                None
            } else {
                match required_cache_push_reference(
                    &cache_config,
                    &derivation.derivation_name,
                    &store_path,
                ) {
                    Ok(reference) => Some(reference),
                    Err(e) => {
                        error!(
                            "❌ Cache reference unavailable after push for job #{}: {:#}",
                            job_id, e
                        );
                        return;
                    }
                }
            };

            if let Err(e) = client
                .complete_job(job_id, &store_path, cache_reference.as_deref())
                .await
            {
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

    if let Some(cleanup) = cleanup_worktree.as_ref() {
        cleanup_source_worktree(cleanup).await;
    }
}

/// Check whether the full `.drv` requisite closure is locally valid.
///
/// Uses `nix-store --check-validity --recursive` so a partially imported
/// closure (e.g. from a mid-stream crash on a previous attempt) is not
/// mistaken for a usable one.  A partial import leaves the top-level `.drv`
/// file on disk but some of its requisites missing; a plain `Path::exists()`
/// check would incorrectly skip re-import and then fail later inside
/// `nix-store --realise`.
async fn drv_closure_available_locally(drv_path: &str) -> anyhow::Result<bool> {
    let output = tokio::process::Command::new("nix-store")
        .arg("--check-validity")
        .arg("--recursive")
        .arg(drv_path)
        .output()
        .await?;

    Ok(output.status.success())
}

/// Compute which of the manifest paths are NOT valid in the local Nix store.
///
/// Chunked to avoid one process per path. `nix-store --check-validity` exits
/// nonzero if ANY path in the batch is invalid, so failed chunks fall back to
/// per-path checks to find the exact missing set.
async fn missing_store_paths_batched(paths: &[String]) -> anyhow::Result<Vec<String>> {
    let mut missing = Vec::new();

    for chunk in paths.chunks(256) {
        let output = tokio::process::Command::new("nix-store")
            .arg("--check-validity")
            .args(chunk)
            .output()
            .await?;

        if output.status.success() {
            continue;
        }

        for path in chunk {
            let single = tokio::process::Command::new("nix-store")
                .arg("--check-validity")
                .arg(path)
                .output()
                .await?;

            if !single.status.success() {
                missing.push(path.clone());
            }
        }
    }

    Ok(missing)
}

/// Delta materialization: fetch the authorized manifest, compute the locally
/// missing subset, and request only those paths from the server.
async fn materialize_derivation_delta(
    client: &BuilderApiClient,
    job_id: uuid::Uuid,
    expected_drv_path: &str,
) -> std::result::Result<(), crystal_forge::builder::api_client::DeltaError> {
    use crystal_forge::builder::api_client::DeltaError;

    let manifest = client.get_derivation_manifest(job_id).await?;

    if manifest.drv_path != expected_drv_path {
        return Err(DeltaError::Fatal(anyhow::anyhow!(
            "server manifest drv path mismatch: expected {expected_drv_path}, got {}",
            manifest.drv_path
        )));
    }

    let missing = missing_store_paths_batched(&manifest.paths)
        .await
        .map_err(DeltaError::Fatal)?;

    if missing.is_empty() {
        info!(
            "✅ All {} manifest paths already valid locally; no transfer needed",
            manifest.paths.len()
        );
        return Ok(());
    }

    info!(
        "📥 Delta materialization: {} of {} manifest paths missing locally; requesting only those",
        missing.len(),
        manifest.paths.len()
    );

    client
        .stream_derivation_delta_archive_to_import(job_id, &missing)
        .await
}

async fn ensure_derivation_available(
    client: &BuilderApiClient,
    job_id: uuid::Uuid,
    drv_path: &str,
) -> anyhow::Result<()> {
    use crystal_forge::builder::api_client::DeltaError;

    if drv_closure_available_locally(drv_path).await? {
        return Ok(());
    }

    // Preferred: delta materialization — request only the manifest paths that
    // are missing locally. Falls back to the full closure archive ONLY when
    // the server does not support the delta endpoints (404/405). Security or
    // validation failures (403, drv mismatch, malformed responses) are hard
    // errors and are never masked by the fallback.
    match materialize_derivation_delta(client, job_id, drv_path).await {
        Ok(()) => {}
        Err(DeltaError::Unsupported(reason)) => {
            info!(
                "ℹ️  Server does not support delta transport ({}); using full closure archive",
                reason
            );
            client
                .stream_derivation_archive_to_import(job_id, drv_path)
                .await?;
        }
        Err(DeltaError::Fatal(err)) => {
            return Err(err.context("delta derivation materialization failed"));
        }
    }

    // Verify the full recursive closure is now valid — not just that the
    // top-level .drv file exists. A truncated or partial import must surface
    // here as a path_materialization failure, not later inside
    // nix-store --realise as a confusing build error.
    if !drv_closure_available_locally(drv_path).await? {
        anyhow::bail!(
            "nix-store --import completed but derivation closure is still incomplete: {drv_path}"
        );
    }
    info!(
        "✅ .drv closure fully materialized and valid for {}",
        drv_path
    );

    // Fire-and-forget: ask the server to publish the closure to the binary cache
    // in the background so future builds (or other builders) can pull it via
    // normal Nix substituters.  Failures here are non-fatal — the build will
    // proceed with the already-imported .drv and outputs will be pushed post-build
    // through the normal cache_push job path.
    let client_bg = client.clone();
    let job_id_bg = job_id;
    tokio::spawn(async move {
        match client_bg.publish_derivation_closure(job_id_bg).await {
            Ok(()) => info!(
                "✅ Background: server published .drv closure to cache for job {}",
                job_id_bg
            ),
            Err(e) => warn!(
                "⚠️  Background cache publish for job {} skipped or failed (non-fatal): {}",
                job_id_bg, e
            ),
        }
    });

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
    use super::{
        PRE_BUILD_SOURCE_FETCH, cleanup_candidate_worktree, drv_path_eval_attr,
        ensure_mirror_has_commit, mock_store_path, should_mock_build_fail, source_flake_ref,
        source_workspace_paths, source_workspace_paths_for_job, verify_drv_identity,
        wait_for_pre_build_verification,
    };
    use crystal_forge::models::builders::{
        BuildFailurePhase, SourceInputDeliveryMode, VerifiedSourceIdentity,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

    fn source(archive_url: Option<&str>) -> VerifiedSourceIdentity {
        VerifiedSourceIdentity {
            repo_url: "https://gitlab.com/example/private.git".to_string(),
            commit_hash: "0123456789abcdef".to_string(),
            flake_target: "nixosConfigurations.host.config.system.build.toplevel".to_string(),
            mirror_id: Some("repo-test".to_string()),
            mirror_path: None,
            worktree_path: None,
            lock_hash: Some("sha256-lock".to_string()),
            archive_url: archive_url.map(str::to_string),
            archive_sha256: Some("sha256-source".to_string()),
        }
    }

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

    #[test]
    fn source_flake_ref_prefers_server_archive_without_file_scheme() {
        let source = source(Some("file:///tmp/source-archive"));
        let flake_ref = source_flake_ref(
            &source,
            SourceInputDeliveryMode::ServerBundledArchive,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
        )
        .expect("server archive source should be accepted");
        assert_eq!(flake_ref, "/tmp/source-archive");
    }

    #[test]
    fn server_bundled_source_requires_archive_url() {
        let source = source(None);
        let failure = source_flake_ref(
            &source,
            SourceInputDeliveryMode::ServerBundledArchive,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
        )
        .expect_err("missing archive_url should fail before build");
        assert_eq!(failure.phase, BuildFailurePhase::SourceFetch);
    }

    #[test]
    fn public_input_mode_builds_pinned_git_ref() {
        let source = source(None);
        let flake_ref = source_flake_ref(
            &source,
            SourceInputDeliveryMode::BuilderFetchPublicInputs,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
        )
        .expect("public input mode should produce a pinned git ref");
        assert_eq!(
            flake_ref,
            "git+https://gitlab.com/example/private.git?rev=0123456789abcdef"
        );
    }

    #[test]
    fn drv_path_eval_attr_appends_drv_path_once() {
        assert_eq!(
            drv_path_eval_attr(
                "/tmp/source",
                "nixosConfigurations.host.config.system.build.toplevel"
            ),
            "/tmp/source#nixosConfigurations.host.config.system.build.toplevel.drvPath"
        );
        assert_eq!(
            drv_path_eval_attr(
                "/tmp/source",
                "nixosConfigurations.host.config.system.build.toplevel.drvPath"
            ),
            "/tmp/source#nixosConfigurations.host.config.system.build.toplevel.drvPath"
        );
    }

    #[test]
    fn drv_identity_mismatch_is_pre_build_failure() {
        let failure = verify_drv_identity(
            "/nix/store/server-nixos-system-host.drv",
            "/nix/store/builder-nixos-system-host.drv",
        )
        .expect_err("mismatch must fail before build");
        assert_eq!(failure.phase, BuildFailurePhase::DerivationMismatch);
        assert!(
            failure
                .message
                .contains("expected server-authorized drvPath")
        );
    }

    #[tokio::test]
    async fn cancelled_pre_build_drops_verification_future_before_finalization() {
        struct DropGuardFuture {
            dropped: Arc<AtomicBool>,
        }

        impl std::future::Future for DropGuardFuture {
            type Output = Result<String, super::PreBuildFailure>;

            fn poll(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                std::task::Poll::Pending
            }
        }

        impl Drop for DropGuardFuture {
            fn drop(&mut self) {
                self.dropped.store(true, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let pre_build_phase = AtomicU8::new(PRE_BUILD_SOURCE_FETCH);
        let outcome = wait_for_pre_build_verification(
            DropGuardFuture {
                dropped: Arc::clone(&dropped),
            },
            std::time::Duration::from_secs(60),
            &pre_build_phase,
            || async { true },
        )
        .await;

        assert!(matches!(outcome, super::VerificationOutcome::Cancelled));

        // This assertion models the production caller's next step: finalization
        // happens only after wait_for_pre_build_verification has returned, which
        // means the pinned verification future has already left scope and run
        // its Drop implementation (killing any child process via kill_on_drop).
        assert!(
            dropped.load(Ordering::SeqCst),
            "verification future must be dropped before cancellation finalization can run"
        );
    }

    #[test]
    fn local_git_worktree_paths_are_derived_from_mirror_id_and_commit() {
        let source = source(None);
        let (mirror, worktree) = source_workspace_paths(
            &source,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
        )
        .expect("paths should resolve");

        assert_eq!(mirror, std::path::PathBuf::from("/mirrors/repo-test.git"));
        assert_eq!(
            worktree,
            std::path::PathBuf::from("/worktrees/repo-test/0123456789abcdef")
        );
    }

    #[test]
    fn local_git_worktree_paths_can_be_scoped_to_job_id() {
        let source = source(None);
        let job_id = uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555")
            .expect("uuid should parse");
        let (mirror, worktree) = source_workspace_paths_for_job(
            &source,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
            Some(job_id),
        )
        .expect("paths should resolve");

        assert_eq!(mirror, std::path::PathBuf::from("/mirrors/repo-test.git"));
        assert_eq!(
            worktree,
            std::path::PathBuf::from(
                "/worktrees/repo-test/0123456789abcdef/11111111-2222-3333-4444-555555555555"
            )
        );
    }

    #[test]
    fn cleanup_candidate_is_limited_to_configured_worktree_root() {
        let job_id = uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555")
            .expect("uuid should parse");
        let payload = crystal_forge::models::builders::BuildJobDerivation {
            id: 1,
            derivation_name: "host".to_string(),
            derivation_type: "nixos".to_string(),
            derivation_path: None,
            store_path: None,
            execution_strategy: crystal_forge::models::builders::RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            source: Some(source(None)),
            source_input_delivery: SourceInputDeliveryMode::LocalGitWorktree,
            expected_drv_path: Some("/nix/store/server-host.drv".to_string()),
            evaluator: None,
            cache_push: None,
        };

        let cleanup = cleanup_candidate_worktree(
            &payload,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
            job_id,
        )
        .expect("worktree under configured root should be cleaned");

        assert_eq!(
            cleanup.mirror_path,
            std::path::PathBuf::from("/mirrors/repo-test.git")
        );
        assert_eq!(
            cleanup.worktree_path,
            std::path::PathBuf::from(
                "/worktrees/repo-test/0123456789abcdef/11111111-2222-3333-4444-555555555555"
            )
        );
    }

    #[tokio::test]
    async fn ensure_mirror_clones_and_fetches_authorized_commits() {
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir should be created");
        let source_repo = temp.path().join("source");
        let mirror_path = temp.path().join("mirror.git");

        git(temp.path(), &["init", source_repo.to_str().unwrap()]);
        git(
            &source_repo,
            &["config", "user.email", "builder@example.invalid"],
        );
        git(&source_repo, &["config", "user.name", "Builder Test"]);

        std::fs::write(source_repo.join("flake.nix"), "{ outputs = { self }: {}; }")
            .expect("fixture file should be written");
        git(&source_repo, &["add", "flake.nix"]);
        git(&source_repo, &["commit", "-m", "initial"]);
        let first_commit = git_stdout(&source_repo, &["rev-parse", "HEAD"]);

        ensure_mirror_has_commit(&mirror_path, source_repo.to_str().unwrap(), &first_commit)
            .await
            .expect("missing mirror should be cloned with the requested commit");
        assert!(mirror_path.exists());

        std::fs::write(source_repo.join("README.md"), "second commit")
            .expect("fixture file should be written");
        git(&source_repo, &["add", "README.md"]);
        git(&source_repo, &["commit", "-m", "second"]);
        let second_commit = git_stdout(&source_repo, &["rev-parse", "HEAD"]);

        ensure_mirror_has_commit(&mirror_path, source_repo.to_str().unwrap(), &second_commit)
            .await
            .expect("existing mirror should fetch the requested missing commit");

        git(
            temp.path(),
            &[
                "--git-dir",
                mirror_path.to_str().unwrap(),
                "cat-file",
                "-e",
                &format!("{second_commit}^{{commit}}"),
            ],
        );
    }

    #[tokio::test]
    async fn failed_mirror_clone_removes_temporary_directory() {
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir should be created");
        let mirror_path = temp.path().join("mirror.git");
        let missing_repo = temp.path().join("missing-source-repo");

        ensure_mirror_has_commit(
            &mirror_path,
            missing_repo.to_str().expect("test path should be utf-8"),
            "0123456789abcdef0123456789abcdef01234567",
        )
        .await
        .expect_err("missing repo should fail clone");

        let leftovers: Vec<_> = std::fs::read_dir(temp.path())
            .expect("tempdir should be readable")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("mirror.git.tmp-")
            })
            .collect();

        assert!(
            leftovers.is_empty(),
            "temporary mirror clone directories should be removed after clone failure: {leftovers:?}"
        );
    }

    fn git(cwd: &std::path::Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_stdout(cwd: &std::path::Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    // ── ServerBundledArchive delivery tests ─────────────────────────────────

    #[test]
    fn server_bundled_archive_with_url_strips_file_scheme() {
        // The archive_url returned by the server starts with the API path
        // (e.g. /api/v1/builders/.../source-archive), NOT a file:// path.
        // source_flake_ref strips "file://" when present so the path is
        // usable directly by nix as a flake ref.
        let mut src = source(Some("file:///tmp/my-source-archive"));
        src.archive_sha256 = None; // sha256 not checked in source_flake_ref
        let flake_ref = source_flake_ref(
            &src,
            SourceInputDeliveryMode::ServerBundledArchive,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
        )
        .expect("should succeed when archive_url is set");
        // file:// prefix is stripped so nix can open it as a path
        assert_eq!(flake_ref, "/tmp/my-source-archive");
    }

    #[test]
    fn server_bundled_archive_without_url_is_source_fetch_failure() {
        let src = source(None);
        let err = source_flake_ref(
            &src,
            SourceInputDeliveryMode::ServerBundledArchive,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
        )
        .expect_err("missing archive_url must produce SourceFetch failure");
        assert_eq!(err.phase, BuildFailurePhase::SourceFetch);
        assert!(err.message.contains("archive_url is missing"));
    }

    #[test]
    fn path_materialization_failure_phase_serializes_correctly() {
        use crystal_forge::models::builders::BuildFailurePhase;
        assert_eq!(
            BuildFailurePhase::PathMaterialization.to_string(),
            "path_materialization"
        );
    }

    #[test]
    fn server_bundled_archive_cleanup_is_job_scoped_with_mirror_dir() {
        // ServerBundledArchive cleanup must include the job-scoped mirror dir
        // so it is removed after the job completes/fails.
        let job_id = uuid::Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
            .expect("uuid should parse");
        let payload = crystal_forge::models::builders::BuildJobDerivation {
            id: 1,
            derivation_name: "host".to_string(),
            derivation_type: "nixos".to_string(),
            derivation_path: None,
            store_path: None,
            execution_strategy: crystal_forge::models::builders::RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            source: Some(source(Some("/api/v1/builders/x/jobs/y/source-archive"))),
            source_input_delivery: SourceInputDeliveryMode::ServerBundledArchive,
            expected_drv_path: Some("/nix/store/server-host.drv".to_string()),
            evaluator: None,
            cache_push: None,
        };

        let cleanup = cleanup_candidate_worktree(
            &payload,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
            job_id,
        )
        .expect("ServerBundledArchive job under configured root should be cleaned");

        // Job-scoped mirror path
        assert_eq!(
            cleanup.mirror_path,
            std::path::PathBuf::from(
                "/mirrors/server-bundled/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/repo-test.git"
            )
        );
        // Worktree is still job-scoped under the standard worktree root
        assert_eq!(
            cleanup.worktree_path,
            std::path::PathBuf::from(
                "/worktrees/repo-test/0123456789abcdef/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
            )
        );
        // job_mirror_dir is set and points at the job-scoped subdirectory
        let job_mirror_dir = cleanup
            .job_mirror_dir
            .expect("job_mirror_dir must be Some for ServerBundledArchive");
        assert_eq!(
            job_mirror_dir,
            std::path::PathBuf::from(
                "/mirrors/server-bundled/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
            )
        );
    }

    #[test]
    fn local_git_worktree_cleanup_has_no_job_mirror_dir() {
        // LocalGitWorktree shares the mirror across jobs so job_mirror_dir must be None.
        let job_id = uuid::Uuid::new_v4();
        let payload = crystal_forge::models::builders::BuildJobDerivation {
            id: 1,
            derivation_name: "host".to_string(),
            derivation_type: "nixos".to_string(),
            derivation_path: None,
            store_path: None,
            execution_strategy: crystal_forge::models::builders::RemoteBuildExecutionStrategy::SourceReEvaluateVerified,
            source: Some(source(None)),
            source_input_delivery: SourceInputDeliveryMode::LocalGitWorktree,
            expected_drv_path: None,
            evaluator: None,
            cache_push: None,
        };

        let cleanup = cleanup_candidate_worktree(
            &payload,
            std::path::Path::new("/mirrors"),
            std::path::Path::new("/worktrees"),
            job_id,
        )
        .expect("LocalGitWorktree job should be cleaned");

        assert!(
            cleanup.job_mirror_dir.is_none(),
            "LocalGitWorktree must not set job_mirror_dir"
        );
    }

    // ── drv_closure_available_locally tests ─────────────────────────────────

    #[tokio::test]
    async fn drv_closure_available_locally_returns_false_for_nonexistent_path() {
        // A path that does not exist in the Nix store must not be considered valid.
        let result = super::drv_closure_available_locally(
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-nonexistent.drv",
        )
        .await
        .expect("nix-store --check-validity should not error even for missing paths");
        assert!(
            !result,
            "nonexistent path must not be reported as locally valid"
        );
    }

    #[tokio::test]
    async fn drv_closure_available_locally_returns_false_for_non_nix_path() {
        // A path outside /nix/store must not be considered valid — guards against
        // accidental path confusion that could allow a partially-imported closure
        // to be skipped.
        let result = super::drv_closure_available_locally("/tmp/definitely-not-a-nix-path.drv")
            .await
            .expect("nix-store --check-validity should not error for arbitrary paths");
        assert!(
            !result,
            "non-nix path must not be reported as locally valid"
        );
    }

    // ── delta materialization: missing path computation ─────────────────────

    #[tokio::test]
    async fn missing_store_paths_batched_reports_all_nonexistent_paths() {
        // All fake paths are invalid, so the missing set must equal the input.
        let paths = vec![
            "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-fake-one.drv".to_string(),
            "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-fake-two".to_string(),
        ];
        let missing = super::missing_store_paths_batched(&paths)
            .await
            .expect("batched validity check should not error");
        assert_eq!(
            missing, paths,
            "all nonexistent paths must be reported missing"
        );
    }

    #[tokio::test]
    async fn missing_store_paths_batched_empty_input_is_empty_output() {
        let missing = super::missing_store_paths_batched(&[])
            .await
            .expect("empty input should not error");
        assert!(missing.is_empty());
    }
}
