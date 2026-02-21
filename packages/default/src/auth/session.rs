use axum::http::{HeaderMap, header};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

pub const SESSION_COOKIE_NAME: &str = "__Host-cf-session";
pub const CSRF_COOKIE_NAME: &str = "__Host-cf-csrf";
pub const CSRF_HEADER_NAME: &str = "x-csrf-token";

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn build_session_cookie(token: &str, max_age_seconds: i64) -> String {
    format!(
        "{SESSION_COOKIE_NAME}={token}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}"
    )
}

pub fn clear_session_cookie() -> String {
    format!("{SESSION_COOKIE_NAME}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0")
}

pub fn build_csrf_cookie(token: &str, max_age_seconds: i64) -> String {
    format!(
        "{CSRF_COOKIE_NAME}={token}; Path=/; Secure; SameSite=Strict; Max-Age={max_age_seconds}"
    )
}

pub fn clear_csrf_cookie() -> String {
    format!("{CSRF_COOKIE_NAME}=; Path=/; Secure; SameSite=Strict; Max-Age=0")
}

pub fn extract_cookie(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|cookie| {
            cookie
                .strip_prefix(cookie_name)
                .and_then(|rest| rest.strip_prefix('='))
                .map(ToString::to_string)
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
}
