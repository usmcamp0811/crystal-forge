//! First-run registration view for initial admin user setup.

use dioxus::prelude::*;

use crate::state::app_state::AppState;
use crate::theme;

#[path = "register_api.rs"]
mod register_api;
use register_api::{RegisterRequest, fetch_setup_status, register_user};

/// First-run registration view.
///
/// Only shown when:
/// - No users exist (first-time setup), OR
/// - Registration is explicitly enabled in server config
///
/// The first registered user automatically becomes an Admin.
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
    let mut registration_allowed = use_signal(|| false);
    let mut is_first_run = use_signal(|| false);
    let mut status_checked = use_signal(|| false);

    // Check if UI screenshot test mode is enabled (debug builds only)
    #[cfg(debug_assertions)]
    let ui_check_mode = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("ui_check_auth=1"))
        .unwrap_or(false);
    #[cfg(not(debug_assertions))]
    let ui_check_mode = false;

    // Check if registration is allowed on mount
    use_effect(move || {
        spawn(async move {
            if let Some(status) = fetch_setup_status(ui_check_mode).await {
                is_first_run.set(status.requires_setup);
                registration_allowed.set(status.requires_setup || status.allow_registration);
                status_checked.set(true);

                if !status.requires_setup && !status.allow_registration {
                    nav.push("/login");
                }
                return;
            }

            status_checked.set(true);
            nav.push("/login");
        });
    });

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

            let payload = RegisterRequest {
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

            if let Err(err) = register_user(&payload).await {
                error_message.set(Some(err));
                is_loading.set(false);
                return;
            }

            // Registration successful - send user into setup flow.
            error_message.set(Some(
                "Registration successful. Redirecting to setup...".to_string(),
            ));
            is_loading.set(false);
            nav.replace("/setup");

            // Fallback hard redirect in case router navigation is not active yet.
            if let Some(window) = web_sys::window() {
                let _ = window.location().set_href("/setup");
            }
        });
    };

    // Show loading state while checking registration status
    if !status_checked() {
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center cf-auth-bg-base",
                p {
                    class: "text-gray-400",
                    "Loading..."
                }
            }
        };
    }

    // If registration not allowed, this shouldn't render (redirect happens in effect)
    // but just in case, show nothing
    if !registration_allowed() {
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center cf-auth-bg-base",
                p {
                    class: "text-gray-400",
                    "Redirecting..."
                }
            }
        };
    }

    rsx! {
        div {
            class: "relative min-h-screen flex items-center justify-center p-6 overflow-hidden cf-auth-bg-ambient",

            // Faded Crystal Forge logo backdrop (inspired by slide styling)
            div {
                class: "absolute pointer-events-none select-none",
                style: "opacity: 0.06; right: 16px; bottom: 16px;",
                img {
                    src: asset!("assets/cf.png"),
                    style: "max-width: 400px; filter: blur(1px);",
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
                class: "relative max-w-md z-10",

                // First-run banner (only shown for initial admin setup)
                if is_first_run() {
                    div {
                        class: "mb-6 p-4 rounded-lg border border-violet-400/50 cf-first-run-banner",
                        h3 {
                            class: "text-sm font-semibold text-white mb-1",
                            "⚡ First-Time Setup"
                        }
                        p {
                            class: "text-xs text-white/90",
                            "Create the initial administrator account. This user will have full system access."
                        }
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
                            class: "h-8 w-8 cf-logo-scale",
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
                                if is_first_run() { "Administrator Registration" } else { "Create Account" }
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
                                    if is_first_run() { "Creating Administrator Account..." } else { "Creating Account..." }
                                } else {
                                    if is_first_run() { "Create Administrator Account" } else { "Create Account" }
                                }
                            }
                        }
                    }

                    // Back to login link (for non-first-run registration)
                    if !is_first_run() {
                        div {
                            class: "mt-4 text-center",
                            span {
                                class: "text-sm text-gray-400",
                                "Already have an account? "
                            }
                            a {
                                href: "/login",
                                class: "text-sm text-violet-400 hover:text-violet-300 transition-colors",
                                "Sign in"
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "mt-6 text-center text-xs text-gray-500",
                    if is_first_run() {
                        "Initial administrator setup • Full system access granted"
                    } else {
                        "New account registration"
                    }
                }
            }
        }
    }
}
