use axum::{
    Json,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use sqlx::PgPool;
use std::net::{IpAddr, SocketAddr};
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
const MAX_FORWARDED_ADDRESSES: usize = 10;
const SESSION_TTL_ENV: &str = "CRYSTAL_FORGE_SESSION_TTL_SECONDS";
const SESSION_TTL_MIN_SECONDS: i64 = 60;
const SESSION_TTL_MAX_SECONDS: i64 = 60 * 60 * 24 * 30;

pub struct SessionCookies {
    pub session_cookie: HeaderValue,
    pub csrf_cookie: HeaderValue,
}

/// Resolve a session's client address using a bounded, right-to-left proxy
/// chain walk. Forwarded addresses are trusted only when the direct peer is in
/// a configured proxy CIDR; malformed, ambiguous, or overlong headers are
/// treated as unavailable.
pub fn resolve_client_ip(
    peer: SocketAddr,
    headers: &HeaderMap,
    trusted_proxy_cidrs: &[String],
) -> Option<IpAddr> {
    if !ip_in_any_cidr(peer.ip(), trusted_proxy_cidrs) {
        return Some(peer.ip());
    }

    let forwarded = headers.get_all("x-forwarded-for");
    if forwarded.iter().count() != 1 {
        return None;
    }
    let Some(raw) = forwarded.iter().next().and_then(|v| v.to_str().ok()) else {
        return None;
    };
    let mut chain = Vec::with_capacity(MAX_FORWARDED_ADDRESSES + 1);
    for value in raw.split(',').map(str::trim) {
        if value.is_empty() || chain.len() >= MAX_FORWARDED_ADDRESSES {
            return None;
        }
        let Ok(address) = value.parse::<IpAddr>() else {
            return None;
        };
        chain.push(address);
    }
    if chain.is_empty() {
        return None;
    }
    chain.push(peer.ip());

    chain
        .into_iter()
        .rev()
        .find(|address| !ip_in_any_cidr(*address, trusted_proxy_cidrs))
}

fn ip_in_any_cidr(address: IpAddr, cidrs: &[String]) -> bool {
    cidrs.iter().any(|cidr| ip_in_cidr(address, cidr))
}

fn ip_in_cidr(address: IpAddr, cidr: &str) -> bool {
    let Some((network, prefix)) = cidr.split_once('/') else {
        return false;
    };
    let Ok(network) = network.parse::<IpAddr>() else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    match (address, network) {
        (IpAddr::V4(address), IpAddr::V4(network)) if prefix <= 32 => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            u32::from(address) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(address), IpAddr::V6(network)) if prefix <= 128 => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            u128::from(address) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

pub async fn establish_user_session(
    pool: &PgPool,
    user_id: Uuid,
    user_agent: Option<String>,
    ip_address: Option<String>,
    auth_source: &str,
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
        auth_source.to_string(),
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
    // Only validate CSRF if a session cookie exists
    // This makes logout idempotent - clients can call it even when already logged out
    if extract_cookie(&headers, SESSION_COOKIE_NAME).is_some() {
        validate_csrf(&headers)?;

        let session_token = extract_cookie(&headers, SESSION_COOKIE_NAME).unwrap();
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
/// Mutation handlers use this through [`RequireCsrf`] or [`require_csrf`].
pub fn validate_csrf(headers: &HeaderMap) -> Result<(), SessionError> {
    let csrf_cookie =
        extract_cookie(headers, CSRF_COOKIE_NAME).ok_or(SessionError::MissingCsrfCookie)?;

    let csrf_header = headers
        .get(&CSRF_HEADER_NAME)
        .and_then(|v| v.to_str().ok())
        .ok_or(SessionError::MissingCsrfHeader)?;

    if csrf_cookie != csrf_header {
        return Err(SessionError::CsrfMismatch);
    }

    Ok(())
}

/// Requires a matching double-submit CSRF cookie and request header.
///
/// Mutation handlers place this extractor after their authentication extractor
/// so an authenticated session and role are not sufficient to mutate state.
pub struct RequireCsrf;

#[axum::async_trait]
impl<S> FromRequestParts<S> for RequireCsrf
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        require_csrf(&parts.headers)?;
        Ok(Self)
    }
}

/// Validates CSRF headers and returns the structured Web API rejection.
///
/// # Errors
///
/// Returns HTTP 403 with `csrf_validation_failed` when the CSRF cookie or
/// header is absent or when their values do not match.
pub(crate) fn require_csrf(headers: &HeaderMap) -> Result<(), Response> {
    validate_csrf(headers).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "csrf_validation_failed",
                "message": "CSRF validation failed",
                "details": null
            })),
        )
            .into_response()
    })
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
        headers.insert(CSRF_HEADER_NAME.clone(), "csrf123".parse().unwrap());

        assert!(validate_csrf(&headers).is_ok());
    }

    #[test]
    fn csrf_validation_fails_on_mismatch() {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "__Host-cf-csrf=csrf123".parse().unwrap());
        headers.insert(CSRF_HEADER_NAME.clone(), "csrf456".parse().unwrap());

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

    #[test]
    fn trusted_proxy_resolution_requires_trusted_direct_peer() {
        let peer = "192.0.2.10:443".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.20".parse().unwrap());

        assert_eq!(
            resolve_client_ip(peer, &headers, &["192.0.2.0/24".to_string()]),
            Some("198.51.100.20".parse().unwrap())
        );
        assert_eq!(
            resolve_client_ip(peer, &headers, &["198.0.2.11/32".to_string()]),
            Some("192.0.2.10".parse().unwrap())
        );
    }

    #[test]
    fn trusted_proxy_resolution_walks_multiple_trusted_hops() {
        let peer = "10.0.0.2:443".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "198.51.100.20, 10.0.0.3".parse().unwrap(),
        );

        assert_eq!(
            resolve_client_ip(peer, &headers, &["10.0.0.0/8".to_string()]),
            Some("198.51.100.20".parse().unwrap())
        );
    }

    #[test]
    fn trusted_proxy_resolution_rejects_ambiguous_and_trusted_only_headers() {
        let peer = "10.0.0.2:443".parse().unwrap();
        let mut multiple = HeaderMap::new();
        multiple.append("x-forwarded-for", "10.0.0.3".parse().unwrap());
        multiple.append("x-forwarded-for", "10.0.0.4".parse().unwrap());
        assert_eq!(
            resolve_client_ip(peer, &multiple, &["10.0.0.0/8".to_string()]),
            None
        );

        let mut trusted_only = HeaderMap::new();
        trusted_only.insert("x-forwarded-for", "10.0.0.3".parse().unwrap());
        assert_eq!(
            resolve_client_ip(peer, &trusted_only, &["10.0.0.0/8".to_string()]),
            None
        );

        let untrusted_peer = "198.51.100.10:443".parse().unwrap();
        assert_eq!(
            resolve_client_ip(untrusted_peer, &multiple, &["10.0.0.0/8".to_string()]),
            Some("198.51.100.10".parse().unwrap())
        );
        let mut malformed = HeaderMap::new();
        malformed.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
        assert_eq!(
            resolve_client_ip(untrusted_peer, &malformed, &["10.0.0.0/8".to_string()]),
            Some("198.51.100.10".parse().unwrap())
        );
    }
}
