//! Development-mode authentication endpoints.
//!
//! These endpoints are only available when AUTH_MODE=dev.

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::net::SocketAddr;
use tracing::error;

use crate::auth::dev_mode::{find_dev_user_by_email, is_valid_dev_user_email};
use crate::handlers::api::auth_session::establish_user_session;

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
/// Accepts a dev fixture email, establishes a session, and returns user information.
/// This endpoint should only be available when AUTH_MODE=dev.
pub async fn dev_login(
    State(pool): State<PgPool>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
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

            let user_agent = headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string);

            let ip_address = Some(addr.ip().to_string());

            // Establish session cookies
            let session_cookies = match establish_user_session(&pool, user.id, user_agent, ip_address).await {
                Ok(cookies) => cookies,
                Err(_) => {
                    error!("Failed to establish session for dev user {}", user.email);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            error: "session_error".to_string(),
                            message: "Failed to create session".to_string(),
                        }),
                    )
                        .into_response();
                }
            };

            let mut response = Json(DevLoginResponse {
                user_id: user.id.to_string(),
                email: user.email.clone(),
                display_name,
            })
            .into_response();

            // Attach session cookies
            response
                .headers_mut()
                .append(header::SET_COOKIE, session_cookies.session_cookie);
            response
                .headers_mut()
                .append(header::SET_COOKIE, session_cookies.csrf_cookie);

            response
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
