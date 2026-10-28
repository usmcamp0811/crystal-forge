//! Client-side routing for Crystal Forge Web UI.
//!
//! Uses Dioxus Router with the `#[derive(Routable)]` macro.

use dioxus::prelude::*;

use crate::components::layout::AppShell;
use crate::views::admin::AdminView;
use crate::views::builders::BuildersView;
use crate::views::builds::BuildsView;
use crate::views::caches::CachesView;
use crate::views::compliance::{ComplianceEvaluationView, ComplianceView};
use crate::views::cves::CvesView;
use crate::views::dashboard::DashboardView;
use crate::views::dev_login::DevLoginView;
use crate::views::environments::EnvironmentsView;
use crate::views::evaluations::{EvaluationsAttemptView, EvaluationsCommitView, EvaluationsView};
use crate::views::flakes::FlakesView;
use crate::views::login::LoginView;
use crate::views::not_found::NotFoundView;
use crate::views::poams::PoamsView;
use crate::views::policies::PoliciesView;
use crate::views::profile::ProfileView;
use crate::views::register::RegisterView;
use crate::views::scanning::ScanningView;
use crate::views::setup::SetupView;
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

    #[route("/systems?:..query")]
    SystemsView { query: String },

    #[route("/environments?:..query")]
    EnvironmentsView { query: String },

    #[route(
        "/systems/:id?:tab&:poam&:config_mode&:revision&:generation&:deploy_generation&:cve_target&:cve_mode"
    )]
    SystemDetailView {
        id: String,
        tab: String,
        poam: String,
        config_mode: String,
        revision: String,
        generation: String,
        deploy_generation: String,
        cve_target: String,
        cve_mode: String,
    },

    #[route("/flakes?:..query")]
    FlakesView { query: String },

    #[route("/builds?:..query")]
    BuildsView { query: String },

    #[route("/evaluations")]
    EvaluationsView {},

    #[route("/evaluations/:commit_id")]
    EvaluationsCommitView { commit_id: i32 },

    /// Opens an exact retained attempt without selecting a newer retry.
    #[route("/evaluations/:commit_id/attempts/:attempt_id")]
    EvaluationsAttemptView {
        /// Database commit identity.
        commit_id: i32,
        /// Full retained attempt UUID.
        attempt_id: uuid::Uuid,
    },

    #[route("/builders")]
    BuildersView {},

    #[route("/caches")]
    CachesView {},

    #[route("/cves?:..query")]
    CvesView { query: String },

    #[route("/scanning")]
    ScanningView {},

    #[route("/deployment-policies")]
    PoliciesView {},

    #[route("/poams?:..query")]
    PoamsView { query: String },

    /// Opens captured evaluation evidence over the normal Compliance catalog.
    #[route(
        "/compliance/evaluations/:commit_id/attempts/:attempt_id?:bundle&:version&:system&:policy"
    )]
    ComplianceEvaluationView {
        /// Database commit identity.
        commit_id: i32,
        /// Full retained attempt UUID.
        attempt_id: uuid::Uuid,
        /// Full captured bundle lineage UUID.
        bundle: String,
        /// Full captured bundle version UUID.
        version: String,
        /// Full captured system UUID.
        system: String,
        /// Full captured policy version UUID; the API supplies its lineage.
        policy: String,
    },

    #[route("/compliance?:bundle&:version&:system&:policy&:poam&:view")]
    ComplianceView {
        bundle: String,
        version: String,
        system: String,
        policy: String,
        poam: String,
        view: String,
    },

    #[route("/admin")]
    AdminView {},

    #[route("/profile")]
    ProfileView {},

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

    #[route("/setup")]
    SetupView {},

    #[route("/dev/login")]
    DevLoginView {},
}

impl Route {
    /// Returns the navigation breadcrumb for this route.
    ///
    /// The bundle catalog retains its Compliance page heading, while the
    /// breadcrumb identifies its specific Bundles destination.
    pub fn title(&self) -> String {
        match self {
            Route::DashboardView { .. } => "Dashboard".to_string(),
            Route::SystemsView { .. } => "Systems".to_string(),
            Route::EnvironmentsView { .. } => "Environments".to_string(),
            Route::SystemDetailView { id, .. } => format!("System: {id}"),
            Route::FlakesView { .. } => "Flakes".to_string(),
            Route::BuildsView { .. } => "Builds".to_string(),
            Route::EvaluationsView { .. } => "Evaluations".to_string(),
            Route::EvaluationsCommitView { commit_id } => {
                format!("Evaluations · commit {commit_id}")
            }
            Route::EvaluationsAttemptView { commit_id, .. } => {
                format!("Evaluations · commit {commit_id} · retained attempt")
            }
            Route::BuildersView { .. } => "Builders".to_string(),
            Route::CachesView { .. } => "Caches".to_string(),
            Route::CvesView { .. } => "CVEs".to_string(),
            Route::ScanningView { .. } => "Scanning".to_string(),
            Route::PoliciesView { .. } => "Deployment Policies".to_string(),
            Route::PoamsView { .. } => "POA&M".to_string(),
            Route::ComplianceView { .. } => "Bundles".to_string(),
            Route::ComplianceEvaluationView { .. } => "Bundles · evaluation evidence".to_string(),
            Route::AdminView { .. } => "Server Management".to_string(),
            Route::ProfileView { .. } => "Profile & Preferences".to_string(),
            Route::StyleGuideView { .. } => "Component Showcase".to_string(),
            Route::LoginView { .. } => "Sign In".to_string(),
            Route::RegisterView { .. } => "Register".to_string(),
            Route::SetupView { .. } => "Setup Wizard".to_string(),
            Route::DevLoginView { .. } => "Development Login".to_string(),
            Route::NotFoundView { .. } => "Not Found".to_string(),
        }
    }
}

#[cfg(test)]
mod historical_route_tests {
    use super::*;

    #[test]
    fn exact_attempt_routes_round_trip_full_uuid_targets() {
        let id = |n| uuid::Uuid::from_u128(n);
        let route = Route::ComplianceEvaluationView {
            commit_id: 42,
            attempt_id: id(1),
            bundle: id(2).to_string(),
            version: id(3).to_string(),
            system: id(4).to_string(),
            policy: id(5).to_string(),
        };
        let url = route.to_string();
        assert!(url.starts_with(&format!("/compliance/evaluations/42/attempts/{}?", id(1))));
        assert_eq!(url.parse::<Route>().unwrap(), route);
        let route = Route::EvaluationsAttemptView {
            commit_id: 42,
            attempt_id: id(1),
        };
        assert_eq!(route.to_string().parse::<Route>().unwrap(), route);
    }

    #[test]
    fn historical_routes_do_not_alias_latest_or_deployed_destinations() {
        let attempt = uuid::Uuid::from_u128(1);
        let exact = format!("/evaluations/42/attempts/{attempt}")
            .parse::<Route>()
            .unwrap();
        assert_ne!(exact, Route::EvaluationsCommitView { commit_id: 42 });
        let historical = format!("/compliance/evaluations/42/attempts/{attempt}")
            .parse::<Route>()
            .unwrap();
        assert!(matches!(historical, Route::ComplianceEvaluationView { .. }));
    }
}
