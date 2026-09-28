//! Serves the authenticated read-only projection of source-owned acceptances.
//!
//! Register visibility does not authorize waiver or CVE disposition mutations.

use axum::{
    Json,
    extract::Path,
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;

use crate::{
    api::models::FleetCvePoamRequest,
    auth::extractors::RequireAuth,
    handlers::api::poam,
    models::poam::CreatePoamRequest,
    queries::acceptance_register::{self, AcceptanceListQuery, AcceptanceReadError},
    services::{
        poam::{self as poam_service, CveAcceptanceSource, SystemClock},
        register_export_selection::{self, RegisterExportSelectionError},
        register_tabular_export::{self, AcceptanceKind, Entry, Evidence, Scope, Snapshot, Source},
    },
};

/// Chooses one supported acceptance-register download encoding.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceExportQuery {
    /// Selects `csv` or `xlsx`.
    pub format: String,
    /// Restricts the source family.
    pub source: Option<acceptance_register::AcceptanceSource>,
    /// Restricts the persisted status.
    pub status: Option<String>,
    /// Restricts the visible source scope.
    pub environment_id: Option<uuid::Uuid>,
    /// Ignored list pagination; export always includes every matching page.
    pub limit: Option<i64>,
    /// Ignored list offset; export starts at the first matching source.
    pub offset: Option<i64>,
}

/// Downloads every authorized acceptance matching the source filters.
///
/// The endpoint ignores list pagination and caps the whole read-only snapshot.
/// Policy finding UUIDs are actual source links. CVE dispositions contain no
/// scan UUID, so their scan cells remain empty rather than implying evidence.
///
/// Returns a structured error for invalid filters, unavailable source context,
/// excessive rows, or failed serialization; no partial download is returned.
pub async fn export(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    query: Result<Query<AcceptanceExportQuery>, QueryRejection>,
) -> Response {
    let Ok(Query(query)) = query else {
        return export_error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "Invalid export query",
        );
    };
    if query.format != "csv" && query.format != "xlsx" {
        return export_error(
            StatusCode::BAD_REQUEST,
            "invalid_format",
            "Unsupported export format",
        );
    }
    let actor = match poam::actor(&pool, user, &headers).await {
        Ok(actor) => actor,
        Err(error) => return error,
    };
    let selection = match register_export_selection::select_acceptances(
        &pool,
        &actor,
        &AcceptanceListQuery {
            source: query.source,
            status: query.status,
            environment_id: query.environment_id,
            limit: None,
            offset: None,
        },
    )
    .await
    {
        Ok(selection) => selection,
        Err(error) => {
            let (status, code) = match error {
                RegisterExportSelectionError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
                RegisterExportSelectionError::InvalidQuery => {
                    (StatusCode::BAD_REQUEST, "invalid_query")
                }
                RegisterExportSelectionError::TooManyRows => {
                    (StatusCode::UNPROCESSABLE_ENTITY, "export_limit")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "export_unavailable"),
            };
            return export_error(status, code, &error.to_string());
        }
    };
    let mut titles = Vec::with_capacity(selection.acceptances.len());
    let mut source_ids = Vec::with_capacity(selection.acceptances.len());
    let mut statuses = Vec::with_capacity(selection.acceptances.len());
    let mut evidence = Vec::with_capacity(selection.acceptances.len());
    for row in &selection.acceptances {
        let Some(context) = selection
            .acceptance_context
            .iter()
            .find(|item| item.source_id == row.source_id)
        else {
            return export_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "export_unavailable",
                "Export source context is unavailable",
            );
        };
        titles.push(match row.source {
            acceptance_register::AcceptanceSource::PolicyWaiver => {
                context.policy_name.clone().unwrap_or_default()
            }
            _ => format!(
                "{} / {}",
                row.canonical_cve_id.as_deref().unwrap_or(""),
                row.canonical_package_name.as_deref().unwrap_or("")
            ),
        });
        source_ids.push(row.source_id.to_string());
        statuses.push(if row.retired_at.is_some() {
            if row.replacement_poam_id.is_some() {
                "converted (retired accepted)".to_owned()
            } else {
                "retired accepted".to_owned()
            }
        } else if row.source == acceptance_register::AcceptanceSource::PolicyWaiver
            && row.replacement_poam_id.is_some()
        {
            "converted (revoked)".to_owned()
        } else {
            row.status.clone()
        });
        evidence.push(
            context
                .finding_id
                .map(|finding_id| {
                    vec![Evidence {
                        finding_id: Some(finding_id),
                        scan_id: None,
                        description: "Policy finding",
                    }]
                })
                .unwrap_or_default(),
        );
    }
    let mut entries = Vec::with_capacity(selection.acceptances.len());
    for (index, row) in selection.acceptances.iter().enumerate() {
        // Context order is not guaranteed by SQL; use the exact identity.
        let Some(context) = selection
            .acceptance_context
            .iter()
            .find(|item| item.source_id == row.source_id)
        else {
            return export_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "export_unavailable",
                "Export source context is unavailable",
            );
        };
        let scope = match (row.system_id, row.environment_id) {
            (Some(id), None) => Scope::System {
                id,
                name: &context.scope_name,
            },
            (None, Some(id)) => Scope::Environment {
                id,
                name: &context.scope_name,
            },
            _ => {
                return export_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "export_unavailable",
                    "Export source scope is unavailable",
                );
            }
        };
        let review_deadline = match row.source {
            acceptance_register::AcceptanceSource::PolicyWaiver => row.review_due_at,
            _ => row.review_date,
        };
        entries.push(Entry {
            uuid: row.source_id,
            source_id: &source_ids[index],
            title: &titles[index],
            description: &row.justification,
            status: &statuses[index],
            scope,
            cve_id: row.canonical_cve_id.as_deref(),
            source: Source::Acceptance {
                kind: if row.source == acceptance_register::AcceptanceSource::PolicyWaiver {
                    AcceptanceKind::Policy
                } else {
                    AcceptanceKind::Cve
                },
                review_deadline,
                authorization_expiry: row.expires_at,
            },
            evidence: &evidence[index],
        });
    }
    let output = match register_tabular_export::write_register(&Snapshot { entries: &entries }) {
        Ok(output) => output,
        Err(_) => {
            return export_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "export_unavailable",
                "Export cannot represent the selected source data",
            );
        }
    };
    let download = if query.format == "csv" {
        output.csv
    } else {
        output.xlsx
    };
    (
        [
            (header::CONTENT_TYPE, download.content_type),
            (
                header::CONTENT_DISPOSITION,
                if query.format == "csv" {
                    "attachment; filename=acceptance-register.csv"
                } else {
                    "attachment; filename=acceptance-register.xlsx"
                },
            ),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        download.bytes,
    )
        .into_response()
}

fn export_error(status: StatusCode, code: &'static str, message: &str) -> Response {
    (status, Json(json!({"error":code,"message":message}))).into_response()
}

/// Requires the UUID of the accepted source decision shown to the actor.
#[derive(Deserialize)]
pub struct RenewAcceptanceRequest {
    /// Detects a stale or misidentified source row before any write.
    pub expected_source_id: uuid::Uuid,
    /// Identifies the waiver's exact last status revision, when applicable.
    pub expected_waiver_updated_at: Option<DateTime<Utc>>,
}

/// Carries a source revision and source-family remediation metadata.
#[derive(Deserialize)]
pub struct ConvertAcceptanceRequest {
    /// Detects a stale or mismatched source row before mutation.
    pub expected_source_id: uuid::Uuid,
    /// Gives the waiver revision where the source can change status in place.
    pub expected_waiver_updated_at: Option<DateTime<Utc>>,
    /// Selects a policy-family plan only if its source compatibility permits reuse.
    pub reuse_poam_id: Option<uuid::Uuid>,
    /// Contains the real source-family POA&M request, never browser-built evidence.
    pub poam: serde_json::Value,
}

/// Returns an authorized page of policy waivers and CVE disposition decisions.
///
/// The decision's source type and UUID are stable identities, but the server
/// must recheck source-specific permissions and evidence before any mutation.
pub async fn list(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    query: Result<Query<AcceptanceListQuery>, QueryRejection>,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_query","message":"Invalid acceptance query"})),
            )
                .into_response();
        }
    };
    match acceptance_register::list(&pool, user.user_id, &query).await {
        Ok(page) => Json(page).into_response(),
        Err(AcceptanceReadError::Validation(message)) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_query","message":message})),
        )
            .into_response(),
        Err(AcceptanceReadError::Forbidden) => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"forbidden","message":"Insufficient permissions"})),
        )
            .into_response(),
        Err(error) => {
            tracing::error!(error=%error, "Acceptance register read failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error":"internal_error","message":"Acceptance register read failed"})),
            )
                .into_response()
        }
    }
}

/// Renews one exact policy waiver or host/environment CVE acceptance.
///
/// Dispatches to the source-owned service after session and CSRF checks. Policy
/// waivers require their last status revision; CVE decisions use the immutable
/// source UUID. A renewal retains its predecessor and cannot extend it again.
pub async fn renew(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    Path((source, id)): Path<(String, uuid::Uuid)>,
    Json(request): Json<RenewAcceptanceRequest>,
) -> Response {
    if let Err(error) = poam::csrf(&headers) {
        return error;
    }
    if request.expected_source_id != id {
        return poam::error_response(poam_service::PoamError::Conflict(
            "acceptance_source_changed",
            "The selected acceptance changed; refresh and retry".into(),
        ));
    }
    let actor = match poam::actor(&pool, user, &headers).await {
        Ok(actor) => actor,
        Err(error) => return error,
    };
    match source.as_str() {
        "policy_waiver" => {
            let Some(revision) = request.expected_waiver_updated_at else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_revision","message":"Waiver status revision is required"})),
                ).into_response();
            };
            match poam_service::renew_waiver(&pool, &actor, id, revision, &SystemClock).await {
                Ok(result) => Json(
                    json!({"source":source,"predecessor_id":result.predecessor_id,
                    "successor_id":result.successor_id,"review_deadline":result.review_due_at}),
                )
                .into_response(),
                Err(error) => poam::error_response(error),
            }
        }
        "cve_host" | "cve_environment" => {
            let typed = if source == "cve_host" {
                CveAcceptanceSource::Host(id)
            } else {
                CveAcceptanceSource::Environment(id)
            };
            match poam_service::renew_cve_acceptance(&pool, &actor, typed, id, &SystemClock).await {
                Ok(result) => Json(
                    json!({"source":source,"predecessor_id":result.predecessor_id,
                    "successor_id":result.successor_id,"review_deadline":result.review_date}),
                )
                .into_response(),
                Err(error) => poam::error_response(error),
            }
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_source","message":"Unknown acceptance source"})),
        )
            .into_response(),
    }
}

/// Converts one typed acceptance to a POA&M in its owning service transaction.
///
/// The browser cannot revoke then create a plan. Every source validates current
/// evidence and the actor again after source-specific writer locks.
pub async fn convert(
    State(pool): State<PgPool>,
    RequireAuth(user): RequireAuth,
    headers: HeaderMap,
    Path((source, id)): Path<(String, uuid::Uuid)>,
    Json(request): Json<ConvertAcceptanceRequest>,
) -> Response {
    if let Err(error) = poam::csrf(&headers) {
        return error;
    }
    if request.expected_source_id != id {
        return poam::error_response(poam_service::PoamError::Conflict(
            "acceptance_source_changed",
            "The selected acceptance changed; refresh and retry".into(),
        ));
    }
    let actor = match poam::actor(&pool, user, &headers).await {
        Ok(actor) => actor,
        Err(error) => return error,
    };
    match source.as_str() {
        "policy_waiver" => {
            let Some(revision) = request.expected_waiver_updated_at else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_revision","message":"Waiver status revision is required"})),
                ).into_response();
            };
            let poam: CreatePoamRequest = match serde_json::from_value(request.poam) {
                Ok(poam) => poam,
                Err(_) => return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_poam","message":"Invalid policy remediation metadata"})),
                ).into_response(),
            };
            match poam_service::convert_waiver_to_poam(
                &pool,
                &actor,
                id,
                revision,
                poam,
                request.reuse_poam_id,
                &SystemClock,
            )
            .await
            {
                Ok(result) => Json(json!({"source":source,"predecessor_id":result.waiver_id,
                    "poam_id":result.poam_id,"poam_reused":result.poam_reused}))
                .into_response(),
                Err(error) => poam::error_response(error),
            }
        }
        "cve_host" | "cve_environment" => {
            if request.reuse_poam_id.is_some() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_poam","message":"CVE plan compatibility is selected by its source service"})),
                ).into_response();
            }
            let poam: FleetCvePoamRequest = match serde_json::from_value(request.poam) {
                Ok(poam) => poam,
                Err(_) => return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":"invalid_poam","message":"Invalid CVE remediation metadata"})),
                ).into_response(),
            };
            let typed = if source == "cve_host" {
                CveAcceptanceSource::Host(id)
            } else {
                CveAcceptanceSource::Environment(id)
            };
            match poam_service::convert_cve_acceptance(&pool, &actor, typed, id, poam, &SystemClock)
                .await
            {
                Ok(result) => Json(
                    json!({"source":source,"predecessor_id":result.predecessor_id,
                    "successor_id":result.successor_id,"poam_id":result.poam_id,
                    "poam_reused":result.poam_reused}),
                )
                .into_response(),
                Err(error) => poam::error_response(error),
            }
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"invalid_source","message":"Unknown acceptance source"})),
        )
            .into_response(),
    }
}
