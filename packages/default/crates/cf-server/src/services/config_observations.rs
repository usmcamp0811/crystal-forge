//! Executes non-authoritative scoped Config Explorer observations.
//!
//! Capacity acquisition is nonblocking. A request becomes `running` only after
//! its cross-process advisory lock and in-process permit are held. Root, prefix,
//! option, and provenance requests use dedicated interactive capacity. The
//! complete configured-option inventory continues to use heavy-Nix capacity.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::Duration as ChronoDuration;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use tempfile::NamedTempFile;
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
const OBSERVATION_STDOUT_LIMIT: usize = 64 * 1024 * 1024;
const OBSERVATION_STDERR_LIMIT: usize = 256 * 1024;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CONFIGURED_DIAGNOSTIC_LIMIT: usize = 128;
const STALE_EXECUTION_THRESHOLD: ChronoDuration = ChronoDuration::minutes(10);

fn interactive_config_limiter() -> Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMITER.get_or_init(|| Arc::new(Semaphore::new(1))).clone()
}

#[derive(Serialize)]
struct ObserverSelection<'a> {
    operation: &'a str,
    path: &'a [String],
    child_offset: u32,
}

struct ObserverSelectionFile(NamedTempFile);

impl ObserverSelectionFile {
    fn create(target: &ConfigObservationExecutionTarget) -> Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix("crystal-forge-config-observation-")
            .suffix(".json")
            .tempfile()
            .context("create private Config observation selection")?;
        // SECURITY: Structured operation input is transported as owner-only
        // JSON. It is never interpolated into trusted Nix source.
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .context("secure Config observation selection")?;
        serde_json::to_writer(
            file.as_file_mut(),
            &ObserverSelection {
                operation: target.kind.as_str(),
                path: &target.path_components,
                child_offset: target.child_offset,
            },
        )?;
        file.as_file_mut().flush()?;
        Ok(Self(file))
    }

    fn path(&self) -> Result<&str> {
        self.0
            .path()
            .to_str()
            .context("Config observation selection path is not UTF-8")
    }
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
            // Persist a stable category. Evaluator-controlled stderr remains
            // bounded and redacted in logs but is not a public cache contract.
            debug!(request_id = %execution.target.request_id, %error, "scoped Config observation failed");
            complete_config_observation_failure(
                pool,
                &execution,
                "Config observation evaluation failed",
            )
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
        let selection = ObserverSelectionFile::create(&execution.target)?;
        let expression = build_observer_expression(
            &flake_ref,
            &execution.target.configuration_name,
            selection.path()?,
        );
        let command = build_observer_command(Path::new(NIX_EVAL_JOBS_PROGRAM), &expression);
        // The command must retain the owner-only selection file until exit.
        return run_observation_command(pool, execution, command, Some(selection)).await;
    } else {
        build_shallow_observer_command(Path::new(NIX_PROGRAM), &flake_ref, &execution.target)
    };
    run_observation_command(pool, execution, command, None).await
}

async fn run_observation_command(
    pool: &PgPool,
    execution: &ConfigObservationExecution,
    mut command: Command,
    _selection: Option<ObserverSelectionFile>,
) -> Result<Value> {
    let evaluation_started_at = Instant::now();
    let mut run = Box::pin(run_nix_command_bounded(
        &mut command,
        "scoped Config observer",
        OBSERVATION_DEADLINE,
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

fn build_observer_expression(
    flake_ref: &str,
    configuration_name: &str,
    selection_path: &str,
) -> String {
    let observer = include_str!("../models/config_observer.nix");
    let shallow_observer = include_str!("../models/config_shallow_observer.nix");
    let encoder = include_str!("../models/config_value_encoding.nix");
    format!(
        "let\n  selection = builtins.fromJSON (builtins.readFile {selection});\n  flake = builtins.getFlake {flake_ref};\n  configuration = builtins.getAttr {configuration} flake.nixosConfigurations;\n  encodeValue = ({encoder}) configuration.pkgs.lib;\nin ({observer}) {{ inherit flake configuration encodeValue; targetKey = builtins.hashString \"sha256\" (builtins.toJSON [ {flake_ref} {configuration} configuration.config.system.build.toplevel.drvPath ]); operation = selection.operation; path = selection.path; childOffset = selection.child_offset; shallowObserver = ({shallow_observer}); }}",
        selection = nix_string_pub(selection_path),
        flake_ref = nix_string_pub(flake_ref),
        configuration = nix_string_pub(configuration_name),
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
                let _ = error;
                continue;
            }
            bail!("Config observer job failed");
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
        let mut index = index.context("configured index result is missing")?;
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
    fn generated_expression_reads_json_selection_without_embedding_path_components() {
        let expression = build_observer_expression(
            "git+https://example.test/repo?rev=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "host",
            "/tmp/selection.json",
        );
        assert!(expression.contains("builtins.fromJSON (builtins.readFile"));
        assert!(expression.contains("childOffset = selection.child_offset"));
        assert!(expression.contains("slice childOffset"));
        assert!(expression.contains("child_offset = childOffset"));
        assert!(!expression.contains("builtins.abort injection"));
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
