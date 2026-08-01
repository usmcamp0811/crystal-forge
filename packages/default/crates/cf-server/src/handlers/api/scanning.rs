use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use sqlx::PgPool;
use tracing::error;

use crate::api::models::{
    ScanSchedulePolicyResponse, ScanningActivityItemResponse, ScanningDeployedResponse,
    ScanningQueueItemResponse, ScanningStatsResponse, ScanningSystemsItemResponse,
    UpdateScanSchedulePolicyRequest,
};
use crate::handlers::api::rbac::require_admin;
use crate::queries::scanning::{
    ScanSchedulePolicyRow, get_scan_activity, get_scan_deployed, get_scan_queue,
    get_scan_queue_for_system, get_scan_schedule_policy, get_scan_stats, get_scan_systems,
    update_scan_schedule_policy,
};

#[derive(Debug, Deserialize)]
pub struct ScanningListParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn get_scanning_stats(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_stats(&pool).await {
        Ok(row) => (
            StatusCode::OK,
            Json(ScanningStatsResponse {
                scanning: row.scanning,
                queued: row.queued,
                stale: row.stale,
                never_scanned: row.never_scanned,
                failed: row.failed,
                coverage_percent: row.coverage_percent,
            }),
        )
            .into_response(),
        Err(e) => {
            error!("scanning stats query failed: {e:#}");
            internal_error("Failed to load scanning stats")
        }
    }
}

pub async fn get_scanning_queue(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<ScanningListParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_queue(&pool, params.limit.clamp(1, 500)).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(scan_queue_row_to_response)
                    .collect::<Vec<_>>(),
            ),
        )
            .into_response(),
        Err(e) => {
            error!("scanning queue query failed: {e:#}");
            internal_error("Failed to load scanning queue")
        }
    }
}

fn scan_queue_row_to_response(
    r: crate::queries::scanning::ScanQueueRow,
) -> ScanningQueueItemResponse {
    ScanningQueueItemResponse {
        scan_id: r.scan_id, // Option<Uuid>: None for never-scanned deployed configs
        hostname: r.hostname,
        flake_name: r.flake_name,
        commit_hash: r.commit_hash,
        status: r.status,
        completed_at: r.completed_at,
        scheduled_at: r.scheduled_at,
        critical_count: r.critical_count,
        high_count: r.high_count,
        medium_count: r.medium_count,
        freshness: r.freshness,
        is_current: r.is_current,
        is_latest_per_flake: r.is_latest_per_flake,
        trigger: None,
    }
}

pub async fn get_scanning_deployed(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<ScanningListParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_deployed(&pool, params.limit.clamp(1, 1000)).await {
        Ok(result) => (
            StatusCode::OK,
            Json(ScanningDeployedResponse {
                items: result
                    .rows
                    .into_iter()
                    .map(scan_queue_row_to_response)
                    .collect(),
                total: result.total,
                has_more: result.has_more,
            }),
        )
            .into_response(),
        Err(e) => {
            error!("scanning deployed query failed: {e:#}");
            internal_error("Failed to load deployed scanning configurations")
        }
    }
}

pub async fn get_scanning_systems(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<ScanningListParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_systems(&pool, params.limit.clamp(1, 500)).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(|r| ScanningSystemsItemResponse {
                        system_id: r.system_id,
                        hostname: r.hostname,
                        environment: r.environment,
                        total_configs: r.total_configs,
                        scanned: r.scanned,
                        stale: r.stale,
                        needs_build: r.needs_build,
                        unscanned: r.unscanned,
                        current_crit: r.current_crit,
                        current_high: r.current_high,
                    })
                    .collect::<Vec<_>>(),
            ),
        )
            .into_response(),
        Err(e) => {
            error!("scanning systems query failed: {e:#}");
            internal_error("Failed to load scanning systems")
        }
    }
}

pub async fn get_scanning_system_scans(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<uuid::Uuid>,
    Query(params): Query<ScanningListParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_queue_for_system(&pool, system_id, params.limit.clamp(1, 500)).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(scan_queue_row_to_response)
                    .collect::<Vec<_>>(),
            ),
        )
            .into_response(),
        Err(e) => {
            error!("scanning system scans query failed: {e:#}");
            internal_error("Failed to load system scan rows")
        }
    }
}

pub async fn get_scanning_activity(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<ScanningListParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_activity(&pool, params.limit.clamp(1, 500)).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(|r| ScanningActivityItemResponse {
                        at: r.at,
                        name: r.name,
                        event: r.event,
                        detail: r.detail,
                        status: r.status,
                    })
                    .collect::<Vec<_>>(),
            ),
        )
            .into_response(),
        Err(e) => {
            error!("scanning activity query failed: {e:#}");
            internal_error("Failed to load scanning activity")
        }
    }
}

pub async fn get_scanning_schedule(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    match get_scan_schedule_policy(&pool).await {
        Ok(p) => (
            StatusCode::OK,
            Json(ScanSchedulePolicyResponse {
                on_build: p.on_build,
                deployed_interval: p.deployed_interval,
                recent_interval: p.recent_interval,
                archived_interval: p.archived_interval,
                archived_enabled: p.archived_enabled,
                rebuild_to_scan: p.rebuild_to_scan,
                updated_at: p.updated_at,
            }),
        )
            .into_response(),
        Err(e) => {
            error!("scanning schedule get failed: {e:#}");
            internal_error("Failed to load scan schedule")
        }
    }
}

/// Validate that a scan interval string is a known, safe value.
/// Accepts `never` or a positive integer followed by `h` (hours) or `d` (days).
fn validate_scan_interval(
    val: &str,
    label: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if val == "never" {
        return Ok(());
    }
    let trimmed = val.trim();
    if trimmed.len() < 2 || (!trimmed.ends_with('h') && !trimmed.ends_with('d')) {
        return Err(validation_error(format!(
            "Invalid {label} interval {val:?}: must be 'never' or a number followed by 'h' or 'd' (e.g. '24h', '7d')"
        )));
    }
    let (num_str, unit) = trimmed.split_at(trimmed.len() - 1);
    let num: u32 = num_str.parse().map_err(|_| {
        validation_error(format!(
            "Invalid {label} interval {val:?}: could not parse number from '{num_str}'"
        ))
    })?;
    if num == 0 {
        return Err(validation_error(format!(
            "Invalid {label} interval {val:?}: interval must be > 0"
        )));
    }
    match unit {
        "h" | "d" => Ok(()),
        _ => Err(validation_error(format!(
            "Invalid {label} interval {val:?}: unit must be 'h' or 'd'"
        ))),
    }
}

fn validation_error(msg: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
}

pub async fn put_scanning_schedule(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<UpdateScanSchedulePolicyRequest>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }

    // Validate interval values before writing to the database.
    if let Err(e) = validate_scan_interval(&payload.deployed_interval, "deployed_interval") {
        return e.into_response();
    }
    if let Err(e) = validate_scan_interval(&payload.recent_interval, "recent_interval") {
        return e.into_response();
    }
    if let Err(e) = validate_scan_interval(&payload.archived_interval, "archived_interval") {
        return e.into_response();
    }

    let row = ScanSchedulePolicyRow {
        on_build: payload.on_build,
        deployed_interval: payload.deployed_interval,
        recent_interval: payload.recent_interval,
        archived_interval: payload.archived_interval,
        archived_enabled: payload.archived_enabled,
        rebuild_to_scan: payload.rebuild_to_scan,
        updated_at: chrono::Utc::now(),
    };

    match update_scan_schedule_policy(&pool, &row).await {
        Ok(_) => match get_scan_schedule_policy(&pool).await {
            Ok(p) => (
                StatusCode::OK,
                Json(ScanSchedulePolicyResponse {
                    on_build: p.on_build,
                    deployed_interval: p.deployed_interval,
                    recent_interval: p.recent_interval,
                    archived_interval: p.archived_interval,
                    archived_enabled: p.archived_enabled,
                    rebuild_to_scan: p.rebuild_to_scan,
                    updated_at: p.updated_at,
                }),
            )
                .into_response(),
            Err(e) => {
                error!("scanning schedule reload failed after update: {e:#}");
                internal_error("Failed to reload scan schedule")
            }
        },
        Err(e) => {
            error!("scanning schedule update failed: {e:#}");
            internal_error("Failed to update scan schedule")
        }
    }
}

fn forbidden_admin() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "forbidden",
            "message": "Admin role required"
        })),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "internal_error", "message": message })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct")
    }

    #[tokio::test]
    async fn get_scanning_stats_requires_admin() {
        let response = get_scanning_stats(State(lazy_pool()), HeaderMap::new())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_scanning_queue_requires_admin() {
        let response = get_scanning_queue(
            State(lazy_pool()),
            HeaderMap::new(),
            Query(ScanningListParams { limit: 50 }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_scanning_systems_requires_admin() {
        let response = get_scanning_systems(
            State(lazy_pool()),
            HeaderMap::new(),
            Query(ScanningListParams { limit: 50 }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_scanning_activity_requires_admin() {
        let response = get_scanning_activity(
            State(lazy_pool()),
            HeaderMap::new(),
            Query(ScanningListParams { limit: 50 }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_scanning_system_scans_requires_admin() {
        let response = get_scanning_system_scans(
            State(lazy_pool()),
            HeaderMap::new(),
            Path(uuid::Uuid::nil()),
            Query(ScanningListParams { limit: 50 }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_scanning_schedule_requires_admin() {
        let response = get_scanning_schedule(State(lazy_pool()), HeaderMap::new())
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn put_scanning_schedule_requires_admin() {
        let payload = UpdateScanSchedulePolicyRequest {
            on_build: true,
            deployed_interval: "24h".to_string(),
            recent_interval: "24h".to_string(),
            archived_interval: "168h".to_string(),
            archived_enabled: true,
            rebuild_to_scan: false,
        };
        let response = put_scanning_schedule(State(lazy_pool()), HeaderMap::new(), Json(payload))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn default_limit_is_fifty() {
        assert_eq!(default_limit(), 50);
    }
}
