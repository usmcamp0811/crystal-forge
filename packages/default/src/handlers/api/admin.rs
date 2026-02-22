use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    AdminUserSummary, ApiError, AuditAction, AuditEvent, PaginatedResponse, Role,
};
use crate::auth::session::{SESSION_COOKIE_NAME, extract_cookie, hash_token};
use crate::models::auth_identity::AuthRole;
use crate::queries::auth_identity::{get_session_by_token_hash, get_user_roles};

#[derive(Debug, sqlx::FromRow)]
struct AdminUserRow {
    id: Uuid,
    username: String,
    email: String,
    is_active: bool,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct AuditRoleRow {
    created_at: DateTime<Utc>,
    actor_email: Option<String>,
    role: AuthRole,
    target_email: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AuditSessionRow {
    invalidated_at: DateTime<Utc>,
    user_email: Option<String>,
}

pub async fn list_users(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    let Some(_admin_user) = require_admin(&pool, &headers).await else {
        return forbidden();
    };

    let rows = match sqlx::query_as::<_, AdminUserRow>(
        "SELECT id, username, email, is_active, updated_at FROM users ORDER BY updated_at DESC",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to list admin users"),
    };

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let role = match get_user_roles(&pool, row.id).await {
            Ok(roles) => highest_role(roles.into_iter().map(|r| r.role).collect()),
            Err(_) => None,
        };

        result.push(AdminUserSummary {
            id: row.id.to_string(),
            identifier: if row.username.trim().is_empty() {
                row.email
            } else {
                format!("{} ({})", row.username, row.email)
            },
            role,
            enabled: row.is_active,
            environments: vec![],
            updated_at: row.updated_at,
        });
    }

    (StatusCode::OK, Json(result)).into_response()
}

pub async fn list_audit_events(
    State(pool): State<PgPool>,
    Query(params): Query<AuditEventsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(_admin_user) = require_admin(&pool, &headers).await else {
        return forbidden();
    };

    let mut events: Vec<AuditEvent> = vec![];

    let role_events = match sqlx::query_as::<_, AuditRoleRow>(
        "SELECT ura.created_at,
                actor.email AS actor_email,
                ura.role,
                target.email AS target_email
         FROM user_role_assignments ura
         LEFT JOIN users actor ON actor.id = ura.granted_by_user_id
         JOIN users target ON target.id = ura.user_id
         ORDER BY ura.created_at DESC
         LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return internal_error("Failed to load audit events"),
    };

    for row in role_events {
        events.push(AuditEvent {
            timestamp: row.created_at,
            actor: row.actor_email,
            action: AuditAction::UserRoleAssigned,
            target: format!("{} -> {:?}", row.target_email, row.role),
            source: "user_role_assignments".to_string(),
        });
    }

    let session_events = match sqlx::query_as::<_, AuditSessionRow>(
        "SELECT us.invalidated_at, u.email AS user_email
         FROM user_sessions us
         LEFT JOIN users u ON u.id = us.user_id
         WHERE us.invalidated_at IS NOT NULL
         ORDER BY us.invalidated_at DESC
         LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return internal_error("Failed to load audit events"),
    };

    for row in session_events {
        events.push(AuditEvent {
            timestamp: row.invalidated_at,
            actor: row.user_email.clone(),
            action: AuditAction::SessionInvalidated,
            target: row.user_email.unwrap_or_else(|| "unknown user".to_string()),
            source: "user_sessions".to_string(),
        });
    }

    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let filtered = match apply_audit_filters(events, &params) {
        Ok(value) => value,
        Err(message) => return bad_request(&message),
    };

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(25).clamp(1, 200);

    let paginated = paginate_events(filtered, page, per_page);

    (StatusCode::OK, Json(paginated)).into_response()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuditEventsQuery {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

fn apply_audit_filters(
    mut events: Vec<AuditEvent>,
    params: &AuditEventsQuery,
) -> Result<Vec<AuditEvent>, String> {
    let actor_filter = params
        .actor
        .as_ref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    let action_filter = params
        .action
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_action_filter)
        .transpose()?
        .flatten();

    let from = params
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_rfc3339)
        .transpose()?;

    let to = params
        .to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_rfc3339)
        .transpose()?;

    if let (Some(start), Some(end)) = (from, to) {
        if start > end {
            return Err("`from` must be less than or equal to `to`".to_string());
        }
    }

    events.retain(|event| {
        if let Some(actor) = &actor_filter {
            let event_actor = event
                .actor
                .as_deref()
                .unwrap_or("system")
                .to_ascii_lowercase();
            if !event_actor.contains(actor) {
                return false;
            }
        }

        if let Some(action) = action_filter {
            if event.action != action {
                return false;
            }
        }

        if let Some(start) = from {
            if event.timestamp < start {
                return false;
            }
        }

        if let Some(end) = to {
            if event.timestamp > end {
                return false;
            }
        }

        true
    });

    Ok(events)
}

fn parse_action_filter(value: &str) -> Result<Option<AuditAction>, String> {
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "" => Ok(None),
        "user_role_assigned" => Ok(Some(AuditAction::UserRoleAssigned)),
        "session_invalidated" => Ok(Some(AuditAction::SessionInvalidated)),
        _ => Err(format!(
            "invalid action `{value}`; expected user_role_assigned or session_invalidated"
        )),
    }
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| format!("invalid RFC3339 timestamp `{value}`"))
}

fn paginate_events(
    events: Vec<AuditEvent>,
    page: i64,
    per_page: i64,
) -> PaginatedResponse<AuditEvent> {
    let total = events.len() as i64;
    let start_index = ((page - 1) * per_page).max(0) as usize;
    let items = if start_index >= events.len() {
        vec![]
    } else {
        events
            .into_iter()
            .skip(start_index)
            .take(per_page as usize)
            .collect()
    };

    PaginatedResponse {
        items,
        total,
        page,
        per_page,
    }
}

async fn require_admin(pool: &PgPool, headers: &HeaderMap) -> Option<Uuid> {
    let token = extract_cookie(headers, SESSION_COOKIE_NAME)?;
    let token_hash = hash_token(&token);
    let session = get_session_by_token_hash(pool, &token_hash).await.ok()??;

    if session.is_expired() || session.is_invalidated() {
        return None;
    }

    let roles = get_user_roles(pool, session.user_id).await.ok()?;
    let is_admin = roles.iter().any(|r| r.role == AuthRole::Admin);
    if is_admin {
        Some(session.user_id)
    } else {
        None
    }
}

fn highest_role(roles: Vec<AuthRole>) -> Option<Role> {
    if roles.contains(&AuthRole::Admin) {
        Some(Role::Admin)
    } else if roles.contains(&AuthRole::Operator) {
        Some(Role::Operator)
    } else if roles.contains(&AuthRole::Viewer) {
        Some(Role::Viewer)
    } else {
        None
    }
}

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Admin role required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "internal_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "validation_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn highest_role_prefers_admin_then_operator_then_viewer() {
        assert_eq!(highest_role(vec![AuthRole::Viewer]), Some(Role::Viewer));
        assert_eq!(
            highest_role(vec![AuthRole::Viewer, AuthRole::Operator]),
            Some(Role::Operator)
        );
        assert_eq!(
            highest_role(vec![AuthRole::Viewer, AuthRole::Admin]),
            Some(Role::Admin)
        );
        assert_eq!(highest_role(vec![]), None);
    }

    fn event(timestamp: DateTime<Utc>, actor: Option<&str>, action: AuditAction) -> AuditEvent {
        AuditEvent {
            timestamp,
            actor: actor.map(str::to_string),
            action,
            target: "target".to_string(),
            source: "source".to_string(),
        }
    }

    #[test]
    fn apply_audit_filters_filters_by_actor_and_action() {
        let events = vec![
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 10, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("admin@example.com"),
                AuditAction::UserRoleAssigned,
            ),
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 11, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("operator@example.com"),
                AuditAction::SessionInvalidated,
            ),
        ];

        let params = AuditEventsQuery {
            actor: Some("admin".to_string()),
            action: Some("user_role_assigned".to_string()),
            ..Default::default()
        };

        let filtered = apply_audit_filters(events, &params).expect("filter succeeds");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].action, AuditAction::UserRoleAssigned);
    }

    #[test]
    fn apply_audit_filters_rejects_invalid_range() {
        let params = AuditEventsQuery {
            from: Some("2026-02-21T12:00:00Z".to_string()),
            to: Some("2026-02-21T10:00:00Z".to_string()),
            ..Default::default()
        };

        let err = apply_audit_filters(vec![], &params).expect_err("range should fail");
        assert!(err.contains("from"));
    }

    #[test]
    fn paginate_events_returns_expected_window() {
        let events = vec![
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 10, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("a@example.com"),
                AuditAction::UserRoleAssigned,
            ),
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 11, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("b@example.com"),
                AuditAction::SessionInvalidated,
            ),
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 12, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("c@example.com"),
                AuditAction::SessionInvalidated,
            ),
        ];

        let page = paginate_events(events, 2, 2);
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1);
    }
}
