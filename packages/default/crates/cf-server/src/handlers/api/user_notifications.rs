use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    NotificationEmailCapability, NotificationPreferencesDto, UpdateNotificationPreferences,
    UserNotificationDto, UserNotificationsResponse,
};
use crate::auth::extractors::AuthenticatedUser;
use crate::config::ServerConfig;
use crate::queries::admin::insert_admin_audit_event;
use crate::queries::auth_identity::get_user_by_id;
use crate::queries::user_notifications::{
    dismiss_notification, get_or_create_notification_preferences, list_notifications,
    mark_all_notifications_read, mark_notification_read,
    materialize_attention_notifications_for_user, unread_notification_count,
    update_notification_preferences,
};

#[derive(Debug, Deserialize)]
pub struct ListNotificationsParams {
    pub limit: Option<i64>,
    pub before: Option<DateTime<Utc>>,
    pub unread_only: Option<bool>,
}

pub async fn get_notification_preferences(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
) -> impl IntoResponse {
    let email_capability = notification_email_capability(&pool, user.user_id, &server_config).await;

    match get_or_create_notification_preferences(&pool, user.user_id).await {
        Ok(preferences) => (
            StatusCode::OK,
            Json(NotificationPreferencesDto::from_model(
                preferences,
                email_capability,
            )),
        )
            .into_response(),
        Err(err) => server_error(err, "notification_preferences_fetch_failed"),
    }
}

pub async fn patch_notification_preferences(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    Json(update): Json<UpdateNotificationPreferences>,
) -> impl IntoResponse {
    let email_capability = notification_email_capability(&pool, user.user_id, &server_config).await;
    if matches!(
        update.delivery_channel,
        Some(crate::api::models::NotificationDeliveryChannel::Email)
            | Some(crate::api::models::NotificationDeliveryChannel::Both)
    ) || update.weekly_digest == Some(true)
    {
        if !email_capability.available {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "email_unavailable",
                    "message": email_capability
                        .unavailable_reason
                        .clone()
                        .unwrap_or_else(|| "Email delivery is unavailable".to_string())
                })),
            )
                .into_response();
        }
    }

    match update_notification_preferences(&pool, user.user_id, &update).await {
        Ok(preferences) => {
            if let Err(err) = insert_admin_audit_event(
                &pool,
                user.user_id,
                &email_capability
                    .delivery_email
                    .clone()
                    .unwrap_or_else(|| user.user_id.to_string()),
                "notification_preferences_updated",
                "user_notification_preferences",
                None,
                serde_json::json!({
                    "deploy_failures": preferences.deploy_failures,
                    "build_failures": preferences.build_failures,
                    "critical_cves": preferences.critical_cves,
                    "policy_violations": preferences.policy_violations,
                    "heartbeat_lost": preferences.heartbeat_lost,
                    "weekly_digest": preferences.weekly_digest,
                    "delivery_channel": format!("{:?}", preferences.delivery_channel),
                }),
            )
            .await
            {
                tracing::warn!(%err, user_id = %user.user_id, "failed to audit notification preference update");
            }

            (
                StatusCode::OK,
                Json(NotificationPreferencesDto::from_model(
                    preferences,
                    email_capability,
                )),
            )
                .into_response()
        }
        Err(err) => server_error(err, "notification_preferences_update_failed"),
    }
}

pub async fn get_notifications(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    Query(params): Query<ListNotificationsParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    let unread_only = params.unread_only.unwrap_or(false);

    if let Err(err) = materialize_attention_notifications_for_user(&pool, user.user_id).await {
        tracing::warn!(%err, user_id = %user.user_id, "failed to materialize attention notifications");
    }

    let notifications =
        match list_notifications(&pool, user.user_id, limit, params.before, unread_only).await {
            Ok(notifications) => notifications,
            Err(err) => return server_error(err, "notifications_fetch_failed"),
        };
    let unread_count = match unread_notification_count(&pool, user.user_id).await {
        Ok(count) => count,
        Err(err) => return server_error(err, "notifications_count_failed"),
    };

    let next_cursor = if notifications.len() as i64 == limit {
        notifications
            .last()
            .map(|notification| notification.created_at)
    } else {
        None
    };

    (
        StatusCode::OK,
        Json(UserNotificationsResponse {
            notifications: notifications
                .into_iter()
                .map(UserNotificationDto::from)
                .collect(),
            unread_count,
            next_cursor,
        }),
    )
        .into_response()
}

pub async fn read_notification(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    Path(notification_id): Path<Uuid>,
) -> impl IntoResponse {
    match mark_notification_read(&pool, user.user_id, notification_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => server_error(err, "notification_mark_read_failed"),
    }
}

pub async fn read_all_notifications(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    match mark_all_notifications_read(&pool, user.user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => server_error(err, "notifications_mark_all_read_failed"),
    }
}

pub async fn delete_notification(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    Path(notification_id): Path<Uuid>,
) -> impl IntoResponse {
    match dismiss_notification(&pool, user.user_id, notification_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => server_error(err, "notification_dismiss_failed"),
    }
}

fn server_error(err: sqlx::Error, code: &'static str) -> axum::response::Response {
    tracing::error!(%err, error = code, "user notification API failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": code,
            "message": "Could not complete notification request"
        })),
    )
        .into_response()
}

async fn notification_email_capability(
    pool: &PgPool,
    user_id: Uuid,
    server_config: &ServerConfig,
) -> NotificationEmailCapability {
    let delivery_email = get_user_by_id(pool, user_id)
        .await
        .ok()
        .map(|u| u.email)
        .filter(|email| !email.trim().is_empty());

    let unavailable_reason = if !server_config.notification_email_enabled {
        Some("Email delivery is not configured for this deployment".to_string())
    } else if !server_config.notification_email_external_delivery_allowed {
        Some("Email delivery is disabled by this deployment's classification policy".to_string())
    } else if server_config
        .notification_email_endpoint
        .as_deref()
        .unwrap_or("")
        .trim()
        .is_empty()
        || server_config
            .notification_email_sender_address
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        Some("Email delivery is missing required transport configuration".to_string())
    } else if delivery_email.is_none() {
        Some("Your account does not have an email address".to_string())
    } else {
        None
    };

    NotificationEmailCapability {
        available: unavailable_reason.is_none(),
        delivery_email,
        unavailable_reason,
    }
}
