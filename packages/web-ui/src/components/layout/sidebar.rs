//! Sidebar navigation component.

use dioxus::prelude::*;

use crate::routes::Route;
use crate::theme;

/// Sidebar navigation for primary routes.
#[component]
pub fn SidebarNav() -> Element {
    rsx! {
        nav {
            class: "hidden lg:flex w-64 {theme::surface::SIDEBAR_BG} border-r {theme::surface::CARD_BORDER} flex-col",
            div {
                class: "p-6 flex items-center gap-3",
                img {
                    class: "h-8 w-8",
                    style: "transform: scale(1.67);",
                    src: asset!("assets/crystal-forge-icon.png"),
                    alt: "Crystal Forge"
                }
                div {
                    h1 {
                        class: "text-xl font-bold text-white",
                        "Crystal Forge"
                    }
                    p {
                        class: "text-xs {theme::text::MUTED} mt-1",
                        "Fleet Management"
                    }
                }
            }
            div {
                class: "flex-1 px-3 space-y-1",
                NavLink {
                    to: Route::DashboardView {},
                    label: "Dashboard",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M3 11l9-7 9 7" }
                            path { d: "M5 10v10h5v-6h4v6h5V10" }
                        }
                    )
                }
                NavLink {
                    to: Route::SystemsView {},
                    label: "Systems",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "3", y: "5", width: "18", height: "12", rx: "2" }
                            path { d: "M7 21h10" }
                            path { d: "M12 17v4" }
                        }
                    )
                }
                NavLink {
                    to: Route::FlakesView {},
                    label: "Flakes",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M7 4a2 2 0 1 0 4 0a2 2 0 0 0-4 0" }
                            path { d: "M13 7h4a2 2 0 1 0 0-4" }
                            path { d: "M9 6v9a3 3 0 1 0 2 0V10h6a3 3 0 1 0 2 0" }
                        }
                    )
                }
                NavLink {
                    to: Route::BuildsView {},
                    label: "Builds",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "4", y: "4", width: "7", height: "7", rx: "1" }
                            rect { x: "13", y: "4", width: "7", height: "7", rx: "1" }
                            rect { x: "4", y: "13", width: "7", height: "7", rx: "1" }
                            rect { x: "13", y: "13", width: "7", height: "7", rx: "1" }
                        }
                    )
                }
                NavLink {
                    to: Route::CvesView {},
                    label: "CVEs",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                        }
                    )
                }
                NavLink {
                    to: Route::StyleGuideView {},
                    label: "Style Guide",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M4 6h16" }
                            path { d: "M4 12h16" }
                            path { d: "M4 18h16" }
                        }
                    )
                }
            }
            div {
                class: "p-4 border-t {theme::surface::CARD_BORDER} text-xs {theme::text::MUTED}",
                "v0.1.0"
            }
        }
    }
}

/// A sidebar navigation link.
#[component]
fn NavLink(to: Route, label: &'static str, icon: Element) -> Element {
    // TODO: highlight active route
    rsx! {
        Link {
            to,
            class: "flex items-center gap-3 px-3 py-2 rounded-lg text-gray-400 hover:text-white hover:bg-gray-800 transition-colors",
            {icon}
            span { "{label}" }
        }
    }
}
