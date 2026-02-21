use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::session::{
        CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, build_csrf_cookie,
        build_session_cookie, clear_csrf_cookie, clear_session_cookie, extract_cookie,
        generate_token, hash_token,
    },
    queries::auth_identity::{create_user_session, invalidate_session_by_token_hash},
};

pub const SESSION_TTL_SECONDS: i64 = 60 * 60 * 8;

pub struct SessionCookies {
    pub session_cookie: String,
    pub csrf_cookie: String,
}

pub async fn establish_user_session(
    pool: &PgPool,
    user_id: Uuid,
    user_agent: Option<String>,
    ip_address: Option<String>,
) -> Result<SessionCookies, SessionError> {
    let session_token = generate_token();
    let csrf_token = generate_token();
    let session_hash = hash_token(&session_token);
    let expires_at = Utc::now() + Duration::seconds(SESSION_TTL_SECONDS);

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
        session_cookie: build_session_cookie(&session_token, SESSION_TTL_SECONDS),
        csrf_cookie: build_csrf_cookie(&csrf_token, SESSION_TTL_SECONDS),
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
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_session_cookie().parse().unwrap());
    response
        .headers_mut()
        .append(header::SET_COOKIE, clear_csrf_cookie().parse().unwrap());
    Ok(response)
}

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
}
