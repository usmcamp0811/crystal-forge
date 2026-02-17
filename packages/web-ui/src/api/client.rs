//! HTTP client for the Crystal Forge REST API.
//!
//! Uses `web-sys` fetch under the hood (via `gloo-net`) for WASM compatibility.
//! All methods return deserialized DTOs from [`super::models`].

use super::models::*;

/// Base URL for the API. In production this is the same origin;
/// during development it may point to a different port.
fn base_url() -> String {
    // In production, the UI is served from the same origin as the API.
    // During development with `dx serve`, we proxy or use the server URL directly.
    let window = web_sys::window().expect("no global window");
    let location = window.location();
    let origin = location
        .origin()
        .unwrap_or_else(|_| "http://localhost:3000".into());
    format!("{origin}/api/v1")
}

/// Fetch the dashboard summary.
pub async fn fetch_dashboard() -> Result<DashboardSummary, ApiClientError> {
    let url = format!("{}/dashboard/summary", base_url());
    fetch_json(&url).await
}

/// Fetch a paginated list of systems.
pub async fn fetch_systems(
    params: &SystemsListParams,
) -> Result<PaginatedResponse<SystemSummary>, ApiClientError> {
    let mut url = format!("{}/systems", base_url());
    let query = serde_urlencoded::to_string(params).unwrap_or_default();
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query);
    }
    fetch_json(&url).await
}

/// Fetch a single system's detail.
pub async fn fetch_system(id: &uuid::Uuid) -> Result<SystemDetail, ApiClientError> {
    let url = format!("{}/systems/{}", base_url(), id);
    fetch_json(&url).await
}

/// Errors that can occur when making API requests.
#[derive(Debug, Clone)]
pub enum ApiClientError {
    /// Network or fetch error.
    Network(String),
    /// Server returned a non-2xx status.
    Status { code: u16, body: String },
    /// Failed to deserialize response JSON.
    Deserialize(String),
}

impl std::fmt::Display for ApiClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "Network error: {msg}"),
            Self::Status { code, body } => write!(f, "HTTP {code}: {body}"),
            Self::Deserialize(msg) => write!(f, "Deserialization error: {msg}"),
        }
    }
}

/// Generic JSON fetch helper using web_sys fetch API.
async fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, ApiClientError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().expect("no global window");

    let resp_value = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| ApiClientError::Network("response is not a Response".into()))?;

    let status = resp.status();

    let text = JsFuture::from(
        resp.text()
            .map_err(|e| ApiClientError::Network(format!("{e:?}")))?,
    )
    .await
    .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;

    let body = text.as_string().unwrap_or_default();

    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status as u16,
            body,
        });
    }

    serde_json::from_str(&body).map_err(|e| ApiClientError::Deserialize(e.to_string()))
}
