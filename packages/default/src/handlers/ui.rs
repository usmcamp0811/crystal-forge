use axum::extract::OriginalUri;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use include_dir::{Dir, include_dir};
use mime_guess::mime;

static UI_DIST: Dir<'static> = include_dir!("$CRYSTAL_FORGE_UI_DIST");

pub async fn serve_ui(OriginalUri(uri): OriginalUri) -> Response {
    let raw_path = uri.path();

    if is_api_path(raw_path) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let normalized = normalize_path(raw_path);
    if normalized.is_empty() {
        return serve_index();
    }

    if let Some(file) = UI_DIST.get_file(normalized) {
        return serve_asset(normalized, file.contents());
    }

    serve_index()
}

fn serve_index() -> Response {
    if let Some(file) = UI_DIST.get_file("index.html") {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return (headers, file.contents().to_vec()).into_response();
    }

    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

fn serve_asset(path: &str, bytes: &'static [u8]) -> Response {
    let content_type = mime_guess::from_path(path).first_or(mime::APPLICATION_OCTET_STREAM);

    let mut headers = HeaderMap::new();
    if let Ok(header_value) = HeaderValue::from_str(content_type.essence_str()) {
        headers.insert(CONTENT_TYPE, header_value);
    }

    if path.ends_with(".html") {
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    } else {
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }

    (headers, bytes.to_vec()).into_response()
}

fn normalize_path(path: &str) -> &str {
    path.trim_start_matches('/')
}

fn is_api_path(path: &str) -> bool {
    path == "/status"
        || path == "/system_state"
        || path.starts_with("/agent/")
        || path == "/webhook"
        || path.starts_with("/api/")
}

#[cfg(test)]
mod tests {
    use super::is_api_path;

    #[test]
    fn detects_api_paths() {
        assert!(is_api_path("/status"));
        assert!(is_api_path("/system_state"));
        assert!(is_api_path("/agent/heartbeat"));
        assert!(is_api_path("/webhook"));
        assert!(is_api_path("/api/v1/dashboard/summary"));
    }

    #[test]
    fn allows_spa_paths() {
        assert!(!is_api_path("/"));
        assert!(!is_api_path("/systems"));
        assert!(!is_api_path("/assets/main.js"));
    }
}
