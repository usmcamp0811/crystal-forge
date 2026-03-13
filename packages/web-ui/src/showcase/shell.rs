use dioxus::prelude::*;

use crate::theme;

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
