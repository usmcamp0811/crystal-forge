//! Asset injection bootstrap for Crystal Forge Web UI.
//!
//! Handles injecting CSS stylesheets and JavaScript scripts into the document.

use dioxus::prelude::*;

/// Inject all required assets into the document.
///
/// This includes:
/// - Tailwind CSS (vendored, works offline)
/// - App-specific theme variables and shared utility classes
/// - Favicon
/// - Highlight.js for code syntax highlighting
pub fn inject_assets() -> dioxus::prelude::Element {
    rsx! {
        // Load vendored Tailwind CSS (works offline).
        document::Stylesheet { href: asset!("assets/tailwind.min.css") }
        // App-specific theme variables and shared utility classes.
        document::Stylesheet { href: asset!("assets/app.css") }
        document::Link {
            rel: "icon",
            r#type: "image/png",
            href: asset!("assets/crystal-forge-icon.png")
        }
        document::Stylesheet {
            href: "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css"
        }
        document::Script {
            src: "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js",
        }
    }
}
