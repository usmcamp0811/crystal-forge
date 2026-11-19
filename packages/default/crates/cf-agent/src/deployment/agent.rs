use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use cf_config::config::{CacheType, CrystalForgeConfig, deployment::DeploymentConfig};
use cf_protocol::Uuid;
use cf_protocol::agent::{LogResponse, RuntimeCacheConfig};
use cf_protocol::deployment_failure::{
    DeploymentFailure, FailureCode, FailureDiagnostics, FailureStage,
};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

// Note: This module requires readlink_path() to be in scope
// readlink_path should be imported from the agent module where it's defined

/// Result of a deployment operation
#[derive(Debug, Clone)]
pub enum DeploymentResult {
    NoDeploymentNeeded,
    AlreadyOnTarget,
    SuccessFromCache {
        cache_url: String,
    },
    SuccessLocalBuild,
    Started {
        unit_name: String,
    },
    Failed {
        error: String,
        desired_target: String,
    },
}

impl DeploymentResult {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            DeploymentResult::NoDeploymentNeeded
                | DeploymentResult::AlreadyOnTarget
                | DeploymentResult::SuccessFromCache { .. }
                | DeploymentResult::SuccessLocalBuild
                | DeploymentResult::Started { .. }
        )
    }

    pub fn description(&self) -> String {
        match self {
            DeploymentResult::NoDeploymentNeeded => "No deployment needed".to_string(),
            DeploymentResult::AlreadyOnTarget => "Already on target".to_string(),
            DeploymentResult::SuccessFromCache { cache_url } => {
                format!("Successfully deployed from cache: {}", cache_url)
            }
            DeploymentResult::SuccessLocalBuild => {
                "Successfully deployed with local build".to_string()
            }
            DeploymentResult::Started { unit_name } => {
                format!("Deployment started in unit: {}", unit_name)
            }
            DeploymentResult::Failed {
                error,
                desired_target,
            } => {
                format!("Deployment failed for {}: {}", desired_target, error)
            }
        }
    }

    pub fn change_reason(&self) -> &'static str {
        match self {
            DeploymentResult::SuccessFromCache { .. }
            | DeploymentResult::SuccessLocalBuild
            | DeploymentResult::Started { .. } => "cf_deployment",
            _ => "heartbeat",
        }
    }
}

/// Agent deployment manager handles applying deployments from server
pub struct AgentDeploymentManager {
    config: DeploymentConfig,
    current_target: Option<String>,
    deployment_lock: Arc<Semaphore>,
    runtime_caches: Vec<RuntimeCacheConfig>,
    started_at: Instant,
}

#[derive(Debug, Serialize)]
struct DeploymentStartedReport<'a> {
    hostname: &'a str,
    target_store_path: &'a str,
}

#[derive(Deserialize)]
struct DeploymentStartedResponse {
    deployment_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
struct DeploymentFailedReport<'a> {
    hostname: &'a str,
    target_store_path: &'a str,
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<&'a DeploymentFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployment_id: Option<Uuid>,
}

/// Carries structured deployment failure data through internal anyhow contexts.
///
/// Only fixed summary text is exposed by Display. Diagnostics are separately
/// masked and are never appended to compatibility report fields.
#[derive(Debug)]
pub struct DeploymentFailureError(
    /// Contains the fixed cause and separately masked diagnostics.
    pub DeploymentFailure,
);

impl std::fmt::Display for DeploymentFailureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.code.summary())
    }
}

impl std::error::Error for DeploymentFailureError {}

fn valid_activation_target(target: &str) -> bool {
    cf_protocol::builder::is_canonical_nix_store_path(target, false)
        && target.strip_prefix("/nix/store/").is_some_and(|name| {
            name.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+-._?=".contains(&byte))
        })
}

fn activation_execution_failure(
    service_result: Option<&str>,
    exit_code: Option<&str>,
    exit_status: Option<&str>,
) -> Result<Option<DeploymentFailure>> {
    let result = service_result
        .filter(|result| !result.is_empty())
        .context("Missing systemd activation service result")?;
    if result == "success" {
        return Ok(None);
    }
    let mut typed = failure(FailureStage::Activation, FailureCode::ActivationExecution);
    // EXIT_STATUS is a signal name for killed/dumped children. Do not invent an
    // exit code or retain systemd's strings as free-form diagnostic text.
    if exit_code == Some("exited") {
        typed.exit_code = exit_status
            .filter(|status| !status.is_empty() && status.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|status| status.parse::<u8>().ok())
            .map(i32::from);
    }
    Ok(Some(typed))
}

/// Reports a detached activation's observed non-success systemd result.
///
/// Runs from the transient activation unit's ExecStopPost, independently of the
/// heartbeat agent service. Reads SERVICE_RESULT, EXIT_CODE and EXIT_STATUS;
/// successful units perform no configuration access or network request. Uses
/// the normal configuration loader, hostname, signing key and five-second POST
/// deadline. No journal output or raw systemd result text enters the report.
/// The optional ID is the matched request returned by deployment-started, not
/// an ID inferred from the target. Older servers omit it; such reports retain
/// pending-only matching and cannot correct an already-succeeded deployment.
///
/// # Errors
/// Returns an error for an invalid target, missing service result, configuration,
/// signing or delivery failure. Delivery is best-effort: systemd ignores helper
/// exit failure, and this helper neither retries activation nor queues reports.
pub async fn report_activation_result(target: &str, deployment_id: Option<Uuid>) -> Result<()> {
    anyhow::ensure!(
        valid_activation_target(target),
        "Invalid activation report target"
    );
    let result = std::env::var("SERVICE_RESULT").ok();
    let code = std::env::var("EXIT_CODE").ok();
    let status = std::env::var("EXIT_STATUS").ok();
    let Some(typed) =
        activation_execution_failure(result.as_deref(), code.as_deref(), status.as_deref())?
    else {
        return Ok(());
    };
    let error = anyhow::Error::new(DeploymentFailureError(typed));
    AgentDeploymentManager::new(DeploymentConfig::default())
        .report_deployment_failed(target, &error, deployment_id)
        .await
}

// ExecStopPost uses systemd's command parser, not a shell. The ':' prefix
// disables environment expansion and '%' must still be doubled for specifiers.
// Quoting plus C escapes preserves executable paths with spaces or punctuation.
fn systemd_command_argument(argument: &str) -> String {
    let mut escaped = String::from("\"");
    for character in argument.chars() {
        match character {
            '%' => escaped.push_str("%%"),
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            character if character.is_control() => {
                for byte in character.to_string().bytes() {
                    escaped.push_str(&format!("\\x{byte:02x}"));
                }
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn activation_callback_property(
    executable: &std::path::Path,
    target: &str,
    deployment_id: Option<Uuid>,
) -> Result<String> {
    anyhow::ensure!(
        executable.is_absolute(),
        "Activation callback requires an absolute executable"
    );
    let executable = executable
        .to_str()
        .context("Invalid activation callback executable encoding")?;
    anyhow::ensure!(
        !executable.contains('\0') && valid_activation_target(target),
        "Invalid activation callback arguments"
    );
    let mut property = format!(
        "ExecStopPost=-:{} --report-activation-result --target {}",
        systemd_command_argument(executable),
        systemd_command_argument(target)
    );
    if let Some(deployment_id) = deployment_id {
        property.push_str(&format!(" --deployment-id {deployment_id}"));
    }
    Ok(property)
}

fn activation_systemd_args(
    executable: &std::path::Path,
    target: &str,
    unit: &str,
    action: &str,
    deployment_id: Option<Uuid>,
    environment_names: impl IntoIterator<Item = String>,
) -> Result<Vec<String>> {
    anyhow::ensure!(
        !unit.is_empty()
            && unit
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte)),
        "Invalid activation unit name"
    );
    anyhow::ensure!(
        matches!(action, "switch" | "boot"),
        "Invalid activation action"
    );
    let callback = activation_callback_property(executable, target, deployment_id)?;
    let mut args = vec![
        "--unit".into(),
        unit.into(),
        "--no-block".into(),
        "--same-dir".into(),
        "--collect".into(),
        "--property".into(),
        callback,
    ];
    // Import names, never values, before '--'. systemd-run obtains each value
    // from its environment; callback configuration overrides stay out of argv.
    for name in environment_names {
        if name.starts_with("CRYSTAL_FORGE_")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            args.push(format!("--setenv={name}"));
        }
    }
    args.extend([
        "--".into(),
        format!("{target}/bin/switch-to-configuration"),
        action.into(),
    ]);
    Ok(args)
}

fn failure(stage: FailureStage, code: FailureCode) -> DeploymentFailure {
    let single_attempt = matches!(
        stage,
        FailureStage::CacheAcquisition
            | FailureStage::GenerationCreation
            | FailureStage::Activation
    )
    .then_some(1);
    DeploymentFailure {
        stage,
        code,
        exit_code: None,
        attempt_count: single_attempt,
        max_attempts: single_attempt,
        diagnostics: None,
    }
}

// Preserve the original error chain locally. Remote reporting only downcasts
// this typed context, never formats the chain or trusts an arbitrary string.
fn operation_error(error: anyhow::Error, stage: FailureStage, code: FailureCode) -> anyhow::Error {
    if error.downcast_ref::<DeploymentFailureError>().is_some() {
        error
    } else {
        error.context(DeploymentFailureError(failure(stage, code)))
    }
}

fn attempt_error(error: anyhow::Error, attempt: u32, max: u32) -> anyhow::Error {
    let mut typed = error
        .downcast_ref::<DeploymentFailureError>()
        .map(|typed| typed.0.clone())
        .unwrap_or_else(|| failure(FailureStage::CacheAcquisition, FailureCode::Unknown));
    typed.attempt_count = Some(attempt);
    typed.max_attempts = Some(max);
    error.context(DeploymentFailureError(typed))
}

fn report_failure(error: &anyhow::Error) -> DeploymentFailure {
    error
        .downcast_ref::<DeploymentFailureError>()
        .map(|typed| typed.0.clone())
        .unwrap_or_else(|| failure(FailureStage::Deployment, FailureCode::Unknown))
}

fn verification_error(error: anyhow::Error, attempt: u32, max: u32) -> anyhow::Error {
    let mut typed = failure(
        FailureStage::GenerationVerification,
        FailureCode::GenerationVerification,
    );
    typed.attempt_count = Some(attempt);
    typed.max_attempts = Some(max);
    error.context(DeploymentFailureError(typed))
}

fn command_failure(
    stage: FailureStage,
    fallback: FailureCode,
    output: &std::process::Output,
) -> DeploymentFailureError {
    // A local profile or systemd failure does not establish a remote cache
    // cause. Reuse only categories meaningful at this operation boundary.
    // SECURITY: The output is complete here, so a direct cut at the first
    // response body suffices; provider text never selects a category.
    let code = match classify_read_failure(before_response_body(&output.stderr)) {
        code @ (FailureCode::DiskFull | FailureCode::PermissionDenied) => code,
        FailureCode::NixDaemon if stage == FailureStage::GenerationCreation => {
            FailureCode::NixDaemon
        }
        _ => fallback,
    };
    let mut typed = failure(stage, code);
    typed.exit_code = output.status.code();
    // These commands have no cache credential inventory. Retain fixed cause
    // and status only rather than treating their arbitrary output as safe.
    DeploymentFailureError(typed)
}

fn run_deployment_command(
    mut command: Command,
    stage: FailureStage,
    fallback: FailureCode,
) -> Result<()> {
    let child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| operation_error(error.into(), stage, FailureCode::Spawn))?;
    let output = child
        .wait_with_output()
        .map_err(|error| operation_error(error.into(), stage, FailureCode::Wait))?;
    if !output.status.success() {
        return Err(command_failure(stage, fallback, &output).into());
    }
    Ok(())
}

#[cfg(test)]
fn deployment_failure_display(error: &anyhow::Error) -> Option<&str> {
    // SECURITY: Error text and outer anyhow contexts are not proof of redaction.
    error
        .downcast_ref::<DeploymentFailureError>()
        .map(|safe| safe.0.code.summary())
}

#[cfg(test)]
mod failure_reporting_tests {
    use super::*;

    #[test]
    fn systemd_activation_outcomes_report_only_observed_non_success() {
        assert_eq!(
            activation_execution_failure(Some("success"), Some("exited"), Some("0")).unwrap(),
            None
        );
        assert_eq!(
            activation_execution_failure(Some("success"), None, None).unwrap(),
            None
        );
        for (result, code, status, expected_exit) in [
            ("exit-code", Some("exited"), Some("17"), Some(17)),
            ("signal", Some("killed"), Some("TERM"), None),
            ("core-dump", Some("dumped"), Some("SEGV"), None),
            ("timeout", Some("killed"), Some("TERM"), None),
            ("resources", None, None, None),
            (
                "unknown-private-result",
                Some("private-code"),
                Some("private-status"),
                None,
            ),
            ("exit-code", Some("exited"), Some("999"), None),
            ("exit-code", Some("exited"), Some("-1"), None),
        ] {
            let typed = activation_execution_failure(Some(result), code, status)
                .unwrap()
                .unwrap();
            assert_eq!(typed.stage, FailureStage::Activation);
            assert_eq!(typed.code, FailureCode::ActivationExecution);
            assert_eq!(typed.exit_code, expected_exit);
            assert_eq!(
                (typed.attempt_count, typed.max_attempts),
                (Some(1), Some(1))
            );
            assert!(typed.diagnostics.is_none());
            let payload = serde_json::to_string(&DeploymentFailedReport {
                hostname: "probe",
                target_store_path: "/nix/store/target",
                error: typed.code.summary(),
                display_error: Some(typed.code.summary()),
                failure: Some(&typed),
                deployment_id: None,
            })
            .unwrap();
            assert!(!payload.contains("private"));
            assert!(!payload.contains("TERM"));
            assert!(!payload.contains("SEGV"));
        }
        // Manual invocation without a systemd result cannot fabricate failure.
        assert!(activation_execution_failure(None, None, None).is_err());
        assert!(activation_execution_failure(Some(""), Some("exited"), Some("17")).is_err());
    }

    #[test]
    fn activation_callback_uses_systemd_escaping_and_ignores_helper_failure() {
        let executable = std::path::Path::new("/nix/store/agent/bin/agent");
        assert_eq!(
            activation_callback_property(executable, "/nix/store/system", None).unwrap(),
            "ExecStopPost=-:\"/nix/store/agent/bin/agent\" --report-activation-result --target \"/nix/store/system\""
        );
        let special = std::path::Path::new("/agent dir/%n/$NAME/quote\"/back\\slash");
        assert_eq!(
            activation_callback_property(special, "/nix/store/system", None).unwrap(),
            "ExecStopPost=-:\"/agent dir/%%n/$NAME/quote\\\"/back\\\\slash\" --report-activation-result --target \"/nix/store/system\""
        );
        for target in [
            "/tmp/system",
            "/nix/store/../system",
            "/nix/store/name with spaces",
            "/nix/store/%n",
            "/nix/store/$VAR",
            "/nix/store/x;reboot",
            "/nix/store/x\n",
        ] {
            assert!(activation_callback_property(executable, target, None).is_err());
        }
        assert!(
            activation_callback_property(
                std::path::Path::new("relative-agent"),
                "/nix/store/system",
                None
            )
            .is_err()
        );
    }

    #[test]
    fn detached_unit_retains_activation_arguments_and_callback_configuration_names() {
        for action in ["switch", "boot"] {
            let args = activation_systemd_args(
                std::path::Path::new("/absolute/agent"),
                "/nix/store/system",
                "crystal-forge-deploy-123",
                action,
                None,
                [
                    "CRYSTAL_FORGE_CONFIG",
                    "CRYSTAL_FORGE__CLIENT__PRIVATE_KEY",
                    "AWS_SECRET_ACCESS_KEY",
                    "CRYSTAL_FORGE_BAD=private-value",
                ]
                .map(str::to_owned),
            )
            .unwrap();
            assert_eq!(
                &args[..6],
                [
                    "--unit",
                    "crystal-forge-deploy-123",
                    "--no-block",
                    "--same-dir",
                    "--collect",
                    "--property"
                ]
            );
            assert!(args[6].starts_with("ExecStopPost=-:"));
            assert_eq!(
                &args[7..9],
                [
                    "--setenv=CRYSTAL_FORGE_CONFIG",
                    "--setenv=CRYSTAL_FORGE__CLIENT__PRIVATE_KEY"
                ]
            );
            assert_eq!(
                &args[9..],
                [
                    "--",
                    "/nix/store/system/bin/switch-to-configuration",
                    action
                ]
            );
            assert!(!args.join(" ").contains("private-value"));
            assert!(!args.join(" ").contains("AWS_SECRET"));
        }
        assert!(
            activation_systemd_args(
                std::path::Path::new("/agent"),
                "/nix/store/system",
                "invalid%unit",
                "switch",
                None,
                []
            )
            .is_err()
        );
        assert!(
            activation_systemd_args(
                std::path::Path::new("/agent"),
                "/nix/store/system",
                "unit",
                "unexpected-action",
                None,
                []
            )
            .is_err()
        );
    }

    #[test]
    fn only_typed_failures_supply_fixed_display_summaries() {
        let raw = anyhow::anyhow!("Authenticated cache copy failed: private diagnostic");
        assert_eq!(deployment_failure_display(&raw), None);

        let safe = anyhow::Error::new(DeploymentFailureError(failure(
            FailureStage::CacheAcquisition,
            FailureCode::Dns,
        )))
        .context("private outer diagnostic");
        assert_eq!(
            deployment_failure_display(&safe),
            Some(FailureCode::Dns.summary())
        );
    }

    #[test]
    fn unmarked_report_omits_display_error() {
        let report = DeploymentFailedReport {
            hostname: "test-host",
            target_store_path: "/nix/store/test-target",
            error: "Deployment failed (diagnostic details unavailable)",
            display_error: None,
            failure: None,
            deployment_id: None,
        };
        let payload = serde_json::to_value(report).unwrap();
        assert!(payload.get("display_error").is_none());
        assert!(payload.get("failure").is_none());
    }

    #[test]
    fn every_stage_reports_fixed_summary_without_context_or_admin_lines() {
        for (stage, code) in [
            (
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration,
            ),
            (FailureStage::CacheAcquisition, FailureCode::Dns),
            (
                FailureStage::GenerationCreation,
                FailureCode::GenerationCreation,
            ),
            (
                FailureStage::GenerationVerification,
                FailureCode::GenerationVerification,
            ),
            (FailureStage::Activation, FailureCode::Activation),
            (FailureStage::Deployment, FailureCode::CurrentSystem),
        ] {
            let mut typed = failure(stage, code);
            typed.diagnostics = Some(FailureDiagnostics {
                lines: vec!["error: masked administrator detail".into()],
                cache_endpoint: None,
                suppression_reason: None,
            });
            let error = anyhow::Error::new(DeploymentFailureError(typed))
                .context("private unmasked outer context");
            let typed = report_failure(&error);
            let payload = serde_json::to_value(DeploymentFailedReport {
                hostname: "probe",
                target_store_path: "/nix/store/target",
                error: typed.code.summary(),
                display_error: Some(typed.code.summary()),
                failure: Some(&typed),
                deployment_id: None,
            })
            .unwrap();
            assert_eq!(payload["error"], code.summary());
            assert_eq!(payload["display_error"], code.summary());
            assert_eq!(
                payload["failure"]["stage"],
                serde_json::to_value(stage).unwrap()
            );
            assert!(!payload.to_string().contains("private unmasked"));
            assert!(!payload["error"].as_str().unwrap().contains("administrator"));
        }
        let fallback = report_failure(&anyhow::anyhow!("private raw error"));
        assert_eq!(fallback.stage, FailureStage::Deployment);
        assert_eq!(fallback.code, FailureCode::Unknown);
        assert!(fallback.diagnostics.is_none());
    }

    #[test]
    fn retry_context_keeps_final_cause_status_and_actual_count() {
        for attempt in 1..=3 {
            let mut typed = failure(FailureStage::CacheAcquisition, FailureCode::Authorization);
            typed.exit_code = Some(17);
            let error = attempt_error(DeploymentFailureError(typed).into(), attempt, 3)
                .context("private retry context");
            let typed = report_failure(&error);
            assert_eq!(typed.attempt_count, Some(attempt));
            assert_eq!(typed.max_attempts, Some(3));
            assert_eq!(typed.exit_code, Some(17));
            assert_eq!(typed.code, FailureCode::Authorization);
        }
        let typed = report_failure(&verification_error(anyhow::anyhow!("private path"), 7, 20));
        assert_eq!(
            (typed.attempt_count, typed.max_attempts),
            (Some(7), Some(20))
        );
    }

    #[test]
    fn generation_and_activation_exit_failures_keep_cause_but_no_raw_output() {
        use std::os::unix::process::ExitStatusExt;
        for (stage, fallback) in [
            (
                FailureStage::GenerationCreation,
                FailureCode::GenerationCreation,
            ),
            (FailureStage::Activation, FailureCode::Activation),
        ] {
            for (stderr, expected) in [
                ("error: private output", fallback),
                (
                    "error: path '/nix/store/example-system' is not valid",
                    fallback,
                ),
                ("error: HTTP error 404", fallback),
                ("error: path lacks a signature by a trusted key", fallback),
                (
                    "error: no space left on device private output",
                    FailureCode::DiskFull,
                ),
                (
                    "error: permission denied private output",
                    FailureCode::PermissionDenied,
                ),
            ] {
                let output = std::process::Output {
                    status: std::process::ExitStatus::from_raw(17 << 8),
                    stdout: b"private stdout".to_vec(),
                    stderr: stderr.as_bytes().to_vec(),
                };
                let typed = command_failure(stage, fallback, &output).0;
                assert_eq!(typed.stage, stage);
                assert_eq!(typed.code, expected);
                assert_eq!(typed.exit_code, Some(17));
                assert_eq!(
                    (typed.attempt_count, typed.max_attempts),
                    (Some(1), Some(1))
                );
                assert!(typed.diagnostics.is_none());
                assert!(!serde_json::to_string(&typed).unwrap().contains("private"));
            }
        }
    }

    #[tokio::test]
    async fn missing_cache_invalid_target_and_activation_script_are_typed() {
        let manager = AgentDeploymentManager::new(DeploymentConfig::default());
        let typed = report_failure(
            &manager
                .execute_deployment("/nix/store/absent")
                .await
                .unwrap_err(),
        );
        assert_eq!(
            (typed.stage, typed.code),
            (
                FailureStage::CacheConfiguration,
                FailureCode::MissingCacheConfiguration
            )
        );
        let typed = report_failure(
            &manager
                .execute_deployment("unsupported-target")
                .await
                .unwrap_err(),
        );
        assert_eq!(
            (typed.stage, typed.code),
            (FailureStage::Deployment, FailureCode::InvalidTarget)
        );
        let typed = report_failure(
            &manager
                .activate_configuration("/nonexistent-cf-test-target", "test")
                .await
                .unwrap_err(),
        );
        assert_eq!(
            (typed.stage, typed.code),
            (
                FailureStage::Activation,
                FailureCode::MissingActivationScript
            )
        );
        let typed = report_failure(
            &manager
                .verify_generation_created("/nonexistent-cf-test-target")
                .await
                .unwrap_err(),
        );
        assert_eq!(
            (typed.stage, typed.code),
            (
                FailureStage::GenerationVerification,
                FailureCode::GenerationVerification
            )
        );
        assert_eq!(typed.attempt_count, None);
    }
}

impl AgentDeploymentManager {
    /// Creates a deployment manager with local fallback cache settings.
    pub fn new(config: DeploymentConfig) -> Self {
        Self {
            config,
            current_target: None,
            deployment_lock: Arc::new(Semaphore::new(1)),
            runtime_caches: Vec::new(),
            started_at: Instant::now(),
        }
    }

    fn effective_runtime_cache(&self) -> Result<Option<RuntimeCacheConfig>> {
        if let Some(cache) = self.runtime_caches.first() {
            if !matches!(
                cache.cache_type.as_str(),
                "Attic" | "S3" | "Http" | "Nix" | "Niks3"
            ) {
                return Err(DeploymentFailureError(failure(
                    FailureStage::CacheConfiguration,
                    FailureCode::InvalidCacheConfiguration,
                ))
                .into());
            }
            return Ok(Some(clone_runtime_cache(cache)));
        }

        // INVARIANT: Static deployment settings cannot express Niks3 auth.
        // Do not infer a public cache when runtime credentials are unavailable.
        if self.config.cache_type == CacheType::Niks3 {
            return Err(DeploymentFailureError(failure(
                FailureStage::CacheConfiguration,
                FailureCode::MissingCacheConfiguration,
            ))
            .into());
        }
        Ok(self
            .config
            .cache_url
            .as_ref()
            .map(|cache_url| RuntimeCacheConfig {
                cache_url: cache_url.clone(),
                cache_type: format!("{:?}", self.config.cache_type),
                cache_public_key: self.config.cache_public_key.clone(),
                attic_cache_name: self.config.attic_cache_name.clone(),
                cache_public_keys: self.config.cache_public_key.iter().cloned().collect(),
                read_auth: cf_protocol::cache::CacheReadAuth::None,
            }))
    }

    /// Read the actual current system from /run/current-system
    fn get_current_system(&self) -> Result<String> {
        let target = readlink_path("/run/current-system")
            .context("Failed to read /run/current-system symlink")?;

        let target_str = target
            .to_str()
            .context("Current system path is not valid UTF-8")?
            .to_string();

        Ok(target_str)
    }

    /// Updates read-only runtime caches and applies an authorized desired target.
    ///
    /// # Errors
    /// Deployment operation failures, including current-system lookup, are
    /// returned as [`DeploymentResult::Failed`] after best-effort reporting.
    pub async fn process_heartbeat_response(
        &mut self,
        response: LogResponse,
    ) -> Result<(DeploymentResult, Option<u64>)> {
        debug!("Processing heartbeat response");

        self.runtime_caches = response.runtime_caches;
        let heartbeat_interval_secs = response.heartbeat_interval_secs;

        let Some(desired_target) = response.desired_target else {
            debug!("No desired target in heartbeat response");
            return Ok((
                DeploymentResult::NoDeploymentNeeded,
                heartbeat_interval_secs,
            ));
        };

        info!("Received desired target: {}", desired_target);

        let uptime = self.started_at.elapsed();
        if uptime < self.config.post_agent_start_deployment_delay {
            info!(
                "Deferring deployment for {} until post-agent-start delay expires ({:?} remaining)",
                desired_target,
                self.config.post_agent_start_deployment_delay - uptime
            );
            return Ok((
                DeploymentResult::NoDeploymentNeeded,
                heartbeat_interval_secs,
            ));
        }

        // Always check the actual running system, not just cached state
        // This handles agent restarts, manual switches, and detached deployments
        let actual_current = match self.get_current_system() {
            Ok(current) => current,
            Err(error) => {
                let error =
                    operation_error(error, FailureStage::Deployment, FailureCode::CurrentSystem);
                self.report_deployment_failed_best_effort(&desired_target, &error)
                    .await;
                return Ok((
                    DeploymentResult::Failed {
                        error: report_failure(&error).code.summary().to_string(),
                        desired_target,
                    },
                    heartbeat_interval_secs,
                ));
            }
        };

        if actual_current == desired_target {
            debug!("Already on target (verified via /run/current-system), skipping deployment");
            self.current_target = Some(desired_target.to_string());
            return Ok((DeploymentResult::AlreadyOnTarget, heartbeat_interval_secs));
        }

        debug!("Current system: {}", actual_current);
        debug!("Desired system: {}", desired_target);

        match self.execute_deployment(&desired_target).await {
            Ok(result) => {
                info!("Deployment completed successfully");
                self.current_target = Some(desired_target.to_string());
                Ok((result, heartbeat_interval_secs))
            }
            Err(e) => {
                error!("Deployment failed: {:#}", e);
                let error_message = report_failure(&e).code.summary().to_string();
                self.report_deployment_failed_best_effort(&desired_target, &e)
                    .await;
                Ok((
                    DeploymentResult::Failed {
                        error: error_message,
                        desired_target: desired_target.to_string(),
                    },
                    heartbeat_interval_secs,
                ))
            }
        }
    }

    async fn execute_deployment(&self, target: &str) -> Result<DeploymentResult> {
        let _permit = self.deployment_lock.acquire().await.map_err(|error| {
            operation_error(error.into(), FailureStage::Deployment, FailureCode::Unknown)
        })?;

        info!("Starting deployment execution for: {}", target);

        let is_store_path = target.starts_with("/nix/store/");

        let effective_cache = self.effective_runtime_cache()?;

        // Store paths REQUIRE cache to be configured
        if is_store_path && effective_cache.is_none() {
            return Err(DeploymentFailureError(failure(
                FailureStage::CacheConfiguration,
                FailureCode::MissingCacheConfiguration,
            ))
            .into());
        }

        let start_time = std::time::Instant::now();

        let result = if is_store_path {
            // Store paths: deploy from cache
            let Some(cache) = effective_cache else {
                return Err(DeploymentFailureError(failure(
                    FailureStage::CacheConfiguration,
                    FailureCode::MissingCacheConfiguration,
                ))
                .into());
            };
            self.deploy_store_path_from_cache(target, &cache).await?
        } else {
            return Err(DeploymentFailureError(failure(
                FailureStage::Deployment,
                FailureCode::InvalidTarget,
            ))
            .into());
        };
        let duration = start_time.elapsed();
        info!(
            "Deployment completed in {:.2} seconds",
            duration.as_secs_f64()
        );

        Ok(result)
    }

    async fn deploy_store_path_from_cache(
        &self,
        store_path: &str,
        cache: &RuntimeCacheConfig,
    ) -> Result<DeploymentResult> {
        info!("Deploying store path from cache: {}", store_path);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| {
                operation_error(error.into(), FailureStage::Deployment, FailureCode::Unknown)
            })?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        // Step 1: Copy from cache with retry logic
        info!("Starting cache copy with retry logic...");
        self.copy_from_cache_with_retry(cache, store_path).await?;

        // Step 2: Activate the configuration using systemd-run
        info!("Activating configuration via systemd-run...");
        self.activate_configuration(store_path, &unit_name).await?;

        info!("Deployment detached to systemd unit: {}", unit_name);
        Ok(DeploymentResult::Started { unit_name })
    }

    async fn copy_from_cache_with_retry(
        &self,
        cache: &RuntimeCacheConfig,
        store_path: &str,
    ) -> Result<()> {
        const MAX_RETRIES: u32 = 3;
        const BASE_RETRY_DELAY: Duration = Duration::from_secs(5);

        for attempt in 1..=MAX_RETRIES {
            // Progressive retry strategies:
            // Attempt 1: normal copy
            // Attempt 2: add --refresh to bypass stale cache metadata
            // Attempt 3: clear local nix cache directory, then retry
            let use_refresh = attempt == 2;

            match self.copy_from_cache(cache, store_path, use_refresh).await {
                Ok(()) => {
                    info!(
                        "Successfully copied {} from cache on attempt {}",
                        store_path, attempt
                    );
                    return Ok(());
                }
                Err(e) if attempt < MAX_RETRIES => {
                    let retry_delay = BASE_RETRY_DELAY.mul_f64(2_f64.powi((attempt - 1) as i32));
                    warn!(
                        "Cache copy attempt {} failed: {}. Retrying in {:.1}s...",
                        attempt,
                        e,
                        retry_delay.as_secs_f64()
                    );

                    // After second failure, clear nix cache before third attempt
                    if attempt == 2 {
                        if let Err(cache_err) = self.clear_nix_cache().await {
                            warn!("Failed to clear nix cache: {}", cache_err);
                        }
                    }

                    tokio::time::sleep(retry_delay).await;
                }
                Err(e) => {
                    error!("Cache copy failed after {} attempts: {}", MAX_RETRIES, e);
                    return Err(attempt_error(e, attempt, MAX_RETRIES)).context(format!(
                        "Failed to copy {} from cache after {} retries",
                        store_path, MAX_RETRIES
                    ));
                }
            }
        }

        Err(attempt_error(
            DeploymentFailureError(failure(
                FailureStage::CacheAcquisition,
                FailureCode::Unknown,
            ))
            .into(),
            MAX_RETRIES,
            MAX_RETRIES,
        ))
    }

    async fn clear_nix_cache(&self) -> Result<()> {
        // Try to determine the cache directory intelligently
        let cache_dir = if let Ok(home) = std::env::var("HOME") {
            format!("{}/.cache/nix", home)
        } else {
            // Fallback to common service user location
            "/var/lib/crystal-forge-agent/.cache/nix".to_string()
        };

        info!("Attempting to clear nix cache directory: {}", cache_dir);

        if std::path::Path::new(&cache_dir).exists() {
            tokio::fs::remove_dir_all(&cache_dir)
                .await
                .context(format!(
                    "Failed to remove nix cache directory: {}",
                    cache_dir
                ))?;
            info!("Successfully cleared nix cache directory: {}", cache_dir);
        } else {
            debug!("Nix cache directory does not exist: {}", cache_dir);
        }

        Ok(())
    }

    async fn copy_from_cache(
        &self,
        cache: &RuntimeCacheConfig,
        store_path: &str,
        refresh: bool,
    ) -> Result<()> {
        use std::process::Stdio;
        use tokio::process::Command as TokioCommand;

        let copy_timeout = self.config.deployment_timeout_minutes * 60;
        if cache.cache_url.trim().is_empty() {
            return Err(DeploymentFailureError(failure(
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration,
            ))
            .into());
        }
        if cache.cache_type == "Niks3"
            || !matches!(cache.read_auth, cf_protocol::cache::CacheReadAuth::None)
        {
            return copy_authenticated_cache(
                clone_runtime_cache(cache),
                store_path.to_owned(),
                refresh,
                Duration::from_secs(copy_timeout),
                std::ffi::OsString::from("nix"),
            )
            .await;
        }
        let cache_url = &cache.cache_url;
        let keys = if cache.cache_public_keys.is_empty() {
            cache.cache_public_key.iter().cloned().collect::<Vec<_>>()
        } else {
            cache.cache_public_keys.clone()
        };
        let joined_keys = keys.join(" ");
        let cache_public_key = (!joined_keys.is_empty()).then_some(joined_keys.as_str());

        let mut copy_args = vec![
            "copy".to_string(),
            "--from".to_string(),
            cache_url.to_string(),
        ];

        // Add --refresh flag to bypass stale local cache metadata
        if refresh {
            info!("Using --refresh flag to bypass stale local cache metadata");
            copy_args.push("--refresh".to_string());
        }

        // Disable HTTP/2 for Attic to avoid framing errors
        if cache.cache_type == "Attic" {
            debug!("Disabling HTTP/2 for Attic cache");
            copy_args.extend(vec![
                "--option".to_string(),
                "http2".to_string(),
                "false".to_string(),
            ]);
        }

        if let Some(public_key) = cache_public_key {
            copy_args.extend(vec![
                "--option".to_string(),
                "extra-trusted-public-keys".to_string(),
                public_key.to_string(),
            ]);
        }

        copy_args.push(store_path.to_string());

        debug!(
            "Executing: nix {}",
            shell_join(&copy_args.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        );

        let mut command = TokioCommand::new("nix");
        command.args(&copy_args).stdout(Stdio::null());
        // SECURITY: Ordinary reads intentionally retain ambient cache auth and
        // Nix configuration. Its credentials cannot be inventoried completely,
        // so only fixed classification/status may leave the capture boundary.
        run_cache_copy(
            command,
            cache,
            &known_cache_secrets(cache),
            Duration::from_secs(copy_timeout),
            false,
        )
        .await
    }

    async fn activate_configuration(&self, store_path: &str, unit_name: &str) -> Result<()> {
        let switch_script = format!("{}/bin/switch-to-configuration", store_path);

        // Verify the script exists
        if !std::path::Path::new(&switch_script).exists() {
            return Err(DeploymentFailureError(failure(
                FailureStage::Activation,
                FailureCode::MissingActivationScript,
            ))
            .into());
        }

        // Step 1: Always create generation (for both strategies)
        info!("Creating new NixOS generation...");
        self.create_generation(store_path).await.map_err(|error| {
            operation_error(
                error,
                FailureStage::GenerationCreation,
                FailureCode::GenerationCreation,
            )
        })?;
        self.verify_generation_created(store_path)
            .await
            .map_err(|error| {
                operation_error(
                    error,
                    FailureStage::GenerationVerification,
                    FailureCode::GenerationVerification,
                )
            })?;

        // Step 2: Activate based on strategy
        use cf_config::config::deployment::DeploymentStrategy;
        let action = match self.config.strategy {
            DeploymentStrategy::ImmediatePersist => {
                info!("Using immediate_persist strategy: activating now");
                "switch"
            }
            DeploymentStrategy::BootOnly => {
                info!("Using boot_only strategy: will activate on next boot");
                "boot"
            }
        };

        let deployment_id = self.report_deployment_started_best_effort(store_path).await;

        self.activate_via_systemd(store_path, unit_name, action, deployment_id)
            .await?;
        Ok(())
    }

    async fn report_deployment_started_best_effort(&self, store_path: &str) -> Option<Uuid> {
        match self.report_deployment_started(store_path).await {
            Ok(deployment_id) => deployment_id,
            Err(error) => {
                debug!(
                target_store_path = %store_path,
                ?error,
                "Failed to report deployment-started; continuing deployment"
                );
                None
            }
        }
    }

    async fn report_deployment_started(&self, store_path: &str) -> Result<Option<Uuid>> {
        let cfg = CrystalForgeConfig::load()?;
        let client_cfg = &cfg.client;
        let hostname = hostname::get()?.to_string_lossy().into_owned();
        let payload = DeploymentStartedReport {
            hostname: &hostname,
            target_store_path: store_path,
        };
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

        let (scheme, port_suffix) = match client_cfg.server_port {
            443 => ("https", "".to_string()),
            80 => ("http", "".to_string()),
            port => ("http", format!(":{}", port)),
        };
        let url = format!(
            "{}://{}{}/agent/deployment-started",
            scheme, client_cfg.server_host, port_suffix
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let response = client
            .post(url)
            .header("X-Signature", signature_b64)
            .header("X-Key-ID", hostname)
            .body(payload_json)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("server responded with {}", response.status());
        }

        // COMPATIBILITY: The status/message response from older servers has no
        // deployment_id. Never infer correlation from a shared target path.
        Ok(response
            .json::<DeploymentStartedResponse>()
            .await
            .context("Invalid deployment-started response")?
            .deployment_id)
    }

    async fn report_deployment_failed_best_effort(&self, store_path: &str, error: &anyhow::Error) {
        if let Err(report_error) = self.report_deployment_failed(store_path, error, None).await {
            debug!(
                target_store_path = %store_path,
                ?report_error,
                "Failed to report deployment-failed; deployment failure already logged locally"
            );
        }
    }

    async fn report_deployment_failed(
        &self,
        store_path: &str,
        error: &anyhow::Error,
        deployment_id: Option<Uuid>,
    ) -> Result<()> {
        let cfg = CrystalForgeConfig::load()?;
        let client_cfg = &cfg.client;
        let hostname = hostname::get()?.to_string_lossy().into_owned();
        let failure = report_failure(error);
        let display_error = Some(failure.code.summary());
        let payload = DeploymentFailedReport {
            hostname: &hostname,
            target_store_path: store_path,
            // COMPATIBILITY: Older servers display `error` directly.
            error: failure.code.summary(),
            display_error,
            failure: Some(&failure),
            deployment_id,
        };
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

        let (scheme, port_suffix) = match client_cfg.server_port {
            443 => ("https", "".to_string()),
            80 => ("http", "".to_string()),
            port => ("http", format!(":{}", port)),
        };
        let url = format!(
            "{}://{}{}/agent/deployment-failed",
            scheme, client_cfg.server_host, port_suffix
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;
        let response = client
            .post(url)
            .header("X-Signature", signature_b64)
            .header("X-Key-ID", hostname)
            .body(payload_json)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("server responded with {}", response.status());
        }

        Ok(())
    }

    /// Create a new NixOS generation
    async fn create_generation(&self, store_path: &str) -> Result<()> {
        let profile_path = "/nix/var/nix/profiles/system";

        debug!(
            "Creating generation: nix-env --profile {} --set {}",
            profile_path, store_path
        );

        let mut command = Command::new("nix-env");
        command.args(&["--profile", profile_path, "--set", store_path]);
        run_deployment_command(
            command,
            FailureStage::GenerationCreation,
            FailureCode::GenerationCreation,
        )
        .context("Failed to create NixOS generation")?;

        info!("✅ Generation created successfully");
        Ok(())
    }

    /// Verify that generation was created correctly with bounded retry for convergence
    async fn verify_generation_created(&self, store_path: &str) -> Result<()> {
        let profile_path = "/nix/var/nix/profiles/system";
        let current_system_path = "/run/current-system";

        // Canonicalize the expected store path once to ensure consistent comparison
        // (handles case where store_path could theoretically contain symlinks or relative components)
        let store_path_owned = store_path.to_string();
        let store_path_canonical =
            tokio::task::spawn_blocking(move || Self::resolve_symlink(&store_path_owned))
                .await
                .context("Task panicked while resolving target store path")
                .and_then(|resolved| resolved)
                .map_err(|error| {
                    operation_error(
                        error,
                        FailureStage::GenerationVerification,
                        FailureCode::GenerationVerification,
                    )
                })?;

        // Retry configuration: up to 20 attempts with 500ms between = 10 seconds max
        const MAX_ATTEMPTS: u32 = 20;
        const RETRY_DELAY: Duration = Duration::from_millis(500);

        for attempt in 1..=MAX_ATTEMPTS {
            // Resolve the actual store paths (follow symlinks completely)
            // Use spawn_blocking to avoid blocking Tokio worker threads with sync fs calls
            let profile_path_owned = profile_path.to_string();
            let profile_resolved =
                tokio::task::spawn_blocking(move || Self::resolve_symlink(&profile_path_owned))
                    .await
                    .context("Task panicked while resolving profile symlink")
                    .and_then(|resolved| resolved)
                    .map_err(|error| verification_error(error, attempt, MAX_ATTEMPTS))?;

            let current_system_path_owned = current_system_path.to_string();
            let current_resolved = tokio::task::spawn_blocking(move || {
                Self::resolve_symlink(&current_system_path_owned)
            })
            .await
            .context("Task panicked while resolving current-system symlink")
            .and_then(|resolved| resolved)
            .map_err(|error| verification_error(error, attempt, MAX_ATTEMPTS))?;

            debug!(
                "Verification attempt {}/{}: profile={}, current_system={}, desired={}",
                attempt, MAX_ATTEMPTS, profile_resolved, current_resolved, store_path_canonical
            );

            // Check if either the profile or current-system points to the desired target
            let profile_matches = profile_resolved == store_path_canonical;
            let current_matches = current_resolved == store_path_canonical;

            if profile_matches || current_matches {
                let which = if profile_matches && current_matches {
                    "both profile and /run/current-system"
                } else if profile_matches {
                    "profile (generation created)"
                } else {
                    "/run/current-system (live system)"
                };
                info!(
                    "✅ Generation verified: {} converged to {}",
                    which, store_path_canonical
                );
                return Ok(());
            }

            // Check if we're in a transient activatable state
            let is_activatable = profile_resolved.contains("-activatable-nixos-system-")
                || current_resolved.contains("-activatable-nixos-system-");

            if is_activatable {
                debug!(
                    "System in transient activatable state, continuing to wait for convergence..."
                );
            } else if attempt == MAX_ATTEMPTS {
                // Final attempt failed and we're not in activatable state
                return Err(verification_error(
                    anyhow::anyhow!("Generation did not converge"),
                    attempt,
                    MAX_ATTEMPTS,
                ));
            }

            // Wait before next attempt (unless this was the last attempt)
            if attempt < MAX_ATTEMPTS {
                tokio::time::sleep(RETRY_DELAY).await;
            }
        }

        Err(verification_error(
            anyhow::anyhow!("Generation did not converge"),
            MAX_ATTEMPTS,
            MAX_ATTEMPTS,
        ))
    }

    /// Resolve a symlink to its final target (equivalent to readlink -f)
    fn resolve_symlink(path: &str) -> Result<String> {
        let path_buf = std::fs::canonicalize(path)
            .with_context(|| format!("Failed to canonicalize path: {}", path))?;

        let resolved = path_buf
            .to_str()
            .context("Resolved path is not valid UTF-8")?
            .to_string();

        Ok(resolved)
    }

    /// Submits activation to systemd with an independent failure reporter.
    ///
    /// `--no-block` deliberately detaches activation. A successful submission
    /// does not prove script success. ExecStopPost reports observed execution
    /// failure even if activation stops or replaces the heartbeat agent service.
    async fn activate_via_systemd(
        &self,
        store_path: &str,
        unit_name: &str,
        action: &str,
        deployment_id: Option<Uuid>,
    ) -> Result<()> {
        let run_args = std::env::current_exe()
            .context("Failed to locate activation reporting executable")
            .and_then(|executable| {
                activation_systemd_args(
                    &executable,
                    store_path,
                    unit_name,
                    action,
                    deployment_id,
                    std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()),
                )
            })
            .map_err(|error| {
                operation_error(error, FailureStage::Activation, FailureCode::Activation)
            })?;

        debug!(
            "Executing: systemd-run {}",
            shell_join(&run_args.iter().map(String::as_str).collect::<Vec<_>>())
        );

        let mut command = Command::new("systemd-run");
        command.args(&run_args);
        run_deployment_command(command, FailureStage::Activation, FailureCode::Activation)
            .context("Failed to submit activation to systemd")?;

        Ok(())
    }

    /// Updates the manager's remembered target without activating a system.
    pub fn update_current_target(&mut self, target: Option<String>) {
        self.current_target = target;
    }
}

fn clone_runtime_cache(cache: &RuntimeCacheConfig) -> RuntimeCacheConfig {
    RuntimeCacheConfig {
        cache_type: cache.cache_type.clone(),
        cache_url: cache.cache_url.clone(),
        cache_public_key: cache.cache_public_key.clone(),
        attic_cache_name: cache.attic_cache_name.clone(),
        cache_public_keys: cache.cache_public_keys.clone(),
        read_auth: cache.read_auth.clone(),
    }
}

/// Bounds the Nix stderr tail kept to classify one failed authenticated read.
const READ_DIAGNOSTIC_TAIL_BYTES: usize = 64 * 1024;

/// Maps private Nix stderr to a fixed category for logs and failure reports.
///
/// SECURITY: Nix stderr can contain cache URLs, credential file paths and
/// provider response text. Only the returned fixed category may leave this
/// function. The input is never echoed, so unrecognized output stays redacted.
/// The cache server controls some of this text, so a category is a diagnostic
/// hint and never an authorization decision.
///
/// Order matters: a signature failure is reported before any transport
/// category, because the transfer succeeded and only trust failed. TLS and
/// HTTP status categories precede generic transport categories, because curl
/// reports some TLS rejections as a receive failure. Transport needles are the
/// curl error texts that Nix includes in `unable to download` messages.
fn classify_read_failure(stderr: &[u8]) -> FailureCode {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if has(&["restricted setting and you are not a trusted user"])
        && has(&["lacks a signature", "lacks a valid signature"])
    {
        FailureCode::UntrustedUser
    } else if has(&["lacks a signature", "lacks a valid signature"]) {
        FailureCode::SignatureVerification
    } else if has(&[
        "certificate required",
        "bad certificate",
        "problem with the local ssl certificate",
        "could not load pem client certificate",
    ]) {
        FailureCode::ClientCertificate
    } else if has(&[
        "ssl peer certificate",
        "ssl certificate problem",
        "unable to get local issuer certificate",
    ]) {
        FailureCode::ServerCertificate
    } else if has(&["http error 401", "http error 403"]) {
        FailureCode::Authorization
    } else if has(&[
        "http error 404",
        "does not exist in binary cache",
        "is not valid",
    ]) {
        FailureCode::MissingPath
    } else if has(&["http error 429"]) {
        FailureCode::RateLimited
    } else if text.match_indices("http error ").any(|(index, prefix)| {
        text[index + prefix.len()..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|status| status.parse::<u16>().ok())
            .is_some_and(|status| (500..=599).contains(&status))
    }) {
        FailureCode::ServerError
    } else if has(&["couldn't resolve host", "could not resolve host"]) {
        FailureCode::Dns
    } else if has(&[
        "couldn't connect to server",
        "could not connect to server",
        "failed to connect",
        "connection refused",
        "network is unreachable",
        "no route to host",
    ]) {
        FailureCode::Connection
    } else if has(&["timeout was reached", "timed out"]) {
        FailureCode::Timeout
    } else if has(&[
        "failure when receiving data from the peer",
        "failed sending data to the peer",
        "transferred a partial file",
        "server returned nothing",
        "connection reset by peer",
        "http2 framing layer",
        "unexpected end-of-file",
    ]) {
        FailureCode::InterruptedTransfer
    } else if has(&["hash mismatch"]) {
        FailureCode::HashMismatch
    } else if has(&["no space left on device"]) {
        FailureCode::DiskFull
    } else if has(&["daemon-socket", "cannot connect to socket"]) {
        FailureCode::NixDaemon
    } else if has(&["permission denied"]) {
        FailureCode::PermissionDenied
    } else if has(&[
        "invalid url",
        "unsupported url",
        "not a valid url",
        "invalid uri",
        "unknown store type",
    ]) {
        FailureCode::InvalidCacheConfiguration
    } else {
        FailureCode::Unknown
    }
}

/// Bounds the redacted Nix detail added to a failure report.
const REDACTED_DETAIL_MAX_LINES: usize = 3;
const REDACTED_DETAIL_MAX_LINE_CHARS: usize = 300;
const REDACTED: &str = "****";

/// One environment variable that the authenticated read child inherits.
struct InheritedVariable {
    name: &'static str,
    /// The value is a `:`-separated list. Mask each element as well.
    list: bool,
    /// The value names filesystem paths. Mask each path's parent directory too.
    parents: bool,
}

/// The only variables the authenticated read child inherits after `env_clear`.
///
/// SECURITY: This is the single source for both the child's environment and
/// the masking inventory. A variable cannot be inherited without being masked,
/// so the inventory is complete by construction. `NIX_REMOTE` can be a store
/// URL that carries credentials, and certificate locations are local paths.
const AUTHENTICATED_COPY_INHERITED_ENV: [InheritedVariable; 4] = [
    InheritedVariable {
        name: "PATH",
        list: true,
        parents: false,
    },
    InheritedVariable {
        name: "NIX_REMOTE",
        list: false,
        parents: false,
    },
    InheritedVariable {
        name: "SSL_CERT_FILE",
        list: false,
        parents: true,
    },
    InheritedVariable {
        name: "SSL_CERT_DIR",
        list: true,
        parents: true,
    },
];

/// Inherited variable names and values, as passed to one authenticated read.
type InheritedEnvironment = Vec<(&'static str, std::ffi::OsString)>;

/// Reads the inherited variables that are set, using `lookup` for each name.
fn inherited_environment(
    lookup: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> InheritedEnvironment {
    AUTHENTICATED_COPY_INHERITED_ENV
        .iter()
        .filter_map(|variable| lookup(variable.name).map(|value| (variable.name, value)))
        .collect()
}

/// Returns every value and path that the inherited environment can reveal.
///
/// Masks the whole value, each list element, and each path's parent directory
/// as [`AUTHENTICATED_COPY_INHERITED_ENV`] specifies. The root directory and an
/// empty parent are skipped: masking `/` would destroy every path separator.
fn inherited_environment_secrets(inherited: &InheritedEnvironment) -> Vec<String> {
    let mut secrets = Vec::new();
    for (name, value) in inherited {
        let value = value.to_string_lossy().into_owned();
        // A name outside the table cannot occur. Use the strictest rule if one does.
        let (list, parents) = AUTHENTICATED_COPY_INHERITED_ENV
            .iter()
            .find(|variable| variable.name == *name)
            .map_or((true, true), |variable| (variable.list, variable.parents));
        let elements: Vec<String> = if list {
            value
                .split(':')
                .filter(|element| !element.is_empty())
                .map(str::to_owned)
                .collect()
        } else {
            vec![value.clone()]
        };
        for element in elements {
            if parents && let Some(parent) = std::path::Path::new(&element).parent() {
                let parent = parent.to_string_lossy();
                if !parent.is_empty() && parent != "/" {
                    secrets.push(parent.into_owned());
                }
            }
            secrets.push(element);
        }
        secrets.push(value);
    }
    secrets
}

/// Returns every credential-bearing value this agent passed to one Nix read.
///
/// SECURITY: The list contains the configured and prepared cache URLs, Basic
/// credentials, every local credential file path and its parent directory, and
/// every inherited environment value (see [`AUTHENTICATED_COPY_INHERITED_ENV`]).
/// [`redact_nix_error_lines`] masks each value by exact match.
fn known_read_secrets(
    cache: &RuntimeCacheConfig,
    read: &cf_config::cache_credentials::PreparedCacheRead,
    inherited: &InheritedEnvironment,
) -> Vec<String> {
    let mut secrets = known_cache_secrets(cache);
    secrets.extend(inherited_environment_secrets(inherited));
    secrets.push(read.url.clone());
    let mut paths: Vec<std::path::PathBuf> = read.ca_certificate_path.iter().cloned().collect();
    paths.extend(
        read.nix_settings
            .iter()
            .filter(|(_, value)| value.starts_with('/'))
            .map(|(_, value)| std::path::PathBuf::from(value)),
    );
    for path in paths {
        if let Some(parent) = path.parent() {
            secrets.push(parent.to_string_lossy().into_owned());
        }
        secrets.push(path.to_string_lossy().into_owned());
    }
    secrets
}

fn known_cache_secrets(cache: &RuntimeCacheConfig) -> Vec<String> {
    let mut secrets = vec![cache.cache_url.clone()];
    if let Ok(url) = reqwest::Url::parse(&cache.cache_url) {
        secrets.push(url.username().to_owned());
        secrets.extend(url.password().map(str::to_owned));
        secrets.extend(url.query().map(str::to_owned));
        secrets.extend(url.fragment().map(str::to_owned));
    }
    match &cache.read_auth {
        cf_protocol::cache::CacheReadAuth::Basic { username, password } => {
            secrets.extend([username.clone(), password.clone()]);
        }
        cf_protocol::cache::CacheReadAuth::Mtls {
            client_certificate,
            client_private_key,
            ca_certificate,
        } => {
            secrets.extend([client_certificate.clone(), client_private_key.clone()]);
            secrets.extend(ca_certificate.iter().cloned());
        }
        cf_protocol::cache::CacheReadAuth::None => {}
    }
    secrets
}

fn sanitized_cache_endpoint(cache: &RuntimeCacheConfig) -> Option<String> {
    let mut url = reqwest::Url::parse(&cache.cache_url).ok()?;
    url.host_str()?;
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    let endpoint = url.to_string();
    // SECURITY: Credentials can also occur in a configured path or host. Do
    // not expose that endpoint, including when the credential is one character.
    if known_cache_secrets(cache)
        .iter()
        .skip(1)
        .any(|secret| !secret.is_empty() && endpoint.contains(secret))
    {
        return None;
    }
    (endpoint.chars().count() <= 300).then_some(endpoint)
}

/// Returns bounded, masked diagnostics only when no response body is present.
///
/// SECURITY: Nix stderr can contain cache URLs, credential file paths, redirect
/// URLs with signed query parameters, and provider response text. This function
/// suppresses all detail if a response-body delimiter is present anywhere.
/// Provider bodies can themselves contain `error:` lines and cannot be
/// distinguished from subsequent Nix diagnostics. Callers must also suppress
/// detail for incomplete captures, which may have lost the delimiter.
/// Otherwise it keeps `error:` lines, masks every `secrets` value by exact
/// match, removes URL user information, and masks every URL query string.
/// Every nonempty secret is masked, even when that reduces readability. The
/// output has at most [`REDACTED_DETAIL_MAX_LINES`] lines of
/// [`REDACTED_DETAIL_MAX_LINE_CHARS`] characters.
fn redact_nix_error_lines(stderr: &[u8], secrets: &[String]) -> Vec<String> {
    let text = String::from_utf8_lossy(stderr);
    // SECURITY: Nix's FileTransferError appends provider-controlled text after
    // this delimiter. Do not resume parsing even if another line looks like a
    // Nix diagnostic. Inline and mixed-case delimiters also fail closed.
    if text.to_ascii_lowercase().contains("response body:") {
        return Vec::new();
    }
    let mut secrets: Vec<&str> = secrets
        .iter()
        .map(String::as_str)
        .filter(|secret| !secret.is_empty())
        .collect();
    // Longest first, so a URL is masked before a shorter value inside it.
    secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
    let lines: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("error:"))
        .map(|line| {
            // SECURITY: URL scrubbing runs first. Exact masking can replace a
            // URL scheme, and URL scrubbing then would not find that URL.
            let mut line = scrub_urls(line);
            for secret in &secrets {
                // Scrubbing can change a configured URL before exact masking.
                // Mask that scrubbed spelling too, so its host/path does not
                // reappear merely because the configured URL had credentials.
                if secret.contains("://") {
                    line = line.replace(&scrub_urls(secret), REDACTED);
                }
                line = line.replace(secret, REDACTED);
            }
            line.chars().take(REDACTED_DETAIL_MAX_LINE_CHARS).collect()
        })
        .collect();
    let start = lines.len().saturating_sub(REDACTED_DETAIL_MAX_LINES);
    lines.into_iter().skip(start).collect()
}

#[cfg(test)]
fn redact_nix_errors(stderr: &[u8], secrets: &[String]) -> String {
    redact_nix_error_lines(stderr, secrets).join(" | ")
}

/// Removes user information and masks query strings in every URL in `line`.
///
/// A URL starts at `://` and ends at whitespace or a quote character.
fn scrub_urls(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(index) = rest.find("://") {
        let (head, tail) = rest.split_at(index + 3);
        out.push_str(head);
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '`'))
            .unwrap_or(tail.len());
        let (url, after) = tail.split_at(end);
        let authority_end = url.find(['/', '?', '#']).unwrap_or(url.len());
        let url = match url[..authority_end].rfind('@') {
            Some(at) => &url[at + 1..],
            None => url,
        };
        match url.find(['?', '#']) {
            Some(query) => {
                out.push_str(&url[..=query]);
                out.push_str(REDACTED);
            }
            None => out.push_str(url),
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Nix prints this delimiter before provider-controlled HTTP response text.
const RESPONSE_BODY_DELIMITER: &[u8] = b"response body:";

/// Returns the index of the first case-insensitive response-body delimiter.
fn find_response_body(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(RESPONSE_BODY_DELIMITER.len())
        .position(|window| window.eq_ignore_ascii_case(RESPONSE_BODY_DELIMITER))
}

/// Returns the complete output before its first response-body delimiter.
///
/// SECURITY: Text after the delimiter is provider-controlled. A later line can
/// imitate a Nix diagnostic, so nothing after the first delimiter may influence
/// a fixed failure category, even a line that looks genuine.
fn before_response_body(output: &[u8]) -> &[u8] {
    find_response_body(output).map_or(output, |index| &output[..index])
}

/// Captures the bounded stderr tail and whether the entire stream was retained.
#[derive(Default)]
struct ReadDiagnostics {
    tail: Vec<u8>,
    complete: bool,
    /// The last bytes before the first response-body delimiter in the whole
    /// stream. Only this text is classified. Equals `tail` when no body occurs.
    classification: Vec<u8>,
    /// A response-body delimiter occurred anywhere in the stream, including in
    /// a part that truncation later removed from `tail`.
    body_seen: bool,
}

/// Splits a stderr stream into a bounded tail and trusted classification text.
///
/// SECURITY: The delimiter is searched across the whole stream as it arrives,
/// including across chunk boundaries, not only in the retained tail. Truncation
/// therefore cannot hide a body and make provider text look like Nix output.
/// Bytes before the first delimiter are emitted by Nix or the agent; they are
/// classified even when truncation removed older bytes. Bytes after it never are.
#[derive(Default)]
struct StderrCapture {
    tail: Vec<u8>,
    truncated: bool,
    classification: Vec<u8>,
    body_seen: bool,
    /// The last `RESPONSE_BODY_DELIMITER.len() - 1` stream bytes, which can begin
    /// a delimiter that the next chunk completes.
    carry: Vec<u8>,
}

impl StderrCapture {
    fn push(&mut self, chunk: &[u8]) {
        self.tail.extend_from_slice(chunk);
        if self.tail.len() > READ_DIAGNOSTIC_TAIL_BYTES {
            self.truncated = true;
            self.tail
                .drain(..self.tail.len() - READ_DIAGNOSTIC_TAIL_BYTES);
        }
        if self.body_seen {
            return;
        }
        let mut combined = std::mem::take(&mut self.carry);
        let carried = combined.len();
        combined.extend_from_slice(chunk);
        match find_response_body(&combined) {
            Some(index) if index < carried => {
                // The delimiter began in bytes already added; remove that part.
                let keep = self.classification.len().saturating_sub(carried - index);
                self.classification.truncate(keep);
                self.body_seen = true;
            }
            Some(index) => {
                self.classification
                    .extend_from_slice(&combined[carried..index]);
                self.body_seen = true;
            }
            None => {
                self.classification.extend_from_slice(chunk);
                if self.classification.len() > READ_DIAGNOSTIC_TAIL_BYTES {
                    self.classification
                        .drain(..self.classification.len() - READ_DIAGNOSTIC_TAIL_BYTES);
                }
                let keep = RESPONSE_BODY_DELIMITER.len() - 1;
                self.carry = combined[combined.len().saturating_sub(keep)..].to_vec();
            }
        }
    }

    fn finish(self, stream_complete: bool) -> ReadDiagnostics {
        ReadDiagnostics {
            tail: self.tail,
            complete: stream_complete && !self.truncated,
            classification: self.classification,
            body_seen: self.body_seen,
        }
    }
}

impl ReadDiagnostics {
    fn structured(
        &self,
        cache: &RuntimeCacheConfig,
        secrets: &[String],
        credential_inventory_complete: bool,
    ) -> FailureDiagnostics {
        let suppression_reason = if !self.complete {
            Some("incomplete_capture".to_owned())
        } else if self.body_seen || find_response_body(&self.tail).is_some() {
            Some("provider_response_body".to_owned())
        } else if !credential_inventory_complete {
            Some("credential_inventory_unavailable".to_owned())
        } else {
            None
        };
        let suppressed = suppression_reason.is_some();
        FailureDiagnostics {
            lines: if suppressed {
                Vec::new()
            } else {
                redact_nix_error_lines(&self.tail, secrets)
            },
            cache_endpoint: if suppressed {
                None
            } else {
                sanitized_cache_endpoint(cache)
            },
            suppression_reason,
        }
    }

    #[cfg(test)]
    fn display_detail(&self, secrets: &[String]) -> String {
        // SECURITY: A truncated/read-failed capture can begin inside a provider
        // body after its delimiter. Only complete streams can supply detail.
        if self.complete {
            redact_nix_errors(&self.tail, secrets)
        } else {
            String::new()
        }
    }
}

/// Drains child stderr concurrently and records whether any bytes were lost.
///
/// CONCURRENCY: Draining prevents a full pipe from stalling Nix. The tail is
/// bounded by [`READ_DIAGNOSTIC_TAIL_BYTES`] regardless of output volume.
fn capture_stderr_tail(
    mut stderr: tokio::process::ChildStderr,
) -> tokio::task::JoinHandle<ReadDiagnostics> {
    tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut capture = StderrCapture::default();
        let mut chunk = [0u8; 8192];
        loop {
            match stderr.read(&mut chunk).await {
                Ok(0) => return capture.finish(true),
                Ok(read) => capture.push(&chunk[..read]),
                Err(_) => return capture.finish(false),
            }
        }
    })
}

// SECURITY: Detail eligibility requires the authenticated read's isolated
// environment and complete explicit credential inventory. Ordinary reads keep
// ambient auth, so their callers must pass false even for complete captures.
async fn run_cache_copy(
    mut command: tokio::process::Command,
    cache: &RuntimeCacheConfig,
    secrets: &[String],
    timeout: Duration,
    credential_inventory_complete: bool,
) -> Result<()> {
    let stage = FailureStage::CacheAcquisition;
    let mut child = command
        .kill_on_drop(true)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| operation_error(error.into(), stage, FailureCode::Spawn))?;
    let Some(pipe) = child.stderr.take() else {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(DeploymentFailureError(failure(stage, FailureCode::Capture)).into());
    };
    let mut stderr = capture_stderr_tail(pipe);
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        result => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            stderr.abort();
            let mut typed = failure(
                stage,
                if result.is_err() {
                    FailureCode::Timeout
                } else {
                    FailureCode::Wait
                },
            );
            typed.diagnostics = Some(ReadDiagnostics::default().structured(
                cache,
                secrets,
                credential_inventory_complete,
            ));
            let error = match result {
                Ok(Err(error)) => anyhow::Error::new(error),
                _ => anyhow::anyhow!("Cache copy deadline exceeded"),
            };
            return Err(error.context(DeploymentFailureError(typed)));
        }
    };
    if status.success() {
        stderr.abort();
        return Ok(());
    }
    // A descendant can retain stderr after child exit. Bound the drain wait;
    // every incomplete stream suppresses all low-level diagnostic fields.
    let tail = match tokio::time::timeout(Duration::from_secs(2), &mut stderr).await {
        Ok(Ok(tail)) => tail,
        _ => {
            stderr.abort();
            ReadDiagnostics::default()
        }
    };
    // SECURITY: Classify only text before the first response body. An empty
    // result (a body with no preceding diagnostic) yields Unknown, never a
    // category read from provider text.
    let code = classify_read_failure(&tail.classification);
    let mut typed = failure(
        stage,
        if code == FailureCode::Unknown && !tail.complete {
            FailureCode::Capture
        } else {
            code
        },
    );
    typed.exit_code = status.code();
    typed.diagnostics = Some(tail.structured(cache, secrets, credential_inventory_complete));
    Err(DeploymentFailureError(typed).into())
}

// CONCURRENCY: The detached task owns read credentials until child exit or
// timeout kill/reap, even when the deployment future is dropped. Child stderr
// is captured only for classification; see `classify_read_failure`.
async fn copy_authenticated_cache(
    cache: RuntimeCacheConfig,
    store_path: String,
    refresh: bool,
    timeout: Duration,
    program: std::ffi::OsString,
) -> Result<()> {
    copy_authenticated_cache_with_environment(
        cache,
        store_path,
        refresh,
        timeout,
        program,
        inherited_environment(|name| std::env::var_os(name)),
    )
    .await
}

// The inherited environment is a parameter so tests can supply one without
// mutating the process-global environment. Production reads the real one.
async fn copy_authenticated_cache_with_environment(
    cache: RuntimeCacheConfig,
    store_path: String,
    refresh: bool,
    timeout: Duration,
    program: std::ffi::OsString,
    inherited: InheritedEnvironment,
) -> Result<()> {
    tokio::spawn(async move {
        if cache.cache_type == "Niks3" && cache.cache_public_keys.is_empty() {
            return Err(DeploymentFailureError(failure(
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration,
            ))
            .into());
        }
        let features = if matches!(
            cache.read_auth,
            cf_protocol::cache::CacheReadAuth::Basic { .. }
        ) {
            probe_nix_read_features(&program).await.map_err(|error| {
                operation_error(
                    error,
                    FailureStage::CacheConfiguration,
                    FailureCode::InvalidCacheConfiguration,
                )
            })?
        } else {
            cf_config::cache_credentials::NixReadFeatures::default()
        };
        let read = cf_config::cache_credentials::PreparedCacheRead::new_with_nix_features(
            &cache.cache_url,
            &cache.cache_public_keys,
            &cache.read_auth,
            &features,
        )
        .map_err(|error| {
            operation_error(
                error,
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration,
            )
        })?;
        let mut command = tokio::process::Command::new(program);
        // SECURITY: Only read auth is available to this child. Ambient write
        // tokens, AWS credentials, and Nix access-token config are excluded.
        command.env_clear();
        // The same table drives the masking inventory below.
        for (name, value) in &inherited {
            command.env(name, value);
        }
        command.args([
            "copy",
            "--from",
            &read.url,
            "--option",
            "extra-trusted-public-keys",
            &read.trusted_public_keys,
            "--option",
            "require-sigs",
            "true",
            &store_path,
        ]);
        if refresh {
            command.arg("--refresh");
        }
        read.apply_to_nix_command(command.as_std_mut());
        command.stdout(std::process::Stdio::null());
        let result = run_cache_copy(
            command,
            &cache,
            &known_read_secrets(&cache, &read, &inherited),
            timeout,
            true,
        )
        .await;
        drop(read);
        result
    })
    .await
    .map_err(|error| {
        operation_error(
            error.into(),
            FailureStage::CacheAcquisition,
            FailureCode::Wait,
        )
    })?
}

/// Probes the read executable for native netrc authority protection.
///
/// Captures settings privately, with a ten-second deadline and a 2 MiB stdout
/// limit. Callers must use the same executable and PATH for the subsequent read.
/// Probe diagnostics are never included in errors because settings can contain
/// unrelated credentials. No credential files are created by this operation.
///
/// # Errors
/// Returns a [`DeploymentFailureError`] through anyhow on spawn, timeout,
/// capture, wait, nonzero exit, or malformed settings. Probe output is never
/// included in the error. A valid unpatched runtime returns absent support.
///
/// # Examples
/// ```no_run
/// # async fn example() -> anyhow::Result<()> {
/// use cf_agent::deployment::agent::probe_nix_read_features;
/// let program = std::ffi::OsStr::new("nix");
/// let features = probe_nix_read_features(program).await?;
/// if !features.supports_netrc_authority() {
///     anyhow::bail!("Basic reads require the native authority guard");
/// }
/// # Ok(())
/// # }
/// ```
pub async fn probe_nix_read_features(
    program: &std::ffi::OsStr,
) -> Result<cf_config::cache_credentials::NixReadFeatures> {
    use tokio::io::AsyncReadExt;
    // SECURITY: Bound both captured streams; never expose settings or stderr.
    const MAX_SETTINGS: u64 = 2 * 1024 * 1024;
    let mut command = tokio::process::Command::new(program);
    command.args([
        "--extra-experimental-features",
        "nix-command",
        "config",
        "show",
        "--json",
    ]);
    let mut child = command
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            operation_error(
                error.into(),
                FailureStage::CacheConfiguration,
                FailureCode::Spawn,
            )
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        DeploymentFailureError(failure(
            FailureStage::CacheConfiguration,
            FailureCode::Capture,
        ))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        DeploymentFailureError(failure(
            FailureStage::CacheConfiguration,
            FailureCode::Capture,
        ))
    })?;
    let capture = async {
        let mut settings = Vec::new();
        let mut diagnostics = Vec::new();
        let mut stdout = stdout.take(MAX_SETTINGS + 1);
        let mut stderr = stderr.take(64 * 1024 + 1);
        let (out, err) = tokio::join!(
            stdout.read_to_end(&mut settings),
            stderr.read_to_end(&mut diagnostics),
        );
        if out.is_err()
            || err.is_err()
            || settings.len() > MAX_SETTINGS as usize
            || diagnostics.len() > 64 * 1024
        {
            return Err(DeploymentFailureError(failure(
                FailureStage::CacheConfiguration,
                FailureCode::Capture,
            ))
            .into());
        }
        let status = child.wait().await.map_err(|error| {
            operation_error(
                error.into(),
                FailureStage::CacheConfiguration,
                FailureCode::Wait,
            )
        })?;
        if !status.success() {
            let mut typed = failure(
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration,
            );
            typed.exit_code = status.code();
            return Err(DeploymentFailureError(typed).into());
        }
        cf_config::cache_credentials::NixReadFeatures::from_settings_json(&settings).map_err(
            |error| {
                operation_error(
                    error,
                    FailureStage::CacheConfiguration,
                    FailureCode::InvalidCacheConfiguration,
                )
            },
        )
    };
    match tokio::time::timeout(Duration::from_secs(10), capture).await {
        Ok(Ok(features)) => Ok(features),
        result => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            match result {
                Ok(Err(error)) => Err(error),
                _ => Err(DeploymentFailureError(failure(
                    FailureStage::CacheConfiguration,
                    FailureCode::Timeout,
                ))
                .into()),
            }
        }
    }
}

fn shell_quote(s: &str) -> String {
    // Simple POSIX single-quote: ' -> '\''  (ends, escaped quote, resumes)
    if s.is_empty() {
        return "''".to_string();
    }
    if s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_./:@".contains(&b))
    {
        // Fast path: no quoting needed for common arg chars
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    }
}

#[cfg(test)]
mod niks3_tests {
    use super::*;
    use cf_protocol::cache::CacheReadAuth;
    use std::os::unix::fs::PermissionsExt;

    // Frozen public X.509 fixture from cf-server/security/cache_secrets.rs.
    // Canonical certificate-only PEM lets ownership tests reach the fake child.
    // No private key is retained for this CA fixture.
    const TEST_CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBYzCCARWgAwIBAgIUUiT5rFbIc6C8wVUsrDv3mmB3PDkwBQYDK2VwMCYxJDAi\n\
BgNVBAMMG3Rhc2s0NzAtY2VydGlmaWNhdGUtZml4dHVyZTAgFw0yNjEwMDMwMDQ1\n\
MTJaGA8yMTI2MDkwOTAwNDUxMlowJjEkMCIGA1UEAwwbdGFzazQ3MC1jZXJ0aWZp\n\
Y2F0ZS1maXh0dXJlMCowBQYDK2VwAyEArmgxjwzB+VI7yxPnKKkVj9uulx0wtyrf\n\
HC/p+1wXxpOjUzBRMB0GA1UdDgQWBBS5G+8DC/fImjFHjQtXKwSe/LTS7TAfBgNV\n\
HSMEGDAWgBS5G+8DC/fImjFHjQtXKwSe/LTS7TAPBgNVHRMBAf8EBTADAQH/MAUG\n\
AytlcANBANv0+9Ic9w11sJMah7/3hykwUUhs+iQqGZWTnWwQumZ7bkfKPl4Illm2\n\
Zh/vQ6oHa2rNyo8ob+V7jS6Zzq1/6Qk=\n\
-----END CERTIFICATE-----\n";

    struct Fixture(PathBuf);
    impl Fixture {
        fn new(script: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("cf-agent-read-test-{}", rand::random::<u64>()));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            let program = path.join("nix");
            std::fs::write(
                &program,
                script
                    .replace("FIXTURE", path.to_str().unwrap())
                    .replace("TEST_CA_CERTIFICATE", TEST_CERTIFICATE.trim_end()),
            )
            .unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn program(&self) -> std::ffi::OsString {
            self.0.join("nix").into_os_string()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn cache(private: bool) -> RuntimeCacheConfig {
        RuntimeCacheConfig {
            cache_type: "Niks3".into(),
            cache_url: "https://read.example".into(),
            cache_public_key: None,
            attic_cache_name: None,
            cache_public_keys: vec!["one:key".into(), "two:key".into()],
            read_auth: if private {
                CacheReadAuth::Mtls {
                    client_certificate: "read-cert".into(),
                    client_private_key: "read-key".into(),
                    ca_certificate: Some(TEST_CERTIFICATE.into()),
                }
            } else {
                CacheReadAuth::None
            },
        }
    }
    const PRIVATE_SCRIPT: &str = r#"#!/bin/sh
set -eu
test "$1" = copy
test "$2" = --from
test "$6" = 'one:key two:key'
test "$9" = true
case "$3" in https://read.example/*tls-certificate=*tls-private-key=*) ;; *) exit 11;; esac
directory=${NIX_SSL_CERT_FILE%/*}
test "$(cat "$directory/client-cert.pem")" = read-cert
test "$(cat "$directory/client-key.pem")" = read-key
test "$(cat "$NIX_SSL_CERT_FILE")" = 'TEST_CA_CERTIFICATE'
test "$(stat -c %a "$directory")" = 700
test "$(stat -c %a "$directory/client-key.pem")" = 600
test -z "${AWS_SECRET_ACCESS_KEY:-}"
test -z "${NIKS3_AUTH_TOKEN_FILE:-}"
test -z "${ATTIC_TOKEN:-}"
printf '%s' "$directory" > FIXTURE/credentials
"#;

    #[tokio::test]
    async fn niks3_basic_read_applies_guard_paths_and_signatures() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
set -eu
if test "$1" = --extra-experimental-features; then
    test "$2" = nix-command; test "$3" = config; test "$4" = show; test "$5" = --json
    printf '%s' '{"cf-netrc-authority":{"value":""}}'
    exit 0
fi
test "$1" = copy; test "$3" = https://read.example/
case "$*" in *private-user*|*private-password*) exit 24 ;; esac
shift 10
netrc=; authority=false; sigs=false; keys=false
while test "$#" -gt 0; do
    test "$1" = --option
    case "$2" in
    netrc-file) netrc=$3 ;;
    cf-netrc-authority) test "$3" = https://read.example; authority=true ;;
    require-sigs) test "$3" = true; sigs=true ;;
    extra-trusted-public-keys) test "$3" = 'one:key two:key'; keys=true ;;
    *) exit 11 ;;
    esac
    shift 3
done
test "$authority" = true; test "$sigs" = true; test "$keys" = true
test -f "$netrc"; test "$(stat -c %a "$netrc")" = 600
test "$(stat -c %a "${netrc%/*}")" = 700
test -z "${NIX_CONFIG:-}"; test -z "${NIKS3_AUTH_TOKEN_FILE:-}"
printf '%s' "${netrc%/*}" > FIXTURE/credentials
"#,
        );
        let mut basic = cache(false);
        basic.read_auth = CacheReadAuth::Basic {
            username: "private-user".into(),
            password: " private-password ".into(),
        };
        copy_authenticated_cache(
            basic,
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap();
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[tokio::test]
    async fn short_basic_credentials_are_redacted_before_marking_report_safe() {
        for credential in ["x", "xy", "xyz"] {
            let fixture = Fixture::new(&format!(
                "#!/bin/sh\n\
                 if test \"$1\" = --extra-experimental-features; then\n\
                 printf '%s' '{{\"cf-netrc-authority\":{{\"value\":\"\"}}}}'; exit 0; fi\n\
                 printf 'error: authentication rejected {credential}\\n' >&2\n\
                 exit 1\n"
            ));
            let mut basic = cache(false);
            basic.read_auth = CacheReadAuth::Basic {
                username: credential.into(),
                password: credential.into(),
            };
            let error = copy_authenticated_cache(
                basic,
                "/nix/store/output".into(),
                false,
                Duration::from_secs(5),
                fixture.program(),
            )
            .await
            .unwrap_err()
            .context("outer diagnostic must not enter report");
            let display = deployment_failure_display(&error).unwrap();
            let structured = report_failure(&error);
            assert_eq!(
                structured.diagnostics.as_ref().unwrap().lines,
                ["error: authentication rejected ****"]
            );
            assert_eq!(display, FailureCode::Unknown.summary());
            let report = DeploymentFailedReport {
                hostname: "probe",
                target_store_path: "/nix/store/output",
                error: display,
                display_error: Some(display),
                failure: Some(&structured),
                deployment_id: None,
            };
            let payload = serde_json::to_value(report).unwrap();
            assert_eq!(payload["display_error"], display);
            assert!(!display.contains("outer diagnostic"));
            assert!(!display.contains(&format!("rejected {credential}")));
        }
    }

    #[tokio::test]
    async fn niks3_basic_read_unpatched_runtime_never_copies() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
set -eu
test "$1" = --extra-experimental-features
printf '%s' '{"require-sigs":{"value":true}}'
"#,
        );
        let mut basic = cache(false);
        basic.read_auth = CacheReadAuth::Basic {
            username: "private-user".into(),
            password: "private-password".into(),
        };
        assert!(
            copy_authenticated_cache(
                basic,
                "/nix/store/output".into(),
                false,
                Duration::from_secs(5),
                fixture.program()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn niks3_private_read_owns_files_and_ca_until_exit() {
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\nsleep 0.05\ntest -f \"$directory/client-key.pem\"\n"
        ));
        copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            true,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap();
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[tokio::test]
    async fn niks3_public_read_preserves_multiple_keys_and_signatures() {
        let fixture = Fixture::new(
            r#"#!/bin/sh
set -eu
test "$3" = https://read.example/
test "$6" = 'one:key two:key'
test "$9" = true
test -z "${NIX_SSL_CERT_FILE:-}"
test -z "${AWS_SECRET_ACCESS_KEY:-}"
"#,
        );
        copy_authenticated_cache(
            cache(false),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn niks3_read_failures_suppress_child_secrets_and_clean_up() {
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\nprintf 'read-key write-token' >&2\nexit 17\n"
        ));
        let error = copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(!error.contains("read-key"));
        assert!(!error.contains("write-token"));
        // Unrecognized output stays redacted behind a generic category.
        assert!(error.contains("unrecognized reason; details are redacted"));
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
        let mut insecure = cache(true);
        insecure.cache_url = "http://read.example".into();
        assert!(
            copy_authenticated_cache(
                insecure,
                "/nix/store/output".into(),
                false,
                Duration::from_secs(5),
                fixture.program()
            )
            .await
            .is_err()
        );
    }

    // Reproduces the incident shape: transport and authentication succeed, then
    // Nix rejects the path because it is signed by a key CF does not trust. The
    // fake echoes private values, and 300 KB of earlier output exceeds both the
    // pipe buffer and the retained tail.
    #[tokio::test]
    async fn niks3_signature_failure_is_named_without_echoing_nix_output() {
        // The TASK-506 incident shape: no response body anywhere, so the
        // signature line is genuine Nix output and must still be classified even
        // though 300 KB of earlier output truncated the retained tail.
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\n\
             head -c 300000 /dev/zero | tr '\\0' x >&2\n\
             printf \"\\nerror: unable to download '%s/x.narinfo?X-Amz-Signature=sig-secret': secret-provider-body\\n\" \"$3\" >&2\n\
             printf \"error: cannot add path '/nix/store/abc-output' because it lacks a signature by a trusted key\\n\" >&2\n\
             exit 1\n"
        ));
        let error = copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(10),
            fixture.program(),
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(error.contains("signature verification failed"));
        assert!(error.contains("signing public key"));
        // A truncated stream cannot prove which lines were Nix diagnostics.
        // Keep the fixed classifier, never the captured detail.
        assert!(!error.contains("; nix:"), "{error}");
        assert!(!error.contains("/nix/store/abc-output"));
        for private in [
            "secret-provider-body",
            "sig-secret",
            "tls-private-key",
            "tls-certificate",
            "client-key",
            "read.example",
            "cf-cache-",
        ] {
            assert!(!error.contains(private), "{private} leaked into: {error}");
        }
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    // A cache provider controls the response body. A body line that imitates a
    // Nix diagnostic must not choose the fixed failure category.
    #[tokio::test]
    async fn response_body_text_never_selects_the_failure_category() {
        let forged = "printf 'error: cannot add path because it lacks a signature by a trusted key\\n' >&2\n";
        for (label, script, expected, reason) in [
            (
                "forged signature in an HTTP 500 body",
                format!(
                    "printf \"error: unable to download '%s/nar': HTTP error 500\\nresponse body:\\n\" \"$3\" >&2\n{forged}"
                ),
                FailureCode::ServerError,
                "provider_response_body",
            ),
            (
                "mixed-case inline delimiter",
                format!(
                    "printf \"error: unable to download '%s/nar': HTTP error 503 ReSpOnSe BoDy: \" \"$3\" >&2\n{forged}"
                ),
                FailureCode::ServerError,
                "provider_response_body",
            ),
            (
                // 300 KB of body evicts both the HTTP line and the delimiter from
                // the retained tail, leaving only provider text in it.
                "truncated body ending in a forged category",
                format!(
                    "printf \"error: unable to download '%s/nar': HTTP error 500\\nresponse body:\\n\" \"$3\" >&2\n\
                     head -c 300000 /dev/zero | tr '\\0' x >&2\n{forged}"
                ),
                FailureCode::ServerError,
                "incomplete_capture",
            ),
            (
                // No genuine diagnostic precedes the body, so the category is
                // unknown rather than anything read from provider text.
                "body without a preceding diagnostic",
                format!("printf 'response body:\\n' >&2\n{forged}"),
                FailureCode::Unknown,
                "provider_response_body",
            ),
        ] {
            let fixture = Fixture::new(&format!("{PRIVATE_SCRIPT}\n{script}exit 1\n"));
            let error = copy_authenticated_cache(
                cache(true),
                "/nix/store/output".into(),
                false,
                Duration::from_secs(10),
                fixture.program(),
            )
            .await
            .unwrap_err();
            let typed = report_failure(&error);
            assert_eq!(typed.code, expected, "{label}");
            assert_ne!(typed.code, FailureCode::SignatureVerification, "{label}");
            let diagnostics = typed.diagnostics.expect("diagnostics");
            assert_eq!(
                diagnostics.suppression_reason.as_deref(),
                Some(reason),
                "{label}"
            );
            assert!(diagnostics.lines.is_empty(), "{label}");
            let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
            assert!(!std::path::Path::new(&directory).exists());
        }
    }

    // The delimiter can arrive split across reads. Every split point must still
    // stop classification at the delimiter, never let body text through.
    #[test]
    fn stderr_capture_detects_the_delimiter_across_every_chunk_boundary() {
        let stream =
            b"error: HTTP error 500\nRESPONSE Body: error: lacks a signature by a trusted key\n";
        let start = find_response_body(stream).unwrap();
        for split in 0..=stream.len() {
            let mut capture = StderrCapture::default();
            capture.push(&stream[..split]);
            capture.push(&stream[split..]);
            let result = capture.finish(true);
            assert!(result.body_seen, "split {split}");
            assert_eq!(result.classification, &stream[..start], "split {split}");
            assert_eq!(
                classify_read_failure(&result.classification),
                FailureCode::ServerError,
                "split {split}"
            );
        }
        // One byte per read is the worst case for the carried prefix.
        let mut capture = StderrCapture::default();
        for byte in stream {
            capture.push(std::slice::from_ref(byte));
        }
        assert_eq!(capture.finish(true).classification, &stream[..start]);
    }

    #[test]
    fn stderr_capture_classifies_large_bodyless_output_and_bounds_memory() {
        // Without a body, truncation keeps the genuine last bytes classifiable.
        let mut capture = StderrCapture::default();
        for _ in 0..40 {
            capture.push(&[b'x'; 8192]);
        }
        capture.push(b"\nerror: path lacks a signature by a trusted key\n");
        let result = capture.finish(true);
        assert!(!result.complete, "truncated stream is incomplete");
        assert!(!result.body_seen);
        assert!(result.classification.len() <= READ_DIAGNOSTIC_TAIL_BYTES);
        assert!(result.tail.len() <= READ_DIAGNOSTIC_TAIL_BYTES);
        assert_eq!(
            classify_read_failure(&result.classification),
            FailureCode::SignatureVerification
        );
    }

    #[test]
    fn local_command_categories_ignore_response_body_text() {
        use std::os::unix::process::ExitStatusExt;
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: b"error: download failed\nresponse body:\nerror: No space left on device\n"
                .to_vec(),
        };
        let typed = command_failure(
            FailureStage::GenerationCreation,
            FailureCode::GenerationCreation,
            &output,
        )
        .0;
        assert_eq!(typed.code, FailureCode::GenerationCreation);
    }

    #[test]
    fn provider_response_bodies_suppress_all_diagnostic_detail() {
        for delimiter in ["response body:", "ReSpOnSe BoDy:", "error: response body:"] {
            let stderr = format!(
                "error: unable to download 'https://cache.example/nar': HTTP error 500\n\
                 {delimiter}\nerror: internal-token=secret-value-12345\n\
                 error: path lacks a signature by a trusted key\n"
            );
            assert_eq!(redact_nix_errors(stderr.as_bytes(), &[]), "");
        }
        let incomplete = ReadDiagnostics {
            tail: b"error: internal-token=secret-value-12345".to_vec(),
            complete: false,
            ..Default::default()
        };
        assert_eq!(incomplete.display_detail(&[]), "");
        assert_eq!(ReadDiagnostics::default().display_detail(&[]), "");
    }

    // Reproduces the review finding: the child inherits these variables after
    // env_clear, so a Nix error that names one must not reach Admin diagnostics.
    #[tokio::test]
    async fn inherited_environment_values_never_enter_admin_diagnostics() {
        use std::ffi::OsString;
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\n\
             printf \"error: unable to load CA bundle '/opt/private-ca/bundle.crt'\\n\" >&2\n\
             printf 'error: cannot open /opt/private-ca-dir/certs: Permission denied\\n' >&2\n\
             printf 'error: bad store ssh-ng://nixuser:hunter22@cache.internal/?ssh-key=/opt/private-ca/id\\n' >&2\n\
             exit 1\n"
        ));
        // The real PATH is required so the fake script finds its tools.
        let mut inherited = inherited_environment(|name| {
            (name == "PATH").then(|| std::env::var_os("PATH")).flatten()
        });
        inherited.extend([
            (
                "SSL_CERT_FILE",
                OsString::from("/opt/private-ca/bundle.crt"),
            ),
            (
                "SSL_CERT_DIR",
                OsString::from("/opt/private-ca-dir/certs:/srv/other/certs"),
            ),
            (
                "NIX_REMOTE",
                OsString::from(
                    "ssh-ng://nixuser:hunter22@cache.internal/?ssh-key=/opt/private-ca/id",
                ),
            ),
        ]);
        let error = copy_authenticated_cache_with_environment(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(10),
            fixture.program(),
            inherited,
        )
        .await
        .unwrap_err();
        let typed = report_failure(&error);
        assert_eq!(typed.stage, FailureStage::CacheAcquisition);
        let diagnostics = typed
            .diagnostics
            .expect("authenticated copy reports diagnostics");
        // Masking, not suppression, must be what protects these lines.
        assert_eq!(diagnostics.suppression_reason, None);
        assert!(!diagnostics.lines.is_empty());
        let shown = diagnostics.lines.join("\n");
        assert!(shown.contains("****"), "{shown}");
        for private in [
            "/opt/private-ca",
            "/opt/private-ca-dir",
            "/srv/other",
            "bundle.crt",
            "hunter22",
            "nixuser",
            "ssh-key",
        ] {
            assert!(!shown.contains(private), "{private} leaked into: {shown}");
        }
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    // The child's environment and the masking inventory share one table, so a
    // newly inherited variable cannot be omitted from masking.
    #[test]
    fn every_inherited_variable_is_masked_by_construction() {
        let inherited: InheritedEnvironment = AUTHENTICATED_COPY_INHERITED_ENV
            .iter()
            .map(|variable| {
                (
                    variable.name,
                    std::ffi::OsString::from(format!(
                        "/zz-{}/inner/value",
                        variable.name.to_lowercase()
                    )),
                )
            })
            .collect();
        assert_eq!(inherited.len(), AUTHENTICATED_COPY_INHERITED_ENV.len());
        let secrets = inherited_environment_secrets(&inherited);
        for (name, value) in &inherited {
            let line = format!("error: cannot use {}", value.to_string_lossy());
            let shown = redact_nix_error_lines(line.as_bytes(), &secrets).join(" ");
            assert!(
                !shown.contains(&format!("zz-{}", name.to_lowercase())),
                "{name} value leaked: {shown}"
            );
        }
        // The root directory must never become a secret: masking "/" would erase
        // every path separator.
        let root = vec![("SSL_CERT_FILE", std::ffi::OsString::from("/cert.pem"))];
        let secrets = inherited_environment_secrets(&root);
        assert!(
            secrets
                .iter()
                .all(|secret| secret != "/" && !secret.is_empty())
        );
        assert!(secrets.contains(&"/cert.pem".to_owned()));
    }

    #[tokio::test]
    async fn provider_error_lines_never_enter_safe_failure_reports() {
        for oversized_body in [false, true] {
            let body = if oversized_body {
                // Evict the HTTP heading and body delimiter from the tail.
                "head -c 300000 /dev/zero | tr '\\0' x >&2\nprintf '\\n' >&2\n"
            } else {
                ""
            };
            let fixture = Fixture::new(&format!(
                "{PRIVATE_SCRIPT}\n\
                 printf \"error: unable to download '%s/nar': HTTP error 500\\nresponse body:\\n\" \"$3\" >&2\n\
                 {body}\
                 printf 'error: internal-token=secret-value-12345\\n' >&2\n\
                 exit 1\n"
            ));
            let error = copy_authenticated_cache(
                cache(true),
                "/nix/store/output".into(),
                false,
                Duration::from_secs(10),
                fixture.program(),
            )
            .await
            .unwrap_err();
            let safe = deployment_failure_display(&error).unwrap();
            assert!(!safe.contains("secret-value-12345"), "{safe}");
            assert!(!safe.contains("internal-token"), "{safe}");
            assert!(!safe.contains("; nix:"), "{safe}");
            if !oversized_body {
                assert!(safe.contains("HTTP 5xx"), "{safe}");
            }
            let report = serde_json::to_string(&DeploymentFailedReport {
                hostname: "probe",
                target_store_path: "/nix/store/output",
                error: safe,
                display_error: Some(safe),
                failure: Some(&report_failure(&error)),
                deployment_id: None,
            })
            .unwrap();
            assert!(!report.contains("secret-value-12345"), "{report}");
            let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
            assert!(!std::path::Path::new(&directory).exists());
        }
    }

    #[test]
    fn short_credentials_are_masked_in_error_details() {
        for credential in ["x", "xy", "xyz"] {
            let detail = redact_nix_errors(
                format!("error: authentication failed for {credential}").as_bytes(),
                &[credential.to_string()],
            );
            assert!(!detail.contains(credential), "{detail}");
            assert!(detail.contains("****"), "{detail}");
        }
        // Empty strings must not insert masking characters between every byte.
        assert_eq!(
            redact_nix_errors(b"error: failure", &[String::new()]),
            "error: failure"
        );
    }

    #[test]
    fn all_http_server_error_statuses_are_classified() {
        for status in 500..=599 {
            assert!(
                classify_read_failure(format!("HTTP error {status}").as_bytes())
                    .summary()
                    .contains("HTTP 5xx"),
                "{status}"
            );
        }
        for status in [499, 600, 5000] {
            assert!(
                !classify_read_failure(format!("HTTP error {status}").as_bytes())
                    .summary()
                    .contains("HTTP 5xx")
            );
        }
    }

    #[test]
    fn redacted_nix_errors_mask_known_and_url_secrets() {
        let stderr = b"warning: noise https://c.example/leak\n\
            error: unable to download 'https://user:pw@other.example/nar/x?sig=abc': HTTP error 503\n\
            error: opening /run/cf-cache-1/netrc: hunter22 rejected\n\
            error: one\nerror: two\n";
        let secrets = vec![
            "https://c.example/".to_string(),
            "/run/cf-cache-1/netrc".to_string(),
            "/run/cf-cache-1".to_string(),
            "hunter22".to_string(),
            "ab".to_string(),
        ];
        let detail = redact_nix_errors(stderr, &secrets);
        assert_eq!(
            detail,
            "error: opening ****: **** rejected | error: one | error: two"
        );
        let first = redact_nix_errors(
            b"error: unable to download 'https://user:pw@other.example/nar/x?sig=abc': HTTP error 503\n",
            &secrets,
        );
        assert!(
            first.contains("'https://other.example/nar/x?****': HTTP error 503"),
            "{first}"
        );
        for private in ["user:pw", "sig=abc", "provider-body", "leak"] {
            assert!(!first.contains(private), "{private} leaked into: {first}");
        }
        assert_eq!(redact_nix_errors(b"no error lines", &secrets), "");
    }

    #[test]
    fn read_failures_map_nix_diagnostics_to_fixed_categories() {
        // The first four messages were captured from Nix 2.34.8 against a
        // private-CA, client-certificate-required cache. HTTP status texts
        // follow Nix's `HTTP error <code>` format.
        for (stderr, expected) in [
            (
                "error: cannot add path '/nix/store/x' because it lacks a signature by a trusted key",
                "signature verification failed",
            ),
            (
                "error: unable to download 'https://c/nix-cache-info': SSL peer certificate or SSH remote key was not OK (60) SSL certificate OpenSSL verify result: self-signed certificate in certificate chain (19)",
                "TLS verification of the cache server certificate failed",
            ),
            (
                "error: unable to download 'https://c/nar/x.nar.xz': Failure when receiving data from the peer (56) OpenSSL SSL_read: error:0A00045C:SSL routines::tlsv13 alert certificate required",
                "rejected or could not use the client certificate",
            ),
            (
                "error: experimental Nix feature 'nix-command' is disabled",
                "unrecognized reason; details are redacted",
            ),
            (
                "error: unable to download 'https://c/x.narinfo': HTTP error 403",
                "denied read authorization",
            ),
            (
                "error: unable to download 'https://c/nar/x.nar.xz': HTTP error 404",
                "missing from the cache",
            ),
            ("", "unrecognized reason; details are redacted"),
            (
                "error: path '/nix/store/x' is not valid",
                "missing from the cache",
            ),
            (
                "error: unable to download 'https://c/x': HTTP error 429",
                "rate-limited",
            ),
            (
                "error: unable to download 'https://c/x': HTTP error 502",
                "HTTP 5xx",
            ),
            (
                "error: unable to download 'https://c/x': Couldn't resolve host name (6)",
                "DNS resolution",
            ),
            (
                "error: unable to download 'https://c/x': Couldn't connect to server (7)",
                "could not connect",
            ),
            (
                "error: unable to download 'https://c/x': Timeout was reached (28)",
                "timed out",
            ),
            (
                "error: unable to download 'https://c/x': Failure when receiving data from the peer (56)",
                "interrupted",
            ),
            ("error: NAR hash mismatch in input", "expected hash"),
            (
                "error: writing to file: No space left on device",
                "out of disk space",
            ),
            (
                "error: cannot connect to socket at '/nix/var/nix/daemon-socket/socket'",
                "local Nix daemon",
            ),
            (
                "error: opening lock file: Permission denied",
                "denied permission",
            ),
            (
                "warning: ignoring the client-specified setting 'trusted-public-keys', because it is a restricted setting and you are not a trusted user\nerror: path lacks a signature by a trusted key",
                "not a trusted Nix user",
            ),
        ] {
            assert!(
                classify_read_failure(stderr.as_bytes())
                    .summary()
                    .contains(expected),
                "{stderr}"
            );
        }
        // A signature failure wins over transport text in the same output.
        assert!(
            classify_read_failure(
                b"warning: HTTP error 404\nerror: path lacks a signature by a trusted key"
            )
            .summary()
            .contains("signature verification failed")
        );
    }

    #[tokio::test]
    async fn niks3_read_cancellation_retains_credentials_through_child_exit() {
        let fixture = Fixture::new(&format!(
            "{PRIVATE_SCRIPT}\nsleep 0.2\ntest -f \"$directory/client-key.pem\"\ntouch FIXTURE/exited\n"
        ));
        let operation = tokio::spawn(copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        ));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !fixture.0.join("credentials").exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        operation.abort();
        assert!(std::path::Path::new(&directory).exists());
        tokio::time::timeout(Duration::from_secs(5), async {
            while std::path::Path::new(&directory).exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(fixture.0.join("exited").exists());
    }

    #[tokio::test]
    async fn niks3_read_timeout_kills_and_reaps_before_credential_cleanup() {
        let fixture = Fixture::new(&format!("{PRIVATE_SCRIPT}\nexec sleep 30\n"));
        // The script runs about a dozen subprocesses before it writes the
        // `credentials` marker read below. A 100 ms deadline can kill it first on
        // a loaded or emulated builder (ENOENT). The script sleeps 30 s, so this
        // deadline still exercises the kill-and-reap path.
        let error = copy_authenticated_cache(
            cache(true),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(5),
            fixture.program(),
        )
        .await
        .unwrap_err();
        let typed = report_failure(&error);
        assert_eq!(
            (typed.stage, typed.code),
            (FailureStage::CacheAcquisition, FailureCode::Timeout)
        );
        assert_eq!(
            (typed.attempt_count, typed.max_attempts),
            (Some(1), Some(1))
        );
        let diagnostics = typed.diagnostics.unwrap();
        assert_eq!(
            diagnostics.suppression_reason.as_deref(),
            Some("incomplete_capture")
        );
        assert!(diagnostics.lines.is_empty());
        assert!(diagnostics.cache_endpoint.is_none());
        let directory = std::fs::read_to_string(fixture.0.join("credentials")).unwrap();
        assert!(!std::path::Path::new(&directory).exists());
    }

    #[test]
    fn niks3_runtime_unknown_types_and_missing_static_auth_fail_closed() {
        let mut manager = AgentDeploymentManager::new(DeploymentConfig::default());
        let mut unknown = cache(false);
        unknown.cache_type = "unknown".into();
        manager.runtime_caches = vec![unknown];
        let typed = report_failure(&manager.effective_runtime_cache().err().unwrap());
        assert_eq!(
            (typed.stage, typed.code),
            (
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration
            )
        );
        manager.runtime_caches.clear();
        manager.config.cache_type = CacheType::Niks3;
        manager.config.cache_url = Some("https://read.example".into());
        let typed = report_failure(&manager.effective_runtime_cache().err().unwrap());
        assert_eq!(
            (typed.stage, typed.code),
            (
                FailureStage::CacheConfiguration,
                FailureCode::MissingCacheConfiguration
            )
        );
    }

    #[test]
    fn structured_details_suppress_all_low_level_fields_and_bound_lines() {
        let selected = cache(false);
        for (tail, complete, reason) in [
            (
                "error: safe heading\nReSpOnSe BoDy: error: provider-token",
                true,
                "provider_response_body",
            ),
            ("error: provider-token", false, "incomplete_capture"),
        ] {
            let diagnostics = ReadDiagnostics {
                tail: tail.as_bytes().to_vec(),
                complete,
                ..Default::default()
            }
            .structured(&selected, &[], true);
            assert_eq!(diagnostics.suppression_reason.as_deref(), Some(reason));
            assert!(diagnostics.lines.is_empty());
            assert!(diagnostics.cache_endpoint.is_none());
        }
        let tail = format!(
            "error: old\n{}",
            format!("error: {}\n", "é".repeat(400)).repeat(4)
        );
        let diagnostics = ReadDiagnostics {
            tail: tail.into_bytes(),
            complete: true,
            ..Default::default()
        }
        .structured(&selected, &[], true);
        assert_eq!(diagnostics.lines.len(), 3);
        assert!(
            diagnostics
                .lines
                .iter()
                .all(|line| line.chars().count() == 300)
        );
        assert!(diagnostics.suppression_reason.is_none());
    }

    #[test]
    fn cache_endpoint_removes_userinfo_query_fragment_and_embedded_credentials() {
        let mut selected = cache(false);
        selected.cache_url = "https://user:password@cache.example/team?token=secret#private".into();
        assert_eq!(
            sanitized_cache_endpoint(&selected).as_deref(),
            Some("https://cache.example/team")
        );
        selected.read_auth = CacheReadAuth::Basic {
            username: "x".into(),
            password: "team".into(),
        };
        assert_eq!(sanitized_cache_endpoint(&selected), None);
        selected.cache_url = "not a URL".into();
        assert_eq!(sanitized_cache_endpoint(&selected), None);
    }

    #[test]
    fn configured_url_is_masked_even_after_userinfo_and_query_scrubbing() {
        let configured =
            "https://u:pw@private.example/private-path?token=private-query#private-fragment";
        let lines = redact_nix_error_lines(
            format!("error: unable to download '{configured}'").as_bytes(),
            &[configured.into()],
        );
        assert_eq!(lines, ["error: unable to download '****'"]);
        assert_eq!(
            scrub_urls("error: https://u:pw@host/path#private-fragment"),
            "error: https://host/path#****"
        );
    }

    #[tokio::test]
    async fn incomplete_child_drain_reports_capture_without_any_low_level_detail() {
        let fixture = Fixture::new(
            "#!/bin/sh\n(sleep 3) &\nprintf 'error: provider-token\\n' >&2\nexit 17\n",
        );
        let selected = cache(false);
        let command = tokio::process::Command::new(fixture.program());
        let error = run_cache_copy(command, &selected, &[], Duration::from_secs(5), false)
            .await
            .unwrap_err();
        let typed = report_failure(&error);
        assert_eq!(typed.code, FailureCode::Capture);
        assert_eq!(typed.exit_code, Some(17));
        let diagnostics = typed.diagnostics.unwrap();
        assert_eq!(
            diagnostics.suppression_reason.as_deref(),
            Some("incomplete_capture")
        );
        assert!(diagnostics.lines.is_empty());
        assert!(diagnostics.cache_endpoint.is_none());
    }

    #[tokio::test]
    async fn ordinary_copy_suppresses_ambient_credentials_but_keeps_fixed_cause() {
        for provider_body in [false, true] {
            let fixture = Fixture::new(&format!(
                "#!/bin/sh\nprintf '%s\\n' \"error: $ATTIC_TOKEN\" \"error: $NIX_CONFIG\" >&2\nprintf 'error: HTTP error 503 https://user:pw@cache.example/nar?signature=secret\\n{}' >&2\nexit 17\n",
                if provider_body {
                    "response body: error: provider-secret\\n"
                } else {
                    ""
                },
            ));
            let mut selected = cache(false);
            selected.cache_type = "Http".into();
            let mut command = tokio::process::Command::new(fixture.program());
            command.env("ATTIC_TOKEN", "opaque-ambient-token-91723");
            command.env(
                "NIX_CONFIG",
                "access-tokens = cache.example=opaque-nix-token-83192",
            );
            let error = run_cache_copy(
                command,
                &selected,
                &known_cache_secrets(&selected),
                Duration::from_secs(5),
                false,
            )
            .await
            .unwrap_err();
            let typed = report_failure(&error);
            assert_eq!(typed.stage, FailureStage::CacheAcquisition);
            assert_eq!(typed.code, FailureCode::ServerError);
            assert_eq!(typed.exit_code, Some(17));
            assert_eq!(
                (typed.attempt_count, typed.max_attempts),
                (Some(1), Some(1))
            );
            let payload = serde_json::to_string(&DeploymentFailedReport {
                hostname: "probe",
                target_store_path: "/nix/store/output",
                error: typed.code.summary(),
                display_error: Some(typed.code.summary()),
                failure: Some(&typed),
                deployment_id: None,
            })
            .unwrap();
            for secret in [
                "opaque-ambient-token-91723",
                "opaque-nix-token-83192",
                "provider-secret",
                "user:pw",
                "signature=secret",
            ] {
                assert!(!payload.contains(secret), "{payload}");
            }
            let diagnostics = typed.diagnostics.unwrap();
            assert!(diagnostics.lines.is_empty());
            assert!(diagnostics.cache_endpoint.is_none());
            if provider_body {
                assert_eq!(
                    diagnostics.suppression_reason.as_deref(),
                    Some("provider_response_body")
                );
            } else {
                assert_eq!(
                    diagnostics.suppression_reason.as_deref(),
                    Some("credential_inventory_unavailable")
                );
            }
        }
    }

    #[tokio::test]
    async fn authenticated_spawn_and_configuration_preparation_failures_are_typed() {
        let fixture = Fixture::new("#!/bin/sh\nexit 0\n");
        let error = copy_authenticated_cache(
            cache(false),
            "/nix/store/output".into(),
            false,
            Duration::from_secs(1),
            fixture.0.join("absent-program").into_os_string(),
        )
        .await
        .unwrap_err();
        let typed = report_failure(&error);
        assert_eq!(
            (typed.stage, typed.code),
            (FailureStage::CacheAcquisition, FailureCode::Spawn)
        );
        assert!(typed.diagnostics.is_none());
        let mut selected = cache(false);
        selected.cache_public_keys.clear();
        let error = copy_authenticated_cache(
            selected,
            "/nix/store/output".into(),
            false,
            Duration::from_secs(1),
            fixture.program(),
        )
        .await
        .unwrap_err();
        let typed = report_failure(&error);
        assert_eq!(
            (typed.stage, typed.code),
            (
                FailureStage::CacheConfiguration,
                FailureCode::InvalidCacheConfiguration
            )
        );
    }

    #[test]
    fn generation_and_activation_commands_report_spawn_and_nonzero_exit() {
        let fixture = Fixture::new(
            "#!/bin/sh\nprintf 'private stdout'\nprintf 'error: private stderr' >&2\nexit 17\n",
        );
        for (stage, fallback) in [
            (
                FailureStage::GenerationCreation,
                FailureCode::GenerationCreation,
            ),
            (FailureStage::Activation, FailureCode::Activation),
        ] {
            let error = run_deployment_command(Command::new(fixture.program()), stage, fallback)
                .unwrap_err();
            let typed = report_failure(&error);
            assert_eq!(
                (typed.stage, typed.code, typed.exit_code),
                (stage, fallback, Some(17))
            );
            assert!(typed.diagnostics.is_none());
            assert!(!serde_json::to_string(&typed).unwrap().contains("private"));
            let error = run_deployment_command(
                Command::new(fixture.0.join("absent-program")),
                stage,
                fallback,
            )
            .unwrap_err();
            let typed = report_failure(&error);
            assert_eq!((typed.stage, typed.code), (stage, FailureCode::Spawn));
            assert_eq!(typed.exit_code, None);
        }
    }
}

fn shell_join(args: &[&str]) -> String {
    args.iter()
        .map(|a| shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a symlink and returns its target as a `PathBuf`.
pub fn readlink_path(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(nix::fcntl::readlink(path)?))
}
