//! Development mode warning banner.

use dioxus::prelude::*;

/// Banner warning that dev authentication mode is active.
///
/// This banner should be displayed at the top of all authenticated pages
/// when AUTH_MODE=dev is enabled on the server.
///
/// # Current Limitation
///
/// This banner is currently rendered unconditionally in the AppShell.
/// This means it will appear even when AUTH_MODE=oidc/production, which is incorrect.
///
/// TODO: Gate rendering behind a reliable dev-mode signal:
/// - Option 1: Add auth_mode field to /api/v1/status endpoint
/// - Option 2: Check if /api/auth/dev/login returns 200 vs 404
/// - Option 3: Add compile-time flag (requires build-time coordination)
///
/// For now, this limitation is documented in TASK-65.0 notes.
#[component]
pub fn DevModeBanner() -> Element {
    // TODO: Add runtime check for AUTH_MODE=dev before rendering
    // For now, banner displays unconditionally (see limitation note above)
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
