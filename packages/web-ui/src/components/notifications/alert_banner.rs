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
    let (
        border_class,
        bg_class,
        icon_class,
        text_class,
        action_class,
        dismiss_hover_class,
        icon_bg_class,
        icon_glyph,
    ) = match severity {
        AlertSeverity::Warning => (
            "border-amber-400/50 shadow-[0_0_0_1px_rgba(251,191,36,0.05)]",
            "bg-gradient-to-r from-amber-950/80 via-amber-900/45 to-amber-950/30",
            "text-amber-300",
            "text-amber-100",
            "text-amber-200",
            "hover:bg-amber-400/10",
            "bg-amber-300/15 border border-amber-300/10",
            "!",
        ),
        AlertSeverity::Info => (
            "border-blue-500/40",
            "bg-blue-900/20",
            "text-blue-400",
            "text-blue-200",
            "text-blue-300",
            "hover:bg-blue-500/10",
            "bg-blue-400/15",
            "i",
        ),
    };

    rsx! {
        div {
            class: "flex items-start gap-3 rounded-xl border px-4 py-3 {border_class} {bg_class}",
            role: "alert",

            // Severity icon
            div {
                class: "shrink-0 rounded-full {icon_class} {icon_bg_class}",
                style: "width: 1.5rem; min-width: 1.5rem; height: 1.5rem; margin-top: 0.125rem; display: flex; align-items: center; justify-content: center; font-size: 0.875rem; line-height: 1; font-weight: 700;",
            "{icon_glyph}"
            }

            // Message and action
            div {
                class: "min-w-0 flex-1",
                style: "display: flex; flex-direction: column; gap: 0.35rem;",
                p {
                    class: "text-sm leading-6 {text_class}",
                    style: "margin: 0;",
                    "{message}"
                }
                if let (Some(label), Some(url)) = (action_label, action_url) {
                    a {
                        class: "text-sm font-medium underline underline-offset-2 transition-opacity hover:opacity-80 {action_class}",
                        href: "{url}",
                        "{label}"
                    }
                }
            }

            // Optional dismiss button
            if let Some(handler) = on_dismiss {
                button {
                    class: "shrink-0 rounded p-1 transition-colors {dismiss_hover_class}",
                    onclick: move |_| handler.call(()),
                    aria_label: "Dismiss alert",
                    svg {
                        class: "block h-4 w-4 text-gray-400",
                        width: "16",
                        height: "16",
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
