//! Executes non-authoritative scoped Config Explorer observations.
//!
//! Capacity acquisition is nonblocking. A request becomes `running` only after
//! both the cross-process heavy-Nix lock and in-process permit are held.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::Duration as ChronoDuration;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use tempfile::NamedTempFile;
use tokio::process::Command;
use tokio::time::MissedTickBehavior;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::derivations::utils::build_flake_reference;
use crate::flake::credentials::FlakeCredentialEnv;
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
const OBSERVATION_DEADLINE: Duration = Duration::from_secs(5 * 60);
const OBSERVATION_STDOUT_LIMIT: usize = 64 * 1024 * 1024;
const OBSERVATION_STDERR_LIMIT: usize = 256 * 1024;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const CONFIGURED_DIAGNOSTIC_LIMIT: usize = 128;
const STALE_EXECUTION_THRESHOLD: ChronoDuration = ChronoDuration::minutes(10);

#[derive(Serialize)]
struct ObserverSelection<'a> {
    operation: &'a str,
    path: &'a [String],
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
pub(crate) async fn process_one_config_observation(pool: &PgPool) -> bool {
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
    if let Err(error) = execute_reserved_config_observation(pool, target).await {
        warn!(%error, "config_observation_execution_failed");
    }
    true
}

async fn execute_reserved_config_observation(
    pool: &PgPool,
    target: ConfigObservationExecutionTarget,
) -> Result<()> {
    let mut lock_conn = pool
        .acquire()
        .await
        .context("acquire Config observation lock session")?;
    let heavy_locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(HEAVY_NIX_ADVISORY_LOCK)
        .fetch_one(&mut *lock_conn)
        .await
        .context("try Config observation heavy-Nix lock")?;
    if !heavy_locked {
        defer_config_observation_capacity(pool, target.request_id).await?;
        return Ok(());
    }
    let permit = match heavy_nix_limiter().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            release_heavy_lock(&mut lock_conn).await;
            defer_config_observation_capacity(pool, target.request_id).await?;
            return Ok(());
        }
    };
    let execution_id = Uuid::new_v4();
    if let Err(error) =
        crate::queries::cve_scans::acquire_execution_lock(&mut lock_conn, execution_id).await
    {
        release_heavy_lock(&mut lock_conn).await;
        let _ = lock_conn.close().await;
        return Err(error.context("acquire Config observation execution lock"));
    }
    let execution = match start_config_observation_execution(pool, target, execution_id).await {
        Ok(Some(execution)) => execution,
        Ok(None) => {
            release_heavy_lock(&mut lock_conn).await;
            crate::queries::cve_scans::release_execution_lock_or_close(lock_conn, execution_id)
                .await;
            drop(permit);
            return Ok(());
        }
        Err(error) => {
            release_heavy_lock(&mut lock_conn).await;
            let _ = lock_conn.close().await;
            drop(permit);
            return Err(error);
        }
    };

    let completion = match run_observation(pool, &execution).await {
        Ok(payload) => complete_config_observation_success(pool, &execution, &payload)
            .await
            .map(|_| ()),
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
    release_heavy_lock(&mut lock_conn).await;
    crate::queries::cve_scans::release_execution_lock_or_close(lock_conn, execution_id).await;
    drop(permit);
    completion
}

async fn release_heavy_lock(conn: &mut PgConnection) {
    if !matches!(
        sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
            .bind(HEAVY_NIX_ADVISORY_LOCK)
            .fetch_one(&mut *conn)
            .await,
        Ok(true)
    ) {
        warn!("Config observation heavy-Nix lock release was not confirmed");
    }
}

async fn run_observation(pool: &PgPool, execution: &ConfigObservationExecution) -> Result<Value> {
    let selection = ObserverSelectionFile::create(&execution.target)?;
    let flake_ref = build_flake_reference(&execution.target.repo_url, &execution.target.revision);
    let expression = build_observer_expression(
        &flake_ref,
        &execution.target.configuration_name,
        selection.path()?,
    );
    let credentials = FlakeCredentialEnv::load(pool, execution.target.flake_id).await?;
    let mut command = build_observer_command(
        Path::new(NIX_EVAL_JOBS_PROGRAM),
        &expression,
        credentials.as_ref(),
    );
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
    reconcile_observer_output(output, execution)
}

fn build_observer_expression(
    flake_ref: &str,
    configuration_name: &str,
    selection_path: &str,
) -> String {
    let observer = include_str!("../models/config_observer.nix");
    let encoder = include_str!("../models/config_value_encoding.nix");
    format!(
        "let\n  selection = builtins.fromJSON (builtins.readFile {selection});\n  flake = builtins.getFlake {flake_ref};\n  configuration = builtins.getAttr {configuration} flake.nixosConfigurations;\n  encodeValue = ({encoder}) configuration.pkgs.lib;\nin ({observer}) {{ inherit flake configuration encodeValue; targetKey = builtins.hashString \"sha256\" (builtins.toJSON [ {flake_ref} {configuration} configuration.config.system.build.toplevel.drvPath ]); operation = selection.operation; path = selection.path; }}",
        selection = nix_string_pub(selection_path),
        flake_ref = nix_string_pub(flake_ref),
        configuration = nix_string_pub(configuration_name),
    )
}

fn build_observer_command(
    program: &Path,
    expression: &str,
    credentials: Option<&FlakeCredentialEnv>,
) -> Command {
    let mut command = Command::new(program);
    command.args([
        "--expr",
        expression,
        "--impure",
        "--meta",
        "--apply",
        "derivation: derivation.meta.crystalForgeConfigObservation",
        "--option",
        "experimental-features",
        "nix-command flakes",
        "--workers",
        "2",
    ]);
    if let Some(credentials) = credentials {
        credentials.apply_to_nix_command(&mut command);
    }
    command
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
        &redacted,
    )?;
    Ok(redacted)
}

#[cfg(test)]
mod tests {
    use super::*;
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
                    "children_truncated": false, "total_children": 0
                }),
            ),
            (
                execution(ConfigObservationKind::Prefix, vec!["services".to_string()]),
                serde_json::json!({
                    "kind": "prefix", "path_components": ["services"], "children": [],
                    "children_truncated": false, "total_children": 0
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
        validate_config_observation_payload(ConfigObservationKind::ConfiguredIndex, &[], &result)
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
        assert!(!expression.contains("builtins.abort injection"));
    }

    #[test]
    fn observer_command_uses_only_closed_server_owned_arguments() {
        let command = build_observer_command(Path::new("nix-eval-jobs"), "{}", None);
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|args| args == ["--expr", "{}"]));
        assert!(!args.iter().any(|arg| arg.contains("path_components")));
    }

    #[tokio::test]
    #[ignore = "requires an isolated migrated database"]
    async fn heavy_nix_try_lock_miss_waits_without_consuming_an_attempt() {
        let pool = PgPool::connect(
            &std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must identify an isolated migrated test database"),
        )
        .await
        .unwrap();
        sqlx::query("DELETE FROM config_observation_requests")
            .execute(&pool)
            .await
            .unwrap();

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
        )
        .await
        .unwrap();
        let CreateConfigObservationOutcome::Resolved(request) = outcome else {
            panic!("capacity fixture should resolve");
        };

        let mut blocker = pool.acquire().await.unwrap();
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(HEAVY_NIX_ADVISORY_LOCK)
            .execute(&mut *blocker)
            .await
            .unwrap();
        assert!(process_one_config_observation(&pool).await);
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
                .bind(HEAVY_NIX_ADVISORY_LOCK)
                .fetch_one(&mut *blocker)
                .await
                .unwrap()
        );
    }
}
