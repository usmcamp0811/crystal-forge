//! Commit-related API handlers.

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures::StreamExt;

use crate::handlers::agent_request::CFState;

/// WebSocket endpoint for streaming evaluation logs
/// GET /api/v1/commits/:commit_id/eval/stream
pub async fn stream_eval_logs(
    ws: WebSocketUpgrade,
    Path(commit_id): Path<i32>,
    State(state): State<CFState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_eval_stream(socket, commit_id, state))
}

async fn handle_eval_stream(mut socket: WebSocket, commit_id: i32, state: CFState) {
    tracing::info!("📡 WebSocket connection established for commit {} evaluation", commit_id);
    
    // Get or create broadcast channel for this commit
    let mut rx = {
        let mut channels = state.eval_log_channels.lock().await;
        let tx = channels
            .entry(commit_id)
            .or_insert_with(|| {
                let (tx, _rx) = tokio::sync::broadcast::channel(1000);
                tracing::debug!("Created new broadcast channel for commit {}", commit_id);
                tx
            })
            .clone();
        tx.subscribe()
    };
    
    // Stream messages from the broadcast channel to this WebSocket client
    while let Ok(log_line) = rx.recv().await {
        if let Err(e) = socket.send(Message::Text(log_line)).await {
            tracing::error!("Failed to send eval log to WebSocket client: {}", e);
            break;
        }
    }
    
    tracing::info!("WebSocket connection closed for commit {} eval", commit_id);
}

/// Ensure a broadcast channel exists for this commit (create if needed)
pub async fn ensure_eval_channel(state: &CFState, commit_id: i32) {
    let mut channels = state.eval_log_channels.lock().await;
    channels.entry(commit_id).or_insert_with(|| {
        let (tx, _rx) = tokio::sync::broadcast::channel(1000);
        tracing::debug!("📡 Created broadcast channel for commit {} evaluation", commit_id);
        tx
    });
}

/// Helper function to broadcast a log line to all connected WebSocket clients for a commit
/// IMPORTANT: This will create a channel if one doesn't exist, so logs are buffered even if no clients are connected yet
pub async fn broadcast_eval_log(state: &CFState, commit_id: i32, log_line: String) {
    let mut channels = state.eval_log_channels.lock().await;
    let tx = channels.entry(commit_id).or_insert_with(|| {
        let (tx, _rx) = tokio::sync::broadcast::channel(1000);
        tracing::debug!("📡 Created broadcast channel for commit {} (first broadcast)", commit_id);
        tx
    });
    // Send to all subscribers (if channel is full, oldest messages are dropped)
    let _ = tx.send(log_line);
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
