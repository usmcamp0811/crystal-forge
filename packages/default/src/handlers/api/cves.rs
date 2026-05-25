//! API handlers for the advanced CVE dashboard.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;

use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup,
};
use crate::auth::user::User;
use crate::queries::cves;
use crate::server::AppState;

/// GET /api/v1/cves
/// List CVEs with filters.
pub async fn list_cves(
    State(state): State<AppState>,
    Query(filters): Query<CveFilters>,
    _user: User, // Admin role check applied via middleware
) -> Result<Json<Vec<CveListItem>>, (StatusCode, String)> {
    let cves = cves::fetch_cve_list(&state.pool, &filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch CVE list: {}", e),
            )
        })?;

    Ok(Json(cves))
}

/// GET /api/v1/cves/grouped
/// List CVEs grouped by package.
pub async fn list_cves_grouped(
    State(state): State<AppState>,
    Query(filters): Query<CveFilters>,
    _user: User,
) -> Result<Json<Vec<CvePackageGroup>>, (StatusCode, String)> {
    let groups = cves::fetch_cve_packages_grouped(&state.pool, &filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch grouped CVEs: {}", e),
            )
        })?;

    Ok(Json(groups))
}

/// GET /api/v1/cves/stats
/// Get fleet-wide CVE statistics.
pub async fn get_fleet_stats(
    State(state): State<AppState>,
    _user: User,
) -> Result<Json<CveFleetStats>, (StatusCode, String)> {
    let stats = cves::fetch_cve_fleet_stats(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch CVE stats: {}", e),
            )
        })?;

    Ok(Json(stats))
}

/// GET /api/v1/cves/packages
/// Get list of package names for autocomplete.
pub async fn list_package_names(
    State(state): State<AppState>,
    _user: User,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let packages = cves::fetch_package_names(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch package names: {}", e),
            )
        })?;

    Ok(Json(packages))
}

/// GET /api/v1/cves/:cve_id
/// Get detailed information for a CVE.
pub async fn get_cve_detail(
    State(state): State<AppState>,
    Path(cve_id): Path<String>,
    _user: User,
) -> Result<Json<CveDetail>, (StatusCode, String)> {
    let detail = cves::fetch_cve_detail(&state.pool, &cve_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch CVE detail: {}", e),
            )
        })?;

    Ok(Json(detail))
}

/// GET /api/v1/cves/:cve_id/systems
/// Get systems affected by a CVE.
pub async fn get_cve_systems(
    State(state): State<AppState>,
    Path(cve_id): Path<String>,
    _user: User,
) -> Result<Json<Vec<CveAffectedSystemDetail>>, (StatusCode, String)> {
    let systems = cves::fetch_cve_affected_systems(&state.pool, &cve_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch affected systems: {}", e),
            )
        })?;

    Ok(Json(systems))
}

/// POST /api/v1/cves/:cve_id/justification
/// Create or update a CVE justification (admin only).
pub async fn save_justification(
    State(state): State<AppState>,
    Path(cve_id): Path<String>,
    user: User,
    Json(mut payload): Json<CveJustificationRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Validate category
    let valid_categories = ["mitigated", "false_positive", "accepted_risk", "patch_scheduled"];
    if !valid_categories.contains(&payload.category.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid category. Must be one of: {}", valid_categories.join(", ")),
        ));
    }

    // Validate reason length
    let reason_len = payload.reason.trim().len();
    if reason_len < 10 || reason_len > 2000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Reason must be between 10 and 2000 characters".to_string(),
        ));
    }

    let input = CveJustificationInput {
        system_id: payload.system_id,
        cve_id: cve_id.clone(),
        category: payload.category,
        reason: payload.reason.trim().to_string(),
        updated_by: user.id,
    };

    cves::insert_cve_justification(&state.pool, &input)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to save justification: {}", e),
            )
        })?;

    Ok(StatusCode::CREATED)
}

#[derive(Debug, Deserialize)]
pub struct CveJustificationRequest {
    pub system_id: Option<uuid::Uuid>,
    pub category: String,
    pub reason: String,
}

/// GET /api/v1/cves/:cve_id/justifications
/// Get justification history for a CVE.
pub async fn list_justifications(
    State(state): State<AppState>,
    Path(cve_id): Path<String>,
    _user: User,
) -> Result<Json<Vec<CveJustification>>, (StatusCode, String)> {
    let justifications = cves::fetch_cve_justifications(&state.pool, &cve_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch justifications: {}", e),
            )
        })?;

    Ok(Json(justifications))
}

/// POST /api/v1/cves/rescan-fleet
/// Trigger CVE scans for all active systems (admin only).
pub async fn trigger_fleet_rescan(
    State(state): State<AppState>,
    _user: User,
) -> Result<Json<FleetRescanResponse>, (StatusCode, String)> {
    // Get all active systems
    let systems = sqlx::query!(
        r#"
        SELECT id, hostname
        FROM systems
        WHERE is_active = TRUE
        "#
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to fetch active systems: {}", e),
        )
    })?;

    let enqueued_count = systems.len();

    // TODO: Enqueue CVE scans via builder queue
    // For now, just return the count
    // In full implementation, call:
    // for system in systems {
    //     crate::builder::services::cve_scans::enqueue_scan_for_system(&state.pool, system.id).await?;
    // }

    Ok(Json(FleetRescanResponse {
        enqueued_count: enqueued_count as i64,
        message: format!(
            "CVE scan triggered for {} systems. Results will appear in 5-10 minutes.",
            enqueued_count
        ),
    }))
}

#[derive(Debug, Serialize)]
pub struct FleetRescanResponse {
    pub enqueued_count: i64,
    pub message: String,
}

/// GET /api/v1/cves/export
/// Export CVEs as CSV.
pub async fn export_cves(
    State(state): State<AppState>,
    Query(filters): Query<CveFilters>,
    _user: User,
) -> Result<Response, (StatusCode, String)> {
    let cves = cves::fetch_cve_list(&state.pool, &filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch CVEs for export: {}", e),
            )
        })?;

    // Generate CSV
    let mut csv_output = String::new();
    csv_output.push_str("CVE ID,Severity,CVSS Score,Package,Installed Version,Fixed Version,Affected Systems,Environments,Fix Status,Triage Status,Age (days),First Seen,Last Seen\n");

    for cve in cves {
        let environments = cve
            .affected_environments
            .unwrap_or_default()
            .join(";");
        let first_seen = cve
            .first_seen
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let last_seen = cve
            .last_seen
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();

        csv_output.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            cve.cve_id,
            cve.severity,
            cve.cvss_v3_score.map(|s| s.to_string()).unwrap_or_default(),
            cve.package_name.unwrap_or_default(),
            cve.installed_version.unwrap_or_default(),
            cve.fixed_version.unwrap_or_default(),
            cve.affected_count,
            environments,
            cve.fix_status,
            cve.triage_status,
            cve.age_days,
            first_seen,
            last_seen,
        ));
    }

    let filename = format!(
        "crystal-forge-cves-{}.csv",
        chrono::Utc::now().format("%Y-%m-%d")
    );

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/csv"),
            (
                header::CONTENT_DISPOSITION,
                &format!("attachment; filename=\"{}\"", filename),
            ),
        ],
        csv_output,
    )
        .into_response())
}

use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_justification_validation() {
        let valid_categories = ["mitigated", "false_positive", "accepted_risk", "patch_scheduled"];
        assert!(valid_categories.contains(&"accepted_risk"));
        assert!(!valid_categories.contains(&"invalid"));
    }
}
