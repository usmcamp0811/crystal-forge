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

/// Base URL for auth endpoints (not under /api/v1).
fn auth_base_url() -> String {
    let window = web_sys::window().expect("no global window");
    let location = window.location();
    let origin = location
        .origin()
        .unwrap_or_else(|_| "http://localhost:3000".into());
    format!("{origin}/api/auth")
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

/// Fetch all flakes from registry.
pub async fn fetch_flakes() -> Result<Vec<FlakeRegistryItem>, ApiClientError> {
    let url = format!("{}/flakes", base_url());
    fetch_json(&url).await
}

/// Create a new flake registry entry.
pub async fn create_flake(
    request: &CreateFlakeRequest,
) -> Result<FlakeRegistryItem, ApiClientError> {
    let url = format!("{}/flakes", base_url());
    send_json("POST", &url, Some(request)).await
}

/// Remove a flake by id.
pub async fn delete_flake(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/flakes/{id}", base_url());
    send_empty("DELETE", &url).await
}

/// Development mode login.
pub async fn dev_login(email: &str) -> Result<DevLoginResponse, ApiClientError> {
    let url = format!("{}/dev/login", auth_base_url());
    let request = DevLoginRequest {
        email: email.to_string(),
    };
    send_json("POST", &url, Some(&request)).await
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
    send_json::<T, ()>("GET", url, None).await
}

async fn send_empty(method: &str, url: &str) -> Result<(), ApiClientError> {
    let (status, body) = send_request(method, url, None).await?;
    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            body: decode_api_error_message(&body),
        });
    }
    Ok(())
}

async fn send_json<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    method: &str,
    url: &str,
    body: Option<&B>,
) -> Result<T, ApiClientError> {
    let payload = match body {
        Some(value) => Some(
            serde_json::to_string(value).map_err(|e| ApiClientError::Deserialize(e.to_string()))?,
        ),
        None => None,
    };

    let (status, text) = send_request(method, url, payload.as_deref()).await?;

    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            body: decode_api_error_message(&text),
        });
    }

    serde_json::from_str(&text).map_err(|e| ApiClientError::Deserialize(e.to_string()))
}

async fn send_request(
    method: &str,
    url: &str,
    body: Option<&str>,
) -> Result<(u16, String), ApiClientError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().expect("no global window");

    let mut opts = web_sys::RequestInit::new();
    opts.set_method(method);
    if let Some(payload) = body {
        opts.set_body(&JsValue::from_str(payload));
    }

    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    if body.is_some() {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    }

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
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

    Ok((status as u16, body))
}

fn decode_api_error_message(body: &str) -> String {
    serde_json::from_str::<ApiError>(body)
        .map(|error| error.message)
        .unwrap_or_else(|_| body.to_string())
}
