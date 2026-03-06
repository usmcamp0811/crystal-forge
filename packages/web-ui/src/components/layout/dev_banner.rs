//! Environment mode banners for non-production markers.

use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsCast;

#[derive(Debug, Deserialize)]
struct EvalQueueModeProbe {
    execution_mode: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerPlacement {
    Top,
    Bottom,
}

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

fn api_base_url() -> String {
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

#[component]
fn EnvironmentBanner(placement: BannerPlacement, title: &'static str, message: &'static str) -> Element {
    let border_class = if placement == BannerPlacement::Top {
        "border-b"
    } else {
        "border-t"
    };

    rsx! {
        div {
            class: "bg-orange-600 {border_class} border-orange-800 px-4 py-2",
            "data-environment-banner": "true",
            div {
                class: "flex items-center justify-center gap-3 max-w-7xl mx-auto",
                svg {
                    class: "w-4 h-4 text-white flex-shrink-0",
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
                div {
                    class: "flex flex-wrap items-center justify-center gap-2 text-sm text-white",
                    span { class: "font-bold", "{title}" }
                    span { class: "opacity-95", "{message}" }
                }
            }
        }
    }
}

/// Renders environment marker banners (top + bottom) when mock mode is active.
///
/// This is intentionally reusable for future environment markers beyond mock mode.
#[component]
pub fn DevModeBanner(placement: BannerPlacement) -> Element {
    let mut is_mock_mode = use_signal(|| false);
    let mut checked = use_signal(|| false);

    use_effect(move || {
        if checked() {
            return;
        }

        spawn(async move {
            let window = web_sys::window().expect("no global window");
            let url = format!(
                "{}/commits/eval-queue?_ts={}",
                api_base_url(),
                js_sys::Date::now()
            );

            let mut opts = web_sys::RequestInit::new();
            opts.set_method("GET");

            if let Ok(request) = web_sys::Request::new_with_str_and_init(&url, &opts) {
                let response_promise = window.fetch_with_request(&request);
                let future = wasm_bindgen_futures::JsFuture::from(response_promise);
                if let Ok(response) = future.await {
                    if let Ok(response) = response.dyn_into::<web_sys::Response>() {
                        if response.status() == 200 {
                            if let Ok(text_future) = response.text() {
                                let text_js = wasm_bindgen_futures::JsFuture::from(text_future);
                                if let Ok(text) = text_js.await {
                                    if let Some(text_str) = text.as_string() {
                                        if let Ok(status) =
                                            serde_json::from_str::<EvalQueueModeProbe>(&text_str)
                                        {
                                            is_mock_mode
                                                .set(status.execution_mode.eq_ignore_ascii_case("mock"));
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
    });

    if !is_mock_mode() {
        return rsx! {};
    }

    rsx! {
        EnvironmentBanner {
            placement,
            title: "NON-PRODUCTION ENVIRONMENT",
            message: "Mock/dev mode is active. This is not a production environment."
        }
    }
}
