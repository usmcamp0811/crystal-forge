use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use sqlx::PgPool;
use tracing::error;

use crate::api::models::{
    ScanSchedulePolicyResponse, ScanningActivityItemResponse, ScanningQueueItemResponse,
    ScanningStatsResponse, ScanningSystemsItemResponse, UpdateScanSchedulePolicyRequest,
};
use crate::handlers::api::rbac::require_admin;
use crate::queries::scanning::{
    ScanSchedulePolicyRow, get_scan_activity, get_scan_queue, get_scan_schedule_policy,
    get_scan_stats, get_scan_systems, update_scan_schedule_policy,
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
                    .map(|r| ScanningQueueItemResponse {
                        scan_id: r.scan_id,
                        hostname: r.hostname,
                        flake_name: r.flake_name,
                        commit_hash: r.commit_hash,
                        status: r.status,
                        completed_at: r.completed_at,
                        scheduled_at: r.scheduled_at,
                        critical_count: r.critical_count,
                        high_count: r.high_count,
                        medium_count: r.medium_count,
                    })
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

pub async fn put_scanning_schedule(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<UpdateScanSchedulePolicyRequest>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
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
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
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
