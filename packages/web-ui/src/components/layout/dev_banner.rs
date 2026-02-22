//! Development mode warning banner.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

/// Banner warning that dev authentication mode is active.
///
/// This banner detects dev mode at runtime by checking if the dev login endpoint exists.
/// It only renders when AUTH_MODE=dev is enabled on the server.
#[component]
pub fn DevModeBanner() -> Element {
    let mut is_dev_mode = use_signal(|| false);
    let mut checked = use_signal(|| false);

    // Check if dev mode is active on component mount
    use_effect(move || {
        if !checked() {
            spawn(async move {
                // Probe the dev login endpoint to detect dev mode
                let window = web_sys::window().expect("no global window");
                let location = window.location();
                let origin = location
                    .origin()
                    .unwrap_or_else(|_| "http://localhost:3000".into());
                let url = format!("{}/api/auth/dev/login", origin);

                // Try an OPTIONS request to see if the endpoint exists
                let mut opts = web_sys::RequestInit::new();
                opts.method("OPTIONS");

                if let Ok(request) = web_sys::Request::new_with_str_and_init(&url, &opts) {
                    let response_promise = window.fetch_with_request(&request);
                    let future = wasm_bindgen_futures::JsFuture::from(response_promise);
                    if let Ok(response) = future.await {
                        if let Ok(response) = response.dyn_into::<web_sys::Response>() {
                            // If we get any response (not 404), dev mode is active
                            is_dev_mode.set(response.status() != 404);
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
