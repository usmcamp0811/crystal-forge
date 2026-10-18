//! Commit-related API handlers.

use axum::{
    Json,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, interval};
use uuid::Uuid;

use crate::api::models::{
    ApiError, CancelEvalOutcome, EvalDependencyGraphResponse, EvalDependencyPackageRow,
    EvalHistoryPage, EvalHistoryParams, EvalPolicyMatrixResponse, EvalPolicySystemRow,
    EvalQueueItem, EvalQueueParams, EvalQueueSummary, ReorderEvalQueueRequest,
};
use crate::handlers::agent_request::CFState;
use crate::handlers::api::auth_session::require_csrf;
use crate::handlers::api::rbac::{require_admin, require_operator_or_admin};

pub(crate) mod log_registry;
mod visibility;
pub(crate) use log_registry::bind_eval_producer;

const EVAL_LOG_CHANNEL_BUFFER: usize = 1000;
const MAX_EVAL_LOG_CHANNELS: usize = 1024;
const EVAL_LOG_HISTORY_BUFFER: usize = 2000;
// Bound authorization and socket backpressure independently. An idle socket
// checks revocation every 20 seconds; a stuck read/send gets only 5 more seconds.
const EVAL_STREAM_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const EVAL_STREAM_REVALIDATION_INTERVAL: Duration = Duration::from_secs(20);

fn parse_id_list(segment: &str) -> Option<Vec<i32>> {
    let trimmed = segment.trim();
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }

    inner
        .split(',')
        .map(|value| value.trim().parse::<i32>().ok())
        .collect()
}

fn reorder_validation_details(message: &str) -> Option<serde_json::Value> {
    let prefix = "invalid eval queue reorder request: ";
    let payload = message.strip_prefix(prefix)?;

    let (duplicates_raw, rest) = payload.split_once("; missing IDs: ")?;
    let duplicates_raw = duplicates_raw.strip_prefix("duplicate IDs: ")?;
    let (missing_raw, extra_raw) = rest.split_once("; extra IDs: ")?;

    let duplicate_ids = parse_id_list(duplicates_raw)?;
    let missing_ids = parse_id_list(missing_raw)?;
    let extra_ids = parse_id_list(extra_raw)?;

    Some(serde_json::json!({
        "duplicate_ids": duplicate_ids,
        "missing_ids": missing_ids,
        "extra_ids": extra_ids,
    }))
}

/// Lists evaluation queue items projected to current viewer membership.
/// GET /api/v1/commits/eval-queue
///
/// Returns 403 before reading queue evidence when viewer authentication fails.
/// Administrators receive the full projection; other viewers pass their actual
/// user UUID to the scoped reader, including for counts and pagination.
pub async fn list_eval_queue(
    State(state): State<CFState>,
    headers: HeaderMap,
    axum::extract::Query(mut params): axum::extract::Query<EvalQueueParams>,
) -> impl IntoResponse {
    let visibility_user = match visibility::viewer_scope(&state.pool, &headers).await {
        Ok(scope) => scope,
        Err(status) => return status.into_response(),
    };

    params.limit = params.limit.max(1).min(crate::api::models::LIMIT_MAX);

    let result = match crate::queries::commits::list_eval_queue_for_user(
        &state.pool,
        &params,
        visibility_user,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            tracing::error!("Failed to list eval queue: {}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let items = result
        .rows
        .into_iter()
        .map(|row| EvalQueueItem {
            commit_id: row.commit_id,
            flake_id: row.flake_id,
            flake_name: row.flake_name,
            branch: row.branch,
            commit_hash: row.commit_hash,
            commit_message: row.commit_message,
            author: row.author,
            committed_at: row.committed_at,
            enqueued_at: row.enqueued_at,
            is_latest_per_flake: row.is_latest_per_flake,
            evaluation_status: row.evaluation_status,
            queue_position: row.queue_position,
            systems: row.systems,
            system_count: row.system_count,
            derivation_count: row.derivation_count,
            policy_counts_available: row.policy_counts_available,
            inventory_available: row.inventory_available,
            policy_pending_count: row.policy_pending_count,
            unselected_count: row.unselected_count,
            policy_unavailable_count: row.policy_unavailable_count,
            passed_count: row.passed_count,
            policy_failed_count: row.policy_failed_count,
            eval_failed_count: row.eval_failed_count,
            attempt_number: row.attempt_number,
            parent_attempt_id: row.parent_attempt_id,
            root_attempt_id: row.root_attempt_id,
            available_at: row.available_at,
        })
        .collect::<Vec<_>>();

    Json(EvalQueueSummary {
        active_count: result.active_count,
        completed_count: result.completed_count,
        successful_count: result.successful_count,
        failed_count: result.failed_count,
        domain_total: result.domain_total,
        filtered_total: result.filtered_total,
        execution_mode: state.server_config.execution_mode.as_str().to_string(),
        items,
        timestamp: chrono::Utc::now(),
    })
    .into_response()
}

/// Persist evaluation queue order for active commits.
/// POST /api/v1/commits/eval-queue/reorder
pub async fn reorder_eval_queue(
    State(state): State<CFState>,
    headers: HeaderMap,
    Json(request): Json<ReorderEvalQueueRequest>,
) -> impl IntoResponse {
    if require_operator_or_admin(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    if let Err(err) =
        crate::queries::commits::reorder_eval_queue(&state.pool, &request.ordered_commit_ids).await
    {
        if err
            .to_string()
            .starts_with("invalid eval queue reorder request:")
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "validation_error".to_string(),
                    message: err.to_string(),
                    details: reorder_validation_details(&err.to_string()),
                }),
            )
                .into_response();
        }

        tracing::error!("Failed to reorder eval queue: {}", err);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::OK.into_response()
}

/// Structured message types for eval log WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvalLogMessage {
    /// Plain text log line
    Log { message: String },

    /// Per-system status update
    SystemStatus {
        system: String,
        status: SystemEvalStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Overall eval status
    EvalStatus {
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemEvalStatus {
    Pending,
    Evaluating,
    Success,
    Failed,
    PolicyFailed,
    QueuedForBuild,
}

/// Streams indivisible commit logs to currently authorized whole-flake viewers.
/// GET /api/v1/commits/:commit_id/eval/stream
///
/// Missing and inaccessible commits both return 404. Nonadministrators require
/// complete current host membership and exact-latest discovery inventory. The
/// stream revalidates session, roles, inventory, and membership before accessing
/// replay history, before every outgoing event, and every 20-second heartbeat.
/// Revocation or authorization errors close the socket without further evidence.
/// Replay and events belong to one exact owner. A successor closes the old
/// generation; its latest authority cannot authorize earlier buffers. Admins
/// bypass membership checks, not attempt coherence. Wire fields stay unchanged.
pub async fn stream_eval_logs(
    ws: WebSocketUpgrade,
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let owner = match authorize_eval_stream_owner(&state, &headers, commit_id, None).await {
        Ok(owner) => owner,
        Err(status) => return status.into_response(),
    };

    ws.on_upgrade(move |socket| handle_eval_stream(socket, owner, state, headers))
}

// SECURITY: Do not retain a handshake principal. A new session/role lookup also
// catches account/session invalidation and administrator-role revocation.
// Bound each check so an unavailable database cannot keep an idle stream alive.
#[cfg(test)]
async fn authorize_eval_stream(
    state: &CFState,
    headers: &HeaderMap,
    commit_id: i32,
) -> Result<(), StatusCode> {
    authorize_eval_stream_owner(state, headers, commit_id, None)
        .await
        .map(|_| ())
}

// SECURITY: The latest-only route must authorize the channel's exact owner,
// including for administrators. New-attempt authority never authorizes replay
// or queued events from an earlier attempt of the same commit.
async fn authorize_eval_stream_owner(
    state: &CFState,
    headers: &HeaderMap,
    commit_id: i32,
    expected: Option<crate::queries::eval_logs::EvalLogOwner>,
) -> Result<crate::queries::eval_logs::EvalLogOwner, StatusCode> {
    tokio::time::timeout(EVAL_STREAM_OPERATION_TIMEOUT, async {
        let scope = visibility::viewer_scope(&state.pool, headers).await?;
        match visibility::full_flake_snapshot(&state.pool, commit_id, scope).await {
            Ok(Some(mut tx)) => {
                let owner = visibility::latest_log_owner(&mut tx, commit_id)
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                    .ok_or(StatusCode::NOT_FOUND)?;
                if expected.is_some_and(|expected| expected != owner) {
                    return Err(StatusCode::NOT_FOUND);
                }
                tx.commit()
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                Ok(owner)
            }
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    })
    .await
    .unwrap_or(Err(StatusCode::SERVICE_UNAVAILABLE))
}

async fn handle_eval_stream(
    mut socket: WebSocket,
    owner: crate::queries::eval_logs::EvalLogOwner,
    state: CFState,
    headers: HeaderMap,
) {
    let commit_id = owner.commit_id;
    if authorize_eval_stream_owner(&state, &headers, commit_id, Some(owner))
        .await
        .is_err()
    {
        return;
    }
    tracing::info!(
        "📡 WebSocket connection established for commit {} evaluation",
        commit_id
    );

    // CONCURRENCY: Subscribe and replay snapshot share the publication mutex.
    // Receivers never own a sender; rotation closes an old socket even if idle.
    let subscription = { state.eval_log_registry.lock().await.subscribe(owner) };
    let Some((mut rx, history_snapshot)) = subscription else {
        return;
    };

    for log_line in history_snapshot {
        if authorize_eval_stream_owner(&state, &headers, commit_id, Some(owner))
            .await
            .is_err()
        {
            return;
        }
        if !matches!(
            tokio::time::timeout(
                EVAL_STREAM_OPERATION_TIMEOUT,
                socket.send(Message::Text(log_line.into())),
            )
            .await,
            Ok(Ok(()))
        ) {
            tracing::debug!(commit_id, "Eval log replay stopped or timed out");
            return;
        }
    }

    let mut keepalive = interval(EVAL_STREAM_REVALIDATION_INTERVAL);

    // Stream messages from the broadcast channel to this WebSocket client
    loop {
        tokio::select! {
            recv = rx.recv() => {
                match recv {
                    Ok(log_line) => {
                        if authorize_eval_stream_owner(&state, &headers, commit_id, Some(owner)).await.is_err() {
                            break;
                        }
                        if !matches!(tokio::time::timeout(
                            EVAL_STREAM_OPERATION_TIMEOUT,
                            socket.send(Message::Text(log_line)),
                        ).await, Ok(Ok(()))) {
                            tracing::debug!(commit_id, "Eval log send stopped or timed out");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            "Eval stream for commit {} lagged; skipped {} messages",
                            commit_id,
                            skipped
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = keepalive.tick() => {
                if authorize_eval_stream_owner(&state, &headers, commit_id, Some(owner)).await.is_err() {
                    break;
                }
                if !matches!(tokio::time::timeout(
                    EVAL_STREAM_OPERATION_TIMEOUT,
                    socket.send(Message::Ping(Vec::new().into())),
                ).await, Ok(Ok(()))) {
                    tracing::debug!(commit_id, "Eval WebSocket heartbeat stopped or timed out");
                    break;
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed for commit {} eval", commit_id);
}

/// Returns commit log history after current whole-flake viewer authorization.
///
/// This endpoint retrieves persisted logs for completed/failed/cancelled evaluations.
/// For in-progress evaluations, clients should use the WebSocket stream instead.
/// Nonadministrators require all current hosts and exact-latest inventory to be
/// authorized. Authorization and logs share one read-only repeatable-read
/// snapshot. Missing and foreign commits both return 404. Only rows owned by
/// that exact latest attempt are returned; unattributed legacy rows stay stored.
pub async fn get_eval_logs_history(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let scope = match visibility::viewer_scope(&state.pool, &headers).await {
        Ok(scope) => scope,
        Err(status) => return status.into_response(),
    };

    let logs = match visibility::logs_for_user(&state.pool, commit_id, scope).await {
        Ok(Some(logs)) => logs,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch eval logs for commit {}: {}", commit_id, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let entries: Vec<EvalLogEntry> = logs
        .into_iter()
        .map(|row| EvalLogEntry {
            timestamp: row.log_timestamp,
            sequence: row.log_sequence,
            level: row.log_level,
            message: row.log_message,
        })
        .collect();

    Json(entries).into_response()
}

/// Fetch policy matrix data for a commit evaluation.
/// GET /api/v1/commits/:commit_id/eval/policy-matrix
///
/// # Responses
/// Returns 403 before reading provenance when viewer authorization fails.
/// Returns 404 for both missing commits and commits outside current environment
/// membership. Nonadmin responses include only currently authorized hosts and
/// metadata reachable from those hosts in the same repeatable-read snapshot.
/// Returns 500 for database errors or invalid captured plans. Otherwise returns
/// 200 with the legacy fields and attempt-scoped matrix, including unavailable
/// provenance when no capture exists.
pub async fn get_eval_policy_matrix(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    eval_policy_matrix_response(commit_id, None, state, headers).await
}

/// Returns policy evidence for the exact attempt UUID belonging to the commit.
///
/// # Responses
/// Returns 403 when viewer authentication fails. Missing, foreign, and
/// inaccessible selections return the same 404. Returns 500 for read errors,
/// otherwise 200 with the same matrix envelope as the latest endpoint.
pub async fn get_eval_policy_matrix_at_attempt(
    Path((commit_id, attempt_id)): Path<(i32, Uuid)>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    eval_policy_matrix_response(commit_id, Some(attempt_id), state, headers).await
}

async fn eval_policy_matrix_response(
    commit_id: i32,
    attempt_id: Option<Uuid>,
    state: CFState,
    headers: HeaderMap,
) -> axum::response::Response {
    let visibility_user = match visibility::viewer_scope(&state.pool, &headers).await {
        Ok(scope) => scope,
        Err(status) => return status.into_response(),
    };

    let selected = match attempt_id {
        Some(attempt_id) => {
            crate::queries::eval_policy_plans::fetch_matrix_for_user_at_attempt(
                &state.pool,
                commit_id,
                visibility_user,
                attempt_id,
            )
            .await
        }
        None => {
            crate::queries::eval_policy_plans::fetch_matrix_for_user(
                &state.pool,
                commit_id,
                visibility_user,
            )
            .await
        }
    };
    let (rows, matrix) = match selected {
        Ok(Some(rows)) => rows,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(
                "Failed to fetch eval policy matrix for commit {}: {}",
                commit_id,
                e
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut body = build_eval_policy_matrix_response(commit_id, rows);
    body.matrix = Some(matrix);

    Json(body).into_response()
}

/// Classify a persisted policy-result object (`{"passed": .., "strict": ..,
/// "blocking": .., "details": ..}`, either the global `cfAgentEnabled` entry
/// or an assigned policy entry) into a matrix cell status and detail text.
///
/// Current documents use `blocking` because assignment mode can make an
/// intrinsically strict failure report-only. Documents written before the
/// field existed fall back to intrinsic `strict`. Non-boolean current metadata,
/// missing/non-boolean `passed`, or a failed legacy result whose strictness is
/// unknown fails closed rather than displaying an invalid result as harmless.
fn classify_policy_result(policy_result: &serde_json::Value) -> (&'static str, Option<String>) {
    let passed = policy_result.get("passed").and_then(|v| v.as_bool());
    let blocking = policy_result.get("blocking").and_then(|v| v.as_bool());
    let strict = policy_result.get("strict").and_then(|v| v.as_bool());
    let detail = || {
        policy_result
            .get("details")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    if policy_result.get("blocking").is_some() && blocking.is_none() {
        return (
            "infrastructure_error",
            Some("policy result has invalid blocking metadata".to_string()),
        );
    }
    if passed == Some(true) && blocking == Some(true) {
        return (
            "infrastructure_error",
            Some("policy result has contradictory blocking metadata".to_string()),
        );
    }

    match (passed, blocking, strict) {
        (Some(true), _, _) => ("pass", None),
        (Some(false), Some(false), _) => ("warn", detail()),
        (Some(false), Some(true), _) => ("fail", detail()),
        (Some(false), None, Some(false)) => ("warn", detail()),
        (Some(false), None, Some(true) | None) => ("fail", detail()),
        _ => (
            "infrastructure_error",
            Some("policy result missing or invalid".to_string()),
        ),
    }
}

fn build_eval_policy_matrix_response(
    commit_id: i32,
    rows: Vec<crate::queries::commits::EvalPolicySystemRow>,
) -> EvalPolicyMatrixResponse {
    let mut assigned_columns = std::collections::BTreeMap::<String, String>::new();
    for row in &rows {
        if let Some(assigned) = row
            .policy_results
            .get("assigned")
            .and_then(|value| value.as_object())
        {
            for (policy_id, result) in assigned {
                // Legacy require_cf_agent assigned results are handled
                // by the unconditional global cfAgentEnabled metadata
                // and must not produce a duplicate column.  Filter them
                // out here so old evaluation data that was persisted
                // before migration 0187 renders correctly without
                // re-evaluation.
                if result.get("type").and_then(|t| t.as_str()) == Some("require_cf_agent") {
                    continue;
                }
                let name = result
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or(policy_id)
                    .to_string();
                assigned_columns.entry(policy_id.clone()).or_insert(name);
            }
        }
    }

    let mut policy_labels = vec!["CF agent".to_string()];
    policy_labels.extend(assigned_columns.values().cloned());

    let systems = rows
        .into_iter()
        .map(|row| {
            let mut results = Vec::with_capacity(policy_labels.len());
            let mut details = Vec::with_capacity(policy_labels.len());

            // Rows written before migration 0185 (or otherwise never
            // evaluated under the current policy-result model) have
            // policy_requirements_met = NULL and no "global.cfAgentEnabled"
            // entry in policy_results. These must not be displayed as
            // passing, failing, or infrastructure errors — they simply have
            // no data yet and require re-evaluation.
            let is_legacy_unknown = row.eval_status != "eval_failed"
                && row.policy_requirements_met.is_none()
                && row
                    .policy_results
                    .get("global")
                    .and_then(|global| global.get("cfAgentEnabled"))
                    .is_none();

            if is_legacy_unknown {
                for _ in 0..policy_labels.len() {
                    results.push("legacy_unknown".to_string());
                    details.push(Some("Policy results unavailable; re-evaluate.".to_string()));
                }
                return EvalPolicySystemRow {
                    system_name: row.system_name,
                    results,
                    details,
                };
            }

            if row.eval_status == "eval_failed" {
                results.push("nix_eval_failure".to_string());
                details.push(row.error_message.clone());
            } else {
                let cf_agent = row
                    .policy_results
                    .get("global")
                    .and_then(|global| global.get("cfAgentEnabled"));
                match cf_agent {
                    Some(result) => {
                        let (status, mut detail) = classify_policy_result(result);
                        if status == "fail" && detail.is_none() {
                            detail = Some("Crystal Forge agent is disabled".to_string());
                        }
                        results.push(status.to_string());
                        details.push(detail);
                    }
                    None => {
                        results.push("infrastructure_error".to_string());
                        details.push(Some(
                            "cfAgentEnabled metadata missing or invalid".to_string(),
                        ));
                    }
                }
            }

            let assigned_results = row
                .policy_results
                .get("assigned")
                .and_then(|value| value.as_object());
            for policy_id in assigned_columns.keys() {
                if row.eval_status == "eval_failed" {
                    results.push("nix_eval_failure".to_string());
                    details.push(row.error_message.clone());
                    continue;
                }

                let Some(policy_result) =
                    assigned_results.and_then(|assigned| assigned.get(policy_id))
                else {
                    // Result absence cannot establish historical applicability.
                    results.push("legacy_unknown".to_string());
                    details.push(Some(
                        "Policy applicability or result unavailable".to_string(),
                    ));
                    continue;
                };

                let (status, detail) = classify_policy_result(policy_result);
                results.push(status.to_string());
                details.push(detail);
            }

            EvalPolicySystemRow {
                system_name: row.system_name,
                results,
                details,
            }
        })
        .collect();

    EvalPolicyMatrixResponse {
        commit_id,
        policies: policy_labels,
        systems,
        matrix: None,
    }
}

/// Returns the latest route's mutable native graph to whole-flake viewers.
/// GET /api/v1/commits/:commit_id/eval/dependency-graph
///
/// Missing and inaccessible commits both return 404. Nonadministrators need
/// complete current host membership and registered exact-latest inventory.
/// Authorization and evidence share one repeatable-read snapshot. Native graph
/// data is mutable and MUST NOT be presented as pinned-attempt evidence.
pub async fn get_eval_dependency_graph(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let scope = match visibility::viewer_scope(&state.pool, &headers).await {
        Ok(scope) => scope,
        Err(status) => return status.into_response(),
    };

    let rows = match visibility::graph_for_user(&state.pool, commit_id, scope).await {
        Ok(Some(rows)) => rows,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!(
                "Failed to fetch eval dependency graph for commit {}: {}",
                commit_id,
                e
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let total_packages = rows.len() as i64;
    let body = EvalDependencyGraphResponse {
        commit_id,
        total_packages,
        packages: rows
            .into_iter()
            .map(|row| EvalDependencyPackageRow {
                package_name: row.package_name,
                closure_counted: row.closure_counted,
                ready_count: row.ready_count,
                pending_count: row.pending_count,
                failed_count: row.failed_count,
            })
            .collect(),
    };

    Json(body).into_response()
}

/// DTO for a single evaluation log entry (REST API response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalLogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub sequence: i32,
    pub level: Option<String>,
    pub message: String,
}

/// Publishes a line using the state's immutable producer capability.
///
/// Unbound, foreign, superseded, and retired producers are dropped. Publication
/// never creates a channel or obtains ownership from the latest attempt.
pub async fn broadcast_eval_log(state: &CFState, commit_id: i32, log_line: String) {
    let msg = EvalLogMessage::Log { message: log_line };
    broadcast_eval_message(state, commit_id, msg).await;
}

/// Publishes a system status through the state's exact producer generation.
///
/// Unbound or superseded producers cannot append to another attempt's replay.
pub async fn broadcast_system_status(
    state: &CFState,
    commit_id: i32,
    system: String,
    status: SystemEvalStatus,
    error: Option<String>,
) {
    let msg = EvalLogMessage::SystemStatus {
        system,
        status,
        error,
    };
    broadcast_eval_message(state, commit_id, msg).await;
}

/// Publishes an overall status through the state's exact producer generation.
///
/// Terminal status remains useful while the generation exists. A superseded
/// publication is deliberately dropped rather than retargeted to a successor.
pub async fn broadcast_eval_status(
    state: &CFState,
    commit_id: i32,
    status: String,
    message: Option<String>,
) {
    let msg = EvalLogMessage::EvalStatus { status, message };
    broadcast_eval_message(state, commit_id, msg).await;
}

/// Internal: broadcast a structured message
async fn broadcast_eval_message(state: &CFState, commit_id: i32, msg: EvalLogMessage) {
    if let Ok(json) = serde_json::to_string(&msg) {
        log_registry::publish(state, commit_id, json).await;
    }
}

/// Schedules retirement of this producer generation after ten minutes.
///
/// The grace period permits late replay of terminal statuses. Cleanup retains
/// only a token and cannot delete a newer attempt or a replacement generation.
pub async fn cleanup_eval_channel(state: &CFState, commit_id: i32) {
    let Some(token) = state
        .eval_log_producer
        .filter(|t| t.owner.commit_id == commit_id)
    else {
        return;
    };
    let registry = state.eval_log_registry.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;
        registry.lock().await.remove(token);
    });
}

/// Trigger manual re-evaluation for a commit
/// POST /api/v1/commits/:commit_id/re-evaluate
pub async fn re_evaluate_commit(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // SECURITY: The evaluator processes every configuration on the commit.
    // Environment-scoped operators must not trigger work for hidden systems.
    if require_admin(&state.pool, &headers).await.is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Err(response) = require_csrf(&headers) {
        return response;
    }

    match crate::queries::commits::reset_commit_evaluation(&state.pool, commit_id).await {
        Ok(outcome) => {
            let queued_new = outcome == crate::queries::commits::EvalQueueTransition::QueuedNew;
            if queued_new {
                state.queue_notifier.notify_eval_queue();
            }
            (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "ok",
                    "queued": queued_new,
                    "message": if queued_new {
                        format!("Commit {} queued for re-evaluation", commit_id)
                    } else {
                        format!("Commit {} evaluation already active", commit_id)
                    }
                })),
            )
                .into_response()
        }
        Err(error) => {
            let error = crate::security::snapshot_redaction::redact_text(&error.to_string());
            tracing::error!(commit_id, error = %error, "failed to reset evaluation");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "status": "error",
                    "message": "Failed to reset evaluation"
                })),
            )
                .into_response()
        }
    }
}

/// Cancel an evaluation (pending → cancelled; in_progress → cancelling).
/// POST /api/v1/commits/:commit_id/cancel-evaluation
pub async fn cancel_commit_evaluation(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_operator_or_admin(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    match crate::queries::commits::cancel_commit_evaluation(&state.pool, commit_id).await {
        Ok(CancelEvalOutcome::Cancelled) => (
            StatusCode::OK,
            Json(serde_json::json!({ "outcome": "cancelled" })),
        )
            .into_response(),
        Ok(CancelEvalOutcome::CancellingInProgress) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "outcome": "cancelling_in_progress" })),
        )
            .into_response(),
        Ok(CancelEvalOutcome::AlreadyTerminal) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "conflict",
                "message": "Evaluation is already in a terminal state"
            })),
        )
            .into_response(),
        Ok(CancelEvalOutcome::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "message": format!("Commit {} not found", commit_id)
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to cancel evaluation for commit {commit_id}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Force-cancel an evaluation stuck in 'cancelling' state.
/// POST /api/v1/commits/:commit_id/force-cancel-evaluation
pub async fn force_cancel_commit_evaluation(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_operator_or_admin(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    match crate::queries::commits::force_cancel_commit_evaluation(&state.pool, commit_id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "outcome": "cancelled" })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "conflict",
                "message": "Evaluation was not in cancelling or in_progress state"
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to force-cancel evaluation for commit {commit_id}: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Lists paginated evaluation history restricted to current viewer membership.
/// GET /api/v1/commits/eval-history
///
/// Returns 403 before reading history when viewer authentication fails.
/// Administrators use unrestricted scope; other viewers pass their user UUID
/// to the scoped reader for evidence, derived metadata, counts, and pagination.
pub async fn list_eval_history(
    State(state): State<CFState>,
    headers: HeaderMap,
    axum::extract::Query(mut params): axum::extract::Query<EvalHistoryParams>,
) -> impl IntoResponse {
    let scope = match visibility::viewer_scope(&state.pool, &headers).await {
        Ok(scope) => scope,
        Err(status) => return status.into_response(),
    };

    params.page = params.page.max(1);
    params.limit = params.limit.max(1).min(crate::api::models::LIMIT_MAX);

    if (params.page - 1).checked_mul(params.limit).is_none() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match crate::queries::commits::list_eval_history_for_user(&state.pool, &params, scope).await {
        Ok(page_result) => {
            let body: EvalHistoryPage = page_result;
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to list eval history: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerConfig;
    use sqlx::postgres::PgPoolOptions;

    fn test_state() -> CFState {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");
        CFState::new(
            pool,
            ServerConfig::default(),
            std::sync::Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        )
    }

    #[tokio::test]
    async fn evaluation_reads_require_identity_before_accessing_evidence() {
        // No cookie must return before the lazy pool connects, including queue
        // and history summary reads and the stream's repeated authorization.
        assert_eq!(
            list_eval_queue(
                State(test_state()),
                HeaderMap::new(),
                axum::extract::Query(EvalQueueParams::default()),
            )
            .await
            .into_response()
            .status(),
            StatusCode::FORBIDDEN,
        );
        assert_eq!(
            list_eval_history(
                State(test_state()),
                HeaderMap::new(),
                axum::extract::Query(EvalHistoryParams::default()),
            )
            .await
            .into_response()
            .status(),
            StatusCode::FORBIDDEN,
        );
        assert_eq!(
            get_eval_logs_history(Path(42), State(test_state()), HeaderMap::new())
                .await
                .into_response()
                .status(),
            StatusCode::FORBIDDEN,
        );
        assert_eq!(
            get_eval_dependency_graph(Path(42), State(test_state()), HeaderMap::new())
                .await
                .into_response()
                .status(),
            StatusCode::FORBIDDEN,
        );
        assert_eq!(
            authorize_eval_stream(&test_state(), &HeaderMap::new(), 42).await,
            Err(StatusCode::FORBIDDEN),
        );
    }

    #[tokio::test]
    async fn policy_matrix_requires_authentication_before_reading_provenance() {
        // No token means RBAC returns before the lazy pool can connect.
        let response = get_eval_policy_matrix(Path(42), State(test_state()), HeaderMap::new())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn pinned_policy_matrix_requires_authentication_before_reading_provenance() {
        let response = get_eval_policy_matrix_at_attempt(
            Path((42, Uuid::new_v4())),
            State(test_state()),
            HeaderMap::new(),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn eval_channel_fanout_and_cleanup() {
        let state = test_state();
        let commit_id = 42;
        let owner = crate::queries::eval_logs::EvalLogOwner {
            commit_id,
            evaluation_attempt_id: Uuid::new_v4(),
            attempt_number: 1,
        };
        let state = log_registry::test_producer(&state, owner).await;
        let (mut rx1, _) = state
            .eval_log_registry
            .lock()
            .await
            .subscribe(owner)
            .unwrap();
        let (mut rx2, _) = state
            .eval_log_registry
            .lock()
            .await
            .subscribe(owner)
            .unwrap();

        broadcast_eval_log(&state, commit_id, "hello".to_string()).await;

        let msg1 = rx1.recv().await.expect("first subscriber receives");
        let msg2 = rx2.recv().await.expect("second subscriber receives");

        assert_eq!(msg1, msg2);
        assert!(msg1.contains("hello"));

        cleanup_eval_channel(&state, commit_id).await;

        let (_, history) = state
            .eval_log_registry
            .lock()
            .await
            .subscribe(owner)
            .unwrap();
        assert_eq!(
            history.len(),
            1,
            "terminal replay survives the cleanup grace period"
        );
    }

    #[tokio::test]
    async fn eval_log_history_is_buffered() {
        let state = test_state();
        let commit_id = 77;
        let owner = crate::queries::eval_logs::EvalLogOwner {
            commit_id,
            evaluation_attempt_id: Uuid::new_v4(),
            attempt_number: 1,
        };
        let state = log_registry::test_producer(&state, owner).await;

        for i in 0..3 {
            broadcast_eval_log(&state, commit_id, format!("line-{i}")).await;
        }

        let (_, entries) = state
            .eval_log_registry
            .lock()
            .await
            .subscribe(owner)
            .unwrap();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].contains("line-0"));
        assert!(entries[2].contains("line-2"));
    }

    #[tokio::test]
    async fn all_message_variants_require_original_producer_generation() {
        let base = test_state();
        let a_owner = crate::queries::eval_logs::EvalLogOwner {
            commit_id: 42,
            evaluation_attempt_id: Uuid::new_v4(),
            attempt_number: 1,
        };
        let a = log_registry::test_producer(&base, a_owner).await;
        broadcast_eval_log(&a, 42, "A-private".into()).await;
        let b_owner = crate::queries::eval_logs::EvalLogOwner {
            evaluation_attempt_id: Uuid::new_v4(),
            attempt_number: 2,
            ..a_owner
        };
        let b = log_registry::test_producer(&base, b_owner).await;
        broadcast_eval_log(&a, 42, "A-late-log".into()).await;
        broadcast_system_status(
            &a,
            42,
            "A-private-system".into(),
            SystemEvalStatus::Failed,
            Some("A-private-error".into()),
        )
        .await;
        broadcast_eval_status(&a, 42, "failed".into(), Some("A-private-status".into())).await;
        broadcast_eval_log(&base, 42, "unbound".into()).await;
        broadcast_eval_log(&b, 43, "foreign-commit".into()).await;
        base.eval_log_registry
            .lock()
            .await
            .remove(a.eval_log_producer.unwrap());
        broadcast_eval_log(&b, 42, "B-visible".into()).await;
        let (_, replay) = base
            .eval_log_registry
            .lock()
            .await
            .subscribe(b_owner)
            .unwrap();
        assert_eq!(replay.len(), 1);
        assert!(replay[0].contains("B-visible"));
    }

    #[test]
    fn eval_log_wire_shapes_remain_compatible() {
        assert_eq!(
            serde_json::to_value(EvalLogMessage::Log {
                message: "line".into()
            })
            .unwrap(),
            serde_json::json!({"type":"log", "message":"line"})
        );
        assert_eq!(
            serde_json::to_value(EvalLogMessage::SystemStatus {
                system: "alpha".into(),
                status: SystemEvalStatus::QueuedForBuild,
                error: None,
            })
            .unwrap(),
            serde_json::json!({"type":"system_status", "system":"alpha", "status":"queued_for_build"})
        );
        assert_eq!(
            serde_json::to_value(EvalLogMessage::EvalStatus {
                status: "complete".into(),
                message: None,
            })
            .unwrap(),
            serde_json::json!({"type":"eval_status", "status":"complete"})
        );
        let entry = EvalLogEntry {
            timestamp: chrono::Utc::now(),
            sequence: 1,
            level: None,
            message: "line".into(),
        };
        let value = serde_json::to_value(entry).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert!(value.get("timestamp").is_some());
        assert_eq!(value["sequence"], 1);
        assert_eq!(value["level"], serde_json::Value::Null);
        assert_eq!(value["message"], "line");
    }

    #[test]
    fn reorder_validation_details_extracts_structured_ids() {
        let message = "invalid eval queue reorder request: duplicate IDs: [11, 22]; missing IDs: [33]; extra IDs: [44, 55]";
        let details = reorder_validation_details(message).expect("details should parse");

        assert_eq!(details["duplicate_ids"], serde_json::json!([11, 22]));
        assert_eq!(details["missing_ids"], serde_json::json!([33]));
        assert_eq!(details["extra_ids"], serde_json::json!([44, 55]));
    }

    #[test]
    fn reorder_validation_details_handles_empty_lists() {
        let message =
            "invalid eval queue reorder request: duplicate IDs: []; missing IDs: []; extra IDs: []";
        let details = reorder_validation_details(message).expect("details should parse");

        assert_eq!(details["duplicate_ids"], serde_json::json!([]));
        assert_eq!(details["missing_ids"], serde_json::json!([]));
        assert_eq!(details["extra_ids"], serde_json::json!([]));
    }

    #[test]
    fn policy_matrix_includes_strict_grafana_failures_for_gray_and_mattis() {
        let policy_id = "f2752e95-40f5-4cb7-9211-7ac8fd98334f";
        let policy_result = serde_json::json!({
            "global": {
                "cfAgentEnabled": {
                    "passed": true,
                    "strict": true,
                    "details": null
                }
            },
            "assigned": {
                policy_id: {
                    "name": "Required packages: grafana",
                    "type": "require_packages",
                    "strict": true,
                    "passed": false,
                    "details": "Missing required packages: grafana"
                }
            }
        });
        let rows = vec![
            crate::queries::commits::EvalPolicySystemRow {
                cf_agent_enabled: None,
                system_name: "gray".to_string(),
                eval_status: "evaluated".to_string(),
                error_message: None,
                policy_results: policy_result.clone(),
                policy_requirements_met: Some(false),
            },
            crate::queries::commits::EvalPolicySystemRow {
                cf_agent_enabled: None,
                system_name: "mattis".to_string(),
                eval_status: "evaluated".to_string(),
                error_message: None,
                policy_results: policy_result,
                policy_requirements_met: Some(false),
            },
        ];

        let matrix = build_eval_policy_matrix_response(2876, rows);

        assert_eq!(
            matrix.policies,
            vec!["CF agent", "Required packages: grafana"]
        );
        assert_eq!(matrix.systems.len(), 2);
        for row in &matrix.systems {
            assert!(matches!(row.system_name.as_str(), "gray" | "mattis"));
            assert_eq!(row.results, vec!["pass", "fail"]);
            assert_eq!(
                row.details,
                vec![None, Some("Missing required packages: grafana".to_string())]
            );
        }
    }

    /// Rows written before migration 0185 (policy_requirements_met = NULL,
    /// policy_results = '{}') must render as "legacy_unknown", never as a
    /// silent pass — the migration deliberately does not infer historical
    /// per-policy outcomes from `cf_agent_enabled`.
    #[test]
    fn policy_matrix_marks_pre_migration_rows_as_legacy_unknown() {
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "ancient-host".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({}),
            policy_requirements_met: None,
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        assert_eq!(matrix.policies, vec!["CF agent"]);
        assert_eq!(matrix.systems.len(), 1);
        let row = &matrix.systems[0];
        assert_eq!(row.results, vec!["legacy_unknown"]);
        assert_eq!(
            row.details,
            vec![Some("Policy results unavailable; re-evaluate.".to_string())]
        );
    }

    /// A row with policy_requirements_met = NULL that nonetheless has a
    /// populated policy_results document (should not happen in practice,
    /// but the check must be evidence-based, not merely "is it NULL") is
    /// evaluated normally rather than forced into legacy_unknown.
    #[test]
    fn policy_matrix_does_not_treat_populated_results_as_legacy_unknown() {
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "gray".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({
                "global": { "cfAgentEnabled": { "passed": true, "strict": true, "details": null } },
                "assigned": {}
            }),
            policy_requirements_met: None,
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        assert_eq!(matrix.systems[0].results, vec!["pass"]);
    }

    /// A non-strict failed policy must render as `warn`, not `fail` — a
    /// non-strict failure does not block deployment
    /// (`policy_requirements_met` only gates on strict failures), so
    /// displaying it as `fail` ("blocks deployment until resolved" in the
    /// UI) would contradict what actually happens to the build.
    #[test]
    fn non_strict_failed_policy_renders_as_warn_not_fail() {
        let policy_id = "f2752e95-40f5-4cb7-9211-7ac8fd98334f";
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "gray".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({
                "global": {
                    "cfAgentEnabled": { "passed": true, "strict": true, "details": null }
                },
                "assigned": {
                    policy_id: {
                        "name": "soft-check",
                        "type": "require_packages",
                        "strict": false,
                        "passed": false,
                        "details": "Missing optional package: htop"
                    }
                }
            }),
            policy_requirements_met: Some(true),
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        assert_eq!(matrix.systems[0].results, vec!["pass", "warn"]);
        assert_eq!(
            matrix.systems[0].details,
            vec![None, Some("Missing optional package: htop".to_string())]
        );
    }

    #[test]
    fn report_only_strict_failure_renders_as_warn_from_effective_blocking() {
        let result = serde_json::json!({
            "passed": false,
            "strict": true,
            "enforcement_mode": "report_only",
            "blocking": false,
            "details": "Report-only evidence failed"
        });

        assert_eq!(
            classify_policy_result(&result),
            ("warn", Some("Report-only evidence failed".to_string()))
        );
    }

    #[test]
    fn legacy_failure_without_blocking_falls_back_to_intrinsic_strict() {
        let strict = serde_json::json!({ "passed": false, "strict": true });
        let non_strict = serde_json::json!({ "passed": false, "strict": false });

        assert_eq!(classify_policy_result(&strict), ("fail", None));
        assert_eq!(classify_policy_result(&non_strict), ("warn", None));
    }

    #[test]
    fn malformed_blocking_metadata_is_an_infrastructure_error() {
        let result = serde_json::json!({
            "passed": true,
            "strict": true,
            "blocking": "false"
        });

        assert_eq!(
            classify_policy_result(&result),
            (
                "infrastructure_error",
                Some("policy result has invalid blocking metadata".to_string())
            )
        );
    }

    #[test]
    fn passed_policy_rejects_contradictory_blocking_evidence() {
        let contradictory = serde_json::json!({
            "passed": true,
            "strict": true,
            "blocking": true
        });
        let nonblocking = serde_json::json!({
            "passed": true,
            "strict": true,
            "blocking": false
        });
        let legacy = serde_json::json!({
            "passed": true,
            "strict": true
        });

        assert_eq!(
            classify_policy_result(&contradictory),
            (
                "infrastructure_error",
                Some("policy result has contradictory blocking metadata".to_string())
            )
        );
        assert_eq!(classify_policy_result(&nonblocking), ("pass", None));
        assert_eq!(classify_policy_result(&legacy), ("pass", None));
    }

    /// A strict failed policy must still render as `fail`.
    #[test]
    fn strict_failed_policy_still_renders_as_fail() {
        let policy_id = "f2752e95-40f5-4cb7-9211-7ac8fd98334f";
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "gray".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({
                "global": {
                    "cfAgentEnabled": { "passed": true, "strict": true, "details": null }
                },
                "assigned": {
                    policy_id: {
                        "name": "failme",
                        "type": "require_packages",
                        "strict": true,
                        "passed": false,
                        "details": "Missing required packages: grafana"
                    }
                }
            }),
            policy_requirements_met: Some(false),
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        assert_eq!(matrix.systems[0].results, vec!["pass", "fail"]);
    }

    /// The persisted matrix column label and "View policy definition"
    /// navigation must use the real DB policy name, not a generated
    /// description string.
    #[test]
    fn policy_matrix_column_uses_db_name_not_description() {
        let policy_id = "f2752e95-40f5-4cb7-9211-7ac8fd98334f";
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "gray".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({
                "global": {
                    "cfAgentEnabled": { "passed": true, "strict": true, "details": null }
                },
                "assigned": {
                    policy_id: {
                        "name": "failme",
                        "description": "Required packages: grafana",
                        "type": "require_packages",
                        "strict": true,
                        "passed": false,
                        "details": "Missing required packages: grafana"
                    }
                }
            }),
            policy_requirements_met: Some(false),
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        assert_eq!(matrix.policies, vec!["CF agent", "failme"]);
    }

    /// A derivation with status_id = 12 (BuildFailed) and a non-NULL
    /// error_message must NOT render as eval_failed in the matrix. The
    /// error_message may be set by a later build failure that occurred
    /// after the system successfully passed evaluation and policies.
    /// Only status_id = 6 (DryRunFailed) proves evaluation failure.
    #[test]
    fn build_failed_derivation_not_eval_failed() {
        for status in [7u32, 8, 10, 12] {
            let rows = vec![crate::queries::commits::EvalPolicySystemRow {
                cf_agent_enabled: None,
                system_name: format!("sys-status-{status}"),
                eval_status: "evaluated".to_string(),
                error_message: Some("Build failed: gcc segfault".to_string()),
                policy_results: serde_json::json!({
                    "global": { "cfAgentEnabled": { "passed": true, "strict": true, "details": null } },
                    "assigned": {}
                }),
                policy_requirements_met: Some(true),
            }];

            let matrix = build_eval_policy_matrix_response(1, rows);

            assert_eq!(
                matrix.systems[0].results,
                vec!["pass"],
                "status_id={status}: build state must not produce nix_eval_failure"
            );

            // Even with error_message set to something plausible, the
            // matrix must not show it as detail for the CF-agent cell
            // because the error is a build error, not an eval error.
            assert_eq!(
                matrix.systems[0].details,
                vec![None],
                "status_id={status}: build error detail must not appear in eval matrix"
            );
        }
    }

    /// A historical persisted result containing a legacy
    /// require_cf_agent assigned entry must produce only one
    /// column ("CF agent") and must not add a duplicate
    /// "Require Crystal Forge Agent" column — the global
    /// cfAgentEnabled metadata already covers this signal.
    #[test]
    fn policy_matrix_deduplicates_legacy_cf_agent_assigned_result() {
        let policy_id = "legacy-cf-agent-uuid";
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "gray".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({
                "global": {
                    "cfAgentEnabled": { "passed": true, "strict": true, "details": null }
                },
                "assigned": {
                    policy_id: {
                        "name": "Require Crystal Forge Agent",
                        "type": "require_cf_agent",
                        "strict": false,
                        "passed": false,
                        "details": null
                    }
                }
            }),
            policy_requirements_met: Some(true),
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        // Must have exactly one column ("CF agent"), not two.
        assert_eq!(
            matrix.policies,
            vec!["CF agent"],
            "legacy require_cf_agent assigned result must not produce a duplicate column"
        );
        // The system must still have a result for that single column.
        assert_eq!(matrix.systems.len(), 1);
        assert_eq!(
            matrix.systems[0].results,
            vec!["pass"],
            "the single CF-agent cell must reflect the global invariant result"
        );
    }

    /// A historical result with a DIFFERENT policy type assigned MUST
    /// produce that policy's column — only require_cf_agent is filtered
    /// from the deduplication.
    #[test]
    fn policy_matrix_does_not_filter_other_policy_types() {
        let policy_id = "some-other-uuid";
        let rows = vec![crate::queries::commits::EvalPolicySystemRow {
            cf_agent_enabled: None,
            system_name: "gray".to_string(),
            eval_status: "evaluated".to_string(),
            error_message: None,
            policy_results: serde_json::json!({
                "global": {
                    "cfAgentEnabled": { "passed": true, "strict": true, "details": null }
                },
                "assigned": {
                    policy_id: {
                        "name": "Require Grafana",
                        "type": "require_packages",
                        "strict": true,
                        "passed": false,
                        "details": "Missing required packages: grafana"
                    }
                }
            }),
            policy_requirements_met: Some(false),
        }];

        let matrix = build_eval_policy_matrix_response(1, rows);

        assert_eq!(
            matrix.policies,
            vec!["CF agent", "Require Grafana"],
            "non-require_cf_agent assigned policies must still appear"
        );
        assert_eq!(matrix.systems[0].results, vec!["pass", "fail"]);
    }
}
