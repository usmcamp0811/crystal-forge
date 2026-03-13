//! AlertBanner component.
//!
//! Displays an inline or top-of-page warning/info banner with an optional
//! action link and optional dismiss handler.

use dioxus::prelude::*;

/// Severity level for an [`AlertBanner`].
#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeverity {
    Warning,
    Info,
}

/// Inline or page-level alert banner.
///
/// # Props
/// - `severity` — visual style: [`AlertSeverity::Warning`] (amber) or [`AlertSeverity::Info`] (blue).
/// - `message` — the text to display.
/// - `action_label` — optional label for the action link (e.g. "Add a flake").
/// - `action_url` — optional URL the action link points to (Dioxus router path).
/// - `on_dismiss` — optional dismiss handler; when `None`, no dismiss button is rendered.
#[component]
pub fn AlertBanner(
    severity: AlertSeverity,
    message: String,
    #[props(default)] action_label: Option<String>,
    #[props(default)] action_url: Option<String>,
    #[props(default)] on_dismiss: Option<EventHandler<()>>,
) -> Element {
    let (border_class, bg_class, icon_class, text_class, icon_path) = match severity {
        AlertSeverity::Warning => (
            "border-amber-500/40",
            "bg-amber-900/20",
            "text-amber-400",
            "text-amber-200",
            // Warning triangle
            "M12 9v4m0 4h.01M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z",
        ),
        AlertSeverity::Info => (
            "border-blue-500/40",
            "bg-blue-900/20",
            "text-blue-400",
            "text-blue-200",
            // Info circle
            "M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z",
        ),
    };

    rsx! {
        div {
            class: "flex items-start gap-3 px-4 py-3 rounded-lg border {border_class} {bg_class}",

            // Severity icon
            svg {
                class: "w-5 h-5 mt-0.5 shrink-0 {icon_class}",
                fill: "none",
                stroke: "currentColor",
                view_box: "0 0 24 24",
                path {
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    stroke_width: "2",
                    d: "{icon_path}",
                }
            }

            // Message and action
            div {
                class: "flex-1 min-w-0",
                span {
                    class: "text-sm {text_class}",
                    "{message}"
                }
                if let (Some(label), Some(url)) = (action_label, action_url) {
                    span { class: "text-sm {text_class}", " " }
                    a {
                        class: "text-sm font-medium underline {icon_class} hover:opacity-80 transition-opacity",
                        href: "{url}",
                        "{label}"
                    }
                }
            }

            // Optional dismiss button
            if let Some(handler) = on_dismiss {
                button {
                    class: "shrink-0 p-1 rounded hover:bg-white/10 transition-colors",
                    onclick: move |_| handler.call(()),
                    svg {
                        class: "w-4 h-4 text-gray-400",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M6 18L18 6M6 6l12 12",
                        }
                    }
                }
            }
        }
    }
}
