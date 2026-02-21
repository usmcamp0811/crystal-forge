//! First-run registration view for initial admin user setup.

use dioxus::prelude::*;

use crate::state::app_state::AppState;
use crate::theme;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
    first_name: Option<String>,
    last_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegisterResponse {
    user_id: String,
    username: String,
    email: String,
}

/// First-run registration view.
///
/// Only shown when no users exist in the system. The first registered user
/// automatically becomes an Admin.
#[component]
pub fn RegisterView() -> Element {
    let _app_state = use_context::<Signal<AppState>>();
    let nav = navigator();
    let mut username = use_signal(|| String::new());
    let mut email = use_signal(|| String::new());
    let mut password = use_signal(|| String::new());
    let mut password_confirm = use_signal(|| String::new());
    let mut first_name = use_signal(|| String::new());
    let mut last_name = use_signal(|| String::new());
    let mut error_message = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| false);

    let password_match = password() == password_confirm() || password_confirm().is_empty();
    let form_valid = !username().is_empty()
        && !email().is_empty()
        && !password().is_empty()
        && password().len() >= 8
        && password_match;

    let handle_register = move |_| {
        if !form_valid || is_loading() {
            return;
        }

        spawn(async move {
            is_loading.set(true);
            error_message.set(None);

            let register_payload = RegisterRequest {
                username: username(),
                email: email(),
                password: password(),
                first_name: if first_name().is_empty() {
                    None
                } else {
                    Some(first_name())
                },
                last_name: if last_name().is_empty() {
                    None
                } else {
                    Some(last_name())
                },
            };

            // Use web-sys fetch API directly
            use wasm_bindgen::JsCast;
            use wasm_bindgen::JsValue;
            use wasm_bindgen_futures::JsFuture;

            let window = web_sys::window().expect("no global window");
            let mut opts = web_sys::RequestInit::new();
            opts.set_method("POST");

            let json_body = match serde_json::to_string(&register_payload) {
                Ok(j) => j,
                Err(e) => {
                    error_message.set(Some(format!("Failed to serialize request: {}", e)));
                    is_loading.set(false);
                    return;
                }
            };
            opts.set_body(&JsValue::from_str(&json_body));

            let request =
                match web_sys::Request::new_with_str_and_init("/api/auth/local/register", &opts) {
                    Ok(req) => req,
                    Err(e) => {
                        error_message.set(Some(format!("Failed to create request: {:?}", e)));
                        is_loading.set(false);
                        return;
                    }
                };

            let _ = request.headers().set("Content-Type", "application/json");

            let resp_value = match JsFuture::from(window.fetch_with_request(&request)).await {
                Ok(v) => v,
                Err(e) => {
                    error_message.set(Some(format!("Network error: {:?}", e)));
                    is_loading.set(false);
                    return;
                }
            };

            let resp: web_sys::Response = match resp_value.dyn_into() {
                Ok(r) => r,
                Err(_) => {
                    error_message.set(Some("Invalid response from server".to_string()));
                    is_loading.set(false);
                    return;
                }
            };

            if resp.status() >= 200 && resp.status() < 300 {
                // Registration successful - redirect to login
                error_message.set(Some(
                    "Registration successful. Redirecting to login...".to_string(),
                ));
                is_loading.set(false);
                nav.replace("/login");

                // Fallback hard redirect in case router navigation is not active yet.
                if let Some(window) = web_sys::window() {
                    let _ = window.location().set_href("/login");
                }
            } else {
                // Try to get error message from response
                let status = resp.status();
                let status_text = resp.status_text();

                if let Ok(text_promise) = resp.text() {
                    if let Ok(text_value) = JsFuture::from(text_promise).await {
                        if let Some(text) = text_value.as_string() {
                            if !text.is_empty() {
                                error_message.set(Some(format!(
                                    "Registration failed ({}): {}",
                                    status, text
                                )));
                            } else {
                                error_message.set(Some(format!(
                                    "Registration failed: {} {}",
                                    status, status_text
                                )));
                            }
                        } else {
                            error_message.set(Some(format!(
                                "Registration failed: {} {}",
                                status, status_text
                            )));
                        }
                    } else {
                        error_message.set(Some(format!(
                            "Registration failed: {} {}",
                            status, status_text
                        )));
                    }
                } else {
                    error_message.set(Some(format!(
                        "Registration failed: {} {}",
                        status, status_text
                    )));
                }
                is_loading.set(false);
            }
        });
    };

    rsx! {
        div {
            class: "relative min-h-screen flex items-center justify-center p-6 overflow-hidden",
            style: "background: radial-gradient(circle at 20% 20%, rgba(139,92,246,0.15) 0%, rgba(15,23,42,0) 45%), radial-gradient(circle at 80% 80%, rgba(168,85,247,0.12) 0%, rgba(15,23,42,0) 40%), linear-gradient(135deg, #0b1020 0%, #111827 50%, #1a1a2e 100%);",

            // Faded Crystal Forge logo backdrop (inspired by slide styling)
            div {
                class: "absolute right-[-100px] bottom-[-80px] pointer-events-none select-none",
                style: "opacity: 0.04;",
                img {
                    src: asset!("assets/cf.png"),
                    class: "max-w-[440px] blur-[1px]",
                    alt: ""
                }
            }

            // Soft purple glow accents
            div {
                class: "absolute -top-24 -left-24 w-72 h-72 rounded-full bg-violet-500/10 blur-3xl pointer-events-none"
            }
            div {
                class: "absolute -bottom-24 -right-16 w-80 h-80 rounded-full bg-fuchsia-500/10 blur-3xl pointer-events-none"
            }

            div {
                class: "relative w-full max-w-md z-10",

                // First-run banner
                div {
                    class: "mb-6 p-3 rounded-lg border border-violet-500/40 bg-slate-900/95 backdrop-blur-sm",
                    h3 {
                        class: "text-sm font-semibold text-violet-300 mb-1",
                        "⚡ First-Time Setup"
                    }
                    p {
                        class: "text-xs text-violet-200/90",
                        "Create the initial administrator account. This user will have full system access."
                    }
                }

                // Card container
                div {
                    class: "relative {theme::surface::CARD_BG} border border-violet-400/25 rounded-2xl p-8 shadow-2xl shadow-violet-950/40 backdrop-blur-sm",

                    // Top accent line
                    div {
                        class: "absolute top-0 left-0 right-0 h-[2px] rounded-t-2xl bg-gradient-to-r from-violet-500/0 via-violet-400/80 to-fuchsia-400/0"
                    }

                    // Header with logo (sidebar style)
                    div {
                        class: "flex items-center gap-3 mb-6",
                        img {
                            class: "h-8 w-8",
                            style: "transform: scale(1.67);",
                            src: asset!("assets/crystal-forge-icon.png"),
                            alt: "Crystal Forge"
                        }
                        div {
                            h1 {
                                class: "text-xl font-bold text-white",
                                "Crystal Forge"
                            }
                            p {
                                class: "text-xs {theme::text::MUTED} mt-1",
                                "Administrator Registration"
                            }
                        }
                    }

                    // Registration form
                    form {
                        onsubmit: move |evt| {
                            evt.prevent_default();
                            handle_register(evt);
                        },

                        div {
                            class: "space-y-4 mb-6",

                            // Username
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                    "Username *"
                                }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border border-violet-500/30 {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500 focus:border-violet-400",
                                    placeholder: "admin",
                                    value: "{username}",
                                    oninput: move |evt| username.set(evt.value().clone()),
                                    required: true,
                                }
                            }

                            // Email
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                    "Email *"
                                }
                                input {
                                    r#type: "email",
                                    class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border border-violet-500/30 {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500 focus:border-violet-400",
                                    placeholder: "admin@example.com",
                                    value: "{email}",
                                    oninput: move |evt| email.set(evt.value().clone()),
                                    required: true,
                                }
                            }

                            // First Name
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                    "First Name"
                                }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border border-violet-500/30 {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500 focus:border-violet-400",
                                    placeholder: "Optional",
                                    value: "{first_name}",
                                    oninput: move |evt| first_name.set(evt.value().clone()),
                                }
                            }

                            // Last Name
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                    "Last Name"
                                }
                                input {
                                    r#type: "text",
                                    class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border border-violet-500/30 {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500 focus:border-violet-400",
                                    placeholder: "Optional",
                                    value: "{last_name}",
                                    oninput: move |evt| last_name.set(evt.value().clone()),
                                }
                            }

                            // Password
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                    "Password *"
                                }
                                input {
                                    r#type: "password",
                                    class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border border-violet-500/30 {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500 focus:border-violet-400",
                                    placeholder: "Minimum 8 characters",
                                    value: "{password}",
                                    oninput: move |evt| password.set(evt.value().clone()),
                                    required: true,
                                    minlength: 8,
                                }
                                if !password().is_empty() && password().len() < 8 {
                                    p {
                                        class: "mt-1 text-xs text-amber-400",
                                        "Password must be at least 8 characters"
                                    }
                                }
                            }

                            // Confirm Password
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                    "Confirm Password *"
                                }
                                input {
                                    r#type: "password",
                                    class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border border-violet-500/30 {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500 focus:border-violet-400",
                                    placeholder: "Re-enter password",
                                    value: "{password_confirm}",
                                    oninput: move |evt| password_confirm.set(evt.value().clone()),
                                    required: true,
                                }
                                if !password_confirm().is_empty() && !password_match {
                                    p {
                                        class: "mt-1 text-xs text-red-400",
                                        "Passwords do not match"
                                    }
                                }
                            }
                        }

                        // Error message
                        if let Some(error) = error_message.read().clone() {
                            div {
                                class: "mb-4 p-3 rounded-lg bg-red-500/10 border border-red-500/30",
                                p {
                                    class: "text-sm text-red-300",
                                    "{error}"
                                }
                            }
                        }

                        // Submit button
                        button {
                            r#type: "submit",
                            class: "w-full py-3 px-4 rounded-lg font-semibold text-sm transition-all",
                            class: if form_valid && !is_loading() {
                                "bg-violet-600 hover:bg-violet-500 text-white shadow-lg shadow-violet-900/30"
                            } else {
                                "bg-gray-700 text-gray-400 cursor-not-allowed"
                            },
                            disabled: !form_valid || is_loading(),
                            {
                                if is_loading() {
                                    "Creating Administrator Account..."
                                } else {
                                    "Create Administrator Account"
                                }
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "mt-6 text-center text-xs text-gray-500",
                    "Initial administrator setup • Full system access granted"
                }
            }
        }
    }
}
