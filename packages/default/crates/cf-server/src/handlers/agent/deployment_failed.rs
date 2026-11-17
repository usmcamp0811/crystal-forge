use crate::handlers::agent_request::{CFState, authenticate_agent_request_with_lookup};
use crate::queries::system_events::{
    insert_deployment_failed_event_tx, mark_pending_deployment_failed_tx,
};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::debug;

const MAX_FAILURE_MESSAGE_LEN: usize = 2000;

#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentFailedRequest {
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub target_store_path: String,
    #[serde(default)]
    pub error: String,
    /// Contains only diagnostics explicitly marked safe for display by the agent.
    #[serde(default)]
    pub display_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeploymentFailedResponse {
    pub status: String,
    pub message: String,
}

pub async fn report(
    State(_state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let agent_request =
        match authenticate_agent_request_with_lookup(&headers, body.clone(), &pool).await {
            Ok(req) => req,
            Err(status) => return status.into_response(),
        };

    let payload = match serde_json::from_slice::<DeploymentFailedRequest>(&body) {
        Ok(payload) => payload,
        Err(error) => {
            debug!(?error, "failed to deserialize deployment-failed report");
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    if payload.target_store_path.trim().is_empty()
        || !payload.target_store_path.starts_with("/nix/store/")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if !payload.hostname.trim().is_empty() && payload.hostname != agent_request.system.hostname {
        return StatusCode::FORBIDDEN.into_response();
    }

    if payload.error.trim().is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let display_failure_message = display_failure_message(payload.display_error.as_deref());
    // SECURITY: Legacy `error` is untrusted diagnostic text, never banner text.
    let failure_message = display_failure_message
        .as_deref()
        .unwrap_or("Deployment failed (diagnostic details unavailable)");

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            debug!(?error, "failed to begin deployment-failed transaction");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let pending = match mark_pending_deployment_failed_tx(
        &mut tx,
        agent_request.system.id,
        payload.target_store_path.trim(),
        failure_message,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            debug!(?error, "failed to mark pending deployment failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let Some(pending) = pending else {
        if let Err(error) = tx.commit().await {
            debug!(
                ?error,
                "failed to commit no-op deployment-failed transaction"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        return (
            StatusCode::OK,
            Json(DeploymentFailedResponse {
                status: "ok".to_string(),
                message: "No matching pending deployment".to_string(),
            }),
        )
            .into_response();
    };

    if let Err(error) =
        insert_deployment_failed_event_tx(&mut tx, &pending, display_failure_message.as_deref())
            .await
    {
        debug!(?error, "failed to insert deployment-failed event");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(error) = tx.commit().await {
        debug!(?error, "failed to commit deployment-failed transaction");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    (
        StatusCode::OK,
        Json(DeploymentFailedResponse {
            status: "ok".to_string(),
            message: "Deployment failure recorded".to_string(),
        }),
    )
        .into_response()
}

fn display_failure_message(message: Option<&str>) -> Option<String> {
    message
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(|message| message.chars().take(MAX_FAILURE_MESSAGE_LEN).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_report_has_no_display_diagnostics() {
        let payload: DeploymentFailedRequest = serde_json::from_value(serde_json::json!({
            "error": "legacy private diagnostic"
        }))
        .unwrap();
        assert_eq!(
            display_failure_message(payload.display_error.as_deref()),
            None
        );
    }

    #[test]
    fn display_diagnostics_are_trimmed_and_bounded() {
        assert_eq!(
            display_failure_message(Some("  copy failed  ")).as_deref(),
            Some("copy failed")
        );
        assert_eq!(display_failure_message(Some(" \n ")), None);
        assert_eq!(
            display_failure_message(Some(&"é".repeat(2001)))
                .unwrap()
                .chars()
                .count(),
            2000
        );
    }
}
