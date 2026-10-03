//! Downloads complete authorized POA&M and source-decision register snapshots.

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::extractors::RequireAuth,
    handlers::api::poam,
    models::poam::PoamListQuery,
    queries::acceptance_register::{AcceptanceListQuery, AcceptanceSource},
    services::{
        poam::SystemClock,
        register_export_selection::{self, RegisterExportSelectionError, RegisterRecordType},
        register_mixed_tabular, register_oscal_snapshot,
    },
};

/// Selects a file format and independent source-family filters.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterExportQuery {
    /// Chooses `csv`, `xlsx`, `oscal-json`, or `oscal-xml`.
    pub format: String,
    /// Selects both source families by default, or only plans or acceptances.
    #[serde(default)]
    pub record_type: RegisterRecordType,
    /// Restricts POA&M lifecycle state.
    pub poam_status: Option<String>,
    /// Restricts POA&M risk.
    pub poam_risk: Option<String>,
    /// Restricts POA&M owner.
    pub poam_owner: Option<String>,
    /// Restricts POA&M linked systems.
    pub poam_system_id: Option<Uuid>,
    /// Restricts POA&M policy lineage.
    pub poam_policy_lineage_id: Option<Uuid>,
    /// Restricts POA&M bundle lineage.
    pub poam_bundle_id: Option<Uuid>,
    /// Restricts POA&M requirement.
    pub poam_requirement: Option<String>,
    /// Restricts POA&M overdue state.
    pub poam_overdue: Option<bool>,
    /// Searches source POA&M fields.
    pub poam_q: Option<String>,
    /// Restricts acceptance source family.
    pub acceptance_source: Option<AcceptanceSource>,
    /// Restricts acceptance source-native status.
    pub acceptance_status: Option<String>,
    /// Restricts acceptance visibility to an environment.
    pub acceptance_environment_id: Option<Uuid>,
}

fn error(status: StatusCode, code: &'static str, message: &str) -> Response {
    (status, Json(json!({"error":code,"message":message}))).into_response()
}

/// Downloads all matching plans and decisions from one authorization snapshot.
///
/// Each format contains the complete scoped source population, not browser
/// pages or display-group copies. No export is returned when the scope cannot
/// be represented without hidden links, fabricated evidence, or truncation.
pub async fn export(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<RegisterExportQuery>, QueryRejection>,
) -> Response {
    let Ok(Query(query)) = query else {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "Invalid register export query",
        );
    };
    if !matches!(
        query.format.as_str(),
        "csv" | "xlsx" | "oscal-json" | "oscal-xml"
    ) {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_format",
            "Unsupported export format",
        );
    }
    if (query.record_type == RegisterRecordType::Plans
        && (query.acceptance_source.is_some()
            || query.acceptance_status.is_some()
            || query.acceptance_environment_id.is_some()))
        || (query.record_type == RegisterRecordType::Acceptances
            && (query.poam_status.is_some()
                || query.poam_risk.is_some()
                || query.poam_owner.is_some()
                || query.poam_system_id.is_some()
                || query.poam_policy_lineage_id.is_some()
                || query.poam_bundle_id.is_some()
                || query.poam_requirement.is_some()
                || query.poam_overdue.is_some()
                || query.poam_q.is_some()))
    {
        return error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "Filter targets an excluded record type",
        );
    }
    let actor = match poam::actor(&pool, user, &headers).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };
    let selection = register_export_selection::select_scoped(
        &pool,
        &actor,
        &PoamListQuery {
            status: query.poam_status,
            risk: query.poam_risk,
            owner: query.poam_owner,
            system_id: query.poam_system_id,
            policy_lineage_id: query.poam_policy_lineage_id,
            bundle_id: query.poam_bundle_id,
            requirement: query.poam_requirement,
            overdue: query.poam_overdue,
            q: query.poam_q,
            ..Default::default()
        },
        &AcceptanceListQuery {
            source: query.acceptance_source,
            status: query.acceptance_status,
            environment_id: query.acceptance_environment_id,
            ..Default::default()
        },
        &SystemClock,
        query.record_type,
    )
    .await;
    let selection = match selection {
        Ok(selection) => selection,
        Err(failure) => {
            let (status, code) = match failure {
                RegisterExportSelectionError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
                RegisterExportSelectionError::InvalidQuery => {
                    (StatusCode::BAD_REQUEST, "invalid_query")
                }
                RegisterExportSelectionError::TooManyRows => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "export_limit")
                }
                RegisterExportSelectionError::CandidateScanLimit => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "candidate_limit")
                }
                RegisterExportSelectionError::PartialContext => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "incomplete_scope")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "export_unavailable"),
            };
            return error(status, code, &failure.to_string());
        }
    };
    // OSCAL 1.1.2 requires at least one POA&M item. A genuinely empty
    // authorized scope has no document to download; never invent an item.
    if query.format.starts_with("oscal-")
        && selection.poams.is_empty()
        && selection.acceptances.is_empty()
    {
        return StatusCode::NO_CONTENT.into_response();
    }
    let (bytes, content_type, filename) = match query.format.as_str() {
        "csv" | "xlsx" => {
            let output = match register_mixed_tabular::write_authorized(&selection) {
                Ok(output) => output,
                Err(_) => {
                    return error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "export_unavailable",
                        "Selection cannot be represented as a complete spreadsheet",
                    );
                }
            };
            let file = if query.format == "csv" {
                output.csv
            } else {
                output.xlsx
            };
            (
                file.bytes,
                file.content_type,
                if query.format == "csv" {
                    "attachment; filename=register.csv"
                } else {
                    "attachment; filename=register.xlsx"
                },
            )
        }
        _ => {
            let output = match register_oscal_snapshot::write_authorized(&selection) {
                Ok(output) => output,
                Err(_) => {
                    return error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "export_unavailable",
                        "Selection cannot be represented as a complete OSCAL document",
                    );
                }
            };
            if query.format == "oscal-json" {
                (
                    output.json.into_bytes(),
                    "application/json",
                    "attachment; filename=register.oscal.json",
                )
            } else {
                (
                    output.xml.into_bytes(),
                    "application/xml",
                    "attachment; filename=register.oscal.xml",
                )
            }
        }
    };
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, filename),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        bytes,
    )
        .into_response()
}
