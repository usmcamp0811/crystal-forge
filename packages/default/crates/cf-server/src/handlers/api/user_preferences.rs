use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use sqlx::PgPool;

use crate::api::models::{UpdateUserPreferences, UserPreferencesResponse};
use crate::auth::extractors::AuthenticatedUser;
use crate::queries::user_preferences::{get_user_preferences, update_user_preferences};

pub async fn get_preferences(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    match get_user_preferences(&pool, user.user_id).await {
        Ok(preferences) => (
            StatusCode::OK,
            Json(UserPreferencesResponse::new(preferences)),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(%err, user_id = %user.user_id, "failed to fetch user preferences");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "preferences_fetch_failed",
                    "message": "Could not load user preferences",
                })),
            )
                .into_response()
        }
    }
}

pub async fn patch_preferences(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    Json(update): Json<UpdateUserPreferences>,
) -> impl IntoResponse {
    match update_user_preferences(&pool, user.user_id, &update).await {
        Ok(preferences) => (
            StatusCode::OK,
            Json(UserPreferencesResponse::from(preferences)),
        )
            .into_response(),
        Err(err) => {
            tracing::error!(%err, user_id = %user.user_id, "failed to update user preferences");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "preferences_update_failed",
                    "message": "Could not save user preferences",
                })),
            )
                .into_response()
        }
    }
}
