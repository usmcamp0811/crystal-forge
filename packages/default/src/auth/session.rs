use axum::http::{HeaderMap, HeaderName, HeaderValue, header};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

pub const SESSION_COOKIE_NAME: &str = "__Host-cf-session";
pub const CSRF_COOKIE_NAME: &str = "__Host-cf-csrf";

// Use HeaderName for type safety and case-insensitive comparison
pub static CSRF_HEADER_NAME: HeaderName = HeaderName::from_static("x-csrf-token");

const SESSION_COOKIE_ATTRIBUTES: &str = "Path=/; Secure; HttpOnly; SameSite=Lax";
const CSRF_COOKIE_ATTRIBUTES: &str = "Path=/; Secure; SameSite=Strict";
const EXPIRES_CLEAR_ATTR: &str = "Expires=Thu, 01 Jan 1970 00:00:00 GMT";
const SESSION_TOKEN_BYTES: usize = 32;

pub fn generate_token() -> String {
    let mut bytes = [0u8; SESSION_TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn build_session_cookie(token: &str, max_age_seconds: i64) -> String {
    format!("{SESSION_COOKIE_NAME}={token}; {SESSION_COOKIE_ATTRIBUTES}; Max-Age={max_age_seconds}")
}

pub fn clear_session_cookie() -> String {
    // SECURITY: Clearing a cookie must use the same path/samesite/security attributes
    // as the cookie being set, otherwise some browsers may retain the cookie.
    format!("{SESSION_COOKIE_NAME}=; {SESSION_COOKIE_ATTRIBUTES}; Max-Age=0; {EXPIRES_CLEAR_ATTR}")
}

pub fn build_csrf_cookie(token: &str, max_age_seconds: i64) -> String {
    format!("{CSRF_COOKIE_NAME}={token}; {CSRF_COOKIE_ATTRIBUTES}; Max-Age={max_age_seconds}")
}

pub fn clear_csrf_cookie() -> String {
    // SECURITY: Clearing a cookie must use the same path/samesite/security attributes
    // as the cookie being set, otherwise some browsers may retain the cookie.
    format!("{CSRF_COOKIE_NAME}=; {CSRF_COOKIE_ATTRIBUTES}; Max-Age=0; {EXPIRES_CLEAR_ATTR}")
}

pub fn parse_set_cookie_header(
    cookie: &str,
) -> Result<HeaderValue, axum::http::header::InvalidHeaderValue> {
    HeaderValue::from_str(cookie)
}

pub fn extract_cookie(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|cookie| {
            let (name, value) = cookie.split_once('=')?;
            if name.trim() != cookie_name {
                return None;
            }

            Some(value.trim().trim_matches('"').to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_is_stable() {
        let hash1 = hash_token("abc");
        let hash2 = hash_token("abc");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn extract_cookie_reads_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "foo=bar; __Host-cf-session=session123; baz=qux"
                .parse()
                .unwrap(),
        );

        let value = extract_cookie(&headers, SESSION_COOKIE_NAME);
        assert_eq!(value.as_deref(), Some("session123"));
    }

    #[test]
    fn session_cookie_contains_host_invariants() {
        let cookie = build_session_cookie("session123", 3600);

        assert!(cookie.contains("__Host-cf-session=session123"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Secure"));
        assert!(cookie.contains("HttpOnly"));
    }

    #[test]
    fn csrf_cookie_is_secure_and_not_httponly() {
        let cookie = build_csrf_cookie("csrf123", 3600);

        assert!(cookie.contains("__Host-cf-csrf=csrf123"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Secure"));
        assert!(!cookie.contains("HttpOnly"));
    }

    #[test]
    fn clear_cookie_includes_expires_for_compatibility() {
        let cookie = clear_session_cookie();
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Expires=Thu, 01 Jan 1970 00:00:00 GMT"));
    }

    #[test]
    fn extract_cookie_handles_quoted_value_and_spacing() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "foo=bar; __Host-cf-csrf=\"quoted-value\" ; baz=qux"
                .parse()
                .unwrap(),
        );

        let value = extract_cookie(&headers, CSRF_COOKIE_NAME);
        assert_eq!(value.as_deref(), Some("quoted-value"));
    }

    #[test]
    fn extract_cookie_does_not_match_similar_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            "__Host-cf-session-extra=bad; __Host-cf-session=good"
                .parse()
                .unwrap(),
        );

        let value = extract_cookie(&headers, SESSION_COOKIE_NAME);
        assert_eq!(value.as_deref(), Some("good"));
    }
}
