//! Info row display components.
//!
//! Provides reusable row components for displaying labeled information
//! in system detail views and info cards.

use dioxus::prelude::*;

use crate::theme;

/// Standard info row with label and value.
#[component]
pub fn InfoRow(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm text-gray-200", "{value}" }
        }
    }
}

/// Info row with monospace value (for paths, hashes, IPs, etc.).
#[component]
pub fn InfoRowMono(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm text-gray-200 font-mono", "{value}" }
        }
    }
}

/// Boolean row with enabled/disabled status indicator.
#[component]
pub fn BooleanRow(label: &'static str, value: bool) -> Element {
    let (icon, color, text) = if value {
        ("✓", "text-emerald-400", "Enabled")
    } else {
        ("✗", "text-gray-500", "Disabled")
    };
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm font-medium {color}", "{icon} {text}" }
        }
    }
}

/// Status badge for displaying status labels with color coding.
#[component]
pub fn StatusBadge(
    label: &'static str,
    color_class: &'static str,
    bg_class: &'static str,
) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium {color_class} {bg_class}",
            "{label}"
        }
    }
}
