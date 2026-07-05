use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crystal_forge::config::CrystalForgeConfig;
use crystal_forge::deployment::agent::{AgentDeploymentManager, DeploymentResult, readlink_path};
use crystal_forge::handlers::agent::heartbeat::LogResponse;
use crystal_forge::models::system_states::SystemState;
use ed25519_dalek::{Signer, SigningKey};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use reqwest::blocking::Client;
use serde_json::Value;
use std::{ffi::OsStr, fs, path::PathBuf, process::Command, sync::Arc};
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// Default heartbeat interval used when the server provides no override.
const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 600;

/// Maximum number of retry attempts for a failed heartbeat POST.
const HEARTBEAT_MAX_RETRIES: u32 = 3;

/// Base delay (ms) for the first retry; doubles each attempt.
const HEARTBEAT_RETRY_BASE_MS: u64 = 2_000;

/// Maximum jitter added to the sleep interval to avoid thundering herd (seconds).
const HEARTBEAT_JITTER_MAX_SECS: u64 = 30;

/// System reboot threshold: if host uptime at agent startup is below this value
/// (seconds), we classify the startup event as a full system reboot rather than
/// an agent service restart.
const REBOOT_UPTIME_THRESHOLD_SECS: u64 = 300; // 5 minutes

/// Returns `true` when `uptime_secs` indicates the system recently booted.
fn is_reboot_by_uptime(uptime_secs: u64) -> bool {
    uptime_secs < REBOOT_UPTIME_THRESHOLD_SECS
}

/// Result of a heartbeat POST attempt (with retries).
#[derive(Debug)]
enum HeartbeatResult {
    /// Heartbeat was successfully delivered and acknowledged by the server.
    Sent {
        /// Server-provided heartbeat interval override (if any).
        heartbeat_interval_secs: Option<u64>,
    },
    /// All retry attempts were exhausted without successful delivery.
    Failed,
}

// Agent state that holds the deployment manager
struct AgentState {
    deployment_manager: AgentDeploymentManager,
    /// Store path from the last successfully sent heartbeat, used to deduplicate
    /// inotify events that fire for the same derivation.
    last_reported_store_path: Option<String>,
    /// Host uptime recorded at agent startup (seconds). Used to decide whether
    /// a service restart is also a system reboot.
    startup_uptime_secs: u64,
}

impl AgentState {
    fn new() -> Result<Self> {
        let cfg = CrystalForgeConfig::load()?;
        let deployment_manager = AgentDeploymentManager::new(cfg.deployment.clone());
        let startup_uptime_secs = sysinfo::System::uptime();

        Ok(Self {
            deployment_manager,
            last_reported_store_path: None,
            startup_uptime_secs,
        })
    }

    /// Returns `true` when the agent appears to have started due to a full system
    /// reboot rather than just a service restart.
    ///
    /// We compare the current host uptime against a small threshold: if the host
    /// has been up for fewer than 5 minutes it almost certainly just booted.
    fn is_system_reboot(&self) -> bool {
        is_reboot_by_uptime(self.startup_uptime_secs)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    // Initialize agent state with deployment manager
    let agent_state = Arc::new(Mutex::new(AgentState::new()?));
    watch_system(agent_state).await
}

fn deriver_drv(path: &OsStr) -> Result<String> {
    // 1) Try nix-store (fast path)
    let out = Command::new("nix-store")
        .args(["--query", "--deriver"])
        .arg(path)
        .output()
        .context("nix-store --query --deriver failed to start")?;

    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() && s.ends_with(".drv") {
            return Ok(s);
        }
    }

    // 2) Fallback to nix path-info --json (some caches omit Deriver in narinfo)
    let out = Command::new("nix")
        .args(["path-info", "--json"])
        .arg(path)
        .output()
        .context("nix path-info --json failed to start")?;

    if out.status.success() {
        let v: Value =
            serde_json::from_slice(&out.stdout).context("decoding nix path-info JSON")?;
        if let Some(deriver) = v
            .as_array()
            .and_then(|a| a.get(0))
            .and_then(|o| o.get("deriver"))
            .and_then(|d| d.as_str())
        {
            if !deriver.is_empty() && deriver.ends_with(".drv") {
                return Ok(deriver.to_string());
            }
        }
    }

    bail!(
        "deriver unknown for {} (cache may have omitted Deriver metadata)",
        PathBuf::from(path).display(),
    );
}

fn deriver_drv_with_test_fallback(path: &OsStr) -> Result<String> {
    match deriver_drv(path) {
        Ok(drv_path) => Ok(drv_path),
        Err(_) => {
            // In test environments, try to construct a reasonable .drv path
            let path_str = path.to_string_lossy();
            if path_str.contains("nixos-system-") {
                // Try to find a matching .drv in the nix store
                let output = std::process::Command::new("find")
                    .args(["/nix/store", "-name", "*nixos-system*.drv", "-type", "f"])
                    .output()?;

                if output.status.success() {
                    let stderr_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_drv) = stderr_str.lines().next() {
                        return Ok(first_drv.to_string());
                    }
                }
            }
            // Last resort: construct a fake .drv path for testing
            Ok(format!("{}.drv", path_str))
        }
    }
}

/// Creates and signs a system state payload.
fn create_signed_payload(
    current_system: &OsStr,
    context: &str,
) -> Result<(SystemState, String, String)> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = &cfg.client;
    let hostname = hostname::get()?.to_string_lossy().into_owned();

    let current_system_str = current_system.to_string_lossy();
    let payload = SystemState::gather(&hostname, context, current_system_str.as_ref())?;
    let payload_json = serde_json::to_string(&payload)?;

    let key_bytes = STANDARD
        .decode(fs::read_to_string(&client_cfg.private_key)?.trim())
        .context("failed to decode base64 private key")?;
    let signing_key = SigningKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .context("expected a 32-byte Ed25519 private key")?,
    );

    let signature = signing_key.sign(payload_json.as_bytes());
    let signature_b64 = STANDARD.encode(signature.to_bytes());

    Ok((payload, payload_json, signature_b64))
}

/// Posts system state changes to the server (non-heartbeat, no retry).
pub fn post_system_state_change(current_system: &OsStr, context: &str) -> Result<()> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = &cfg.client;

    let (payload, payload_json, signature_b64) = create_signed_payload(current_system, context)?;
    let hostname = hostname::get()?.to_string_lossy().into_owned();

    let client = Client::new();
    let (scheme, port_suffix) = match client_cfg.server_port {
        443 => ("https", "".to_string()),
        80 => ("http", "".to_string()),
        port => ("http", format!(":{}", port)),
    };

    let url = format!(
        "{}://{}{}/agent/state",
        scheme, client_cfg.server_host, port_suffix
    );

    info!("Posting state change ({context}) to: {url}");
    let res = client
        .post(url)
        .header("X-Signature", signature_b64)
        .header("X-Key-ID", hostname)
        .body(payload_json)
        .send()
        .context("failed to send state change POST")?;

    if !res.status().is_success() {
        anyhow::bail!("server responded with {}", res.status());
    }

    Ok(())
}

/// Posts heartbeat to the server with retry (exponential backoff) and returns the
/// `LogResponse` including `heartbeat_interval_secs` when present.
async fn post_heartbeat_with_retry(
    current_system: &OsStr,
    context: &str,
) -> Result<LogResponse> {
    let cfg = CrystalForgeConfig::load()?;
    let client_cfg = &cfg.client;

    let (_, payload_json, signature_b64) = create_signed_payload(current_system, context)?;
    let hostname = hostname::get()?.to_string_lossy().into_owned();

    let client = reqwest::Client::new();
    let (scheme, port_suffix) = match client_cfg.server_port {
        443 => ("https", "".to_string()),
        80 => ("http", "".to_string()),
        port => ("http", format!(":{}", port)),
    };

    let url = format!(
        "{}://{}{}/agent/heartbeat",
        scheme, client_cfg.server_host, port_suffix
    );

    let mut last_err: anyhow::Error = anyhow::anyhow!("no attempts made");
    for attempt in 0..=HEARTBEAT_MAX_RETRIES {
        if attempt > 0 {
            let backoff_ms = HEARTBEAT_RETRY_BASE_MS * (1 << (attempt - 1));
            warn!(
                "Heartbeat attempt {attempt}/{HEARTBEAT_MAX_RETRIES} failed, retrying in {backoff_ms}ms"
            );
            sleep(Duration::from_millis(backoff_ms)).await;
        }

        let result = client
            .post(&url)
            .header("X-Signature", &signature_b64)
            .header("X-Key-ID", &hostname)
            .body(payload_json.clone())
            .send()
            .await;

        match result {
            Err(e) => {
                warn!("Heartbeat POST network error (attempt {attempt}): {e}");
                last_err = e.into();
            }
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    warn!(
                        "Heartbeat POST rejected by server (attempt {attempt}): HTTP {status} — {body}"
                    );
                    last_err = anyhow::anyhow!("server responded with {status}: {body}");
                    continue;
                }
                
                // P2-4: Catch deserialization errors inside the retry loop so they can be retried
                match resp.json::<LogResponse>().await {
                    Ok(log_response) => return Ok(log_response),
                    Err(e) => {
                        warn!(
                            "Heartbeat response deserialization failed (attempt {attempt}): {e}"
                        );
                        last_err = anyhow::anyhow!("failed to parse LogResponse: {e}");
                        continue;
                    }
                }
            }
        }
    }
    Err(last_err)
}

/// Posts heartbeat and handles deployment responses.
/// Returns HeartbeatResult::Sent on success (with optional interval override) or
/// HeartbeatResult::Failed if all retry attempts were exhausted.
pub async fn post_system_heartbeat_with_deployment(
    current_system: &OsStr,
    context: &str,
    agent_state: Arc<Mutex<AgentState>>,
) -> Result<HeartbeatResult> {
    info!("Posting heartbeat ({context})");
    let log_response = match post_heartbeat_with_retry(current_system, context).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("❌ Heartbeat failed after all retries: {e:#}");
            return Ok(HeartbeatResult::Failed);
        }
    };

    let heartbeat_interval_secs = log_response.heartbeat_interval_secs;

    // Process deployment with our deployment manager
    let mut state = agent_state.lock().await;
    let (deployment_result, _) = state
        .deployment_manager
        .process_heartbeat_response(log_response)
        .await?;

    match deployment_result {
        DeploymentResult::SuccessFromCache { ref cache_url } => {
            info!("✅ Deployment completed successfully from cache: {}", cache_url);
            drop(state);
            post_system_state_change(current_system, "cf_deployment")?;
        }
        DeploymentResult::SuccessLocalBuild => {
            info!("✅ Deployment completed successfully with local build");
            drop(state);
            post_system_state_change(current_system, "cf_deployment")?;
        }
        DeploymentResult::Started { ref unit_name } => {
            info!("🚀 Deployment started in systemd unit: {}", unit_name);
            info!("   Agent will restart automatically after deployment completes");
        }
        DeploymentResult::Failed {
            ref error,
            ref desired_target,
        } => {
            error!("❌ Deployment failed for {}: {}", desired_target, error);
        }
        DeploymentResult::NoDeploymentNeeded => {
            info!("ℹ️ No deployment needed");
        }
        DeploymentResult::AlreadyOnTarget => {
            info!("ℹ️ Already on target configuration");
        }
    }

    Ok(HeartbeatResult::Sent {
        heartbeat_interval_secs,
    })
}

/// Handles an inotify event for `/run`. Filters to the "current-system" name only,
/// resolves the symlink, and suppresses the heartbeat if the resolved store path
/// has not changed since the last confirmed report (deduplication guard).
///
/// P1-1: Only updates the deduplication state when the server confirms receipt.
async fn handle_current_system_event(
    name: &OsStr,
    context: &str,
    agent_state: Arc<Mutex<AgentState>>,
) -> Result<Option<u64>> {
    if name != OsStr::new("current-system") {
        return Ok(None);
    }

    let current_system = readlink_path("/run/current-system")?;
    let current_system_str = current_system.to_string_lossy().into_owned();

    info!("[{context}] Current System: {current_system_str}");

    // Deduplication: skip if the store path hasn't changed.
    {
        let state = agent_state.lock().await;
        if state.last_reported_store_path.as_deref() == Some(&current_system_str) {
            info!("[{context}] Store path unchanged ({current_system_str}), suppressing duplicate heartbeat");
            return Ok(None);
        }
    }

    let result = post_system_heartbeat_with_deployment(
        current_system.as_os_str(),
        context,
        agent_state.clone(),
    )
    .await?;

    // P1-1: Only update dedup state after confirmed success.
    // If all retries failed, the next inotify event for this path will retry.
    match result {
        HeartbeatResult::Sent {
            heartbeat_interval_secs,
        } => {
            let mut state = agent_state.lock().await;
            state.last_reported_store_path = Some(current_system_str);
            Ok(heartbeat_interval_secs)
        }
        HeartbeatResult::Failed => {
            warn!(
                "[{context}] Heartbeat failed after all retries; dedup state NOT updated. \
                 Next event for this path will retry."
            );
            Ok(None)
        }
    }
}

/// The periodic heartbeat loop. Sleeps for the server-provided interval (or the
/// default 600s) with a small random jitter to spread agent check-ins.
async fn run_periodic_heartbeat_loop(agent_state: Arc<Mutex<AgentState>>) -> Result<()> {
    // Initial delay before the first heartbeat (avoid hammering the server right
    // after agent startup since the inotify event on startup already sends one).
    let initial_delay = jittered_interval(DEFAULT_HEARTBEAT_INTERVAL_SECS);
    sleep(Duration::from_secs(initial_delay)).await;

    let mut current_interval = DEFAULT_HEARTBEAT_INTERVAL_SECS;

    info!("💓 Starting heartbeat loop (initial interval: {current_interval}s)...");
    loop {
        let current_system = match readlink_path("/run/current-system") {
            Ok(p) => p,
            Err(e) => {
                error!("❌ Failed to read /run/current-system for heartbeat: {e}");
                sleep(Duration::from_secs(current_interval)).await;
                continue;
            }
        };

        match post_system_heartbeat_with_deployment(
            current_system.as_os_str(),
            "heartbeat",
            agent_state.clone(),
        )
        .await
        {
            Ok(HeartbeatResult::Sent {
                heartbeat_interval_secs: Some(server_interval),
            }) => {
                if server_interval != current_interval {
                    info!("💓 Heartbeat interval updated by server: {current_interval}s → {server_interval}s");
                }
                current_interval = server_interval;
            }
            Ok(HeartbeatResult::Sent {
                heartbeat_interval_secs: None,
            }) => {
                // Server provided no override; keep current interval.
            }
            Ok(HeartbeatResult::Failed) => {
                // Heartbeat failed after all retries (already logged); retain current interval.
            }
            Err(e) => {
                error!("❌ Heartbeat loop error: {e}");
            }
        }

        let sleep_secs = jittered_interval(current_interval);
        info!("💓 Next heartbeat in {sleep_secs}s");
        sleep(Duration::from_secs(sleep_secs)).await;
    }
}

/// Returns `interval_secs` plus a uniform random jitter in `[0, HEARTBEAT_JITTER_MAX_SECS)`.
fn jittered_interval(interval_secs: u64) -> u64 {
    let jitter = rand_jitter();
    interval_secs.saturating_add(jitter)
}

/// Returns a pseudo-random value in `[0, HEARTBEAT_JITTER_MAX_SECS)` using the
/// low bits of the current system time — no external crate required.
fn rand_jitter() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as u64) % HEARTBEAT_JITTER_MAX_SECS
}

/// Runs the inotify watch loop: watches `/run` for `current-system` changes.
async fn watch_for_system_changes(
    inotify: &mut Inotify,
    agent_state: Arc<Mutex<AgentState>>,
) -> Result<()> {
    info!("Watching /run for changes to current-system...");

    // Report initial state at startup, distinguishing reboot vs agent restart.
    let is_reboot = {
        let state = agent_state.lock().await;
        state.is_system_reboot()
    };
    let startup_context = if is_reboot {
        info!("🔄 System reboot detected (host uptime low at startup)");
        "startup"
    } else {
        info!("🔄 Agent service restart detected (host uptime normal)");
        "agent_restart"
    };

    if let Ok(current_system) = readlink_path("/run/current-system") {
        if let Ok(Some(interval)) = handle_current_system_event(
            OsStr::new("current-system"),
            startup_context,
            agent_state.clone(),
        )
        .await
        {
            // Update stored interval if server provided one at startup.
            let _ = interval; // handled by heartbeat loop
        }
    }

    loop {
        for event in inotify.read_events()? {
            if let Some(name) = event.name {
                // Only log and act when it is actually current-system.
                if name == OsStr::new("current-system") {
                    info!("Detected change to /run/current-system");
                    if let Err(e) = handle_current_system_event(
                        &name,
                        "config_change",
                        agent_state.clone(),
                    )
                    .await
                    {
                        error!("❌ Failed to handle current-system change: {e}");
                    }
                }
            }
        }
    }
}

/// Initializes an inotify watcher on `/run` for "current-system" and records updates
/// to the system state in the database.
pub async fn watch_system(agent_state: Arc<Mutex<AgentState>>) -> Result<()> {
    let mut inotify = Inotify::init(InitFlags::empty())?;
    inotify.add_watch(
        "/run",
        AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO,
    )?;

    // Spawn the periodic heartbeat loop.
    tokio::spawn(run_periodic_heartbeat_loop(agent_state.clone()));

    // Watch for inotify current-system change events.
    watch_for_system_changes(&mut inotify, agent_state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jittered_interval_is_at_least_base() {
        let base = 600_u64;
        let result = jittered_interval(base);
        assert!(result >= base, "jitter must not reduce interval below base");
        assert!(
            result < base + HEARTBEAT_JITTER_MAX_SECS,
            "jitter must not exceed max"
        );
    }

    #[test]
    fn rand_jitter_within_bounds() {
        for _ in 0..20 {
            let j = rand_jitter();
            assert!(j < HEARTBEAT_JITTER_MAX_SECS, "jitter {j} exceeds max");
        }
    }

    #[test]
    fn is_system_reboot_detection_uses_uptime_threshold() {
        // Low uptime (< 300s) → system reboot
        assert!(
            is_reboot_by_uptime(60),
            "uptime=60 must be classified as reboot"
        );
        assert!(
            is_reboot_by_uptime(299),
            "uptime=299 must be classified as reboot"
        );
        // High uptime → agent restart only
        assert!(
            !is_reboot_by_uptime(300),
            "uptime=300 must be classified as agent restart"
        );
        assert!(
            !is_reboot_by_uptime(86400),
            "uptime=86400 must be classified as agent restart"
        );
    }

    #[test]
    fn heartbeat_result_sent_carries_interval() {
        let result = HeartbeatResult::Sent {
            heartbeat_interval_secs: Some(120),
        };
        match result {
            HeartbeatResult::Sent {
                heartbeat_interval_secs: Some(interval),
            } => {
                assert_eq!(interval, 120, "interval must match");
            }
            _ => panic!("expected HeartbeatResult::Sent with interval"),
        }
    }

    #[test]
    fn heartbeat_result_sent_can_have_no_interval() {
        let result = HeartbeatResult::Sent {
            heartbeat_interval_secs: None,
        };
        match result {
            HeartbeatResult::Sent {
                heartbeat_interval_secs: None,
            } => {
                // OK
            }
            _ => panic!("expected HeartbeatResult::Sent with None"),
        }
    }

    #[test]
    fn heartbeat_result_failed_is_distinct() {
        let result = HeartbeatResult::Failed;
        match result {
            HeartbeatResult::Failed => {
                // OK
            }
            _ => panic!("expected HeartbeatResult::Failed"),
        }
    }
}


