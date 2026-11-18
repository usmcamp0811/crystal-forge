use crate::handlers::agent_request::{CFState, authenticate_agent_request_with_lookup};
use crate::queries::system_events::{
    insert_deployment_failed_event_tx, mark_reported_deployment_failed_tx,
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

/// Accepts authenticated failure reports from current and legacy agents.
///
/// Only structured failure data can supply display information. Partial data
/// remains distinguishable from an agent without structured-report support.
#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentFailedRequest {
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub target_store_path: String,
    #[serde(default)]
    pub error: String,
    /// Retains wire compatibility; free-form text is never used for display.
    #[serde(default)]
    pub display_error: Option<String>,
    /// Preserves structured input, including invalid partial reports, for projection.
    #[serde(default)]
    pub failure: Option<serde_json::Value>,
    /// Binds a detached callback to its exact request; mismatches never fall back.
    #[serde(default)]
    pub deployment_id: Option<uuid::Uuid>,
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
    // SECURITY: Neither legacy error field proves redaction. Persist only a
    // server-derived summary; role-specific diagnostics use structured metadata.
    let metadata = serde_json::json!({ "deployment_failure": payload.failure });
    let failure = crate::api::models::project_failure(&metadata, crate::auth::models::Role::Viewer);
    let failure_message = failure.summary.as_str();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(error) => {
            debug!(?error, "failed to begin deployment-failed transaction");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let structured_failure = payload.failure.as_ref().and_then(|value| {
        serde_json::from_value::<cf_protocol::deployment_failure::DeploymentFailure>(value.clone())
            .ok()
    });
    let pending = match mark_reported_deployment_failed_tx(
        &mut tx,
        agent_request.system.id,
        payload.target_store_path.trim(),
        failure_message,
        payload.deployment_id,
        structured_failure.as_ref(),
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
        insert_deployment_failed_event_tx(&mut tx, &pending, payload.failure.as_ref()).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_report_has_no_display_diagnostics() {
        let payload: DeploymentFailedRequest = serde_json::from_value(serde_json::json!({
            "error": "legacy private diagnostic"
        }))
        .unwrap();
        assert_eq!(payload.failure, None);
        assert_eq!(payload.deployment_id, None);
    }

    #[test]
    fn callback_request_retains_exact_id_and_rejects_invalid_uuid() {
        let id = uuid::Uuid::new_v4();
        let mut value = serde_json::json!({
            "deployment_id":id, "target_store_path":"/nix/store/callback-system",
            "error":"the deployment activation command failed",
            "failure":{"stage":"activation", "code":"activation_execution", "exit_code":17}
        });
        let request: DeploymentFailedRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(request.deployment_id, Some(id));
        let failure: cf_protocol::deployment_failure::DeploymentFailure =
            serde_json::from_value(request.failure.unwrap()).unwrap();
        assert_eq!(
            failure.code,
            cf_protocol::deployment_failure::FailureCode::ActivationExecution
        );
        value["deployment_id"] = serde_json::json!("not-a-uuid");
        assert!(serde_json::from_value::<DeploymentFailedRequest>(value).is_err());
    }

    #[test]
    fn partial_report_remains_distinct_from_legacy() {
        let payload: DeploymentFailedRequest = serde_json::from_value(serde_json::json!({
            "error": "private", "display_error": "private", "failure": {"stage": "activation"}
        }))
        .unwrap();
        let view = crate::api::models::project_failure(
            &serde_json::json!({"deployment_failure": payload.failure}),
            crate::auth::models::Role::Admin,
        );
        assert_eq!(view.reason_status, "incomplete_report");
        assert_eq!(view.stage, "Activation");
        assert!(view.code.is_none());
        assert!(!serde_json::to_string(&view).unwrap().contains("private"));
    }
}
