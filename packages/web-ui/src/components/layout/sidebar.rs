//! Sidebar navigation component.

use dioxus::prelude::*;

use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

/// Sidebar navigation for primary routes.
#[component]
pub fn SidebarNav() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let show_admin = auth::is_admin(&auth_context);

    rsx! {
        nav {
            class: "hidden lg:flex w-64 {theme::surface::SIDEBAR_BG} border-r {theme::surface::CARD_BORDER} flex-col",
            div {
                class: "p-6 flex items-center gap-3",
                img {
                    class: "h-8 w-8 cf-logo-scale",
                    src: asset!("assets/crystal-forge-icon.png"),
                    alt: "Crystal Forge"
                }
                div {
                    h1 {
                        class: "text-xl font-bold {theme::text::PRIMARY}",
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
                    to: Route::EnvironmentsView {},
                    label: "Environments",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M3 7h18" }
                            path { d: "M6 12h12" }
                            path { d: "M9 17h6" }
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
                            path { d: "M3 7h18" }
                            path { d: "M3 12h18" }
                            path { d: "M3 17h18" }
                            path { d: "M7 7v10" }
                            path { d: "M17 7v10" }
                        }
                    )
                }
                NavLink {
                    to: Route::EvalsView {},
                    label: "Evaluations",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" }
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
                    to: Route::BuildersView {},
                    label: "Builders",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "3", y: "4", width: "18", height: "16", rx: "2" }
                            path { d: "M8 8h8" }
                            path { d: "M8 12h8" }
                            path { d: "M8 16h5" }
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
                    to: Route::PoliciesView {},
                    label: "Deployment Policies",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                        }
                    )
                }
                if show_admin {
                    NavLink {
                        to: Route::AdminView {},
                        label: "Server Management",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M12 3l8 4v5c0 5-3 8-8 9-5-1-8-4-8-9V7l8-4z" }
                                path { d: "M9 12l2 2 4-4" }
                            }
                        )
                    }
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
            class: "flex items-center gap-3 px-3 py-2 rounded-lg {theme::text::SECONDARY} {theme::interactive::HOVER_BG} cf-hover-text-primary transition-colors",
            {icon}
            span { "{label}" }
        }
    }
}
