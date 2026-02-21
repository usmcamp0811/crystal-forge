use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::session::{
        CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, build_csrf_cookie,
        build_session_cookie, clear_csrf_cookie, clear_session_cookie, extract_cookie,
        generate_token, hash_token, parse_set_cookie_header,
    },
    queries::auth_identity::{create_user_session, invalidate_session_by_token_hash},
};

pub const SESSION_TTL_SECONDS: i64 = 60 * 60 * 8;
const SESSION_TTL_ENV: &str = "CRYSTAL_FORGE_SESSION_TTL_SECONDS";
const SESSION_TTL_MIN_SECONDS: i64 = 60;
const SESSION_TTL_MAX_SECONDS: i64 = 60 * 60 * 24 * 30;

pub struct SessionCookies {
    pub session_cookie: HeaderValue,
    pub csrf_cookie: HeaderValue,
}

pub async fn establish_user_session(
    pool: &PgPool,
    user_id: Uuid,
    user_agent: Option<String>,
    ip_address: Option<String>,
) -> Result<SessionCookies, SessionError> {
    let ttl_seconds = session_ttl_seconds();
    let session_token = generate_token();
    let csrf_token = generate_token();
    let session_hash = hash_token(&session_token);
    let expires_at = Utc::now() + Duration::seconds(ttl_seconds);

    create_user_session(
        pool,
        user_id,
        session_hash,
        expires_at,
        user_agent,
        ip_address,
    )
    .await
    .map_err(|_| SessionError::Database)?;

    Ok(SessionCookies {
        session_cookie: parse_set_cookie_header(&build_session_cookie(&session_token, ttl_seconds))
            .map_err(|_| SessionError::InvalidCookieHeader)?,
        csrf_cookie: parse_set_cookie_header(&build_csrf_cookie(&csrf_token, ttl_seconds))
            .map_err(|_| SessionError::InvalidCookieHeader)?,
    })
}

pub async fn logout(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, SessionError> {
    validate_csrf(&headers)?;

    if let Some(session_token) = extract_cookie(&headers, SESSION_COOKIE_NAME) {
        let session_hash = hash_token(&session_token);
        invalidate_session_by_token_hash(&pool, &session_hash)
            .await
            .map_err(|_| SessionError::Database)?;
    }

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

fn session_ttl_seconds() -> i64 {
    let parsed = match std::env::var(SESSION_TTL_ENV) {
        Ok(raw) => parse_session_ttl(Some(&raw)),
        Err(_) => SESSION_TTL_SECONDS,
    };

    parsed.clamp(SESSION_TTL_MIN_SECONDS, SESSION_TTL_MAX_SECONDS)
}

fn parse_session_ttl(raw: Option<&str>) -> i64 {
    match raw {
        Some(value) => match value.parse::<i64>() {
            Ok(parsed) if parsed > 0 => parsed,
            Ok(_) => {
                tracing::warn!(
                    "Ignoring non-positive {} value {}; using default {}",
                    SESSION_TTL_ENV,
                    value,
                    SESSION_TTL_SECONDS
                );
                SESSION_TTL_SECONDS
            }
            Err(_) => {
                tracing::warn!(
                    "Ignoring invalid {} value '{}'; using default {}",
                    SESSION_TTL_ENV,
                    value,
                    SESSION_TTL_SECONDS
                );
                SESSION_TTL_SECONDS
            }
        },
        None => SESSION_TTL_SECONDS,
    }
}

/// Reusable double-submit CSRF validation for state-changing cookie-auth endpoints.
///
/// Currently used by logout and intended to be reused by future cookie-auth write actions.
pub fn validate_csrf(headers: &HeaderMap) -> Result<(), SessionError> {
    let csrf_cookie =
        extract_cookie(headers, CSRF_COOKIE_NAME).ok_or(SessionError::MissingCsrfCookie)?;

    let csrf_header = headers
        .get(CSRF_HEADER_NAME)
        .and_then(|v| v.to_str().ok())
        .ok_or(SessionError::MissingCsrfHeader)?;

    if csrf_cookie != csrf_header {
        return Err(SessionError::CsrfMismatch);
    }

    Ok(())
}

#[derive(Debug)]
pub enum SessionError {
    MissingCsrfCookie,
    MissingCsrfHeader,
    CsrfMismatch,
    Database,
    InvalidCookieHeader,
}

impl IntoResponse for SessionError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            SessionError::MissingCsrfCookie => {
                (StatusCode::FORBIDDEN, "Missing CSRF cookie".to_string())
            }
            SessionError::MissingCsrfHeader => {
                (StatusCode::FORBIDDEN, "Missing CSRF header".to_string())
            }
            SessionError::CsrfMismatch => {
                (StatusCode::FORBIDDEN, "CSRF token mismatch".to_string())
            }
            SessionError::Database => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to persist session state".to_string(),
            ),
            SessionError::InvalidCookieHeader => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to construct cookie header".to_string(),
            ),
        };

        (status, message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;

    #[test]
    fn csrf_validation_succeeds_when_cookie_matches_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "__Host-cf-csrf=csrf123; __Host-cf-session=session"
                .parse()
                .unwrap(),
        );
        headers.insert(CSRF_HEADER_NAME, "csrf123".parse().unwrap());

        assert!(validate_csrf(&headers).is_ok());
    }

    #[test]
    fn csrf_validation_fails_on_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "__Host-cf-csrf=csrf123".parse().unwrap());
        headers.insert(CSRF_HEADER_NAME, "csrf456".parse().unwrap());

        assert!(matches!(
            validate_csrf(&headers),
            Err(SessionError::CsrfMismatch)
        ));
    }

    #[test]
    fn session_ttl_rejects_non_positive_and_invalid_values() {
        assert_eq!(parse_session_ttl(Some("0")), SESSION_TTL_SECONDS);
        assert_eq!(parse_session_ttl(Some("-10")), SESSION_TTL_SECONDS);
        assert_eq!(parse_session_ttl(Some("abc")), SESSION_TTL_SECONDS);
        assert_eq!(parse_session_ttl(Some("120")), 120);
    }
}
