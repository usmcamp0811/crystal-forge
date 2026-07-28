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

use crate::api::models::{
    ApiError, CancelEvalOutcome, EvalDependencyGraphResponse, EvalDependencyPackageRow,
    EvalHistoryPage, EvalPolicyMatrixResponse, EvalPolicySystemRow, EvalQueueItem,
    EvalQueueSummary, ReorderEvalQueueRequest,
};
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{require_operator_or_admin, require_viewer_or_above};

const EVAL_LOG_CHANNEL_BUFFER: usize = 1000;
const MAX_EVAL_LOG_CHANNELS: usize = 1024;
const EVAL_LOG_HISTORY_BUFFER: usize = 2000;

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

/// List evaluation queue items.
/// GET /api/v1/commits/eval-queue
pub async fn list_eval_queue(
    State(state): State<CFState>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(200)
        .max(1)
        .min(crate::api::models::LIMIT_MAX);

    let rows = match crate::queries::commits::list_eval_queue(&state.pool, limit).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::error!("Failed to list eval queue: {}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let active_count = rows.first().map(|row| row.active_total_count).unwrap_or(0);
    let completed_count = rows
        .first()
        .map(|row| row.completed_total_count)
        .unwrap_or(0);
    let failed_count = rows.first().map(|row| row.failed_total_count).unwrap_or(0);

    let items = rows
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
            evaluation_status: row.evaluation_status,
            queue_position: row.queue_position,
            systems: row.systems,
            system_count: row.system_count,
            passed_count: row.passed_count,
            policy_failed_count: row.policy_failed_count,
            eval_failed_count: row.eval_failed_count,
        })
        .collect::<Vec<_>>();

    Json(EvalQueueSummary {
        active_count,
        completed_count,
        failed_count,
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

/// WebSocket endpoint for streaming evaluation logs
/// GET /api/v1/commits/:commit_id/eval/stream
pub async fn stream_eval_logs(
    ws: WebSocketUpgrade,
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    ws.on_upgrade(move |socket| handle_eval_stream(socket, commit_id, state))
}

async fn handle_eval_stream(mut socket: WebSocket, commit_id: i32, state: CFState) {
    tracing::info!(
        "📡 WebSocket connection established for commit {} evaluation",
        commit_id
    );

    // Get or create broadcast channel for this commit
    let Some(tx) = get_or_create_eval_channel(&state, commit_id).await else {
        tracing::warn!(
            "Rejecting eval websocket for commit {}: eval channel cap reached",
            commit_id
        );
        let _ = socket
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 1013,
                reason: "Server overloaded".into(),
            })))
            .await;
        return;
    };

    let history_snapshot = {
        let history = state.eval_log_history.lock().await;
        history.get(&commit_id).cloned().unwrap_or_default()
    };

    for log_line in history_snapshot {
        if let Err(e) = socket.send(Message::Text(log_line.into())).await {
            tracing::error!(
                "Failed to replay eval log history to WebSocket client for commit {}: {}",
                commit_id,
                e
            );
            return;
        }
    }

    let mut rx = tx.subscribe();
    let mut keepalive = interval(Duration::from_secs(20));

    // Stream messages from the broadcast channel to this WebSocket client
    loop {
        tokio::select! {
            recv = rx.recv() => {
                match recv {
                    Ok(log_line) => {
                        if let Err(e) = socket.send(Message::Text(log_line)).await {
                            tracing::error!("Failed to send eval log to WebSocket client: {}", e);
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
                if let Err(e) = socket.send(Message::Ping(Vec::new().into())).await {
                    tracing::debug!("Eval WebSocket ping failed for commit {}: {}", commit_id, e);
                    break;
                }
            }
        }
    }

    tracing::info!("WebSocket connection closed for commit {} eval", commit_id);
}

/// Fetch historical evaluation logs from database for a specific commit.
///
/// This endpoint retrieves persisted logs for completed/failed/cancelled evaluations.
/// For in-progress evaluations, clients should use the WebSocket stream instead.
pub async fn get_eval_logs_history(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let logs =
        match crate::queries::eval_logs::fetch_eval_logs_by_commit(&state.pool, commit_id).await {
            Ok(logs) => logs,
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
pub async fn get_eval_policy_matrix(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let rows = match crate::queries::commits::fetch_eval_policy_matrix(&state.pool, commit_id).await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(
                "Failed to fetch eval policy matrix for commit {}: {}",
                commit_id,
                e
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let body = build_eval_policy_matrix_response(commit_id, rows);

    Json(body).into_response()
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
                match cf_agent.and_then(|result| result.get("passed")) {
                    Some(value) if value.as_bool() == Some(true) => {
                        results.push("pass".to_string());
                        details.push(None);
                    }
                    Some(value) if value.as_bool() == Some(false) => {
                        results.push("fail".to_string());
                        details.push(
                            cf_agent
                                .and_then(|result| result.get("details"))
                                .and_then(|value| value.as_str())
                                .map(str::to_string)
                                .or_else(|| Some("Crystal Forge agent is disabled".to_string())),
                        );
                    }
                    _ => {
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
                    results.push("not_assigned".to_string());
                    details.push(None);
                    continue;
                };

                match policy_result.get("passed") {
                    Some(value) if value.as_bool() == Some(true) => {
                        results.push("pass".to_string());
                        details.push(None);
                    }
                    Some(value) if value.as_bool() == Some(false) => {
                        results.push("fail".to_string());
                        details.push(
                            policy_result
                                .get("details")
                                .and_then(|value| value.as_str())
                                .map(str::to_string),
                        );
                    }
                    _ => {
                        results.push("infrastructure_error".to_string());
                        details.push(Some("policy result missing or invalid".to_string()));
                    }
                }
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
    }
}

/// Fetch dependency/derivation package breakdown for a commit evaluation.
/// GET /api/v1/commits/:commit_id/eval/dependency-graph
pub async fn get_eval_dependency_graph(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let rows = match crate::queries::commits::fetch_eval_dependency_breakdown(
        &state.pool,
        commit_id,
    )
    .await
    {
        Ok(rows) => rows,
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

/// Ensure a broadcast channel exists for this commit (create if needed)
pub async fn ensure_eval_channel(state: &CFState, commit_id: i32) {
    let _ = get_or_create_eval_channel(state, commit_id).await;
}

/// Helper function to broadcast a log line to all connected WebSocket clients for a commit
/// IMPORTANT: This will create a channel if one doesn't exist, so logs are buffered even if no clients are connected yet
pub async fn broadcast_eval_log(state: &CFState, commit_id: i32, log_line: String) {
    let msg = EvalLogMessage::Log { message: log_line };
    broadcast_eval_message(state, commit_id, msg).await;
}

/// Broadcast a system status update
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

/// Broadcast an overall eval status update
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
    let Some(tx) = get_or_create_eval_channel(state, commit_id).await else {
        tracing::warn!(
            "Dropping eval broadcast for commit {}: eval channel cap reached",
            commit_id
        );
        return;
    };

    // Serialize to JSON
    if let Ok(json) = serde_json::to_string(&msg) {
        {
            let mut history = state.eval_log_history.lock().await;
            let entry = history.entry(commit_id).or_default();
            entry.push(json.clone());
            if entry.len() > EVAL_LOG_HISTORY_BUFFER {
                let overflow = entry.len() - EVAL_LOG_HISTORY_BUFFER;
                entry.drain(0..overflow);
            }
        }
        let _ = tx.send(json);
    }
}

async fn get_or_create_eval_channel(
    state: &CFState,
    commit_id: i32,
) -> Option<tokio::sync::broadcast::Sender<String>> {
    let mut channels = state.eval_log_channels.lock().await;
    if let Some(tx) = channels.get(&commit_id) {
        return Some(tx.clone());
    }

    if channels.len() >= MAX_EVAL_LOG_CHANNELS {
        return None;
    }

    let (tx, _rx) = tokio::sync::broadcast::channel(EVAL_LOG_CHANNEL_BUFFER);
    tracing::debug!("📡 Created broadcast channel for commit {}", commit_id);
    channels.insert(commit_id, tx.clone());
    Some(tx)
}

/// Cleanup broadcast channel when evaluation completes (with delay)
/// This allows late-connecting WebSocket clients to still receive log history
pub async fn cleanup_eval_channel(state: &CFState, commit_id: i32) {
    // Clone state for the delayed cleanup task
    let state = state.clone();

    // Spawn a background task to cleanup after 10 minutes
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(600)).await;

        let mut channels = state.eval_log_channels.lock().await;
        channels.remove(&commit_id);
        drop(channels);

        let mut history = state.eval_log_history.lock().await;
        history.remove(&commit_id);
        tracing::debug!(
            "Cleaned up broadcast channel for commit {} (after 10min delay)",
            commit_id
        );
    });

    tracing::debug!("Scheduled cleanup for commit {} in 10 minutes", commit_id);
}

/// Trigger manual re-evaluation for a commit
/// POST /api/v1/commits/:commit_id/re-evaluate
pub async fn re_evaluate_commit(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
) -> impl IntoResponse {
    match crate::queries::commits::reset_commit_evaluation(&state.pool, commit_id).await {
        Ok(_) => {
            state.queue_notifier.notify_eval_queue();
            (
                axum::http::StatusCode::OK,
                axum::Json(serde_json::json!({
                    "status": "ok",
                    "message": format!("Commit {} queued for re-evaluation", commit_id)
                })),
            )
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to reset evaluation: {}", e)
            })),
        ),
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

/// List paginated eval history (complete, failed, cancelled).
/// GET /api/v1/commits/eval-history
pub async fn list_eval_history(
    State(state): State<CFState>,
    headers: HeaderMap,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers)
        .await
        .is_none()
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    let page: i64 = params
        .get("page")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
        .max(1);
    let limit: i64 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
        .max(1)
        .min(crate::api::models::LIMIT_MAX);

    if (page - 1).checked_mul(limit).is_none() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let status_filter = params.get("status").map(|s| s.as_str());
    let flake_filter = params.get("flake").map(|s| s.as_str());

    match crate::queries::commits::list_eval_history(
        &state.pool,
        page,
        limit,
        status_filter,
        flake_filter,
    )
    .await
    {
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
    async fn eval_channel_fanout_and_cleanup() {
        let state = test_state();
        let commit_id = 42;

        ensure_eval_channel(&state, commit_id).await;

        let tx = {
            let channels = state.eval_log_channels.lock().await;
            channels.get(&commit_id).expect("channel exists").clone()
        };

        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        broadcast_eval_log(&state, commit_id, "hello".to_string()).await;

        let msg1 = rx1.recv().await.expect("first subscriber receives");
        let msg2 = rx2.recv().await.expect("second subscriber receives");

        assert_eq!(msg1, msg2);
        assert!(msg1.contains("hello"));

        cleanup_eval_channel(&state, commit_id).await;

        // Cleanup is now delayed (spawned task), so channel should still exist immediately after
        let channels = state.eval_log_channels.lock().await;
        assert!(
            channels.contains_key(&commit_id),
            "channel should still exist immediately after cleanup (delayed)"
        );

        drop(channels);
        let history = state.eval_log_history.lock().await;
        assert!(
            history.contains_key(&commit_id),
            "history should still exist immediately after cleanup (delayed)"
        );
    }

    #[tokio::test]
    async fn eval_log_history_is_buffered() {
        let state = test_state();
        let commit_id = 77;

        for i in 0..3 {
            broadcast_eval_log(&state, commit_id, format!("line-{i}")).await;
        }

        let history = state.eval_log_history.lock().await;
        let entries = history.get(&commit_id).expect("history should exist");
        assert_eq!(entries.len(), 3);
        assert!(entries[0].contains("line-0"));
        assert!(entries[2].contains("line-2"));
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
                system_name: "gray".to_string(),
                eval_status: "evaluated".to_string(),
                error_message: None,
                policy_results: policy_result.clone(),
                policy_requirements_met: Some(false),
            },
            crate::queries::commits::EvalPolicySystemRow {
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
}
