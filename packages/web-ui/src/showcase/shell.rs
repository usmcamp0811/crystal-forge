//! Reusable showcase shell components for consistent component isolation demos.
//!
//! These components provide structured patterns for displaying:
//! - Sectioned showcase areas with titles and descriptions
//! - State matrices for displaying multiple component states side-by-side
//! - Responsive preview wrappers for testing breakpoints
//! - Layout helpers for organizing showcase content

use dioxus::prelude::*;

use crate::theme;

/// Standard breakpoint widths for responsive preview
pub const MOBILE_WIDTH: &str = "max-w-[375px]";
pub const TABLET_WIDTH: &str = "max-w-[768px]";
pub const DESKTOP_WIDTH: &str = "max-w-[1024px]";
pub const WIDE_WIDTH: &str = "max-w-[1440px]";

#[component]
pub fn ShowcaseSection(
    title: &'static str,
    description: &'static str,
    children: Element,
) -> Element {
    rsx! {
        section { class: "mb-10",
            div { class: "mb-4",
                h2 { class: "{theme::typography::SECTION_TITLE}", "{title}" }
                p { class: "text-sm {theme::text::MUTED} mt-1", "{description}" }
            }
            div { class: "space-y-4", {children} }
        }
    }
}

#[component]
pub fn StateMatrix(title: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl p-4",
            h3 { class: "text-sm font-semibold {theme::text::SECONDARY} mb-3", "{title}" }
            div { class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-5 gap-3", {children} }
        }
    }
}

#[component]
pub fn StateTile(label: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "rounded-lg border {theme::surface::CARD_BORDER} p-3 bg-gray-900/30",
            p { class: "text-xs uppercase tracking-wide {theme::text::MUTED} mb-2", "{label}" }
            {children}
        }
    }
}

#[component]
pub fn ResponsivePreview(
    label: &'static str,
    width_class: &'static str,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "rounded-xl border {theme::surface::CARD_BORDER} p-4 bg-gray-900/20",
            p { class: "text-xs uppercase tracking-wide {theme::text::MUTED} mb-2", "{label}" }
            div { class: "mx-auto {width_class}", {children} }
        }
    }
}

/// Grid layout for responsive preview comparison across breakpoints
#[component]
pub fn ResponsiveGrid(children: Element) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 lg:grid-cols-2 gap-4", {children} }
    }
}

/// Wrapper for grouping related component variants
#[component]
pub fn VariantGroup(title: &'static str, children: Element) -> Element {
    rsx! {
        div { class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl p-4",
            h3 { class: "text-sm font-semibold {theme::text::SECONDARY} mb-3", "{title}" }
            div { class: "space-y-3", {children} }
        }
    }
}

/// Helper for displaying component props/documentation inline
#[component]
pub fn PropDoc(name: &'static str, prop_type: &'static str, description: &'static str) -> Element {
    rsx! {
        div { class: "text-xs space-y-1",
            div { class: "flex gap-2 items-baseline",
                code { class: "font-mono {theme::text::SECONDARY}", "{name}" }
                span { class: "{theme::text::MUTED}", ": {prop_type}" }
            }
            p { class: "{theme::text::MUTED} pl-2", "{description}" }
        }
    }
}
