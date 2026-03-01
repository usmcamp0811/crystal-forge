//! Development mode login selector.

use dioxus::prelude::*;

use crate::api::client::dev_login;
use crate::theme;

const DEV_ADMIN_EMAIL: &str = "dev-admin@crystal-forge.local";
const DEV_OPERATOR_EMAIL: &str = "dev-operator@crystal-forge.local";
const DEV_VIEWER_EMAIL: &str = "dev-viewer@crystal-forge.local";

#[derive(Debug, Clone)]
struct DevRole {
    email: &'static str,
    display_name: &'static str,
    description: &'static str,
    color: &'static str,
}

const DEV_ROLES: &[DevRole] = &[
    DevRole {
        email: DEV_ADMIN_EMAIL,
        display_name: "Admin",
        description: "Full system access",
        color: "bg-red-500/20 border-red-500/40 text-red-300",
    },
    DevRole {
        email: DEV_OPERATOR_EMAIL,
        display_name: "Operator",
        description: "Deployment and monitoring",
        color: "bg-yellow-500/20 border-yellow-500/40 text-yellow-300",
    },
    DevRole {
        email: DEV_VIEWER_EMAIL,
        display_name: "Viewer",
        description: "Read-only access",
        color: "bg-blue-500/20 border-blue-500/40 text-blue-300",
    },
];

/// Development mode login selector view.
#[component]
pub fn DevLoginView() -> Element {
    let mut selected_role = use_signal(|| None::<String>);
    let mut error_message = use_signal(|| None::<String>);
    let mut is_loading = use_signal(|| false);
    let nav = navigator();

    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center p-6 cf-dev-login-bg",

            div {
                class: "w-full max-w-md",

                // Warning banner
                div {
                    class: "mb-6 p-4 rounded-lg border-2 border-amber-500/50 bg-amber-500/10",
                    div {
                        class: "flex items-start gap-3",
                        svg {
                            class: "w-4 h-4 text-amber-400 flex-shrink-0 mt-0.5",
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
                            h3 {
                                class: "text-sm font-semibold text-amber-300 mb-1",
                                "Development Mode"
                            }
                            p {
                                class: "text-xs text-amber-200/80",
                                "This login screen is for local development only. Never use AUTH_MODE=dev in production."
                            }
                        }
                    }
                }

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
                            "Select a development role to continue"
                        }
                    }

                    // Role selection
                    div {
                        class: "space-y-3 mb-6",
                        for role in DEV_ROLES {
                            div {
                                class: "relative",
                                button {
                                    class: if selected_role.read().as_ref() == Some(&role.email.to_string()) {
                                        format!("w-full p-4 rounded-lg border-2 transition-all text-left {} ring-2 ring-offset-2 ring-offset-gray-900", role.color)
                                    } else {
                                        format!("w-full p-4 rounded-lg border-2 transition-all text-left {} hover:scale-[1.02]", role.color)
                                    },
                                    onclick: move |_| selected_role.set(Some(role.email.to_string())),
                                    div {
                                        class: "flex items-center justify-between",
                                        div {
                                            h3 {
                                                class: "font-semibold text-base mb-1",
                                                "{role.display_name}"
                                            }
                                            p {
                                                class: "text-xs opacity-80",
                                                "{role.description}"
                                            }
                                        }
                                        if selected_role.read().as_ref() == Some(&role.email.to_string()) {
                                            svg {
                                                class: "w-4 h-4",
                                                fill: "currentColor",
                                                view_box: "0 0 20 20",
                                                path {
                                                    fill_rule: "evenodd",
                                                    d: "M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z",
                                                    clip_rule: "evenodd"
                                                }
                                            }
                                        }
                                    }
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

                    // Continue button
                    button {
                        class: "w-full py-3 px-4 rounded-lg font-semibold text-sm transition-all",
                        class: if selected_role.read().is_some() && !is_loading() {
                            "bg-violet-600 hover:bg-violet-500 text-white shadow-lg shadow-violet-900/30"
                        } else {
                            "bg-gray-700 text-gray-400 cursor-not-allowed"
                        },
                        disabled: selected_role.read().is_none() || is_loading(),
                        onclick: move |_| {
                            if let Some(email) = selected_role.read().clone() {
                                spawn(async move {
                                    is_loading.set(true);
                                    error_message.set(None);

                                    match dev_login(&email).await {
                                        Ok(_response) => {
                                            // TODO: Store session/user info in context or local storage
                                            // Redirect to dashboard (root route)
                                            nav.push("/");
                                        }
                                        Err(e) => {
                                            error_message.set(Some(format!("Login failed: {}", e)));
                                            is_loading.set(false);
                                        }
                                    }
                                });
                            }
                        },
                        {
                            if is_loading() {
                                "Logging in..."
                            } else {
                                "Continue"
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "mt-6 text-center text-xs text-gray-500",
                    "Development authentication mode • Not for production use"
                }
            }
        }
    }
}
