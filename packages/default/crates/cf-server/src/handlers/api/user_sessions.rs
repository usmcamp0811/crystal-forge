use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{UserSessionDto, UserSessionsResponse};
use crate::auth::extractors::AuthenticatedUser;
use crate::auth::session::{
    SESSION_COOKIE_NAME, clear_csrf_cookie, clear_session_cookie, extract_cookie, hash_token,
    parse_set_cookie_header,
};
use crate::handlers::api::auth_session::{SessionError, validate_csrf};
use crate::models::auth_identity::UserSession;
use crate::queries::admin::insert_admin_audit_event;
use crate::queries::auth_identity::{
    find_active_session_by_token_hash, invalidate_all_user_sessions, invalidate_user_session_by_id,
    list_active_sessions_for_user,
};

pub async fn list_sessions(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let current_session_id = match current_session_id(&pool, &headers).await {
        Ok(Some(id)) => id,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(response) => return response,
    };

    match list_active_sessions_for_user(&pool, user.user_id).await {
        Ok(mut sessions) => {
            sessions.sort_by(|left, right| {
                let left_current = left.id == current_session_id;
                let right_current = right.id == current_session_id;
                right_current
                    .cmp(&left_current)
                    .then_with(|| right.last_seen_at.cmp(&left.last_seen_at))
                    .then_with(|| right.issued_at.cmp(&left.issued_at))
            });

            (
                StatusCode::OK,
                Json(UserSessionsResponse {
                    sessions: sessions
                        .into_iter()
                        .map(|session| session_to_dto(session, current_session_id))
                        .collect(),
                }),
            )
                .into_response()
        }
        Err(err) => server_error(err, "sessions_fetch_failed"),
    }
}

pub async fn revoke_session(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(session_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(err) = validate_csrf(&headers) {
        return err.into_response();
    }

    let current_session_id = match current_session_id(&pool, &headers).await {
        Ok(Some(id)) => id,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(response) => return response,
    };

    if session_id == current_session_id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "cannot_revoke_current_session",
                "message": "Use sign out to end the current session"
            })),
        )
            .into_response();
    }

    match invalidate_user_session_by_id(&pool, user.user_id, session_id).await {
        Ok(true) => {
            audit_session_event(
                &pool,
                user.user_id,
                "session_revoked",
                serde_json::json!({ "session_id": session_id }),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => server_error(err, "session_revoke_failed"),
    }
}

pub async fn revoke_all_sessions(
    user: AuthenticatedUser,
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, SessionError> {
    validate_csrf(&headers)?;
    invalidate_all_user_sessions(&pool, user.user_id)
        .await
        .map_err(|_| SessionError::Database)?;
    audit_session_event(
        &pool,
        user.user_id,
        "all_sessions_revoked",
        serde_json::json!({ "included_current_session": true }),
    )
    .await;

    let mut response = StatusCode::NO_CONTENT.into_response();
    let clear_session = parse_set_cookie_header(&clear_session_cookie())
        .map_err(|_| SessionError::InvalidCookieHeader)?;
    let clear_csrf = parse_set_cookie_header(&clear_csrf_cookie())
        .map_err(|_| SessionError::InvalidCookieHeader)?;
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_session);
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_csrf);
    Ok(response)
}

async fn current_session_id(
    pool: &PgPool,
    headers: &HeaderMap,
) -> Result<Option<Uuid>, axum::response::Response> {
    let Some(token) = extract_cookie(headers, SESSION_COOKIE_NAME) else {
        return Ok(None);
    };
    let token_hash = hash_token(&token);
    find_active_session_by_token_hash(pool, &token_hash)
        .await
        .map(|session| session.map(|session| session.id))
        .map_err(|err| server_error(err, "current_session_lookup_failed"))
}

fn session_to_dto(session: UserSession, current_session_id: Uuid) -> UserSessionDto {
    let (browser, operating_system, device_class) = parse_user_agent(session.user_agent.as_deref());
    let device_label = format!("{operating_system} · {browser}");
    UserSessionDto {
        id: session.id,
        current: session.id == current_session_id,
        device_label,
        browser,
        operating_system,
        device_class,
        ip_address: session.ip_address,
        auth_source: session.auth_source,
        created_at: session.issued_at,
        last_seen_at: session.last_seen_at,
        expires_at: session.expires_at,
    }
}

fn parse_user_agent(user_agent: Option<&str>) -> (String, String, String) {
    let Some(ua) = user_agent else {
        return (
            "Unknown browser".to_string(),
            "Unknown OS".to_string(),
            "Unknown device".to_string(),
        );
    };

    let browser = if ua.contains("Firefox/") {
        "Firefox"
    } else if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Chrome/") || ua.contains("Chromium/") {
        "Chrome"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        "Unknown browser"
    };
    let operating_system = if ua.contains("Android") {
        "Android"
    } else if ua.contains("iPhone") || ua.contains("iPad") {
        "iOS"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Mac OS X") || ua.contains("Macintosh") {
        "macOS"
    } else if ua.contains("Linux") {
        "Linux"
    } else {
        "Unknown OS"
    };
    let device_class = if ua.contains("Mobile") || ua.contains("Android") || ua.contains("iPhone") {
        "mobile"
    } else if ua.contains("iPad") || ua.contains("Tablet") {
        "tablet"
    } else if browser == "Unknown browser" && operating_system == "Unknown OS" {
        "Unknown device"
    } else {
        "desktop"
    };

    (
        browser.to_string(),
        operating_system.to_string(),
        device_class.to_string(),
    )
}

fn server_error<E: std::fmt::Display>(err: E, code: &'static str) -> axum::response::Response {
    tracing::error!(%err, error = code, "user sessions API failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": code,
            "message": "Could not complete session request"
        })),
    )
        .into_response()
}

async fn audit_session_event(
    pool: &PgPool,
    user_id: Uuid,
    action: &'static str,
    metadata: serde_json::Value,
) {
    if let Err(err) = insert_admin_audit_event(
        pool,
        user_id,
        &user_id.to_string(),
        action,
        "user_sessions",
        None,
        metadata,
    )
    .await
    {
        tracing::warn!(%err, %user_id, action, "failed to audit user session event");
    }
}

#[cfg(test)]
mod tests {
    use super::parse_user_agent;

    #[test]
    fn user_sessions_parse_chrome_linux_desktop() {
        let (browser, operating_system, device_class) = parse_user_agent(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ));

        assert_eq!(browser, "Chrome");
        assert_eq!(operating_system, "Linux");
        assert_eq!(device_class, "desktop");
    }

    #[test]
    fn user_sessions_parse_android_before_linux() {
        let (browser, operating_system, device_class) = parse_user_agent(Some(
            "Mozilla/5.0 (Linux; Android 14; Pixel) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36",
        ));

        assert_eq!(browser, "Chrome");
        assert_eq!(operating_system, "Android");
        assert_eq!(device_class, "mobile");
    }

    #[test]
    fn user_sessions_unknown_agent_uses_explicit_unknowns() {
        let (browser, operating_system, device_class) = parse_user_agent(None);

        assert_eq!(browser, "Unknown browser");
        assert_eq!(operating_system, "Unknown OS");
        assert_eq!(device_class, "Unknown device");
    }
}
