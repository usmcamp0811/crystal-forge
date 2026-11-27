//! Classifies request paths for the embedded web UI fallback.
//!
//! This module is compiled without the `embedded-ui` feature so that the core
//! server build tests it; [`super::ui`] uses it only when the UI is embedded.

/// Returns whether `path` belongs to a server API or agent endpoint, which the
/// embedded UI fallback must answer with 404 instead of `index.html`.
#[cfg_attr(not(feature = "embedded-ui"), allow(dead_code))]
pub(crate) fn is_api_path(path: &str) -> bool {
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
