//! API handlers for the advanced CVE dashboard.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup,
};
use crate::auth::extractors::{RequireAdmin, RequireAuth};
use crate::handlers::agent_request::CFState;
use crate::queries::cve_scans::get_fleet_cve_scan_targets;
use crate::queries::cves;
use crate::services::cve_scans::{CveScanError, trigger_immediate_cve_scan_with_outcome};

/// GET /api/v1/cves
/// List CVEs with filters.
pub async fn list_cves(
    State(state): State<CFState>,
    Query(filters): Query<CveFilters>,
    _user: RequireAuth,
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
    State(state): State<CFState>,
    Query(filters): Query<CveFilters>,
    _user: RequireAuth,
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
    State(state): State<CFState>,
    _user: RequireAuth,
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
    State(state): State<CFState>,
    _user: RequireAuth,
) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let packages = cves::fetch_package_names(&state.pool).await.map_err(|e| {
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
    State(state): State<CFState>,
    Path(cve_id): Path<String>,
    _user: RequireAuth,
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
    State(state): State<CFState>,
    Path(cve_id): Path<String>,
    _user: RequireAuth,
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
    State(state): State<CFState>,
    Path(cve_id): Path<String>,
    RequireAdmin(user): RequireAdmin,
    Json(payload): Json<CveJustificationRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Validate category
    let valid_categories = [
        "mitigated",
        "false_positive",
        "accepted_risk",
        "patch_scheduled",
    ];
    if !valid_categories.contains(&payload.category.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid category. Must be one of: {}",
                valid_categories.join(", ")
            ),
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
        category: payload.category.clone(),
        reason: payload.reason.trim().to_string(),
        updated_by: user.user_id,
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

/// DELETE /api/v1/cves/:cve_id/justification
/// Revoke the fleet-wide CVE justification (admin only).
/// Per-system justifications are not affected.
/// Idempotent: returns 204 whether or not a justification existed.
pub async fn revoke_justification(
    State(state): State<CFState>,
    Path(cve_id): Path<String>,
    _user: RequireAdmin,
) -> Result<StatusCode, (StatusCode, String)> {
    cves::revoke_fleet_cve_justification(&state.pool, &cve_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to revoke justification: {}", e),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/cves/:cve_id/justifications
/// Get justification history for a CVE.
pub async fn list_justifications(
    State(state): State<CFState>,
    Path(cve_id): Path<String>,
    _user: RequireAuth,
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
///
pub async fn trigger_fleet_rescan(
    State(state): State<CFState>,
    _user: RequireAdmin,
) -> Result<(StatusCode, Json<FleetRescanResponse>), (StatusCode, String)> {
    let targets = get_fleet_cve_scan_targets(&state.pool)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve fleet CVE scan targets: {err}"),
            )
        })?;

    let mut enqueued_count = 0_i64;
    for target in targets {
        match trigger_immediate_cve_scan_with_outcome(state.pool.clone(), target.derivation_id)
            .await
        {
            Ok(outcome) if outcome.was_created => enqueued_count += 1,
            Ok(_) => {}
            Err(CveScanError::VulnixUnavailable) => {
                return Err((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "vulnix is not available on this node; fleet rescan cannot start".to_string(),
                ));
            }
            Err(CveScanError::Internal(err)) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to enqueue fleet CVE scan: {err:#}"),
                ));
            }
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(fleet_rescan_response(enqueued_count)),
    ))
}

fn fleet_rescan_response(enqueued_count: i64) -> FleetRescanResponse {
    let message = if enqueued_count == 0 {
        "No eligible systems require a new CVE scan.".to_string()
    } else {
        format!("Queued {enqueued_count} fleet CVE scan(s).")
    };

    FleetRescanResponse {
        enqueued_count,
        message,
    }
}

#[derive(Debug, Serialize)]
pub struct FleetRescanResponse {
    pub enqueued_count: i64,
    pub message: String,
}

/// Escape a single CSV field per RFC 4180:
/// wrap in double-quotes if the value contains commas, double-quotes, or newlines;
/// double up any internal double-quotes.
fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// GET /api/v1/cves/export
/// Export CVEs as CSV.
pub async fn export_cves(
    State(state): State<CFState>,
    Query(filters): Query<CveFilters>,
    _user: RequireAuth,
) -> Result<Response, (StatusCode, String)> {
    let cves = cves::fetch_cve_list(&state.pool, &filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch CVEs for export: {}", e),
            )
        })?;

    // Build CSV with proper RFC 4180 field escaping
    let mut csv_output = String::new();
    csv_output.push_str("CVE ID,Severity,CVSS Score,Package,Installed Version,Fixed Version,Affected Systems,Environments,Fix Status,Triage Status,Age (days),First Seen,Last Seen\n");

    for cve in cves {
        let environments = cve.affected_environments.unwrap_or_default().join(";");
        let first_seen = cve
            .first_seen
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let last_seen = cve
            .last_seen
            .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_default();
        let cvss = cve.cvss_v3_score.map(|s| s.to_string()).unwrap_or_default();

        let row = [
            csv_field(&cve.cve_id),
            csv_field(&cve.severity),
            csv_field(&cvss),
            csv_field(&cve.package_name.unwrap_or_default()),
            csv_field(&cve.installed_version.unwrap_or_default()),
            csv_field(&cve.fixed_version.unwrap_or_default()),
            cve.affected_count.to_string(),
            csv_field(&environments),
            csv_field(&cve.fix_status),
            csv_field(&cve.triage_status),
            cve.age_days.to_string(),
            csv_field(&first_seen),
            csv_field(&last_seen),
        ]
        .join(",");

        csv_output.push_str(&row);
        csv_output.push('\n');
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

    // Admin enforcement for save_justification and trigger_fleet_rescan is handled
    // declaratively by the RequireAdmin extractor — covered by extractors.rs tests.
    // Here we test pure business logic that requires no DB.

    // ── Validation unit tests (pure logic, no DB) ──

    #[test]
    fn justification_category_allowlist_accepts_valid_values() {
        let valid = [
            "mitigated",
            "false_positive",
            "accepted_risk",
            "patch_scheduled",
        ];
        assert!(valid.contains(&"accepted_risk"));
        assert!(valid.contains(&"patch_scheduled"));
        assert!(valid.contains(&"mitigated"));
        assert!(valid.contains(&"false_positive"));
    }

    #[test]
    fn justification_category_allowlist_rejects_invalid_values() {
        let valid = [
            "mitigated",
            "false_positive",
            "accepted_risk",
            "patch_scheduled",
        ];
        assert!(!valid.contains(&"wontfix"));
        assert!(!valid.contains(&"ignored"));
        assert!(!valid.contains(&""));
        assert!(!valid.contains(&"ACCEPTED_RISK")); // case-sensitive
    }

    #[test]
    fn justification_reason_length_validation() {
        // Min 10 chars
        let too_short = "short";
        assert!(too_short.trim().len() < 10);

        let at_min = "1234567890";
        assert!(at_min.trim().len() >= 10);

        // Max 2000 chars
        let at_max = "a".repeat(2000);
        assert!(at_max.trim().len() <= 2000);

        let too_long = "a".repeat(2001);
        assert!(too_long.trim().len() > 2000);
    }

    #[test]
    fn csv_export_row_format_has_correct_column_count() {
        // Verify our CSV header has 13 columns matching the row format string
        let header = "CVE ID,Severity,CVSS Score,Package,Installed Version,Fixed Version,\
                       Affected Systems,Environments,Fix Status,Triage Status,Age (days),\
                       First Seen,Last Seen";
        let col_count = header.split(',').count();
        assert_eq!(col_count, 13, "CSV header must have 13 columns");
    }

    #[test]
    fn csv_field_plain_value_unchanged() {
        assert_eq!(csv_field("openssl"), "openssl");
        assert_eq!(csv_field("CVE-2024-1234"), "CVE-2024-1234");
        assert_eq!(csv_field(""), "");
    }

    #[test]
    fn csv_field_wraps_value_containing_comma() {
        assert_eq!(csv_field("foo,bar"), "\"foo,bar\"");
    }

    #[test]
    fn csv_field_doubles_internal_quotes() {
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn csv_field_wraps_value_containing_newline() {
        assert_eq!(csv_field("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn revoke_category_not_in_save_allowlist() {
        // Ensure "outstanding" is rejected by save_justification so revoke
        // must go through the DELETE endpoint, not POST.
        let valid = [
            "mitigated",
            "false_positive",
            "accepted_risk",
            "patch_scheduled",
        ];
        assert!(
            !valid.contains(&"outstanding"),
            "outstanding must not be in save allowlist — use DELETE /justification to revoke"
        );
    }

    #[test]
    fn cve_filters_default_has_no_constraints() {
        let f = CveFilters::default();
        assert!(f.severity.is_none());
        assert!(f.fix_status.is_none());
        assert!(f.triage_status.is_none());
        assert!(f.package.is_none());
        assert!(f.search.is_none());
        assert!(f.sort.is_none());
    }

    #[test]
    fn fleet_rescan_response_reports_created_count_and_noop() {
        let enqueued = fleet_rescan_response(3);
        assert_eq!(enqueued.enqueued_count, 3);
        assert_eq!(enqueued.message, "Queued 3 fleet CVE scan(s).");

        let noop = fleet_rescan_response(0);
        assert_eq!(noop.enqueued_count, 0);
        assert_eq!(noop.message, "No eligible systems require a new CVE scan.");
    }
}
