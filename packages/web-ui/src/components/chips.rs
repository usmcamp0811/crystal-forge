//! Chip components for status indicators and tags.
//!
//! Implements the Crystal Forge design system chip patterns with semantic colors
//! and consistent styling across health, deployment, and CVE status indicators.

use dioxus::prelude::*;

/// Chip color variant based on semantic meaning.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChipVariant {
    Healthy,
    Warning,
    Critical,
    Unknown,
    Info,
}

impl ChipVariant {
    pub fn class(&self) -> &'static str {
        match self {
            ChipVariant::Healthy => "chip-healthy",
            ChipVariant::Warning => "chip-warning",
            ChipVariant::Critical => "chip-critical",
            ChipVariant::Unknown => "chip-unknown",
            ChipVariant::Info => "chip-info",
        }
    }
}

/// A chip component with optional dot indicator.
///
/// # Example
/// ```
/// rsx! {
///     Chip {
///         variant: ChipVariant::Healthy,
///         show_dot: true,
///         "Healthy"
///     }
/// }
/// ```
#[component]
pub fn Chip(
    /// The color variant for the chip
    variant: ChipVariant,
    /// Whether to show a dot indicator
    #[props(default = false)]
    show_dot: bool,
    /// The content to display
    children: Element,
) -> Element {
    rsx! {
        span {
            class: "chip {variant.class()}",
            if show_dot {
                span { class: "chip-dot" }
            }
            {children}
        }
    }
}

/// Environment badge with custom color styling.
///
/// # Example
/// ```
/// rsx! {
///     EnvBadge {
///         name: "production",
///         fg: "#f87171",
///         bg: "rgba(220,38,38,0.10)",
///         border: "rgba(248,113,113,0.25)",
///     }
/// }
/// ```
#[component]
pub fn EnvBadge(
    /// The environment name to display
    name: String,
    /// Foreground color (CSS color value)
    fg: String,
    /// Background color (CSS color value)  
    bg: String,
    /// Border color (CSS color value)
    border: String,
) -> Element {
    let style = format!("--env-fg: {}; --env-bg: {}; --env-border: {}", fg, bg, border);

    rsx! {
        span {
            class: "env-badge",
            style: "{style}",
            span { class: "chip-dot" }
            "{name}"
        }
    }
}

/// Status dot component for health indicators.
///
/// # Example
/// ```
/// rsx! {
///     StatusDot {
///         color: "#34d399",
///         large: false,
///     }
/// }
/// ```
#[component]
pub fn StatusDot(
    /// The status color (CSS color value)
    color: String,
    /// Whether to render a large variant
    #[props(default = false)]
    large: bool,
) -> Element {
    let style = format!("--status-color: {}", color);
    let size_class = if large { "lg" } else { "" };

    rsx! {
        span {
            class: "status-dot {size_class}",
            style: "{style}",
        }
    }
}
