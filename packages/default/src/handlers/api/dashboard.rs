//! Dashboard API handler — `GET /api/v1/dashboard/summary`
//!
//! Aggregates fleet-wide metrics from database views into a single
//! [`DashboardSummary`] response for the web UI dashboard.
//!
//! All SQL lives in [`crate::queries::dashboard`]; this module is
//! responsible only for HTTP concerns (extraction, response formatting).

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;
use sqlx::PgPool;
use tracing::error;

use crate::api::models::ApiError;
use crate::api::models::DashboardSummary;
use crate::handlers::api::rbac::require_viewer_or_above;
use crate::queries::dashboard::{
    fetch_active_builds, fetch_build_queue, fetch_cve_summary, fetch_deployment_status,
    fetch_fleet_health, fetch_recent_deployments, fetch_total_systems,
};

/// `GET /api/v1/dashboard/summary`
///
/// Returns a [`DashboardSummary`] containing fleet health, deployment status,
/// CVE counts, active builds, and recent deployments.
pub async fn dashboard_summary(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden();
    }

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

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Viewer, operator, or admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
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
        build_queue,
        recent_deployments,
    ) = tokio::try_join!(
        fetch_fleet_health(pool),
        fetch_deployment_status(pool),
        fetch_cve_summary(pool),
        fetch_total_systems(pool),
        fetch_active_builds(pool),
        fetch_build_queue(pool, 100),
        fetch_recent_deployments(pool),
    )?;

    Ok(DashboardSummary {
        fleet_health,
        deployment_status,
        cve_summary,
        total_systems,
        active_builds,
        build_queue: Some(build_queue),
        recent_deployments,
        timestamp: Utc::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn dashboard_summary_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = dashboard_summary(State(pool), HeaderMap::new())
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
