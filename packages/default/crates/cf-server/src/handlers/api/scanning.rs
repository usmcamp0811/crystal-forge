use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use sqlx::PgPool;
use tracing::error;

use crate::api::models::{
    ScanSchedulePolicyResponse, ScanningActivityItemResponse, ScanningDeployedResponse,
    ScanningQueueItemResponse, ScanningScanDetailResponse, ScanningScanDiagnosticEventResponse,
    ScanningScanRecordResponse, ScanningScanRecordsResponse, ScanningStatsResponse,
    ScanningSystemsItemResponse, UpdateScanSchedulePolicyRequest, UpdateScanningArchiveRequest,
    UpdateScanningArchiveResponse,
};
use crate::auth::extractors::RequireAdmin;
use crate::handlers::api::auth_session::RequireCsrf;
use crate::handlers::api::rbac::require_admin;
use crate::queries::scanning::{
    InvalidCursorError, InvalidScanRecordCursor, ScanRecordCollection, ScanRecordDirection,
    ScanRecordRequest, ScanRecordRevision, ScanRecordSort, ScanRecordStatus, ScanSchedulePolicyRow,
    get_scan_activity, get_scan_deployed, get_scan_queue, get_scan_queue_for_system,
    get_scan_records, get_scan_schedule_policy, get_scan_stats, get_scan_systems,
    set_scan_archive_state, update_scan_schedule_policy,
};

#[derive(Debug, Deserialize, Default)]
pub struct ScanningListParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Keyset cursor for the deployed endpoint. Pass the `next_cursor` value
    /// from the previous response to retrieve the next page.
    #[serde(default)]
    pub after: Option<String>,
}

/// Controls exact scan lifecycle history returned to an administrator.
#[derive(Debug, Deserialize)]
pub struct ScanningRecordParams {
    /// Selects `active`, `completed`, or `history` rows.
    #[serde(default = "default_record_collection")]
    pub collection: String,
    /// Includes archive-marked terminal rows when true.
    #[serde(default)]
    pub include_archived: bool,
    /// Restricts history to one active system's exact flake and configuration.
    #[serde(default)]
    pub system_id: Option<uuid::Uuid>,
    /// Bounds the response size.
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// Searches configuration, flake, commit, scan UUID, and derivation ID.
    #[serde(default, alias = "search")]
    pub q: Option<String>,
    /// Selects `all`, `completed`, or `failed` terminal rows.
    #[serde(default = "default_all")]
    pub status: String,
    /// Selects `all`, `deployed`, `recent`, or `superseded` revisions.
    #[serde(default = "default_all")]
    pub revision: String,
    /// Requires the latest ready commit for each flake when true.
    #[serde(default)]
    pub latest_only: bool,
    /// Selects `configuration`, `revision`, `status`, `severity`, or `timestamp`.
    #[serde(default = "default_record_sort")]
    pub sort: String,
    /// Selects `asc` or `desc` primary ordering.
    #[serde(default = "default_record_direction")]
    pub direction: String,
    /// Continues from the opaque cursor returned by the previous page.
    #[serde(default)]
    pub after: Option<String>,
}

fn default_record_collection() -> String {
    "active".to_string()
}

fn default_all() -> String {
    "all".to_string()
}

fn default_record_sort() -> String {
    "timestamp".to_string()
}

fn default_record_direction() -> String {
    "desc".to_string()
}

fn parse_scanning_record_request(
    params: ScanningRecordParams,
) -> Result<ScanRecordRequest, &'static str> {
    let collection = match params.collection.as_str() {
        "active" => ScanRecordCollection::Active,
        "completed" => ScanRecordCollection::Completed,
        "history" => ScanRecordCollection::History,
        _ => return Err("collection must be active, completed, or history"),
    };
    if !(1..=500).contains(&params.limit) {
        return Err("limit must be between 1 and 500");
    }
    let search = params.q.and_then(|value| {
        let normalized = value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        (!normalized.is_empty()).then_some(normalized)
    });
    if search
        .as_ref()
        .is_some_and(|value| value.chars().count() > 200)
    {
        return Err("q must be 200 characters or less after normalization");
    }
    let status = match params.status.as_str() {
        "all" => ScanRecordStatus::All,
        "completed" => ScanRecordStatus::Completed,
        "failed" => ScanRecordStatus::Failed,
        _ => return Err("status must be all, completed, or failed"),
    };
    let revision = match params.revision.as_str() {
        "all" => ScanRecordRevision::All,
        "deployed" => ScanRecordRevision::Deployed,
        "recent" => ScanRecordRevision::Recent,
        "superseded" => ScanRecordRevision::Superseded,
        _ => return Err("revision must be all, deployed, recent, or superseded"),
    };
    let sort = match params.sort.as_str() {
        "configuration" => ScanRecordSort::Configuration,
        "revision" => ScanRecordSort::Revision,
        "status" => ScanRecordSort::Status,
        "severity" => ScanRecordSort::Severity,
        "timestamp" => ScanRecordSort::Timestamp,
        _ => {
            return Err("sort must be configuration, revision, status, severity, or timestamp");
        }
    };
    let direction = match params.direction.as_str() {
        "asc" => ScanRecordDirection::Asc,
        "desc" => ScanRecordDirection::Desc,
        _ => return Err("direction must be asc or desc"),
    };
    Ok(ScanRecordRequest {
        collection,
        include_archived: params.include_archived,
        system_id: params.system_id,
        limit: params.limit as u16,
        search,
        status,
        revision,
        latest_only: params.latest_only,
        sort,
        direction,
        after: params.after,
    })
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
                awaiting_build: row.awaiting_build,
                awaiting_closure: row.awaiting_closure,
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

/// Returns exact active, completed, or complete scan history for administrators.
pub async fn get_scanning_scan_records(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<ScanningRecordParams>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }
    let request = match parse_scanning_record_request(params) {
        Ok(request) => request,
        Err(message) => return validation_error(message.into()).into_response(),
    };
    match get_scan_records(&pool, &request).await {
        Ok(result) => (
            StatusCode::OK,
            Json(ScanningScanRecordsResponse {
                items: result
                    .rows
                    .into_iter()
                    .map(|row| ScanningScanRecordResponse {
                        scan_id: row.scan_id,
                        derivation_id: row.derivation_id,
                        hostname: row.hostname,
                        flake_name: row.flake_name,
                        commit_hash: row.commit_hash,
                        is_current: row.is_current,
                        is_latest_per_flake: row.is_latest_per_flake,
                        status: row.status,
                        source_trigger: row.source_trigger,
                        created_at: row.created_at,
                        scheduled_at: row.scheduled_at,
                        started_at: row.started_at,
                        completed_at: row.completed_at,
                        scanner_name: row.scanner_name,
                        scanner_version: row.scanner_version,
                        executor: row.executor,
                        failure: row.failure,
                        wait_reason: row.wait_reason,
                        total_packages: row.total_packages,
                        total_vulnerabilities: row.total_vulnerabilities,
                        critical_count: row.critical_count,
                        high_count: row.high_count,
                        medium_count: row.medium_count,
                        low_count: row.low_count,
                        scan_duration_ms: row.scan_duration_ms,
                        attempts: row.attempts,
                        archived_at: row.archived_at,
                        cancellable: row.cancellable,
                    })
                    .collect(),
                total: result.total,
                hidden_archived: result.hidden_archived,
                has_more: result.has_more,
                next_cursor: result.next_cursor,
            }),
        )
            .into_response(),
        Err(error) if error.downcast_ref::<InvalidScanRecordCursor>().is_some() => {
            validation_error("Invalid or request-incompatible pagination cursor.".into())
                .into_response()
        }
        Err(error) => {
            error!("scan record query failed: {error:#}");
            internal_error("Failed to load scan records")
        }
    }
}

/// Applies bounded idempotent archive or restore state to terminal scans.
pub async fn update_scanning_archive(
    State(pool): State<PgPool>,
    RequireAdmin(user): RequireAdmin,
    _csrf: RequireCsrf,
    Json(payload): Json<UpdateScanningArchiveRequest>,
) -> impl IntoResponse {
    let mut scan_ids = payload.scan_ids;
    scan_ids.sort_unstable();
    scan_ids.dedup();
    if scan_ids.is_empty() || scan_ids.len() > 100 {
        return validation_error("scan_ids must contain between 1 and 100 unique values".into())
            .into_response();
    }
    match set_scan_archive_state(&pool, &scan_ids, payload.archived, user.user_id).await {
        Ok(changed) => (
            StatusCode::OK,
            Json(UpdateScanningArchiveResponse {
                requested: scan_ids.len(),
                changed,
                archived: payload.archived,
            }),
        )
            .into_response(),
        Err(error) => {
            error!("scan archive update failed: {error:#}");
            internal_error("Failed to update scan archive state")
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

    match get_scan_queue(&pool, params.limit.clamp(1, 10_000)).await {
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
        derivation_id: r.derivation_id,
        rescan_eligible: r.rescan_eligible,
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
        source_trigger: r.source_trigger,
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

    match get_scan_deployed(&pool, params.limit.clamp(1, 1000), params.after.as_deref()).await {
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
                next_cursor: result.next_cursor,
            }),
        )
            .into_response(),
        Err(e) if e.downcast_ref::<InvalidCursorError>().is_some() => (
            StatusCode::BAD_REQUEST,
            axum::Json(crate::api::models::ApiError {
                error: "Bad Request".into(),
                message: "Invalid or malformed pagination cursor.".into(),
                details: None,
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

    match get_scan_systems(&pool, params.limit.clamp(1, 10_000)).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(
                rows.into_iter()
                    .map(|r| ScanningSystemsItemResponse {
                        system_id: r.system_id,
                        hostname: r.hostname,
                        flake_name: r.flake_name,
                        environment: r.environment,
                        total_configs: r.total_configs,
                        scanned: r.scanned,
                        stale: r.stale,
                        needs_build: r.needs_build,
                        unscanned: r.unscanned,
                        current_crit: r.current_crit,
                        current_high: r.current_high,
                        current_medium: r.current_medium,
                        current_low: r.current_low,
                        current_scan_id: r.current_scan_id,
                        historical_evidence: r.historical_evidence,
                        current_derivation_id: r.current_derivation_id,
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

    match get_scan_queue_for_system(&pool, system_id, params.limit.clamp(1, 10_000)).await {
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

/// Returns bounded redacted diagnostics for one exact scan.
pub async fn get_scanning_scan_detail(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(scan_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    if require_admin(&pool, &headers).await.is_none() {
        return forbidden_admin();
    }
    match crate::queries::cve_scan_diagnostics::get_scan_diagnostics(&pool, scan_id).await {
        Ok(Some(detail)) => (
            StatusCode::OK,
            Json(ScanningScanDetailResponse {
                scan_id,
                derivation_id: detail.derivation_id,
                hostname: detail.hostname,
                flake_name: detail.flake_name,
                commit_hash: detail.commit_hash,
                status: detail.status,
                scanner_name: detail.scanner_name,
                scanner_version: detail.scanner_version,
                source_trigger: detail.source_trigger,
                created_at: detail.created_at,
                scheduled_at: detail.scheduled_at,
                started_at: detail.started_at,
                completed_at: detail.completed_at,
                scan_duration_ms: detail.scan_duration_ms,
                attempts: detail.attempts,
                total_packages: detail.total_packages,
                total_vulnerabilities: detail.total_vulnerabilities,
                critical_count: detail.critical_count,
                high_count: detail.high_count,
                medium_count: detail.medium_count,
                low_count: detail.low_count,
                failure: detail.failure,
                wait_reason: detail.wait_reason,
                build_job_id: detail.build_job_id,
                build_status: detail.build_status,
                executor: detail.executor,
                archived_at: detail.archived_at,
                cancellable: false,
                events: detail
                    .events
                    .into_iter()
                    .map(|event| ScanningScanDiagnosticEventResponse {
                        id: event.id,
                        execution_id: event.execution_id,
                        attempt_number: event.attempt_number,
                        occurred_at: event.occurred_at,
                        level: event.level,
                        source: event.source,
                        event_type: event.event_type,
                        message: event.message,
                        truncated: event.truncated,
                    })
                    .collect(),
                truncated: detail.truncated,
            }),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found", "message": "Scan not found" })),
        )
            .into_response(),
        Err(error) => {
            error!("scan diagnostic detail query failed: {error:#}");
            internal_error("Failed to load scan diagnostics")
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
            Query(ScanningListParams {
                limit: 50,
                after: None,
            }),
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
            Query(ScanningListParams {
                limit: 50,
                after: None,
            }),
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
            Query(ScanningListParams {
                limit: 50,
                after: None,
            }),
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
            Query(ScanningListParams {
                limit: 50,
                after: None,
            }),
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
    async fn get_scanning_scan_detail_requires_admin() {
        let response = get_scanning_scan_detail(
            State(lazy_pool()),
            HeaderMap::new(),
            Path(uuid::Uuid::nil()),
        )
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

    fn record_params() -> ScanningRecordParams {
        ScanningRecordParams {
            collection: "completed".to_string(),
            include_archived: false,
            system_id: None,
            limit: 50,
            q: None,
            status: "all".to_string(),
            revision: "all".to_string(),
            latest_only: false,
            sort: "timestamp".to_string(),
            direction: "desc".to_string(),
            after: None,
        }
    }

    #[test]
    fn record_params_normalize_search_and_validate_bounds() {
        let request = parse_scanning_record_request(ScanningRecordParams {
            q: Some("  Mixed   CASE  ".to_string()),
            ..record_params()
        })
        .expect("valid Completed parameters should parse");
        assert_eq!(request.search.as_deref(), Some("mixed case"));
        assert_eq!(request.limit, 50);

        assert_eq!(
            parse_scanning_record_request(ScanningRecordParams {
                limit: 501,
                ..record_params()
            })
            .expect_err("oversized pages must fail"),
            "limit must be between 1 and 500"
        );
    }

    #[test]
    fn record_params_reject_unvalidated_sql_controls() {
        for (field, params) in [
            (
                "status",
                ScanningRecordParams {
                    status: "failed DESC".to_string(),
                    ..record_params()
                },
            ),
            (
                "revision",
                ScanningRecordParams {
                    revision: "current".to_string(),
                    ..record_params()
                },
            ),
            (
                "sort",
                ScanningRecordParams {
                    sort: "completed_at; DROP TABLE cve_scans".to_string(),
                    ..record_params()
                },
            ),
            (
                "direction",
                ScanningRecordParams {
                    direction: "sideways".to_string(),
                    ..record_params()
                },
            ),
        ] {
            assert!(
                parse_scanning_record_request(params).is_err(),
                "invalid {field} must fail before query construction"
            );
        }
    }
}
