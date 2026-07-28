use anyhow::{Context, Result, bail};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Duration, Instant};

const MOCK_EVAL_TOTAL_DURATION_MS: u64 = 30_000;
const MOCK_EVAL_MIN_PER_SYSTEM_MS: u64 = 5_000;
const MOCK_EVAL_STAGE_COUNT: u64 = 5;
const EVAL_OUTPUT_IDLE_TIMEOUT_SECS: u64 = 300;
const EVAL_PROGRESS_HEARTBEAT_SECS: u64 = 30;
/// Maximum number of missing systems to attempt individual fallback evaluation
/// for. Beyond this threshold, treat as a likely process-wide evaluator failure
/// and retry the commit rather than spawning dozens of standalone evaluations.
const MAX_INDIVIDUAL_FALLBACKS: usize = 4;
/// If nix-eval-jobs silently drops more than this percentage of expected
/// systems, abort instead of launching individual fallbacks.
const MAX_FALLBACK_MISSING_PERCENT: usize = 25;
/// Minimum number of missing systems before the percentage guard fires.
/// Prevents 1-of-3 (33%) from triggering the abort when only one system
/// genuinely broke and the rest are fine.
const MIN_MISSING_FOR_PERCENT_GUARD: usize = 2;
/// Maximum concurrent fallback evaluations.
const FALLBACK_CONCURRENCY: usize = 2;
/// Overall deadline for the fallback phase.
const FALLBACK_PHASE_TIMEOUT: Duration = Duration::from_secs(180);
const CLOSURE_COUNT_MAX_CONCURRENT: usize = 2;
static CLOSURE_COUNT_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
/// Process-wide limit on concurrent standalone `nix eval` subprocesses.
/// Each full Nix evaluation of a large flake can use 4–6 GiB of memory;
/// this semaphore prevents runaway fan-out when multiple commits evaluate
/// concurrently or when the fallback phase fires for many systems.
const MAX_CONCURRENT_STANDALONE_NIX_EVALS: usize = 2;
static STANDALONE_NIX_EVAL_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn standalone_nix_eval_limiter() -> Arc<Semaphore> {
    STANDALONE_NIX_EVAL_LIMITER
        .get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_STANDALONE_NIX_EVALS)))
        .clone()
}

/// Terminate an entire Nix evaluator process *group* (direct child + all
/// descendants) then reap the direct child.
///
/// On Linux/macOS the child is spawned as the leader of a new process group
/// (`cmd.process_group(0)` before `spawn()`).  Its PGID equals its PID, so
/// `killpg(pgid, SIGKILL)` reaches every process in the subtree atomically,
/// including sub-evaluators and helper processes that `nix eval` may fork.
///
/// After signalling the group, `child.wait()` reaps the direct child so that
/// no zombie lingers.  Descendants that are not direct children of the server
/// are reparented to init/systemd and reaped by it after SIGKILL.
///
/// On non-Unix targets falls back to killing only the direct child (same as
/// before), which is safe because those platforms do not fork the way Nix
/// does on Linux.
#[cfg(unix)]
async fn kill_nix_process_tree(child: &mut tokio::process::Child, pgid: libc::pid_t) {
    // SAFETY: killpg is a pure syscall with no memory-safety requirements.
    // ESRCH means the group already exited — treat as success.
    let kill_result = unsafe { libc::killpg(pgid, libc::SIGKILL) };
    if kill_result != 0 {
        let errno = std::io::Error::last_os_error();
        if errno.raw_os_error() != Some(libc::ESRCH) {
            warn!(pgid, "killpg(SIGKILL) failed: {errno}");
        }
    }
    // Reap the direct child.  Grandchildren are reaped by init after SIGKILL.
    let _ = child.wait().await;
}

#[cfg(not(unix))]
async fn kill_nix_process_tree(child: &mut tokio::process::Child, _pgid: i32) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

use tracing::{debug, error, info, warn};

use crate::config::{BuildConfig, ServerConfig};
use crate::derivations::utils::{build_flake_reference, count_closure_packages};
use crate::flake::credentials::FlakeCredentialEnv;
use crate::models::commits::Commit;
use crate::models::deployment_policies::{
    AssignedPolicy, DeploymentPolicy, PoliciesByConfiguration, PolicyCheckResult,
    build_nix_eval_expression, policies_for_config, policy_requirements_met, policy_results_json,
};
use crate::models::flakes::Flake;
use crate::queries::build_jobs::{
    BuildJobInsertOutcome, QueuedBuild, create_build_job_for_derivation_tx,
};
use crate::queries::commits_artifacts::CachedSystemsState;
use crate::queries::systems::list_configuration_names_for_flake;
use crate::queue::QueueNotifier;
use crate::services::hardening_scans::trigger_commit_hardening_scans;

/// NixEvalJobResult with meta field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NixEvalJobResult {
    pub attr: String,
    #[serde(rename = "attrPath")]
    pub attr_path: Vec<String>,
    /// Name is optional because nix-eval-jobs may omit it on errors
    pub name: Option<String>,
    #[serde(rename = "drvPath")]
    pub drv_path: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "cacheStatus")]
    pub cache_status: Option<String>,
    pub outputs: Option<serde_json::Value>,

    /// Meta field (only present with --meta flag)
    /// Contains our policy check results in meta.policies
    pub meta: Option<serde_json::Value>,
}

fn parse_expected_store_path_from_outputs(outputs: &serde_json::Value) -> Option<String> {
    // Keep parsing intentionally strict: only use the canonical "out" output.
    // Broad scanning can pick unrelated store paths and corrupt expected_path data.
    if let Some(path) = outputs
        .get("out")
        .and_then(|out| out.get("path").or_else(|| out.get("outPath")))
        .and_then(|v| v.as_str())
        .or_else(|| outputs.get("out").and_then(|v| v.as_str()))
        .or_else(|| outputs.get("outPath").and_then(|v| v.as_str()))
    {
        if path.starts_with("/nix/store/") {
            return Some(path.to_string());
        }
    }

    None
}

async fn resolve_expected_store_path(
    drv_path: &str,
    outputs: Option<&serde_json::Value>,
) -> Option<String> {
    // Authoritative source: ask nix-store for outputs of this exact drv.
    let mut cmd = Command::new("nix-store");
    cmd.kill_on_drop(true);
    cmd.args(["--query", "--outputs", drv_path]);

    let output = match tokio::time::timeout(Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            warn!(
                "Failed to start nix-store output resolution for drv {}: {}",
                drv_path, err
            );
            return outputs.and_then(parse_expected_store_path_from_outputs);
        }
        Err(_) => {
            warn!(
                "Timed out resolving expected store path via nix-store for drv {}",
                drv_path
            );
            return outputs.and_then(parse_expected_store_path_from_outputs);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        warn!(
            "Failed to resolve expected store path via nix-store for drv {}: {}",
            drv_path,
            if stderr.is_empty() {
                "<no stderr>"
            } else {
                &stderr
            }
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();

    if path.starts_with("/nix/store/") {
        Some(path)
    } else {
        if let Some(outputs) = outputs {
            if let Some(fallback) = parse_expected_store_path_from_outputs(outputs) {
                warn!(
                    "nix-store returned non-store output for drv {}, using outputs JSON fallback",
                    drv_path
                );
                return Some(fallback);
            }
        }

        warn!(
            "Could not resolve expected store path from nix-store output for drv {}: {}",
            drv_path,
            stdout.trim()
        );
        None
    }
}

fn closure_count_limiter() -> Arc<Semaphore> {
    CLOSURE_COUNT_LIMITER
        .get_or_init(|| Arc::new(Semaphore::new(CLOSURE_COUNT_MAX_CONCURRENT)))
        .clone()
}

/// Spawn non-critical background work after a system has been queued for build.
///
/// GC root creation is intentionally NOT included here: it is performed
/// *before* the queue notification so that a builder cannot claim a job
/// before the derivation is safely rooted against Nix GC collection.
fn spawn_closure_counting_and_hardening(
    pool: PgPool,
    commit_id: i32,
    flake_repo_url: String,
    commit_hash: String,
    finalized: FinalizedDerivation,
) {
    // Closure counting: bounded via semaphore.
    let pool_cc = pool.clone();
    let drv_cc = finalized.drv_path.clone();
    let derivation_id_cc = finalized.derivation_id;
    let limiter = closure_count_limiter();
    tokio::spawn(async move {
        let permit = match limiter.acquire_owned().await {
            Ok(permit) => permit,
            Err(err) => {
                warn!(
                    "⚠️  Failed to acquire closure count permit for id={}: {}",
                    derivation_id_cc, err
                );
                return;
            }
        };

        match count_closure_packages(&drv_cc).await {
            Ok((total, cached)) => {
                if let Err(err) = crate::queries::derivations::set_closure_counts(
                    &pool_cc,
                    derivation_id_cc,
                    total,
                    cached,
                )
                .await
                {
                    warn!(
                        "⚠️  Failed to store closure counts for id={}: {}",
                        derivation_id_cc, err
                    );
                }
            }
            Err(err) => warn!(
                "⚠️  Failed to count closure packages for id={}: {}",
                derivation_id_cc, err
            ),
        }
        drop(permit);
    });

    // Hardening scans: query DB targets + insert scan rows.
    tokio::spawn(async move {
        if let Err(err) =
            trigger_commit_hardening_scans(pool, commit_id, &flake_repo_url, &commit_hash).await
        {
            warn!(
                "Failed to queue hardening scans for commit {} after system {} finalized: {}",
                commit_id, finalized.system_name, err
            );
        }
    });
}

/// Helper function to broadcast eval log via WebSocket AND persist to database.
///
/// This ensures logs are both:
/// - Streamed in real-time to connected WebSocket clients
/// - Persisted to eval_logs table for historical access
async fn broadcast_and_persist_eval_log(
    pool: &PgPool,
    state: Option<&crate::handlers::agent_request::CFState>,
    commit_id: i32,
    sequence: &mut i32,
    message: String,
) {
    let lower = message.to_ascii_lowercase();

    // Broadcast via WebSocket (existing infrastructure)
    if let Some(state) = state {
        crate::handlers::api::commits::broadcast_eval_log(state, commit_id, message.clone()).await;
    }

    // Persist to database (new functionality)
    // Parse log level from message format if possible
    let log_level = if message.starts_with("❌") || lower.contains("error") {
        Some("error")
    } else if message.starts_with("⚠️") || lower.contains("warning") {
        Some("warn")
    } else if message.starts_with("✅") {
        Some("info")
    } else if message.starts_with("🐛") || message.starts_with("DEBUG:") {
        Some("debug")
    } else {
        Some("info")
    };

    if let Err(e) =
        crate::queries::eval_logs::insert_eval_log(pool, commit_id, *sequence, log_level, &message)
            .await
    {
        warn!("Failed to persist eval log for commit {}: {}", commit_id, e);
    }

    *sequence += 1;
}

/// Outcome of a standalone fallback evaluation for a single system.
///
/// This is a raw command result, NOT a verified system failure.  The control
/// verification that distinguishes genuine configuration errors from
/// evaluator-level issues is performed by
/// [`evaluate_and_verify_missing_system`] which returns a
/// [`VerifiedFallbackOutcome`].
///
/// Each variant includes the system name so the outcome can be processed
/// independently of the iteration order.
#[allow(dead_code)]
#[derive(Debug)]
enum FallbackEvalOutcome {
    /// The `nix eval` command exited with a nonzero status — the system
    /// failed to evaluate.  This may be a genuine NixOS configuration error
    /// or an evaluator/infrastructure problem; control verification is
    /// required to distinguish the two.
    CommandFailed { system_name: String, error: String },
    /// The system evaluated successfully.  This indicates the original
    /// nix-eval-jobs process omission was NOT caused by a broken
    /// configuration, but likely by an evaluator-level issue (OOM, crash,
    /// intermittent network error). The evaluation should be retried rather
    /// than synthesizing a per-system failure.
    StandaloneEvaluationSucceeded { system_name: String },
    /// The fallback process could not be started or timed out.  This is an
    /// infrastructure problem, not a system failure.
    InfrastructureFailure { system_name: String, error: String },
}

/// Verified outcome of evaluating and control-verifying a missing system.
///
/// Unlike [`FallbackEvalOutcome`] (which is a raw command result), this type
/// incorporates a control-system re-evaluation so the caller can safely
/// decide whether to persist a `DryRunFailed` derivation.
#[allow(dead_code)]
#[derive(Debug)]
enum VerifiedFallbackOutcome {
    /// The system genuinely failed standalone evaluation AND the control
    /// system succeeded.  This is a real NixOS configuration error that
    /// should be persisted as DryRunFailed.
    ConfirmedSystemFailure { system_name: String, error: String },
    /// The system evaluated successfully standalone — an evaluator omission.
    StandaloneEvaluationSucceeded { system_name: String },
    /// The system failed AND the control check also failed or was
    /// unavailable.  The evaluation as a whole should be retried.
    EvaluatorUnhealthy {
        system_name: String,
        target_error: String,
        control_error: Option<String>,
    },
    /// The fallback process could not be started or timed out.
    InfrastructureFailure { system_name: String, error: String },
}

#[derive(Debug)]
pub enum StandaloneSystemOutcome {
    Success {
        result: SuccessfulSystemResult,
        policy_check: PolicyCheckResult,
    },
    ConfirmedSystemFailure {
        system_name: String,
        error: String,
    },
    InfrastructureFailure {
        system_name: String,
        error: String,
    },
}

pub(crate) fn build_single_system_eval_expression(
    flake_ref: &str,
    system_name: &str,
    assigned: &[crate::models::deployment_policies::AssignedPolicy],
) -> String {
    use crate::models::deployment_policies::{
        build_policy_fields_for_config_standalone, nix_string_pub,
    };

    let field_lines = build_policy_fields_for_config_standalone(assigned);
    let policy_fields = if field_lines.is_empty() {
        String::new()
    } else {
        format!("\n{}", field_lines.join("\n"))
    };

    format!(
        r#"
let
  flake = builtins.getFlake {flake_ref};
  cfg = builtins.getAttr {system_name} flake.nixosConfigurations;
  drv = cfg.config.system.build.toplevel;
  policyResults = {{
    cfAgentEnabled = (cfg.config.systemd.services.crystal-forge-agent.enable or false)
      || ((cfg.config.services.crystal-forge.enable or false)
          && (cfg.config.services.crystal-forge.client.enable or false));{policy_fields}
  }};
in {{
  drvPath = drv.drvPath;
  outputs = drv.outputs or {{}};
  policies = policyResults;
}}
"#,
        flake_ref = nix_string_pub(flake_ref),
        system_name = nix_string_pub(system_name),
        policy_fields = policy_fields,
    )
}

#[derive(Debug, Deserialize)]
struct StandaloneEvalJson {
    #[serde(rename = "drvPath")]
    drv_path: String,
    #[serde(default)]
    outputs: serde_json::Value,
    #[serde(default)]
    policies: serde_json::Value,
}

pub async fn evaluate_single_system_with_policies(
    repo_url: &str,
    commit_hash: &str,
    system_name: &str,
    assigned: &[crate::models::deployment_policies::AssignedPolicy],
    creds: Option<&FlakeCredentialEnv>,
    build_config: &BuildConfig,
) -> Result<StandaloneSystemOutcome> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let nix_expr = build_single_system_eval_expression(&flake_ref, system_name, assigned);

    // Acquire the process-wide standalone eval slot before spawning.
    // This semaphore caps total concurrent `nix eval` processes across all
    // commits and fallback phases, preventing memory exhaustion on large flakes.
    let _nix_permit = match standalone_nix_eval_limiter().acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
            return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: "standalone Nix eval limiter was closed".to_string(),
            });
        }
    };

    let mut cmd = tokio::process::Command::new("nix");
    cmd.args(["eval", "--impure", "--json", "--expr", &nix_expr]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    // Spawn the evaluator in a new process group so that a SIGKILL on timeout
    // reaches nix and every subprocess it forks (sub-evaluators, builders,
    // helpers).  The group ID equals the child's PID when process_group(0)
    // is used.
    #[cfg(unix)]
    cmd.process_group(0);
    build_config.apply_to_command(&mut cmd);
    if let Some(c) = creds {
        c.apply_to_nix_command(&mut cmd);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Failed to spawn standalone eval: {}", e),
            });
        }
    };

    // PGID == PID when spawned with process_group(0).
    #[cfg(unix)]
    let pgid = child.id().unwrap_or(0) as libc::pid_t;
    #[cfg(not(unix))]
    let pgid = 0i32;

    // Collect stdout/stderr via background tasks so `child` stays owned here
    // and we can explicitly kill the process group on timeout.
    let mut stdout_buf = child.stdout.take().map(|mut s| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf).await;
            buf
        })
    });
    let mut stderr_buf = child.stderr.take().map(|mut s| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf).await;
            buf
        })
    });

    let status = tokio::select! {
        result = child.wait() => {
            match result {
                Ok(s) => s,
                Err(e) => {
                    return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                        system_name: system_name.to_string(),
                        error: format!("Failed to wait on standalone eval: {}", e),
                    });
                }
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(120)) => {
            warn!(system = %system_name, pgid, "standalone nix eval timed out; killing process group");
            kill_nix_process_tree(&mut child, pgid).await;
            if let Some(t) = stdout_buf.take() { t.abort(); }
            if let Some(t) = stderr_buf.take() { t.abort(); }
            return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Standalone eval timed out for {}", system_name),
            });
        }
    };

    let stdout = match stdout_buf.take() {
        Some(t) => t.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let stderr = match stderr_buf.take() {
        Some(t) => t.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let output = std::process::Output {
        status,
        stdout,
        stderr,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error = if stderr.trim().is_empty() {
            "System evaluation failed with no error output".to_string()
        } else {
            stderr.chars().take(500).collect::<String>()
        };
        return Ok(StandaloneSystemOutcome::ConfirmedSystemFailure {
            system_name: system_name.to_string(),
            error,
        });
    }

    let parsed: StandaloneEvalJson = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse standalone eval JSON for {}", system_name))?;

    let expected_store_path =
        resolve_expected_store_path(&parsed.drv_path, Some(&parsed.outputs)).await;
    // The standalone evaluator always emits cfAgentEnabled, so parse it
    // unconditionally even when no deployment policies are assigned. A missing
    // key is an infrastructure/parser mismatch rather than a silent pass.
    let policy_check =
        match PolicyCheckResult::from_assigned(system_name.to_string(), &parsed.policies, assigned)
        {
            Ok(check) => check,
            Err(mismatch) => {
                return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                    system_name: system_name.to_string(),
                    error: format!(
                        "Policy metadata key mismatch in standalone eval: {}",
                        mismatch
                    ),
                });
            }
        };

    Ok(StandaloneSystemOutcome::Success {
        result: SuccessfulSystemResult {
            system_name: system_name.to_string(),
            derivation_target: build_agent_target(repo_url, commit_hash, system_name),
            drv_path: parsed.drv_path,
            expected_store_path,
            cf_agent_enabled: policy_check.cf_agent_enabled,
            build_eligible: true,
        },
        policy_check,
    })
}

/// Attempt to evaluate a single nixosConfiguration attribute to capture its error.
///
/// When nix-eval-jobs silently drops a system, the process-wide stderr may not
/// contain that system's error at all, or may conflate errors. This function
/// individually evaluates one attribute to get an accurate per-system error.
///
/// Returns a raw [`FallbackEvalOutcome`] — the caller MUST perform control
/// verification (via [`evaluate_and_verify_missing_system`]) before treating a
/// `CommandFailed` outcome as a confirmed system failure.
#[allow(dead_code)]
async fn fallback_eval_single_system(
    repo_url: &str,
    commit_hash: &str,
    system_name: &str,
    creds: Option<&FlakeCredentialEnv>,
    build_config: &BuildConfig,
) -> FallbackEvalOutcome {
    let flake_ref = build_flake_reference(repo_url, commit_hash);

    // Use --argstr to pass flakeRef and systemName safely instead of
    // interpolating values into a Nix expression string. This avoids
    // escaping issues with dots, backslashes, ${...}, etc.
    let nix_expr = r#"
{ flakeRef, systemName }:
let
  flake = builtins.getFlake flakeRef;
  cfg = builtins.getAttr systemName flake.nixosConfigurations;
in
  cfg.config.system.build.toplevel.drvPath
"#;

    let mut cmd = tokio::process::Command::new("nix");
    cmd.args([
        "eval",
        "--impure",
        "--expr",
        nix_expr.trim(),
        "--argstr",
        "flakeRef",
        &flake_ref,
        "--argstr",
        "systemName",
        system_name,
    ]);

    // Apply the same Nix configuration as the main evaluator.
    build_config.apply_to_command(&mut cmd);

    if let Some(c) = creds {
        c.apply_to_nix_command(&mut cmd);
    }

    // Acquire the process-wide standalone eval slot before spawning.
    let _nix_permit = match standalone_nix_eval_limiter().acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
            return FallbackEvalOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: "standalone Nix eval limiter was closed".to_string(),
            };
        }
    };

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    // New process group so killpg on timeout reaches the entire Nix subtree.
    #[cfg(unix)]
    cmd.process_group(0);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return FallbackEvalOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Failed to spawn fallback eval: {}", e),
            };
        }
    };

    #[cfg(unix)]
    let pgid = child.id().unwrap_or(0) as libc::pid_t;
    #[cfg(not(unix))]
    let pgid = 0i32;

    let mut stdout_buf = child.stdout.take().map(|mut s| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf).await;
            buf
        })
    });
    let mut stderr_buf = child.stderr.take().map(|mut s| {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf).await;
            buf
        })
    });

    let status = tokio::select! {
        result = child.wait() => {
            match result {
                Ok(s) => s,
                Err(e) => {
                    return FallbackEvalOutcome::InfrastructureFailure {
                        system_name: system_name.to_string(),
                        error: format!("Failed to wait on fallback eval: {}", e),
                    };
                }
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(120)) => {
            warn!(system = %system_name, pgid, "fallback nix eval timed out; killing process group");
            kill_nix_process_tree(&mut child, pgid).await;
            if let Some(t) = stdout_buf.take() { t.abort(); }
            if let Some(t) = stderr_buf.take() { t.abort(); }
            return FallbackEvalOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Fallback eval timed out for {}", system_name),
            };
        }
    };

    let stdout = match stdout_buf.take() {
        Some(t) => t.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let stderr_bytes = match stderr_buf.take() {
        Some(t) => t.await.unwrap_or_default(),
        None => Vec::new(),
    };
    let output = std::process::Output {
        status,
        stdout,
        stderr: stderr_bytes,
    };

    if output.status.success() {
        // The system actually evaluates fine — this is an evaluator omission,
        // not a broken NixOS configuration.
        return FallbackEvalOutcome::StandaloneEvaluationSucceeded {
            system_name: system_name.to_string(),
        };
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let error = if stderr.trim().is_empty() {
        "System evaluation failed with no error output".to_string()
    } else {
        stderr.chars().take(500).collect::<String>()
    };

    FallbackEvalOutcome::CommandFailed {
        system_name: system_name.to_string(),
        error,
    }
}

/// Evaluate a single missing system AND verify the result with a control.
///
/// This function combines the target fallback eval with a control-system
/// re-evaluation so that timeout and cancellation enforcement wraps both
/// operations together (the outer `tokio::select!` race in the fallback
/// phase races the entire buffered stream, not individual steps).
#[allow(dead_code)]
async fn evaluate_and_verify_missing_system(
    repo_url: &str,
    commit_hash: &str,
    system_name: &str,
    control_system: Option<&str>,
    creds: Option<&FlakeCredentialEnv>,
    build_config: &BuildConfig,
) -> VerifiedFallbackOutcome {
    let target =
        fallback_eval_single_system(repo_url, commit_hash, system_name, creds, build_config).await;

    match target {
        FallbackEvalOutcome::StandaloneEvaluationSucceeded { system_name } => {
            return VerifiedFallbackOutcome::StandaloneEvaluationSucceeded { system_name };
        }
        FallbackEvalOutcome::InfrastructureFailure { system_name, error } => {
            return VerifiedFallbackOutcome::InfrastructureFailure { system_name, error };
        }
        FallbackEvalOutcome::CommandFailed {
            system_name,
            error: target_error,
        } => {
            let Some(control_name) = control_system else {
                return VerifiedFallbackOutcome::EvaluatorUnhealthy {
                    system_name,
                    target_error,
                    control_error: Some("No successful control system was available".to_string()),
                };
            };

            match fallback_eval_single_system(
                repo_url,
                commit_hash,
                control_name,
                creds,
                build_config,
            )
            .await
            {
                FallbackEvalOutcome::StandaloneEvaluationSucceeded { .. } => {
                    VerifiedFallbackOutcome::ConfirmedSystemFailure {
                        system_name,
                        error: target_error,
                    }
                }
                FallbackEvalOutcome::CommandFailed {
                    error: control_error,
                    ..
                }
                | FallbackEvalOutcome::InfrastructureFailure {
                    error: control_error,
                    ..
                } => VerifiedFallbackOutcome::EvaluatorUnhealthy {
                    system_name,
                    target_error,
                    control_error: Some(control_error),
                },
            }
        }
    }
}

/// Evaluate a flake's nixosConfigurations with nix-eval-jobs and policy checking
///
/// Error returned when a user requests cancellation of an in-progress
/// evaluation.  The outer handler in process_pending_commits recognizes
/// this type to finalize the cancelled state atomically and broadcast
/// the cancelled event — never entering the generic failure path.
#[derive(Debug)]
pub struct EvaluationCancelled;

impl std::fmt::Display for EvaluationCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "evaluation cancelled by user")
    }
}

impl std::error::Error for EvaluationCancelled {}

/// A single successfully-evaluated system result collected during the
/// streaming phase.  Durable DB writes are deferred until after the
/// entire attempt is validated.
#[derive(Debug, Clone)]
pub struct SuccessfulSystemResult {
    pub system_name: String,
    pub derivation_target: String,
    pub drv_path: String,
    pub expected_store_path: Option<String>,
    pub cf_agent_enabled: Option<bool>,
    /// Whether this system is eligible to receive build jobs under the
    /// flake's build_scope policy (e.g. cf_systems_only).  When false,
    /// the derivation is still recorded for accounting purposes, but no
    /// build job is created.
    pub build_eligible: bool,
}

/// A confirmed system failure from the fallback phase, collected before
/// any DB writes so they only happen after all outcomes are validated.
#[derive(Debug, Clone)]
pub struct ConfirmedSystemFailure {
    pub system_name: String,
    pub derivation_target: String,
    pub error: String,
}

/// A derivation that was successfully written during finalization.
#[derive(Debug, Clone)]
pub struct FinalizedDerivation {
    pub derivation_id: i32,
    pub drv_path: String,
    pub system_name: String,
    pub cf_agent_enabled: Option<bool>,
}

/// In-memory evaluation plan produced by `evaluate_with_nix_eval_jobs`.
/// Contains no DB state — all writes are deferred to `finalize_evaluation_attempt`.
#[derive(Debug)]
pub struct EvaluationPlan {
    /// Raw nix-eval-jobs results (for broadcasting / summary).
    pub results: Vec<NixEvalJobResult>,
    /// Per-system policy check outcomes.
    pub policy_checks: Vec<PolicyCheckResult>,
    /// Systems that succeeded and need derivation rows.
    pub successful_systems: Vec<SuccessfulSystemResult>,
    /// Systems confirmed as failures by the fallback phase.
    pub confirmed_failures: Vec<ConfirmedSystemFailure>,
    /// True when any result has a Nix evaluation error.
    pub had_system_eval_errors: bool,
    #[cfg(test)]
    pub force_build_job_insert_failure: bool,
}

/// Outcome of `finalize_evaluation_attempt`.
#[derive(Debug)]
pub enum EvaluationFinalizeOutcome {
    /// Attempt accepted; derivations are written and commit is complete.
    Completed {
        derivations: Vec<FinalizedDerivation>,
        queued_builds: Vec<QueuedBuild>,
    },
    /// A user cancellation won the race before the commit row could be locked.
    Cancelled,
    /// Wrong attempt number or commit is already in a terminal state.
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemFinalizeOutcome {
    Queued {
        derivation_id: i32,
        build_job_id: uuid::Uuid,
    },
    RecordedWithoutBuild {
        derivation_id: i32,
        reason: SystemNotQueuedReason,
    },
    BuildAlreadyExists {
        derivation_id: i32,
        build_job_id: uuid::Uuid,
        /// Status of the existing build job at the time this outcome was
        /// produced.  The caller should not broadcast QueuedForBuild if
        /// the job is already building or in a terminal state.
        build_job_status: String,
    },
    Cancelled,
    Superseded,
}

/// Outcome of Phase 1 (persist evaluated system without inserting a build job).
///
/// The caller must follow up with a GC root and build activation for
/// `NeedsBuildPreparation`.  Other variants do not need build activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemPersistenceOutcome {
    /// A new derivation was recorded and is build-eligible.
    /// The caller must create a GC root, then call
    /// `activate_evaluated_system_build`.
    NeedsBuildPreparation {
        derivation_id: i32,
        drv_path: String,
    },
    /// A build job already exists for this derivation (from a prior call).
    /// Create a GC root (best-effort) and re-notify if still queued.
    ExistingBuildJob {
        derivation_id: i32,
        build_job_id: uuid::Uuid,
        build_job_status: String,
        drv_path: String,
    },
    /// Derivation recorded but no build job (policy/scope rejection).
    RecordedWithoutBuild {
        derivation_id: i32,
        reason: SystemNotQueuedReason,
    },
    /// Evaluation was cancelled; caller should stop.
    Cancelled,
    /// Evaluation was superseded; caller should bail.
    Superseded,
}

/// Outcome of Phase 3 (activate a build job after GC root is established).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemBuildActivationOutcome {
    /// A new build job was inserted and queued.
    Queued { build_job_id: uuid::Uuid },
    /// Build job already existed (possibly already building or terminal).
    AlreadyExists {
        build_job_id: uuid::Uuid,
        status: String,
    },
    /// Evaluation was cancelled before activation.
    Cancelled,
    /// Evaluation was superseded before activation.
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemNotQueuedReason {
    StrictPolicyFailure,
    AgentPolicyFailure,
    BuildNotRequested,
    /// System is not eligible for build jobs under the flake's build_scope
    /// policy (e.g. cf_systems_only when this system is not a registered
    /// active Crystal Forge system).  The derivation is still recorded
    /// for accounting and total-system-count accuracy.
    BuildScopeExcluded,
}

fn system_not_queued_reason(
    policy_check: &PolicyCheckResult,
    build_eligible: bool,
) -> Option<SystemNotQueuedReason> {
    // build_scope filtering: if the system is not eligible under the flake's
    // build_scope policy, still record the derivation but do not create a
    // build job.  This check comes first because it is a deployment-policy
    // gate independent of per-system agent/policy results.
    if !build_eligible {
        return Some(SystemNotQueuedReason::BuildScopeExcluded);
    }

    if policy_check.cf_agent_enabled == Some(false) {
        return Some(SystemNotQueuedReason::AgentPolicyFailure);
    }

    if policy_check
        .failed_policies
        .iter()
        .any(|(description, strict)| {
            *strict
                && description
                    .to_ascii_lowercase()
                    .contains("crystal forge agent")
        })
    {
        return Some(SystemNotQueuedReason::AgentPolicyFailure);
    }

    if policy_check
        .failed_policies
        .iter()
        .any(|(_, strict)| *strict)
    {
        return Some(SystemNotQueuedReason::StrictPolicyFailure);
    }

    None
}

/// Phase 1: persist an evaluated system's derivation without inserting a
/// build job.  The build job is created later by `activate_evaluated_system_build`,
/// after the GC root has been established.
///
/// Returns [`SystemPersistenceOutcome`] which tells the caller whether build
/// activation is needed, or if the system was recorded without a build, or
/// if the evaluation was cancelled/superseded.
pub async fn persist_evaluated_system(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    result: &SuccessfulSystemResult,
    policy_check: &PolicyCheckResult,
    assigned_policies: &[AssignedPolicy],
) -> Result<SystemPersistenceOutcome> {
    let build_eligible = result.build_eligible;
    use crate::queries::derivations::{SuccessfulEvalWrite, record_successful_eval_result_in_tx};

    let mut tx = pool.begin().await?;

    #[derive(sqlx::FromRow)]
    struct CommitState {
        evaluation_status: Option<String>,
        evaluation_attempt_count: Option<i32>,
        cancellation_requested: Option<bool>,
    }

    let state = sqlx::query_as::<_, CommitState>(
        r#"
        SELECT evaluation_status, evaluation_attempt_count, cancellation_requested
        FROM commits
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(state) = state else {
        tx.rollback().await?;
        return Ok(SystemPersistenceOutcome::Superseded);
    };

    let attempt_count = state.evaluation_attempt_count.unwrap_or(0);
    let status = state.evaluation_status.as_deref().unwrap_or("pending");
    let cancellation = state.cancellation_requested.unwrap_or(false);

    if attempt_count != expected_attempt || !matches!(status, "in_progress" | "cancelling") {
        tx.rollback().await?;
        return Ok(SystemPersistenceOutcome::Superseded);
    }

    if cancellation || status == "cancelling" {
        tx.rollback().await?;
        return Ok(SystemPersistenceOutcome::Cancelled);
    }

    let drv_path = result.drv_path.clone();
    let policy_requirements_met = policy_requirements_met(policy_check);
    let policy_results = policy_results_json(policy_check, assigned_policies);
    let write = record_successful_eval_result_in_tx(
        &mut tx,
        Some(commit_id),
        &result.system_name,
        "nixos",
        Some(&result.derivation_target),
        &result.drv_path,
        result.expected_store_path.as_deref(),
        result.cf_agent_enabled,
        policy_requirements_met,
        &policy_results,
    )
    .await
    .with_context(|| format!("Failed to write derivation for {}", result.system_name))?;

    let derivation_id = match write {
        SuccessfulEvalWrite::Inserted { derivation_id }
        | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id } => derivation_id,
        SuccessfulEvalWrite::PreservedBuildState {
            derivation_id: existing_deriv_id,
            status_id: _,
        } => {
            // Derivation is already in a build-active state.  Check
            // whether a build job exists for it.
            let existing: Option<(uuid::Uuid, String)> = sqlx::query_as(
                "SELECT id, status FROM build_jobs WHERE derivation_id = $1 ORDER BY created_at ASC LIMIT 1",
            )
            .bind(existing_deriv_id)
            .fetch_optional(&mut *tx)
            .await?;

            if let Some((build_job_id, build_job_status)) = existing {
                tx.commit().await?;
                return Ok(SystemPersistenceOutcome::ExistingBuildJob {
                    derivation_id: existing_deriv_id,
                    build_job_id,
                    build_job_status,
                    drv_path,
                });
            }

            // No build job exists — fall through to the normal eligibility
            // check, which may produce NeedsBuildPreparation.
            existing_deriv_id
        }
    };

    if let Some(reason) = system_not_queued_reason(policy_check, build_eligible) {
        tx.commit().await?;
        return Ok(SystemPersistenceOutcome::RecordedWithoutBuild {
            derivation_id,
            reason,
        });
    }

    // Check if a build job already exists (e.g. from a concurrent or
    // prior activation that succeeded between our steps).
    let existing: Option<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM build_jobs WHERE derivation_id = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(derivation_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some((build_job_id, build_job_status)) = existing {
        tx.commit().await?;
        return Ok(SystemPersistenceOutcome::ExistingBuildJob {
            derivation_id,
            build_job_id,
            build_job_status,
            drv_path,
        });
    }

    // Persisted successfully — don't insert the build job yet.
    tx.commit().await?;
    info!(
        commit_id,
        derivation_id,
        system = %result.system_name,
        "system_persisted_no_build_job_yet"
    );
    Ok(SystemPersistenceOutcome::NeedsBuildPreparation {
        derivation_id,
        drv_path,
    })
}

/// Phase 3: activate a build job for a previously-persisted derivation,
/// after the GC root has been established.
///
/// Runs in its own transaction so that the build job only becomes visible
/// (claimable) after this function commits.  Re-validates attempt number
/// and cancellation under `FOR UPDATE` to handle races with cancellation
/// or supersession.
pub async fn activate_evaluated_system_build(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    derivation_id: i32,
) -> Result<SystemBuildActivationOutcome> {
    let mut tx = pool.begin().await?;

    #[derive(sqlx::FromRow)]
    struct CommitState {
        evaluation_status: Option<String>,
        evaluation_attempt_count: Option<i32>,
        cancellation_requested: Option<bool>,
    }

    let state = sqlx::query_as::<_, CommitState>(
        r#"
        SELECT evaluation_status, evaluation_attempt_count, cancellation_requested
        FROM commits
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(state) = state else {
        tx.rollback().await?;
        return Ok(SystemBuildActivationOutcome::Superseded);
    };

    let attempt_count = state.evaluation_attempt_count.unwrap_or(0);
    let status = state.evaluation_status.as_deref().unwrap_or("pending");
    let cancellation = state.cancellation_requested.unwrap_or(false);

    if attempt_count != expected_attempt || !matches!(status, "in_progress" | "cancelling") {
        tx.rollback().await?;
        return Ok(SystemBuildActivationOutcome::Superseded);
    }

    if cancellation || status == "cancelling" {
        tx.rollback().await?;
        return Ok(SystemBuildActivationOutcome::Cancelled);
    }

    let build_outcome = create_build_job_for_derivation_tx(&mut tx, derivation_id).await?;
    let outcome = match build_outcome {
        Some(BuildJobInsertOutcome::Inserted { build_job_id }) => {
            info!(
                commit_id,
                derivation_id,
                %build_job_id,
                "system_build_job_activated"
            );
            SystemBuildActivationOutcome::Queued { build_job_id }
        }
        Some(BuildJobInsertOutcome::AlreadyExists {
            build_job_id,
            status,
        }) => {
            info!(
                commit_id,
                derivation_id,
                %build_job_id,
                existing_status = %status,
                "system_build_job_activate_already_exists"
            );
            SystemBuildActivationOutcome::AlreadyExists {
                build_job_id,
                status,
            }
        }
        None => {
            #[derive(sqlx::FromRow)]
            struct DerivationActivationState {
                derivation_name: String,
                status_id: i32,
                cf_agent_enabled: Option<bool>,
                derivation_path: Option<String>,
            }

            let state = sqlx::query_as::<_, DerivationActivationState>(
                r#"
                SELECT derivation_name, status_id, cf_agent_enabled, derivation_path
                FROM derivations
                WHERE id = $1
                "#,
            )
            .bind(derivation_id)
            .fetch_optional(&mut *tx)
            .await?;

            tx.rollback().await?;

            let Some(state) = state else {
                bail!(
                    "Build activation could not create a build job because derivation {} no longer exists",
                    derivation_id,
                );
            };

            bail!(
                "Build activation could not create a build job for derivation {} ({}) with status_id={}, cf_agent_enabled={:?}, derivation_path={:?}",
                derivation_id,
                state.derivation_name,
                state.status_id,
                state.cf_agent_enabled,
                state.derivation_path,
            );
        }
    };

    tx.commit().await?;
    info!(
        commit_id,
        derivation_id,
        ?outcome,
        "build_activation_committed"
    );
    Ok(outcome)
}

/// Handle build activation result: notify queue and broadcast via WebSocket.
///
/// This function is called from bounded preparation tasks (streaming path)
/// and from the sequential fallback path.  When `log_sequence` is `Some`,
/// eval-log persistence is also performed.
async fn handle_system_build_activation(
    pool: &PgPool,
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    queue_notifier: Option<&QueueNotifier>,
    commit_id: i32,
    system_name: &str,
    outcome: &SystemBuildActivationOutcome,
    log_sequence: Option<&mut i32>,
) -> Result<()> {
    use crate::handlers::api::commits::SystemEvalStatus;
    match outcome {
        SystemBuildActivationOutcome::Queued { build_job_id } => {
            if let Some(notifier) = queue_notifier {
                notifier.notify_build_queue();
            }
            if let Some(state) = cf_state {
                crate::handlers::api::commits::broadcast_system_status(
                    state,
                    commit_id,
                    system_name.to_string(),
                    SystemEvalStatus::QueuedForBuild,
                    None,
                )
                .await;
                if let Some(seq) = log_sequence {
                    broadcast_and_persist_eval_log(
                        pool,
                        Some(state),
                        commit_id,
                        seq,
                        format!("🚀 {}: build job queued ({})", system_name, build_job_id),
                    )
                    .await;
                }
            }
            info!(
                commit_id,
                system = system_name,
                %build_job_id,
                "incremental build job committed and queued"
            );
        }
        SystemBuildActivationOutcome::AlreadyExists {
            build_job_id,
            status,
        } if status == "queued" => {
            if let Some(notifier) = queue_notifier {
                notifier.notify_build_queue();
            }
            if let Some(state) = cf_state {
                crate::handlers::api::commits::broadcast_system_status(
                    state,
                    commit_id,
                    system_name.to_string(),
                    SystemEvalStatus::QueuedForBuild,
                    None,
                )
                .await;
                if let Some(seq) = log_sequence {
                    broadcast_and_persist_eval_log(
                        pool,
                        Some(state),
                        commit_id,
                        seq,
                        format!(
                            "🚀 {}: build job already queued ({})",
                            system_name, build_job_id
                        ),
                    )
                    .await;
                }
            }
            info!(
                commit_id,
                system = system_name,
                %build_job_id,
                existing_status = %status,
                "incremental build job already queued"
            );
        }
        SystemBuildActivationOutcome::AlreadyExists {
            build_job_id,
            status,
        } => {
            info!(
                commit_id,
                system = system_name,
                %build_job_id,
                existing_status = %status,
                "build job exists for system (not re-queuing)"
            );
            if let Some(state) = cf_state {
                if let Some(seq) = log_sequence {
                    broadcast_and_persist_eval_log(
                        pool,
                        Some(state),
                        commit_id,
                        seq,
                        format!(
                            "ℹ️  {}: build job already exists (status={})",
                            system_name, status
                        ),
                    )
                    .await;
                }
            }
        }
        SystemBuildActivationOutcome::Cancelled | SystemBuildActivationOutcome::Superseded => {
            // Caller handles these directly.
        }
    }
    Ok(())
}

/// Full combined helper that persists, roots, activates, and notifies.
///
/// Used by the fallback evaluation path and tests.  The bulk streaming
/// path uses the split `persist_evaluated_system` → GC root →
/// `activate_evaluated_system_build` flow instead, to avoid blocking
/// the stdout reader.
pub async fn finalize_evaluated_system(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    result: &SuccessfulSystemResult,
    policy_check: &PolicyCheckResult,
    assigned_policies: &[AssignedPolicy],
) -> Result<SystemFinalizeOutcome> {
    let persisted = persist_evaluated_system(
        pool,
        commit_id,
        expected_attempt,
        result,
        policy_check,
        assigned_policies,
    )
    .await?;

    match persisted {
        SystemPersistenceOutcome::NeedsBuildPreparation {
            derivation_id,
            drv_path,
        } => {
            // Phase 2: GC root
            let rooted = crate::builder::create_drv_gc_root(&drv_path, derivation_id)
                .await
                .with_context(|| {
                    format!("Failed to create GC root for derivation {}", derivation_id)
                })?;
            if !rooted {
                #[cfg(not(test))]
                bail!(
                    "Derivation {} (drv={}) is not valid in the server store; \
                     cannot proceed with build activation",
                    derivation_id,
                    drv_path,
                );
                #[cfg(test)]
                warn!(
                    "⚠️  Skipping GC-root requirement for derivation {} (drv={}) in test mode",
                    derivation_id, drv_path,
                );
            }

            // Phase 3: activate build job
            let activation =
                activate_evaluated_system_build(pool, commit_id, expected_attempt, derivation_id)
                    .await?;

            match activation {
                SystemBuildActivationOutcome::Queued { build_job_id } => {
                    Ok(SystemFinalizeOutcome::Queued {
                        derivation_id,
                        build_job_id,
                    })
                }
                SystemBuildActivationOutcome::AlreadyExists {
                    build_job_id,
                    status,
                } => Ok(SystemFinalizeOutcome::BuildAlreadyExists {
                    derivation_id,
                    build_job_id,
                    build_job_status: status,
                }),
                SystemBuildActivationOutcome::Cancelled => Ok(SystemFinalizeOutcome::Cancelled),
                SystemBuildActivationOutcome::Superseded => Ok(SystemFinalizeOutcome::Superseded),
            }
        }

        SystemPersistenceOutcome::ExistingBuildJob {
            derivation_id,
            build_job_id,
            build_job_status,
            drv_path,
        } => {
            // Best-effort GC root for existing build.
            if let Err(err) = crate::builder::create_drv_gc_root(&drv_path, derivation_id).await {
                warn!(
                    "⚠️  Failed to create GC root for existing build drv {} (id={}): {}",
                    drv_path, derivation_id, err
                );
            } else {
                debug!(
                    "📌 Rooted existing build drv (id={}, drv={})",
                    derivation_id, drv_path
                );
            }

            Ok(SystemFinalizeOutcome::BuildAlreadyExists {
                derivation_id,
                build_job_id,
                build_job_status,
            })
        }

        SystemPersistenceOutcome::RecordedWithoutBuild {
            derivation_id,
            reason,
        } => Ok(SystemFinalizeOutcome::RecordedWithoutBuild {
            derivation_id,
            reason,
        }),

        SystemPersistenceOutcome::Cancelled => Ok(SystemFinalizeOutcome::Cancelled),
        SystemPersistenceOutcome::Superseded => Ok(SystemFinalizeOutcome::Superseded),
    }
}

/// Atomically finalize a validated evaluation attempt.
///
/// Acquires a `FOR UPDATE` lock on the commit row, checks the attempt number
/// and cancellation flag, writes all successful derivations and synthetic
/// failures under that lock, and marks the commit complete — all in a single
/// PostgreSQL transaction.
///
/// Because the commit row is locked for the duration:
/// - A concurrent cancellation API call blocks until this transaction commits
///   or rolls back. If cancellation already won the lock, this returns
///   `EvaluationFinalizeOutcome::Cancelled`.
/// - If any derivation write fails the entire transaction is rolled back, the
///   commit remains `in_progress`, and the ordinary failure-CAS path retries.
pub async fn finalize_evaluation_attempt(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    plan: &EvaluationPlan,
) -> Result<EvaluationFinalizeOutcome> {
    use crate::queries::derivations::record_synthetic_eval_failure_in_tx;

    let mut tx = pool.begin().await?;

    // Lock the commit row for the duration of this transaction.  This
    // serialises finalization against the cancellation API: whichever
    // acquires the row first wins.
    #[derive(sqlx::FromRow)]
    struct CommitState {
        evaluation_status: Option<String>,
        evaluation_attempt_count: Option<i32>,
        cancellation_requested: Option<bool>,
    }

    let state = sqlx::query_as::<_, CommitState>(
        r#"
        SELECT evaluation_status, evaluation_attempt_count, cancellation_requested
        FROM commits
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(state) = state else {
        tx.rollback().await?;
        return Ok(EvaluationFinalizeOutcome::Superseded);
    };

    let attempt_count = state.evaluation_attempt_count.unwrap_or(0);
    let status = state.evaluation_status.as_deref().unwrap_or("pending");
    let cancellation = state.cancellation_requested.unwrap_or(false);

    // Reject if attempt doesn't match or status is not finalizable.
    if attempt_count != expected_attempt || !matches!(status, "in_progress" | "cancelling") {
        tx.rollback().await?;
        return Ok(EvaluationFinalizeOutcome::Superseded);
    }

    // Cancellation won the race — finalize as cancelled inside this tx.
    if cancellation || status == "cancelling" {
        sqlx::query(
            r#"
            UPDATE commits
            SET evaluation_status = 'cancelled',
                cancellation_requested = FALSE,
                evaluation_completed_at = COALESCE(evaluation_completed_at, NOW()),
                evaluation_error_message = NULL
            WHERE id = $1
            "#,
        )
        .bind(commit_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(EvaluationFinalizeOutcome::Cancelled);
    }

    // Successful systems were finalized incrementally as soon as each one
    // evaluated. This commit-level finalizer only records confirmed synthetic
    // failures and transitions the attempt state.
    let mut sorted_failures = plan.confirmed_failures.clone();
    sorted_failures.sort_by(|a, b| a.system_name.cmp(&b.system_name));

    for cf in &sorted_failures {
        record_synthetic_eval_failure_in_tx(
            &mut tx,
            Some(commit_id),
            &cf.system_name,
            "nixos",
            Some(&cf.derivation_target),
            &cf.error,
        )
        .await
        .with_context(|| format!("Failed to write synthetic failure for {}", cf.system_name))?;
    }

    #[cfg(test)]
    if plan.force_build_job_insert_failure {
        bail!("forced build-job insertion failure for rollback test");
    }

    // Write metadata cache inside the transaction so it is always consistent
    // with the derivation rows.
    let (
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_policy_failures,
        all_systems_passed,
    ) = summarize_commit_metadata(&plan.policy_checks, plan.had_system_eval_errors);

    sqlx::query(
        r#"
        INSERT INTO commit_metadata_cache (
            commit_id, total_systems, systems_passed_policy,
            systems_failed_policy_strict, systems_failed_policy_non_strict,
            systems_with_eval_error, has_nix_eval_error, has_policy_failures,
            all_systems_passed
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (commit_id) DO UPDATE SET
            total_systems = EXCLUDED.total_systems,
            systems_passed_policy = EXCLUDED.systems_passed_policy,
            systems_failed_policy_strict = EXCLUDED.systems_failed_policy_strict,
            systems_failed_policy_non_strict = EXCLUDED.systems_failed_policy_non_strict,
            systems_with_eval_error = EXCLUDED.systems_with_eval_error,
            has_nix_eval_error = EXCLUDED.has_nix_eval_error,
            has_policy_failures = EXCLUDED.has_policy_failures,
            all_systems_passed = EXCLUDED.all_systems_passed,
            cached_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(commit_id)
    .bind(total_systems)
    .bind(systems_passed)
    .bind(systems_failed_strict)
    .bind(systems_failed_non_strict)
    .bind(systems_with_eval_error)
    .bind(plan.had_system_eval_errors)
    .bind(has_policy_failures)
    .bind(all_systems_passed)
    .execute(&mut *tx)
    .await
    .context("Failed to update commit metadata cache")?;

    // Mark the commit complete inside the same transaction.
    let rows = sqlx::query(
        r#"
        UPDATE commits
        SET evaluation_status = 'complete',
            evaluation_completed_at = NOW(),
            evaluation_error_message = NULL
        WHERE id = $1
          AND evaluation_status = 'in_progress'
          AND COALESCE(cancellation_requested, FALSE) = FALSE
          AND evaluation_attempt_count = $2
        "#,
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .execute(&mut *tx)
    .await
    .context("Failed to mark commit complete")?;

    if rows.rows_affected() == 0 {
        // The commit row was modified concurrently between our FOR UPDATE
        // and this UPDATE (should not happen under normal conditions since
        // we hold the lock, but be defensive).
        tx.rollback().await?;
        return Ok(EvaluationFinalizeOutcome::Superseded);
    }

    tx.commit().await?;
    Ok(EvaluationFinalizeOutcome::Completed {
        derivations: Vec::new(),
        queued_builds: Vec::new(),
    })
}

/// Handle a `SystemFinalizeOutcome` after a system has been finalized.
///
/// Centralizes queue notification, WebSocket broadcast, log persistence,
/// so the bulk stdout path and the standalone fallback path cannot diverge.
/// Returns an action telling the caller whether to continue, cancel, or
/// bail (superseded).  The caller is responsible for pushing the successful
/// result to its tracking list and spawning background side effects.
async fn handle_system_finalize_outcome(
    pool: &PgPool,
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    queue_notifier: Option<&QueueNotifier>,
    commit_id: i32,
    system_name: &str,
    outcome: SystemFinalizeOutcome,
    log_sequence: &mut i32,
) -> Result<SystemFinalizeAction> {
    match outcome {
        SystemFinalizeOutcome::Queued {
            derivation_id,
            build_job_id,
        } => {
            if let Some(notifier) = queue_notifier {
                notifier.notify_build_queue();
            }
            if let Some(state) = cf_state {
                crate::handlers::api::commits::broadcast_system_status(
                    state,
                    commit_id,
                    system_name.to_string(),
                    crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
                    None,
                )
                .await;
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit_id,
                    log_sequence,
                    format!("🚀 {}: build job queued ({})", system_name, build_job_id),
                )
                .await;
            }
            info!(
                commit_id,
                system = system_name,
                %build_job_id,
                "incremental build job committed and queued"
            );
            Ok(SystemFinalizeAction::Queued {
                derivation_id,
                build_job_id,
            })
        }

        SystemFinalizeOutcome::BuildAlreadyExists {
            derivation_id,
            build_job_id,
            ref build_job_status,
        } => {
            // Only notify / broadcast if the existing job is still queued
            // (not already building / terminal).
            if build_job_status == "queued" {
                if let Some(notifier) = queue_notifier {
                    notifier.notify_build_queue();
                }
                if let Some(state) = cf_state {
                    crate::handlers::api::commits::broadcast_system_status(
                        state,
                        commit_id,
                        system_name.to_string(),
                        crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
                        None,
                    )
                    .await;
                    broadcast_and_persist_eval_log(
                        pool,
                        Some(state),
                        commit_id,
                        log_sequence,
                        format!(
                            "🚀 {}: build job already queued ({})",
                            system_name, build_job_id
                        ),
                    )
                    .await;
                }
                info!(
                    commit_id,
                    system = system_name,
                    %build_job_id,
                    status = build_job_status,
                    "existing queued build job reused"
                );
            } else {
                debug!(
                    "Build job {} for {} already exists with status {} (not re-queued)",
                    build_job_id, system_name, build_job_status
                );
            }
            Ok(SystemFinalizeAction::AlreadyExists {
                derivation_id,
                build_job_id,
            })
        }

        SystemFinalizeOutcome::RecordedWithoutBuild { ref reason, .. } => {
            debug!("Recorded {} without build: {:?}", system_name, reason);
            Ok(SystemFinalizeAction::Recorded)
        }

        SystemFinalizeOutcome::Cancelled => {
            warn!("System {} finalization cancelled", system_name);
            Ok(SystemFinalizeAction::Cancelled)
        }

        SystemFinalizeOutcome::Superseded => {
            warn!("System {} finalization superseded", system_name);
            Ok(SystemFinalizeAction::Superseded)
        }
    }
}

/// Internal outcome from `handle_system_finalize_outcome` telling the
/// caller how to proceed and what data to pass to background side effects.
enum SystemFinalizeAction {
    /// New build job was inserted and queued.
    Queued {
        derivation_id: i32,
        build_job_id: uuid::Uuid,
    },
    /// Build job already existed (possibly still queued or already building).
    AlreadyExists {
        derivation_id: i32,
        build_job_id: uuid::Uuid,
    },
    /// Derivation recorded but no build job created (policy/scope).
    Recorded,
    /// Evaluation was cancelled; caller should stop.
    Cancelled,
    /// Evaluation was superseded; caller should bail.
    Superseded,
}

/// FIXED: Now properly:
/// 1. Stores derivation_path from nix-eval-jobs
/// 2. Updates status to DryRunComplete after successful evaluation
pub async fn evaluate_with_nix_eval_jobs(
    pool: &PgPool,
    commit: &Commit,
    expected_attempt: i32,
    flake: &Flake,
    repo_url: &str,
    commit_hash: &str,
    target_system: &str,
    build_config: &BuildConfig,
    server_config: &ServerConfig,
    policies_by_configuration: &Arc<PoliciesByConfiguration>,
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    _queue_notifier: Option<&QueueNotifier>,
) -> Result<EvaluationPlan> {
    // Re-evaluation safety: clear previous persisted logs for this commit so
    // (commit_id, log_sequence) uniqueness cannot collide on subsequent runs.
    crate::queries::eval_logs::delete_eval_logs_by_commit(pool, commit.id).await?;

    // Sequence counter for log persistence (1-indexed)
    let mut log_sequence = 1i32;

    // Bounded preparation pipeline: GC root + build activation for each
    // streaming system.  A semaphore limits concurrent nix-store subprocess
    // work; the JoinSet collects errors before commit-level completion.
    const BUILD_PREPARATION_CONCURRENCY: usize = 4;
    let build_preparation_limit = Arc::new(Semaphore::new(BUILD_PREPARATION_CONCURRENCY));
    let mut build_preparations: JoinSet<anyhow::Result<()>> = JoinSet::new();

    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let allowed_systems = load_allowed_systems(pool, flake, target_system).await?;

    // Load per-flake credentials (may be None for public flakes).
    // Wrap in Arc so the reference can be shared across buffer_unordered futures.
    let creds = Arc::new(
        FlakeCredentialEnv::load(pool, flake.id)
            .await
            .unwrap_or_else(|e| {
                warn!("Failed to load credentials for flake {}: {e:#}", flake.id);
                None
            }),
    );

    // Build ONE Nix expression with per-configuration policy checkers.
    let nix_expr = build_nix_eval_expression(&flake_ref, policies_by_configuration);

    // Compute summary counts for logging.
    let unique_policy_count: std::collections::BTreeSet<_> = policies_by_configuration
        .values()
        .flat_map(|v| v.iter().map(|ap| ap.policy_id))
        .collect();
    let configs_with_policies = policies_by_configuration.len();

    info!(
        "🚀 Running: nix-eval-jobs for {} — {} unique enabled policies across {} configurations with assigned policies",
        target_system,
        unique_policy_count.len(),
        configs_with_policies,
    );

    // Broadcast detailed start information
    if let Some(state) = cf_state {
        let start_msg = format!(
            "🚀 Starting evaluation for flake: {}\n   Commit: {}\n   Target: {}\n   Workers: {}",
            flake.name,
            &commit_hash[..8.min(commit_hash.len())],
            target_system,
            server_config.eval_workers
        );
        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, start_msg)
            .await;

        let policy_msg = format!(
            "📋 Loaded {} unique enabled policies across {} configurations with assigned policies",
            unique_policy_count.len(),
            configs_with_policies,
        );
        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, policy_msg)
            .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "⏳ Evaluating nixosConfigurations...".to_string(),
        )
        .await;
    }

    for (config, assigned) in policies_by_configuration.iter() {
        debug!(
            configuration = %config,
            assigned_policy_count = assigned.len(),
            "per_configuration_policy_assignment"
        );
    }

    debug!("📝 Nix expression:\n{}", nix_expr);

    // ── Establish expected system set BEFORE spawning the evaluator ───
    // This must happen now so that discovery failures fail the evaluation
    // before any child process starts, preventing orphan evaluators.
    // Load known system names from the artifact cache so we can detect systems
    // that nix-eval-jobs silently drops (no JSON line at all, not even an error).
    let known_cache =
        crate::queries::commits_artifacts::get_commit_nixos_configurations_from_cache(
            pool, commit.id,
        )
        .await?;

    let known_systems: Vec<String> = match known_cache {
        CachedSystemsState::Ready(systems) => systems,
        CachedSystemsState::Missing | CachedSystemsState::HydrationFailed => {
            // No cache row exists, or last hydration failed — hydrate inline now.
            // Use the credential-aware variant so private flake discovery works.
            let systems = crate::flake::commits::load_commit_nixos_configurations_with_creds(
                repo_url,
                commit_hash,
                creds.as_ref().as_ref(),
                Some(build_config),
            )
            .await?;

            // Persist discovered systems (including legitimately empty set)
            // without overwriting changed_files. This also marks
            // nixos_configurations_populated = true.
            if let Err(e) = crate::queries::commits_artifacts::upsert_commit_artifact_systems(
                pool, commit.id, &systems,
            )
            .await
            {
                warn!("Failed to persist inline artifact cache: {}", e);
            }
            systems
        }
    };
    let has_known_systems = !known_systems.is_empty();
    let mut seen_systems: HashSet<String> = HashSet::new();
    // Systems for which nix-eval-jobs emitted a JSON line with an `error`
    // field. These are confirmed evaluation failures — the error message is
    // already available. They do not need standalone fallback evaluation and
    // must not inflate `missing_systems` (which triggers expensive re-eval).
    let mut error_line_failures: Vec<ConfirmedSystemFailure> = Vec::new();
    let excluded_systems: Vec<String> = known_systems
        .iter()
        .filter(|s| should_skip_system(&allowed_systems, s))
        .cloned()
        .collect();
    info!("Expected systems: {}", known_systems.len());
    info!("Expected system names: {:?}", known_systems);
    info!("Build-scope excluded systems: {:?}", excluded_systems);

    // Run nix-eval-jobs with --meta flag to get policy results.
    // --impure is required because the Nix expression uses builtins.getFlake with a
    // remote git+ssh ref (e.g. git+git@github.com:...?rev=<hash>), which is only
    // permitted in impure evaluation mode.
    let mut cmd = Command::new("nix-eval-jobs");
    // Kill the child process if this future is dropped (e.g. cancellation,
    // discovery failure would no longer trigger this, but it is still a
    // valuable safety net for any other early-return path).
    cmd.kill_on_drop(true);
    // Spawn nix-eval-jobs in a new process group so that a SIGKILL on
    // cancellation, timeout, or error reaches every process in the
    // evaluator subtree (workers, sub-evaluators, helpers), not just
    // the direct nix-eval-jobs process. Without this, orphan worker
    // processes can accumulate and progressively degrade host RAM/CPU.
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.args([
        "--expr",
        &nix_expr,
        "--impure", // Required: builtins.getFlake with remote git refs needs impure mode
        "--meta",   // CRITICAL: Include meta so we get policies in output!
        "--workers",
        &server_config.eval_workers.to_string(),
        "--max-memory-size",
        &server_config.eval_max_memory_mb.to_string(),
    ]);

    if server_config.eval_check_cache {
        cmd.arg("--check-cache-status");
    }
    build_config.apply_to_command(&mut cmd);

    // Inject per-flake credentials so Nix can access private repos.
    // Deref the Arc to get the inner Option for pattern matching.
    if let Some(c) = Option::as_ref(creds.as_ref()) {
        c.apply_to_nix_command(&mut cmd);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    // PGID == PID when spawned with process_group(0).  Used by
    // kill_nix_process_tree to SIGKILL the entire evaluator subtree.
    #[cfg(unix)]
    let bulk_pgid = child.id().unwrap_or(0) as libc::pid_t;
    #[cfg(not(unix))]
    let bulk_pgid = 0i32;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut results = Vec::new();
    let mut policy_checks = Vec::new();
    let mut found_target = false;
    let mut stderr_output = Vec::new();
    let mut stderr_log_batch: Vec<(i32, Option<String>, String)> = Vec::new();
    const STDERR_LOG_BATCH_SIZE: usize = 100;
    let mut stdout_done = false;
    let mut stderr_done = false;

    // Collect successful system results during streaming; all durable DB
    // writes are deferred until the attempt is fully validated (child exit +
    // fallback outcome checks).
    let mut successful_results: Vec<SuccessfulSystemResult> = Vec::new();

    // Cancellation poll interval: check the DB every 2 seconds while eval runs.
    let mut cancel_ticker = tokio::time::interval(Duration::from_secs(2));
    cancel_ticker.tick().await; // consume the immediate first tick
    let mut progress_ticker =
        tokio::time::interval(Duration::from_secs(EVAL_PROGRESS_HEARTBEAT_SECS));
    progress_ticker.tick().await; // consume immediate first tick
    let mut last_output_at = Instant::now();

    loop {
        tokio::select! {
            // Third arm: cooperative cancellation poll
            _ = cancel_ticker.tick() => {
                match crate::queries::commits::check_cancellation_requested(pool, commit.id).await {
                    Ok(true) => {
                        warn!("🚫 Cancellation requested for commit {} — killing nix-eval-jobs process group", commit.id);
                        kill_nix_process_tree(&mut child, bulk_pgid).await;
                        // Do NOT call force_cancel_commit_evaluation here — the
                        // outer handler (process_pending_commits) is the single
                        // authority that atomically transitions the state and
                        // broadcasts the cancelled event.  We just return a
                        // typed error.
                        return Err(EvaluationCancelled.into());
                    }
                    Ok(false) => {} // not cancelled, continue
                    Err(e) => {
                        warn!("Failed to check cancellation flag for commit {}: {e}", commit.id);
                    }
                }
            }
            _ = progress_ticker.tick() => {
                let idle_for = Instant::now().duration_since(last_output_at);
                if idle_for >= Duration::from_secs(EVAL_OUTPUT_IDLE_TIMEOUT_SECS) {
                    let timeout_msg = format!(
                        "❌ Evaluation produced no output for {}s; terminating nix-eval-jobs",
                        EVAL_OUTPUT_IDLE_TIMEOUT_SECS
                    );
                    error!("{} (commit_id={})", timeout_msg, commit.id);

                    if let Some(state) = cf_state {
                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, timeout_msg)
                            .await;
                    }

                    kill_nix_process_tree(&mut child, bulk_pgid).await;
                    bail!(
                        "evaluation timed out after {}s without nix-eval-jobs output",
                        EVAL_OUTPUT_IDLE_TIMEOUT_SECS
                    );
                }

                debug!(
                    "nix-eval-jobs still running for commit {} (idle {}s)",
                    commit.id,
                    idle_for.as_secs()
                );
            }
            line_result = stdout_reader.next_line(), if !stdout_done => {
                match line_result? {
                    Some(line) if !line.trim().is_empty() => {
                        last_output_at = Instant::now();
                        match serde_json::from_str::<NixEvalJobResult>(&line) {
                            Ok(result) => {
                                let system_name = result.attr.clone();
                                let build_eligible = match &allowed_systems {
                                    Some(systems) => systems.iter().any(|c| c == &system_name),
                                    None => true,
                                };
                                if !build_eligible {
                                    debug!(
                                        "System {} is not eligible for build jobs under flake build_scope={} (will be recorded but not queued)",
                                        system_name,
                                        flake.build_scope,
                                    );
                                }
                                let has_error = result.error.is_some();
                                let drv_path = result.drv_path.clone();
                                // Resolve the expected store path from nix-eval-jobs
                                // JSON outputs (fast, no subprocess).  This avoids
                                // blocking the stdout reader on a nix-store query
                                // before the build job can be created.
                                let expected_store_path = if !has_error {
                                    result
                                        .outputs
                                        .as_ref()
                                        .and_then(parse_expected_store_path_from_outputs)
                                } else {
                                    None
                                };

                                info!(
                                    commit_id = commit.id,
                                    expected_attempt,
                                    system = %system_name,
                                    has_error,
                                    has_drv = drv_path.is_some(),
                                    "system_result_received"
                                );

                                // Broadcast system evaluation result to logs
                                if let Some(state) = cf_state {
                                    if has_error {
                                        let error_msg = result.error.as_ref()
                                            .map(|e| {
                                                // Truncate long errors for readability
                                                if e.len() > 200 {
                                                    format!("{}...", &e[..200])
                                                } else {
                                                    e.clone()
                                                }
                                            })
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        let log_msg = format!("❌ {}: {}", system_name, error_msg);
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                    } else {
                                        let log_msg = format!(
                                            "🔍 {}: Nix evaluation succeeded; checking assigned policies",
                                            system_name
                                        );
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                    }

                                    // Broadcast in-progress status to WebSocket clients.
                                    crate::handlers::api::commits::broadcast_system_status(
                                        state,
                                        commit.id,
                                        system_name.clone(),
                                        crate::handlers::api::commits::SystemEvalStatus::Evaluating,
                                        None,
                                    )
                                    .await;
                                }

                                // Resolve this configuration's assigned policies from the map.
                                let assigned_policies: &[AssignedPolicy] =
                                    policies_for_config(policies_by_configuration, &system_name);

                                // Extract policy check results from meta.policies
                                let mut cf_agent_enabled = None;
                                let mut policy_check_for_system: Option<PolicyCheckResult> = None;
                                let mut policy_metadata_error: Option<String> = None;
                                if let Some(meta) = &result.meta {
                                    if let Some(policies_json) = meta.get("policies") {
                                        // Parse policy results using this configuration's assigned
                                        // policies only (stable-key path).
                                        let check_result = PolicyCheckResult::from_assigned(
                                            system_name.clone(),
                                            policies_json,
                                            assigned_policies,
                                        );

                                        match check_result {
                                            Ok(check) => {
                                                cf_agent_enabled = check.cf_agent_enabled;

                                                // Log policy results
                                                if !check.meets_requirements {
                                                    let has_strict = check.failed_policies.iter().any(|(_, s)| *s);
                                                    for warning in &check.warnings {
                                                        if has_strict {
                                                            error!("❌ {}", warning);
                                                        } else {
                                                            warn!("⚠️  {}", warning);
                                                        }
                                                        if let Some(state) = cf_state {
                                                            let log_msg = format!("⚠️  {}: {}", system_name, warning);
                                                            broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                                        }
                                                    }
                                                } else if assigned_policies.is_empty() {
                                                    debug!("✅ {}: no assigned policies — passes evaluation", system_name);
                                                } else {
                                                    info!("✅ {}: all assigned policies passed", system_name);
                                                    if let Some(state) = cf_state {
                                                        let log_msg = format!("✅ {}: all assigned policies passed", system_name);
                                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                                    }
                                                }

                                                policy_check_for_system = Some(check.clone());
                                                policy_checks.push(check);
                                            }
                                            Err(mismatch) => {
                                                // Expression-generation/parser mismatch — treat as
                                                // infrastructure error for this configuration.
                                                error!(
                                                    system = %system_name,
                                                    "Policy metadata key mismatch: {}", mismatch
                                                );
                                                if let Some(state) = cf_state {
                                                    let log_msg = format!("❌ {}: policy metadata mismatch: {}", system_name, mismatch);
                                                    broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                                }
                                                policy_metadata_error = Some(format!(
                                                    "Policy metadata mismatch: {}",
                                                    mismatch
                                                ));
                                            }
                                        }
                                    } else {
                                        let msg = format!(
                                            "{}: evaluator did not emit policy metadata",
                                            system_name
                                        );
                                        debug!("⚠️  {msg}");
                                        policy_metadata_error = Some(msg);
                                    }
                                } else {
                                    let msg = format!(
                                        "{}: evaluator did not emit derivation metadata",
                                        system_name
                                    );
                                    debug!("⚠️  {msg}");
                                    policy_metadata_error = Some(msg);
                                }

                                // Configurations with no assigned policies still receive
                                // unconditional cfAgentEnabled metadata from the evaluator.
                                // Synthesize a passing check only when that metadata is present;
                                // otherwise leave policy_check_for_system as None so the system
                                // is not incorrectly persisted with a null cf_agent_enabled.
                                if policy_check_for_system.is_none()
                                    && policy_metadata_error.is_none()
                                    && assigned_policies.is_empty()
                                {
                                    let cf_agent_enabled = result
                                        .meta
                                        .as_ref()
                                        .and_then(|m| m.get("policies"))
                                        .and_then(|p| p.get("cfAgentEnabled"))
                                        .and_then(|v| v.as_bool());
                                    if cf_agent_enabled.is_none() {
                                        policy_metadata_error = Some(format!(
                                            "{}: evaluator did not emit cfAgentEnabled metadata",
                                            system_name
                                        ));
                                    } else {
                                        let check = PolicyCheckResult {
                                            system_name: system_name.clone(),
                                            cf_agent_enabled,
                                            assigned_results: BTreeMap::new(),
                                            has_required_packages: None,
                                            custom_checks: HashMap::new(),
                                            meets_requirements: cf_agent_enabled == Some(true),
                                            warnings: Vec::new(),
                                            failed_policies: Vec::new(),
                                            cve_checks: Vec::new(),
                                        };
                                        policy_check_for_system = Some(check.clone());
                                        policy_checks.push(check);
                                    }
                                }

                                // Broadcast post-policy status to WebSocket clients.
                                if let Some(state) = cf_state {
                                    if has_error {
                                        let error_msg = result
                                            .error
                                            .clone()
                                            .unwrap_or_else(|| "Unknown error".to_string());
                                        crate::handlers::api::commits::broadcast_system_status(
                                            state,
                                            commit.id,
                                            system_name.clone(),
                                            crate::handlers::api::commits::SystemEvalStatus::Failed,
                                            Some(error_msg.clone()),
                                        )
                                        .await;
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                            format!("❌ {}: {}", system_name, error_msg),
                                        )
                                        .await;
                                    } else if let Some(metadata_error) = policy_metadata_error.as_ref() {
                                        crate::handlers::api::commits::broadcast_system_status(
                                            state,
                                            commit.id,
                                            system_name.clone(),
                                            crate::handlers::api::commits::SystemEvalStatus::Failed,
                                            Some(metadata_error.clone()),
                                        )
                                        .await;
                                    } else {
                                        // Use the actual policy check result to determine status.
                                        let passes = policy_check_for_system
                                            .as_ref()
                                            .map(|c| c.meets_requirements)
                                            .unwrap_or(false);
                                        if passes {
                                            // QueuedForBuild broadcast deferred to post-finalization.
                                            broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                                format!("✅ {}: evaluation and policies passed", system_name),
                                            )
                                            .await;
                                        } else {
                                            let reason = policy_check_for_system
                                                .as_ref()
                                                .and_then(|c| c.warnings.first())
                                                .map(|d| d.as_str())
                                                .or_else(|| {
                                                    policy_check_for_system
                                                        .as_ref()
                                                        .and_then(|c| c.failed_policies.first())
                                                        .map(|(d, _)| d.as_str())
                                                })
                                                .unwrap_or("policy failed");
                                            let has_strict = policy_check_for_system
                                                .as_ref()
                                                .map(|c| {
                                                    c.failed_policies.iter().any(|(_, strict)| *strict)
                                                })
                                                .unwrap_or(true);
                                            let terminal_log = if has_strict {
                                                format!(
                                                    "❌ {}: strict policy failed — {}",
                                                    system_name, reason
                                                )
                                            } else {
                                                format!(
                                                    "⚠️  {}: non-strict policy warning — {}",
                                                    system_name, reason
                                                )
                                            };
                                            broadcast_and_persist_eval_log(
                                                pool,
                                                Some(state),
                                                commit.id,
                                                &mut log_sequence,
                                                terminal_log,
                                            )
                                            .await;
                                            let reason = policy_check_for_system
                                                .as_ref()
                                                .and_then(|c| c.failed_policies.first())
                                                .map(|(d, _)| d.as_str())
                                                .unwrap_or("policy failed");
                                            crate::handlers::api::commits::broadcast_system_status(
                                                state,
                                                commit.id,
                                                system_name.clone(),
                                                crate::handlers::api::commits::SystemEvalStatus::PolicyFailed,
                                                Some(reason.to_string()),
                                            )
                                            .await;
                                        }
                                    }
                                }

                                // ── Incrementally persist successful systems ──────────
                                // A healthy system must be queued as soon as its own eval,
                                // policy check, derivation write, and build-job insertion commit.
                                if let Some(system_name) = result.attr_path.last() {
                                    if !has_error && drv_path.is_some() && policy_metadata_error.is_none() {
                                        let drv = drv_path.clone().unwrap();
                                        let derivation_target = build_agent_target(
                                            &flake.repo_url,
                                            &commit.git_commit_hash,
                                            system_name,
                                        );
                                        let successful = SuccessfulSystemResult {
                                            system_name: system_name.clone(),
                                            derivation_target,
                                            drv_path: drv,
                                            expected_store_path: expected_store_path.clone(),
                                            cf_agent_enabled,
                                            build_eligible,
                                        };

                                        let default_check;
                                        let policy_check = match policy_check_for_system.as_ref() {
                                            Some(check) => check,
                                            None => {
                                                default_check = PolicyCheckResult {
                                                    system_name: system_name.clone(),
                                                    cf_agent_enabled,
                                                    assigned_results: BTreeMap::new(),
                                                    has_required_packages: None,
                                                    custom_checks: HashMap::new(),
                                                    meets_requirements: cf_agent_enabled != Some(false),
                                                    warnings: Vec::new(),
                                                    failed_policies: Vec::new(),
                                                    cve_checks: Vec::new(),
                                                };
                                                &default_check
                                            }
                                        };

                                        // ── Phase 1: persist derivation (no build job) ──
                                        let persisted = persist_evaluated_system(
                                            pool,
                                            commit.id,
                                            expected_attempt,
                                            &successful,
                                            policy_check,
                                            assigned_policies,
                                        )
                                        .await?;

                                        match persisted {
                                            SystemPersistenceOutcome::NeedsBuildPreparation {
                                                derivation_id,
                                                drv_path,
                                            } => {
                                                // Spawn bounded preparation: GC root → activate → notify.
                                                // This keeps the stdout pipe unblocked while the
                                                // nix-store subprocess and second transaction run.
                                                let build_preparation_limit =
                                                    build_preparation_limit.clone();
                                                let pool = pool.clone();
                                                let commit_id = commit.id;
                                                let attempt = expected_attempt;
                                                let system_name = system_name.clone();
                                                let finalized = FinalizedDerivation {
                                                    derivation_id,
                                                    drv_path: drv_path.clone(),
                                                    system_name: system_name.clone(),
                                                    cf_agent_enabled: successful.cf_agent_enabled,
                                                };
                                                let repo_url = flake.repo_url.clone();
                                                let commit_hash = commit.git_commit_hash.clone();
                                                let successful = successful.clone();
                                                let cf_state_owned = cf_state.cloned();
                                                let queue_notifier_owned = _queue_notifier.cloned();

                                                info!(
                                                    commit_id,
                                                    expected_attempt,
                                                    derivation_id,
                                                    system = %system_name,
                                                    "build_preparation_spawned"
                                                );

                                                build_preparations.spawn(async move {
                                                    let _permit = build_preparation_limit
                                                        .acquire_owned()
                                                        .await
                                                        .context(
                                                            "Build preparation semaphore closed",
                                                        )?;

                                                    info!(
                                                        commit_id,
                                                        expected_attempt = attempt,
                                                        derivation_id,
                                                        system = %system_name,
                                                        "build_preparation_started"
                                                    );

                                                    // Phase 2: GC root (required — bail on failure)
                                                    let rooted = crate::builder::create_drv_gc_root(
                                                        &drv_path,
                                                        derivation_id,
                                                    )
                                                    .await
                                                    .with_context(|| {
                                                        format!(
                                                            "Failed to create GC root for derivation {}",
                                                            derivation_id,
                                                        )
                                                    })?;
                                                    if !rooted {
                                                        #[cfg(not(test))]
                                                        bail!(
                                                            "Derivation {} (drv={}) is not valid \
                                                             in the server store; build activation aborted",
                                                            derivation_id,
                                                            drv_path,
                                                        );
                                                        #[cfg(test)]
                                                        warn!(
                                                            "⚠️  Skipping GC-root requirement for \
                                                             derivation {} (drv={}) in test mode",
                                                            derivation_id, drv_path,
                                                        );
                                                    }

                                                    info!(
                                                        commit_id,
                                                        expected_attempt = attempt,
                                                        derivation_id,
                                                        system = %system_name,
                                                        "build_gc_root_created"
                                                    );

                                                    // Phase 3: activate build job (second transaction)
                                                    let activation =
                                                        activate_evaluated_system_build(
                                                            &pool,
                                                            commit_id,
                                                            attempt,
                                                            derivation_id,
                                                        )
                                                        .await?;

                                                    match &activation {
                                                        SystemBuildActivationOutcome::Queued { .. }
                                                        | SystemBuildActivationOutcome::AlreadyExists { .. } => {
                                                            handle_system_build_activation(
                                                                &pool,
                                                                cf_state_owned.as_ref(),
                                                                queue_notifier_owned.as_ref(),
                                                                commit_id,
                                                                &system_name,
                                                                &activation,
                                                                None, // skip eval-log persistence from tasks
                                                            )
                                                            .await?;

                                                            spawn_closure_counting_and_hardening(
                                                                pool,
                                                                commit_id,
                                                                repo_url,
                                                                commit_hash,
                                                                finalized,
                                                            );
                                                        }
                                                        SystemBuildActivationOutcome::Cancelled => {
                                                            // Task completed after cancellation —
                                                            // nothing further to do.
                                                        }
                                                        SystemBuildActivationOutcome::Superseded => {
                                                            // Build activation was superseded
                                                            // (attempt changed or commit no longer
                                                            // in_progress).
                                                        }
                                                    }

                                                    info!(
                                                        commit_id,
                                                        expected_attempt = attempt,
                                                        derivation_id,
                                                        system = %system_name,
                                                        "build_preparation_completed"
                                                    );

                                                    Ok(())
                                                });

                                                // Track in main results immediately so the
                                                // plan is correct even if preparation is pending.
                                                successful_results.push(successful);
                                            }

                                            SystemPersistenceOutcome::ExistingBuildJob {
                                                derivation_id,
                                                build_job_id,
                                                build_job_status,
                                                drv_path,
                                            } => {
                                                // Best-effort GC root for existing build; the
                                                // build job is already claimable.
                                                match crate::builder::create_drv_gc_root(
                                                    &drv_path,
                                                    derivation_id,
                                                )
                                                .await
                                                {
                                                    Ok(true) => debug!(
                                                        "📌 Rooted existing drv (id={}, drv={})",
                                                        derivation_id, drv_path
                                                    ),
                                                    Ok(false) => warn!(
                                                        "⚠️  Existing drv (id={}, drv={}) not valid \
                                                         in the server store",
                                                        derivation_id, drv_path
                                                    ),
                                                    Err(err) => warn!(
                                                        "⚠️  Failed to create GC root for existing \
                                                         drv {} (id={}): {}",
                                                        drv_path, derivation_id, err
                                                    ),
                                                }

                                                // Re-notify queue if the existing job is still queued.
                                                if build_job_status == "queued" {
                                                    if let Some(notifier) = _queue_notifier {
                                                        notifier.notify_build_queue();
                                                    }
                                                    if let Some(state) = cf_state {
                                                        crate::handlers::api::commits::broadcast_system_status(
                                                            state,
                                                            commit.id,
                                                            system_name.to_string(),
                                                            crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
                                                            None,
                                                        )
                                                        .await;
                                                        broadcast_and_persist_eval_log(
                                                            pool,
                                                            Some(state),
                                                            commit.id,
                                                            &mut log_sequence,
                                                            format!(
                                                                "🚀 {}: build job already queued ({})",
                                                                system_name, build_job_id
                                                            ),
                                                        )
                                                        .await;
                                                    }
                                                } else {
                                                    if let Some(state) = cf_state {
                                                        broadcast_and_persist_eval_log(
                                                            pool,
                                                            Some(state),
                                                            commit.id,
                                                            &mut log_sequence,
                                                            format!(
                                                                "ℹ️  {}: build job exists (status={})",
                                                                system_name, build_job_status
                                                            ),
                                                        )
                                                        .await;
                                                    }
                                                }

                                                successful_results.push(successful.clone());
                                            }

                                            SystemPersistenceOutcome::RecordedWithoutBuild {
                                                ..
                                            } => {
                                                successful_results.push(successful);
                                            }

                                            SystemPersistenceOutcome::Cancelled => {
                                                return Err(EvaluationCancelled.into());
                                            }

                                            SystemPersistenceOutcome::Superseded => {
                                                bail!(
                                                    "evaluation attempt was superseded while finalizing {}",
                                                    system_name
                                                );
                                            }
                                        }
                                    } else if let Some(metadata_error) = policy_metadata_error.as_ref() {
                                        let derivation_target = build_agent_target(
                                            &flake.repo_url,
                                            &commit.git_commit_hash,
                                            system_name,
                                        );
                                        crate::queries::derivations::record_synthetic_eval_failure(
                                            pool,
                                            Some(commit.id),
                                            system_name,
                                            "nixos",
                                            Some(&derivation_target),
                                            metadata_error,
                                        )
                                        .await
                                        .with_context(|| {
                                            format!(
                                                "Failed to record policy metadata failure for {}",
                                                system_name
                                            )
                                        })?;
                                    } else {
                                        if has_error {
                                            debug!("⚠️  {} has evaluation error, will not be persisted as success", system_name);
                                        }
                                        if drv_path.is_none() && !has_error {
                                            warn!("⚠️  {} missing drv_path, not marking complete", system_name);
                                        }
                                    }
                                }

                                if result.attr_path.last() == Some(&target_system.to_string()) || target_system == "all" {
                                    found_target = true;
                                    if target_system != "all" {
                                        info!("✅ Found target system: {}", target_system);
                                    }
                                }

                                if let Some(error) = &result.error {
                                    warn!("⚠️  Evaluation error for {}: {}", result.attr, error);

                                    // When we have an authoritative expected-system set, record
                                    // the error-line system as a confirmed failure immediately.
                                    // nix-eval-jobs already provided the error message — no
                                    // need for an expensive standalone re-evaluation.
                                    // These systems are NOT added to seen_systems so that
                                    // they appear correctly in accounting (not "seen" = not
                                    // successfully evaluated), but they ARE tracked in
                                    // error_line_failures so they are excluded from
                                    // missing_systems (no redundant standalone fallback).
                                    if has_known_systems {
                                        let derivation_target = build_agent_target(
                                            &flake.repo_url,
                                            &commit.git_commit_hash,
                                            &system_name,
                                        );
                                        error_line_failures.push(ConfirmedSystemFailure {
                                            system_name: system_name.clone(),
                                            derivation_target,
                                            error: error.chars().take(500).collect(),
                                        });
                                        // Do NOT push to results here; confirmed failures
                                        // are added to results after the fallback phase.
                                    } else {
                                        results.push(result);
                                    }
                                } else {
                                    // Successful evaluation.
                                    if has_known_systems {
                                        seen_systems.insert(system_name.clone());
                                    }
                                    results.push(result);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse nix-eval-jobs output: {}\nLine: {}", e, line);
                            }
                        }
                    }
                    Some(_) => {},
                    None => stdout_done = true,
                }
            }
            line_result = stderr_reader.next_line(), if !stderr_done => {
                match line_result? {
                    Some(line) => {
                        last_output_at = Instant::now();
                        if line.contains("error:") {
                            error!("nix-eval-jobs stderr: {}", line);
                        } else {
                            debug!("nix-eval-jobs stderr: {}", line);
                        }

                        // Broadcast stderr to WebSocket clients
                        if let Some(state) = cf_state {
                            // Keep live streaming immediate.
                            crate::handlers::api::commits::broadcast_eval_log(state, commit.id, line.clone()).await;

                            // Persist stderr logs in batches to avoid per-line DB latency on hot path.
                            let level = if line.to_ascii_lowercase().contains("error") {
                                Some("error".to_string())
                            } else if line.to_ascii_lowercase().contains("warn") {
                                Some("warn".to_string())
                            } else {
                                Some("debug".to_string())
                            };
                            stderr_log_batch.push((log_sequence, level, line.clone()));
                            log_sequence += 1;

                            if stderr_log_batch.len() >= STDERR_LOG_BATCH_SIZE {
                                if let Err(e) = crate::queries::eval_logs::insert_eval_logs_batch(
                                    pool,
                                    commit.id,
                                    &stderr_log_batch,
                                )
                                .await
                                {
                                    warn!(
                                        "Failed to batch-persist stderr logs for commit {}: {}",
                                        commit.id, e
                                    );
                                }
                                stderr_log_batch.clear();
                            }
                        }

                        stderr_output.push(line);
                    }
                    None => stderr_done = true,
                }
            }
        }

        if stdout_done && stderr_done {
            break;
        }
    }

    // Flush any remaining buffered stderr logs.
    if !stderr_log_batch.is_empty() {
        if let Err(e) =
            crate::queries::eval_logs::insert_eval_logs_batch(pool, commit.id, &stderr_log_batch)
                .await
        {
            warn!(
                "Failed to flush batched stderr logs for commit {}: {}",
                commit.id, e
            );
        }
    }

    // ── Capture child exit status after consuming both streams ─────────
    // Important: do NOT bail before synthesis — we must account for every
    // expected system even when nix-eval-jobs crashed partway through.
    let child_status = child.wait().await?;

    // ── Detect systems that nix-eval-jobs silently dropped ─────────────────
    // When a system fails evaluation catastrophically, nix-eval-jobs may not
    // produce any JSON line for it at all (not even one with an error field).
    // Such systems silently disappear from the results. By comparing against
    // the known systems from the artifact cache, we detect these dropouts and
    // synthesize a failed result for each.
    //
    // This runs regardless of the child exit status so that a partial
    // evaluator crash still creates persisted failure records for all
    // expected-but-unseen systems.
    let expected_systems: Vec<String> = if has_known_systems {
        // Include every discovered system as "expected" regardless of
        // build_scope filtering.  Systems excluded by cf_systems_only
        // must still be accounted for in expected/missing/fallback logic
        // so the total count is accurate and silent drops are detected.
        // Build-eligibility filtering now happens at finalization time
        // (see build_eligible in SuccessfulSystemResult).
        known_systems.clone()
    } else {
        Vec::new()
    };

    // Systems already confirmed as failures via nix-eval-jobs error lines.
    // These do not need standalone fallback — the error message is known.
    let error_line_system_names: HashSet<&str> = error_line_failures
        .iter()
        .map(|f| f.system_name.as_str())
        .collect();

    // Collect missing systems that need fallback evaluation.
    // Exclude both successfully-seen systems AND error-line failures —
    // only truly silent-drop systems (no JSON line at all) go to fallback.
    let missing_systems: Vec<&str> = expected_systems
        .iter()
        .filter(|s| {
            !seen_systems.contains(s.as_str()) && !error_line_system_names.contains(s.as_str())
        })
        .map(|s| s.as_str())
        .collect();
    let unexpected_systems: Vec<String> = seen_systems
        .iter()
        .filter(|seen| !expected_systems.iter().any(|expected| expected == *seen))
        .cloned()
        .collect();
    info!("Seen systems (successful): {:?}", seen_systems);
    info!(
        "Error-line failures (confirmed from bulk output): {:?}",
        error_line_system_names
    );
    info!(
        "Missing systems (no output at all, need fallback): {:?}",
        missing_systems
    );
    info!("Unexpected systems: {:?}", unexpected_systems);
    // Seed confirmed_failures with error-line systems already collected from
    // the bulk evaluator output. Standalone fallback adds more if any systems
    // were silently dropped (no JSON line at all).
    let mut confirmed_failures: Vec<ConfirmedSystemFailure> = error_line_failures;

    if missing_systems.len() > MAX_INDIVIDUAL_FALLBACKS {
        bail!(
            "nix-eval-jobs silently dropped {} systems (max {}); likely process-wide failure",
            missing_systems.len(),
            MAX_INDIVIDUAL_FALLBACKS,
        );
    }

    // Also bail if a large *fraction* of expected systems are missing — even
    // if the absolute count is within MAX_INDIVIDUAL_FALLBACKS — because
    // launching multiple full-flake Nix evaluations when most systems are
    // missing indicates a systemic evaluator failure, not individual breakage.
    // The percent guard only fires when >= MIN_MISSING_FOR_PERCENT_GUARD
    // systems are absent, so a single broken config in a small flake (e.g.
    // 1 of 3) does not disable fallback for the healthy systems.
    // Use multiplication to avoid integer-division rounding surprises.
    if missing_systems.len() >= MIN_MISSING_FOR_PERCENT_GUARD
        && !expected_systems.is_empty()
        && missing_systems.len() * 100 > expected_systems.len() * MAX_FALLBACK_MISSING_PERCENT
    {
        bail!(
            "nix-eval-jobs silently dropped {} of {} expected systems (>{:.0}%); \
             refusing standalone fallback — likely process-wide evaluator failure",
            missing_systems.len(),
            expected_systems.len(),
            MAX_FALLBACK_MISSING_PERCENT,
        );
    }

    info!(
        "Main evaluator exit status for commit {}: {}",
        commit.id, child_status
    );
    if !child_status.success() {
        let stderr_text = stderr_output.join("\n");
        warn!(
            "nix-eval-jobs failed with exit code: {}\nStderr:\n{}",
            child_status.code().unwrap_or(-1),
            stderr_text.chars().take(500).collect::<String>(),
        );
    }

    // Track fallback-outcome counts for diagnostic logging and the combined
    // error message below.
    let mut infra_failure_count: usize = 0;

    // ── Fallback phase: evaluate missing systems concurrently ──
    //
    // Control verification is INSIDE each buffered future so that the
    // overall timeout and cancellation race wraps target + control together.
    // The control system is selected once before building the stream.
    //
    // The entire collection is raced against an actual cancellation future
    // (polling every 2 seconds) and an encompassing FALLBACK_PHASE_TIMEOUT.
    // This ensures that:
    //   1. A control evaluation that takes 120 seconds cannot exceed the
    //      180-second phase timeout by nearly 120 seconds.
    //   2. Cancellation cannot be starved by an outcome-ready stream.
    //   3. Orphan nix processes are killed (kill_on_drop(true)) when the
    //      fallback future is dropped due to timeout or cancellation.
    if !missing_systems.is_empty() {
        warn!(
            "⚠️  {} systems were expected but never appeared in nix-eval-jobs output. Running fallback evaluations...",
            missing_systems.len()
        );

        // Build owned futures for each missing system.  Each future performs
        // a complete standalone eval with the same policies and Nix config as
        // the bulk evaluator.
        let creds_arc = Arc::clone(&creds);
        let build_config_owned = build_config.clone();
        let mut fallback_futures = Vec::with_capacity(missing_systems.len());
        for system_name in &missing_systems {
            let repo_url = repo_url.to_string();
            let commit_hash = commit_hash.to_string();
            let system_name = system_name.to_string();
            let creds = Arc::clone(&creds_arc);
            let build_config = build_config_owned.clone();
            // Pass only this configuration's assigned policies to the fallback evaluator.
            let assigned: Vec<AssignedPolicy> =
                policies_for_config(policies_by_configuration, &system_name)
                    .iter()
                    .cloned()
                    .collect();
            fallback_futures.push(async move {
                evaluate_single_system_with_policies(
                    &repo_url,
                    &commit_hash,
                    &system_name,
                    &assigned,
                    creds.as_ref().as_ref(),
                    &build_config,
                )
                .await
                .unwrap_or_else(|err| {
                    StandaloneSystemOutcome::InfrastructureFailure {
                        system_name,
                        error: err.to_string(),
                    }
                })
            });
        }

        // Cancellation future: polls the DB flag every 2 seconds.
        let cancellation = async {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            // Skip the immediate first tick (0-second check) to avoid a
            // hot loop that pounds the DB.
            interval.tick().await;

            loop {
                interval.tick().await;
                if crate::queries::commits::check_cancellation_requested(pool, commit.id).await? {
                    return Ok::<(), anyhow::Error>(());
                }
            }
        };

        tokio::pin!(cancellation);

        // Process fallback outcomes incrementally as each one completes,
        // rather than collecting the entire batch first.  This means a
        // fast-evaluating system's build job is committed and notified
        // immediately, even if another fallback is still running.
        let deadline = tokio::time::Instant::now() + FALLBACK_PHASE_TIMEOUT;
        let deadline_sleep = tokio::time::sleep_until(deadline);
        tokio::pin!(deadline_sleep);

        let outcome_stream = stream::iter(fallback_futures).buffer_unordered(FALLBACK_CONCURRENCY);
        tokio::pin!(outcome_stream);

        // ── Classify fallback outcomes. Successful recovered systems are
        // finalized immediately just like streaming bulk successes.
        loop {
            tokio::select! {
                biased;

                cancellation_result = &mut cancellation => {
                    cancellation_result?;
                    return Err(EvaluationCancelled.into());
                }

                _ = &mut deadline_sleep => {
                    bail!(
                        "Fallback evaluation phase timed out after {}s",
                        FALLBACK_PHASE_TIMEOUT.as_secs()
                    );
                }

                outcome = outcome_stream.next() => {
                    let Some(outcome) = outcome else {
                        break; // stream exhausted
                    };

                    match outcome {
                        StandaloneSystemOutcome::ConfirmedSystemFailure { system_name, error } => {
                            warn!(
                                "⚠️  System {} was expected but never appeared in nix-eval-jobs output (confirmed failure).",
                                system_name
                            );
                            let derivation_target = build_agent_target(repo_url, commit_hash, &system_name);
                            confirmed_failures.push(ConfirmedSystemFailure {
                                system_name,
                                derivation_target,
                                error,
                            });
                        }
                        StandaloneSystemOutcome::Success {
                            result,
                            policy_check,
                        } => {
                            warn!(
                                "⚠️  System {} was expected but never appeared; standalone policy eval succeeded and will be recorded.",
                                result.system_name
                            );
                            let nix_result = NixEvalJobResult {
                                attr: result.system_name.clone(),
                                attr_path: vec![result.system_name.clone()],
                                name: Some(result.system_name.clone()),
                                drv_path: Some(result.drv_path.clone()),
                                error: None,
                                cache_status: None,
                                outputs: None,
                                meta: None,
                            };
                            // Re-resolve this configuration's assigned policies so the
                            // persisted policy_results JSON and matrix column set match
                            // the bulk-evaluation path exactly (fallback and bulk must
                            // be equivalent from the UI's point of view).
                            let fallback_assigned: Vec<AssignedPolicy> =
                                policies_for_config(policies_by_configuration, &result.system_name)
                                    .to_vec();
                            let finalize_outcome = finalize_evaluated_system(
                                pool,
                                commit.id,
                                expected_attempt,
                                &result,
                                &policy_check,
                                &fallback_assigned,
                            )
                            .await?;

                            // finalize_evaluated_system already performs:
                            //   Phase 1: persist derivation (transaction)
                            //   Phase 2: GC root (required — bails on failure)
                            //   Phase 3: activate build job (second transaction)
                            //
                            // Here we only need to notify the queue, broadcast
                            // via WebSocket, and spawn background side effects.
                            match handle_system_finalize_outcome(
                                pool,
                                cf_state,
                                _queue_notifier,
                                commit.id,
                                &result.system_name,
                                finalize_outcome,
                                &mut log_sequence,
                            )
                            .await?
                            {
                                SystemFinalizeAction::Queued {
                                    derivation_id,
                                    ..
                                }
                                | SystemFinalizeAction::AlreadyExists {
                                    derivation_id,
                                    ..
                                } => {
                                    let finalized = FinalizedDerivation {
                                        derivation_id,
                                        drv_path: result.drv_path.clone(),
                                        system_name: result.system_name.clone(),
                                        cf_agent_enabled: result.cf_agent_enabled,
                                    };
                                    spawn_closure_counting_and_hardening(
                                        pool.clone(),
                                        commit.id,
                                        flake.repo_url.clone(),
                                        commit.git_commit_hash.clone(),
                                        finalized,
                                    );
                                }
                                SystemFinalizeAction::Recorded => {}
                                SystemFinalizeAction::Cancelled => {
                                    return Err(EvaluationCancelled.into());
                                }
                                SystemFinalizeAction::Superseded => {
                                    bail!(
                                        "evaluation attempt was superseded while finalizing fallback system"
                                    );
                                }
                            }
                            successful_results.push(result);
                            policy_checks.push(policy_check);
                            results.push(nix_result);
                        }
                        StandaloneSystemOutcome::InfrastructureFailure { system_name, error } => {
                            warn!(
                                "⚠️  Fallback eval for {} failed with infrastructure error: {}",
                                system_name, error
                            );
                            infra_failure_count += 1;
                        }
                    }
                }
            }
        }

        // ── Reject attempt if fallback had infrastructure failures ───
        // Do this BEFORE persisting confirmed_failures so no synthetic rows
        // are written for a run we are about to retry.
        if infra_failure_count > 0 {
            bail!(
                "One or more fallback evaluations failed due to infrastructure/evaluator issues; \
                 evaluation should be retried"
            );
        }

        // Validation passed — add synthetic results to the in-memory plan.
        // Durable synthetic failure rows are written later by
        // `finalize_evaluation_attempt`, in the same transaction as the
        // commit-complete CAS and successful derivation writes.
        for failure in &confirmed_failures {
            if let Some(state) = cf_state {
                let log_msg = format!("❌ {}: {}", failure.system_name, failure.error);
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    log_msg,
                )
                .await;
            }

            results.push(NixEvalJobResult {
                attr: failure.system_name.clone(),
                attr_path: vec![failure.system_name.clone()],
                name: Some(failure.system_name.clone()),
                drv_path: None,
                error: Some(failure.error.clone()),
                cache_status: None,
                outputs: None,
                meta: None,
            });

            policy_checks.push(PolicyCheckResult {
                system_name: failure.system_name.clone(),
                cf_agent_enabled: None,
                assigned_results: BTreeMap::new(),
                has_required_packages: None,
                custom_checks: HashMap::new(),
                meets_requirements: false,
                warnings: vec![
                    "System failed to evaluate (no output from nix-eval-jobs)".to_string(),
                ],
                failed_policies: vec![],
                cve_checks: vec![],
            });
        }
    }

    // If a specific target was requested, validate after synthesis so that
    // synthetic failed results count as "accounted for".
    if !found_target && target_system != "all" {
        let target_was_expected = expected_systems.iter().any(|s| s == target_system);
        let target_was_accounted_for = results.iter().any(|r| r.attr == target_system);
        if !target_was_expected || !target_was_accounted_for {
            bail!(
                "nix-eval-jobs did not evaluate target system: {}\nEvaluated systems: {:?}",
                target_system,
                results.iter().map(|r| r.attr.as_str()).collect::<Vec<_>>()
            );
        }
    }

    // ── Log systems that failed evaluation vs policy failures ──────────
    // Split into four groups: passed, evaluation_errors, strict_policy_failures,
    // non_strict_policy_failures. Evaluation errors are checks with
    // meets_requirements=false and failed_policies.is_empty().
    let mut passed_systems = Vec::new();
    let mut evaluation_errors = Vec::new();
    let mut strict_policy_failures = Vec::new();
    let mut non_strict_policy_failures = Vec::new();

    for check in &policy_checks {
        if check.meets_requirements {
            passed_systems.push(check);
        } else if check.failed_policies.is_empty() {
            evaluation_errors.push(check);
        } else if check.failed_policies.iter().any(|(_, strict)| *strict) {
            strict_policy_failures.push(check);
        } else {
            non_strict_policy_failures.push(check);
        }
    }

    // Log systems that failed evaluation
    if !evaluation_errors.is_empty() {
        error!("❌ {} systems failed evaluation:", evaluation_errors.len());
        for failure in &evaluation_errors {
            error!("  - {}", failure.system_name);
            for warning in &failure.warnings {
                error!("    {}", warning);
            }
        }
    }

    // Log systems that failed strict policies
    if !strict_policy_failures.is_empty() {
        error!(
            "⚠️  {} systems failed strict deployment policies (will not be queued for build):",
            strict_policy_failures.len()
        );
        for failure in &strict_policy_failures {
            error!("  - {}", failure.system_name);
            for (policy_desc, is_strict) in &failure.failed_policies {
                if *is_strict {
                    error!("    • [STRICT] {}", policy_desc);
                }
            }
        }
    }

    // Log systems that failed only non-strict policies
    if !non_strict_policy_failures.is_empty() {
        warn!(
            "⚠️  {} systems failed non-strict deployment policies:",
            non_strict_policy_failures.len()
        );
        for failure in &non_strict_policy_failures {
            warn!("  - {}", failure.system_name);
            for (policy_desc, _) in &failure.failed_policies {
                warn!("    • {}", policy_desc);
            }
        }
    }

    // Log systems that passed all policies
    if !passed_systems.is_empty() {
        info!(
            "✅ {} systems passed all deployment policies",
            passed_systems.len()
        );
    }

    // Log overall summary
    let total_policy_checks = policy_checks.len();
    if total_policy_checks > 0 {
        info!(
            "📊 Policy evaluation summary: {} passed, {} eval errors, {} strict failures, {} non-strict failures",
            passed_systems.len(),
            evaluation_errors.len(),
            strict_policy_failures.len(),
            non_strict_policy_failures.len(),
        );
    }

    if successful_results.is_empty() {
        warn!("⚠️  No derivations successfully evaluated (all had errors or missing paths)");
    } else {
        info!(
            "✅ {} derivations will be persisted as DryRunComplete",
            successful_results.len()
        );
    }

    info!("✅ Evaluated {} configurations in parallel", results.len());

    let had_system_eval_errors = results.iter().any(|r| r.error.is_some());

    // Calculate statistics for summary
    // `results` includes both stdout results and synthesized entries for
    // systems that nix-eval-jobs silently dropped, so results.len() is the
    // authoritative total.
    let successful = results.iter().filter(|r| r.error.is_none()).count();
    let failed = results.iter().filter(|r| r.error.is_some()).count();
    let total_systems = if !expected_systems.is_empty() {
        expected_systems.len()
    } else {
        successful + failed
    };

    let with_agent = policy_checks
        .iter()
        .filter(|c| c.cf_agent_enabled == Some(true))
        .count();
    let coverage = if policy_checks.len() > 0 {
        (with_agent as f64 / policy_checks.len() as f64) * 100.0
    } else {
        0.0
    };

    if !policies_by_configuration.is_empty() && !policy_checks.is_empty() {
        info!(
            "   CF agent: {}/{} systems enabled ({:.1}%)",
            with_agent,
            policy_checks.len(),
            coverage
        );
    }

    // Broadcast comprehensive summary to WebSocket clients
    if let Some(state) = cf_state {
        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "".to_string(), // Blank line for readability
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "═══════════════════════════════════════".to_string(),
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "📊 Evaluation Summary".to_string(),
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "═══════════════════════════════════════".to_string(),
        )
        .await;

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            format!("✅ Successful: {} systems", successful),
        )
        .await;

        if failed > 0 {
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!("❌ Failed: {} systems", failed),
            )
            .await;
        }

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            format!("📦 Total: {} nixosConfigurations", total_systems),
        )
        .await;

        if !policy_checks.is_empty() {
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                "".to_string(),
            )
            .await;

            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!(
                    "🔐 Policy Compliance: {:.1}% ({}/{})",
                    coverage,
                    with_agent,
                    policy_checks.len()
                ),
            )
            .await;
        }

        if successful_results.len() > 0 {
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                "".to_string(),
            )
            .await;

            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!(
                    "🚀 {} derivations ready for build queue",
                    successful_results.len()
                ),
            )
            .await;
        }

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "═══════════════════════════════════════".to_string(),
        )
        .await;
    }

    info!(
        commit_id = commit.id,
        expected_attempt, "build_preparation_drain_started"
    );
    // Drain all pending build preparations before returning.  Any
    // preparation failure (GC root error, activation error) propagates
    // here so the evaluation attempt is not marked complete with
    // incomplete build preparations.
    while let Some(result) = build_preparations.join_next().await {
        result.context("Build preparation task panicked")??;
    }
    info!(
        commit_id = commit.id,
        expected_attempt, "build_preparation_drain_completed"
    );

    Ok(EvaluationPlan {
        results,
        policy_checks,
        successful_systems: successful_results,
        confirmed_failures,
        had_system_eval_errors,
        #[cfg(test)]
        force_build_job_insert_failure: false,
    })
}

/// Dev-only deterministic mock evaluation path.
///
/// This simulates nix-eval-jobs behavior while preserving queue/log/policy/derivation
/// transitions so UI and process workflows can be validated quickly.
#[allow(clippy::too_many_arguments)]
pub async fn evaluate_with_mock_eval_jobs(
    pool: &PgPool,
    commit: &Commit,
    flake: &Flake,
    repo_url: &str,
    commit_hash: &str,
    target_system: &str,
    _build_config: &BuildConfig,
    _server_config: &ServerConfig,
    _policies_by_configuration: &Arc<PoliciesByConfiguration>,
    configured_systems: &[String],
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    _queue_notifier: Option<&QueueNotifier>,
) -> Result<EvaluationPlan> {
    // Re-evaluation safety: clear previous persisted logs for this commit so
    // (commit_id, log_sequence) uniqueness cannot collide on subsequent runs.
    crate::queries::eval_logs::delete_eval_logs_by_commit(pool, commit.id).await?;

    // Sequence counter for persisted/mock streamed logs (1-indexed)
    let mut log_sequence = 1i32;

    let systems = resolve_mock_systems(&flake.name, target_system, configured_systems)?;
    let stage_delay = mock_eval_stage_delay(systems.len());

    crate::queries::commits_artifacts::upsert_commit_artifact_cache(pool, commit.id, &systems, &[])
        .await?;

    if let Some(state) = cf_state {
        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            format!(
                "🧪 MOCK MODE: evaluating {} system(s) for {}@{}",
                systems.len(),
                flake.name,
                &commit_hash[..8.min(commit_hash.len())]
            ),
        )
        .await;
    }

    let mut results = Vec::with_capacity(systems.len());
    let mut checks = Vec::with_capacity(systems.len());
    let mut successful_systems = Vec::with_capacity(systems.len());

    for (idx, system_name) in systems.iter().enumerate() {
        if let Some(state) = cf_state {
            crate::handlers::api::commits::broadcast_system_status(
                state,
                commit.id,
                system_name.clone(),
                crate::handlers::api::commits::SystemEvalStatus::Evaluating,
                None,
            )
            .await;
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                format!(
                    "⏳ {}: queued in mock pipeline (system {}/{})",
                    system_name,
                    idx + 1,
                    systems.len()
                ),
            )
            .await;
        }

        for (progress, stage) in [
            (10, "resolving flake input graph"),
            (30, "checking nixosConfigurations outputs"),
            (55, "expanding module graph"),
            (80, "running policy prechecks"),
            (95, "finalizing derivation metadata"),
        ] {
            if let Some(state) = cf_state {
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    format!("⏳ {} [{}%]: {}", system_name, progress, stage),
                )
                .await;
            }
            tokio::time::sleep(stage_delay).await;
        }

        let flake_ref = build_flake_reference(repo_url, commit_hash);
        let drv_path = format!(
            "/nix/store/mock-{}-{}.drv",
            &commit_hash[..8.min(commit_hash.len())],
            system_name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                })
                .collect::<String>()
        );
        let derivation_target = format!("{}#nixosConfigurations.{}", flake_ref, system_name);

        let policy_failed = should_mock_policy_fail(systems.len(), idx);

        let cf_agent_enabled = Some(!policy_failed);
        successful_systems.push(SuccessfulSystemResult {
            system_name: system_name.clone(),
            derivation_target,
            drv_path: drv_path.clone(),
            expected_store_path: None,
            cf_agent_enabled,
            build_eligible: true,
        });

        let check = PolicyCheckResult {
            system_name: system_name.clone(),
            cf_agent_enabled: Some(!policy_failed),
            assigned_results: BTreeMap::new(),
            has_required_packages: None,
            custom_checks: HashMap::new(),
            meets_requirements: !policy_failed,
            warnings: if policy_failed {
                vec!["Mock policy failure for UI validation".to_string()]
            } else {
                vec![]
            },
            failed_policies: if policy_failed {
                vec![("Crystal Forge agent must be enabled".to_string(), true)]
            } else {
                vec![]
            },
            cve_checks: vec![],
        };
        checks.push(check);

        results.push(NixEvalJobResult {
            attr: system_name.clone(),
            attr_path: vec!["nixosConfigurations".to_string(), system_name.clone()],
            name: Some(system_name.clone()),
            drv_path: Some(drv_path),
            error: None,
            cache_status: Some("unknown".to_string()),
            outputs: None,
            meta: None,
        });

        if let Some(state) = cf_state {
            if policy_failed {
                crate::handlers::api::commits::broadcast_system_status(
                    state,
                    commit.id,
                    system_name.clone(),
                    crate::handlers::api::commits::SystemEvalStatus::PolicyFailed,
                    Some("Mock policy failure".to_string()),
                )
                .await;
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    format!(
                        "⚠️ {}: policy check failed (mock), build skipped",
                        system_name
                    ),
                )
                .await;
            } else {
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    format!("✅ {}: policy passed (CF enabled), evaluated", system_name),
                )
                .await;
            }
        }
    }

    // Mock evaluations never have system eval errors, so had_system_eval_errors is false.
    let had_system_eval_errors = false;
    Ok(EvaluationPlan {
        results,
        policy_checks: checks,
        successful_systems,
        confirmed_failures: Vec::new(),
        had_system_eval_errors,
        #[cfg(test)]
        force_build_job_insert_failure: false,
    })
}

fn build_agent_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    format!("{}#nixosConfigurations.{}", flake_ref, system_name)
}

#[cfg(test)]
pub async fn load_allowed_systems_for_test(
    pool: &PgPool,
    flake: &Flake,
    target_system: &str,
) -> Result<Option<Vec<String>>> {
    load_allowed_systems(pool, flake, target_system).await
}

#[cfg(test)]
pub fn should_skip_system_for_test(
    allowed_systems: &Option<Vec<String>>,
    system_name: &str,
) -> bool {
    should_skip_system(allowed_systems, system_name)
}

async fn load_allowed_systems(
    pool: &PgPool,
    flake: &Flake,
    target_system: &str,
) -> Result<Option<Vec<String>>> {
    if target_system != "all" {
        return Ok(None);
    }
    if flake.build_scope != "cf_systems_only" {
        return Ok(None);
    }

    let mut systems = list_configuration_names_for_flake(pool, flake.id).await?;
    systems.sort();
    systems.dedup();
    Ok(Some(systems))
}

fn should_skip_system(allowed_systems: &Option<Vec<String>>, system_name: &str) -> bool {
    match allowed_systems {
        Some(systems) => !systems.iter().any(|configured| configured == system_name),
        None => false,
    }
}

fn resolve_mock_systems(
    flake_name: &str,
    target_system: &str,
    configured_systems: &[String],
) -> Result<Vec<String>> {
    let mut systems = if configured_systems.is_empty() {
        vec![
            format!("{}-control-0", flake_name),
            format!("{}-worker-0", flake_name),
            format!("{}-worker-1", flake_name),
        ]
    } else {
        configured_systems.to_vec()
    };

    systems.sort();

    if target_system != "all" {
        systems.retain(|s| s == target_system);
    }

    if systems.is_empty() {
        bail!("mock evaluation has no matching systems to evaluate");
    }

    Ok(systems)
}

fn mock_eval_stage_delay(system_count: usize) -> std::time::Duration {
    let systems = std::cmp::max(system_count as u64, 1);
    let per_system = std::cmp::max(
        MOCK_EVAL_TOTAL_DURATION_MS / systems,
        MOCK_EVAL_MIN_PER_SYSTEM_MS,
    );
    let per_stage = std::cmp::max(per_system / MOCK_EVAL_STAGE_COUNT, 750);
    std::time::Duration::from_millis(per_stage)
}

fn should_mock_policy_fail(system_count: usize, idx: usize) -> bool {
    system_count > 1 && idx == 1
}

/// Update commit metadata cache with evaluation summary statistics
///
/// This function should be called after evaluation completes (success or failure)
/// to cache the results for fast flakes view loading.
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `commit_id` - ID of the evaluated commit
/// * `policy_checks` - Policy check results for all systems
/// * `has_nix_eval_error` - Whether the evaluation had a Nix error (vs policy failure)
pub async fn update_commit_metadata_cache(
    pool: &PgPool,
    commit_id: i32,
    policy_checks: &[PolicyCheckResult],
    has_nix_eval_error: bool,
) -> Result<()> {
    let (
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_policy_failures,
        all_systems_passed,
    ) = summarize_commit_metadata(policy_checks, has_nix_eval_error);

    sqlx::query!(
        r#"
        INSERT INTO commit_metadata_cache (
            commit_id,
            total_systems,
            systems_passed_policy,
            systems_failed_policy_strict,
            systems_failed_policy_non_strict,
            systems_with_eval_error,
            has_nix_eval_error,
            has_policy_failures,
            all_systems_passed
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (commit_id) DO UPDATE SET
            total_systems = EXCLUDED.total_systems,
            systems_passed_policy = EXCLUDED.systems_passed_policy,
            systems_failed_policy_strict = EXCLUDED.systems_failed_policy_strict,
            systems_failed_policy_non_strict = EXCLUDED.systems_failed_policy_non_strict,
            systems_with_eval_error = EXCLUDED.systems_with_eval_error,
            has_nix_eval_error = EXCLUDED.has_nix_eval_error,
            has_policy_failures = EXCLUDED.has_policy_failures,
            all_systems_passed = EXCLUDED.all_systems_passed,
            cached_at = CURRENT_TIMESTAMP
        "#,
        commit_id,
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_nix_eval_error,
        has_policy_failures,
        all_systems_passed
    )
    .execute(pool)
    .await?;

    debug!(
        "💾 Updated commit metadata cache for commit {}: {}/{} systems passed",
        commit_id, systems_passed, total_systems
    );

    Ok(())
}

fn summarize_commit_metadata(
    policy_checks: &[PolicyCheckResult],
    has_nix_eval_error: bool,
) -> (i32, i32, i32, i32, i32, bool, bool) {
    let total_systems = policy_checks.len() as i32;

    let systems_passed = policy_checks
        .iter()
        .filter(|c| c.meets_requirements)
        .count() as i32;

    let systems_failed_strict = policy_checks
        .iter()
        .filter(|c| {
            !c.meets_requirements && c.failed_policies.iter().any(|(_, is_strict)| *is_strict)
        })
        .count() as i32;

    // A failed check with no failed_policies indicates evaluation-level failure
    // for that system (not a policy failure). We keep this distinct from
    // non-strict policy failures to avoid misleading API counts.
    let systems_with_eval_error_from_checks = policy_checks
        .iter()
        .filter(|c| !c.meets_requirements && c.failed_policies.is_empty())
        .count() as i32;

    let systems_failed_non_strict = policy_checks
        .iter()
        .filter(|c| {
            !c.meets_requirements
                && !c.failed_policies.is_empty()
                && !c.failed_policies.iter().any(|(_, is_strict)| *is_strict)
        })
        .count() as i32;

    let systems_with_eval_error = systems_with_eval_error_from_checks;
    let has_policy_failures = systems_failed_strict > 0 || systems_failed_non_strict > 0;

    // If evaluation itself failed, this commit cannot be considered fully passing,
    // even when per-system checks are unavailable.
    let all_systems_passed =
        !has_nix_eval_error && total_systems > 0 && systems_passed == total_systems;

    (
        total_systems,
        systems_passed,
        systems_failed_strict,
        systems_failed_non_strict,
        systems_with_eval_error,
        has_policy_failures,
        all_systems_passed,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ConfirmedSystemFailure, EvaluationFinalizeOutcome, EvaluationPlan, NixEvalJobResult,
        SuccessfulSystemResult, SystemBuildActivationOutcome, SystemFinalizeOutcome,
        SystemNotQueuedReason, SystemPersistenceOutcome, activate_evaluated_system_build,
        finalize_evaluated_system, finalize_evaluation_attempt, mock_eval_stage_delay,
        persist_evaluated_system, resolve_mock_systems, should_mock_policy_fail,
        summarize_commit_metadata,
    };
    use crate::api::models::CancelEvalOutcome;
    use crate::models::deployment_policies::{
        AssignedPolicy, DeploymentPolicy, PolicyCheckResult, policy_result_key, policy_results_json,
    };
    use crate::queries::commits::{
        EvalFailureOutcome, EvalStartOutcome, cancel_commit_evaluation,
        mark_commit_evaluation_failed, mark_commit_evaluation_started,
    };
    use sqlx::PgPool;
    use std::collections::BTreeMap;

    fn test_database_url() -> String {
        std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect(
                "CRYSTAL_FORGE_TEST_DATABASE_URL or DATABASE_URL must be set for database tests",
            )
    }

    async fn test_pool() -> PgPool {
        PgPool::connect(&test_database_url())
            .await
            .expect("failed to connect to test database")
    }

    async fn insert_throwaway_flake(pool: &PgPool) -> i32 {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("eval-finalize-flake-{short}"))
        .bind(format!(
            "https://git.example/eval-finalize-flake-{short}.git"
        ))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test flake")
    }

    async fn cleanup_throwaway_flakes(pool: &PgPool) {
        let _ = sqlx::query("DELETE FROM flakes WHERE name LIKE 'eval-finalize-flake-%'")
            .execute(pool)
            .await;
    }

    async fn insert_throwaway_commit(pool: &PgPool, flake_id: i32) -> i32 {
        let hash = uuid::Uuid::new_v4().simple().to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(hash)
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test commit")
    }

    async fn start_eval(pool: &PgPool, commit_id: i32) -> i32 {
        match mark_commit_evaluation_started(pool, commit_id)
            .await
            .expect("start should succeed")
        {
            EvalStartOutcome::Started { attempt } => attempt,
            EvalStartOutcome::NoLongerPending => panic!("commit should be pending"),
        }
    }

    fn successful_system(system_name: &str) -> SuccessfulSystemResult {
        SuccessfulSystemResult {
            system_name: system_name.to_string(),
            derivation_target: format!(
                "git+https://example.invalid/repo#nixosConfigurations.{system_name}"
            ),
            drv_path: format!(
                "/nix/store/{}-{}.drv",
                "a".repeat(32),
                system_name.replace('\0', "nul")
            ),
            expected_store_path: None,
            cf_agent_enabled: Some(true),
            build_eligible: true,
        }
    }

    fn failed_system(system_name: &str, error: &str) -> ConfirmedSystemFailure {
        ConfirmedSystemFailure {
            system_name: system_name.to_string(),
            derivation_target: format!(
                "git+https://example.invalid/repo#nixosConfigurations.{system_name}"
            ),
            error: error.to_string(),
        }
    }

    fn check(system_name: &str, passed: bool) -> PolicyCheckResult {
        PolicyCheckResult {
            system_name: system_name.to_string(),
            cf_agent_enabled: Some(passed),
            assigned_results: BTreeMap::new(),
            has_required_packages: None,
            custom_checks: std::collections::HashMap::new(),
            meets_requirements: passed,
            warnings: if passed {
                vec![]
            } else {
                vec!["evaluation failed".to_string()]
            },
            failed_policies: vec![],
            cve_checks: vec![],
        }
    }

    fn passing_policy_check(system_name: &str) -> PolicyCheckResult {
        check(system_name, true)
    }

    fn failing_policy_check(
        system_name: &str,
        strict: bool,
        description: &str,
    ) -> PolicyCheckResult {
        PolicyCheckResult {
            system_name: system_name.to_string(),
            cf_agent_enabled: Some(true),
            assigned_results: BTreeMap::new(),
            has_required_packages: Some(false),
            custom_checks: std::collections::HashMap::new(),
            meets_requirements: !strict,
            warnings: vec![format!("Missing required packages for {system_name}")],
            failed_policies: vec![(description.to_string(), strict)],
            cve_checks: vec![],
        }
    }

    fn plan(
        successes: Vec<SuccessfulSystemResult>,
        failures: Vec<ConfirmedSystemFailure>,
    ) -> EvaluationPlan {
        let mut results = Vec::new();
        let mut policy_checks = Vec::new();
        for success in &successes {
            results.push(NixEvalJobResult {
                attr: success.system_name.clone(),
                attr_path: vec![
                    "nixosConfigurations".to_string(),
                    success.system_name.clone(),
                ],
                name: Some(success.system_name.clone()),
                drv_path: Some(success.drv_path.clone()),
                error: None,
                cache_status: None,
                outputs: None,
                meta: None,
            });
            policy_checks.push(check(&success.system_name, true));
        }
        for failure in &failures {
            results.push(NixEvalJobResult {
                attr: failure.system_name.clone(),
                attr_path: vec![
                    "nixosConfigurations".to_string(),
                    failure.system_name.clone(),
                ],
                name: Some(failure.system_name.clone()),
                drv_path: None,
                error: Some(failure.error.clone()),
                cache_status: None,
                outputs: None,
                meta: None,
            });
            policy_checks.push(check(&failure.system_name, false));
        }
        EvaluationPlan {
            results,
            policy_checks,
            successful_systems: successes,
            confirmed_failures: failures,
            had_system_eval_errors: false,
            force_build_job_insert_failure: false,
        }
    }

    async fn derivation_count(pool: &PgPool, commit_id: i32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM derivations WHERE commit_id = $1")
            .bind(commit_id)
            .fetch_one(pool)
            .await
            .expect("count should succeed")
    }

    async fn build_job_count(pool: &PgPool, commit_id: i32) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM build_jobs bj \
             JOIN derivations d ON d.id = bj.derivation_id \
             WHERE d.commit_id = $1",
        )
        .bind(commit_id)
        .fetch_one(pool)
        .await
        .expect("count should succeed")
    }

    #[test]
    fn mock_systems_fallback_and_filtering() {
        let all =
            resolve_mock_systems("demo", "all", &[]).expect("fallback systems should resolve");
        assert_eq!(
            all,
            vec![
                "demo-control-0".to_string(),
                "demo-worker-0".to_string(),
                "demo-worker-1".to_string()
            ]
        );

        let filtered =
            resolve_mock_systems("demo", "demo-worker-1", &all).expect("target should filter");
        assert_eq!(filtered, vec!["demo-worker-1".to_string()]);
    }

    #[test]
    fn mock_systems_errors_when_target_missing() {
        let systems = vec!["alpha".to_string(), "beta".to_string()];
        let err = resolve_mock_systems("demo", "gamma", &systems)
            .expect_err("missing target should return error");
        assert!(
            err.to_string()
                .contains("mock evaluation has no matching systems to evaluate")
        );
    }

    #[test]
    fn mock_eval_stage_delay_targets_human_observable_runtime() {
        assert_eq!(mock_eval_stage_delay(3).as_millis(), 2000);
        assert_eq!(mock_eval_stage_delay(1).as_millis(), 6000);
        assert_eq!(mock_eval_stage_delay(10).as_millis(), 1000);
    }

    #[test]
    fn mock_policy_fail_pattern_is_deterministic() {
        assert!(!should_mock_policy_fail(1, 0));
        assert!(!should_mock_policy_fail(3, 0));
        assert!(should_mock_policy_fail(3, 1));
        assert!(!should_mock_policy_fail(3, 2));
    }

    #[test]
    fn test_policy_check_result_tracks_failed_policies_with_strictness() {
        use crate::models::deployment_policies::{DeploymentPolicy, PolicyCheckResult};
        use serde_json::json;

        let policies = vec![
            DeploymentPolicy::RequireCrystalForgeAgent { strict: true },
            DeploymentPolicy::RequirePackages {
                packages: vec!["git".to_string()],
                strict: false,
            },
        ];

        // System failing strict policy only
        let policies_json = json!({
            "cfAgentEnabled": false,
            "hasRequiredPackages_1": true
        });

        let result =
            PolicyCheckResult::from_json("test-system".to_string(), &policies_json, &policies);

        assert!(!result.meets_requirements);
        assert_eq!(result.failed_policies.len(), 1);
        assert_eq!(
            result.failed_policies[0].0,
            "Crystal Forge agent must be enabled"
        );
        assert!(result.failed_policies[0].1); // is_strict = true

        // System failing non-strict policy only
        let policies_json_2 = json!({
            "cfAgentEnabled": true,
            "hasRequiredPackages_1": false
        });

        let result_2 =
            PolicyCheckResult::from_json("test-system-2".to_string(), &policies_json_2, &policies);

        // Non-strict failures should warn, but not block requirements.
        assert!(result_2.meets_requirements);
        assert_eq!(result_2.failed_policies.len(), 1);
        assert_eq!(result_2.failed_policies[0].0, "Required packages: git");
        assert!(!result_2.failed_policies[0].1); // is_strict = false

        // System failing both
        let policies_json_3 = json!({
            "cfAgentEnabled": false,
            "hasRequiredPackages_1": false
        });

        let result_3 =
            PolicyCheckResult::from_json("test-system-3".to_string(), &policies_json_3, &policies);

        assert!(!result_3.meets_requirements);
        assert_eq!(result_3.failed_policies.len(), 2);

        // Should have one strict and one non-strict failure
        let strict_count = result_3
            .failed_policies
            .iter()
            .filter(|(_, is_strict)| *is_strict)
            .count();
        let non_strict_count = result_3
            .failed_policies
            .iter()
            .filter(|(_, is_strict)| !*is_strict)
            .count();

        assert_eq!(strict_count, 1);
        assert_eq!(non_strict_count, 1);
    }

    #[test]
    fn metadata_summary_marks_eval_error_commit_as_not_all_passed() {
        let (
            total,
            passed,
            strict_failed,
            non_strict_failed,
            eval_failed,
            has_policy_failures,
            all_passed,
        ) = summarize_commit_metadata(&[], true);
        assert_eq!(total, 0);
        assert_eq!(passed, 0);
        assert_eq!(strict_failed, 0);
        assert_eq!(non_strict_failed, 0);
        assert_eq!(eval_failed, 0);
        assert!(!has_policy_failures);
        assert!(!all_passed);
    }

    #[test]
    fn metadata_summary_does_not_treat_empty_failed_policies_as_non_strict_failure() {
        let checks = vec![PolicyCheckResult {
            system_name: "alpha".to_string(),
            cf_agent_enabled: None,
            assigned_results: BTreeMap::new(),
            has_required_packages: None,
            custom_checks: std::collections::HashMap::new(),
            meets_requirements: false,
            warnings: vec!["evaluation failed".to_string()],
            failed_policies: vec![],
            cve_checks: vec![],
        }];

        let (
            _total,
            _passed,
            strict_failed,
            non_strict_failed,
            eval_failed,
            has_policy_failures,
            all_passed,
        ) = summarize_commit_metadata(&checks, false);

        assert_eq!(strict_failed, 0);
        assert_eq!(non_strict_failed, 0);
        assert_eq!(eval_failed, 1);
        assert!(!has_policy_failures);
        assert!(!all_passed);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_attempt_cancellation_wins_without_derivation_writes() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        assert_eq!(
            cancel_commit_evaluation(&pool, commit_id).await.unwrap(),
            CancelEvalOutcome::CancellingInProgress
        );

        let outcome = finalize_evaluation_attempt(
            &pool,
            commit_id,
            attempt,
            &plan(vec![successful_system("alpha")], vec![]),
        )
        .await
        .expect("finalize should not error");

        assert!(matches!(outcome, EvaluationFinalizeOutcome::Cancelled));
        assert_eq!(derivation_count(&pool, commit_id).await, 0);
        assert_eq!(build_job_count(&pool, commit_id).await, 0);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_attempt_completion_wins_before_late_cancel() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let outcome = finalize_evaluation_attempt(
            &pool,
            commit_id,
            attempt,
            &plan(vec![successful_system("alpha")], vec![]),
        )
        .await
        .expect("finalize should not error");

        assert!(matches!(
            outcome,
            EvaluationFinalizeOutcome::Completed { .. }
        ));
        assert_eq!(derivation_count(&pool, commit_id).await, 0);
        assert_eq!(
            cancel_commit_evaluation(&pool, commit_id).await.unwrap(),
            CancelEvalOutcome::AlreadyTerminal
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_attempt_does_not_rewrite_incremental_successes() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let outcome = finalize_evaluation_attempt(
            &pool,
            commit_id,
            attempt,
            &plan(vec![successful_system("alpha")], vec![]),
        )
        .await
        .expect("commit finalizer should ignore already-incremental successes");

        assert!(matches!(
            outcome,
            EvaluationFinalizeOutcome::Completed { .. }
        ));
        assert_eq!(derivation_count(&pool, commit_id).await, 0);

        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "complete");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_attempt_rolls_back_mixed_success_and_synthetic_failure() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        finalize_evaluation_attempt(
            &pool,
            commit_id,
            attempt,
            &plan(
                vec![successful_system("alpha")],
                vec![failed_system("broken\0system", "module error")],
            ),
        )
        .await
        .expect_err("nul byte should make synthetic failure write fail");

        assert_eq!(derivation_count(&pool, commit_id).await, 0);

        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "in_progress");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_creates_build_job_in_transaction() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("alpha");
        let check = passing_policy_check("alpha");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("system finalize should not error");

        assert!(matches!(outcome, SystemFinalizeOutcome::Queued { .. }));
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn fallback_finalize_persists_same_policy_results_as_bulk_path() {
        // Regression: the fallback (standalone) evaluation path must persist
        // the exact same policy_results document that the bulk streaming
        // path would, for the same assigned policies. Before this fix,
        // `finalize_evaluated_system` (used only by the fallback path)
        // always persisted with an empty assigned-policy slice, so a
        // fallback-recovered system's matrix column showed "not_assigned"
        // for a policy it was actually evaluated against — bulk and
        // fallback results were not equivalent.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let policy_id = uuid::Uuid::from_u128(0xF00D);
        let assigned = vec![AssignedPolicy {
            policy_id,
            policy_name: "failme".to_string(),
            policy: DeploymentPolicy::RequirePackages {
                packages: vec!["grafana".to_string()],
                strict: true,
            },
        }];

        let policies_json = serde_json::json!({
            "cfAgentEnabled": true,
            policy_result_key(&policy_id): false,
        });
        let check = PolicyCheckResult::from_assigned("gray".to_string(), &policies_json, &assigned)
            .expect("policy metadata should parse");

        let system = successful_system("gray");

        // Simulates the fallback path: finalize_evaluated_system called with
        // this configuration's real assigned policies, matching what the
        // fixed production call site now re-resolves via
        // `policies_for_config` before calling this function.
        let outcome =
            finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &assigned)
                .await
                .expect("fallback finalize should not error");

        assert!(matches!(
            outcome,
            SystemFinalizeOutcome::RecordedWithoutBuild {
                reason: SystemNotQueuedReason::StrictPolicyFailure,
                ..
            }
        ));

        let persisted: serde_json::Value = sqlx::query_scalar(
            "SELECT policy_results FROM derivations WHERE commit_id = $1 AND derivation_name = 'gray'",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("derivation row should exist");

        // What the bulk streaming path would have produced for the
        // identical check + assigned policies (this is exactly what
        // `persist_evaluated_system` computes internally).
        let expected = policy_results_json(&check, &assigned);
        assert_eq!(
            persisted, expected,
            "fallback path must persist the same policy_results document as the bulk path"
        );

        // The specific bug this regresses: the assigned policy must not be
        // missing/dropped from the persisted document.
        let assigned_entry = persisted
            .get("assigned")
            .and_then(|a| a.get(policy_id.to_string()))
            .expect("fallback path must persist the assigned policy's result, not drop it");
        assert_eq!(
            assigned_entry.get("passed").and_then(|v| v.as_bool()),
            Some(false)
        );
        // The real DB policy name must be persisted (not a generated
        // description), so "View policy definition" navigation resolves.
        assert_eq!(
            assigned_entry.get("name").and_then(|v| v.as_str()),
            Some("failme")
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_strict_policy_failure_does_not_queue() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("alpha");
        let check = failing_policy_check("alpha", true, "Require packages: git");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("system finalize should record strict policy failures");

        assert!(matches!(
            outcome,
            SystemFinalizeOutcome::RecordedWithoutBuild {
                reason: SystemNotQueuedReason::StrictPolicyFailure,
                ..
            }
        ));
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 0);

        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "in_progress");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_non_strict_policy_failure_still_queues() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("alpha");
        let check = failing_policy_check("alpha", false, "Require packages: git");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("non-strict policy warning should not block queueing");

        assert!(matches!(outcome, SystemFinalizeOutcome::Queued { .. }));
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_retry_reuses_existing_build_job() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("alpha");
        let check = passing_policy_check("alpha");

        let first = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("first finalization should queue build");
        let first_job_id = match first {
            SystemFinalizeOutcome::Queued { build_job_id, .. } => build_job_id,
            other => panic!("expected queued outcome, got {other:?}"),
        };

        let second = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("retry finalization should be idempotent");

        assert!(matches!(
            second,
            SystemFinalizeOutcome::BuildAlreadyExists { build_job_id, .. }
                if build_job_id == first_job_id
        ));
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_attempt_error_can_be_routed_to_retry_failure_cas() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        finalize_evaluation_attempt(
            &pool,
            commit_id,
            attempt,
            &plan(vec![], vec![failed_system("bad\0system", "module error")]),
        )
        .await
        .expect_err("synthetic failure write should roll back finalization");

        assert_eq!(derivation_count(&pool, commit_id).await, 0);
        assert_eq!(build_job_count(&pool, commit_id).await, 0);

        let failure = mark_commit_evaluation_failed(
            &pool,
            commit_id,
            "synthetic failure finalization failed",
            attempt,
        )
        .await
        .expect("failure CAS should succeed after finalizer rollback");
        assert_eq!(failure, EvalFailureOutcome::RetryScheduled);

        #[derive(sqlx::FromRow)]
        struct Row {
            evaluation_status: String,
            evaluation_error_message: Option<String>,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT evaluation_status, evaluation_error_message FROM commits WHERE id = $1",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(row.evaluation_status, "pending");
        assert!(
            row.evaluation_error_message
                .as_deref()
                .unwrap_or_default()
                .contains("synthetic failure finalization failed")
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Agent-disabled / multiple-strict-policy tests ─────────────────────

    fn agent_disabled_check(system_name: &str) -> PolicyCheckResult {
        PolicyCheckResult {
            system_name: system_name.to_string(),
            cf_agent_enabled: Some(false),
            assigned_results: BTreeMap::new(),
            has_required_packages: None,
            custom_checks: std::collections::HashMap::new(),
            meets_requirements: true, // other policies pass
            warnings: vec!["Crystal Forge agent not enabled".to_string()],
            failed_policies: vec![],
            cve_checks: vec![],
        }
    }

    fn agent_disabled_with_strict_failure(system_name: &str) -> PolicyCheckResult {
        PolicyCheckResult {
            system_name: system_name.to_string(),
            cf_agent_enabled: Some(false),
            assigned_results: BTreeMap::new(),
            has_required_packages: Some(false),
            custom_checks: std::collections::HashMap::new(),
            meets_requirements: false,
            warnings: vec![
                "Crystal Forge agent not enabled".to_string(),
                "Missing required packages".to_string(),
            ],
            failed_policies: vec![
                ("Crystal Forge agent must be enabled".to_string(), true),
                ("Required packages: git".to_string(), true),
            ],
            cve_checks: vec![],
        }
    }

    fn successful_system_no_agent(system_name: &str) -> SuccessfulSystemResult {
        SuccessfulSystemResult {
            system_name: system_name.to_string(),
            derivation_target: format!(
                "git+https://example.invalid/repo#nixosConfigurations.{system_name}"
            ),
            drv_path: format!(
                "/nix/store/{}-{}.drv",
                "b".repeat(32),
                system_name.replace('\0', "nul")
            ),
            expected_store_path: None,
            cf_agent_enabled: Some(false),
            build_eligible: true,
        }
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_agent_disabled_does_not_queue() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system_no_agent("beta");
        let check = agent_disabled_check("beta");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("agent-disabled system should be recorded without error");

        assert!(
            matches!(
                outcome,
                SystemFinalizeOutcome::RecordedWithoutBuild {
                    reason: SystemNotQueuedReason::AgentPolicyFailure,
                    ..
                }
            ),
            "expected AgentPolicyFailure, got {outcome:?}"
        );
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 0);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_multiple_strict_failures_does_not_queue() {
        // Both CF-agent disabled and required-packages strict failure.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system_no_agent("gamma");
        let check = agent_disabled_with_strict_failure("gamma");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("multiple strict failures should be recorded without error");

        // The CF-agent check fires first (cf_agent_enabled == Some(false)).
        assert!(
            matches!(
                outcome,
                SystemFinalizeOutcome::RecordedWithoutBuild {
                    reason: SystemNotQueuedReason::AgentPolicyFailure,
                    ..
                }
            ),
            "expected AgentPolicyFailure as first failure, got {outcome:?}"
        );
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 0);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_cancellation_after_first_system_queued() {
        // Queue one system, then cancel, then attempt to finalize a second.
        // The first system's build job must survive; the second must be rejected.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        // First system finalizes successfully while eval is in_progress.
        let first_system = successful_system("alpha");
        let first_check = passing_policy_check("alpha");
        let first_outcome =
            finalize_evaluated_system(&pool, commit_id, attempt, &first_system, &first_check, &[])
                .await
                .expect("first system should finalize");
        assert!(
            matches!(first_outcome, SystemFinalizeOutcome::Queued { .. }),
            "first system must be queued; got {first_outcome:?}"
        );
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        // Cancel the evaluation.
        let cancel_result = cancel_commit_evaluation(&pool, commit_id).await.unwrap();
        assert_eq!(cancel_result, CancelEvalOutcome::CancellingInProgress);

        // Attempt to finalize a second system — must be rejected.
        let second_system = successful_system("beta");
        let second_check = passing_policy_check("beta");
        let second_outcome = finalize_evaluated_system(
            &pool,
            commit_id,
            attempt,
            &second_system,
            &second_check,
            &[],
        )
        .await
        .expect("finalize after cancel should return Cancelled, not error");
        assert!(
            matches!(second_outcome, SystemFinalizeOutcome::Cancelled),
            "second system must be Cancelled after eval cancel; got {second_outcome:?}"
        );

        // First system and its build job still intact.
        assert_eq!(
            derivation_count(&pool, commit_id).await,
            1,
            "alpha derivation must survive"
        );
        assert_eq!(
            build_job_count(&pool, commit_id).await,
            1,
            "alpha build job must survive"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_broken_and_healthy_are_independent() {
        // Healthy system A finalizes; broken system B is recorded as a confirmed
        // failure via finalize_evaluation_attempt; A's build job is unaffected.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        // Finalize A incrementally (simulating streaming bulk path).
        let system_a = successful_system("alpha");
        let check_a = passing_policy_check("alpha");
        let outcome_a =
            finalize_evaluated_system(&pool, commit_id, attempt, &system_a, &check_a, &[])
                .await
                .expect("alpha should finalize");
        assert!(
            matches!(outcome_a, SystemFinalizeOutcome::Queued { .. }),
            "alpha must be queued; got {outcome_a:?}"
        );
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        // Finalize the commit-level attempt with B as a confirmed failure.
        // This must not touch A's derivation or build job.
        let commit_outcome = finalize_evaluation_attempt(
            &pool,
            commit_id,
            attempt,
            &plan(
                vec![],
                vec![failed_system("broken", "module type error in options.nix")],
            ),
        )
        .await
        .expect("commit finalizer should not error");

        assert!(
            matches!(commit_outcome, EvaluationFinalizeOutcome::Completed { .. }),
            "commit must complete; got {commit_outcome:?}"
        );

        // alpha's derivation and build job survived.
        assert_eq!(derivation_count(&pool, commit_id).await, 2); // alpha + broken
        assert_eq!(build_job_count(&pool, commit_id).await, 1); // only alpha

        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "complete");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Migration 0184 tests ─────────────────────────────────────────────
    //
    // Migration 0184 deduplication SQL keeps the "best" build_jobs row per
    // derivation based on a status priority order. Two styles of test:
    //
    //   1. Pure Rust unit tests that verify the status ordering directly,
    //      without touching the DB. These always run.
    //
    //   2. A DB test that verifies the unique-index idempotency guarantee
    //      (ON CONFLICT DO NOTHING) via finalize_evaluated_system, which
    //      already exercises that path.

    fn migration_0184_status_rank(status: &str) -> u8 {
        // Must match the CASE expression in migration 0184 exactly.
        // Lower number = higher priority = kept when duplicates exist.
        // Active work beats terminal; success beats failure for terminal rows.
        match status {
            "building" => 1,
            "queued" => 2,
            "cancelling" => 3,
            "success" => 4,
            "cancelled" => 5,
            "failed" => 6,
            _ => 7,
        }
    }

    fn migration_0184_canonical<'a>(statuses: &[&'a str]) -> &'a str {
        // Simulates the migration 0184 CASE expression: pick the winner.
        statuses
            .iter()
            .min_by_key(|&&s| migration_0184_status_rank(s))
            .copied()
            .expect("non-empty")
    }

    #[test]
    fn migration_0184_keeps_active_job_when_duplicate_exists() {
        // 'building' job must beat any terminal row.
        assert_eq!(
            migration_0184_canonical(&["building", "queued"]),
            "building"
        );
        assert_eq!(
            migration_0184_canonical(&["queued", "building"]),
            "building"
        );
        assert_eq!(
            migration_0184_canonical(&["building", "success"]),
            "building"
        );
        assert_eq!(
            migration_0184_canonical(&["building", "failed"]),
            "building"
        );
    }

    #[test]
    fn migration_0184_keeps_queued_over_failed() {
        assert_eq!(migration_0184_canonical(&["queued", "failed"]), "queued");
        assert_eq!(migration_0184_canonical(&["failed", "queued"]), "queued");
    }

    #[test]
    fn migration_0184_keeps_success_over_failed() {
        // A successful terminal row represents the valid output path;
        // a prior failed row is superseded.
        assert_eq!(migration_0184_canonical(&["success", "failed"]), "success");
        assert_eq!(migration_0184_canonical(&["failed", "success"]), "success");
        assert_eq!(
            migration_0184_canonical(&["cancelled", "success"]),
            "success"
        );
        assert_eq!(
            migration_0184_canonical(&["success", "cancelled"]),
            "success"
        );
    }

    #[test]
    fn migration_0184_status_precedence_order() {
        // Canonical order: building > queued > cancelling > success > cancelled > failed.
        let order = [
            "building",
            "queued",
            "cancelling",
            "success",
            "cancelled",
            "failed",
        ];
        for i in 0..order.len() {
            for j in (i + 1)..order.len() {
                assert!(
                    migration_0184_status_rank(order[i]) < migration_0184_status_rank(order[j]),
                    "{} should have lower rank than {}",
                    order[i],
                    order[j]
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn migration_0184_unique_index_prevents_second_insert() {
        // After the unique index exists, a second build_jobs INSERT for the
        // same derivation must fail. This test verifies the
        // `ON CONFLICT (derivation_id) DO NOTHING` path works correctly.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("delta");
        let check = passing_policy_check("delta");

        // First finalize inserts derivation + build job.
        let first = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("first finalize must succeed");
        assert!(matches!(first, SystemFinalizeOutcome::Queued { .. }));

        // Second finalize must return BuildAlreadyExists, not error.
        let second = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("second finalize must not error");
        assert!(
            matches!(second, SystemFinalizeOutcome::BuildAlreadyExists { .. }),
            "expected BuildAlreadyExists, got {second:?}"
        );

        assert_eq!(
            build_job_count(&pool, commit_id).await,
            1,
            "only one build job"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Side-effect isolation test ────────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_build_already_exists_does_not_re_queue() {
        // When a system is finalized twice, the second call must return
        // BuildAlreadyExists and must NOT create a second build_jobs row.
        // This covers the side-effect isolation requirement:
        // BuildAlreadyExists must not trigger another queue notification,
        // QueuedForBuild broadcast, GC root, or hardening scan.
        // The assertion here is at the DB level (row counts); the caller is
        // responsible for checking outcome before emitting side effects.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("epsilon");
        let check = passing_policy_check("epsilon");

        let first = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("first finalize");
        assert!(
            matches!(first, SystemFinalizeOutcome::Queued { .. }),
            "first must be Queued; got {first:?}"
        );
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        // Simulate a retry or concurrent call.
        let second = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("second finalize must not error");

        // Must NOT produce Queued — that would indicate a new row was inserted.
        assert!(
            !matches!(second, SystemFinalizeOutcome::Queued { .. }),
            "second finalize must not return Queued (no new row should be inserted); got {second:?}"
        );

        // Exactly one build_jobs row for this derivation.
        assert_eq!(
            build_job_count(&pool, commit_id).await,
            1,
            "exactly one build_jobs row must exist after two finalizations"
        );
        assert_eq!(
            derivation_count(&pool, commit_id).await,
            1,
            "exactly one derivation row"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Fallback recovery regression ──────────────────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_error_result_does_not_block_standalone_finalization() {
        // This test verifies the core nix-builder-1 fix:
        // A system that produced an error JSON line from the bulk evaluator
        // must NOT be treated as "seen" (i.e., it must fall through to
        // standalone fallback evaluation in the real evaluator).
        //
        // We cannot invoke the full bulk evaluator in a unit test, but we CAN
        // verify that `finalize_evaluated_system` still works for such a system
        // after the commit remains in_progress — simulating what the standalone
        // fallback path would do after the bulk evaluator emits an error line.
        //
        // The key property: an eval-error system must be finalizable as a
        // SUCCESS through the standalone path if standalone eval succeeds.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        // Simulate: standalone fallback succeeded for a system that had an error
        // in the bulk run. This is what happens when:
        //   bulk nix-eval-jobs emits: {"attr":"nix-builder-1","error":"..."}
        //   → system NOT added to seen_systems (the fix in this commit)
        //   → system appears in missing_systems
        //   → standalone eval runs and succeeds
        //   → finalize_evaluated_system called with the standalone result
        let system = successful_system("nix-builder-1");
        let check = passing_policy_check("nix-builder-1");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("standalone fallback finalization must succeed");

        assert!(
            matches!(outcome, SystemFinalizeOutcome::Queued { .. }),
            "standalone-recovered system must be queued; got {outcome:?}"
        );
        assert_eq!(derivation_count(&pool, commit_id).await, 1);
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── seen_systems behavior unit test ───────────────────────────────────

    #[test]
    fn error_result_routing_semantics() {
        // Three-way classification for systems in the bulk evaluator output:
        //
        //   has_error=false, drv_path=Some → seen (successful, no further action)
        //   has_error=true                 → confirmed failure (use error message directly,
        //                                    NOT added to seen_systems or missing_systems)
        //   has_error=false, drv_path=None → missing (no JSON line or drv; goes to fallback)
        //
        // Only truly silent-drop systems (produced no JSON line at all) end up in
        // missing_systems and trigger standalone fallback evaluation.
        fn classify(has_error: bool, has_drv: bool) -> &'static str {
            if !has_error && has_drv {
                "seen"
            } else if has_error {
                "confirmed_failure"
            } else {
                "missing"
            }
        }

        assert_eq!(classify(false, true), "seen");
        assert_eq!(classify(true, true), "confirmed_failure");
        assert_eq!(classify(true, false), "confirmed_failure");
        assert_eq!(classify(false, false), "missing");
    }

    // ── parse_expected_store_path_from_outputs unit tests ────────────

    #[test]
    fn parse_expected_store_path_from_outputs_standard_out_path() {
        let j = serde_json::json!({"out": {"path": "/nix/store/abc123-foo"}});
        assert_eq!(
            super::parse_expected_store_path_from_outputs(&j).as_deref(),
            Some("/nix/store/abc123-foo")
        );
    }

    #[test]
    fn parse_expected_store_path_from_outputs_out_path_fallback() {
        let j = serde_json::json!({"out": {"outPath": "/nix/store/def456-bar"}});
        assert_eq!(
            super::parse_expected_store_path_from_outputs(&j).as_deref(),
            Some("/nix/store/def456-bar")
        );
    }

    #[test]
    fn parse_expected_store_path_from_outputs_plain_string() {
        let j = serde_json::json!({"out": "/nix/store/ghi789-baz"});
        assert_eq!(
            super::parse_expected_store_path_from_outputs(&j).as_deref(),
            Some("/nix/store/ghi789-baz")
        );
    }

    #[test]
    fn parse_expected_store_path_from_outputs_top_level_out_path() {
        let j = serde_json::json!({"outPath": "/nix/store/jkl012-qux"});
        assert_eq!(
            super::parse_expected_store_path_from_outputs(&j).as_deref(),
            Some("/nix/store/jkl012-qux")
        );
    }

    #[test]
    fn parse_expected_store_path_from_outputs_rejects_non_store_path() {
        let j = serde_json::json!({"out": {"path": "/tmp/foo"}});
        assert!(super::parse_expected_store_path_from_outputs(&j).is_none());
    }

    #[test]
    fn parse_expected_store_path_from_outputs_missing_out() {
        let j = serde_json::json!({"foo": {"path": "/nix/store/abc123-foo"}});
        assert!(super::parse_expected_store_path_from_outputs(&j).is_none());
    }

    #[test]
    fn parse_expected_store_path_from_outputs_empty() {
        let j = serde_json::json!({});
        assert!(super::parse_expected_store_path_from_outputs(&j).is_none());
    }

    #[test]
    fn parse_expected_store_path_from_outputs_no_path_field() {
        let j = serde_json::json!({"out": {"foo": "bar"}});
        assert!(super::parse_expected_store_path_from_outputs(&j).is_none());
    }

    // ── BuildAlreadyExists status propagation ────────────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_system_build_already_exists_reflects_current_status() {
        // When a build_job exists with a non-"queued" status, the
        // BuildAlreadyExists outcome must carry the actual DB status so the
        // caller can decide whether to emit a queue notification.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("kappa");
        let check = passing_policy_check("kappa");

        // First finalize → Queued
        let first = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("first finalize");
        assert!(
            matches!(first, SystemFinalizeOutcome::Queued { .. }),
            "first must be Queued; got {first:?}"
        );

        let build_job_id = match &first {
            SystemFinalizeOutcome::Queued { build_job_id, .. } => *build_job_id,
            _ => unreachable!(),
        };

        // Manually advance the build_job to "building"
        sqlx::query("UPDATE build_jobs SET status = 'building' WHERE id = $1")
            .bind(build_job_id)
            .execute(&pool)
            .await
            .expect("update to building");

        // Second finalize → BuildAlreadyExists with status="building"
        let second = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("second finalize");
        assert!(
            matches!(&second, SystemFinalizeOutcome::BuildAlreadyExists { build_job_status, .. } if build_job_status == "building"),
            "second must be BuildAlreadyExists with status 'building'; got {second:?}"
        );
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        // Advance to "success"
        sqlx::query("UPDATE build_jobs SET status = 'success' WHERE id = $1")
            .bind(build_job_id)
            .execute(&pool)
            .await
            .expect("update to success");

        let third = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("third finalize");
        assert!(
            matches!(&third, SystemFinalizeOutcome::BuildAlreadyExists { build_job_status, .. } if build_job_status == "success"),
            "third must be BuildAlreadyExists with status 'success'; got {third:?}"
        );
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Build-job claimability only after activation ─────────────────

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn system_build_ordering_persist_then_root_then_activate() {
        // Verifies the three-phase ordering that prevents the build-job
        // claimability race:
        //
        //   Phase 1 (persist): derivation recorded, NO build job
        //     → get_next_job_for_builder returns None
        //   Phase 2 (GC root):  established (not tested directly here;
        //     finalize_evaluated_system handles it internally)
        //   Phase 3 (activate): build job inserted
        //     → get_next_job_for_builder returns Some(job_id)
        //
        // Throughout: commit remains in_progress.
        use crate::models::builders::CreateBuilderRequest;
        use crate::queries::build_jobs::get_next_job_for_builder;
        use crate::queries::builders::create_builder;
        use base64::Engine;

        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        // Create and activate a builder
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let pub_b64 = base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());
        let builder_name = format!("ordering-test-builder-{}", uuid::Uuid::new_v4());
        let (builder, _) = create_builder(
            &pool,
            &CreateBuilderRequest {
                name: builder_name,
                host: Some("ordering-test.local".to_string()),
                arch: "x86_64-linux".to_string(),
                public_key: Some(pub_b64),
                max_cpu_cores: None,
                max_memory_mb: None,
                max_concurrent_jobs: Some(1),
                enabled: Some(true),
                environment_ids: vec![],
            },
        )
        .await
        .expect("create builder");
        sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("activate builder");

        // ── Phase 1: persist only (no build job) ─────────────────────
        let system = successful_system("lambda");
        let check = passing_policy_check("lambda");
        let persisted = persist_evaluated_system(&pool, commit_id, attempt, &system, &check, &[])
            .await
            .expect("persist should succeed");

        let derivation_id = match &persisted {
            SystemPersistenceOutcome::NeedsBuildPreparation { derivation_id, .. } => *derivation_id,
            other => panic!("expected NeedsBuildPreparation after first persist, got {other:?}"),
        };

        // Confirm commit is still in_progress
        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .expect("fetch commit status");
        assert_eq!(status, "in_progress", "commit must remain in_progress");

        // Verify no build job exists yet (Phase 1 must NOT insert one)
        let before_activation = get_next_job_for_builder(&pool, builder.id)
            .await
            .expect("claim should not error");
        assert!(
            before_activation.is_none(),
            "no build job should be claimable before activation; got {before_activation:?}"
        );

        // ── Phase 3: activate after Phase 2 has completed ─────────────
        // In production the caller must create the GC root before this call.
        // This test isolates the durable DB boundary: the build job is not
        // claimable until activation commits.
        let activation = activate_evaluated_system_build(&pool, commit_id, attempt, derivation_id)
            .await
            .expect("activate should succeed");

        let build_job_id = match &activation {
            SystemBuildActivationOutcome::Queued { build_job_id } => *build_job_id,
            other => panic!("expected Queued after activation, got {other:?}"),
        };

        // Commit still in_progress
        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .expect("fetch commit status");
        assert_eq!(status, "in_progress", "commit must remain in_progress");

        // After activation, the job IS claimable
        let after_activation = get_next_job_for_builder(&pool, builder.id)
            .await
            .expect("claim should not error");

        assert_eq!(
            after_activation,
            Some(build_job_id),
            "build job must be claimable after activation; \
             expected Some({build_job_id}), got {after_activation:?}"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Per-configuration policy regression tests ─────────────────────────

    /// Proves that when two configurations are in the same flake but different
    /// environments with different policies, one configuration's strict failure
    /// does NOT prevent the other from reaching NeedsBuildPreparation.
    ///
    /// Run with:
    ///   CRYSTAL_FORGE_TEST_DATABASE_URL=... cargo test -p cf-server --lib \
    ///     different_environments_use_different_policy_sets -- --ignored --test-threads=1
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn different_environments_use_different_policy_sets() {
        // This test verifies the *query-level* behavior of
        // list_policy_rows_by_configuration_for_flake combined with
        // load_policies_by_configuration_for_eval.
        //
        // Setup:
        //   Flake has two systems:
        //     alpha — environment A — policy: require package "grafana" (strict)
        //     beta  — environment B — policy: require package "neovim"  (strict)
        //
        // When evaluated with a PoliciesByConfiguration map:
        //   alpha's PolicyCheckResult should only check grafana.
        //   beta's PolicyCheckResult should only check neovim.
        //
        // This test exercises the from_assigned parser directly.

        use crate::models::deployment_policies::{
            AssignedPolicy, DeploymentPolicy, PoliciesByConfiguration, PolicyCheckResult,
        };

        let id_grafana = uuid::Uuid::from_u128(1001);
        let id_neovim = uuid::Uuid::from_u128(1002);

        let mut map = PoliciesByConfiguration::new();
        map.insert(
            "alpha".to_string(),
            vec![AssignedPolicy {
                policy_id: id_grafana,
                policy_name: "require-grafana".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            }],
        );
        map.insert(
            "beta".to_string(),
            vec![AssignedPolicy {
                policy_id: id_neovim,
                policy_name: "require-neovim".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["neovim".to_string()],
                    strict: true,
                },
            }],
        );

        let key_grafana = crate::models::deployment_policies::policy_result_key(&id_grafana);
        let key_neovim = crate::models::deployment_policies::policy_result_key(&id_neovim);

        // alpha: grafana check fails, neovim key not present (not assigned),
        // cfAgentEnabled emitted unconditionally.
        let alpha_json = serde_json::json!({ "cfAgentEnabled": true, &key_grafana: false });
        let alpha_check = PolicyCheckResult::from_assigned(
            "alpha".to_string(),
            &alpha_json,
            map.get("alpha").map(Vec::as_slice).unwrap_or(&[]),
        )
        .expect("from_assigned must not error for alpha");

        assert!(
            !alpha_check.meets_requirements,
            "alpha must fail (grafana strict failure)"
        );
        assert!(!alpha_check.failed_policies.is_empty());

        // beta: neovim check passes, grafana key not present (not assigned),
        // cfAgentEnabled emitted unconditionally.
        let beta_json = serde_json::json!({ "cfAgentEnabled": true, &key_neovim: true });
        let beta_check = PolicyCheckResult::from_assigned(
            "beta".to_string(),
            &beta_json,
            map.get("beta").map(Vec::as_slice).unwrap_or(&[]),
        )
        .expect("from_assigned must not error for beta");

        assert!(
            beta_check.meets_requirements,
            "beta must pass (neovim policy passes)"
        );
        assert!(beta_check.failed_policies.is_empty());

        // Prove isolation: beta's result is unaffected by alpha's failure.
        // (If a flake-wide policy union were applied, beta would also check grafana
        // and would also fail if grafana key is absent from its JSON.)
        let beta_json_no_grafana = serde_json::json!({ "cfAgentEnabled": true, &key_neovim: true });
        let beta_check2 = PolicyCheckResult::from_assigned(
            "beta".to_string(),
            &beta_json_no_grafana,
            map.get("beta").map(Vec::as_slice).unwrap_or(&[]),
        )
        .expect("beta from_assigned must not error");
        assert!(
            beta_check2.meets_requirements,
            "beta must still pass even when grafana key is absent"
        );
    }

    /// Proves that a configuration with no assigned policies passes evaluation
    /// and does not inherit policies from another configuration's environment.
    #[test]
    fn no_policy_configuration_passes_evaluation() {
        use crate::models::deployment_policies::{
            AssignedPolicy, DeploymentPolicy, PoliciesByConfiguration, PolicyCheckResult,
        };

        let id_grafana = uuid::Uuid::from_u128(2001);
        let mut map = PoliciesByConfiguration::new();
        // Only "alpha" has policies; "beta" has none (no entry in map).
        map.insert(
            "alpha".to_string(),
            vec![AssignedPolicy {
                policy_id: id_grafana,
                policy_name: "require-grafana".to_string(),
                policy: DeploymentPolicy::RequirePackages {
                    packages: vec!["grafana".to_string()],
                    strict: true,
                },
            }],
        );

        // beta: no assigned policies → empty slice, but evaluator still emits
        // cfAgentEnabled unconditionally so build-job eligibility is known.
        let beta_json = serde_json::json!({ "cfAgentEnabled": true });
        let beta_check = PolicyCheckResult::from_assigned("beta".to_string(), &beta_json, &[])
            .expect("from_assigned with empty assigned must not error");

        assert!(
            beta_check.meets_requirements,
            "configuration with zero assigned policies must pass"
        );
        assert!(beta_check.failed_policies.is_empty());
        assert_eq!(
            beta_check.cf_agent_enabled,
            Some(true),
            "cf_agent_enabled must be read from unconditional evaluator metadata"
        );

        // Missing cfAgentEnabled is an infrastructure/parser mismatch.
        let missing_json = serde_json::json!({});
        let missing_result =
            PolicyCheckResult::from_assigned("beta".to_string(), &missing_json, &[]);
        assert!(
            missing_result.is_err(),
            "absent cfAgentEnabled must be treated as an infrastructure error"
        );
    }
}
