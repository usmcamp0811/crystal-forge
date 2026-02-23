//! Build queue API handlers.
//!
//! Exposes the current build queue state to authenticated clients.
//!
//! # Endpoints
//!
//! - `GET /api/v1/builds` — returns [`BuildQueueSummary`] with live queue data
//! - `GET /api/v1/builds/:id` — returns a single [`BuildQueueItem`] by `nixos_id`

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;
use sqlx::PgPool;

use crate::api::models::{ApiError, BuildQueueItem, BuildQueueSummary, BuildStatus};
use crate::handlers::api::rbac::require_viewer_or_above;
use crate::queries::build_reservations::{QueueStatus, get_queue_status, get_queue_status_for_system};

/// `GET /api/v1/builds`
///
/// Returns the current build queue as a [`BuildQueueSummary`].
///
/// Requires viewer-or-above authentication.
pub async fn list_builds(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden();
    }

    let rows = match get_queue_status(&pool).await {
        Ok(rows) => rows,
        Err(_) => return internal_error("Failed to load build queue"),
    };

    let items: Vec<BuildQueueItem> = rows.iter().map(queue_status_to_item).collect();
    let building_count = items.iter().filter(|i| i.status == BuildStatus::Building).count() as i64;
    let queued_count = items.iter().filter(|i| i.status == BuildStatus::Queued).count() as i64;

    let summary = BuildQueueSummary {
        building_count,
        queued_count,
        items,
        timestamp: Utc::now(),
    };

    (StatusCode::OK, Json(summary)).into_response()
}

/// `GET /api/v1/builds/:id`
///
/// Returns a single build queue item by `nixos_id`.
///
/// Requires viewer-or-above authentication.
pub async fn get_build(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(nixos_id): Path<i32>,
) -> impl IntoResponse {
    if require_viewer_or_above(&pool, &headers).await.is_none() {
        return forbidden();
    }

    match get_queue_status_for_system(&pool, nixos_id).await {
        Ok(Some(row)) => {
            let item = queue_status_to_item(&row);
            (StatusCode::OK, Json(item)).into_response()
        }
        Ok(None) => not_found(),
        Err(_) => internal_error("Failed to load build"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mapping
// ─────────────────────────────────────────────────────────────────────────────

fn queue_status_to_item(row: &QueueStatus) -> BuildQueueItem {
    let status = match row.status.as_str() {
        "building" => BuildStatus::Building,
        "ready_for_system_build" => BuildStatus::Queued,
        _ => BuildStatus::Queued,
    };

    // Elapsed seconds: difference between now and earliest_reservation if building.
    let elapsed_secs = row.earliest_reservation.map(|reserved_at| {
        (Utc::now() - reserved_at).num_seconds().max(0)
    });

    BuildQueueItem {
        hostname: row.system_name.clone(),
        flake_name: extract_flake_name(&row.system_name),
        commit_hash: row.git_commit_hash.clone(),
        commit_message: None,
        status,
        queued_at: row.commit_timestamp,
        started_at: row.earliest_reservation,
        elapsed_secs,
    }
}

/// Extract a flake name from a system name by taking the first path component.
///
/// For example `"campground#nixosConfigurations.atlas-01"` → `"campground"`.
/// If the name does not contain `#` we return the whole name.
fn extract_flake_name(system_name: &str) -> String {
    system_name
        .split('#')
        .next()
        .unwrap_or(system_name)
        .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Error Helpers
// ─────────────────────────────────────────────────────────────────────────────

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

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_string(),
            message: "Build not found".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error(message: &str) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "internal_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use sqlx::postgres::PgPoolOptions;

    fn make_test_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct")
    }

    #[tokio::test]
    async fn list_builds_requires_authenticated_role() {
        let pool = make_test_pool();

        let response = list_builds(State(pool), HeaderMap::new())
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_build_requires_authenticated_role() {
        let pool = make_test_pool();

        let response = get_build(State(pool), HeaderMap::new(), Path(1))
            .await
            .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn extract_flake_name_splits_on_hash() {
        assert_eq!(
            extract_flake_name("campground#nixosConfigurations.atlas-01"),
            "campground"
        );
        assert_eq!(extract_flake_name("my-flake"), "my-flake");
        assert_eq!(extract_flake_name(""), "");
    }

    #[test]
    fn queue_status_to_item_maps_building_status() {
        let row = QueueStatus {
            nixos_id: 1,
            system_name: "campground#nixosConfigurations.atlas-01".to_string(),
            commit_timestamp: Utc::now(),
            git_commit_hash: "abc123def456".to_string(),
            total_packages: 10,
            completed_packages: 5,
            building_packages: 2,
            pending_packages: 3,
            cached_packages: 4,
            active_workers: 2,
            worker_ids: Some(vec!["worker-a".to_string()]),
            earliest_reservation: Some(Utc::now()),
            latest_heartbeat: Some(Utc::now()),
            status: "building".to_string(),
            cache_status: None,
            has_stale_workers: false,
        };

        let item = queue_status_to_item(&row);
        assert_eq!(item.hostname, "campground#nixosConfigurations.atlas-01");
        assert_eq!(item.flake_name, "campground");
        assert_eq!(item.commit_hash, "abc123def456");
        assert_eq!(item.status, BuildStatus::Building);
        assert!(item.elapsed_secs.is_some());
    }

    #[test]
    fn queue_status_to_item_maps_queued_status() {
        let row = QueueStatus {
            nixos_id: 2,
            system_name: "my-flake".to_string(),
            commit_timestamp: Utc::now(),
            git_commit_hash: "def456".to_string(),
            total_packages: 5,
            completed_packages: 5,
            building_packages: 0,
            pending_packages: 0,
            cached_packages: 5,
            active_workers: 0,
            worker_ids: None,
            earliest_reservation: None,
            latest_heartbeat: None,
            status: "ready_for_system_build".to_string(),
            cache_status: None,
            has_stale_workers: false,
        };

        let item = queue_status_to_item(&row);
        assert_eq!(item.status, BuildStatus::Queued);
        assert!(item.elapsed_secs.is_none());
    }
}
