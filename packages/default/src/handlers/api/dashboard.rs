//! Dashboard API handler — `GET /api/v1/dashboard/summary`
//!
//! Aggregates fleet-wide metrics from database views into a single
//! [`DashboardSummary`] response for the web UI dashboard.
//!
//! All SQL lives in [`crate::queries::dashboard`]; this module is
//! responsible only for HTTP concerns (extraction, response formatting).

use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;
use sqlx::PgPool;
use sqlx::Row;
use tracing::error;

use crate::api::models::ApiError;
use crate::api::models::CveDashboardVulnerability;
use crate::api::models::CveDashboardSummary;
use crate::api::models::CveDashboardTopSystem;
use crate::api::models::CveScanFreshnessRow;
use crate::api::models::CveSummary;
use crate::api::models::DashboardSummary;
use crate::handlers::api::rbac::require_admin;
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

/// `GET /api/v1/cves/summary`
///
/// Returns admin-only fleet CVE metrics for the CVE dashboard page.
pub async fn cve_dashboard_summary(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    let row = match sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, Option<i64>)>(
        r#"
        WITH severity_totals AS (
            SELECT
                COALESCE(SUM(critical_cves), 0) AS critical,
                COALESCE(SUM(high_cves), 0) AS high,
                COALESCE(SUM(medium_cves), 0) AS medium,
                COALESCE(SUM(low_cves), 0) AS low,
                COUNT(*) FILTER (WHERE total_cves > 0) AS affected_systems
            FROM view_systems_cve_summary
        ),
        new_cves AS (
            SELECT COUNT(DISTINCT v.cve_id) AS count
            FROM view_system_vulnerabilities v
            LEFT JOIN cves c ON c.id = v.cve_id
            WHERE c.published_date >= (CURRENT_DATE - INTERVAL '7 days')
        ),
        oldest_cve AS (
            SELECT
                DATE_PART('day', CURRENT_DATE - MIN(c.published_date))::BIGINT AS age_days
            FROM view_system_vulnerabilities v
            LEFT JOIN cves c ON c.id = v.cve_id
            WHERE c.published_date IS NOT NULL
        )
        SELECT
            s.critical,
            s.high,
            s.medium,
            s.low,
            s.affected_systems,
            n.count,
            o.age_days
        FROM severity_totals s
        CROSS JOIN new_cves n
        CROSS JOIN oldest_cve o
        "#,
    )
    .fetch_one(&pool)
    .await
    {
        Ok(value) => value,
        Err(e) => {
            error!("CVE dashboard summary query failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to load CVE dashboard summary"
                })),
            )
                .into_response();
        }
    };

    let severity = CveSummary {
        critical: row.0,
        high: row.1,
        medium: row.2,
        low: row.3,
    };

    let payload = CveDashboardSummary {
        total_open: severity.total(),
        severity,
        affected_systems: row.4,
        new_cves_last_7_days: row.5,
        oldest_cve_age_days: row.6,
    };

    (StatusCode::OK, Json(payload)).into_response()
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct CveDashboardFilterParams {
    pub severity: Option<String>,
    pub status: Option<String>,
    pub system: Option<String>,
    pub environment: Option<String>,
    pub package: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub limit: Option<i64>,
}

/// `GET /api/v1/cves/vulnerabilities`
///
/// Returns admin-only CVE rows for dashboard drill-down tables.
pub async fn cve_dashboard_vulnerabilities(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<CveDashboardFilterParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    let severity_filter = match normalize_severity_filter(params.severity.as_deref()) {
        Ok(value) => value,
        Err(message) => return bad_request(message),
    };
    let status_filter = match normalize_status_filter(params.status.as_deref()) {
        Ok(value) => value,
        Err(message) => return bad_request(message),
    };

    let system_filter = normalize_text_filter(params.system.as_deref());
    let environment_filter = normalize_text_filter(params.environment.as_deref());
    let package_filter = normalize_text_filter(params.package.as_deref());
    let date_from = normalize_text_filter(params.date_from.as_deref());
    let date_to = normalize_text_filter(params.date_to.as_deref());

    let limit = params.limit.unwrap_or(200).clamp(1, 1000);

    let rows = match sqlx::query(
        r#"
        SELECT
            s.id AS system_id,
            v.hostname,
            v.cve_id,
            lower(v.severity) AS severity,
            v.cvss_v3_score::double precision AS cvss_score,
            v.package_name,
            v.package_version AS installed_version,
            v.fixed_version,
            v.completed_at AS first_seen,
            CASE WHEN v.fixed_version IS NULL THEN 'open' ELSE 'fixed' END AS status
        FROM view_system_vulnerabilities v
        JOIN systems s ON s.hostname = v.hostname
        LEFT JOIN environments e ON e.id = s.environment_id
        WHERE ($1::text IS NULL OR lower(v.severity) = $1)
          AND (
            $2::text IS NULL
            OR (CASE WHEN v.fixed_version IS NULL THEN 'open' ELSE 'fixed' END) = $2
          )
          AND ($3::text IS NULL OR v.hostname ILIKE ('%' || $3 || '%'))
          AND ($4::text IS NULL OR COALESCE(e.name, '') ILIKE ('%' || $4 || '%'))
          AND (
            $5::text IS NULL
            OR v.package_name ILIKE ('%' || $5 || '%')
            OR v.package_pname ILIKE ('%' || $5 || '%')
          )
          AND ($6::text IS NULL OR v.completed_at::date >= $6::date)
          AND ($7::text IS NULL OR v.completed_at::date <= $7::date)
        ORDER BY v.cvss_v3_score DESC NULLS LAST, v.cve_id ASC, v.hostname ASC
        LIMIT $8
        "#,
    )
    .bind(severity_filter.as_deref())
    .bind(status_filter.as_deref())
    .bind(system_filter.as_deref())
    .bind(environment_filter.as_deref())
    .bind(package_filter.as_deref())
    .bind(date_from.as_deref())
    .bind(date_to.as_deref())
    .bind(limit)
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(e) => {
            error!("CVE dashboard vulnerabilities query failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to load CVE dashboard vulnerabilities"
                })),
            )
                .into_response();
        }
    };

    let payload = rows
        .into_iter()
        .map(|row| CveDashboardVulnerability {
            system_id: row.get("system_id"),
            hostname: row.get("hostname"),
            cve_id: row.get("cve_id"),
            severity: parse_cve_severity(row.get::<String, _>("severity").as_str()),
            cvss_score: row.get("cvss_score"),
            package_name: row.get("package_name"),
            installed_version: row.get("installed_version"),
            fixed_version: row.get("fixed_version"),
            first_seen: row.get("first_seen"),
            status: row.get("status"),
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(payload)).into_response()
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

fn forbidden_admin() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: impl Into<String>) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "bad_request".to_string(),
            message: message.into(),
            details: None,
        }),
    )
        .into_response()
}

fn parse_cve_severity(value: &str) -> crate::api::models::CveSeverity {
    match value {
        "critical" => crate::api::models::CveSeverity::Critical,
        "high" => crate::api::models::CveSeverity::High,
        "medium" => crate::api::models::CveSeverity::Medium,
        _ => crate::api::models::CveSeverity::Low,
    }
}

fn normalize_severity_filter(value: Option<&str>) -> Result<Option<String>, &'static str> {
    let Some(raw) = value.map(|v| v.trim().to_ascii_lowercase()) else {
        return Ok(None);
    };
    if raw.is_empty() || raw == "all" {
        return Ok(None);
    }
    match raw.as_str() {
        "critical" | "high" | "medium" | "low" => Ok(Some(raw)),
        _ => Err("Invalid severity filter"),
    }
}

fn normalize_status_filter(value: Option<&str>) -> Result<Option<String>, &'static str> {
    let Some(raw) = value.map(|v| v.trim().to_ascii_lowercase()) else {
        return Ok(None);
    };
    if raw.is_empty() || raw == "all" {
        return Ok(None);
    }
    match raw.as_str() {
        "open" | "fixed" => Ok(Some(raw)),
        _ => Err("Invalid status filter"),
    }
}

fn normalize_text_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// `GET /api/v1/cves/top-systems`
///
/// Returns admin-only top-affected systems for CVE dashboard visualization.
pub async fn cve_dashboard_top_systems(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    let rows = match sqlx::query(
        r#"
        SELECT
            s.id AS system_id,
            v.hostname,
            COALESCE(v.total_cves, 0) AS total_cves,
            COALESCE(v.critical_cves, 0) AS critical_cves,
            COALESCE(v.high_cves, 0) AS high_cves,
            COALESCE(v.medium_cves, 0) AS medium_cves,
            COALESCE(v.low_cves, 0) AS low_cves,
            v.days_since_scan::BIGINT AS days_since_scan,
            v.last_cve_scan AS last_cve_scan
        FROM view_systems_cve_summary v
        JOIN systems s ON s.hostname = v.hostname
        WHERE v.total_cves > 0
        ORDER BY v.critical_cves DESC, v.high_cves DESC, v.total_cves DESC
        LIMIT 20
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(e) => {
            error!("CVE top-systems query failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to load CVE top-systems data"
                })),
            )
                .into_response();
        }
    };

    let payload = rows
        .into_iter()
        .map(|row| CveDashboardTopSystem {
            system_id: row.get("system_id"),
            hostname: row.get("hostname"),
            total_cves: row.get("total_cves"),
            critical_cves: row.get("critical_cves"),
            high_cves: row.get("high_cves"),
            medium_cves: row.get("medium_cves"),
            low_cves: row.get("low_cves"),
            days_since_scan: row.get("days_since_scan"),
            last_cve_scan: row.get("last_cve_scan"),
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(payload)).into_response()
}

/// `GET /api/v1/cves/scan-freshness`
///
/// Returns admin-only scan freshness/coverage per system.
pub async fn cve_scan_freshness(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    let rows = match sqlx::query(
        r#"
        SELECT
            s.id AS system_id,
            v.hostname,
            v.days_since_scan::BIGINT AS days_since_scan,
            v.last_cve_scan AS last_cve_scan,
            COALESCE(v.total_cves, 0) AS total_cves
        FROM view_systems_cve_summary v
        JOIN systems s ON s.hostname = v.hostname
        ORDER BY
            CASE WHEN v.days_since_scan IS NULL THEN 1 ELSE 0 END DESC,
            v.days_since_scan DESC
        LIMIT 50
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(e) => {
            error!("CVE scan-freshness query failed: {e:#}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "Failed to load CVE scan freshness data"
                })),
            )
                .into_response();
        }
    };

    let payload = rows
        .into_iter()
        .map(|row| CveScanFreshnessRow {
            system_id: row.get("system_id"),
            hostname: row.get("hostname"),
            days_since_scan: row.get("days_since_scan"),
            last_cve_scan: row.get("last_cve_scan"),
            total_cves: row.get("total_cves"),
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(payload)).into_response()
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

    #[tokio::test]
    async fn cve_dashboard_summary_requires_admin_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = cve_dashboard_summary(State(pool), HeaderMap::new())
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn cve_dashboard_drilldown_requires_admin_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = cve_dashboard_vulnerabilities(
            State(pool),
            HeaderMap::new(),
            Query(CveDashboardFilterParams::default()),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn normalize_severity_filter_accepts_expected_values() {
        assert_eq!(normalize_severity_filter(Some("critical")).unwrap(), Some("critical".to_string()));
        assert_eq!(normalize_severity_filter(Some("all")).unwrap(), None);
        assert!(normalize_severity_filter(Some("urgent")).is_err());
    }

    #[test]
    fn normalize_status_filter_accepts_expected_values() {
        assert_eq!(normalize_status_filter(Some("open")).unwrap(), Some("open".to_string()));
        assert_eq!(normalize_status_filter(Some("all")).unwrap(), None);
        assert!(normalize_status_filter(Some("ignored")).is_err());
    }
}
