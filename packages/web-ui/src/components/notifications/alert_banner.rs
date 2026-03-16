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
        container_style,
        icon_style,
        text_style,
        action_style,
    ) = match severity {
        AlertSeverity::Warning => (
            "",
            "",
            "",
            "",
            "",
            "hover:bg-amber-300/14",
            "",
            "!",
            "border-color: rgba(245, 158, 11, 0.7); background: linear-gradient(135deg, rgba(120, 53, 15, 0.92), rgba(146, 64, 14, 0.78) 55%, rgba(113, 63, 18, 0.7)); box-shadow: inset 0 1px 0 rgba(253, 230, 138, 0.14), 0 0 0 1px rgba(245, 158, 11, 0.08);",
            "background: rgba(245, 158, 11, 0.18); color: rgb(254, 243, 199); border: 1px solid rgba(252, 211, 77, 0.22);",
            "color: rgb(255, 251, 235);",
            "color: rgb(253, 230, 138);",
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
            "",
            "",
            "",
            "",
        ),
    };

    rsx! {
        div {
            class: "flex items-start gap-3 rounded-xl border px-4 py-3 {border_class} {bg_class}",
            style: "{container_style}",
            role: "alert",

            // Severity icon
            div {
                class: "shrink-0 rounded-full {icon_class} {icon_bg_class}",
                style: "width: 1.5rem; min-width: 1.5rem; height: 1.5rem; margin-top: 0.125rem; display: flex; align-items: center; justify-content: center; font-size: 0.875rem; line-height: 1; font-weight: 700; {icon_style}",
            "{icon_glyph}"
            }

            // Message and action
            div {
                class: "min-w-0 flex-1",
                style: "display: flex; flex-direction: column; gap: 0.35rem;",
                p {
                    class: "text-sm leading-6 {text_class}",
                    style: "margin: 0; {text_style}",
                    "{message}"
                }
                if let (Some(label), Some(url)) = (action_label, action_url) {
                    a {
                        class: "text-sm font-medium underline underline-offset-2 transition-opacity hover:opacity-80 {action_class}",
                        style: "{action_style}",
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
