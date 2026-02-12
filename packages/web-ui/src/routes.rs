//! Client-side routing for Crystal Forge Web UI.
//!
//! Uses Dioxus Router with the `#[derive(Routable)]` macro.

use dioxus::prelude::*;

use crate::components::layout::AppLayout;
use crate::views::dashboard::DashboardView;
use crate::views::not_found::NotFoundView;
use crate::views::style_guide::StyleGuideView;
use crate::views::system_detail::SystemDetailView;
use crate::views::systems::SystemsView;

/// All application routes.
///
/// Variant names must match the component function names — the Dioxus router
/// macro derives component names from enum variants.
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    DashboardView {},

    #[route("/systems")]
    SystemsView {},

    #[route("/systems/:id")]
    SystemDetailView { id: String },

    #[route("/style-guide")]
    StyleGuideView {},

    #[end_layout]
    #[route("/:..route")]
    NotFoundView { route: Vec<String> },
}
