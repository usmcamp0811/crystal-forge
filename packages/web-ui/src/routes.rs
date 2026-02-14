//! Client-side routing for Crystal Forge Web UI.
//!
//! Uses Dioxus Router with the `#[derive(Routable)]` macro.

use dioxus::prelude::*;

use crate::components::layout::AppShell;
use crate::views::builds::BuildsView;
use crate::views::cves::CvesView;
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
    #[layout(AppShell)]
    #[route("/")]
    DashboardView {},

    #[route("/systems")]
    SystemsView {},

    #[route("/systems/:id")]
    SystemDetailView { id: String },

    #[route("/builds")]
    BuildsView {},

    #[route("/cves")]
    CvesView {},

    #[route("/style-guide")]
    StyleGuideView {},

    #[end_layout]
    #[route("/:..route")]
    NotFoundView { route: Vec<String> },
}

impl Route {
    pub fn title(&self) -> String {
        match self {
            Route::DashboardView { .. } => "Dashboard".to_string(),
            Route::SystemsView { .. } => "Systems".to_string(),
            Route::SystemDetailView { id } => format!("System: {id}"),
            Route::BuildsView { .. } => "Builds".to_string(),
            Route::CvesView { .. } => "CVEs".to_string(),
            Route::StyleGuideView { .. } => "Style Guide".to_string(),
            Route::NotFoundView { .. } => "Not Found".to_string(),
        }
    }
}
