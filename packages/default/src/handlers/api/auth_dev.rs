//! Development-mode authentication endpoints.
//!
//! These endpoints are only available when AUTH_MODE=dev.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::error;

use crate::auth::dev_mode::{find_dev_user_by_email, is_valid_dev_user_email};

#[derive(Debug, Deserialize)]
pub struct DevLoginRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct DevLoginResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

/// Development mode login endpoint.
///
/// Accepts a dev fixture email and returns user information.
/// This endpoint should only be available when AUTH_MODE=dev.
pub async fn dev_login(
    State(pool): State<PgPool>,
    Json(payload): Json<DevLoginRequest>,
) -> impl IntoResponse {
    if !is_valid_dev_user_email(&payload.email) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "invalid_email".to_string(),
                message: "Not a valid dev fixture email".to_string(),
            }),
        )
            .into_response();
    }

    match find_dev_user_by_email(&pool, &payload.email).await {
        Ok(user) => {
            let display_name = match (&user.first_name, &user.last_name) {
                (Some(first), Some(last)) => Some(format!("{} {}", first, last)),
                (Some(first), None) => Some(first.clone()),
                _ => None,
            };

            (
                StatusCode::OK,
                Json(DevLoginResponse {
                    user_id: user.id.to_string(),
                    email: user.email.clone(),
                    display_name,
                }),
            )
                .into_response()
        }
        Err(e) => {
            error!("Dev login failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to authenticate dev user".to_string(),
                }),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_login_request_deserializes() {
        let json = r#"{"email":"dev-admin@crystal-forge.local"}"#;
        let req: DevLoginRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.email, "dev-admin@crystal-forge.local");
    }
}
