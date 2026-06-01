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

/// Get environment color matching JSX data.js ENVIRONMENTS
fn env_color(env: &str) -> &'static str {
    let normalized = env.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "production" => "#dc2626",
        "prod" => "#dc2626",
        "staging" => "#d97706",
        "dev" => "#2563eb",
        "development" => "#2563eb",
        "edge" => "#0f766e",
        "lab" => "#7c3aed",
        _ => "#6b7280", // unknown/default gray
    }
}

/// Environment badge with auto-color or custom styling.
///
/// Supports two modes:
/// 1. Auto-color: Pass only `name` and colors are derived from env name
/// 2. Custom: Pass `name`, `fg`, `bg`, `border` for full control
///
/// # Example (auto-color)
/// ```
/// rsx! {
///     EnvBadge {
///         name: "production".to_string(),
///     }
/// }
/// ```
///
/// # Example (custom)
/// ```
/// rsx! {
///     EnvBadge {
///         name: "production".to_string(),
///         fg: "#f87171".to_string(),
///         bg: "rgba(220,38,38,0.10)".to_string(),
///         border: "rgba(248,113,113,0.25)".to_string(),
///     }
/// }
/// ```
#[component]
pub fn EnvBadge(
    /// The environment name to display
    name: String,
    /// Optional foreground color (auto-derived if not provided)
    #[props(default)]
    fg: Option<String>,
    /// Optional background color (auto-derived if not provided)
    #[props(default)]
    bg: Option<String>,
    /// Optional border color (auto-derived if not provided)
    #[props(default)]
    border: Option<String>,
) -> Element {
    let (fg_color, bg_color, border_color) =
        if let (Some(fg), Some(bg), Some(border)) = (&fg, &bg, &border) {
            // Custom colors provided
            (fg.clone(), bg.clone(), border.clone())
        } else {
            // Auto-derive colors from env name
            let color = env_color(&name);
            (
                color.to_string(),
                format!("color-mix(in oklab, {} 14%, var(--cf-card-bg))", color),
                color.to_string(),
            )
        };

    rsx! {
        span {
            style: "
                padding: 2px 6px;
                border-radius: 99px;
                font-size: 10px;
                border: 1px solid {border_color};
                background: {bg_color};
                color: {fg_color};
                display: inline-flex;
                align-items: center;
                gap: 4px;
                font-family: inherit;
            ",
            span {
                style: "width: 4px; height: 4px; border-radius: 50%; background: {fg_color};"
            }
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
