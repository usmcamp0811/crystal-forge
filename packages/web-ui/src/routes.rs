//! Client-side routing for Crystal Forge Web UI.
//!
//! Uses Dioxus Router with the `#[derive(Routable)]` macro.

use dioxus::prelude::*;

use crate::components::layout::AppShell;
use crate::views::admin::AdminView;
use crate::views::builders::BuildersView;
use crate::views::builds::BuildsView;
use crate::views::cves::CvesView;
use crate::views::dashboard::DashboardView;
use crate::views::dev_login::DevLoginView;
use crate::views::environments::EnvironmentsView;
use crate::views::evals::EvalsView;
use crate::views::flakes::FlakesView;
use crate::views::login::LoginView;
use crate::views::not_found::NotFoundView;
use crate::views::policies::PoliciesView;
use crate::views::register::RegisterView;
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

    #[route("/environments")]
    EnvironmentsView {},

    #[route("/systems/:id")]
    SystemDetailView { id: String },

    #[route("/flakes")]
    FlakesView {},

    #[route("/builds")]
    BuildsView {},

    #[route("/evals")]
    EvalsView {},

    #[route("/builders")]
    BuildersView {},

    #[route("/cves")]
    CvesView {},

    #[route("/deployment-policies")]
    PoliciesView {},

    #[route("/admin")]
    AdminView {},

    #[route("/style-guide")]
    StyleGuideView {},

    // Catch-all for 404 - inside AppShell so auth guard applies
    #[route("/:..route")]
    NotFoundView { route: Vec<String> },

    #[end_layout]
    // Auth routes - outside AppShell (no sidebar/topbar)
    #[route("/login")]
    LoginView {},

    #[route("/register")]
    RegisterView {},

    #[route("/dev/login")]
    DevLoginView {},
}

impl Route {
    pub fn title(&self) -> String {
        match self {
            Route::DashboardView { .. } => "Dashboard".to_string(),
            Route::SystemsView { .. } => "Systems".to_string(),
            Route::EnvironmentsView { .. } => "Environments".to_string(),
            Route::SystemDetailView { id } => format!("System: {id}"),
            Route::FlakesView { .. } => "Flakes".to_string(),
            Route::BuildsView { .. } => "Builds".to_string(),
            Route::EvalsView { .. } => "Evaluations".to_string(),
            Route::BuildersView { .. } => "Builders".to_string(),
            Route::CvesView { .. } => "CVEs".to_string(),
            Route::PoliciesView { .. } => "Deployment Policies".to_string(),
            Route::AdminView { .. } => "Server Management".to_string(),
            Route::StyleGuideView { .. } => "Style Guide".to_string(),
            Route::LoginView { .. } => "Sign In".to_string(),
            Route::RegisterView { .. } => "Register".to_string(),
            Route::DevLoginView { .. } => "Development Login".to_string(),
            Route::NotFoundView { .. } => "Not Found".to_string(),
        }
    }
}
