//! Dashboard API handler — `GET /api/v1/dashboard/summary`
//!
//! Aggregates fleet-wide metrics from database views into a single
//! [`DashboardSummary`] response for the web UI dashboard.
//!
//! All SQL lives in [`crate::queries::dashboard`]; this module is
//! responsible only for HTTP concerns (extraction, response formatting).

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::Utc;
use sqlx::PgPool;
use tracing::error;

use crate::api::models::{BuildQueueSummary, DashboardSummary};
use crate::auth::extractors::RequireAuth;
use crate::queries::dashboard::{
    fetch_active_builds, fetch_cve_summary, fetch_deployment_status, fetch_fleet_health,
    fetch_recent_deployments, fetch_total_systems,
};

/// `GET /api/v1/dashboard/summary`
///
/// Returns a [`DashboardSummary`] containing fleet health, deployment status,
/// CVE counts, active builds, and recent deployments.
///
/// **Authorization**: Requires any authenticated user (Viewer, Operator, or Admin).
pub async fn dashboard_summary(
    RequireAuth(_user): RequireAuth,
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let result = build_dashboard_summary(&pool).await;

    match result {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => {
            error!("Dashboard summary query failed: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to build dashboard summary"
                })),
            )
                .into_response()
        }
    }
}

/// Build the full dashboard summary by running parallel queries.
async fn build_dashboard_summary(pool: &PgPool) -> anyhow::Result<DashboardSummary> {
    // Run all queries concurrently.
    let (
        fleet_health,
        deployment_status,
        cve_summary,
        total_systems,
        active_builds,
        recent_deployments,
    ) = tokio::try_join!(
        fetch_fleet_health(pool),
        fetch_deployment_status(pool),
        fetch_cve_summary(pool),
        fetch_total_systems(pool),
        fetch_active_builds(pool),
        fetch_recent_deployments(pool),
    )?;

    Ok(DashboardSummary {
        fleet_health,
        deployment_status,
        cve_summary,
        total_systems,
        active_builds,
        build_queue: Some(BuildQueueSummary {
            building_count: active_builds,
            queued_count: 0,
            items: vec![],
            timestamp: Utc::now(),
        }),
        recent_deployments,
        timestamp: Utc::now(),
    })
}
