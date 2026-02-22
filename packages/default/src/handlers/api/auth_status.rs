//! Authentication status and system setup endpoints.

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::config::ServerConfig;

/// Response for setup status check.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    /// Whether initial setup is required (no users exist).
    pub requires_setup: bool,
    /// Whether registration is allowed (config setting).
    pub allow_registration: bool,
    /// Number of users in the system.
    pub user_count: i64,
}

/// Check if initial system setup is required.
///
/// Returns true if no users exist in the database (first-run scenario).
/// This is used by the UI to show the registration form for the initial admin user.
pub async fn setup_status(
    State(pool): State<PgPool>,
    State(config): State<ServerConfig>,
) -> impl IntoResponse {
    let user_count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Failed to count users: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(SetupStatusResponse {
                    requires_setup: false,
                    allow_registration: false,
                    user_count: 0,
                }),
            );
        }
    };

    (
        StatusCode::OK,
        Json(SetupStatusResponse {
            requires_setup: user_count == 0,
            allow_registration: config.allow_registration,
            user_count,
        }),
    )
}
