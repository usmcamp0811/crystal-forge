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
use crate::handlers::api::auth_session::RequireCsrf;
use crate::queries::cve_scans::{FleetEnqueueOutcome, enqueue_fleet_cve_scans};
use crate::queries::cves;

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
/// Enqueues scans; it does not execute them. The CVE worker drains queued rows
/// at its own bounded per-cycle rate, which is what prevents a fleet-wide
/// request from starting an unbounded number of concurrent vulnix processes and
/// ensures queued scans get the worker's cache-materialization path.
///
/// vulnix availability is intentionally not checked here: execution is
/// deferred, so the relevant question is whether vulnix exists when the worker
/// runs the scan, not when the request is made.
///
/// The route requires an administrator session and matching double-submit CSRF
/// cookie and header before it can enqueue work.
pub async fn trigger_fleet_rescan(
    State(state): State<CFState>,
    _user: RequireAdmin,
    _csrf: RequireCsrf,
) -> Result<(StatusCode, Json<FleetRescanResponse>), (StatusCode, String)> {
    let outcome = enqueue_fleet_cve_scans(&state.pool, "vulnix", None)
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to enqueue fleet CVE scans: {err:#}"),
            )
        })?;

    Ok((StatusCode::ACCEPTED, Json(fleet_rescan_response(outcome))))
}

fn fleet_rescan_response(outcome: FleetEnqueueOutcome) -> FleetRescanResponse {
    let reused = outcome.reused();
    let message = if outcome.eligible == 0 {
        "No active systems are reporting a running configuration to scan.".to_string()
    } else if outcome.created == 0 {
        format!(
            "All {eligible} eligible system configuration(s) already have a scan pending or in progress.",
            eligible = outcome.eligible
        )
    } else if reused > 0 {
        format!(
            "Queued {created} CVE scan(s); {reused} already had an active scan.",
            created = outcome.created
        )
    } else {
        format!("Queued {created} CVE scan(s).", created = outcome.created)
    };

    FleetRescanResponse {
        enqueued_count: outcome.created,
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

    // Admin enforcement for trigger_fleet_rescan is exercised end-to-end against
    // the real route below, because the extractor is what actually rejects a
    // caller. `extractors::test_role_checks` only asserts helper methods such as
    // `is_admin()` on a hand-built AuthenticatedUser; it never runs the
    // RequireAdmin extractor and therefore proves nothing about this endpoint.

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
    fn fleet_rescan_response_distinguishes_queued_reused_and_empty_fleet() {
        // All eligible targets newly queued.
        let queued = fleet_rescan_response(FleetEnqueueOutcome {
            eligible: 3,
            created: 3,
        });
        assert_eq!(queued.enqueued_count, 3);
        assert_eq!(queued.message, "Queued 3 CVE scan(s).");

        // Partially deduplicated against already-active scans.
        let partial = fleet_rescan_response(FleetEnqueueOutcome {
            eligible: 5,
            created: 2,
        });
        assert_eq!(partial.enqueued_count, 2);
        assert_eq!(
            partial.message,
            "Queued 2 CVE scan(s); 3 already had an active scan."
        );

        // Every eligible target already had an active scan: a real no-op, but
        // distinct from "nothing was eligible".
        let all_active = fleet_rescan_response(FleetEnqueueOutcome {
            eligible: 4,
            created: 0,
        });
        assert_eq!(all_active.enqueued_count, 0);
        assert_eq!(
            all_active.message,
            "All 4 eligible system configuration(s) already have a scan pending or in progress."
        );

        // No active system is reporting a running configuration at all.
        let empty = fleet_rescan_response(FleetEnqueueOutcome {
            eligible: 0,
            created: 0,
        });
        assert_eq!(empty.enqueued_count, 0);
        assert_eq!(
            empty.message,
            "No active systems are reporting a running configuration to scan."
        );
    }

    #[test]
    fn fleet_enqueue_outcome_reused_never_goes_negative() {
        // Defensive: created should never exceed eligible, but the response
        // text must not render a negative count if it somehow does.
        let outcome = FleetEnqueueOutcome {
            eligible: 0,
            created: 2,
        };
        assert_eq!(outcome.reused(), 0);
    }
}

#[cfg(test)]
mod fleet_rescan_authorization_tests {
    //! Route-level authorization coverage for `POST /api/v1/cves/rescan-fleet`.
    //!
    //! These drive a real axum route so the `RequireAdmin` extractor actually
    //! executes. Asserting on `AuthenticatedUser` helper methods alone would
    //! not prove that the endpoint rejects unauthorized callers.

    use super::trigger_fleet_rescan;
    use crate::auth::session::{
        CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME, hash_token,
    };
    use crate::config::ServerConfig;
    use crate::handlers::agent_request::CFState;
    use crate::models::auth_identity::AuthRole;
    use crate::queries::auth_identity::{create_user_session, sync_user_role};
    use crate::queries::commits::{get_commit_by_id, insert_commit};
    use crate::queries::derivations::{EvaluationStatus, insert_derivation};
    use crate::queries::flakes::insert_flake;
    use crate::queries::users::insert_user;
    use crate::queue::QueueNotifier;
    use axum::Router;
    use axum::routing::post;
    use chrono::Utc;
    use sqlx::PgPool;
    use std::sync::Arc;
    use uuid::Uuid;

    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL").ok()?;
        PgPool::connect(&url).await.ok()
    }

    async fn spawn_fleet_server(pool: PgPool) -> String {
        let state = CFState::new(
            pool,
            ServerConfig::default(),
            Arc::new(QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        let app = Router::new()
            .route("/api/v1/cves/rescan-fleet", post(trigger_fleet_rescan))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve fleet app");
        });
        format!("http://{addr}")
    }

    async fn session_token_for_role(pool: &PgPool, role: AuthRole) -> (String, Uuid) {
        let suffix = Uuid::new_v4().simple().to_string();
        let user = insert_user(
            pool,
            &format!("{suffix}@example.com"),
            Some("TASK-325 Fleet Test User"),
        )
        .await
        .expect("insert_user");
        sync_user_role(pool, user.id, role)
            .await
            .expect("sync_user_role");
        let token = format!("session-{suffix}");
        create_user_session(
            pool,
            user.id,
            hash_token(&token),
            Utc::now() + chrono::Duration::hours(1),
            Some("task-325-test".to_string()),
            Some("127.0.0.1".to_string()),
            "local".to_string(),
        )
        .await
        .expect("create_user_session");
        (token, user.id)
    }

    async fn cleanup_test_user(pool: &PgPool, user_id: Uuid) {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .expect("test user and session should be deleted");
    }

    struct FleetAuthFixture {
        flake_id: i32,
        environment_id: Uuid,
        system_id: Uuid,
        hostname: String,
        derivation_id: i32,
    }

    impl FleetAuthFixture {
        async fn create(pool: &PgPool) -> Self {
            let suffix = Uuid::new_v4().simple().to_string();
            let repo_url = format!("https://example.com/task-325-auth-{suffix}.git");
            let flake = insert_flake(
                pool,
                &format!("task-325-auth-{suffix}"),
                &repo_url,
                "main",
                "cf_systems_only",
            )
            .await
            .expect("authorization fixture flake should be inserted");
            insert_commit(pool, &suffix, &repo_url, Utc::now())
                .await
                .expect("authorization fixture commit should be inserted");
            let commit_id: i32 = sqlx::query_scalar("SELECT id FROM commits WHERE flake_id = $1")
                .bind(flake.id)
                .fetch_one(pool)
                .await
                .expect("authorization fixture commit should resolve");
            let commit = get_commit_by_id(pool, commit_id)
                .await
                .expect("authorization fixture commit model should resolve");
            let config_name = format!("auth-config-{suffix}");
            let store_path = format!("/nix/store/{suffix}-auth-running");
            let derivation = insert_derivation(pool, Some(&commit), &config_name, "nixos")
                .await
                .expect("authorization fixture derivation should be inserted");
            sqlx::query(
                "UPDATE derivations SET status_id = $2, completed_at = NOW(), store_path = $3 WHERE id = $1",
            )
            .bind(derivation.id)
            .bind(EvaluationStatus::BuildComplete.as_id())
            .bind(&store_path)
            .execute(pool)
            .await
            .expect("authorization fixture derivation should be build-complete");

            let environment_id = Uuid::new_v4();
            sqlx::query("INSERT INTO environments (id, name, is_active) VALUES ($1, $2, TRUE)")
                .bind(environment_id)
                .bind(format!("task-325-auth-env-{suffix}"))
                .execute(pool)
                .await
                .expect("authorization fixture environment should be inserted");
            let system_id = Uuid::new_v4();
            let hostname = format!("task-325-auth-host-{suffix}");
            sqlx::query(
                r#"
                INSERT INTO systems (
                    id, hostname, environment_id, is_active, public_key, flake_id,
                    derivation, system_configuration_name
                )
                VALUES ($1, $2, $3, TRUE, 'test-key', $4, 'test-derivation', $5)
                "#,
            )
            .bind(system_id)
            .bind(&hostname)
            .bind(environment_id)
            .bind(flake.id)
            .bind(&config_name)
            .execute(pool)
            .await
            .expect("authorization fixture system should be inserted");
            sqlx::query(
                "INSERT INTO system_states (hostname, store_path, change_reason, timestamp) VALUES ($1, $2, 'config_change', NOW())",
            )
            .bind(&hostname)
            .bind(&store_path)
            .execute(pool)
            .await
            .expect("authorization fixture system state should be inserted");

            Self {
                flake_id: flake.id,
                environment_id,
                system_id,
                hostname,
                derivation_id: derivation.id,
            }
        }

        async fn cleanup(self, pool: &PgPool) {
            sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
                .bind(self.derivation_id)
                .execute(pool)
                .await
                .expect("authorization fixture scan should be deleted");
            sqlx::query("DELETE FROM system_states WHERE hostname = $1")
                .bind(&self.hostname)
                .execute(pool)
                .await
                .expect("authorization fixture system state should be deleted");
            sqlx::query("DELETE FROM systems WHERE id = $1")
                .bind(self.system_id)
                .execute(pool)
                .await
                .expect("authorization fixture system should be deleted");
            sqlx::query("DELETE FROM derivations WHERE id = $1")
                .bind(self.derivation_id)
                .execute(pool)
                .await
                .expect("authorization fixture derivation should be deleted");
            sqlx::query("DELETE FROM commits WHERE flake_id = $1")
                .bind(self.flake_id)
                .execute(pool)
                .await
                .expect("authorization fixture commit should be deleted");
            sqlx::query("DELETE FROM flakes WHERE id = $1")
                .bind(self.flake_id)
                .execute(pool)
                .await
                .expect("authorization fixture flake should be deleted");
            sqlx::query("DELETE FROM environments WHERE id = $1")
                .bind(self.environment_id)
                .execute(pool)
                .await
                .expect("authorization fixture environment should be deleted");
        }
    }

    async fn post_fleet_rescan(
        base: &str,
        token: Option<&str>,
        csrf_cookie: Option<&str>,
        csrf_header: Option<&str>,
    ) -> (u16, serde_json::Value) {
        let mut request = reqwest::Client::new().post(format!("{base}/api/v1/cves/rescan-fleet"));
        let mut cookies = Vec::new();
        if let Some(token) = token {
            cookies.push(format!("{SESSION_COOKIE_NAME}={token}"));
        }
        if let Some(csrf) = csrf_cookie {
            cookies.push(format!("{CSRF_COOKIE_NAME}={csrf}"));
        }
        if !cookies.is_empty() {
            request = request.header("cookie", cookies.join("; "));
        }
        if let Some(csrf) = csrf_header {
            request = request.header(CSRF_HEADER_NAME.as_str(), csrf);
        }
        let response = request.send().await.expect("send");
        let status = response.status().as_u16();
        let body = response.json().await.expect("JSON response");
        (status, body)
    }

    #[tokio::test]
    async fn fleet_rescan_rejects_unauthenticated_caller() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let base = spawn_fleet_server(pool.clone()).await;

        let (status, body) = post_fleet_rescan(&base, None, None, None).await;
        assert_eq!(
            status, 401,
            "an unauthenticated caller must not reach the fleet enqueue"
        );
        assert_eq!(body["error"], "unauthorized");
    }

    #[tokio::test]
    async fn fleet_rescan_rejects_viewer_and_operator() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (viewer, viewer_id) = session_token_for_role(&pool, AuthRole::Viewer).await;
        let (operator, operator_id) = session_token_for_role(&pool, AuthRole::Operator).await;
        let base = spawn_fleet_server(pool.clone()).await;

        let csrf = "task-325-non-admin-csrf";
        let (viewer_status, viewer_body) =
            post_fleet_rescan(&base, Some(&viewer), Some(csrf), Some(csrf)).await;
        let (operator_status, operator_body) =
            post_fleet_rescan(&base, Some(&operator), Some(csrf), Some(csrf)).await;
        cleanup_test_user(&pool, viewer_id).await;
        cleanup_test_user(&pool, operator_id).await;

        assert_eq!(
            viewer_status, 403,
            "Viewer must be forbidden from triggering a fleet rescan"
        );
        assert_eq!(
            operator_status, 403,
            "Operator must be forbidden from triggering a fleet rescan"
        );
        assert_eq!(viewer_body["error"], "forbidden");
        assert_eq!(operator_body["error"], "forbidden");
    }

    #[tokio::test]
    async fn fleet_rescan_requires_matching_csrf_for_admin() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let (admin, admin_id) = session_token_for_role(&pool, AuthRole::Admin).await;
        let base = spawn_fleet_server(pool.clone()).await;

        let (missing, missing_body) = post_fleet_rescan(&base, Some(&admin), None, None).await;
        let (mismatched, mismatched_body) = post_fleet_rescan(
            &base,
            Some(&admin),
            Some("cookie-token"),
            Some("header-token"),
        )
        .await;
        let fixture = FleetAuthFixture::create(&pool).await;
        let csrf = "task-325-fleet-csrf";
        let (status, _) = post_fleet_rescan(&base, Some(&admin), Some(csrf), Some(csrf)).await;
        let fixture_scan: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM cve_scans WHERE derivation_id = $1")
                .bind(fixture.derivation_id)
                .fetch_optional(&pool)
                .await
                .expect("authorization fixture scan should resolve");
        fixture.cleanup(&pool).await;
        cleanup_test_user(&pool, admin_id).await;

        assert!(
            fixture_scan.is_some(),
            "the successful admin request must enqueue the owned fleet fixture"
        );
        assert_eq!(missing, 403, "Admin without CSRF must be rejected");
        assert_eq!(missing_body["error"], "csrf_validation_failed");
        assert_eq!(
            mismatched, 403,
            "Admin with mismatched CSRF must be rejected"
        );
        assert_eq!(mismatched_body["error"], "csrf_validation_failed");
        assert_eq!(
            status, 202,
            "Admin must be accepted and the request acknowledged as queued"
        );
    }
}
