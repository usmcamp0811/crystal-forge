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
    let nav = navigator();

    // Fetch auth mode on mount and check if already authenticated
    use_effect(move || {
        spawn(async move {
            if let Ok(context) = fetch_whoami().await {
                auth_mode.set(Some(context.auth_mode));
                
                // If already authenticated, update app state and redirect to dashboard
                if context.is_authenticated {
                    app_state.write().auth = Some(context);
                    nav.push("/");
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
            class: "min-h-screen flex items-center justify-center p-6",
            style: "background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%);",

            div {
                class: "w-full max-w-md",

                // Card container
                div {
                    class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-2xl p-8 shadow-2xl",

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
                                    onsubmit: handle_login,
                                    prevent_default: "onsubmit",
                                    
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
