//! Executes non-authoritative scoped Config Explorer observations.
//!
//! Capacity acquisition is nonblocking. A request becomes `running` only after
//! its cross-process advisory lock and in-process permit are held. Root, prefix,
//! option, and provenance requests use dedicated interactive capacity. The
//! complete configured-option inventory continues to use heavy-Nix capacity.

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::Duration as ChronoDuration;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::time::MissedTickBehavior;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::flake::credentials::FlakeCredentialEnv;
use crate::flake::verified_source::materialize_immutable_source;
use crate::models::config_observations::{
    ConfigObservationKind, MAX_CONFIGURED_OBSERVATION_ITEMS, validate_config_observation_payload,
};
use crate::models::deployment_policies::nix_string_pub;
use crate::models::evaluate_with_policies::{
    BoundedProcessOutput, HEAVY_NIX_ADVISORY_LOCK, heavy_nix_limiter, run_nix_command_bounded,
};
use crate::queries::config_observations::{
    ConfigObservationExecution, ConfigObservationExecutionTarget,
    complete_config_observation_failure, complete_config_observation_success,
    defer_config_observation_capacity, heartbeat_config_observation_execution,
    recover_stale_config_observations, reserve_next_config_observation,
    start_config_observation_execution,
};

const NIX_EVAL_JOBS_PROGRAM: &str = "nix-eval-jobs";
const NIX_PROGRAM: &str = "nix";
// CONCURRENCY: Every Config Inspector worker process uses this PostgreSQL key
// for shallow requests. It bounds interactive work across processes without
// making short operations wait behind full inventory or primary evaluation.
const INTERACTIVE_CONFIG_ADVISORY_LOCK: i64 = 0x4346_4346_4753;
const OBSERVATION_DEADLINE: Duration = Duration::from_secs(5 * 60);
// PERFORMANCE: A bounded automatic value preview must not compete with
// explicit inspection for the single interactive execution slot for long.
// Ten seconds is a conservative starting budget, not a measured optimum;
// see docs/config-explorer-architecture.md for the supporting probe data.
// A preview that exceeds this budget fails without consuming a retry: the
// option remains explicitly inspectable at the full deadline afterward.
const AUTOMATIC_OBSERVATION_DEADLINE: Duration = Duration::from_secs(10);
const OBSERVATION_STDOUT_LIMIT: usize = 64 * 1024 * 1024;
const OBSERVATION_STDERR_LIMIT: usize = 256 * 1024;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CONFIGURED_DIAGNOSTIC_LIMIT: usize = 128;
// A preserved evaluator failure must stay readable in a persisted error
// column and an API response. Nix traces are frequently multi-kilobyte.
const OBSERVER_ERROR_EXCERPT_LIMIT: usize = 512;
const STALE_EXECUTION_THRESHOLD: ChronoDuration = ChronoDuration::minutes(10);

fn interactive_config_limiter() -> Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMITER.get_or_init(|| Arc::new(Semaphore::new(1))).clone()
}

/// Attempts one scoped request before the optional complete-inspection queue.
///
/// Returns `true` when a scoped candidate existed, including a capacity miss.
/// The caller can use this result to preserve scoped queue priority.
pub(crate) async fn process_one_config_observation(
    pool: &PgPool,
    source_archive_root: &Path,
) -> bool {
    if let Err(error) = recover_stale_config_observations(pool, STALE_EXECUTION_THRESHOLD).await {
        warn!(%error, "config_observation_stale_recovery_failed");
    }
    let target = match reserve_next_config_observation(pool).await {
        Ok(Some(target)) => target,
        Ok(None) => return false,
        Err(error) => {
            warn!(%error, "config_observation_reserve_failed");
            return true;
        }
    };
    if let Err(error) = execute_reserved_config_observation(pool, source_archive_root, target).await
    {
        warn!(%error, "config_observation_execution_failed");
    }
    true
}

async fn execute_reserved_config_observation(
    pool: &PgPool,
    source_archive_root: &Path,
    target: ConfigObservationExecutionTarget,
) -> Result<()> {
    let mut lock_conn = pool
        .acquire()
        .await
        .context("acquire Config observation lock session")?;
    let capacity_lock = if target.kind == ConfigObservationKind::ConfiguredIndex {
        HEAVY_NIX_ADVISORY_LOCK
    } else {
        INTERACTIVE_CONFIG_ADVISORY_LOCK
    };
    let capacity_locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(capacity_lock)
        .fetch_one(&mut *lock_conn)
        .await
        .context("try Config observation heavy-Nix lock")?;
    if !capacity_locked {
        defer_config_observation_capacity(pool, target.request_id).await?;
        return Ok(());
    }
    let permit = match if target.kind == ConfigObservationKind::ConfiguredIndex {
        heavy_nix_limiter().try_acquire_owned()
    } else {
        interactive_config_limiter().try_acquire_owned()
    } {
        Ok(permit) => permit,
        Err(_) => {
            release_capacity_lock(&mut lock_conn, capacity_lock).await;
            defer_config_observation_capacity(pool, target.request_id).await?;
            return Ok(());
        }
    };
    let execution_id = Uuid::new_v4();
    if let Err(error) =
        crate::queries::cve_scans::acquire_execution_lock(&mut lock_conn, execution_id).await
    {
        release_capacity_lock(&mut lock_conn, capacity_lock).await;
        let _ = lock_conn.close().await;
        return Err(error.context("acquire Config observation execution lock"));
    }
    let execution = match start_config_observation_execution(pool, target, execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            release_capacity_lock(&mut lock_conn, capacity_lock).await;
            crate::queries::cve_scans::release_execution_lock_or_close(lock_conn, execution_id)
                .await;
            drop(permit);
            return Ok(());
        }
        Err(error) => {
            release_capacity_lock(&mut lock_conn, capacity_lock).await;
            let _ = lock_conn.close().await;
            drop(permit);
            return Err(error);
        }
    };

    let completion = match run_observation(pool, source_archive_root, &execution).await {
        Ok(payload) => {
            let persistence_started_at = Instant::now();
            let result = complete_config_observation_success(pool, &execution, &payload)
                .await
                .map(|_| ());
            debug!(
                request_id = %execution.target.request_id,
                elapsed_ms = persistence_started_at.elapsed().as_millis(),
                "config_observation_persistence_finished"
            );
            result
        }
        Err(error) => {
            let safe_error = persisted_observer_failure(&error);
            debug!(
                request_id = %execution.target.request_id,
                error = %safe_error,
                "scoped Config observation failed"
            );
            complete_config_observation_failure(pool, &execution, &safe_error)
                .await
                .map(|_| ())
        }
    };
    release_capacity_lock(&mut lock_conn, capacity_lock).await;
    crate::queries::cve_scans::release_execution_lock_or_close(lock_conn, execution_id).await;
    drop(permit);
    completion
}

async fn release_capacity_lock(conn: &mut PgConnection, lock: i64) {
    if !matches!(
        sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
            .bind(lock)
            .fetch_one(&mut *conn)
            .await,
        Ok(true)
    ) {
        warn!(
            lock,
            "Config observation capacity lock release was not confirmed"
        );
    }
}

async fn run_observation(
    pool: &PgPool,
    source_archive_root: &Path,
    execution: &ConfigObservationExecution,
) -> Result<Value> {
    let source_started_at = Instant::now();
    let credentials = FlakeCredentialEnv::load(pool, execution.target.flake_id).await?;
    let immutable_source = materialize_immutable_source(
        pool,
        execution.target.commit_id,
        source_archive_root,
        &execution.target.repo_url,
        &execution.target.revision,
        credentials.as_ref(),
    )
    .await
    .context("materialize Config observation immutable source")?;
    debug!(
        request_id = %execution.target.request_id,
        elapsed_ms = source_started_at.elapsed().as_millis(),
        "config_observation_source_ready"
    );
    let flake_ref = cf_protocol::builder::nar_qualified_store_flake_ref(
        &immutable_source.server_store_path,
        &immutable_source.nar_hash,
    );
    let command = if execution.target.kind == ConfigObservationKind::ConfiguredIndex {
        let expression = build_observer_expression(&flake_ref, &execution.target);
        let command = build_observer_command(Path::new(NIX_EVAL_JOBS_PROGRAM), &expression);
        return run_observation_command(pool, execution, command).await;
    } else {
        build_shallow_observer_command(Path::new(NIX_PROGRAM), &flake_ref, &execution.target)
    };
    run_observation_command(pool, execution, command).await
}

async fn run_observation_command(
    pool: &PgPool,
    execution: &ConfigObservationExecution,
    mut command: Command,
) -> Result<Value> {
    let evaluation_started_at = Instant::now();
    // PERFORMANCE: Only a bounded automatic value preview uses the short
    // deadline. Explicit inspection, provenance, tree pages, and the
    // complete configured index keep the existing generous deadline.
    let deadline = if execution.target.is_automatic {
        AUTOMATIC_OBSERVATION_DEADLINE
    } else {
        OBSERVATION_DEADLINE
    };
    let mut run = Box::pin(run_nix_command_bounded(
        &mut command,
        "scoped Config observer",
        deadline,
        OBSERVATION_STDOUT_LIMIT,
        OBSERVATION_STDERR_LIMIT,
    ));
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Skip);
    heartbeat.tick().await;
    let output = loop {
        tokio::select! {
            result = &mut run => break result?,
            _ = heartbeat.tick() => {
                if !heartbeat_config_observation_execution(
                    pool,
                    execution.target.request_id,
                    execution.execution_id,
                ).await? {
                    bail!("Config observation execution ownership was lost");
                }
            }
        }
    };
    debug!(
        request_id = %execution.target.request_id,
        kind = execution.target.kind.as_str(),
        elapsed_ms = evaluation_started_at.elapsed().as_millis(),
        "config_observation_nix_finished"
    );
    if execution.target.kind == ConfigObservationKind::ConfiguredIndex {
        reconcile_observer_output(output, execution)
    } else {
        reconcile_shallow_observer_output(output, execution)
    }
}

fn build_observer_expression(flake_ref: &str, target: &ConfigObservationExecutionTarget) -> String {
    let observer = include_str!("../models/config_observer.nix");
    let shallow_observer = include_str!("../models/config_shallow_observer.nix");
    let encoder = include_str!("../models/config_value_encoding.nix");
    format!(
        "let\n  flake = builtins.getFlake {flake_ref};\n  configuration = builtins.getAttr {configuration} flake.nixosConfigurations;\n  encodeValue = ({encoder}) configuration.pkgs.lib;\nin ({observer}) {{ inherit flake configuration encodeValue; targetKey = builtins.hashString \"sha256\" (builtins.toJSON [ {flake_ref} {configuration} configuration.config.system.build.toplevel.drvPath ]); operation = {operation}; path = [ {path} ]; childOffset = {child_offset}; shallowObserver = ({shallow_observer}); }}",
        flake_ref = nix_string_pub(flake_ref),
        configuration = nix_string_pub(&target.configuration_name),
        operation = nix_string_pub(target.kind.as_str()),
        path = target
            .path_components
            .iter()
            .map(|component| nix_string_pub(component))
            .collect::<Vec<_>>()
            .join(" "),
        child_offset = target.child_offset,
        shallow_observer = shallow_observer,
    )
}

fn build_observer_command(program: &Path, expression: &str) -> Command {
    let mut command = Command::new(program);
    command.args([
        "--expr",
        expression,
        "--option",
        "pure-eval",
        "true",
        "--meta",
        "--apply",
        "derivation: derivation.meta.crystalForgeConfigObservation",
        "--option",
        "experimental-features",
        "nix-command flakes",
        "--workers",
        "2",
    ]);
    command.env_remove("NETRC");
    command.env_remove("GIT_SSH_COMMAND");
    command
}

fn build_shallow_observer_command(
    program: &Path,
    flake_ref: &str,
    target: &ConfigObservationExecutionTarget,
) -> Command {
    let observer = include_str!("../models/config_shallow_observer.nix");
    let encoder = include_str!("../models/config_value_encoding.nix");
    let expression = format!(
        "{{ flakeRef, configurationName, operation, path, childOffset }}:\nlet\n  flake = builtins.getFlake flakeRef;\n  configuration = builtins.getAttr configurationName flake.nixosConfigurations;\n  encodeValue = ({encoder}) configuration.pkgs.lib;\nin ({observer}) {{ inherit configuration operation path childOffset encodeValue; }}"
    );
    let path = target
        .path_components
        .iter()
        .map(|component| nix_string_pub(component))
        .collect::<Vec<_>>()
        .join(" ");
    let application = format!(
        "observer: observer {{ flakeRef = {}; configurationName = {}; operation = {}; path = [ {} ]; childOffset = {}; }}",
        nix_string_pub(flake_ref),
        nix_string_pub(&target.configuration_name),
        nix_string_pub(target.kind.as_str()),
        path,
        target.child_offset,
    );
    let mut command = Command::new(program);
    command.args([
        "eval",
        "--extra-experimental-features",
        "nix-command flakes",
        "--option",
        "pure-eval",
        "true",
        "--option",
        "allow-import-from-derivation",
        "true",
        "--option",
        "eval-cache",
        "false",
        "--no-update-lock-file",
        "--no-write-lock-file",
        "--json",
        "--expr",
        &expression,
        "--apply",
        &application,
    ]);
    command.env_remove("NETRC");
    command.env_remove("GIT_SSH_COMMAND");
    command
}

fn reconcile_shallow_observer_output(
    output: BoundedProcessOutput,
    execution: &ConfigObservationExecution,
) -> Result<Value> {
    if output.stdout.is_truncated() {
        bail!("Config observer output exceeded its bound");
    }
    if !output.status.success() {
        bail!("Config observer process failed");
    }
    let payload: Value = serde_json::from_slice(&output.stdout.bytes)
        .context("parse direct shallow Config observation")?;
    let mut redacted = crate::security::snapshot_redaction::redact_json(&payload);
    if execution.target.kind == ConfigObservationKind::Option {
        if let Some(value) = redacted.get_mut("value") {
            *value = crate::security::snapshot_redaction::redact_option_value(
                &execution.target.path_components.join("."),
                value,
            );
        }
    }
    validate_config_observation_payload(
        execution.target.kind,
        &execution.target.path_components,
        execution.target.child_offset,
        &redacted,
    )?;
    Ok(redacted)
}

/// Returns bounded, redacted evaluator text for an operator-facing failure.
///
/// Evaluator errors can quote option values, module sources, and repository
/// URLs. The excerpt is redacted before it can reach a persisted failure
/// message, an API response, or a log record, and it is bounded so a large
/// Nix trace cannot dominate stored failure text.
fn observer_error_excerpt(error: &str) -> String {
    let collapsed = error.split_whitespace().collect::<Vec<_>>().join(" ");
    let redacted = crate::security::snapshot_redaction::redact_text(&collapsed);
    if redacted.chars().count() <= OBSERVER_ERROR_EXCERPT_LIMIT {
        return redacted;
    }
    let mut excerpt: String = redacted
        .chars()
        .take(OBSERVER_ERROR_EXCERPT_LIMIT)
        .collect();
    excerpt.push_str("...");
    excerpt
}

/// Returns the stable failure category with a bounded, redacted cause.
///
/// The returned text is safe for request persistence and API display. The
/// stable prefix lets clients classify the operation, while the excerpt keeps
/// evaluator failures actionable without exposing credentials or unbounded
/// Nix traces.
fn persisted_observer_failure(error: &anyhow::Error) -> String {
    let cause = observer_error_excerpt(&format!("{error:#}"));
    if cause.trim().is_empty() {
        "Config observation evaluation failed".to_string()
    } else {
        format!("Config observation evaluation failed: {cause}")
    }
}

fn reconcile_observer_output(
    output: BoundedProcessOutput,
    execution: &ConfigObservationExecution,
) -> Result<Value> {
    if output.stdout.is_truncated() {
        bail!("Config observer output exceeded its bound");
    }
    if !output.status.success() && execution.target.kind != ConfigObservationKind::ConfiguredIndex {
        bail!("Config observer process failed");
    }
    let mut successful_carrier: Option<String> = None;
    let mut observation = None;
    let mut configured = Vec::new();
    let mut diagnostics = Vec::new();
    let mut diagnostics_truncated = false;
    let mut index = None;
    for line in output
        .stdout
        .bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let row: Value = serde_json::from_slice(line).context("parse Config observer JSONL")?;
        let attr = row
            .get("attr")
            .and_then(Value::as_str)
            .context("Config observer result attr")?;
        if let Some(error) = row.get("error").and_then(Value::as_str) {
            if execution.target.kind == ConfigObservationKind::ConfiguredIndex
                && attr.starts_with("configured_")
            {
                if diagnostics.len() < CONFIGURED_DIAGNOSTIC_LIMIT {
                    diagnostics.push(json!({
                        "key": attr.trim_start_matches("configured_"),
                        "code": "configured_classifier_failed",
                        "message": "Configured-state classification failed"
                    }));
                } else {
                    diagnostics_truncated = true;
                }
                continue;
            }
            // `nix-eval-jobs` reports a failed job as an `error` record and
            // still exits 0. Losing this text leaves an operator with a
            // generic failure for an exactly diagnosable launch or evaluation
            // fault, so the bounded redacted cause is preserved here.
            bail!(
                "Config observer job {attr} failed: {}",
                observer_error_excerpt(error)
            );
        }
        let drv = row
            .get("drvPath")
            .and_then(Value::as_str)
            .context("Config observer carrier")?;
        if drv != execution.target.carrier_drv_path {
            bail!("Config observer carrier does not match request identity");
        }
        if successful_carrier
            .replace(drv.to_string())
            .is_some_and(|prior| prior != drv)
        {
            bail!("Config observer jobs used different carriers");
        }
        let payload = row
            .get("extraValue")
            .cloned()
            .context("Config observer payload")?;
        match execution.target.kind {
            ConfigObservationKind::ConfiguredIndex => {
                if attr == "__crystalForgeConfiguredIndex" {
                    index = Some(payload);
                } else if payload.get("configured").and_then(Value::as_bool) == Some(true) {
                    configured.push(json!({
                        "path_components": payload.get("path_components").cloned().unwrap_or(Value::Null),
                        "key": payload.get("key").cloned().unwrap_or(Value::Null)
                    }));
                }
            }
            _ => {
                if attr != "observation" || observation.replace(payload).is_some() {
                    bail!("Config observer returned an unexpected result set");
                }
            }
        }
    }
    let payload = if execution.target.kind == ConfigObservationKind::ConfiguredIndex {
        // A failing index job bails above with its preserved cause, so this
        // path means the evaluator returned no index record at all.
        let mut index = index.context(
            "configured index job produced no result record; classification did not run",
        )?;
        let object = index
            .as_object_mut()
            .context("configured index payload is not an object")?;
        configured.sort_by_key(|entry| entry.get("path_components").map(Value::to_string));
        let total_configured = configured.len();
        configured.truncate(MAX_CONFIGURED_OBSERVATION_ITEMS);
        object.insert(
            "total_configured".to_string(),
            Value::from(total_configured),
        );
        object.insert(
            "configured_truncated".to_string(),
            Value::Bool(total_configured > configured.len()),
        );
        object.insert("configured".to_string(), Value::Array(configured));
        object.insert(
            "classifier_diagnostics".to_string(),
            Value::Array(diagnostics),
        );
        object.insert(
            "classifier_diagnostics_truncated".to_string(),
            Value::Bool(diagnostics_truncated),
        );
        index
    } else {
        observation.context("Config observation result is missing")?
    };
    let mut redacted = crate::security::snapshot_redaction::redact_json(&payload);
    if execution.target.kind == ConfigObservationKind::Option {
        if let Some(value) = redacted.get_mut("value") {
            *value = crate::security::snapshot_redaction::redact_option_value(
                &execution.target.path_components.join("."),
                value,
            );
        }
    }
    validate_config_observation_payload(
        execution.target.kind,
        &execution.target.path_components,
        execution.target.child_offset,
        &redacted,
    )?;
    Ok(redacted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config_inspector::option_key;
    use crate::models::evaluate_with_policies::CappedOutput;
    use crate::queries::config_observations::{
        CreateConfigObservationOutcome, create_or_reuse_config_observation_request,
    };
    use std::os::unix::process::ExitStatusExt;

    fn execution(
        kind: ConfigObservationKind,
        path_components: Vec<String>,
    ) -> ConfigObservationExecution {
        ConfigObservationExecution {
            target: ConfigObservationExecutionTarget {
                request_id: Uuid::new_v4(),
                commit_id: 1,
                derivation_id: 2,
                configuration_name: "host".to_string(),
                carrier_drv_path: "/nix/store/carrier.drv".to_string(),
                revision: "a".repeat(40),
                repo_url: "https://example.test/repo.git".to_string(),
                flake_id: 3,
                kind,
                path_components,
                child_offset: 0,
                is_automatic: false,
            },
            execution_id: Uuid::new_v4(),
            attempts: 1,
        }
    }

    fn successful_output(rows: Vec<Value>) -> BoundedProcessOutput {
        let mut bytes = rows
            .into_iter()
            .flat_map(|row| [row.to_string().into_bytes(), vec![b'\n']])
            .flatten()
            .collect::<Vec<_>>();
        let total_bytes = bytes.len();
        BoundedProcessOutput {
            status: std::process::ExitStatus::from_raw(0),
            stdout: CappedOutput {
                bytes: std::mem::take(&mut bytes),
                total_bytes,
            },
            stderr: CappedOutput::default(),
        }
    }

    #[test]
    fn reconciliation_accepts_every_operation_payload_contract() {
        let key = "a".repeat(64);
        let cases = [
            (
                execution(ConfigObservationKind::Root, Vec::new()),
                serde_json::json!({
                    "kind": "root", "path_components": [], "children": [],
                    "child_offset": 0, "children_truncated": false, "total_children": 0
                }),
            ),
            (
                execution(ConfigObservationKind::Prefix, vec!["services".to_string()]),
                serde_json::json!({
                    "kind": "prefix", "path_components": ["services"], "children": [],
                    "child_offset": 0, "children_truncated": false, "total_children": 0
                }),
            ),
            (
                execution(
                    ConfigObservationKind::Option,
                    vec!["services".to_string(), "nginx".to_string()],
                ),
                serde_json::json!({
                    "kind": "option", "path_components": ["services", "nginx"],
                    "key": key, "declared_type": "boolean", "is_defined": true,
                    "highest_prio": 100, "value": {"kind": "scalar", "value": true}
                }),
            ),
            (
                execution(
                    ConfigObservationKind::Provenance,
                    vec!["services".to_string(), "nginx".to_string()],
                ),
                serde_json::json!({
                    "kind": "provenance", "path_components": ["services", "nginx"],
                    "key": key, "definitions": [{"source_path": "/flake/module.nix", "priority": 100}],
                    "definitions_truncated": false, "total_definitions": 1
                }),
            ),
        ];

        for (execution, payload) in cases {
            let result = reconcile_observer_output(
                successful_output(vec![serde_json::json!({
                    "attr": "observation",
                    "drvPath": execution.target.carrier_drv_path,
                    "extraValue": payload
                })]),
                &execution,
            )
            .unwrap_or_else(|error| {
                panic!("{:?} should reconcile: {error:#}", execution.target.kind)
            });
            validate_config_observation_payload(
                execution.target.kind,
                &execution.target.path_components,
                execution.target.child_offset,
                &result,
            )
            .unwrap();
        }

        let configured = execution(ConfigObservationKind::ConfiguredIndex, Vec::new());
        let result = reconcile_observer_output(
            successful_output(vec![
                serde_json::json!({
                    "attr": "__crystalForgeConfiguredIndex",
                    "drvPath": configured.target.carrier_drv_path,
                    "extraValue": {
                        "kind": "configured_index", "path_components": [], "total_traversed": 1,
                        "diagnostics": [], "diagnostics_truncated": false
                    }
                }),
                serde_json::json!({
                    "attr": format!("configured_{key}"),
                    "drvPath": configured.target.carrier_drv_path,
                    "extraValue": {
                        "kind": "configured_classifier", "path_components": ["services", "nginx"],
                        "key": key, "configured": true
                    }
                }),
            ]),
            &configured,
        )
        .expect("configured index should reconcile");
        validate_config_observation_payload(
            ConfigObservationKind::ConfiguredIndex,
            &[],
            0,
            &result,
        )
        .expect("configured index should validate");
    }

    #[test]
    fn generated_expression_embeds_validated_structured_selection_for_pure_eval() {
        let mut execution = execution(
            ConfigObservationKind::ConfiguredIndex,
            vec!["safe".to_string()],
        );
        execution.target.child_offset = 7;
        let expression = build_observer_expression(
            "git+https://example.test/repo?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &execution.target,
        );
        assert!(!expression.contains("builtins.readFile"));
        assert!(expression.contains("operation = \"configured_index\""));
        assert!(expression.contains("path = [ \"safe\" ]"));
        assert!(expression.contains("childOffset = 7"));
        assert!(expression.contains("slice childOffset"));
        assert!(expression.contains("child_offset = childOffset"));
    }

    /// `nix-eval-jobs` exits 0 and reports a failed job as an `error` record.
    /// The demonstrated production fault took exactly this shape, so it must
    /// surface as an actionable failure rather than success, an empty
    /// configured list, or a bare missing-index message.
    #[test]
    fn exit_zero_with_an_index_error_record_fails_with_the_preserved_cause() {
        let configured = execution(ConfigObservationKind::ConfiguredIndex, Vec::new());
        let output = successful_output(vec![serde_json::json!({
            "attr": "__crystalForgeConfiguredIndex",
            "error": "access to absolute path '/tmp/selection.json' is forbidden in pure evaluation mode (use '--impure' to override)"
        })]);
        assert!(
            output.status.success(),
            "fixture must reproduce exit code 0"
        );

        let error = reconcile_observer_output(output, &configured)
            .expect_err("an index error record must not reconcile as success");
        let rendered = format!("{error:#}");

        assert!(
            rendered.contains("__crystalForgeConfiguredIndex"),
            "failure must name the job that failed: {rendered}"
        );
        assert!(
            rendered.contains("forbidden in pure evaluation mode"),
            "failure must preserve the underlying evaluator cause: {rendered}"
        );
        assert!(
            !rendered.contains("produced no result record"),
            "an errored index must not be reported as a merely missing index: {rendered}"
        );
    }

    /// A per-option classifier failure is a bounded partial-result diagnostic.
    /// It must never be promoted into a whole-index failure.
    #[test]
    fn classifier_error_records_stay_partial_diagnostics_with_a_usable_index() {
        let configured = execution(ConfigObservationKind::ConfiguredIndex, Vec::new());
        let key = "b".repeat(64);
        let result = reconcile_observer_output(
            successful_output(vec![
                serde_json::json!({
                    "attr": "__crystalForgeConfiguredIndex",
                    "drvPath": configured.target.carrier_drv_path,
                    "extraValue": {
                        "kind": "configured_index", "path_components": [], "total_traversed": 2,
                        "diagnostics": [], "diagnostics_truncated": false
                    }
                }),
                serde_json::json!({
                    "attr": format!("configured_{key}"),
                    "error": "ambiguous classifier poison"
                }),
            ]),
            &configured,
        )
        .expect("a classifier failure must not fail the whole index");

        assert_eq!(
            result["classifier_diagnostics"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(
            result["classifier_diagnostics"][0]["code"],
            "configured_classifier_failed"
        );
        validate_config_observation_payload(
            ConfigObservationKind::ConfiguredIndex,
            &[],
            0,
            &result,
        )
        .expect("partial-diagnostic index should still validate");
    }

    /// Evaluator failure text can quote option values and credential-bearing
    /// URLs. The preserved cause must be redacted and bounded.
    #[test]
    fn preserved_observer_error_is_redacted_and_bounded() {
        let excerpt = observer_error_excerpt(
            "error: while evaluating https://user:sw0rdf1sh@example.test/repo.git\n  trace line",
        );
        assert!(
            !excerpt.contains("sw0rdf1sh"),
            "credential must not survive into a preserved failure: {excerpt}"
        );

        let bounded = observer_error_excerpt(&"n".repeat(OBSERVER_ERROR_EXCERPT_LIMIT * 4));
        assert!(bounded.chars().count() <= OBSERVER_ERROR_EXCERPT_LIMIT + 3);
        assert!(bounded.ends_with("..."));
    }

    #[test]
    fn persisted_observer_failure_keeps_safe_cause_and_stable_category() {
        let error = anyhow::anyhow!(
            "access to https://user:sw0rdf1sh@example.test/repo.git is forbidden in pure evaluation mode"
        )
        .context("configured index job failed");
        let persisted = persisted_observer_failure(&error);

        assert!(persisted.starts_with("Config observation evaluation failed: "));
        assert!(persisted.contains("configured index job failed"));
        assert!(persisted.contains("forbidden in pure evaluation mode"));
        assert!(!persisted.contains("sw0rdf1sh"));
        assert!(
            persisted.chars().count()
                <= "Config observation evaluation failed: ".chars().count()
                    + OBSERVER_ERROR_EXCERPT_LIMIT
                    + 3
        );
    }

    /// Inline selection transport replaced a private temporary JSON file, so
    /// path components now reach trusted Nix source directly. Each component
    /// must be escaped as a Nix string literal, and component-array identity
    /// must survive: a component containing a dot stays one component.
    #[test]
    fn inline_selection_escapes_hostile_path_components_and_keeps_array_identity() {
        let hostile = vec![
            "quote\"component".to_string(),
            "back\\slash".to_string(),
            "${builtins.abort \"injection\"}".to_string(),
            "dotted.component".to_string(),
        ];
        let execution = execution(ConfigObservationKind::ConfiguredIndex, hostile);
        let expression = build_observer_expression(
            "git+https://example.test/repo?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &execution.target,
        );

        // Interpolation must be neutralised, never evaluated. The escaped
        // form still contains "${", so assert every occurrence is preceded
        // by a backslash rather than asserting simple absence.
        assert!(
            expression.contains("\\${builtins.abort"),
            "interpolation must be escaped: {expression}"
        );
        assert!(
            expression
                .match_indices("${builtins.abort")
                .all(|(index, _)| index > 0 && expression.as_bytes()[index - 1] == b'\\'),
            "every interpolation must be backslash-escaped: {expression}"
        );
        assert!(expression.contains("quote\\\"component"));
        assert!(expression.contains("back\\\\slash"));
        // A dot is data inside one component, not a component separator.
        assert!(expression.contains("\"dotted.component\""));

        let path_list = expression
            .split_once("path = [ ")
            .and_then(|(_, rest)| rest.split_once(" ];"))
            .map(|(list, _)| list)
            .expect("expression must contain a bracketed path list");
        assert_eq!(
            path_list.matches("\" \"").count() + 1,
            4,
            "four components must remain four Nix strings: {path_list}"
        );
    }

    #[test]
    fn shallow_command_uses_direct_nix_eval_without_a_derivation_carrier() {
        let target = execution(ConfigObservationKind::Prefix, vec!["services".to_string()]);
        let command = build_shallow_observer_command(
            Path::new("nix"),
            "path:/nix/store/source?narHash=sha256-test",
            &target.target,
        );
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert_eq!(args.first().map(|arg| arg.as_ref()), Some("eval"));
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(args.iter().any(|arg| arg.contains("configuration.options")));
        assert!(
            args.iter()
                .any(|arg| arg.contains("operation = \"prefix\""))
        );
        assert!(!args.iter().any(|arg| arg.contains("system.build.toplevel")));
    }

    #[test]
    fn reconciliation_binds_tree_payload_to_requested_child_offset() {
        let mut execution = execution(ConfigObservationKind::Prefix, vec!["services".to_string()]);
        execution.target.child_offset = 512;
        let child_path = vec!["services".to_string(), "last".to_string()];
        let result = reconcile_observer_output(
            successful_output(vec![serde_json::json!({
                "attr": "observation",
                "drvPath": execution.target.carrier_drv_path,
                "extraValue": {
                    "kind": "prefix",
                    "path_components": ["services"],
                    "child_offset": 512,
                    "children": [{
                        "path_components": ["services", "last"],
                        "key": option_key(&child_path),
                        "kind": "option"
                    }],
                    "children_truncated": false,
                    "total_children": 513
                }
            })]),
            &execution,
        )
        .expect("matching child page should reconcile");
        assert_eq!(result["child_offset"], 512);
    }

    #[test]
    fn observer_command_uses_only_closed_server_owned_arguments() {
        let command = build_observer_command(Path::new("nix-eval-jobs"), "{}");
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|args| args == ["--expr", "{}"]));
        assert!(!args.iter().any(|arg| arg.contains("path_components")));
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn interactive_try_lock_miss_waits_without_consuming_an_attempt(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("capacity-{suffix}"))
        .bind(format!("https://example.test/capacity-{suffix}.git"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let revision = format!("{:0>40}", &suffix[..32]);
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_status) VALUES ($1, $2, now(), 'complete') RETURNING id",
        )
        .bind(flake_id)
        .bind(&revision)
        .fetch_one(&pool)
        .await
        .unwrap();
        let configuration = format!("capacity-{suffix}");
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, flake_id, derivation, system_configuration_name) VALUES ($1, 'test-key', $2, '', $3) RETURNING id",
        )
        .bind(format!("host-{suffix}"))
        .bind(flake_id)
        .bind(&configuration)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, completed_at) VALUES ($1, 'nixos', $2, $3, 5, now())",
        )
        .bind(commit_id)
        .bind(&configuration)
        .bind(format!("/nix/store/{suffix}-{configuration}.drv"))
        .execute(&pool)
        .await
        .unwrap();
        let outcome = create_or_reuse_config_observation_request(
            &pool,
            system_id,
            &revision,
            ConfigObservationKind::Root,
            &[],
            0,
            false,
        )
        .await
        .unwrap();
        let CreateConfigObservationOutcome::Resolved(request) = outcome else {
            panic!("capacity fixture should resolve");
        };

        let mut blocker = pool.acquire().await.unwrap();
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(INTERACTIVE_CONFIG_ADVISORY_LOCK)
            .execute(&mut *blocker)
            .await
            .unwrap();
        assert!(process_one_config_observation(&pool, Path::new("/tmp")).await);
        let state: (String, i32, Option<Uuid>, Option<chrono::DateTime<chrono::Utc>>) =
            sqlx::query_as(
                "SELECT status, attempts, execution_id, execution_heartbeat_at FROM config_observation_requests WHERE id = $1",
            )
            .bind(request.request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(state, ("waiting_for_capacity".to_string(), 0, None, None));
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
                .bind(INTERACTIVE_CONFIG_ADVISORY_LOCK)
                .fetch_one(&mut *blocker)
                .await
                .unwrap()
        );
    }
}
