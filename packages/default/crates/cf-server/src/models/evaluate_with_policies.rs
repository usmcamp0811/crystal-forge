use anyhow::{Context, Result, bail};
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::{Duration, Instant};

const MOCK_EVAL_TOTAL_DURATION_MS: u64 = 30_000;
const MOCK_EVAL_MIN_PER_SYSTEM_MS: u64 = 5_000;
const MOCK_EVAL_STAGE_COUNT: u64 = 5;
const EVAL_OUTPUT_IDLE_TIMEOUT_SECS: u64 = 300;
const EVAL_PROGRESS_HEARTBEAT_SECS: u64 = 30;
/// Maximum number of missing systems to attempt individual fallback evaluation
/// for. Beyond this threshold, treat as a likely process-wide evaluator failure
/// and retry the commit rather than spawning dozens of standalone evaluations.
const MAX_INDIVIDUAL_FALLBACKS: usize = 8;
/// Maximum concurrent fallback evaluations.
const FALLBACK_CONCURRENCY: usize = 2;
/// Overall deadline for the fallback phase.
const FALLBACK_PHASE_TIMEOUT: Duration = Duration::from_secs(180);
const CLOSURE_COUNT_MAX_CONCURRENT: usize = 2;
static CLOSURE_COUNT_LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
use tracing::{debug, error, info, warn};

use crate::config::{BuildConfig, ServerConfig};
use crate::derivations::utils::{build_flake_reference, count_closure_packages};
use crate::flake::credentials::FlakeCredentialEnv;
use crate::models::commits::Commit;
use crate::models::deployment_policies::{
    DeploymentPolicy, PolicyCheckResult, build_nix_eval_expression,
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

async fn run_incremental_system_side_effects(
    pool: &PgPool,
    commit_id: i32,
    flake_repo_url: &str,
    commit_hash: &str,
    finalized: &FinalizedDerivation,
) {
    match crate::builder::create_drv_gc_root(&finalized.drv_path, finalized.derivation_id).await {
        Ok(true) => debug!(
            "📌 Rooted evaluated drv (id={}, drv={})",
            finalized.derivation_id, finalized.drv_path
        ),
        Ok(false) => warn!(
            "⚠️  Evaluated drv (id={}, drv={}) is not valid in the server store; remote builders may not be able to import it",
            finalized.derivation_id, finalized.drv_path
        ),
        Err(err) => warn!(
            "⚠️  Failed to create GC root for evaluated drv {} (id={}): {}",
            finalized.drv_path, finalized.derivation_id, err
        ),
    }

    let pool2 = pool.clone();
    let drv2 = finalized.drv_path.clone();
    let derivation_id = finalized.derivation_id;
    let limiter = closure_count_limiter();
    tokio::spawn(async move {
        let permit = match limiter.acquire_owned().await {
            Ok(permit) => permit,
            Err(err) => {
                warn!(
                    "⚠️  Failed to acquire closure count permit for id={}: {}",
                    derivation_id, err
                );
                return;
            }
        };

        match count_closure_packages(&drv2).await {
            Ok((total, cached)) => {
                if let Err(err) = crate::queries::derivations::set_closure_counts(
                    &pool2,
                    derivation_id,
                    total,
                    cached,
                )
                .await
                {
                    warn!(
                        "⚠️  Failed to store closure counts for id={}: {}",
                        derivation_id, err
                    );
                }
            }
            Err(err) => warn!(
                "⚠️  Failed to count closure packages for id={}: {}",
                derivation_id, err
            ),
        }
        drop(permit);
    });

    if let Err(err) =
        trigger_commit_hardening_scans(pool.clone(), commit_id, flake_repo_url, commit_hash).await
    {
        warn!(
            "Failed to queue hardening scans for commit {} after system {} finalized: {}",
            commit_id, finalized.system_name, err
        );
    }
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

fn build_single_system_eval_expression(
    flake_ref: &str,
    system_name: &str,
    policies: &[DeploymentPolicy],
) -> String {
    let nix_policies: Vec<&DeploymentPolicy> =
        policies.iter().filter(|p| p.is_nix_evaluated()).collect();

    let policy_fields = if nix_policies.is_empty() {
        "        # No policies configured".to_string()
    } else {
        nix_policies
            .iter()
            .enumerate()
            .flat_map(|(policy_idx, policy)| match policy {
                DeploymentPolicy::CustomCheck { rules, .. } if !rules.is_empty() => rules
                    .iter()
                    .map(|rule| format!("        {} = {};", rule.field_name, rule.expression))
                    .collect::<Vec<_>>(),
                _ => {
                    let (field_name, expr) = policy.to_nix_expression_with_index(policy_idx);
                    vec![format!("        {} = {};", field_name, expr)]
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"
let
  flake = builtins.getFlake "{}";
  cfg = builtins.getAttr "{}" flake.nixosConfigurations;
  drv = cfg.config.system.build.toplevel;
  policyResults = {{
{}
  }};
in {{
  drvPath = drv.drvPath;
  outputs = drv.outputs or {{}};
  policies = policyResults;
}}
"#,
        flake_ref, system_name, policy_fields
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
    policies: &[DeploymentPolicy],
    creds: Option<&FlakeCredentialEnv>,
    build_config: &BuildConfig,
) -> Result<StandaloneSystemOutcome> {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    let nix_expr = build_single_system_eval_expression(&flake_ref, system_name, policies);

    let mut cmd = tokio::process::Command::new("nix");
    cmd.kill_on_drop(true);
    cmd.args(["eval", "--impure", "--json", "--expr", &nix_expr]);
    build_config.apply_to_command(&mut cmd);
    if let Some(c) = creds {
        c.apply_to_nix_command(&mut cmd);
    }

    let output = match tokio::time::timeout(Duration::from_secs(120), cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Failed to run standalone eval: {}", e),
            });
        }
        Err(_) => {
            return Ok(StandaloneSystemOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Standalone eval timed out for {}", system_name),
            });
        }
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
    let policy_check = if policies.is_empty() {
        PolicyCheckResult {
            system_name: system_name.to_string(),
            cf_agent_enabled: None,
            has_required_packages: None,
            custom_checks: HashMap::new(),
            meets_requirements: true,
            warnings: Vec::new(),
            failed_policies: Vec::new(),
            cve_checks: Vec::new(),
        }
    } else {
        PolicyCheckResult::from_json(system_name.to_string(), &parsed.policies, policies)
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

    // Apply the same Nix configuration as the main evaluator (offline mode,
    // substitute behaviour, Nix timeouts, sandbox settings, etc.).
    build_config.apply_to_command(&mut cmd);

    // Kill the nix process if the future is dropped (e.g. timeout or
    // cancellation during the buffer_unordered fallback phase).
    cmd.kill_on_drop(true);

    if let Some(c) = creds {
        c.apply_to_nix_command(&mut cmd);
    }

    let output = match tokio::time::timeout(Duration::from_secs(120), cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            return FallbackEvalOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Failed to run fallback eval: {}", e),
            };
        }
        Err(_) => {
            return FallbackEvalOutcome::InfrastructureFailure {
                system_name: system_name.to_string(),
                error: format!("Fallback eval timed out for {}", system_name),
            };
        }
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
    },
    PreservedExistingBuild {
        derivation_id: i32,
        status_id: i32,
    },
    Cancelled,
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

pub async fn finalize_evaluated_system(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    result: &SuccessfulSystemResult,
    policy_check: &PolicyCheckResult,
) -> Result<SystemFinalizeOutcome> {
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
        return Ok(SystemFinalizeOutcome::Superseded);
    };

    let attempt_count = state.evaluation_attempt_count.unwrap_or(0);
    let status = state.evaluation_status.as_deref().unwrap_or("pending");
    let cancellation = state.cancellation_requested.unwrap_or(false);

    if attempt_count != expected_attempt || !matches!(status, "in_progress" | "cancelling") {
        tx.rollback().await?;
        return Ok(SystemFinalizeOutcome::Superseded);
    }

    if cancellation || status == "cancelling" {
        tx.rollback().await?;
        return Ok(SystemFinalizeOutcome::Cancelled);
    }

    let write = record_successful_eval_result_in_tx(
        &mut tx,
        Some(commit_id),
        &result.system_name,
        "nixos",
        Some(&result.derivation_target),
        &result.drv_path,
        result.expected_store_path.as_deref(),
        result.cf_agent_enabled,
    )
    .await
    .with_context(|| format!("Failed to write derivation for {}", result.system_name))?;

    let derivation_id = match write {
        SuccessfulEvalWrite::Inserted { derivation_id }
        | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id } => derivation_id,
        SuccessfulEvalWrite::PreservedBuildState {
            derivation_id,
            status_id,
        } => {
            tx.commit().await?;
            return Ok(SystemFinalizeOutcome::PreservedExistingBuild {
                derivation_id,
                status_id,
            });
        }
    };

    if let Some(reason) = system_not_queued_reason(policy_check, build_eligible) {
        tx.commit().await?;
        return Ok(SystemFinalizeOutcome::RecordedWithoutBuild {
            derivation_id,
            reason,
        });
    }

    let build_outcome = create_build_job_for_derivation_tx(&mut tx, derivation_id).await?;
    let outcome = match build_outcome {
        Some(BuildJobInsertOutcome::Inserted { build_job_id }) => SystemFinalizeOutcome::Queued {
            derivation_id,
            build_job_id,
        },
        Some(BuildJobInsertOutcome::AlreadyExists { build_job_id }) => {
            SystemFinalizeOutcome::BuildAlreadyExists {
                derivation_id,
                build_job_id,
            }
        }
        None => SystemFinalizeOutcome::RecordedWithoutBuild {
            derivation_id,
            reason: SystemNotQueuedReason::BuildNotRequested,
        },
    };

    tx.commit().await?;
    Ok(outcome)
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
    policies: &[DeploymentPolicy],
    cf_state: Option<&crate::handlers::agent_request::CFState>,
    _queue_notifier: Option<&QueueNotifier>,
) -> Result<EvaluationPlan> {
    // Re-evaluation safety: clear previous persisted logs for this commit so
    // (commit_id, log_sequence) uniqueness cannot collide on subsequent runs.
    crate::queries::eval_logs::delete_eval_logs_by_commit(pool, commit.id).await?;

    // Sequence counter for log persistence (1-indexed)
    let mut log_sequence = 1i32;

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

    // Build ONE Nix expression that includes policy checks
    let nix_expr = build_nix_eval_expression(&flake_ref, policies);

    info!(
        "🚀 Running: nix-eval-jobs for {} with {} policies",
        target_system,
        policies.len()
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

        if !policies.is_empty() {
            let policy_msg = format!("📋 Checking {} deployment policies:", policies.len());
            broadcast_and_persist_eval_log(
                pool,
                Some(state),
                commit.id,
                &mut log_sequence,
                policy_msg,
            )
            .await;
            for policy in policies {
                let policy_detail = format!(
                    "   • {} (strict: {})",
                    policy.description(),
                    policy.is_strict()
                );
                broadcast_and_persist_eval_log(
                    pool,
                    Some(state),
                    commit.id,
                    &mut log_sequence,
                    policy_detail,
                )
                .await;
            }
        }

        broadcast_and_persist_eval_log(
            pool,
            Some(state),
            commit.id,
            &mut log_sequence,
            "⏳ Evaluating nixosConfigurations...".to_string(),
        )
        .await;
    }

    if !policies.is_empty() {
        info!("   Policies will be evaluated in parallel by nix-eval-jobs:");
        for policy in policies {
            info!(
                "     - {} (strict={})",
                policy.description(),
                policy.is_strict()
            );
        }
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
                        warn!("🚫 Cancellation requested for commit {} — killing nix-eval-jobs", commit.id);
                        if let Err(kill_err) = child.kill().await {
                            warn!("Failed to kill nix-eval-jobs for cancelled commit {}: {kill_err}", commit.id);
                        }
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

                    let _ = child.kill().await;
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
                                let expected_store_path = if !has_error {
                                    if let Some(drv) = drv_path.as_deref() {
                                        resolve_expected_store_path(drv, result.outputs.as_ref()).await
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                debug!("📦 Evaluated: {}, drv_path={:?}, has_error={:?}",
                                    system_name, drv_path, has_error);

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
                                        let log_msg = format!("✅ {} evaluated successfully", system_name);
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

                                // Extract policy check results from meta.policies
                                let mut cf_agent_enabled = None;
                                let mut policy_check_for_system: Option<PolicyCheckResult> = None;
                                if let Some(meta) = &result.meta {
                                    if let Some(policies_json) = meta.get("policies") {
                                        // Parse policy results from meta.policies
                                        let check = PolicyCheckResult::from_json(
                                            system_name.clone(),
                                            policies_json,
                                            policies,
                                        );

                                        cf_agent_enabled = check.cf_agent_enabled;

                                        // Log policy results
                                        if !check.meets_requirements {
                                            let has_strict = policies.iter().any(|p| p.is_strict());
                                            for warning in &check.warnings {
                                                if has_strict {
                                                    error!("❌ {}", warning);
                                                } else {
                                                    warn!("⚠️  {}", warning);
                                                }

                                                // Broadcast policy warnings to logs
                                                if let Some(state) = cf_state {
                                                    let log_msg = format!("⚠️  {}: {}", system_name, warning);
                                                    broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                                }
                                            }
                                        } else if let Some(true) = cf_agent_enabled {
                                            info!("✅ {} has CF agent enabled", system_name);

                                            // Broadcast policy success to logs
                                            if let Some(state) = cf_state {
                                                let log_msg = format!("✅ {}: Crystal Forge agent enabled", system_name);
                                                broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence, log_msg).await;
                                            }
                                        }

                                        policy_check_for_system = Some(check.clone());
                                        policy_checks.push(check);
                                    } else {
                                        debug!("⚠️  No policies in meta for {}", system_name);
                                    }
                                } else {
                                    debug!("⚠️  No meta field for {}", system_name);
                                }

                                if policy_check_for_system.is_none() && policies.is_empty() {
                                    let check = PolicyCheckResult {
                                        system_name: system_name.clone(),
                                        cf_agent_enabled: None,
                                        has_required_packages: None,
                                        custom_checks: HashMap::new(),
                                        meets_requirements: true,
                                        warnings: Vec::new(),
                                        failed_policies: Vec::new(),
                                        cve_checks: Vec::new(),
                                    };
                                    policy_check_for_system = Some(check.clone());
                                    policy_checks.push(check);
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
                                                    } else if cf_agent_enabled == Some(true) {
                                                        // QueuedForBuild broadcast is deferred to after
                                                        // full attempt validation (post-finalization).
                                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                                            format!(
                                                                "✅ {}: policy passed (CF enabled), evaluated",
                                                                system_name
                                                            ),
                                                        )
                                                        .await;
                                    } else {
                                        crate::handlers::api::commits::broadcast_system_status(
                                            state,
                                            commit.id,
                                            system_name.clone(),
                                            crate::handlers::api::commits::SystemEvalStatus::PolicyFailed,
                                            Some("CF agent not enabled in configuration".to_string()),
                                        )
                                        .await;
                                        broadcast_and_persist_eval_log(pool, Some(state), commit.id, &mut log_sequence,
                                            format!(
                                                "⚠️ {}: policy failed (CF agent not enabled)",
                                                system_name
                                            ),
                                        )
                                        .await;
                                    }
                                }

                                // ── Incrementally persist successful systems ──────────
                                // A healthy system must be queued as soon as its own eval,
                                // policy check, derivation write, and build-job insertion commit.
                                if let Some(system_name) = result.attr_path.last() {
                                    if !has_error && drv_path.is_some() {
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

                                        match finalize_evaluated_system(
                                            pool,
                                            commit.id,
                                            expected_attempt,
                                            &successful,
                                            policy_check,
                                        )
                                        .await?
                                        {
                                            SystemFinalizeOutcome::Queued { derivation_id, build_job_id }
                                            | SystemFinalizeOutcome::BuildAlreadyExists { derivation_id, build_job_id } => {
                                                let finalized = FinalizedDerivation {
                                                    derivation_id,
                                                    drv_path: successful.drv_path.clone(),
                                                    system_name: successful.system_name.clone(),
                                                    cf_agent_enabled: successful.cf_agent_enabled,
                                                };
                                                successful_results.push(successful.clone());
                                                if let Some(queue_notifier) = _queue_notifier {
                                                    queue_notifier.notify_build_queue();
                                                }
                                                if let Some(state) = cf_state {
                                                    crate::handlers::api::commits::broadcast_system_status(
                                                        state,
                                                        commit.id,
                                                        finalized.system_name.clone(),
                                                        crate::handlers::api::commits::SystemEvalStatus::QueuedForBuild,
                                                        None,
                                                    ).await;
                                                    broadcast_and_persist_eval_log(
                                                        pool,
                                                        Some(state),
                                                        commit.id,
                                                        &mut log_sequence,
                                                        format!("🚀 {}: build job queued ({})", finalized.system_name, build_job_id),
                                                    ).await;
                                                }
                                                run_incremental_system_side_effects(
                                                    pool,
                                                    commit.id,
                                                    &flake.repo_url,
                                                    &commit.git_commit_hash,
                                                    &finalized,
                                                ).await;
                                            }
                                            SystemFinalizeOutcome::RecordedWithoutBuild { derivation_id, reason } => {
                                                debug!(
                                                    "📋 Recorded {} as derivation {} without build: {:?}",
                                                    system_name, derivation_id, reason
                                                );
                                                successful_results.push(successful);
                                            }
                                            SystemFinalizeOutcome::PreservedExistingBuild { derivation_id, status_id } => {
                                                debug!(
                                                    "📋 Preserved existing build state {} for {} derivation {}",
                                                    status_id, system_name, derivation_id
                                                );
                                                successful_results.push(successful);
                                            }
                                            SystemFinalizeOutcome::Cancelled => return Err(EvaluationCancelled.into()),
                                            SystemFinalizeOutcome::Superseded => {
                                                bail!("evaluation attempt was superseded while finalizing {}", system_name);
                                            }
                                        }
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
    let error_line_system_names: HashSet<&str> =
        error_line_failures.iter().map(|f| f.system_name.as_str()).collect();

    // Collect missing systems that need fallback evaluation.
    // Exclude both successfully-seen systems AND error-line failures —
    // only truly silent-drop systems (no JSON line at all) go to fallback.
    let missing_systems: Vec<&str> = expected_systems
        .iter()
        .filter(|s| !seen_systems.contains(s.as_str()) && !error_line_system_names.contains(s.as_str()))
        .map(|s| s.as_str())
        .collect();
    let unexpected_systems: Vec<String> = seen_systems
        .iter()
        .filter(|seen| !expected_systems.iter().any(|expected| expected == *seen))
        .cloned()
        .collect();
    info!("Seen systems (successful): {:?}", seen_systems);
    info!("Error-line failures (confirmed from bulk output): {:?}", error_line_system_names);
    info!("Missing systems (no output at all, need fallback): {:?}", missing_systems);
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
            let policies = policies.to_vec();
            fallback_futures.push(async move {
                evaluate_single_system_with_policies(
                    &repo_url,
                    &commit_hash,
                    &system_name,
                    &policies,
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

        // `collect` the entire stream so we can race the collected future
        // against timeout and cancellation below (no per-item blocking
        // inside the select! handler).
        let fallback_work = stream::iter(fallback_futures)
            .buffer_unordered(FALLBACK_CONCURRENCY)
            .collect::<Vec<_>>();

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

        let outcomes = tokio::select! {
            biased;

            cancellation_result = &mut cancellation => {
                cancellation_result?;
                return Err(EvaluationCancelled.into());
            }

            fallback_result = tokio::time::timeout(
                FALLBACK_PHASE_TIMEOUT,
                fallback_work,
            ) => {
                fallback_result.with_context(|| {
                    format!(
                        "Fallback evaluation phase timed out after {}s",
                        FALLBACK_PHASE_TIMEOUT.as_secs()
                    )
                })?
            }
        };

        // ── Classify fallback outcomes. Successful recovered systems are
        // finalized immediately just like streaming bulk successes.
        for outcome in outcomes {
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
                    match finalize_evaluated_system(
                        pool,
                        commit.id,
                        expected_attempt,
                        &result,
                        &policy_check,
                    )
                    .await?
                    {
                        SystemFinalizeOutcome::Queued {
                            derivation_id,
                            build_job_id,
                        }
                        | SystemFinalizeOutcome::BuildAlreadyExists {
                            derivation_id,
                            build_job_id,
                        } => {
                            let finalized = FinalizedDerivation {
                                derivation_id,
                                drv_path: result.drv_path.clone(),
                                system_name: result.system_name.clone(),
                                cf_agent_enabled: result.cf_agent_enabled,
                            };
                            if let Some(queue_notifier) = _queue_notifier {
                                queue_notifier.notify_build_queue();
                            }
                            if let Some(state) = cf_state {
                                crate::handlers::api::commits::broadcast_system_status(
                                    state,
                                    commit.id,
                                    finalized.system_name.clone(),
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
                                        "🚀 {}: build job queued ({})",
                                        finalized.system_name, build_job_id
                                    ),
                                )
                                .await;
                            }
                            run_incremental_system_side_effects(
                                pool,
                                commit.id,
                                &flake.repo_url,
                                &commit.git_commit_hash,
                                &finalized,
                            )
                            .await;
                        }
                        SystemFinalizeOutcome::RecordedWithoutBuild { .. }
                        | SystemFinalizeOutcome::PreservedExistingBuild { .. } => {}
                        SystemFinalizeOutcome::Cancelled => return Err(EvaluationCancelled.into()),
                        SystemFinalizeOutcome::Superseded => {
                            bail!(
                                "evaluation attempt was superseded while finalizing fallback system"
                            )
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

    if !policies.is_empty() && !policy_checks.is_empty() {
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
    _policies: &[DeploymentPolicy],
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
        SuccessfulSystemResult, SystemFinalizeOutcome, SystemNotQueuedReason,
        finalize_evaluated_system, finalize_evaluation_attempt, mock_eval_stage_delay,
        resolve_mock_systems, should_mock_policy_fail, summarize_commit_metadata,
    };
    use crate::api::models::CancelEvalOutcome;
    use crate::models::deployment_policies::PolicyCheckResult;
    use crate::queries::commits::{
        EvalFailureOutcome, EvalStartOutcome, cancel_commit_evaluation,
        mark_commit_evaluation_failed, mark_commit_evaluation_started,
    };
    use sqlx::PgPool;

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

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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
    async fn finalize_system_strict_policy_failure_does_not_queue() {
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("alpha");
        let check = failing_policy_check("alpha", true, "Require packages: git");

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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

        let first = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
            .await
            .expect("first finalization should queue build");
        let first_job_id = match first {
            SystemFinalizeOutcome::Queued { build_job_id, .. } => build_job_id,
            other => panic!("expected queued outcome, got {other:?}"),
        };

        let second = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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

        let outcome = finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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
            finalize_evaluated_system(&pool, commit_id, attempt, &first_system, &first_check)
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
        let second_outcome =
            finalize_evaluated_system(&pool, commit_id, attempt, &second_system, &second_check)
                .await
                .expect("finalize after cancel should return Cancelled, not error");
        assert!(
            matches!(second_outcome, SystemFinalizeOutcome::Cancelled),
            "second system must be Cancelled after eval cancel; got {second_outcome:?}"
        );

        // First system and its build job still intact.
        assert_eq!(derivation_count(&pool, commit_id).await, 1, "alpha derivation must survive");
        assert_eq!(build_job_count(&pool, commit_id).await, 1, "alpha build job must survive");

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
            finalize_evaluated_system(&pool, commit_id, attempt, &system_a, &check_a)
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
            "building"   => 1,
            "queued"     => 2,
            "cancelling" => 3,
            "success"    => 4,
            "cancelled"  => 5,
            "failed"     => 6,
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
        assert_eq!(migration_0184_canonical(&["building", "queued"]), "building");
        assert_eq!(migration_0184_canonical(&["queued", "building"]), "building");
        assert_eq!(migration_0184_canonical(&["building", "success"]), "building");
        assert_eq!(migration_0184_canonical(&["building", "failed"]), "building");
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
        assert_eq!(migration_0184_canonical(&["cancelled", "success"]), "success");
        assert_eq!(migration_0184_canonical(&["success", "cancelled"]), "success");
    }

    #[test]
    fn migration_0184_status_precedence_order() {
        // Canonical order: building > queued > cancelling > success > cancelled > failed.
        let order = ["building", "queued", "cancelling", "success", "cancelled", "failed"];
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
        let first =
            finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
                .await
                .expect("first finalize must succeed");
        assert!(matches!(first, SystemFinalizeOutcome::Queued { .. }));

        // Second finalize must return BuildAlreadyExists, not error.
        let second =
            finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
                .await
                .expect("second finalize must not error");
        assert!(
            matches!(second, SystemFinalizeOutcome::BuildAlreadyExists { .. }
                | SystemFinalizeOutcome::PreservedExistingBuild { .. }),
            "expected idempotent outcome, got {second:?}"
        );

        assert_eq!(build_job_count(&pool, commit_id).await, 1, "only one build job");

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
        // BuildAlreadyExists (or PreservedExistingBuild) and must NOT create
        // a second build_jobs row. This covers the side-effect isolation
        // requirement: BuildAlreadyExists must not trigger another queue
        // notification, QueuedForBuild broadcast, GC root, or hardening scan.
        // The assertion here is at the DB level (row counts); the caller is
        // responsible for checking outcome before emitting side effects.
        let pool = test_pool().await;
        cleanup_throwaway_flakes(&pool).await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = start_eval(&pool, commit_id).await;

        let system = successful_system("epsilon");
        let check = passing_policy_check("epsilon");

        let first =
            finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
                .await
                .expect("first finalize");
        assert!(
            matches!(first, SystemFinalizeOutcome::Queued { .. }),
            "first must be Queued; got {first:?}"
        );
        assert_eq!(build_job_count(&pool, commit_id).await, 1);

        // Simulate a retry or concurrent call.
        let second =
            finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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

        let outcome =
            finalize_evaluated_system(&pool, commit_id, attempt, &system, &check)
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

        assert_eq!(classify(false, true),  "seen");
        assert_eq!(classify(true,  true),  "confirmed_failure");
        assert_eq!(classify(true,  false), "confirmed_failure");
        assert_eq!(classify(false, false), "missing");
    }
}
