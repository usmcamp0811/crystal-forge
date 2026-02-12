//! Client-side routing for Crystal Forge Web UI.
//!
//! Uses Dioxus Router with the `#[derive(Routable)]` macro.

use dioxus::prelude::*;

use crate::components::layout::AppLayout;
use crate::views::dashboard::DashboardView;
use crate::views::not_found::NotFoundView;
use crate::views::system_detail::SystemDetailView;
use crate::views::systems::SystemsView;

/// All application routes.
///
/// Variant names must match the component function names (sans the `View` suffix
/// doesn't matter — Dioxus router derives component names from variants).
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    #[layout(AppLayout)]
    #[route("/")]
    DashboardView {},

    #[route("/systems")]
    SystemsView {},

    #[route("/systems/:id")]
    SystemDetailView { id: String },

    #[end_layout]
    #[route("/:..route")]
    NotFoundView { route: Vec<String> },
}
