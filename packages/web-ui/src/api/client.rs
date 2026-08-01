//! HTTP client for the Crystal Forge REST API.
//!
//! Uses `web-sys` fetch under the hood (via `gloo-net`) for WASM compatibility.
//! All methods return deserialized DTOs from [`super::models`].

use super::models::*;
use chrono::{DateTime, Utc};
use uuid::Uuid;

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
pub fn base_url() -> String {
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

/// Fetch admin-only CVE dashboard summary.
pub async fn fetch_cve_dashboard_summary() -> Result<CveDashboardSummary, ApiClientError> {
    let url = format!("{}/cves/summary", base_url());
    fetch_json(&url).await
}

/// Fetch admin-only top-affected systems for CVE dashboard visualization.
pub async fn fetch_cve_top_systems() -> Result<Vec<CveDashboardTopSystem>, ApiClientError> {
    let url = format!("{}/cves/top-systems", base_url());
    fetch_json(&url).await
}

/// Fetch admin-only CVE scan freshness/coverage per system.
pub async fn fetch_cve_scan_freshness() -> Result<Vec<CveScanFreshnessRow>, ApiClientError> {
    let url = format!("{}/cves/scan-freshness", base_url());
    fetch_json(&url).await
}

pub async fn fetch_scanning_stats() -> Result<ScanningStatsResponse, ApiClientError> {
    let url = format!("{}/scanning/stats", base_url());
    fetch_json(&url).await
}

pub async fn fetch_scanning_queue(
    limit: Option<i64>,
) -> Result<Vec<ScanningQueueItemResponse>, ApiClientError> {
    let mut url = format!("{}/scanning/queue", base_url());
    if let Some(limit) = limit {
        url.push_str(&format!("?limit={}", limit.clamp(1, 500)));
    }
    fetch_json(&url).await
}

pub async fn fetch_scanning_systems(
    limit: Option<i64>,
) -> Result<Vec<ScanningSystemsItemResponse>, ApiClientError> {
    let mut url = format!("{}/scanning/systems", base_url());
    if let Some(limit) = limit {
        url.push_str(&format!("?limit={}", limit.clamp(1, 500)));
    }
    fetch_json(&url).await
}

pub async fn fetch_scanning_deployed(
    limit: Option<i64>,
    after_cursor: Option<&str>,
) -> Result<ScanningDeployedResponse, ApiClientError> {
    let mut url = format!("{}/scanning/deployed", base_url());
    let limit_val = limit.unwrap_or(500).clamp(1, 1000);
    url.push_str(&format!("?limit={}", limit_val));
    if let Some(cursor) = after_cursor {
        url.push_str(&format!("&after={}", js_sys::encode_uri_component(cursor)));
    }
    fetch_json(&url).await
}

/// Export exact policy version IDs as canonical JSON or TOML.
pub async fn export_policy_versions(
    policy_version_ids: &[Uuid],
    format: &str,
) -> Result<String, ApiClientError> {
    let url = format!("{}/policies/interchange/export", base_url());
    let payload = serde_json::json!({
        "policy_version_ids": policy_version_ids,
        "format": format,
    });
    let (status, body) = send_request_with_csrf(
        "POST",
        &url,
        Some(
            &serde_json::to_string(&payload)
                .map_err(|error| ApiClientError::Deserialize(error.to_string()))?,
        ),
    )
    .await?;
    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            body: decode_api_error_message(&body),
        });
    }
    Ok(body)
}

pub async fn preview_policy_interchange(
    bytes: &[u8],
    filename: &str,
) -> Result<PolicyInterchangePreviewResponse, ApiClientError> {
    let url = format!("{}/policies/interchange/preview", base_url());
    let (status, body) = send_policy_multipart(&url, bytes, filename, None).await?;
    parse_policy_interchange_response(status, &body)
}

pub async fn import_policy_interchange(
    bytes: &[u8],
    filename: &str,
    expected_sha256: &str,
) -> Result<PolicyInterchangeImportResponse, ApiClientError> {
    let url = format!("{}/policies/interchange/import", base_url());
    let (status, body) =
        send_policy_multipart(&url, bytes, filename, Some(expected_sha256)).await?;
    parse_policy_interchange_response(status, &body)
}

async fn send_policy_multipart(
    url: &str,
    bytes: &[u8],
    filename: &str,
    expected_sha256: Option<&str>,
) -> Result<(u16, String), ApiClientError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let form =
        web_sys::FormData::new().map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
    form.append_with_blob_and_filename("file", &blob, filename)
        .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;

    let window =
        web_sys::window().ok_or_else(|| ApiClientError::Network("no global window".into()))?;
    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(form.as_ref());
    let _ = js_sys::Reflect::set(
        opts.as_ref(),
        &JsValue::from_str("credentials"),
        &JsValue::from_str("include"),
    );
    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
    if let Some(expected) = expected_sha256 {
        request
            .headers()
            .set("X-Policy-Source-SHA256", expected)
            .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
    }
    add_csrf_header(&request, &window)?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
    let response: web_sys::Response = response
        .dyn_into()
        .map_err(|_| ApiClientError::Network("response is not a Response".into()))?;
    let status = response.status() as u16;
    let text = JsFuture::from(
        response
            .text()
            .map_err(|error| ApiClientError::Network(format!("{error:?}")))?,
    )
    .await
    .map_err(|error| ApiClientError::Network(format!("{error:?}")))?
    .as_string()
    .unwrap_or_default();
    Ok((status, text))
}

fn add_csrf_header(
    request: &web_sys::Request,
    window: &web_sys::Window,
) -> Result<(), ApiClientError> {
    let cookie_js = js_sys::eval("document.cookie").unwrap_or(wasm_bindgen::JsValue::NULL);
    if let Some(cookie_str) = cookie_js.as_string() {
        for cookie in cookie_str.split(';').map(str::trim) {
            if let Some(value) = cookie.strip_prefix("__Host-cf-csrf=") {
                request
                    .headers()
                    .set("X-CSRF-Token", value)
                    .map_err(|error| ApiClientError::Network(format!("{error:?}")))?;
                break;
            }
        }
    }
    let _ = window;
    Ok(())
}

fn parse_policy_interchange_response<T: serde::de::DeserializeOwned>(
    status: u16,
    body: &str,
) -> Result<T, ApiClientError> {
    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            // Policy interchange has a small endpoint-specific error contract
            // (including conflict details). Preserve it for the UI adapter.
            body: body.to_string(),
        });
    }
    serde_json::from_str(body).map_err(|error| ApiClientError::Deserialize(error.to_string()))
}

pub async fn fetch_scanning_system_scans(
    system_id: &Uuid,
    limit: Option<i64>,
) -> Result<Vec<ScanningQueueItemResponse>, ApiClientError> {
    let mut url = format!("{}/scanning/systems/{}/scans", base_url(), system_id);
    if let Some(limit) = limit {
        url.push_str(&format!("?limit={}", limit.clamp(1, 500)));
    }
    fetch_json(&url).await
}

pub async fn fetch_scanning_activity(
    limit: Option<i64>,
) -> Result<Vec<ScanningActivityItemResponse>, ApiClientError> {
    let mut url = format!("{}/scanning/activity", base_url());
    if let Some(limit) = limit {
        url.push_str(&format!("?limit={}", limit.clamp(1, 500)));
    }
    fetch_json(&url).await
}

pub async fn fetch_scanning_schedule() -> Result<ScanSchedulePolicyResponse, ApiClientError> {
    let url = format!("{}/scanning/schedule", base_url());
    fetch_json(&url).await
}

pub async fn update_scanning_schedule(
    req: &UpdateScanSchedulePolicyRequest,
) -> Result<ScanSchedulePolicyResponse, ApiClientError> {
    let url = format!("{}/scanning/schedule", base_url());
    send_json_with_csrf("PUT", &url, Some(req)).await
}

pub async fn fetch_hardening_fleet_summary() -> Result<HardeningFleetSummaryResponse, ApiClientError>
{
    let url = format!("{}/hardening/summary", base_url());
    fetch_json(&url).await
}

pub async fn fetch_hardening_top_services(
    limit: Option<i64>,
) -> Result<Vec<HardeningTopServiceResponse>, ApiClientError> {
    let mut url = format!("{}/hardening/top-services", base_url());
    if let Some(limit) = limit {
        url.push_str(&format!("?limit={}", limit.clamp(1, 50)));
    }
    fetch_json(&url).await
}

pub async fn fetch_hardening_system_postures()
-> Result<Vec<HardeningSystemPostureResponse>, ApiClientError> {
    let url = format!("{}/hardening/systems", base_url());
    fetch_json(&url).await
}

/// Fetch admin-only CVE dashboard drill-down vulnerabilities with filters.
pub async fn fetch_cve_dashboard_vulnerabilities(
    params: &CveDashboardVulnerabilityParams,
) -> Result<Vec<CveDashboardVulnerability>, ApiClientError> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(severity) = &params.severity {
        if !severity.is_empty() {
            parts.push(format!("severity={}", encode_query_value(severity)));
        }
    }

    if let Some(status) = &params.status {
        if !status.is_empty() {
            parts.push(format!("status={}", encode_query_value(status)));
        }
    }

    if let Some(limit) = params.limit {
        parts.push(format!("limit={limit}"));
    }

    if let Some(system) = &params.system {
        if !system.is_empty() {
            parts.push(format!("system={}", encode_query_value(system)));
        }
    }

    if let Some(environment) = &params.environment {
        if !environment.is_empty() {
            parts.push(format!("environment={}", encode_query_value(environment)));
        }
    }

    if let Some(package) = &params.package {
        if !package.is_empty() {
            parts.push(format!("package={}", encode_query_value(package)));
        }
    }

    if let Some(date_from) = &params.date_from {
        if !date_from.is_empty() {
            parts.push(format!("date_from={}", encode_query_value(date_from)));
        }
    }

    if let Some(date_to) = &params.date_to {
        if !date_to.is_empty() {
            parts.push(format!("date_to={}", encode_query_value(date_to)));
        }
    }

    let mut url = format!("{}/cves/vulnerabilities", base_url());
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }

    fetch_json(&url).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Advanced CVE Dashboard Client Functions (TASK-322)
// ─────────────────────────────────────────────────────────────────────────────

fn cve_filter_query_parts(filters: &CveFilters) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(severity) = &filters.severity {
        if !severity.is_empty() {
            parts.push(format!("severity={}", encode_query_value(severity)));
        }
    }

    if let Some(fix_status) = &filters.fix_status {
        if !fix_status.is_empty() {
            parts.push(format!("fix_status={}", encode_query_value(fix_status)));
        }
    }

    if let Some(triage_status) = &filters.triage_status {
        if !triage_status.is_empty() {
            parts.push(format!(
                "triage_status={}",
                encode_query_value(triage_status)
            ));
        }
    }

    if let Some(package) = &filters.package {
        if !package.is_empty() {
            parts.push(format!("package={}", encode_query_value(package)));
        }
    }

    if let Some(search) = &filters.search {
        if !search.is_empty() {
            parts.push(format!("search={}", encode_query_value(search)));
        }
    }

    if let Some(sort) = &filters.sort {
        if !sort.is_empty() {
            parts.push(format!("sort={}", encode_query_value(sort)));
        }
    }

    if let Some(limit) = filters.limit {
        parts.push(format!("limit={limit}"));
    }

    parts
}

/// Fetch CVE list with filters.
pub async fn fetch_cves(filters: &CveFilters) -> Result<Vec<CveListItem>, ApiClientError> {
    let parts = cve_filter_query_parts(filters);

    let mut url = format!("{}/cves", base_url());
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }

    fetch_json(&url).await
}

/// Fetch CVEs grouped by package.
pub async fn fetch_cves_grouped(
    filters: &CveFilters,
) -> Result<Vec<CvePackageGroup>, ApiClientError> {
    let parts = cve_filter_query_parts(filters);

    let mut url = format!("{}/cves/grouped", base_url());
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }

    fetch_json(&url).await
}

/// Fetch fleet-wide CVE statistics.
pub async fn fetch_cve_fleet_stats() -> Result<CveFleetStats, ApiClientError> {
    let url = format!("{}/cves/stats", base_url());
    fetch_json(&url).await
}

/// Fetch package names for autocomplete.
pub async fn fetch_cve_package_names() -> Result<Vec<String>, ApiClientError> {
    let url = format!("{}/cves/packages", base_url());
    fetch_json(&url).await
}

/// Fetch detailed information for a single CVE.
pub async fn fetch_cve_detail(cve_id: &str) -> Result<CveDetail, ApiClientError> {
    let url = format!("{}/cves/{}", base_url(), encode_uri_component(cve_id));
    fetch_json(&url).await
}

/// Fetch systems affected by a CVE.
pub async fn fetch_cve_systems(
    cve_id: &str,
) -> Result<Vec<CveAffectedSystemDetail>, ApiClientError> {
    let url = format!(
        "{}/cves/{}/systems",
        base_url(),
        encode_uri_component(cve_id)
    );
    fetch_json(&url).await
}

/// Fetch justification history for a CVE.
pub async fn fetch_cve_justifications(
    cve_id: &str,
) -> Result<Vec<CveJustification>, ApiClientError> {
    let url = format!(
        "{}/cves/{}/justifications",
        base_url(),
        encode_uri_component(cve_id)
    );
    fetch_json(&url).await
}

/// Save a CVE justification.
pub async fn save_cve_justification(
    cve_id: &str,
    input: &CveJustificationInput,
) -> Result<(), ApiClientError> {
    let url = format!(
        "{}/cves/{}/justification",
        base_url(),
        encode_uri_component(cve_id)
    );
    send_empty_with_csrf("POST", &url, Some(input)).await
}

/// Revoke the fleet-wide justification for a CVE (DELETE).
///
/// Idempotent: the server returns 204 whether or not a justification existed.
pub async fn revoke_cve_justification(cve_id: &str) -> Result<(), ApiClientError> {
    let url = format!(
        "{}/cves/{}/justification",
        base_url(),
        encode_uri_component(cve_id)
    );
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}

/// Trigger CVE scan for all active systems (fleet rescan).
pub async fn trigger_cve_fleet_rescan() -> Result<FleetRescanResponse, ApiClientError> {
    let url = format!("{}/cves/rescan-fleet", base_url());
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Export CVEs as CSV (triggers browser download).
pub async fn export_cves_csv(filters: &CveFilters) -> Result<(), ApiClientError> {
    let parts = cve_filter_query_parts(filters);

    let mut url = format!("{}/cves/export", base_url());
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }

    // Trigger browser download by setting window.location
    let window = web_sys::window().expect("no window");
    window
        .location()
        .set_href(&url)
        .map_err(|_| ApiClientError::Network("Failed to trigger download".to_string()))?;

    Ok(())
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

/// Fetch CVE vulnerabilities for a single system.
pub async fn fetch_system_cves(
    id: &uuid::Uuid,
) -> Result<Vec<SystemVulnerability>, ApiClientError> {
    let url = format!("{}/systems/{}/cves", base_url(), id);
    fetch_json(&url).await
}

pub async fn fetch_system_hardening(
    id: &uuid::Uuid,
) -> Result<Vec<HardeningServiceResultResponse>, ApiClientError> {
    let url = format!("{}/systems/{}/hardening", base_url(), id);
    fetch_json(&url).await
}

pub async fn fetch_system_hardening_justifications(
    id: &uuid::Uuid,
) -> Result<Vec<HardeningJustificationResponse>, ApiClientError> {
    let url = format!("{}/systems/{}/hardening/justifications", base_url(), id);
    fetch_json(&url).await
}

pub async fn save_system_hardening_justification(
    id: &uuid::Uuid,
    service_name: &str,
    request: &SaveHardeningJustificationRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let encoded_service: String = js_sys::encode_uri_component(service_name).into();
    let url = format!(
        "{}/systems/{}/hardening/{}/justification",
        base_url(),
        id,
        encoded_service
    );
    send_json_with_csrf("PUT", &url, Some(request)).await
}

pub async fn fetch_system_hardening_scan_eligibility(
    id: &uuid::Uuid,
) -> Result<HardeningScanEligibilityResponse, ApiClientError> {
    let url = format!("{}/systems/{}/hardening-scan-eligibility", base_url(), id);
    fetch_json(&url).await
}

pub async fn trigger_system_hardening_scan(
    id: &uuid::Uuid,
) -> Result<HardeningScanTriggerResponse, ApiClientError> {
    let url = format!("{}/systems/{}/hardening-scan", base_url(), id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn fetch_hardening_scan_status(
    scan_id: &uuid::Uuid,
) -> Result<HardeningScanStatusResponse, ApiClientError> {
    let url = format!("{}/hardening-scans/{}", base_url(), scan_id);
    fetch_json(&url).await
}

pub async fn fetch_system_cve_scan_eligibility(
    id: &uuid::Uuid,
) -> Result<CveScanEligibilityResponse, ApiClientError> {
    let url = format!("{}/systems/{}/cve-scan-eligibility", base_url(), id);
    fetch_json(&url).await
}

pub async fn trigger_system_cve_scan(
    id: &uuid::Uuid,
) -> Result<CveScanTriggerResponse, ApiClientError> {
    let url = format!("{}/systems/{}/cve-scan", base_url(), id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn save_system_cve_justification(
    id: &uuid::Uuid,
    cve_id: &str,
    request: &SaveSystemCveJustificationRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let encoded_cve: String = js_sys::encode_uri_component(cve_id).into();
    let url = format!(
        "{}/systems/{}/cves/{}/justification",
        base_url(),
        id,
        encoded_cve
    );
    send_json_with_csrf("PUT", &url, Some(request)).await
}

pub async fn trigger_flake_config_cve_scan(
    flake_id: i32,
    config_name: &str,
) -> Result<CveScanTriggerResponse, ApiClientError> {
    let encoded_config: String = js_sys::encode_uri_component(config_name).into();
    let url = format!(
        "{}/flakes/{}/configs/{}/cve-scan",
        base_url(),
        flake_id,
        encoded_config
    );
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn fetch_cve_scan_status(
    scan_id: &uuid::Uuid,
) -> Result<CveScanStatusResponse, ApiClientError> {
    let url = format!("{}/cve-scans/{}", base_url(), scan_id);
    fetch_json(&url).await
}

/// Create a new system.
pub async fn create_system(request: &CreateSystemRequest) -> Result<SystemDetail, ApiClientError> {
    let url = format!("{}/systems", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn update_system(
    id: &uuid::Uuid,
    request: &UpdateSystemRequest,
) -> Result<SystemDetail, ApiClientError> {
    let url = format!("{}/systems/{}", base_url(), id);
    send_json_with_csrf("PATCH", &url, Some(request)).await
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

pub async fn deploy_system(
    id: &uuid::Uuid,
    request: &crate::api::models::DeploySystemRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/deploy", base_url(), id);
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn fetch_system_commits(
    id: &uuid::Uuid,
) -> Result<crate::api::models::SystemCommitsResponse, ApiClientError> {
    let url = format!("{}/systems/{}/commits", base_url(), id);
    fetch_json(&url).await
}

pub async fn fetch_system_generations(
    id: &uuid::Uuid,
) -> Result<crate::api::models::SystemGenerationsResponse, ApiClientError> {
    let url = format!("{}/systems/{}/generations", base_url(), id);
    fetch_json(&url).await
}

pub async fn fetch_system_history(
    id: &uuid::Uuid,
) -> Result<Vec<crate::api::models::SystemHistoryEntry>, ApiClientError> {
    let url = format!("{}/systems/{}/history", base_url(), id);
    fetch_json(&url).await
}

pub async fn get_system_deployment_progress(
    id: &uuid::Uuid,
) -> Result<Option<crate::api::models::SystemDeploymentProgress>, ApiClientError> {
    let url = format!("{}/systems/{}/deployment-status", base_url(), id);
    let (status, body) = send_request("GET", &url, None).await?;
    if status == 204 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            body: decode_api_error_message(&body),
        });
    }

    serde_json::from_str(&body)
        .map(Some)
        .map_err(|error| ApiClientError::Deserialize(error.to_string()))
}

pub async fn fetch_system_agent_events(
    id: &uuid::Uuid,
) -> Result<Vec<crate::api::models::SystemAgentEvent>, ApiClientError> {
    let url = format!("{}/systems/{}/agent-events", base_url(), id);
    fetch_json(&url).await
}

/// Fetch the evaluation queue (active + completed commits).
pub async fn fetch_eval_queue(
    limit: i64,
    search: Option<&str>,
    latest_only: bool,
) -> Result<EvalQueueSummary, ApiClientError> {
    let mut params = vec![
        format!("limit={limit}"),
        format!("latest_only={latest_only}"),
    ];
    if let Some(search) = search {
        params.push(format!("search={}", encode_query_value(search)));
    }
    params.push(format!("_ts={}", js_sys::Date::now()));
    let url = format!("{}/commits/eval-queue?{}", base_url(), params.join("&"));
    fetch_json(&url).await
}

/// Persist queue ordering for active commit evaluations.
pub async fn reorder_eval_queue(ordered_commit_ids: &[i32]) -> Result<(), ApiClientError> {
    let url = format!("{}/commits/eval-queue/reorder", base_url());
    let request = ReorderEvalQueueRequest {
        ordered_commit_ids: ordered_commit_ids.to_vec(),
    };
    send_empty_with_csrf("POST", &url, Some(&request)).await
}

/// Cancel an evaluation (pending → cancelled; in_progress → cancelling).
///
/// Returns Ok(()) on HTTP 200 (cancelled) or 202 (cancelling in progress).
pub async fn cancel_commit_evaluation(commit_id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/commits/{}/cancel-evaluation", base_url(), commit_id);
    let (status, body) = send_request_with_csrf("POST", &url, None).await?;
    if status == 200 || status == 202 {
        Ok(())
    } else {
        Err(ApiClientError::Status {
            code: status,
            body: decode_api_error_message(&body),
        })
    }
}

/// Trigger manual re-evaluation for a commit (resets attempt count and re-queues).
pub async fn re_evaluate_commit(commit_id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/commits/{}/re-evaluate", base_url(), commit_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Force-cancel an evaluation stuck in 'cancelling' state.
pub async fn force_cancel_commit_evaluation(commit_id: i32) -> Result<(), ApiClientError> {
    let url = format!(
        "{}/commits/{}/force-cancel-evaluation",
        base_url(),
        commit_id
    );
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Fetch historical evaluation logs from database for a specific commit.
///
/// Returns persisted logs for completed/failed/cancelled evaluations.
/// For in-progress evaluations, use WebSocket streaming instead.
pub async fn fetch_eval_logs(commit_id: i32) -> Result<Vec<EvalLogEntry>, ApiClientError> {
    let url = format!(
        "{}/commits/{}/eval/logs?_ts={}",
        base_url(),
        commit_id,
        js_sys::Date::now()
    );
    fetch_json(&url).await
}

pub async fn fetch_eval_policy_matrix(
    commit_id: i32,
) -> Result<EvalPolicyMatrixResponse, ApiClientError> {
    let url = format!(
        "{}/commits/{}/eval/policy-matrix?_ts={}",
        base_url(),
        commit_id,
        js_sys::Date::now()
    );
    fetch_json(&url).await
}

pub async fn fetch_eval_dependency_graph(
    commit_id: i32,
) -> Result<EvalDependencyGraphResponse, ApiClientError> {
    let url = format!(
        "{}/commits/{}/eval/dependency-graph?_ts={}",
        base_url(),
        commit_id,
        js_sys::Date::now()
    );
    fetch_json(&url).await
}

/// Fetch paginated evaluation history (complete, failed, cancelled).
pub async fn fetch_eval_history(
    page: i64,
    limit: i64,
    status: Option<&str>,
    flake: Option<&str>,
    search: Option<&str>,
    latest_only: bool,
) -> Result<EvalHistoryPage, ApiClientError> {
    let mut params = format!("page={page}&limit={limit}&latest_only={latest_only}");
    if let Some(s) = status {
        params.push_str(&format!("&status={}", encode_query_value(s)));
    }
    if let Some(f) = flake {
        params.push_str(&format!("&flake={}", encode_query_value(f)));
    }
    if let Some(search) = search {
        params.push_str(&format!("&search={}", encode_query_value(search)));
    }
    let url = format!("{}/commits/eval-history?{}", base_url(), params);
    fetch_json(&url).await
}

/// Percent-encode a query parameter value using the browser's encodeURIComponent.
///
/// This ensures characters like spaces, &, #, %, +, and other reserved characters
/// are safely encoded before being interpolated into a URL query string.
fn encode_query_value(value: &str) -> String {
    js_sys::encode_uri_component(value).into()
}

/// URL-encode a path component (e.g., CVE ID) for safe interpolation into URL paths.
fn encode_uri_component(value: &str) -> String {
    js_sys::encode_uri_component(value).into()
}

/// Fetch build jobs with pagination and optional filtering.
///
/// All user-supplied filter values are percent-encoded via encodeURIComponent before
/// being appended to the URL, so inputs containing spaces, &, #, or similar characters
/// are transmitted correctly.
pub async fn fetch_build_queue_paginated(
    params: &crate::api::models::BuildQueueParams,
) -> Result<crate::api::models::BuildQueuePageResponse, ApiClientError> {
    let base = format!("{}/build-jobs", base_url());
    let mut parts: Vec<String> = Vec::new();

    // Numeric params are safe to interpolate directly (no user-controlled text).
    if let Some(p) = params.page {
        parts.push(format!("page={}", p));
    }
    if let Some(l) = params.limit {
        parts.push(format!("limit={}", l));
    }

    // String params must be percent-encoded.
    if let Some(s) = &params.status {
        if !s.is_empty() {
            parts.push(format!("status={}", encode_query_value(s)));
        }
    }
    if let Some(ch) = &params.commit_hash {
        if !ch.is_empty() {
            parts.push(format!("commit_hash={}", encode_query_value(ch)));
        }
    }
    if let Some(fn_) = &params.flake_name {
        if !fn_.is_empty() {
            parts.push(format!("flake_name={}", encode_query_value(fn_)));
        }
    }
    if let Some(cn) = &params.config_name {
        if !cn.is_empty() {
            parts.push(format!("config_name={}", encode_query_value(cn)));
        }
    }
    // RFC 3339 timestamps are ASCII-safe but encode them for defensive correctness
    // (colons and plus signs in timezone offsets are not query-safe in all contexts).
    if let Some(qa) = params.queued_after {
        parts.push(format!(
            "queued_after={}",
            encode_query_value(&qa.to_rfc3339())
        ));
    }
    if let Some(qb) = params.queued_before {
        parts.push(format!(
            "queued_before={}",
            encode_query_value(&qb.to_rfc3339())
        ));
    }
    if let Some(search) = &params.search {
        if !search.is_empty() {
            parts.push(format!("search={}", encode_query_value(search)));
        }
    }
    parts.push(format!("latest_only={}", params.latest_only));

    let url = if parts.is_empty() {
        base
    } else {
        format!("{}?{}", base, parts.join("&"))
    };
    fetch_json(&url).await
}

/// Move a queued build job to the front of the queue (admin/operator).
pub async fn prioritize_build_job(job_id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/build-jobs/{}/prioritize", base_url(), job_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Move a queued build job one position earlier in the queue.
pub async fn move_build_job_up(job_id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/build-jobs/{}/move-up", base_url(), job_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Move a queued build job one position later in the queue.
pub async fn move_build_job_down(job_id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/build-jobs/{}/move-down", base_url(), job_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Reorder the entire build queue with a new order.
/// ordered_job_ids must contain all queued job UUIDs exactly once.
pub async fn reorder_build_queue(ordered_job_ids: &[uuid::Uuid]) -> Result<(), ApiClientError> {
    #[derive(serde::Serialize)]
    struct ReorderRequest {
        ordered_job_ids: Vec<uuid::Uuid>,
    }

    let url = format!("{}/build-queue/reorder", base_url());
    let request = ReorderRequest {
        ordered_job_ids: ordered_job_ids.to_vec(),
    };
    send_empty_with_csrf("POST", &url, Some(&request)).await
}

/// Cancel a queued or building job (admin/operator).
/// Returns the updated job with new status (cancelled or cancelling).
pub async fn cancel_build_job(job_id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/build-jobs/{}/cancel", base_url(), job_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Re-enqueue a terminal job (operator/admin).
///
/// Creates a new queued build attempt row for the same derivation/context while
/// preserving immutable history on prior attempts.
pub async fn requeue_build_job(job_id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/build-jobs/{}/requeue", base_url(), job_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Force-cancel a build job stuck in 'cancelling' state (admin-only).
///
/// Unlike regular cancel, this immediately transitions to 'cancelled' without
/// waiting for builder confirmation. Use this for stuck builds that failed to
/// complete graceful shutdown.
pub async fn force_cancel_build_job(job_id: &uuid::Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/build-jobs/{}/force-cancel", base_url(), job_id);
    send_empty_with_csrf("POST", &url, None::<&()>).await
}

/// Fetch recent completed/failed build jobs.
pub async fn fetch_recent_build_jobs(
    params: &crate::api::models::BuildQueueParams,
) -> Result<crate::api::models::BuildQueuePageResponse, ApiClientError> {
    let mut parts = vec![format!("limit={}", params.limit.unwrap_or(100))];
    for (name, value) in [
        ("status", params.status.as_deref()),
        ("commit_hash", params.commit_hash.as_deref()),
        ("flake_name", params.flake_name.as_deref()),
        ("config_name", params.config_name.as_deref()),
        ("search", params.search.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            parts.push(format!("{name}={}", encode_query_value(value)));
        }
    }
    parts.push(format!("latest_only={}", params.latest_only));
    let url = format!("{}/build-jobs/recent?{}", base_url(), parts.join("&"));
    fetch_json(&url).await
}

pub async fn request_system_rollback(
    id: &uuid::Uuid,
    request: &SystemRollbackRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/rollback", base_url(), id);
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn request_system_generation_rollback(
    id: &uuid::Uuid,
    request: &SystemRollbackGenerationRequest,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/systems/{}/rollback-generation", base_url(), id);
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn verify_generation_closure(
    id: &uuid::Uuid,
    request: &VerifyGenerationClosureRequest,
) -> Result<VerifyGenerationClosureResponse, ApiClientError> {
    let url = format!("{}/systems/{}/verify-generation-closure", base_url(), id);
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
pub async fn fetch_environment_policies_map()
-> Result<Vec<EnvironmentPolicyMapEntry>, ApiClientError> {
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

pub async fn fetch_compliance_bundles() -> Result<Vec<ComplianceBundleSummary>, ApiClientError> {
    let url = format!("{}/compliance/bundles", base_url());
    fetch_json(&url).await
}

/// Fetch custom policy grouping schemes visible to the authenticated user.
pub async fn fetch_compliance_grouping_schemes(
) -> Result<Vec<ComplianceGroupingScheme>, ApiClientError> {
    let url = format!("{}/compliance/grouping-schemes", base_url());
    fetch_json(&url).await
}

pub async fn create_compliance_grouping_scheme(
    request: &ComplianceGroupingSchemeRequest,
) -> Result<ComplianceGroupingScheme, ApiClientError> {
    let url = format!("{}/compliance/grouping-schemes", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn update_compliance_grouping_scheme(
    id: &Uuid,
    request: &ComplianceGroupingSchemeRequest,
) -> Result<ComplianceGroupingScheme, ApiClientError> {
    let url = format!("{}/compliance/grouping-schemes/{}", base_url(), id);
    send_json_with_csrf("PUT", &url, Some(request)).await
}

pub async fn delete_compliance_grouping_scheme(id: &Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/compliance/grouping-schemes/{}", base_url(), id);
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}

/// Fetch the exact policy-version membership for one bundle revision.
pub async fn fetch_bundle_version_policy_membership(
    bundle_version_id: &Uuid,
) -> Result<Vec<BundleVersionPolicyMembership>, ApiClientError> {
    let url = format!(
        "{}/compliance/bundle-versions/{}/policies",
        base_url(),
        bundle_version_id
    );
    fetch_json(&url).await
}

pub async fn fetch_bundle_version_requirement_membership(
    version_id: &Uuid,
) -> Result<Vec<BundleVersionRequirementMembership>, ApiClientError> {
    let url = format!(
        "{}/compliance/bundle-versions/{}/requirements",
        base_url(),
        version_id
    );
    fetch_json(&url).await
}

pub async fn fetch_compliance_bundle_systems(
    bundle_id: &Uuid,
    version_id: Option<&Uuid>,
) -> Result<ComplianceBundleSystemsResponse, ApiClientError> {
    let url = match version_id {
        Some(version_id) => format!(
            "{}/compliance/bundles/{}/systems?version_id={}",
            base_url(),
            bundle_id,
            version_id
        ),
        None => format!("{}/compliance/bundles/{}/systems", base_url(), bundle_id),
    };
    fetch_json(&url).await
}

/// Fetch compliance bundles applicable to a specific system with rollups.
/// Optimized system-scoped endpoint that avoids N×fleet fetches.
pub async fn fetch_system_compliance_bundles(
    system_id: &Uuid,
) -> Result<SystemComplianceBundlesResponse, ApiClientError> {
    let url = format!("{}/systems/{}/compliance", base_url(), system_id);
    fetch_json(&url).await
}

pub async fn fetch_compliance_system_evidence(
    bundle_id: &Uuid,
    system_id: &Uuid,
    version_id: Option<&Uuid>,
) -> Result<ComplianceEvidenceResponse, ApiClientError> {
    let url = match version_id {
        Some(version_id) => format!(
            "{}/compliance/bundles/{}/systems/{}/evidence?version_id={}",
            base_url(),
            bundle_id,
            system_id,
            version_id
        ),
        None => format!(
            "{}/compliance/bundles/{}/systems/{}/evidence",
            base_url(),
            bundle_id,
            system_id
        ),
    };
    fetch_json(&url).await
}

pub async fn create_compliance_bundle(
    request: &CreateComplianceBundleRequest,
) -> Result<ComplianceBundleSummary, ApiClientError> {
    let url = format!("{}/compliance/bundles", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn update_compliance_bundle(
    bundle_id: &Uuid,
    request: &UpdateComplianceBundleRequest,
) -> Result<ComplianceBundleSummary, ApiClientError> {
    let url = format!("{}/compliance/bundles/{}", base_url(), bundle_id);
    send_json_with_csrf("PUT", &url, Some(request)).await
}

pub async fn delete_compliance_bundle(bundle_id: &Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/compliance/bundles/{}", base_url(), bundle_id);
    send_empty_with_csrf::<()>("DELETE", &url, None).await
}

/// Fetch deletion eligibility for a compliance bundle without mutating anything.
pub async fn fetch_bundle_deletion_eligibility(
    bundle_id: &Uuid,
) -> Result<DeletionEligibility, ApiClientError> {
    let url = format!(
        "{}/compliance/bundles/{}/deletion-eligibility",
        base_url(),
        bundle_id
    );
    fetch_json(&url).await
}

// =============================================================================
// Deployment Policies CRUD API
// =============================================================================

/// Fetch deployment policies with pagination.
pub async fn fetch_deployment_policies(
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<DeploymentPoliciesListResponse, ApiClientError> {
    let mut url = format!("{}/deployment-policies", base_url());
    let mut query_parts = Vec::new();
    if let Some(l) = limit {
        query_parts.push(format!("limit={}", l));
    }
    if let Some(o) = offset {
        query_parts.push(format!("offset={}", o));
    }
    if !query_parts.is_empty() {
        url.push('?');
        url.push_str(&query_parts.join("&"));
    }
    fetch_json(&url).await
}

/// Fetch a single deployment policy by ID.
pub async fn fetch_deployment_policy(id: &Uuid) -> Result<DeploymentPolicyRecord, ApiClientError> {
    let url = format!("{}/deployment-policies/{}", base_url(), id);
    fetch_json(&url).await
}

/// Create a new deployment policy (Admin/Operator only).
pub async fn create_deployment_policy(
    request: &CreateDeploymentPolicyRequest,
) -> Result<DeploymentPolicyRecord, ApiClientError> {
    let url = format!("{}/deployment-policies", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Update an existing deployment policy (Admin/Operator only).
pub async fn update_deployment_policy(
    id: &Uuid,
    request: &UpdateDeploymentPolicyRequest,
) -> Result<DeploymentPolicyRecord, ApiClientError> {
    let url = format!("{}/deployment-policies/{}", base_url(), id);
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Delete a deployment policy (Admin only).
pub async fn delete_deployment_policy(id: &Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/deployment-policies/{}", base_url(), id);
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}

/// Fetch deletion eligibility for a deployment policy without mutating anything.
pub async fn fetch_policy_deletion_eligibility(
    id: &Uuid,
) -> Result<DeletionEligibility, ApiClientError> {
    let url = format!("{}/deployment-policies/{}/deletion-eligibility", base_url(), id);
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

/// Fetch credential summary for a flake.
pub async fn fetch_flake_credentials(id: i32) -> Result<FlakeCredentialSummary, ApiClientError> {
    let url = format!("{}/flakes/{id}/credentials", base_url());
    fetch_json(&url).await
}

/// Replace flake credentials.
pub async fn put_flake_credentials(
    id: i32,
    request: &CreateFlakeCredentialRequest,
) -> Result<FlakeCredentialSummary, ApiClientError> {
    let url = format!("{}/flakes/{id}/credentials", base_url());
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Partially update flake credentials.
pub async fn patch_flake_credentials(
    id: i32,
    request: &UpdateFlakeCredentialRequest,
) -> Result<FlakeCredentialSummary, ApiClientError> {
    let url = format!("{}/flakes/{id}/credentials", base_url());
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

/// Delete flake credentials.
pub async fn delete_flake_credentials(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/flakes/{id}/credentials", base_url());
    send_empty_with_csrf::<()>("DELETE", &url, None).await
}

/// Test flake credentials against remote repository access.
pub async fn test_flake_credentials(
    id: i32,
    request: &TestFlakeCredentialRequest,
) -> Result<TestFlakeCredentialResponse, ApiClientError> {
    let url = format!("{}/flakes/{id}/credentials/test", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Remove a flake by id.
pub async fn delete_flake(id: i32, hard: bool, cascade: bool) -> Result<(), ApiClientError> {
    let mut url = format!("{}/flakes/{id}", base_url());

    let mut params = Vec::new();
    if hard {
        params.push("hard=true");
    }
    if cascade {
        params.push("cascade=true");
    }

    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    send_empty_with_csrf::<()>("DELETE", &url, None).await
}

/// Refresh a flake's cached git repository.
///
/// Forces Nix to re-fetch the flake from remote, clearing stale cached references.
/// Useful when a flake repository has been force-pushed or history rewritten.
pub async fn refresh_flake(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/flakes/{}/refresh", base_url(), id);
    send_empty_with_csrf::<()>("POST", &url, None).await
}

/// Fetch flake timelines with recent commits for dashboard.
pub async fn fetch_flake_timelines() -> Result<Vec<FlakeTimeline>, ApiClientError> {
    let url = format!("{}/flakes/timelines", base_url());
    fetch_json(&url).await
}

/// Fetch flake timelines for a subset of flake IDs.
pub async fn fetch_flake_timelines_for_ids(
    flake_ids: &[i32],
) -> Result<Vec<FlakeTimeline>, ApiClientError> {
    if flake_ids.is_empty() {
        return Ok(Vec::new());
    }

    let ids = flake_ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let url = format!("{}/flakes/timelines?ids={}", base_url(), ids);
    fetch_json(&url).await
}

/// Fetch flake timeline for a single flake with extended commit limit (for tray view).
pub async fn fetch_flake_timeline_for_tray(
    flake_id: i32,
) -> Result<Vec<FlakeTimeline>, ApiClientError> {
    // Keep the initial tray payload bounded. Fifty commits is enough for the
    // immediately visible history while avoiding path/config enrichment for
    // 200 commits on every tray open.
    let url = format!("{}/flakes/timelines?ids={}&limit=50", base_url(), flake_id);
    fetch_json(&url).await
}

/// Fetch flake timelines for dashboard (CF system deployment counts).
pub async fn fetch_dashboard_flake_timelines() -> Result<Vec<FlakeTimeline>, ApiClientError> {
    let url = format!("{}/flakes/timelines?view=dashboard", base_url());
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

/// Accept a detected history rewrite for a specific flake.
pub async fn accept_flake_history_rewrite(
    id: i32,
) -> Result<SystemMutationResponse, ApiClientError> {
    let url = format!("{}/flakes/{id}/accept-rewrite", base_url());
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Fetch navigation badge counts for the sidebar.
pub async fn get_navigation_badges() -> Result<NavigationBadges, ApiClientError> {
    let url = format!("{}/navigation/badges", base_url());
    fetch_json(&url).await
}

/// Record that the current user has acknowledged an alert category (e.g. by
/// visiting Systems/Flakes/Environments/CVEs, or opening the failures tab on
/// Builds/Evaluations). Persists server-side so the corresponding badge stays
/// hidden across page refresh, browser restart, and re-login until something
/// new appears — see `alerts::acknowledge`.
///
/// `observed_at` must be the `observed_at` field from the `NavigationBadges`
/// response the user was actually shown, so the server anchors `last_seen_at`
/// to that snapshot rather than to the POST receive time.
pub async fn acknowledge_navigation_category(
    category: &str,
    observed_at: &str,
    occurrence_ids: &[String],
) -> Result<NavigationBadges, ApiClientError> {
    #[derive(serde::Serialize)]
    struct AcknowledgeRequest<'a> {
        category: &'a str,
        observed_at: &'a str,
        occurrence_ids: &'a [String],
    }
    let url = format!("{}/navigation/acknowledge", base_url());
    let body = AcknowledgeRequest {
        category,
        observed_at,
        occurrence_ids,
    };
    send_json_with_csrf("POST", &url, Some(&body)).await
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

pub async fn fetch_user_preferences() -> Result<UserPreferencesResponse, ApiClientError> {
    let url = format!("{}/user/preferences", base_url());
    fetch_json(&url).await
}

pub async fn update_user_preferences(
    request: &UpdateUserPreferences,
) -> Result<UserPreferencesResponse, ApiClientError> {
    let url = format!("{}/user/preferences", base_url());
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

pub async fn initialize_user_preferences(
    request: &UpdateUserPreferences,
) -> Result<UserPreferencesResponse, ApiClientError> {
    let url = format!("{}/user/preferences/initialize", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

pub async fn fetch_notification_preferences() -> Result<NotificationPreferencesDto, ApiClientError>
{
    let url = format!("{}/user/notification-preferences", base_url());
    fetch_json(&url).await
}

pub async fn update_notification_preferences(
    request: &UpdateNotificationPreferences,
) -> Result<NotificationPreferencesDto, ApiClientError> {
    let url = format!("{}/user/notification-preferences", base_url());
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

pub async fn fetch_user_notifications(
    limit: Option<i64>,
    before: Option<DateTime<Utc>>,
    unread_only: bool,
) -> Result<UserNotificationsResponse, ApiClientError> {
    let mut params = Vec::new();
    if let Some(limit) = limit {
        params.push(format!("limit={limit}"));
    }
    if let Some(before) = before {
        params.push(format!("before={}", before.to_rfc3339()));
    }
    if unread_only {
        params.push("unread_only=true".to_string());
    }

    let mut url = format!("{}/user/notifications", base_url());
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }
    fetch_json(&url).await
}

pub async fn mark_user_notification_read(notification_id: Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/user/notifications/{}/read", base_url(), notification_id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn mark_all_user_notifications_read() -> Result<(), ApiClientError> {
    let url = format!("{}/user/notifications/read-all", base_url());
    send_json_with_csrf("POST", &url, None::<&()>).await
}

pub async fn dismiss_user_notification(notification_id: Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/user/notifications/{}", base_url(), notification_id);
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}

pub async fn fetch_user_sessions() -> Result<UserSessionsResponse, ApiClientError> {
    let url = format!("{}/user/sessions", base_url());
    fetch_json(&url).await
}

pub async fn revoke_user_session(session_id: Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/user/sessions/{}", base_url(), session_id);
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}

pub async fn revoke_all_user_sessions() -> Result<(), ApiClientError> {
    let url = format!("{}/user/sessions/revoke-all", base_url());
    send_json_with_csrf("POST", &url, None::<&()>).await
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

/// Fetch pipeline readiness health checks (admin only).
pub async fn fetch_config_health() -> Result<ConfigHealthResponse, ApiClientError> {
    let url = format!("{}/admin/config-health", base_url());
    fetch_json(&url).await
}

/// Fetch real server/build/database runtime info (admin only).
pub async fn fetch_admin_server_info() -> Result<ServerRuntimeInfoResponse, ApiClientError> {
    let url = format!("{}/admin/server-info", base_url());
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

/// Fetch setup wizard progress for the current admin user.
pub async fn fetch_setup_wizard_progress() -> Result<SetupWizardProgressResponse, ApiClientError> {
    let url = format!("{}/admin/setup-progress", base_url());
    fetch_json(&url).await
}

/// Set setup wizard dismissal state for current admin user.
pub async fn set_setup_wizard_dismissed(dismissed: bool) -> Result<(), ApiClientError> {
    let url = format!("{}/admin/setup-wizard/dismiss", base_url());
    let request = SetupWizardDismissRequest { dismissed };
    send_empty_with_csrf("POST", &url, Some(&request)).await
}

/// Set agent step acknowledgment for current admin user.
pub async fn set_setup_wizard_agent_acknowledged(acknowledged: bool) -> Result<(), ApiClientError> {
    let url = format!("{}/admin/setup-wizard/agent-acknowledge", base_url());
    let request = SetupWizardAcknowledgeAgentRequest { acknowledged };
    send_empty_with_csrf("POST", &url, Some(&request)).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Builder Management
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch all builders with summary info
pub async fn fetch_builders() -> Result<Vec<BuilderSummary>, ApiClientError> {
    let url = format!("{}/builders", base_url());
    fetch_json(&url).await
}

/// Fetch a single builder's details
pub async fn fetch_builder(id: &Uuid) -> Result<BuilderDetail, ApiClientError> {
    let url = format!("{}/builders/{}", base_url(), id);
    fetch_json(&url).await
}

/// Create a new builder (admin only)
pub async fn create_builder(
    request: &CreateBuilderRequest,
) -> Result<BuilderCreatedResponse, ApiClientError> {
    let url = format!("{}/builders", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Update a builder's configuration (admin only)
pub async fn update_builder(
    id: &Uuid,
    request: &UpdateBuilderRequest,
) -> Result<BuilderDetail, ApiClientError> {
    let url = format!("{}/builders/{}", base_url(), id);
    send_json_with_csrf("PATCH", &url, Some(request)).await
}

/// Update a builder's public key (admin only)
pub async fn update_builder_public_key(
    id: &Uuid,
    request: &UpdateBuilderPublicKeyRequest,
) -> Result<BuilderDetail, ApiClientError> {
    let url = format!("{}/builders/{}/public-key", base_url(), id);
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Deactivate a builder (admin only)
pub async fn deactivate_builder(id: &Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/builders/{}", base_url(), id);
    let _deleted: serde_json::Value = send_json_with_csrf("DELETE", &url, None::<&()>).await?;
    Ok(())
}

/// Permanently delete a builder (admin only)
pub async fn delete_builder_permanently(id: &Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/builders/{}/permanent", base_url(), id);
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}

/// Update builder environment assignments (admin only)
pub async fn update_builder_environments(
    id: &Uuid,
    request: &UpdateBuilderEnvironmentsRequest,
) -> Result<(), ApiClientError> {
    let url = format!("{}/builders/{}/environments", base_url(), id);
    send_empty_with_csrf("PATCH", &url, Some(request)).await
}

/// Fetch builder metrics history
pub async fn fetch_builder_metrics(id: &Uuid) -> Result<Vec<BuilderMetrics>, ApiClientError> {
    let url = format!("{}/builders/{}/metrics", base_url(), id);
    fetch_json(&url).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Cache Management API
// ─────────────────────────────────────────────────────────────────────────────

/// List cache destinations
pub async fn fetch_cache_destinations(
    enabled_only: bool,
) -> Result<Vec<CacheDestination>, ApiClientError> {
    let url = if enabled_only {
        format!("{}/caches?enabled_only=true", base_url())
    } else {
        format!("{}/caches", base_url())
    };
    fetch_json(&url).await
}

/// Get a single cache destination by ID
pub async fn fetch_cache_destination(id: i32) -> Result<CacheDestination, ApiClientError> {
    let url = format!("{}/caches/{}", base_url(), id);
    fetch_json(&url).await
}

/// Create a new cache destination
pub async fn create_cache_destination(
    data: &CreateCacheDestination,
) -> Result<CacheDestination, ApiClientError> {
    let url = format!("{}/caches", base_url());
    send_json_with_csrf("POST", &url, Some(data)).await
}

/// Test cache destination credentials/configuration
pub async fn test_cache_destination_credentials(
    data: &CreateCacheDestination,
) -> Result<CacheCredentialTestResult, ApiClientError> {
    let url = format!("{}/caches/test-credentials", base_url());
    send_json_with_csrf("POST", &url, Some(data)).await
}

/// Update an existing cache destination
pub async fn update_cache_destination(
    id: i32,
    data: &UpdateCacheDestination,
) -> Result<CacheDestination, ApiClientError> {
    let url = format!("{}/caches/{}", base_url(), id);
    send_json_with_csrf("PUT", &url, Some(data)).await
}

/// Delete a cache destination
pub async fn delete_cache_destination(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/caches/{}", base_url(), id);
    send_empty_with_csrf::<()>("DELETE", &url, None).await
}

/// List cache push jobs with optional filtering
pub async fn fetch_cache_push_jobs(
    status: Option<&str>,
    limit: i32,
    offset: i32,
) -> Result<Vec<CachePushJob>, ApiClientError> {
    let mut url = format!(
        "{}/cache-push-jobs?limit={}&offset={}",
        base_url(),
        limit,
        offset
    );
    if let Some(s) = status {
        url.push_str(&format!("&status={}", s));
    }
    fetch_json(&url).await
}

/// Get a single cache push job by ID
pub async fn fetch_cache_push_job(id: i32) -> Result<CachePushJob, ApiClientError> {
    let url = format!("{}/cache-push-jobs/{}", base_url(), id);
    fetch_json(&url).await
}

/// Retry a failed cache push job
pub async fn retry_cache_push_job(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/cache-push-jobs/{}/retry", base_url(), id);
    send_empty_with_csrf::<()>("POST", &url, None).await
}

/// Cancel a pending cache push job
pub async fn cancel_cache_push_job(id: i32) -> Result<(), ApiClientError> {
    let url = format!("{}/cache-push-jobs/{}/cancel", base_url(), id);
    send_empty_with_csrf::<()>("POST", &url, None).await
}

/// Bulk retry cache push jobs
pub async fn bulk_retry_cache_push_jobs(job_ids: Vec<i32>) -> Result<(), ApiClientError> {
    let url = format!("{}/cache-push-jobs/bulk-retry", base_url());
    let data = BulkJobAction { job_ids };
    send_empty_with_csrf("POST", &url, Some(&data)).await
}

/// Bulk cancel cache push jobs
pub async fn bulk_cancel_cache_push_jobs(job_ids: Vec<i32>) -> Result<(), ApiClientError> {
    let url = format!("{}/cache-push-jobs/bulk-cancel", base_url());
    let data = BulkJobAction { job_ids };
    send_empty_with_csrf("POST", &url, Some(&data)).await
}

// Cache environment assignment
#[derive(Debug, serde::Serialize)]
struct AssignEnvironmentsRequest {
    environment_ids: Vec<Uuid>,
}

pub async fn get_cache_environments(cache_id: i32) -> Result<Vec<Uuid>, ApiClientError> {
    let url = format!("{}/caches/{}/environments", base_url(), cache_id);
    send_json_with_csrf("GET", &url, None::<&()>).await
}

pub async fn assign_cache_environments(
    cache_id: i32,
    environment_ids: Vec<Uuid>,
) -> Result<(), ApiClientError> {
    let url = format!("{}/caches/{}/environments", base_url(), cache_id);
    let data = AssignEnvironmentsRequest { environment_ids };
    send_empty_with_csrf("PUT", &url, Some(&data)).await
}

pub async fn get_environment_caches(
    environment_id: Uuid,
) -> Result<Vec<CacheDestination>, ApiClientError> {
    let url = format!("{}/environments/{}/caches", base_url(), environment_id);
    send_json_with_csrf("GET", &url, None::<&()>).await
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

async fn send_empty_with_csrf<B: serde::Serialize>(
    method: &str,
    url: &str,
    body: Option<&B>,
) -> Result<(), ApiClientError> {
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

    Ok(())
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

    let opts = web_sys::RequestInit::new();
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
    if window.document().is_some() {
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

/// POST with JSON body, expecting JSON response.
async fn post_json<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    url: &str,
    body: &B,
) -> Result<T, ApiClientError> {
    send_json("POST", url, Some(body)).await
}

/// POST without body, expecting JSON response.
async fn post_json_no_body<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, ApiClientError> {
    send_json::<T, ()>("POST", url, None).await
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

    let opts = web_sys::RequestInit::new();
    opts.set_method(method);
    if method.eq_ignore_ascii_case("GET") {
        let _ = js_sys::Reflect::set(
            opts.as_ref(),
            &JsValue::from_str("cache"),
            &JsValue::from_str("no-store"),
        );
    }
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
    if body.trim().is_empty() {
        return "Internal server error".to_string();
    }

    serde_json::from_str::<ApiError>(body)
        .map(|error| error.message)
        .unwrap_or_else(|_| body.to_string())
}

/// Fetch the persisted classification banner configuration.
pub async fn fetch_classification_config() -> Result<ClassificationBannerConfig, ApiClientError> {
    let url = format!("{}/admin/classification-config", base_url());
    fetch_json(&url).await
}

/// Persist classification banner configuration.
pub async fn set_classification_config(
    request: &UpdateClassificationBannerRequest,
) -> Result<ClassificationBannerConfig, ApiClientError> {
    let url = format!("{}/admin/classification-config", base_url());
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Fetch the persisted server-wide automatic retry policy.
pub async fn fetch_automatic_retry_policy() -> Result<AutomaticRetryPolicy, ApiClientError> {
    let url = format!("{}/admin/automatic-retry-policy", base_url());
    fetch_json(&url).await
}

/// Persist the complete server-wide automatic retry policy.
pub async fn set_automatic_retry_policy(
    request: &UpdateAutomaticRetryPolicyRequest,
) -> Result<AutomaticRetryPolicy, ApiClientError> {
    let url = format!("{}/admin/automatic-retry-policy", base_url());
    send_json_with_csrf("PUT", &url, Some(request)).await
}

// ─────────────────────────────────────────────────────────────────────────────
// XCCDF preview and import
// ─────────────────────────────────────────────────────────────────────────────

/// Preview an XCCDF XML/ZIP file without any durable writes.
///
/// Returns parsed benchmark metadata, profiles, rules, and diagnostics.
pub async fn preview_xccdf(
    bytes: &[u8],
    filename: &str,
) -> Result<XccdfPreviewResponse, ApiClientError> {
    let url = format!("{}/compliance/xccdf/preview", base_url());
    let (status, body) = send_xccdf_multipart(&url, bytes, filename, None).await?;
    parse_json_response(status, &body)
}

/// Submit an XCCDF import plan along with the original file bytes.
///
/// The server reparses the file, verifies the digest in the plan, validates
/// every rule action, and commits atomically.
pub async fn import_xccdf(
    bytes: &[u8],
    filename: &str,
    plan: &XccdfImportPlan,
) -> Result<XccdfImportResponse, ApiClientError> {
    let url = format!("{}/compliance/xccdf/import", base_url());
    let plan_json =
        serde_json::to_string(plan).map_err(|e| ApiClientError::Deserialize(e.to_string()))?;
    let (status, body) = send_xccdf_multipart(&url, bytes, filename, Some(&plan_json)).await?;
    parse_json_response(status, &body)
}

/// Send a multipart POST with one `file` field and an optional JSON `plan` field.
async fn send_xccdf_multipart(
    url: &str,
    bytes: &[u8],
    filename: &str,
    plan_json: Option<&str>,
) -> Result<(u16, String), ApiClientError> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let form = web_sys::FormData::new().map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    form.append_with_blob_and_filename("file", &blob, filename)
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;

    if let Some(plan) = plan_json {
        // Keep the import plan a normal multipart text field.  Supplying a
        // filename makes browsers mark it as a file part, which the server
        // correctly rejects because only the XCCDF payload may be a file.
        form.append_with_str("plan", plan)
            .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    }

    let window =
        web_sys::window().ok_or_else(|| ApiClientError::Network("no global window".into()))?;
    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(form.as_ref());
    let _ = js_sys::Reflect::set(
        opts.as_ref(),
        &JsValue::from_str("credentials"),
        &JsValue::from_str("include"),
    );
    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    add_csrf_header(&request, &window)?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| ApiClientError::Network(format!("{e:?}")))?;
    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| ApiClientError::Network("response is not a Response".into()))?;
    let status = resp.status() as u16;
    let text = JsFuture::from(
        resp.text()
            .map_err(|e| ApiClientError::Network(format!("{e:?}")))?,
    )
    .await
    .map_err(|e| ApiClientError::Network(format!("{e:?}")))?
    .as_string()
    .unwrap_or_default();
    Ok((status, text))
}

fn parse_json_response<T: serde::de::DeserializeOwned>(
    status: u16,
    body: &str,
) -> Result<T, ApiClientError> {
    if !(200..300).contains(&status) {
        return Err(ApiClientError::Status {
            code: status,
            body: body.to_string(),
        });
    }
    serde_json::from_str(body).map_err(|e| ApiClientError::Deserialize(e.to_string()))
}

// ─────────────────────────────────────────────────────────────────────────────
// Trust and publication
// ─────────────────────────────────────────────────────────────────────────────

/// Trust or reject a policy version.
pub async fn trust_policy_version(
    version_id: &Uuid,
    request: &TrustPolicyVersionRequest,
) -> Result<TrustPolicyVersionResponse, ApiClientError> {
    let url = format!("{}/policy-versions/{}/trust", base_url(), version_id);
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Publish a policy version (makes it immutable / accepted).
pub async fn publish_policy_version(
    version_id: &Uuid,
) -> Result<serde_json::Value, ApiClientError> {
    let url = format!("{}/policy-versions/{}/publish", base_url(), version_id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Create a new mutable draft from a published policy version.
pub async fn create_policy_draft(policy_id: &Uuid) -> Result<serde_json::Value, ApiClientError> {
    let url = format!("{}/policies/{}/drafts", base_url(), policy_id);
    send_json_with_csrf("POST", &url, None::<&()>).await
}

/// Trust or reject a bundle version.
pub async fn trust_bundle_version(
    version_id: &Uuid,
    request: &TrustBundleVersionRequest,
) -> Result<TrustBundleVersionResponse, ApiClientError> {
    let url = format!(
        "{}/compliance/bundle-versions/{}/trust",
        base_url(),
        version_id
    );
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Publish a bundle version (makes it immutable / accepted).
pub async fn publish_bundle_version(
    version_id: &Uuid,
    request: &PublishBundleVersionRequest,
) -> Result<PublishBundleVersionResponse, ApiClientError> {
    let url = format!(
        "{}/compliance/bundle-versions/{}/publish",
        base_url(),
        version_id
    );
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Create a new mutable draft from a published bundle version.
pub async fn create_bundle_draft(
    bundle_id: &Uuid,
    request: &CreateBundleDraftRequest,
) -> Result<CreateBundleDraftResponse, ApiClientError> {
    let url = format!("{}/compliance/bundles/{}/drafts", base_url(), bundle_id);
    send_json_with_csrf("POST", &url, Some(request)).await
}

// ─────────────────────────────────────────────────────────────────────────────
// Assignments
// ─────────────────────────────────────────────────────────────────────────────

/// Create a new bundle assignment for an environment or system.
pub async fn create_compliance_assignment(
    request: &CreateAssignmentRequest,
) -> Result<AssignmentResponse, ApiClientError> {
    let url = format!("{}/compliance/assignments", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Preview an assignment overlay without persisting it.
pub async fn preview_compliance_assignment(
    request: &CreateAssignmentRequest,
) -> Result<EffectivePolicySetResponse, ApiClientError> {
    let url = format!("{}/compliance/assignments/preview", base_url());
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Fetch a single assignment by ID.
pub async fn fetch_compliance_assignment(
    assignment_id: &Uuid,
) -> Result<AssignmentResponse, ApiClientError> {
    let url = format!("{}/compliance/assignments/{}", base_url(), assignment_id);
    fetch_json(&url).await
}

/// Fetch all assignments for an environment.
/// The server returns `{ "assignments": [...] }`; this unwraps to the inner Vec.
pub async fn fetch_environment_assignments(
    environment_id: &Uuid,
) -> Result<Vec<AssignmentResponse>, ApiClientError> {
    let url = format!(
        "{}/environments/{}/compliance-assignments",
        base_url(),
        environment_id
    );
    let wrapper: AssignmentListResponse = fetch_json(&url).await?;
    Ok(wrapper.assignments)
}

/// Fetch all assignments for a system.
/// The server returns `{ "assignments": [...] }`; this unwraps to the inner Vec.
pub async fn fetch_system_assignments(
    system_id: &Uuid,
) -> Result<Vec<AssignmentResponse>, ApiClientError> {
    let url = format!(
        "{}/systems/{}/compliance-assignments",
        base_url(),
        system_id
    );
    let wrapper: AssignmentListResponse = fetch_json(&url).await?;
    Ok(wrapper.assignments)
}

/// URL for downloading the server-generated effective XCCDF for an assignment.
/// The assignment export includes its overlay and resolved policy configuration;
/// XML generation remains entirely on the server.
pub fn compliance_assignment_xccdf_url(assignment_id: &Uuid) -> String {
    format!(
        "{}/compliance/assignments/{}/xccdf",
        base_url(),
        assignment_id
    )
}

/// Delete (deactivate) an assignment.
pub async fn delete_compliance_assignment(assignment_id: &Uuid) -> Result<(), ApiClientError> {
    let url = format!("{}/compliance/assignments/{}", base_url(), assignment_id);
    send_empty_with_csrf::<()>("DELETE", &url, None).await
}

/// Update an assignment (creates a new immutable version).
pub async fn update_compliance_assignment(
    assignment_id: &Uuid,
    request: &UpdateAssignmentRequest,
) -> Result<AssignmentResponse, ApiClientError> {
    let url = format!("{}/compliance/assignments/{}", base_url(), assignment_id);
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Fetch the resolved effective policy set for a system.
pub async fn fetch_system_effective_policies(
    system_id: &Uuid,
) -> Result<EffectivePolicySetResponse, ApiClientError> {
    let url = format!("{}/systems/{}/effective-policies", base_url(), system_id);
    fetch_json(&url).await
}

// ── TASK-418: Frameworks, requirements, mappings, coverage ────────────────────

/// List all compliance framework lineages.
pub async fn fetch_compliance_frameworks(
) -> Result<Vec<ComplianceFrameworkSummary>, ApiClientError> {
    let url = format!("{}/compliance/frameworks", base_url());
    fetch_json(&url).await
}

/// List all versions of a specific compliance framework.
pub async fn fetch_compliance_framework_versions(
    framework_id: &Uuid,
) -> Result<Vec<ComplianceFrameworkVersionSummary>, ApiClientError> {
    let url = format!("{}/compliance/frameworks/{}/versions", base_url(), framework_id);
    fetch_json(&url).await
}

/// Fetch policy versions that have a normalized requirement mapping to a
/// framework.  This is a bulk projection for the bundle policy picker.
pub async fn fetch_framework_mapped_policy_versions(
    framework_id: &Uuid,
) -> Result<FrameworkMappedPolicyVersionsResponse, ApiClientError> {
    let url = format!(
        "{}/compliance/frameworks/{}/mapped-policy-versions",
        base_url(),
        framework_id
    );
    fetch_json(&url).await
}

/// Server-side requirement search within a framework version.
///
/// `q` is matched against external_id, title, CCI IDs, and SRG IDs.
pub async fn search_requirements(
    framework_version_id: &Uuid,
    q: Option<&str>,
    kind: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<RequirementVersionSummary>, ApiClientError> {
    let mut params = vec![
        format!("limit={}", limit),
        format!("offset={}", offset),
    ];
    if let Some(q_str) = q {
        params.push(format!("q={}", encode_uri_component(q_str)));
    }
    if let Some(kind_str) = kind {
        params.push(format!("kind={}", encode_uri_component(kind_str)));
    }
    let url = format!(
        "{}/compliance/framework-versions/{}/requirements?{}",
        base_url(),
        framework_version_id,
        params.join("&")
    );
    fetch_json(&url).await
}

/// List child requirement versions for a parent in the hierarchy.
pub async fn fetch_requirement_children(
    parent_id: &Uuid,
) -> Result<Vec<RequirementVersionSummary>, ApiClientError> {
    let url = format!(
        "{}/compliance/requirement-versions/{}/children",
        base_url(),
        parent_id
    );
    fetch_json(&url).await
}

/// Fetch authoritative requirement coverage for a bundle version.
pub async fn fetch_bundle_requirement_coverage(
    bundle_version_id: &Uuid,
) -> Result<BundleCoverageReport, ApiClientError> {
    let url = format!(
        "{}/compliance/bundle-versions/{}/requirement-coverage",
        base_url(),
        bundle_version_id
    );
    fetch_json(&url).await
}

/// List all requirement mappings for a policy version.
pub async fn fetch_policy_requirement_mappings(
    policy_version_id: &Uuid,
) -> Result<Vec<PolicyMappingRow>, ApiClientError> {
    let url = format!(
        "{}/policy-versions/{}/requirement-mappings",
        base_url(),
        policy_version_id
    );
    fetch_json(&url).await
}

/// Create a new requirement mapping on a mutable (draft) policy version.
pub async fn create_policy_mapping(
    policy_version_id: &Uuid,
    request: &CreatePolicyMappingRequest,
) -> Result<serde_json::Value, ApiClientError> {
    let url = format!(
        "{}/policy-versions/{}/requirement-mappings",
        base_url(),
        policy_version_id
    );
    send_json_with_csrf("POST", &url, Some(request)).await
}

/// Update relationship/coverage/rationale on an existing mapping.
pub async fn update_policy_mapping(
    policy_version_id: &Uuid,
    mapping_id: &Uuid,
    request: &UpdatePolicyMappingRequest,
) -> Result<serde_json::Value, ApiClientError> {
    let url = format!(
        "{}/policy-versions/{}/requirement-mappings/{}",
        base_url(),
        policy_version_id,
        mapping_id
    );
    send_json_with_csrf("PUT", &url, Some(request)).await
}

/// Delete a requirement mapping.
pub async fn delete_policy_mapping(
    policy_version_id: &Uuid,
    mapping_id: &Uuid,
) -> Result<(), ApiClientError> {
    let url = format!(
        "{}/policy-versions/{}/requirement-mappings/{}",
        base_url(),
        policy_version_id,
        mapping_id
    );
    send_empty_with_csrf("DELETE", &url, None::<&()>).await
}
