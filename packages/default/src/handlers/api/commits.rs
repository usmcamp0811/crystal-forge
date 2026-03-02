//! Commit-related API handlers.

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Serialize, Deserialize};
use tokio::time::{interval, Duration};

use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::require_viewer_or_above;

const EVAL_LOG_CHANNEL_BUFFER: usize = 1000;
const MAX_EVAL_LOG_CHANNELS: usize = 1024;

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
}

/// WebSocket endpoint for streaming evaluation logs
/// GET /api/v1/commits/:commit_id/eval/stream
pub async fn stream_eval_logs(
    ws: WebSocketUpgrade,
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&state.pool, &headers).await.is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }

    ws.on_upgrade(move |socket| handle_eval_stream(socket, commit_id, state))
}

async fn handle_eval_stream(mut socket: WebSocket, commit_id: i32, state: CFState) {
    tracing::info!("📡 WebSocket connection established for commit {} evaluation", commit_id);
    
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
    let msg = EvalLogMessage::SystemStatus { system, status, error };
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

/// Cleanup broadcast channel when evaluation completes
pub async fn cleanup_eval_channel(state: &CFState, commit_id: i32) {
    let mut channels = state.eval_log_channels.lock().await;
    channels.remove(&commit_id);
    tracing::debug!("Cleaned up broadcast channel for commit {}", commit_id);
}

/// Trigger manual re-evaluation for a commit
/// POST /api/v1/commits/:commit_id/re-evaluate
pub async fn re_evaluate_commit(
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
) -> impl IntoResponse {
    match crate::queries::commits::reset_commit_evaluation(&state.pool, commit_id).await {
        Ok(_) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({
                "status": "ok",
                "message": format!("Commit {} queued for re-evaluation", commit_id)
            })),
        ),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to reset evaluation: {}", e)
            })),
        ),
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
        CFState::new(pool, ServerConfig::default())
    }

    #[tokio::test]
    async fn eval_channel_fanout_and_cleanup() {
        let state = test_state();
        let commit_id = 42;

        ensure_eval_channel(&state, commit_id).await;

        let tx = {
            let channels = state.eval_log_channels.lock().await;
            channels
                .get(&commit_id)
                .expect("channel exists")
                .clone()
        };

        let mut rx1 = tx.subscribe();
        let mut rx2 = tx.subscribe();

        broadcast_eval_log(&state, commit_id, "hello".to_string()).await;

        let msg1 = rx1.recv().await.expect("first subscriber receives");
        let msg2 = rx2.recv().await.expect("second subscriber receives");

        assert_eq!(msg1, msg2);
        assert!(msg1.contains("hello"));

        cleanup_eval_channel(&state, commit_id).await;
        let channels = state.eval_log_channels.lock().await;
        assert!(!channels.contains_key(&commit_id));
    }
}
