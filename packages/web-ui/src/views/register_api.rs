use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct SetupStatus {
    pub requires_setup: bool,
    pub allow_registration: bool,
}

pub(crate) async fn fetch_setup_status(ui_check_mode: bool) -> Option<SetupStatus> {
    if ui_check_mode {
        return Some(SetupStatus {
            requires_setup: true,
            allow_registration: true,
        });
    }

    let window = web_sys::window()?;
    let mut opts = web_sys::RequestInit::new();
    opts.set_method("GET");

    let request = web_sys::Request::new_with_str_and_init("/api/auth/setup-status", &opts).ok()?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .ok()?;
    let resp = resp_value.dyn_into::<web_sys::Response>().ok()?;
    if !resp.ok() {
        return None;
    }

    let text = JsFuture::from(resp.text().ok()?).await.ok()?.as_string()?;
    serde_json::from_str::<SetupStatus>(&text).ok()
}

pub(crate) async fn register_user(payload: &RegisterRequest) -> Result<(), String> {
    let window = web_sys::window().ok_or_else(|| "Missing browser window".to_string())?;

    let mut opts = web_sys::RequestInit::new();
    opts.set_method("POST");

    let json_body =
        serde_json::to_string(payload).map_err(|e| format!("Failed to serialize request: {e}"))?;
    opts.set_body(&JsValue::from_str(&json_body));

    let request = web_sys::Request::new_with_str_and_init("/api/auth/local/register", &opts)
        .map_err(|e| format!("Failed to create request: {e:?}"))?;
    let _ = request.headers().set("Content-Type", "application/json");

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Network error: {e:?}"))?;
    let resp = resp_value
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "Invalid response from server".to_string())?;

    if (200..300).contains(&resp.status()) {
        return Ok(());
    }

    let status = resp.status();
    let status_text = resp.status_text();
    if let Ok(text_promise) = resp.text() {
        if let Ok(text_value) = JsFuture::from(text_promise).await {
            if let Some(text) = text_value.as_string() {
                if !text.is_empty() {
                    return Err(format!("Registration failed ({status}): {text}"));
                }
            }
        }
    }

    Err(format!("Registration failed: {status} {status_text}"))
}
