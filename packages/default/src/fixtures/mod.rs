/// Fixture mode — serves pre-computed API responses from a JSON file.
///
/// When `FIXTURE_ROUTES_JSON` is set, the server reads a JSON file mapping
/// URL paths → response bodies and intercepts matching GET requests.
///
/// This replaces Playwright route interception with proper server-side
/// fixture data, used by the `ui-screenshots` Nix derivation.
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Pre-computed API route responses keyed by URL path.
#[derive(Clone, Debug, Default)]
pub struct FixtureDb {
    routes: Option<Arc<HashMap<String, Value>>>,
}

impl FixtureDb {
    /// Create a new empty fixture DB (fixtures disabled).
    pub fn empty() -> Self {
        Self { routes: None }
    }

    /// Load fixture routes from a JSON file path → response map.
    pub fn load(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read fixture routes file '{}': {}", path, e))?;
        let map: HashMap<String, Value> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse fixture routes JSON: {}", e))?;
        tracing::info!(
            "Loaded {} fixture routes from {}",
            map.len(),
            path
        );
        Ok(Self {
            routes: Some(Arc::new(map)),
        })
    }

    /// Check if fixtures are enabled.
    pub fn is_enabled(&self) -> bool {
        self.routes.is_some()
    }

    /// Number of loaded fixture routes.
    pub fn route_count(&self) -> usize {
        self.routes.as_ref().map_or(0, |r| r.len())
    }

    /// Look up a fixture response for the given path (without query string).
    pub fn lookup(&self, path: &str) -> Option<Value> {
        let routes = self.routes.as_ref()?;

        // 1. Exact match
        if let Some(body) = routes.get(path) {
            return Some(body.clone());
        }

        // 2. Check with trailing slash (some routes are registered as /api/v1/flakes/)
        let with_slash = format!("{}/", path);
        if let Some(body) = routes.get(&with_slash) {
            return Some(body.clone());
        }

        // 3. Prefix match — find the longest key that is a prefix of path
        let mut best: Option<(usize, &Value)> = None;
        for (key, body) in routes.iter() {
            if key.ends_with('/') && path.starts_with(key.as_str()) {
                let len = key.len();
                match best {
                    Some((best_len, _)) if len > best_len => {
                        best = Some((len, body));
                    }
                    None => {
                        best = Some((len, body));
                    }
                }
            }
        }

        best.map(|(_, body)| body.clone())
    }
}

/// Axum middleware: intercepts GET /api/… requests and returns fixture responses.
pub async fn fixture_middleware(
    State(db): State<FixtureDb>,
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, StatusCode> {
    if !db.is_enabled() {
        // Fixtures disabled — pass through
        return Ok(next.run(request).await);
    }

    let method = request.method().clone();
    let path = request.uri().path().to_string();

    // Only intercept GET requests for /api/ paths
    if method != Method::GET || !path.starts_with("/api/") {
        return Ok(next.run(request).await);
    }

    if let Some(body) = db.lookup(&path) {
        tracing::debug!("Fixture: {} → {} bytes", path, 
            body.to_string().len());
        let json = serde_json::to_vec(&body)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let response = Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(Body::from(json))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(response);
    }

    // No fixture match — pass through to real handler
    Ok(next.run(request).await)
}
