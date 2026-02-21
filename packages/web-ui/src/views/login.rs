//! Unified login view that adapts based on server auth mode.

use dioxus::prelude::*;

use crate::api::client::{fetch_whoami, local_login};
use crate::api::models::AuthMode;
use crate::state::app_state::AppState;
use crate::theme;

/// Unified login view.
///
/// Determines the auth mode from the server and presents the appropriate
/// login interface (OIDC redirect, local username/password, or dev selector).
#[component]
pub fn LoginView() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let mut username = use_signal(|| String::new());
    let mut password = use_signal(|| String::new());
    let mut error_message = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| false);
    let mut auth_mode = use_signal(|| None::<AuthMode>);
    let mut requires_setup = use_signal(|| false);
    let nav = navigator();

    // Fetch auth mode and setup status on mount
    use_effect(move || {
        spawn(async move {
            // Check auth status
            if let Ok(context) = fetch_whoami().await {
                auth_mode.set(Some(context.auth_mode));
                
                // If already authenticated, update app state and redirect to dashboard
                if context.is_authenticated {
                    app_state.write().auth = Some(context);
                    nav.push("/");
                    return;
                }
                
                // If local auth mode, check if initial setup is required
                if context.auth_mode == AuthMode::Local {
                    use wasm_bindgen::JsCast;
                    use wasm_bindgen_futures::JsFuture;
                    
                    let window = web_sys::window().expect("no global window");
                    let mut opts = web_sys::RequestInit::new();
                    opts.set_method("GET");
                    
                    if let Ok(request) = web_sys::Request::new_with_str_and_init("/api/auth/setup-status", &opts) {
                        if let Ok(resp_value) = JsFuture::from(window.fetch_with_request(&request)).await {
                            if let Ok(resp) = resp_value.dyn_into::<web_sys::Response>() {
                                if resp.ok() {
                                    if let Ok(text_promise) = resp.text() {
                                        if let Ok(text_value) = JsFuture::from(text_promise).await {
                                            if let Some(text) = text_value.as_string() {
                                                if let Ok(status) = serde_json::from_str::<serde_json::Value>(&text) {
                                                    if status.get("requires_setup").and_then(|v| v.as_bool()).unwrap_or(false) {
                                                        // Redirect to registration for first-time setup
                                                        nav.push("/register");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
    });

    let handle_login = move |_| {
        spawn(async move {
            is_loading.set(true);
            error_message.set(None);

            match local_login(&username.read(), &password.read()).await {
                Ok(_response) => {
                    // Refresh auth context
                    if let Ok(auth_context) = fetch_whoami().await {
                        app_state.write().auth = Some(auth_context);
                    }
                    // Redirect to dashboard
                    nav.push("/");
                }
                Err(e) => {
                    error_message.set(Some(format!("Login failed: {}", e)));
                    is_loading.set(false);
                }
            }
        });
    };

    rsx! {
        div {
            class: "relative min-h-screen flex items-center justify-center p-6 overflow-hidden",
            style: "background: radial-gradient(circle at 20% 20%, rgba(139,92,246,0.15) 0%, rgba(15,23,42,0) 45%), radial-gradient(circle at 80% 80%, rgba(168,85,247,0.12) 0%, rgba(15,23,42,0) 40%), linear-gradient(135deg, #0b1020 0%, #111827 50%, #1a1a2e 100%);",

            // Faded Crystal Forge logo backdrop (inspired by slide styling)
            div {
                class: "absolute right-[-60px] bottom-[-40px] opacity-10 pointer-events-none select-none",
                img {
                    src: "/assets/cf.png",
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

                // Card container
                div {
                    class: "relative {theme::surface::CARD_BG} border border-violet-400/25 rounded-2xl p-8 shadow-2xl shadow-violet-950/40 backdrop-blur-sm",

                    // Top accent line
                    div {
                        class: "absolute top-0 left-0 right-0 h-[2px] rounded-t-2xl bg-gradient-to-r from-violet-500/0 via-violet-400/80 to-fuchsia-400/0"
                    }

                    // Header
                    div {
                        class: "text-center mb-8",
                        h1 {
                            class: "text-2xl font-bold text-white mb-2",
                            "Crystal Forge"
                        }
                        p {
                            class: "text-sm {theme::text::SECONDARY}",
                            "Sign in to continue"
                        }
                    }

                    // Show different login interfaces based on auth mode
                    match auth_mode() {
                        Some(AuthMode::Dev) => rsx! {
                            div {
                                class: "text-center",
                                p {
                                    class: "text-sm {theme::text::SECONDARY} mb-4",
                                    "This server is in development mode."
                                }
                                a {
                                    href: "/dev/login",
                                    class: "inline-block px-6 py-3 rounded-lg font-semibold text-sm bg-violet-600 hover:bg-violet-500 text-white shadow-lg shadow-violet-900/30 transition-all",
                                    "Go to Dev Login"
                                }
                            }
                        },
                        Some(AuthMode::Oidc) => rsx! {
                            div {
                                class: "text-center",
                                p {
                                    class: "text-sm {theme::text::SECONDARY} mb-4",
                                    "This server uses OIDC authentication."
                                }
                                a {
                                    href: "/api/auth/oidc/login",
                                    class: "inline-block px-6 py-3 rounded-lg font-semibold text-sm bg-violet-600 hover:bg-violet-500 text-white shadow-lg shadow-violet-900/30 transition-all",
                                    "Sign in with OIDC"
                                }
                            }
                        },
                        Some(AuthMode::Local) | None => rsx! {
                            div {
                                // Local login form
                                form {
                                    onsubmit: move |evt| {
                                        evt.prevent_default();
                                        handle_login(evt);
                                    },
                                    
                                    div {
                                        class: "space-y-4 mb-6",
                                        
                                        // Username field
                                        div {
                                            label {
                                                class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                                "Username"
                                            }
                                            input {
                                                r#type: "text",
                                                class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500",
                                                placeholder: "Enter your username",
                                                value: "{username}",
                                                oninput: move |evt| username.set(evt.value().clone()),
                                            }
                                        }
                                        
                                        // Password field
                                        div {
                                            label {
                                                class: "block text-sm font-medium {theme::text::PRIMARY} mb-2",
                                                "Password"
                                            }
                                            input {
                                                r#type: "password",
                                                class: "w-full px-4 py-2 rounded-lg {theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} {theme::text::PRIMARY} focus:outline-none focus:ring-2 focus:ring-violet-500",
                                                placeholder: "Enter your password",
                                                value: "{password}",
                                                oninput: move |evt| password.set(evt.value().clone()),
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
                                        class: if !username.read().is_empty() && !password.read().is_empty() && !is_loading() {
                                            "bg-violet-600 hover:bg-violet-500 text-white shadow-lg shadow-violet-900/30"
                                        } else {
                                            "bg-gray-700 text-gray-400 cursor-not-allowed"
                                        },
                                        disabled: username.read().is_empty() || password.read().is_empty() || is_loading(),
                                        {
                                            if is_loading() {
                                                "Signing in..."
                                            } else {
                                                "Sign In"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
