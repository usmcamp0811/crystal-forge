//! Development mode warning banner.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

#[derive(Debug, Serialize, Deserialize)]
struct SetupStatus {
    requires_setup: bool,
    allow_registration: bool,
    user_count: i64,
    auth_mode: String,
}

/// Banner warning that dev authentication mode is active.
///
/// This banner detects dev mode at runtime by checking the auth_mode from setup-status.
/// It only renders when AUTH_MODE=dev is enabled on the server.
#[component]
pub fn DevModeBanner() -> Element {
    let mut is_dev_mode = use_signal(|| false);
    let mut checked = use_signal(|| false);

    // Check if dev mode is active on component mount
    use_effect(move || {
        if !checked() {
            spawn(async move {
                // Fetch setup status to check auth_mode
                let window = web_sys::window().expect("no global window");
                let location = window.location();
                let origin = location
                    .origin()
                    .unwrap_or_else(|_| "http://localhost:3000".into());
                let url = format!("{}/api/auth/setup-status", origin);

                let mut opts = web_sys::RequestInit::new();
                opts.method("GET");

                if let Ok(request) = web_sys::Request::new_with_str_and_init(&url, &opts) {
                    let response_promise = window.fetch_with_request(&request);
                    let future = wasm_bindgen_futures::JsFuture::from(response_promise);
                    if let Ok(response) = future.await {
                        if let Ok(response) = response.dyn_into::<web_sys::Response>() {
                            if response.status() == 200 {
                                let text_promise = response.text();
                                if let Ok(text_future) = text_promise {
                                    let text_js = wasm_bindgen_futures::JsFuture::from(text_future);
                                    if let Ok(text) = text_js.await {
                                        if let Some(text_str) = text.as_string() {
                                            if let Ok(status) =
                                                serde_json::from_str::<SetupStatus>(&text_str)
                                            {
                                                is_dev_mode.set(status.auth_mode == "dev");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                checked.set(true);
            });
        }
    });

    // Only render if dev mode is detected
    if !is_dev_mode() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "bg-yellow-500 border-b-2 border-yellow-600 px-4 py-2",
            "data-dev-mode-banner": "true",
            div {
                class: "flex items-center justify-center gap-3 max-w-7xl mx-auto",
                // Warning icon
                svg {
                    class: "w-4 h-4 text-yellow-900 flex-shrink-0",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                    }
                }
                // Message
                div {
                    class: "flex items-center gap-2 text-sm",
                    span {
                        class: "font-bold text-yellow-900",
                        "Development Mode:"
                    }
                    span {
                        class: "text-yellow-950",
                        "Authentication bypass is active. Never use AUTH_MODE=dev in production."
                    }
                }
            }
        }
    }
}
