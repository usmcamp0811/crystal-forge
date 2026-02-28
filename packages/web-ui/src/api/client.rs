//! HTTP client for the Crystal Forge REST API.
//!
//! Uses `web-sys` fetch under the hood (via `gloo-net`) for WASM compatibility.
//! All methods return deserialized DTOs from [`super::models`].

use super::models::*;

fn backend_origin_for_dev(window: &web_sys::Window, origin: &str) -> Option<String> {
    if !(origin.contains(":8080") || origin.contains(":8000") || origin.contains(":8081")) {
        return None;
    }

    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(custom_origin)) = storage.get_item("cf_backend_origin") {
            let trimmed = custom_origin.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let host = window
        .location()
        .hostname()
        .unwrap_or_else(|_| "localhost".to_string());
    Some(format!("http://{host}:3445"))
}

/// Base URL for the API. In production this is the same origin;
/// during development it may point to a different port.
fn base_url() -> String {
    let window = web_sys::window().expect("no global window");
    let location = window.location();
    let origin = location
        .origin()
        .unwrap_or_else(|_| "http://localhost:3445".into());

    if let Some(dev_origin) = backend_origin_for_dev(&window, &origin) {
        return format!("{dev_origin}/api/v1");
    }

    format!("{origin}/api/v1")
}

/// Base URL for auth endpoints (not under /api/v1).
fn auth_base_url() -> String {
    let window = web_sys::window().expect("no global window");
    let location = window.location();
    let origin = location
        .origin()
        .unwrap_or_else(|_| "http://localhost:3445".into());

    if let Some(dev_origin) = backend_origin_for_dev(&window, &origin) {
        return format!("{dev_origin}/api/auth");
    }

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

/// Create a new system.
pub async fn create_system(request: &CreateSystemRequest) -> Result<SystemDetail, ApiClientError> {
    let url = format!("{}/systems", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Update a system's public key.
pub async fn update_system_public_key(
    id: &uuid::Uuid,
    request: &UpdateSystemPublicKeyRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/public-key", base_url(), id);
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Disable (soft-delete) a system.
pub async fn deactivate_system(id: &uuid::Uuid) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/deactivate", base_url(), id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn request_system_sync(
    id: &uuid::Uuid,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/sync", base_url(), id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn request_system_rollback(
    id: &uuid::Uuid,
    request: &SystemRollbackRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/rollback", base_url(), id);
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Fetch the list of environments visible to the authenticated user.
pub async fn fetch_environments() -> Result<Vec<EnvironmentSummary>, ApiClientError> {
    let url = format!("{}/environments", base_url());
    fetch_json(&url).await
}

/// Fetch a single environment by ID.
pub async fn fetch_environment(id: &uuid::Uuid) -> Result<EnvironmentSummary, ApiClientError> {
    let url = format!("{}/environments/{}", base_url(), id);
    fetch_json(&url).await
}

/// Fetch a single environment with required policies.
pub async fn fetch_environment_policies(
    id: &uuid::Uuid,
) -> Result<EnvironmentWithPolicies, ApiClientError> {
    let url = format!("{}/environments/{}/policies", base_url(), id);
    fetch_json(&url).await
}

/// Fetch required policy assignments for visible environments.
pub async fn fetch_environment_policies_map(
) -> Result<Vec<EnvironmentPolicyMapEntry>, ApiClientError> {
    let url = format!("{}/environments/policies-map", base_url());
    fetch_json(&url).await
}

/// Create an environment.
pub async fn create_environment(
    request: &CreateEnvironmentRequest,
) -> Result<EnvironmentSummary, ApiClientError> {
    let url = format!("{}/environments", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Delete an environment by id.
pub async fn delete_environment(id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/environments/{}", base_url(), id);
    send_empty("DELETE", &url).await
}

/// Update an environment by id.
pub async fn update_environment(
    id: &uuid::Uuid,
    request: &UpdateEnvironmentRequest,
) -> Result<EnvironmentSummary, ApiClientError> {
    let url = format!("{}/environments/{}", base_url(), id);
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

/// Update environment required policies.
pub async fn update_environment_policies(
    id: &uuid::Uuid,
    required_policy_ids: &[uuid::Uuid],
) -> Result<(), ApiClientError> {
    use super::models::UpdateEnvironmentPoliciesRequest as Req;
    let url = format!("{}/environments/{}/policies", base_url(), id);
    let req = Req {
        required_policy_ids: required_policy_ids.to_vec(),
    };
    let _: serde_json::Value = send_json_with_csrf("PATCH", &url, Some(&req)).await?;
    Ok(())
}

/// Fetch available deployment policies.
pub async fn fetch_policies() -> Result<Vec<DeploymentPolicySummary>, ApiClientError> {
    let url = format!("{}/policies", base_url());
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

/// Update an existing flake registry entry.
pub async fn update_flake(
    id: i32,
    request: &UpdateFlakeRequest,
) -> Result<FlakeRegistryItem, ApiClientError> {
    let url = format!("{}/flakes/{id}", base_url());
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

/// Remove a flake by id.
pub async fn delete_flake(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/flakes/{id}", base_url());
    send_empty("DELETE", &url).await
}

/// Fetch flake timelines with recent commits for dashboard.
pub async fn fetch_flake_timelines() -> Result<Vec<FlakeTimeline>, ApiClientError> {
    let url = format!("{}/flakes/timelines", base_url());
    fetch_json(&url).await
}

/// Trigger sync for all flakes.
pub async fn request_sync_all_flakes() -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/flakes/sync", base_url());
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Trigger sync for a specific flake.
pub async fn request_sync_flake(id: i32) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/flakes/{id}/sync", base_url());
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Fetch the git diff for a specific commit in a flake.
pub async fn fetch_commit_diff(
    flake_id: i32,
    commit_hash: &str,
) -> Result<CommitDiffResponse, ApiClientError> {
    let url = format!(
        "{}/flakes/{}/commits/{}/diff",
        base_url(),
        flake_id,
        commit_hash
    );
    fetch_json(&url).await
}

/// Fetch current authentication context.
pub async fn fetch_whoami() -> Result<AuthContext, ApiClientError> {
    let url = format!("{}/whoami", auth_base_url());
    fetch_json(&url).await
}

/// Development mode login.
pub async fn dev_login(email: &str) -> Result<DevLoginResponse, ApiClientError> {
    let url = format!("{}/dev/login", auth_base_url());
    let request = DevLoginRequest {
        email: email.to_string(),
    };
    send_json("POST", &url, Some(&request)).await
}

/// Local username/password login.
pub async fn local_login(
    username: &str,
    password: &str,
) -> Result<LocalLoginResponse, ApiClientError> {
    let url = format!("{}/local/login", auth_base_url());
    let request = LocalLoginRequest {
        username: username.to_string(),
        password: password.to_string(),
    };
    send_json("POST", &url, Some(&request)).await
}

/// Logout (invalidates current session).
pub async fn logout() -> Result<(), ApiClientError> {
    let url = format!("{}/logout", auth_base_url());

    // We need to get the CSRF token from cookie and send it in header
    // For now, just send the request - the CSRF validation will happen server-side
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Fetch admin users view data.
pub async fn fetch_admin_users() -> Result<Vec<AdminUserSummary>, ApiClientError> {
    let url = format!("{}/admin/users", base_url());
    fetch_json(&url).await
}

/// Fetch admin audit events.
pub async fn fetch_admin_audit_events(
    params: &AdminAuditEventsParams,
) -> Result<PaginatedResponse<AuditEvent>, ApiClientError> {
    let mut url = format!("{}/admin/audit-events", base_url());
    let query = serde_urlencoded::to_string(params).unwrap_or_default();
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query);
    }
    fetch_json(&url).await
}

/// Create a local user from the admin console.
pub async fn create_admin_user(
    request: &AdminCreateUserRequest,
) -> Result<AdminUserSummary, ApiClientError> {
    let url = format!("{}/admin/users", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Update a local user from the admin console.
pub async fn update_admin_user(
    user_id: &str,
    request: &AdminUpdateUserRequest,
) -> Result<AdminUserSummary, ApiClientError> {
    let url = format!("{}/admin/users/{user_id}", base_url());
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

/// Delete a local user from the admin console.
pub async fn delete_admin_user(user_id: &str) -> Result<(), ApiClientError> {
    let url = format!("{}/admin/users/{user_id}", base_url());
    let _deleted: serde_json::Value = send_json_with_csrf("DELETE", &url, None::<&()>).await?;
    Ok(())
}

/// Fetch OIDC group mappings managed by admins.
pub async fn fetch_admin_oidc_mappings() -> Result<Vec<OidcGroupMapping>, ApiClientError> {
    let url = format!("{}/admin/oidc-mappings", base_url());
    fetch_json(&url).await
}

/// Create or update an OIDC group mapping.
pub async fn upsert_admin_oidc_mapping(
    request: &AdminUpsertOidcMappingRequest,
) -> Result<OidcGroupMapping, ApiClientError> {
    let url = format!("{}/admin/oidc-mappings", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Delete an OIDC group mapping by id.
pub async fn delete_admin_oidc_mapping(mapping_id: &str) -> Result<(), ApiClientError> {
    let url = format!("{}/admin/oidc-mappings/{mapping_id}", base_url());
    let _deleted: serde_json::Value = send_json_with_csrf("DELETE", &url, None::<&()>).await?;
    Ok(())
}

/// Send JSON request with CSRF token from cookie.
async fn send_json_with_csrf<T: serde::de::DeserializeOwned, B: serde::Serialize>(
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

    let (status, text) = send_request_with_csrf(method, url, payload.as_deref()).await?;

    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            body: decode_api_error_message(&text),
        });
    }

    // For 204 No Content, return default value if possible
    if status == 204 {
        return serde_json::from_str("null")
            .map_err(|e| ApiClientError::Deserialize(e.to_string()));
    }

    serde_json::from_str(&text).map_err(|e| ApiClientError::Deserialize(e.to_string()))
}

async fn send_request_with_csrf(
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
    let _ = js_sys::Reflect::set(
        opts.as_ref(),
        &JsValue::from_str("credentials"),
        &JsValue::from_str("include"),
    );
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

    // Extract CSRF token from cookie and add to header
    if let Some(document) = window.document() {
        // Use js_sys to call document.cookie
        let document_obj = js_sys::Object::from(
            js_sys::Reflect::get(&document, &JsValue::from_str("document"))
                .unwrap_or(JsValue::NULL),
        );

        // Simpler: just get the cookie string directly from the global document
        let cookie_js = js_sys::eval("document.cookie").unwrap_or(JsValue::NULL);
        if let Some(cookie_str) = cookie_js.as_string() {
            for cookie in cookie_str.split(';') {
                let cookie = cookie.trim();
                if let Some(value) = cookie.strip_prefix("__Host-cf-csrf=") {
                    request
                        .headers()
                        .set("X-CSRF-Token", value)
                        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
                    break;
                }
            }
        }
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
    let _ = js_sys::Reflect::set(
        opts.as_ref(),
        &JsValue::from_str("credentials"),
        &JsValue::from_str("include"),
    );
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
