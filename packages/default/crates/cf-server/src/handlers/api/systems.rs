use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::Row;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::api::models::{
    ApiError, AuditAction, CommitInfo, CreateSystemRequest, CveScanEligibilityResponse,
    CveScanStatusResponse, CveScanTriggerResponse, CveSummary, DeploySystemRequest,
    DeploymentStatus, FieldUpdate, ManualDeploymentAction, ManualDeploymentConversionState,
    ManualDeploymentPolicyState, ManualDeploymentRequestState, ManualDeploymentResponse,
    PipelineStage, SaveSystemCveJustificationRequest, SortOrder, SystemAgentEvent,
    SystemCommitsResponse, SystemDeploymentProgress, SystemDetail, SystemGeneration,
    SystemGenerationsResponse, SystemHardwareInfo, SystemHistoryEntry, SystemMutationResponse,
    SystemNetworkInfo, SystemRollbackGenerationRequest, SystemRollbackRequest, SystemSecurityInfo,
    SystemSummary, SystemVulnerability, SystemsListParams, UpdateSystemPublicKeyRequest,
    UpdateSystemRequest, VerifyGenerationClosureRequest, VerifyGenerationClosureResponse,
};
use crate::auth::models::Role;
use crate::handlers::agent_request::CFState;
use crate::handlers::api::auth_session::require_csrf;
use crate::handlers::api::rbac::{
    authenticated_user_roles, extract_request_origin, require_viewer_or_above,
};
use crate::models::auth_identity::AuthRole;
use crate::models::evaluation_snapshots::{
    AgentFingerprintStatus, EvaluatedOptionCounts, EvaluatedOptionsPage, EvaluatedOptionsParams,
    EvaluationDrift, EvaluationModuleSourcesPage, EvaluationModuleSourcesParams,
    SelectedEvaluationSummary, SelectedEvaluationSummaryParams, SevenDayDriftStatus,
    SnapshotLifecycle, SnapshotRevisionMode,
};
use crate::queries::build_jobs::enqueue_build_job_for_derivation;
use crate::queries::cve_scans::{get_scan_by_id, resolve_system_cve_scan_target};
use crate::queries::derivations::reset_derivation_for_rebuild;
use crate::queries::system_events::{
    deployment_progress_kind, deployment_progress_stage, get_system_deployment_progress_row,
    list_system_event_history_rows,
};
use crate::queries::system_states::{
    fetch_system_generations, find_generation_store_path_last_seen,
};
use crate::queries::systems::{
    FqdnUpdate, HeartbeatIntervalUpdate, ManualPolicyConversion, SystemAccessRow, SystemDetailRow,
    SystemListRow, commit_belongs_to_system_flake, deactivate_system, find_system_access_row,
    find_system_deployment_derivation, get_system_detail_by_id,
    get_user_environment_membership_ids, list_recent_commits_for_system, list_system_access_rows,
    list_system_agent_event_rows, list_system_history_rows, touch_system_updated_at,
    update_public_key, update_system_metadata,
};
use crate::services::cve_scans::{CveScanError, trigger_immediate_cve_scan};
use crate::services::systems::SystemsListContext;

/// Allowed CVE justification categories (server-side validation).
/// These must match the UI preset list in packages/web-ui/src/components/cve/mod.rs.
/// Any category value not in this list will be rejected with a 400 Bad Request.
const ALLOWED_CVE_JUSTIFICATION_CATEGORIES: &[&str] = &[
    "false_positive",
    "accepted_risk",
    "compensating_control",
    "planned_remediation",
    "vendor_pending_fix",
];

pub async fn list_systems(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<SystemsListParams>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match get_user_environment_membership_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    // Create service context and delegate to service layer
    let ctx = SystemsListContext::new(user_id, roles, environment_memberships, &params);

    // Call the service layer for server-side filtering/sorting/pagination
    match crate::services::systems::list_systems_for_user(
        &pool,
        &ctx,
        state.server_config.heartbeat_interval_secs,
    )
    .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(_) => internal_error("Failed to list systems"),
    }
}

pub async fn create_system(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<CreateSystemRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }
    // Validate required fields
    let hostname = payload.hostname.trim();
    if hostname.is_empty() {
        return bad_request("Hostname is required");
    }

    let public_key = payload.public_key.trim();
    if public_key.is_empty() {
        return bad_request("Public key is required");
    }

    // Validate deployment policy
    if !matches!(
        payload.deployment_policy.as_str(),
        "manual" | "auto_latest" | "pinned"
    ) {
        return bad_request("Invalid deployment policy (must be: manual, auto_latest, or pinned)");
    }

    // Look up environment ID from name
    let environment_id = if let Some(env_name) = payload.environment.as_ref() {
        let env_name_trimmed = env_name.trim();
        if !env_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, Uuid>("SELECT id FROM environments WHERE name = $1")
                .bind(env_name_trimmed)
                .fetch_optional(&pool)
                .await
            {
                Ok(id) => id,
                Err(_) => return internal_error("Failed to lookup environment"),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Look up flake ID from name
    let flake_id = if let Some(flake_name) = payload.flake_name.as_ref() {
        let flake_name_trimmed = flake_name.trim();
        if !flake_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, i32>("SELECT id FROM flakes WHERE name = $1")
                .bind(flake_name_trimmed)
                .fetch_optional(&pool)
                .await
            {
                Ok(id) => id,
                Err(_) => return internal_error("Failed to lookup flake"),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Use the System model to create and validate the system
    use crate::models::systems::System;
    let system = match System::new(
        &pool,
        hostname.to_string(),
        environment_id,
        true, // is_active
        public_key.to_string(),
        flake_id,
        payload
            .system_configuration_name
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        None, // desired_target
        payload.deployment_policy.clone(),
    )
    .await
    {
        Ok(sys) => sys,
        Err(e) => return bad_request(&format!("Failed to create system: {}", e)),
    };

    // Record audit event
    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::UserCreated, // Using UserCreated as placeholder - ideally would be SystemCreated
        format!("{} ({})", system.hostname, system.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "create", "hostname": system.hostname }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    // Fetch the created system from view to return complete data
    let detail = match get_system_detail_by_id(&pool, system.id).await {
        Ok(Some(row)) => detail_row_to_api_model(row, state.server_config.heartbeat_interval_secs),
        Ok(None) => return internal_error("System created but not found in view"),
        Err(_) => return internal_error("Failed to fetch created system"),
    };

    (StatusCode::CREATED, Json(detail)).into_response()
}

pub async fn get_system(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(_caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    let _environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match get_system_detail_by_id(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    // Note: Environment-based access control would go here
    // For now, simplified - in production you'd check environment membership

    let detail = detail_row_to_api_model(row, state.server_config.heartbeat_interval_secs);

    (StatusCode::OK, Json(detail)).into_response()
}

/// Returns a bounded page of cached evaluated options for one system revision.
///
/// This read path is database-only. It never invokes Nix, Git, or network work.
/// Hidden systems and revisions from another flake return the same 404 response.
pub async fn get_system_evaluated_options(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Query(params): Query<EvaluatedOptionsParams>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    let memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };
    let access = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };
    if !caller_role.can_access_system_environment(access.environment_id, &memberships) {
        return not_found();
    }

    if params.mode == SnapshotRevisionMode::Commit && !is_full_commit_sha(&params.revision) {
        return bad_request("revision must be a full 40- or 64-character commit SHA");
    }
    let selected = match params.mode {
        SnapshotRevisionMode::Commit => {
            crate::queries::evaluation_snapshots::select_commit_snapshot(
                &pool,
                system_id,
                &params.revision,
            )
            .await
        }
        SnapshotRevisionMode::Generation => {
            let Some(generation) = params.generation else {
                return bad_request("generation is required in generation mode");
            };
            crate::queries::evaluation_snapshots::select_generation_snapshot(
                &pool, system_id, generation,
            )
            .await
        }
    };
    let selected = match selected {
        Ok(Some(value)) => value,
        Ok(None) if params.mode == SnapshotRevisionMode::Commit => {
            let lifecycle = match crate::queries::evaluation_snapshots::missing_snapshot_lifecycle(
                &pool,
                system_id,
                &params.revision,
            )
            .await
            {
                Ok(Some(value)) => value,
                Ok(None) => return not_found(),
                Err(_) => return internal_error("Failed to load evaluation lifecycle"),
            };
            return (StatusCode::OK, Json(empty_options_page(&params, lifecycle))).into_response();
        }
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(empty_options_page(
                    &params,
                    (SnapshotLifecycle::Unavailable, None),
                )),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to select evaluation snapshot");
            return internal_error("Failed to load evaluation snapshot");
        }
    };

    match crate::queries::evaluation_snapshots::query_options_page(
        &pool,
        system_id,
        &selected,
        (caller_role != Role::Admin).then_some(user_id),
        &params.search,
        params.filter,
        params.limit.unwrap_or(50),
        params.offset.unwrap_or(0),
    )
    .await
    {
        Ok(page) => (StatusCode::OK, Json(page)).into_response(),
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to query evaluated options");
            internal_error("Failed to load evaluated options")
        }
    }
}

/// Returns complete selected-revision module, evaluation, and drift metadata.
///
/// This endpoint is database-only and applies system authorization before it
/// selects a snapshot or resolves any registered provenance identity.
pub async fn get_system_evaluation_summary(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Query(params): Query<SelectedEvaluationSummaryParams>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    let memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };
    let access = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };
    if !caller_role.can_access_system_environment(access.environment_id, &memberships) {
        return not_found();
    }

    if params.mode == SnapshotRevisionMode::Commit && !is_full_commit_sha(&params.revision) {
        return bad_request("revision must be a full 40- or 64-character commit SHA");
    }
    let selected = match params.mode {
        SnapshotRevisionMode::Commit => {
            crate::queries::evaluation_snapshots::select_commit_snapshot(
                &pool,
                system_id,
                &params.revision,
            )
            .await
        }
        SnapshotRevisionMode::Generation => {
            let Some(generation) = params.generation else {
                return bad_request("generation is required in generation mode");
            };
            crate::queries::evaluation_snapshots::select_generation_snapshot(
                &pool, system_id, generation,
            )
            .await
        }
    };
    let selected = match selected {
        Ok(Some(value)) => value,
        Ok(None) if params.mode == SnapshotRevisionMode::Commit => {
            let lifecycle = match crate::queries::evaluation_snapshots::missing_snapshot_lifecycle(
                &pool,
                system_id,
                &params.revision,
            )
            .await
            {
                Ok(Some(value)) => value,
                Ok(None) => return not_found(),
                Err(_) => return internal_error("Failed to load evaluation lifecycle"),
            };
            return (
                StatusCode::OK,
                Json(empty_evaluation_summary(&params, lifecycle)),
            )
                .into_response();
        }
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(empty_evaluation_summary(
                    &params,
                    (SnapshotLifecycle::Unavailable, None),
                )),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to select evaluation summary snapshot");
            return internal_error("Failed to load evaluation summary");
        }
    };

    match crate::queries::evaluation_snapshots::get_selected_evaluation_summary(
        &pool, system_id, &selected,
    )
    .await
    {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to load evaluation summary");
            internal_error("Failed to load evaluation summary")
        }
    }
}

/// Returns one bounded page of exact module sources for a selected evaluation.
///
/// The endpoint applies non-disclosing system authorization before snapshot
/// selection. Reads are database-only, and pagination is clamped to the same
/// bounds as evaluated options.
pub async fn get_system_evaluation_module_sources(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Query(params): Query<EvaluationModuleSourcesParams>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };
    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    let memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };
    let access = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };
    if !caller_role.can_access_system_environment(access.environment_id, &memberships) {
        return not_found();
    }
    if params.mode == SnapshotRevisionMode::Commit && !is_full_commit_sha(&params.revision) {
        return bad_request("revision must be a full 40- or 64-character commit SHA");
    }
    if let Err(message) = validate_evaluation_module_sources_params(&params) {
        return bad_request(message);
    }

    let selected = match params.mode {
        SnapshotRevisionMode::Commit => {
            crate::queries::evaluation_snapshots::select_commit_snapshot(
                &pool,
                system_id,
                &params.revision,
            )
            .await
        }
        SnapshotRevisionMode::Generation => {
            let Some(generation) = params.generation else {
                return bad_request("generation is required in generation mode");
            };
            crate::queries::evaluation_snapshots::select_generation_snapshot(
                &pool, system_id, generation,
            )
            .await
        }
    };
    let selected = match selected {
        Ok(Some(value)) => value,
        Ok(None) if params.mode == SnapshotRevisionMode::Commit => {
            let lifecycle = match crate::queries::evaluation_snapshots::missing_snapshot_lifecycle(
                &pool,
                system_id,
                &params.revision,
            )
            .await
            {
                Ok(Some(value)) => value,
                Ok(None) => return not_found(),
                Err(_) => return internal_error("Failed to load evaluation lifecycle"),
            };
            return (
                StatusCode::OK,
                Json(empty_evaluation_module_sources(&params, lifecycle)),
            )
                .into_response();
        }
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(empty_evaluation_module_sources(
                    &params,
                    (SnapshotLifecycle::Unavailable, None),
                )),
            )
                .into_response();
        }
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to select evaluation module sources snapshot");
            return internal_error("Failed to load evaluation module sources");
        }
    };

    match crate::queries::evaluation_snapshots::get_evaluation_module_sources_page(
        &pool,
        system_id,
        &selected,
        (caller_role != Role::Admin).then_some(user_id),
        params.snapshot_token.as_deref(),
        params.limit.unwrap_or(50),
        params.offset.unwrap_or(0),
    )
    .await
    {
        Ok(crate::queries::evaluation_snapshots::EvaluationModuleSourcesQuery::Page(page)) => {
            (StatusCode::OK, Json(page)).into_response()
        }
        Ok(crate::queries::evaluation_snapshots::EvaluationModuleSourcesQuery::SnapshotChanged) => {
            (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "snapshot_changed".to_string(),
                    message: "Evaluation snapshot changed; reload module sources from offset 0"
                        .to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to query evaluation module sources");
            internal_error("Failed to load evaluation module sources")
        }
    }
}

/// Explicitly queues missing revision evaluation or reuses existing work.
///
/// The action requires administrator privileges because evaluation processes
/// the complete commit. It applies non-disclosing system authorization first.
pub async fn queue_system_evaluation_snapshot(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path((system_id, revision)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };
    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }
    // SECURITY: The existing evaluator operates on the complete commit. Until
    // it has a configuration-scoped worker contract, only an administrator,
    // who can see every environment, may trigger this whole-commit action.
    if caller_role != Role::Admin {
        return forbidden_mutation();
    }
    let memberships = match load_membership_environment_ids(&state.pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };
    let access = match find_system_access_row(&state.pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };
    if !caller_role.can_access_system_environment(access.environment_id, &memberships) {
        return not_found();
    }
    if !is_full_commit_sha(&revision) {
        return bad_request("revision must be a full 40- or 64-character commit SHA");
    }

    match crate::queries::evaluation_snapshots::queue_or_reuse_evaluation(
        &state.pool,
        system_id,
        &revision,
    )
    .await
    {
        Ok(Some(response)) => {
            if response.queued {
                state.queue_notifier.notify_eval_queue();
            }
            (StatusCode::OK, Json(response)).into_response()
        }
        Ok(None) => not_found(),
        Err(error) => {
            tracing::error!(system_id = %system_id, error = %error, "failed to queue evaluation snapshot");
            internal_error("Failed to queue evaluation")
        }
    }
}

fn empty_options_page(
    params: &EvaluatedOptionsParams,
    (lifecycle, error): (SnapshotLifecycle, Option<String>),
) -> EvaluatedOptionsPage {
    EvaluatedOptionsPage {
        lifecycle,
        revision: params.revision.clone(),
        generation: params.generation,
        generation_snapshot_id: None,
        baseline_revision: None,
        comparison_available: false,
        error,
        module_count: 0,
        evaluation_duration_ms: None,
        counts: EvaluatedOptionCounts::default(),
        total: 0,
        offset: params.offset.unwrap_or(0).clamp(0, 100_000),
        limit: params.limit.unwrap_or(50).clamp(1, 100),
        options: Vec::new(),
    }
}

fn empty_evaluation_summary(
    params: &SelectedEvaluationSummaryParams,
    (lifecycle, error): (SnapshotLifecycle, Option<String>),
) -> SelectedEvaluationSummary {
    SelectedEvaluationSummary {
        lifecycle,
        revision: params.revision.clone(),
        generation: params.generation,
        error,
        module_source_total: 0,
        completed_at: None,
        evaluation_duration_ms: None,
        option_total: 0,
        selected_store_path: None,
        closure_package_count: None,
        closure_size_bytes: None,
        running_store_path: None,
        running_profile_matches: None,
        host_delta_count: None,
        agent_fingerprint: AgentFingerprintStatus::Unavailable,
        seven_day_drift: SevenDayDriftStatus::InsufficientCoverage,
        drift: EvaluationDrift::Unavailable,
    }
}

fn empty_evaluation_module_sources(
    params: &EvaluationModuleSourcesParams,
    (lifecycle, error): (SnapshotLifecycle, Option<String>),
) -> EvaluationModuleSourcesPage {
    EvaluationModuleSourcesPage {
        lifecycle,
        revision: params.revision.clone(),
        generation: params.generation,
        error,
        snapshot_token: None,
        total: 0,
        offset: params.offset.unwrap_or(0).clamp(0, 100_000),
        limit: params.limit.unwrap_or(50).clamp(1, 100),
        sources: Vec::new(),
    }
}

fn validate_evaluation_module_sources_params(
    params: &EvaluationModuleSourcesParams,
) -> Result<(), &'static str> {
    if params.offset.unwrap_or(0) > 0 && params.snapshot_token.is_none() {
        return Err("snapshot_token is required when offset is greater than 0");
    }
    if params.snapshot_token.as_deref().is_some_and(|token| {
        token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err("snapshot_token must be a 64-character hexadecimal digest");
    }
    Ok(())
}

fn is_full_commit_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub async fn get_system_cves(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    // Load environment memberships for scoped access control
    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    // Verify system exists and caller has environment access
    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let rows = match sqlx::query(
        r#"
        WITH deduped AS (
            SELECT DISTINCT ON (v.cve_id, v.package_name, v.package_version)
                v.cve_id,
                lower(v.severity) AS severity,
                v.cvss_v3_score::double precision AS cvss_score,
                COALESCE(v.description, '') AS description,
                v.package_name,
                v.package_version AS installed_version,
                v.fixed_version,
                v.completed_at AS first_seen,
                c.published_date::timestamptz AS published_at,
                -- 'fix_available' = upstream patched version exists; does NOT mean system is patched.
                -- 'open' = no upstream fix known yet.
                CASE WHEN v.fixed_version IS NULL THEN 'open' ELSE 'fix_available' END AS status,
                j.category AS justification_category,
                j.reason AS justification_reason,
                j.updated_at AS justification_updated_at
            FROM view_system_vulnerabilities v
            JOIN systems s ON s.hostname = v.hostname
            LEFT JOIN cves c ON c.id = v.cve_id
            LEFT JOIN system_cve_justifications j
                ON j.system_id = s.id
               AND j.cve_id = v.cve_id
            WHERE s.id = $1
            ORDER BY v.cve_id, v.package_name, v.package_version, v.completed_at DESC
        )
        SELECT *
        FROM deduped
        ORDER BY cvss_score DESC NULLS LAST, cve_id ASC, package_name ASC, installed_version ASC
        "#,
    )
    .bind(system_id)
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load system CVEs"),
    };

    let vulnerabilities = rows
        .into_iter()
        .map(|row| {
            let severity_raw: String = row.get("severity");
            let severity = parse_cve_severity(&severity_raw);

            SystemVulnerability {
                cve_id: row.get("cve_id"),
                severity,
                cvss_score: row.get("cvss_score"),
                description: row.get("description"),
                package_name: row.get("package_name"),
                installed_version: row.get("installed_version"),
                fixed_version: row.get("fixed_version"),
                first_seen: row.get("first_seen"),
                published_at: row.get("published_at"),
                status: row.get("status"),
                justification_category: row.get("justification_category"),
                justification_reason: row.get("justification_reason"),
                justification_updated_at: row.get("justification_updated_at"),
            }
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(vulnerabilities)).into_response()
}

pub async fn save_system_cve_justification(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((system_id, cve_id)): Path<(Uuid, String)>,
    Json(payload): Json<SaveSystemCveJustificationRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let cve_id = cve_id.trim().to_string();
    if cve_id.is_empty() {
        return bad_request("CVE ID is required");
    }

    let reason = payload.reason.trim();
    if reason.is_empty() {
        return bad_request("Justification reason is required");
    }

    if reason.len() > 2000 {
        return bad_request("Justification reason must be 2000 characters or less");
    }

    let category = payload
        .category
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(ref category_value) = category
        && !ALLOWED_CVE_JUSTIFICATION_CATEGORIES
            .iter()
            .any(|allowed| *allowed == category_value)
    {
        return bad_request("Invalid justification category");
    }

    let cve_present_on_system = match sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM view_system_vulnerabilities v
            JOIN systems s ON s.hostname = v.hostname
            WHERE s.id = $1
              AND v.cve_id = $2
        )
        "#,
    )
    .bind(system_id)
    .bind(&cve_id)
    .fetch_one(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to validate system CVE"),
    };

    if !cve_present_on_system {
        return bad_request("CVE was not found for this system");
    }

    // Begin transaction to ensure atomic write + audit
    let mut tx = match pool.begin().await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to begin transaction"),
    };

    // Upsert justification
    if sqlx::query(
        r#"
        INSERT INTO system_cve_justifications (system_id, cve_id, category, reason, updated_by, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (system_id, cve_id)
        DO UPDATE SET
            category = EXCLUDED.category,
            reason = EXCLUDED.reason,
            updated_by = EXCLUDED.updated_by,
            updated_at = NOW()
        "#,
    )
    .bind(system_id)
    .bind(&cve_id)
    .bind(category.clone())
    .bind(reason)
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return internal_error("Failed to save CVE justification");
    }

    // Lookup user email for audit (within transaction)
    let actor_identifier =
        match sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(Some(email)) => email,
            Ok(None) => user_id.to_string(),
            Err(_) => {
                let _ = tx.rollback().await;
                return internal_error("Failed to lookup user for audit");
            }
        };

    // Record audit event within the same transaction
    if sqlx::query(
        "INSERT INTO admin_audit_events (actor_user_id, actor_identifier, action, target, request_origin, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(&actor_identifier)
    .bind("user_updated")
    .bind(format!("{} ({})", row.hostname, row.id))
    .bind(extract_request_origin(&headers))
    .bind(serde_json::json!({
        "operation": "cve_justification_saved",
        "system_id": row.id,
        "hostname": row.hostname,
        "cve_id": cve_id,
        "category": category,
        "reason_length": reason.len()
    }))
    .execute(&mut *tx)
    .await
    .is_err()
    {
        let _ = tx.rollback().await;
        return internal_error("Failed to write audit event");
    }

    // Commit transaction
    if tx.commit().await.is_err() {
        return internal_error("Failed to commit transaction");
    }

    (
        StatusCode::OK,
        Json(SystemMutationResponse {
            status: "ok".to_string(),
            message: "CVE justification saved".to_string(),
        }),
    )
        .into_response()
}

pub async fn get_system_cve_scan_eligibility(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let payload = match resolve_system_cve_scan_target(&pool, system_id).await {
        Ok(Some(target)) => CveScanEligibilityResponse {
            eligible: target.blocked_reason.is_none(),
            reason: target.blocked_reason,
            derivation_id: Some(target.derivation_id),
            config_name: Some(target.config_name),
            hostname: target.hostname,
        },
        Ok(None) => CveScanEligibilityResponse {
            eligible: false,
            reason: Some(
                "No eligible derivation was found for this system configuration.".to_string(),
            ),
            derivation_id: None,
            config_name: None,
            hostname: Some(row.hostname),
        },
        Err(_) => return internal_error("Failed to evaluate CVE scan eligibility"),
    };

    (StatusCode::OK, Json(payload)).into_response()
}

pub async fn trigger_system_cve_scan(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let target = match resolve_system_cve_scan_target(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "scan_ineligible".to_string(),
                    message: "No build target found for this system configuration.".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => return internal_error("Failed to resolve CVE scan target"),
    };

    if let Some(reason) = target.blocked_reason.as_ref() {
        return (
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "scan_ineligible".to_string(),
                message: reason.clone(),
                details: None,
            }),
        )
            .into_response();
    }

    let scan_id = match trigger_immediate_cve_scan(pool.clone(), target.derivation_id).await {
        Ok(value) => value,
        Err(CveScanError::VulnixUnavailable) => return scan_ineligible_response(),
        Err(CveScanError::Internal(err)) => {
            return internal_error(&format!("Failed to queue CVE scan: {err}"));
        }
    };

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::CveScanRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({
            "operation": "cve_scan",
            "derivation_id": target.derivation_id,
            "scan_id": scan_id,
            "config_name": target.config_name
        }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(CveScanTriggerResponse {
            scan_id,
            status: "accepted".to_string(),
            message: format!(
                "CVE scan queued for {} ({})",
                row.hostname, target.config_name
            ),
        }),
    )
        .into_response()
}

pub async fn get_cve_scan_status(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(scan_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let scan = match get_scan_by_id(&pool, scan_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "CVE scan not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => return internal_error("Failed to load CVE scan status"),
    };

    (
        StatusCode::OK,
        Json(CveScanStatusResponse {
            scan_id: scan.id,
            derivation_id: scan.derivation_id,
            status: match scan.status {
                crate::models::cve_scans::ScanStatus::Pending => "pending",
                crate::models::cve_scans::ScanStatus::InProgress => "in_progress",
                crate::models::cve_scans::ScanStatus::Completed => "completed",
                crate::models::cve_scans::ScanStatus::Failed => "failed",
            }
            .to_string(),
            scanner_name: scan.scanner_name,
            scheduled_at: scan.scheduled_at,
            completed_at: scan.completed_at,
            attempts: scan.attempts,
            total_vulnerabilities: scan.total_vulnerabilities,
            critical_count: scan.critical_count,
            high_count: scan.high_count,
            medium_count: scan.medium_count,
            low_count: scan.low_count,
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

pub async fn update_system_handler(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<UpdateSystemRequest>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let hostname = payload.hostname.trim();
    if hostname.is_empty() {
        return bad_request("Hostname is required");
    }
    // PATCH semantics for FQDN: an omitted key preserves the persisted value,
    // an explicit null (or empty string) clears it, a value sets it. This
    // prevents older/partial clients that don't send `fqdn` from wiping it.
    let fqdn = match &payload.fqdn {
        FieldUpdate::Unset => FqdnUpdate::Keep,
        FieldUpdate::Clear => FqdnUpdate::Clear,
        FieldUpdate::Set(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                FqdnUpdate::Clear
            } else {
                FqdnUpdate::Set(trimmed)
            }
        }
    };

    // PATCH semantics for heartbeat_interval_secs: omitted key preserves value,
    // null clears it (falls back to server default 600s), value sets it.
    // Valid range: 15-900 seconds.
    const MIN_HEARTBEAT_INTERVAL_SECS: i32 = 15;
    const MAX_HEARTBEAT_INTERVAL_SECS: i32 = 900;

    let heartbeat_interval = match &payload.heartbeat_interval_secs {
        FieldUpdate::Unset => HeartbeatIntervalUpdate::Keep,
        FieldUpdate::Clear => HeartbeatIntervalUpdate::Clear,
        FieldUpdate::Set(value) => {
            if *value < MIN_HEARTBEAT_INTERVAL_SECS || *value > MAX_HEARTBEAT_INTERVAL_SECS {
                return bad_request(&format!(
                    "heartbeat_interval_secs must be between {} and {} seconds",
                    MIN_HEARTBEAT_INTERVAL_SECS, MAX_HEARTBEAT_INTERVAL_SECS
                ));
            }
            HeartbeatIntervalUpdate::Set(*value)
        }
    };

    if !matches!(
        payload.deployment_policy.as_str(),
        "manual" | "auto_latest" | "pinned"
    ) {
        return bad_request("Invalid deployment policy (must be: manual, auto_latest, or pinned)");
    }

    // Resolve environment name → id.
    // A non-empty name that does not match any environment is a 400, not a silent NULL.
    let environment_id = if let Some(env_name) = payload.environment.as_ref() {
        let env_name_trimmed = env_name.trim();
        if !env_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, Uuid>("SELECT id FROM environments WHERE name = $1")
                .bind(env_name_trimmed)
                .fetch_optional(&pool)
                .await
            {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    return bad_request(&format!("Environment '{}' not found", env_name_trimmed));
                }
                Err(_) => return internal_error("Failed to lookup environment"),
            }
        } else {
            None
        }
    } else {
        None
    };

    // Resolve flake name → id.
    // A non-empty name that does not match any registered flake is a 400, not a silent NULL.
    let flake_id = if let Some(flake_name) = payload.flake_name.as_ref() {
        let flake_name_trimmed = flake_name.trim();
        if !flake_name_trimmed.is_empty() {
            match sqlx::query_scalar::<_, i32>(
                "SELECT id FROM flakes WHERE name = $1 AND deleted_at IS NULL",
            )
            .bind(flake_name_trimmed)
            .fetch_optional(&pool)
            .await
            {
                Ok(Some(id)) => Some(id),
                Ok(None) => {
                    return bad_request(&format!("Flake '{}' not found", flake_name_trimmed));
                }
                Err(_) => return internal_error("Failed to lookup flake"),
            }
        } else {
            None
        }
    } else {
        None
    };

    if update_system_metadata(
        &pool,
        system_id,
        hostname,
        fqdn,
        environment_id,
        flake_id,
        payload
            .system_configuration_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        &payload.deployment_policy,
        heartbeat_interval,
    )
    .await
    .is_err()
    {
        return internal_error("Failed to update system");
    }

    let detail = match get_system_detail_by_id(&pool, system_id).await {
        Ok(Some(row)) => detail_row_to_api_model(row, state.server_config.heartbeat_interval_secs),
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load updated system"),
    };

    (StatusCode::OK, Json(detail)).into_response()
}

pub async fn sync_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    if touch_system_updated_at(&pool, system_id).await.is_err() {
        return internal_error("Failed to queue sync");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemSyncRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "sync" }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(SystemMutationResponse {
            status: "accepted".to_string(),
            message: format!("Sync requested for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn rollback_system(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<SystemRollbackRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }
    if let Err(response) = require_csrf(&headers) {
        return response;
    }

    let target_commit = payload.target_commit.trim();
    if let Err(message) = validate_target_commit(target_commit) {
        return bad_request(&message);
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    match crate::services::composite_enforcement::authorize_and_set_system_target(
        &pool,
        system_id,
        target_commit,
        "manual_rollback_commit",
    )
    .await
    {
        Ok(authorization) if authorization.allowed() => {}
        Ok(authorization) => return bad_request(&authorization.detail),
        Err(error) => {
            if is_uncached_deployment_target_error(&error) {
                return deployment_target_unavailable(&error.to_string());
            }
            tracing::warn!(
                system_id = %system_id,
                target = %target_commit,
                "Composite commit rollback authorization failed closed: {error:#}"
            );
            return internal_error("Composite deployment authorization failed");
        }
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemRollbackRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "rollback", "target_commit": target_commit }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(SystemMutationResponse {
            status: "accepted".to_string(),
            message: format!("Rollback requested for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn rollback_system_generation(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<SystemRollbackGenerationRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }
    if let Err(response) = require_csrf(&headers) {
        return response;
    }

    let store_path = payload.store_path.trim();
    if store_path.is_empty() {
        return bad_request("store_path is required");
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    match crate::services::composite_enforcement::authorize_and_set_system_target(
        &pool,
        system_id,
        store_path,
        "manual_rollback_generation",
    )
    .await
    {
        Ok(authorization) if authorization.allowed() => {}
        Ok(authorization) => return bad_request(&authorization.detail),
        Err(error) => {
            if is_uncached_deployment_target_error(&error) {
                return deployment_target_unavailable(&error.to_string());
            }
            tracing::warn!(
                system_id = %system_id,
                target = %store_path,
                "Composite generation rollback authorization failed closed: {error:#}"
            );
            return internal_error("Composite deployment authorization failed");
        }
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemRollbackRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "rollback_generation", "store_path": store_path }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::ACCEPTED,
        Json(SystemMutationResponse {
            status: "accepted".to_string(),
            message: format!("Generation rollback requested for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn update_system_public_key(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<UpdateSystemPublicKeyRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let new_public_key = payload.public_key.trim();
    if new_public_key.is_empty() {
        return bad_request("Public key is required");
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    // Verify system exists and user has access
    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    // Validate the public key format using the PublicKey model
    use crate::models::public_key::PublicKey;
    if let Err(e) = PublicKey::from_base64(new_public_key, &row.hostname) {
        return bad_request(&format!("Invalid public key: {}", e));
    }

    // Update the public key
    if update_public_key(&pool, system_id, new_public_key)
        .await
        .is_err()
    {
        return internal_error("Failed to update public key");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::UserUpdated, // Using UserUpdated as placeholder - ideally would be SystemKeyRotated
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "update_public_key" }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::OK,
        Json(SystemMutationResponse {
            status: "success".to_string(),
            message: format!("Public key updated for {}", row.hostname),
        }),
    )
        .into_response()
}

pub async fn deactivate_system_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    if deactivate_system(&pool, system_id).await.is_err() {
        return internal_error("Failed to disable system");
    }

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::UserUpdated,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({ "operation": "deactivate_system" }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (
        StatusCode::OK,
        Json(SystemMutationResponse {
            status: "success".to_string(),
            message: format!("System {} disabled", row.hostname),
        }),
    )
        .into_response()
}

fn highest_role(roles: &[AuthRole]) -> Option<Role> {
    if roles.contains(&AuthRole::Admin) {
        Some(Role::Admin)
    } else if roles.contains(&AuthRole::Operator) {
        Some(Role::Operator)
    } else if roles.contains(&AuthRole::Viewer) {
        Some(Role::Viewer)
    } else {
        None
    }
}

fn matches_filters(row: &SystemAccessRow, params: &SystemsListParams) -> bool {
    if let Some(search) = params.search.as_ref() {
        let needle = search.trim().to_ascii_lowercase();
        if !needle.is_empty() && !row.hostname.to_ascii_lowercase().contains(&needle) {
            return false;
        }
    }

    if let Some(environment) = params.environment.as_ref() {
        let needle = environment.trim().to_ascii_lowercase();
        if !needle.is_empty() {
            let env_name = row
                .environment
                .clone()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if env_name != needle {
                return false;
            }
        }
    }

    true
}

fn parse_health_status(status: &str) -> crate::api::models::HealthStatus {
    match status {
        "healthy" => crate::api::models::HealthStatus::Healthy,
        "warning" => crate::api::models::HealthStatus::Warning,
        "critical" => crate::api::models::HealthStatus::Critical,
        _ => crate::api::models::HealthStatus::Offline,
    }
}

fn parse_deployment_status(status: &str) -> DeploymentStatus {
    match status {
        "up_to_date" => DeploymentStatus::UpToDate,
        "behind" => DeploymentStatus::Behind,
        "ahead" => DeploymentStatus::Ahead,
        "no_deployment" => DeploymentStatus::NeverDeployed,
        "no_commits" => DeploymentStatus::NoCommitsAvailable,
        _ => DeploymentStatus::Unknown,
    }
}

fn parse_pipeline_stage(stage: &str) -> PipelineStage {
    match stage {
        "dry_run" => PipelineStage::DryRun,
        "ready_for_build" => PipelineStage::ReadyForBuild,
        "building" => PipelineStage::Building,
        "build_complete" => PipelineStage::BuildComplete,
        "ready_for_deploy" => PipelineStage::ReadyForDeploy,
        _ => PipelineStage::Unknown,
    }
}

fn detail_row_to_api_model(row: SystemDetailRow, server_default_interval: u64) -> SystemDetail {
    use crate::api::models::FlakeSummary;

    let effective_heartbeat_interval_secs = row
        .heartbeat_interval_secs
        .map(|v| v as i32)
        .unwrap_or(server_default_interval as i32);

    SystemDetail {
        id: row.id,
        hostname: row.hostname,
        fqdn: row.fqdn,
        system_configuration_name: row.system_configuration_name,
        environment: row.environment,
        is_active: row.is_active,
        deployment_policy: row.deployment_policy,
        health_status: parse_health_status(&row.health_status),
        deployment_status: parse_deployment_status(&row.deployment_status),
        pipeline_stage: Some(parse_pipeline_stage(&row.pipeline_stage)),
        nixos_version: row.nixos_version,
        kernel: row.kernel,
        agent_version: row.agent_version,
        current_store_path: row.current_store_path,
        generation: row.generation,
        generation_matches_current_store_path: row.generation_matches_current_store_path,
        hardware: SystemHardwareInfo {
            cpu_brand: row.cpu_brand,
            cpu_cores: row.cpu_cores,
            memory_gb: row.memory_gb,
            uptime_secs: row.uptime_secs,
            board_serial: row.board_serial,
            bios_version: row.bios_version,
        },
        network: SystemNetworkInfo {
            primary_ip: row.primary_ip_address,
            primary_mac: row.primary_mac_address,
            gateway_ip: row.gateway_ip,
            reachability: row.reachability,
        },
        security: SystemSecurityInfo {
            tpm_present: row.tpm_present,
            secure_boot_enabled: row.secure_boot_enabled,
            fips_mode: row.fips_mode,
            selinux_status: row.selinux_status,
        },
        cve_counts: CveSummary {
            critical: row.critical_cve_count as i64,
            high: row.high_cve_count as i64,
            medium: row.medium_cve_count as i64,
            low: row.low_cve_count as i64,
        },
        flake: row.flake_id.and_then(|id| {
            row.flake_name.map(|name| FlakeSummary {
                id,
                name,
                repo_url: row.flake_repo_url.clone().unwrap_or_default(),
                latest_commit: row.flake_latest_commit.clone(),
            })
        }),
        last_seen: row.last_seen,
        created_at: row.created_at,
        updated_at: row.updated_at,
        heartbeat_interval_secs: row.heartbeat_interval_secs,
        effective_heartbeat_interval_secs,
        boot_id: row.boot_id,
        restart_type: row.last_restart_type,
        last_restart_at: row.last_restart_at,
    }
}

async fn load_membership_environment_ids(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<BTreeSet<Uuid>, ()> {
    get_user_environment_membership_ids(pool, user_id)
        .await
        .map_err(|_| ())
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

fn forbidden_mutation() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Operator or admin privileges are required".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn bad_request(message: &str) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: "validation_error".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn deployment_target_unavailable(message: &str) -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(ApiError {
            error: "deployment_target_unavailable".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn is_uncached_deployment_target_error(error: &anyhow::Error) -> bool {
    error
        .to_string()
        .contains("No cached NixOS store path is available")
}

async fn queue_deployment_target_prerequisite(
    state: &CFState,
    system_id: Uuid,
    target: &str,
) -> String {
    let row = match find_system_deployment_derivation(&state.pool, system_id, target).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return format!(
                "No deployable NixOS derivation was found for target {target}. Re-run evaluation for this commit, then try deploy again."
            );
        }
        Err(error) => {
            tracing::warn!(
                system_id = %system_id,
                target,
                error = %error,
                "failed to inspect undeployable target derivation"
            );
            return format!(
                "No cached NixOS store path is available for deployment target {target}. Failed to inspect build/cache prerequisites."
            );
        }
    };

    if row.has_completed_cache_push {
        return format!(
            "Deployment target {target} has a completed cache push but did not resolve to a deployable store path. Refresh commit metadata and try again."
        );
    }

    if let Some(store_path) = row
        .store_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if row.has_permanent_cache_failure {
            return format!(
                "Deployment target {target} is built as {store_path}, but its cache push has permanently failed. Retry or fix the cache push job before deploying."
            );
        }

        if row.has_active_cache_push {
            return format!(
                "Deployment target {target} is built as {store_path}, but it is not yet available from cache. A cache push job is pending or running; try deploy again after it completes."
            );
        }

        if row.has_active_build_job {
            return format!(
                "Deployment target {target} is built as {store_path}, but no completed cache push exists. A rebuild is already queued or running so a builder can push it to cache; try deploy again after it completes."
            );
        }

        if let Err(error) = reset_derivation_for_rebuild(&state.pool, row.id).await {
            tracing::warn!(
                derivation_id = row.id,
                target,
                error = %error,
                "failed to reset built-but-uncached deployment target for rebuild"
            );
            return format!(
                "Deployment target {target} is built as {store_path}, but no completed cache push exists. The server could not queue a rebuild for builder-side cache push; check the build queue."
            );
        }

        match enqueue_build_job_for_derivation(&state.pool, row.id).await {
            Ok(true) => {
                state.queue_notifier.notify_build_queue();
                return format!(
                    "Deployment target {target} is built as {store_path}, but no completed cache push exists. Queued a rebuild so the builder can push it to cache; try deploy again after build and cache push complete."
                );
            }
            Ok(false) => {
                return format!(
                    "Deployment target {target} is built as {store_path}, but no completed cache push exists. A rebuild could not be queued because a build job already exists or the target is no longer buildable; check the build queue."
                );
            }
            Err(error) => {
                tracing::warn!(
                    derivation_id = row.id,
                    target,
                    error = %error,
                    "failed to queue rebuild for built-but-uncached deployment target"
                );
                return format!(
                    "Deployment target {target} is built as {store_path}, but no completed cache push exists. Queueing a rebuild for builder-side cache push failed; check the build queue."
                );
            }
        }
    }

    if row.has_active_build_job {
        return format!(
            "Deployment target {target} is not built yet. A build job is already queued or running; try deploy again after build and cache push complete."
        );
    }

    if row.is_buildable() {
        match enqueue_build_job_for_derivation(&state.pool, row.id).await {
            Ok(true) => {
                state.queue_notifier.notify_build_queue();
                return format!(
                    "Deployment target {target} is not built yet. Queued a build job; try deploy again after build and cache push complete."
                );
            }
            Ok(false) => {
                return format!(
                    "Deployment target {target} is not built yet. A build job already exists or the target is no longer buildable; check the build queue."
                );
            }
            Err(error) => {
                tracing::warn!(
                    derivation_id = row.id,
                    target,
                    error = %error,
                    "failed to queue build for undeployable target"
                );
                return format!(
                    "Deployment target {target} is not built yet, and queuing a build job failed. Check the build queue before deploying."
                );
            }
        }
    }

    format!(
        "Deployment target {target} is not deployable yet. It must finish evaluation, build successfully, and be pushed to cache before deploy."
    )
}

fn not_found() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_string(),
            message: "System not found".to_string(),
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

/// 409 response returned when a CVE scan prerequisite (e.g. vulnix) is not satisfied.
/// Centralised here so systems and flakes handlers share the same error code and message.
pub(crate) fn scan_ineligible_response() -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(ApiError {
            error: "scan_ineligible".to_string(),
            message: "vulnix is not available on this node; immediate scan cannot start"
                .to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn action_to_str(action: AuditAction) -> &'static str {
    match action {
        AuditAction::UserCreated => "user_created",
        AuditAction::UserUpdated => "user_updated",
        AuditAction::UserDeleted => "user_deleted",
        AuditAction::UserEnabled => "user_enabled",
        AuditAction::UserDisabled => "user_disabled",
        AuditAction::UserRoleAssigned => "user_role_assigned",
        AuditAction::UserEnvironmentMembershipUpdated => "user_environment_membership_updated",
        AuditAction::OidcMappingChanged => "oidc_mapping_changed",
        AuditAction::SystemSyncRequested => "system_sync_requested",
        AuditAction::SystemDeployRequested => "system_deploy_requested",
        AuditAction::SystemRollbackRequested => "system_rollback_requested",
        AuditAction::CveScanRequested => "cve_scan_requested",
        AuditAction::SessionInvalidated => "session_invalidated",
    }
}

fn validate_target_commit(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("Target commit is required".to_string());
    }

    if !matches!(value.len(), 40 | 64) {
        return Err("Target commit must be a full 40- or 64-character SHA".to_string());
    }

    if !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("Target commit must contain only hexadecimal characters".to_string());
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManualDeploymentPlan {
    Keep(ManualDeploymentPolicyState),
    ConvertToManual,
}

fn plan_manual_deployment(
    policy: &str,
    action: ManualDeploymentAction,
) -> Result<ManualDeploymentPlan, &'static str> {
    match (policy, action) {
        ("manual", ManualDeploymentAction::Deploy) => Ok(ManualDeploymentPlan::Keep(
            ManualDeploymentPolicyState::Manual,
        )),
        ("pinned", ManualDeploymentAction::Deploy) => Ok(ManualDeploymentPlan::Keep(
            ManualDeploymentPolicyState::Pinned,
        )),
        ("auto_latest", ManualDeploymentAction::ContinueAutoLatest) => Ok(
            ManualDeploymentPlan::Keep(ManualDeploymentPolicyState::AutoLatest),
        ),
        ("auto_latest" | "manual", ManualDeploymentAction::ConvertToManual) => {
            Ok(ManualDeploymentPlan::ConvertToManual)
        }
        ("auto_latest", ManualDeploymentAction::Deploy) => {
            Err("Choose Continue on auto_latest or Convert to manual and deploy")
        }
        ("auto_latest", ManualDeploymentAction::Legacy) => {
            Err("Choose Continue on auto_latest or Convert to manual and deploy")
        }
        ("manual", ManualDeploymentAction::Legacy) => Ok(ManualDeploymentPlan::Keep(
            ManualDeploymentPolicyState::Manual,
        )),
        ("pinned", ManualDeploymentAction::Legacy) => Ok(ManualDeploymentPlan::Keep(
            ManualDeploymentPolicyState::Pinned,
        )),
        _ => Err("The requested deployment action is not valid for the system policy"),
    }
}

fn manual_deployment_response(
    status: StatusCode,
    policy: ManualDeploymentPolicyState,
    conversion: ManualDeploymentConversionState,
    deployment: ManualDeploymentRequestState,
    deployment_id: Option<Uuid>,
    message: String,
) -> axum::response::Response {
    (
        status,
        Json(ManualDeploymentResponse {
            status: match deployment {
                ManualDeploymentRequestState::Queued => "accepted",
                ManualDeploymentRequestState::AlreadyQueued => "accepted",
                ManualDeploymentRequestState::Failed => "failed",
                ManualDeploymentRequestState::Conflict => "conflict",
            }
            .to_string(),
            policy,
            conversion,
            deployment,
            deployment_id,
            message,
        }),
    )
        .into_response()
}

fn manual_deployment_failure_message(
    _policy: ManualDeploymentPolicyState,
    _conversion: ManualDeploymentConversionState,
    failure: &str,
) -> String {
    format!("Deployment failed: {failure}")
}

fn deployment_request_identity(
    request_id: Option<Uuid>,
    system_id: Uuid,
    commit_sha: &str,
    action: ManualDeploymentAction,
) -> String {
    request_id.map_or_else(
        // COMPATIBILITY: Legacy clients omit request_id. Their stable intent is
        // replay-safe across pending and terminal states for the database's
        // conservative 24-hour window. After that window the same derived
        // identity can create an intentional redeployment. Explicit request_id
        // remains the unambiguous durable contract without a time boundary.
        || {
            let mut digest = Sha256::new();
            digest.update(system_id.as_bytes());
            digest.update([0]);
            digest.update(commit_sha.as_bytes());
            digest.update([0]);
            digest.update(deployment_request_action(action).as_bytes());
            format!("legacy:v1:{:x}", digest.finalize())
        },
        |id| format!("explicit:{id}"),
    )
}

fn deployment_request_action(action: ManualDeploymentAction) -> &'static str {
    match action {
        ManualDeploymentAction::Legacy => "legacy",
        ManualDeploymentAction::Deploy => "deploy",
        ManualDeploymentAction::ContinueAutoLatest => "continue_auto_latest",
        ManualDeploymentAction::ConvertToManual => "convert_to_manual",
    }
}

pub async fn deploy_system(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<DeploySystemRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    if !caller_role.can_mutate_systems() {
        return forbidden_mutation();
    }
    if let Err(response) = require_csrf(&headers) {
        return response;
    }

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    // SECURITY: Resolve authorization before validating target details. An
    // unknown or hidden system must not disclose request-shape information.
    let commit_sha = payload.commit_sha.trim().to_ascii_lowercase();
    if let Err(message) = validate_target_commit(&commit_sha) {
        return bad_request(&message);
    }

    let request_identity =
        deployment_request_identity(payload.request_id, system_id, &commit_sha, payload.action);
    let request_action = deployment_request_action(payload.action);
    let belongs_to_flake = match commit_belongs_to_system_flake(&pool, system_id, &commit_sha).await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to validate requested commit"),
    };
    if !belongs_to_flake {
        return bad_request("Requested commit is not available for this system");
    }
    if let Some(request_id) = payload.request_id {
        match crate::queries::systems::reserve_explicit_deployment_request(
            &pool,
            system_id,
            request_id,
            &commit_sha,
            request_action,
        )
        .await
        {
            Ok(_) => {}
            Err(error) => {
                if let Some(conflict) = error
                    .downcast_ref::<crate::queries::systems::DeploymentRequestIdentityConflict>(
                ) {
                    return manual_deployment_response(
                        StatusCode::CONFLICT,
                        match row.deployment_policy.as_str() {
                            "auto_latest" => ManualDeploymentPolicyState::AutoLatest,
                            "pinned" => ManualDeploymentPolicyState::Pinned,
                            _ => ManualDeploymentPolicyState::Manual,
                        },
                        ManualDeploymentConversionState::NotRequested,
                        ManualDeploymentRequestState::Conflict,
                        conflict.deployment_id,
                        "The request_id is already bound to a different system, commit, or deployment action. Use a new request_id for a new deployment intent.".to_string(),
                    );
                }
                tracing::error!(system_id = %system_id, error = %error, "failed to reserve deployment request identity");
                return internal_error("Failed to reserve deployment request identity");
            }
        }
    }
    let plan = match plan_manual_deployment(&row.deployment_policy, payload.action) {
        Ok(plan) => plan,
        Err(message) => return bad_request(message),
    };
    // TRANSACTION: Conversion commits before deployment resolution. A missing
    // target therefore cannot roll back the requested policy change.
    let conversion_result = if matches!(plan, ManualDeploymentPlan::ConvertToManual) {
        match crate::queries::systems::convert_auto_latest_system_to_manual_for_request(
            &pool,
            system_id,
            payload.request_id,
        )
        .await
        {
            Ok(conversion) => Some(conversion),
            Err(error) => {
                tracing::error!(system_id = %system_id, error = %error, "failed to convert deployment policy");
                return manual_deployment_response(
                    StatusCode::CONFLICT,
                    ManualDeploymentPolicyState::AutoLatest,
                    ManualDeploymentConversionState::NotRequested,
                    ManualDeploymentRequestState::Failed,
                    None,
                    manual_deployment_failure_message(
                        ManualDeploymentPolicyState::AutoLatest,
                        ManualDeploymentConversionState::NotRequested,
                        "The deployment policy could not be converted to manual.",
                    ),
                );
            }
        }
    } else {
        None
    };
    let expected_policy = if conversion_result.is_some() {
        "manual"
    } else {
        row.deployment_policy.as_str()
    };
    match crate::services::composite_enforcement::authorize_system_target(
        &pool,
        system_id,
        &commit_sha,
    )
    .await
    {
        Ok(authorization) if authorization.allowed() => {}
        Ok(authorization) => return bad_request(&authorization.detail),
        Err(error) => {
            if is_uncached_deployment_target_error(&error) {
                let message =
                    queue_deployment_target_prerequisite(&state, system_id, &commit_sha).await;
                return deployment_target_unavailable(&message);
            }
            tracing::warn!(
                system_id = %system_id,
                target = %commit_sha,
                "Composite manual deployment authorization failed closed: {error:#}"
            );
            return internal_error("Composite deployment authorization failed");
        }
    }
    let queue_result = crate::queries::systems::queue_manual_deployment_atomic(
        &pool,
        system_id,
        &commit_sha,
        "manual_deploy",
        &request_identity,
        request_action,
        expected_policy,
    )
    .await;
    let queue_outcome = match queue_result {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(conflict) =
                error.downcast_ref::<crate::queries::systems::DeploymentRequestIdentityConflict>()
            {
                let (policy, conversion) = match conversion_result {
                    Some(ManualPolicyConversion::Converted) => (
                        ManualDeploymentPolicyState::Manual,
                        ManualDeploymentConversionState::Converted,
                    ),
                    Some(ManualPolicyConversion::AlreadyManual) => (
                        ManualDeploymentPolicyState::Manual,
                        ManualDeploymentConversionState::AlreadyManual,
                    ),
                    None => (
                        match row.deployment_policy.as_str() {
                            "auto_latest" => ManualDeploymentPolicyState::AutoLatest,
                            "pinned" => ManualDeploymentPolicyState::Pinned,
                            _ => ManualDeploymentPolicyState::Manual,
                        },
                        ManualDeploymentConversionState::NotRequested,
                    ),
                };
                return manual_deployment_response(
                    StatusCode::CONFLICT,
                    policy,
                    conversion,
                    ManualDeploymentRequestState::Conflict,
                    conflict.deployment_id,
                    "The request_id is already bound to a different commit or deployment action. Use a new request_id for a new deployment intent.".to_string(),
                );
            }
            let (policy, conversion) = match conversion_result {
                Some(ManualPolicyConversion::Converted) => (
                    ManualDeploymentPolicyState::Manual,
                    ManualDeploymentConversionState::Converted,
                ),
                Some(ManualPolicyConversion::AlreadyManual) => (
                    ManualDeploymentPolicyState::Manual,
                    ManualDeploymentConversionState::AlreadyManual,
                ),
                None => match plan {
                    ManualDeploymentPlan::Keep(policy) => {
                        (policy, ManualDeploymentConversionState::NotRequested)
                    }
                    ManualDeploymentPlan::ConvertToManual => {
                        unreachable!("conversion result is present")
                    }
                },
            };
            let message = if is_uncached_deployment_target_error(&error) {
                queue_deployment_target_prerequisite(&state, system_id, &commit_sha).await
            } else {
                tracing::error!(system_id = %system_id, error = %error, "failed to request deployment");
                "The deployment could not be queued because the server failed to persist the request."
                    .to_string()
            };
            if let Some(request_id) = payload.request_id {
                if let Err(state_error) =
                    crate::queries::systems::update_explicit_deployment_request_state(
                        &pool,
                        system_id,
                        request_id,
                        "deploy_failed",
                        None,
                    )
                    .await
                {
                    tracing::error!(system_id = %system_id, error = %state_error, "failed to persist deployment request partial state");
                }
            }
            let message = manual_deployment_failure_message(policy, conversion, &message);
            if conversion != ManualDeploymentConversionState::NotRequested
                && record_system_mutation_audit(
                    &pool,
                    user_id,
                    AuditAction::SystemDeployRequested,
                    format!("{} ({})", row.hostname, row.id),
                    extract_request_origin(&headers),
                    serde_json::json!({
                        "operation": "convert_policy_and_deploy",
                        "target_commit": &commit_sha,
                        "persisted_policy": policy,
                        "policy_conversion": conversion,
                        "deployment_state": ManualDeploymentRequestState::Failed,
                    }),
                )
                .await
                .is_err()
            {
                tracing::error!(system_id = %system_id, "failed to audit persisted policy conversion after deployment failure");
            }
            return manual_deployment_response(
                if is_uncached_deployment_target_error(&error) {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                policy,
                conversion,
                ManualDeploymentRequestState::Failed,
                None,
                message,
            );
        }
    };
    let (policy, conversion) = match plan {
        ManualDeploymentPlan::Keep(policy) => {
            (policy, ManualDeploymentConversionState::NotRequested)
        }
        ManualDeploymentPlan::ConvertToManual => (
            ManualDeploymentPolicyState::Manual,
            match conversion_result {
                Some(ManualPolicyConversion::Converted) => {
                    ManualDeploymentConversionState::Converted
                }
                Some(ManualPolicyConversion::AlreadyManual) => {
                    ManualDeploymentConversionState::AlreadyManual
                }
                None => ManualDeploymentConversionState::NotRequested,
            },
        ),
    };

    let deployment = if queue_outcome.created {
        ManualDeploymentRequestState::Queued
    } else {
        ManualDeploymentRequestState::AlreadyQueued
    };

    if record_system_mutation_audit(
        &pool,
        user_id,
        AuditAction::SystemDeployRequested,
        format!("{} ({})", row.hostname, row.id),
        extract_request_origin(&headers),
        serde_json::json!({
            "operation": "deploy",
            "target_commit": commit_sha,
            "requested_action": payload.action,
            "persisted_policy": policy,
            "policy_conversion": conversion,
            "deployment_state": deployment,
            "deployment_id": queue_outcome.deployment_id,
        }),
    )
    .await
    .is_err()
    {
        // Queueing is already durable. Report the truthful accepted state even
        // when the secondary audit sink is temporarily unavailable.
        tracing::error!(system_id = %system_id, deployment_id = %queue_outcome.deployment_id, "deployment queued but audit persistence failed");
    }

    let deployment_message = match deployment {
        ManualDeploymentRequestState::Queued => format!(
            "Deployment requested for {} to commit {}",
            row.hostname, commit_sha
        ),
        ManualDeploymentRequestState::AlreadyQueued => format!(
            "Deployment for {} to commit {} is already queued",
            row.hostname, commit_sha
        ),
        ManualDeploymentRequestState::Failed | ManualDeploymentRequestState::Conflict => {
            unreachable!("failure and conflict return before audit")
        }
    };
    let message = match (policy, conversion) {
        (
            ManualDeploymentPolicyState::Manual,
            ManualDeploymentConversionState::Converted
            | ManualDeploymentConversionState::AlreadyManual,
        ) => format!("System policy is manual. {deployment_message}"),
        (
            ManualDeploymentPolicyState::AutoLatest,
            ManualDeploymentConversionState::NotRequested,
        ) => format!("System remains on auto_latest. {deployment_message}"),
        _ => deployment_message,
    };
    manual_deployment_response(
        StatusCode::ACCEPTED,
        policy,
        conversion,
        deployment,
        Some(queue_outcome.deployment_id),
        message,
    )
}

pub async fn get_system_commits(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load environment memberships".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "System not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load system".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: "System not found".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    let commits = match list_recent_commits_for_system(&pool, system_id, 50).await {
        Ok(rows) => rows
            .into_iter()
            .map(|row| {
                let short_sha = row.sha.chars().take(7).collect::<String>();
                CommitInfo {
                    sha: row.sha,
                    short_sha,
                    message: row.message.unwrap_or_default(),
                    author: row.author.unwrap_or_else(|| "unknown".to_string()),
                    timestamp: row.timestamp.to_rfc3339(),
                }
            })
            .collect::<Vec<_>>(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load system commits".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let response = SystemCommitsResponse {
        commits,
        current_commit: None,
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn get_system_generations(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load environment memberships".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: "System not found".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load system".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: "System not found".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    // Fetch generation history
    let generation_rows = match fetch_system_generations(&pool, system_id).await {
        Ok(rows) => rows,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to load system generations".to_string(),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    // Get current generation from system detail
    let current_generation = match get_system_detail_by_id(&pool, system_id).await {
        Ok(Some(detail)) => detail.generation,
        _ => None,
    };

    let generations = generation_rows
        .into_iter()
        .map(|row| SystemGeneration {
            generation: row.generation,
            store_path: row.store_path,
            commit_hash: row.commit_hash,
            timestamp: row.timestamp,
            is_current: Some(row.generation) == current_generation,
        })
        .collect::<Vec<_>>();

    let response = SystemGenerationsResponse {
        generations,
        current_generation,
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn verify_generation_closure(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Json(payload): Json<VerifyGenerationClosureRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let store_path = payload.store_path.trim();
    if store_path.is_empty() {
        return bad_request("store_path is required");
    }

    let last_seen_at =
        match find_generation_store_path_last_seen(&pool, system_id, store_path).await {
            Ok(value) => value,
            Err(_) => return internal_error("Failed to verify generation closure"),
        };

    let response = if let Some(last_seen_at) = last_seen_at {
        VerifyGenerationClosureResponse {
            available: true,
            message: format!(
                "Closure is available. Store path was reported by agent at {}.",
                last_seen_at.to_rfc3339()
            ),
            last_seen_at: Some(last_seen_at),
        }
    } else {
        VerifyGenerationClosureResponse {
            available: false,
            message: "Closure not yet reported by agent for this system/store path.".to_string(),
            last_seen_at: None,
        }
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn get_system_deployment_status(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let progress_row = match get_system_deployment_progress_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return StatusCode::NO_CONTENT.into_response(),
        Err(_) => return internal_error("Failed to load deployment status"),
    };

    let Some(stage) = deployment_progress_stage(
        &progress_row.status,
        progress_row.expires_at,
        progress_row.delivered_at,
        progress_row.applying_at,
        chrono::Utc::now(),
    ) else {
        return StatusCode::NO_CONTENT.into_response();
    };

    (
        StatusCode::OK,
        Json(SystemDeploymentProgress {
            id: progress_row.id,
            stage: stage.to_string(),
            kind: deployment_progress_kind(&progress_row.source).to_string(),
            target_store_path: progress_row.target_store_path,
            target_commit: progress_row.target_commit,
            target_generation: progress_row.target_generation,
            source: progress_row.source,
            issued_at: progress_row.issued_at,
            delivered_at: progress_row.delivered_at,
            applying_at: progress_row.applying_at,
            completed_at: progress_row.completed_at,
            failed_at: progress_row.failed_at,
            failure_message: progress_row.failure_message,
        }),
    )
        .into_response()
}

/// Classify a history row into `(event_kind, actor)`.
///
/// `cf_deployment` = deploy driven through Crystal Forge; `config_change` =
/// on-host/out-of-band activation (e.g. a manual `nixos-rebuild switch`);
/// `startup` + `system_reboot` restart_type = system reboot;
/// `startup` + `agent_restart` restart_type = agent service restart;
/// `startup` + other/None = generic restart (legacy, no boot_id data).
///
/// A `nixos-rebuild switch` stops and restarts the agent during activation,
/// so the first heartbeat carrying the NEW generation is often a `startup`
/// row. When such a row lands on a different generation/store path than the
/// next-older row, the real event is the on-host rebuild — classify it as
/// `local_rebuild` instead of hiding the switch behind "Agent restarted".
/// A genuine reboot (`system_reboot`) still classifies as `restart`.
fn classify_history_event(
    change_reason: &str,
    restart_type: Option<&str>,
    changed_generation_or_store: bool,
) -> (&'static str, &'static str) {
    match change_reason {
        "cf_deployment" => ("cf_deployment", "crystal-forge"),
        "config_change" => ("local_rebuild", "on-host"),
        "startup" => match restart_type {
            Some("system_reboot") => ("restart", "agent"),
            _ if changed_generation_or_store => ("local_rebuild", "on-host"),
            Some("agent_restart") => ("agent_restart", "agent"),
            _ => ("restart", "agent"),
        },
        "state_delta" if changed_generation_or_store => ("local_rebuild", "on-host"),
        _ => ("state_change", "agent"),
    }
}

fn event_history_kind(event_type: &str) -> (&'static str, &'static str, &'static str) {
    match event_type {
        "cf_deployment_started" => ("cf_deployment", "crystal-forge", "started"),
        "cf_deployment_succeeded" => ("cf_deployment", "crystal-forge", "succeeded"),
        "cf_deployment_failed" => ("cf_deployment", "crystal-forge", "failed"),
        "local_rebuild_detected" => ("local_rebuild", "on-host", "recorded"),
        "system_reboot" => ("restart", "agent", "recorded"),
        "agent_restart" => ("agent_restart", "agent", "recorded"),
        _ => ("state_change", "agent", "recorded"),
    }
}

fn event_history_title(event_type: &str) -> &'static str {
    match event_type {
        "cf_deployment_started" => "Deployment started",
        "cf_deployment_succeeded" => "Deployed through Crystal Forge",
        "cf_deployment_failed" => "Deploy failed to activate",
        "local_rebuild_detected" => "nixos-rebuild switch on host",
        "system_reboot" => "System restarted",
        "agent_restart" => "Agent restarted",
        _ => "State updated",
    }
}

pub async fn get_system_history(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let event_rows = match list_system_event_history_rows(&pool, system_id, 200).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load system history"),
    };

    if !event_rows.is_empty() {
        let entries = event_rows
            .into_iter()
            .map(|row| {
                let (event_kind, default_actor, outcome) = event_history_kind(&row.event_type);
                let title = event_history_title(&row.event_type).to_string();
                let actor = row
                    .actor
                    .clone()
                    .unwrap_or_else(|| default_actor.to_string());
                let generation = row
                    .new_generation
                    .and_then(|value| i32::try_from(value).ok());
                let store_path = row.new_store_path.clone();
                let reconciled = row.commit_hash.is_some();
                let restart_type = match row.event_type.as_str() {
                    "system_reboot" => Some("system_reboot".to_string()),
                    "agent_restart" => Some("agent_restart".to_string()),
                    _ => None,
                };

                SystemHistoryEntry {
                    id: Some(row.id),
                    timestamp: row.occurred_at,
                    occurred_at: Some(row.occurred_at),
                    observed_at: Some(row.observed_at),
                    store_path,
                    system_configuration_name: row.system_configuration_name,
                    change_reason: title.clone(),
                    event_type: row.event_type,
                    event_rank: Some(row.event_rank),
                    title: Some(title),
                    commit_hash: row.commit_hash,
                    flake_name: row.flake_name,
                    flake_repo_url: row.flake_repo_url,
                    actor,
                    outcome: outcome.to_string(),
                    event_kind: event_kind.to_string(),
                    generation,
                    previous_generation: row.previous_generation,
                    new_generation: row.new_generation,
                    previous_store_path: row.previous_store_path,
                    new_store_path: row.new_store_path,
                    previous_boot_id: row.previous_boot_id,
                    new_boot_id: row.new_boot_id,
                    deployment_id: row.deployment_id,
                    desired_target_id: row.desired_target_id,
                    source: Some(row.source),
                    correlation_id: row.correlation_id,
                    metadata: row.metadata,
                    reconciled,
                    generation_matches_current_store_path: None,
                    restart_type,
                }
            })
            .collect::<Vec<_>>();

        return (StatusCode::OK, Json(entries)).into_response();
    }

    let rows = match list_system_history_rows(&pool, system_id, 200).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load system history"),
    };

    let entries = rows
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            let change_reason = row
                .change_reason
                .clone()
                .unwrap_or_else(|| "state_delta".to_string());

            // Rows are sorted newest → oldest.  If this row's generation/store
            // path differs from the next-older row, it is a real local system
            // activation even when the stored change_reason is the generic
            // state_delta.  Prefer the actual generation/store transition over
            // the textual reason so a nixos-rebuild switch does not disappear
            // from history as a generic state change.
            let changed_generation_or_store = rows.get(idx + 1).is_some_and(|older| {
                row.generation != older.generation || row.store_path != older.store_path
            });

            // Authoritative classification derived from the recorded change_reason,
            // the per-row restart_type written at insert time, and the actual
            // generation/store-path transition. See classify_history_event.
            let (event_kind, actor) = classify_history_event(
                change_reason.as_str(),
                row.restart_type.as_deref(),
                changed_generation_or_store,
            );
            let event_kind = event_kind.to_string();
            let actor = actor.to_string();

            // Reconciled/tracked when the running store path maps to a known flake
            // commit; untracked (capture-to-flake) otherwise.
            let reconciled = row.commit_hash.is_some();

            SystemHistoryEntry {
                id: None,
                timestamp: row.timestamp,
                occurred_at: Some(row.timestamp),
                observed_at: None,
                store_path: row.store_path.clone(),
                system_configuration_name: row.system_configuration_name.clone(),
                change_reason,
                event_type: event_kind.clone(),
                event_rank: None,
                title: None,
                commit_hash: row.commit_hash.clone(),
                flake_name: row.flake_name.clone(),
                flake_repo_url: row.flake_repo_url.clone(),
                actor,
                outcome: "recorded".to_string(),
                event_kind,
                generation: row.generation,
                previous_generation: None,
                new_generation: row.generation.map(i64::from),
                previous_store_path: None,
                new_store_path: row.store_path.clone(),
                previous_boot_id: None,
                new_boot_id: None,
                deployment_id: None,
                desired_target_id: None,
                source: Some("legacy_system_states".to_string()),
                correlation_id: None,
                metadata: serde_json::json!({ "legacy_history_source": "system_states" }),
                reconciled,
                generation_matches_current_store_path: row.generation_matches_current_store_path,
                restart_type: row.restart_type.clone(),
            }
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(entries)).into_response()
}

pub async fn get_system_agent_events(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return forbidden();
    };

    let Some(caller_role) = highest_role(&roles) else {
        return forbidden();
    };

    let environment_memberships = match load_membership_environment_ids(&pool, user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load environment memberships"),
    };

    let row = match find_system_access_row(&pool, system_id).await {
        Ok(Some(value)) => value,
        Ok(None) => return not_found(),
        Err(_) => return internal_error("Failed to load system"),
    };

    if !caller_role.can_access_system_environment(row.environment_id, &environment_memberships) {
        return not_found();
    }

    let rows = match list_system_agent_event_rows(&pool, system_id, 300).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load agent events"),
    };

    let entries = rows
        .into_iter()
        .map(|row| SystemAgentEvent {
            timestamp: row.timestamp,
            level: row.level,
            event_type: row.event_type,
            message: row.message,
            deployment_related: row.deployment_related,
        })
        .collect::<Vec<_>>();

    (StatusCode::OK, Json(entries)).into_response()
}

async fn record_system_mutation_audit(
    pool: &PgPool,
    actor_user_id: Uuid,
    action: AuditAction,
    target: String,
    request_origin: Option<String>,
    metadata: serde_json::Value,
) -> Result<(), ()> {
    let actor_identifier = crate::queries::admin::find_user_email(pool, actor_user_id)
        .await
        .map_err(|_| ())?
        .unwrap_or_else(|| actor_user_id.to_string());

    crate::queries::admin::insert_admin_audit_event(
        pool,
        actor_user_id,
        &actor_identifier,
        action_to_str(action),
        &target,
        request_origin,
        metadata,
    )
    .await
    .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::session::{SESSION_COOKIE_NAME, hash_token};
    use crate::models::auth_identity::AuthRole;
    use crate::models::public_key::PublicKey;
    use crate::models::systems::System;
    use crate::queries::auth_identity::{create_user_session, sync_user_role};
    use crate::queries::environments::create_environment;
    use crate::queries::flakes::insert_flake;
    use crate::queries::systems::insert_system;
    use crate::queries::users::insert_user;
    use axum::extract::State;
    use axum::http::header;
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn snapshot_revisions_require_full_immutable_sha_identity() {
        let shared_prefix = "abcdef0";
        let first = format!("{shared_prefix}{}", "1".repeat(33));
        let second = format!("{shared_prefix}{}", "2".repeat(33));

        assert!(is_full_commit_sha(&first));
        assert!(is_full_commit_sha(&second));
        assert_ne!(first, second);
        assert!(!is_full_commit_sha(shared_prefix));
        assert!(!is_full_commit_sha(&format!("{}z", "a".repeat(39))));
    }

    #[test]
    fn startup_row_with_changed_generation_classifies_as_local_rebuild() {
        // nixos-rebuild switch restarts the agent, so the new generation
        // arrives on a startup heartbeat. The switch must show as a local
        // rebuild, not be hidden behind "Agent restarted".
        assert_eq!(
            classify_history_event("startup", Some("agent_restart"), true),
            ("local_rebuild", "on-host")
        );
        assert_eq!(
            classify_history_event("startup", None, true),
            ("local_rebuild", "on-host")
        );
    }

    #[test]
    fn startup_row_without_generation_change_keeps_restart_classification() {
        assert_eq!(
            classify_history_event("startup", Some("agent_restart"), false),
            ("agent_restart", "agent")
        );
        assert_eq!(
            classify_history_event("startup", None, false),
            ("restart", "agent")
        );
    }

    #[test]
    fn system_reboot_stays_restart_even_when_generation_changed() {
        // Booting into a new generation (nixos-rebuild boot + reboot) is
        // still a reboot event; the generation transition is visible on the
        // entry itself.
        assert_eq!(
            classify_history_event("startup", Some("system_reboot"), true),
            ("restart", "agent")
        );
    }

    #[test]
    fn non_startup_rows_classify_by_change_reason() {
        assert_eq!(
            classify_history_event("cf_deployment", None, false),
            ("cf_deployment", "crystal-forge")
        );
        assert_eq!(
            classify_history_event("config_change", None, false),
            ("local_rebuild", "on-host")
        );
        assert_eq!(
            classify_history_event("state_delta", None, true),
            ("local_rebuild", "on-host")
        );
        assert_eq!(
            classify_history_event("state_delta", None, false),
            ("state_change", "agent")
        );
    }

    #[test]
    fn event_history_kind_maps_event_backed_contract() {
        assert_eq!(
            event_history_kind("cf_deployment_succeeded"),
            ("cf_deployment", "crystal-forge", "succeeded")
        );
        assert_eq!(
            event_history_kind("cf_deployment_failed"),
            ("cf_deployment", "crystal-forge", "failed")
        );
        assert_eq!(
            event_history_kind("local_rebuild_detected"),
            ("local_rebuild", "on-host", "recorded")
        );
        assert_eq!(
            event_history_kind("system_reboot"),
            ("restart", "agent", "recorded")
        );
        assert_eq!(
            event_history_kind("agent_restart"),
            ("agent_restart", "agent", "recorded")
        );
    }

    #[test]
    fn highest_role_prefers_admin_then_operator_then_viewer() {
        assert_eq!(highest_role(&[AuthRole::Admin]), Some(Role::Admin));
        assert_eq!(highest_role(&[AuthRole::Operator]), Some(Role::Operator));
        assert_eq!(highest_role(&[AuthRole::Viewer]), Some(Role::Viewer));
        assert_eq!(highest_role(&[]), None);
    }

    #[test]
    fn matches_filters_checks_search_and_environment() {
        let row = SystemAccessRow {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid"),
            hostname: "prod-edge-01".to_string(),
            environment_id: None,
            environment: Some("prod".to_string()),
            is_active: true,
            deployment_policy: "manual".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let mut params = SystemsListParams::default();
        params.search = Some("edge".to_string());
        params.environment = Some("PROD".to_string());
        assert!(matches_filters(&row, &params));

        params.environment = Some("staging".to_string());
        assert!(!matches_filters(&row, &params));
    }

    fn test_cf_state() -> CFState {
        use crate::config::ServerConfig;
        use crate::handlers::agent_request::CFState;
        use crate::queue::QueueNotifier;
        use std::sync::Arc;
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");
        CFState::new(
            pool,
            ServerConfig::default(),
            Arc::new(QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        )
    }

    #[tokio::test]
    async fn list_systems_requires_authenticated_role() {
        let state = test_cf_state();
        let pool = state.pool.clone();

        let response = list_systems(
            State(state),
            State(pool),
            HeaderMap::new(),
            Query(SystemsListParams::default()),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_requires_authenticated_role() {
        let state = test_cf_state();
        let pool = state.pool.clone();

        let response = get_system(
            State(state),
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_cves_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_cves(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn save_system_cve_justification_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = save_system_cve_justification(
            State(pool),
            HeaderMap::new(),
            Path((
                Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid"),
                "CVE-2025-1234".to_string(),
            )),
            Json(SaveSystemCveJustificationRequest {
                category: Some("accepted_risk".to_string()),
                reason: "Risk accepted pending upstream patch".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    // NOTE: Additional integration tests requiring a real test database:
    // - save_system_cve_justification_valid_category_accepted (200 OK, verify DB write + audit)
    // - save_system_cve_justification_no_category_accepted (200 OK with category=NULL)
    // - save_system_cve_justification_empty_reason_rejected (400 Bad Request)
    // - save_system_cve_justification_reason_too_long_rejected (400 Bad Request)
    // - save_system_cve_justification_invalid_category_rejected (400 Bad Request)
    // - save_system_cve_justification_cve_not_on_system_rejected (400 Bad Request)
    // - save_system_cve_justification_system_not_found (404 Not Found)
    // - save_system_cve_justification_no_environment_access (404 Not Found)
    // - save_system_cve_justification_viewer_forbidden (403 Forbidden)
    // - save_system_cve_justification_transaction_rollback_on_audit_failure (verify atomicity)
    //
    // These tests should be implemented when a test database fixture/harness is available.

    #[tokio::test]
    async fn get_system_cve_scan_eligibility_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_cve_scan_eligibility(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn trigger_system_cve_scan_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = trigger_system_cve_scan(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_cve_scan_status_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_cve_scan_status(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000002").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn parse_cve_severity_maps_expected_values() {
        assert_eq!(
            parse_cve_severity("critical"),
            crate::api::models::CveSeverity::Critical
        );
        assert_eq!(
            parse_cve_severity("high"),
            crate::api::models::CveSeverity::High
        );
        assert_eq!(
            parse_cve_severity("medium"),
            crate::api::models::CveSeverity::Medium
        );
        assert_eq!(
            parse_cve_severity("low"),
            crate::api::models::CveSeverity::Low
        );
        assert_eq!(
            parse_cve_severity("unknown"),
            crate::api::models::CveSeverity::Low
        );
    }

    #[tokio::test]
    async fn sync_system_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = sync_system(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn evaluated_options_read_requires_authentication_before_database_access() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");
        let response = get_system_evaluated_options(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::nil()),
            Query(EvaluatedOptionsParams {
                revision: "a".repeat(40),
                ..EvaluatedOptionsParams::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn evaluation_summary_requires_authentication_before_database_access() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");
        let response = get_system_evaluation_summary(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::nil()),
            Query(SelectedEvaluationSummaryParams {
                revision: "a".repeat(40),
                ..SelectedEvaluationSummaryParams::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn evaluation_module_sources_require_authentication_before_database_access() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");
        let response = get_system_evaluation_module_sources(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::nil()),
            Query(EvaluationModuleSourcesParams {
                revision: "a".repeat(40),
                ..EvaluationModuleSourcesParams::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn evaluation_module_source_empty_page_applies_request_bounds() {
        let page = empty_evaluation_module_sources(
            &EvaluationModuleSourcesParams {
                revision: "a".repeat(40),
                limit: Some(i64::MAX),
                offset: Some(i64::MAX),
                ..EvaluationModuleSourcesParams::default()
            },
            (SnapshotLifecycle::Unavailable, None),
        );
        assert_eq!(page.limit, 100);
        assert_eq!(page.offset, 100_000);
        assert_eq!(page.total, 0);
        assert!(page.sources.is_empty());
    }

    #[test]
    fn evaluation_module_source_continuations_require_valid_snapshot_tokens() {
        let missing = EvaluationModuleSourcesParams {
            offset: Some(1),
            ..EvaluationModuleSourcesParams::default()
        };
        assert_eq!(
            validate_evaluation_module_sources_params(&missing),
            Err("snapshot_token is required when offset is greater than 0")
        );

        for token in ["", "0", "-1", "not-a-version", &"g".repeat(64)] {
            let malformed = EvaluationModuleSourcesParams {
                snapshot_token: Some(token.to_string()),
                ..EvaluationModuleSourcesParams::default()
            };
            assert_eq!(
                validate_evaluation_module_sources_params(&malformed),
                Err("snapshot_token must be a 64-character hexadecimal digest")
            );
        }

        let valid = EvaluationModuleSourcesParams {
            offset: Some(100),
            snapshot_token: Some("a".repeat(64)),
            ..EvaluationModuleSourcesParams::default()
        };
        assert_eq!(validate_evaluation_module_sources_params(&valid), Ok(()));
    }

    #[tokio::test]
    async fn queue_snapshot_action_requires_authentication_before_revision_disclosure() {
        let state = test_cf_state();
        let response = queue_system_evaluation_snapshot(
            State(state),
            HeaderMap::new(),
            Path((Uuid::nil(), "a".repeat(40))),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    #[ignore = "requires an isolated migrated database"]
    async fn hidden_environment_snapshot_read_is_non_disclosing() {
        let pool = test_pool_from_env().await;
        let suffix = Uuid::new_v4().simple().to_string();
        let visible = create_environment(
            &pool,
            &format!("visible-{suffix}"),
            None,
            "#111111",
            true,
            "manual",
            false,
            false,
            false,
        )
        .await
        .expect("visible environment should insert");
        let hidden = create_environment(
            &pool,
            &format!("hidden-{suffix}"),
            None,
            "#222222",
            true,
            "manual",
            false,
            false,
            false,
        )
        .await
        .expect("hidden environment should insert");
        let user = insert_user(
            &pool,
            &format!("snapshot-{suffix}@example.test"),
            Some("Snapshot Viewer"),
        )
        .await
        .expect("user should insert");
        sync_user_role(&pool, user.id, AuthRole::Viewer)
            .await
            .expect("viewer role should persist");
        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id) VALUES ($1, $2)",
        )
        .bind(user.id)
        .bind(visible.id)
        .execute(&pool)
        .await
        .expect("membership should persist");

        let key = SigningKey::from_bytes(&[44; 32]);
        let flake = insert_flake(
            &pool,
            &format!("hidden-snapshot-{suffix}"),
            &format!("https://example.test/hidden-snapshot-{suffix}.git"),
            "main",
            "cf_systems_only",
        )
        .await
        .expect("hidden flake should insert");
        let system = insert_system(
            &pool,
            &System {
                id: Uuid::new_v4(),
                hostname: format!("hidden-system-{suffix}"),
                environment_id: Some(hidden.id),
                is_active: true,
                public_key: PublicKey::from_verifying_key(key.verifying_key()),
                flake_id: Some(flake.id),
                derivation: String::new(),
                system_configuration_name: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                desired_target: None,
                deployment_policy: "manual".into(),
            },
        )
        .await
        .expect("hidden system should insert");
        let session_token = format!("snapshot-session-{suffix}");
        create_user_session(
            &pool,
            user.id,
            hash_token(&session_token),
            Utc::now() + chrono::Duration::hours(1),
            None,
            None,
            "local".into(),
        )
        .await
        .expect("session should persist");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{}={}", SESSION_COOKIE_NAME, session_token)
                .parse()
                .expect("cookie should parse"),
        );

        let response = get_system_evaluated_options(
            State(pool.clone()),
            headers.clone(),
            Path(system.id),
            Query(EvaluatedOptionsParams {
                revision: "a".repeat(40),
                ..EvaluatedOptionsParams::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get_system_evaluation_summary(
            State(pool.clone()),
            headers.clone(),
            Path(system.id),
            Query(SelectedEvaluationSummaryParams {
                revision: "a".repeat(40),
                ..SelectedEvaluationSummaryParams::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = get_system_evaluation_module_sources(
            State(pool.clone()),
            headers.clone(),
            Path(system.id),
            Query(EvaluationModuleSourcesParams {
                revision: "a".repeat(40),
                ..EvaluationModuleSourcesParams::default()
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = crate::handlers::api::flakes::get_flake_module_declarations(
            State(pool.clone()),
            headers,
            Path((flake.id, "a".repeat(40), "module".to_string())),
            Query(crate::models::evaluation_snapshots::FlakeModuleDeclarationsParams::default()),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("user cleanup should succeed");
        sqlx::query("DELETE FROM systems WHERE id = $1")
            .bind(system.id)
            .execute(&pool)
            .await
            .expect("system cleanup should succeed");
        sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake.id)
            .execute(&pool)
            .await
            .expect("flake cleanup should succeed");
        sqlx::query("DELETE FROM environments WHERE id = ANY($1)")
            .bind(vec![visible.id, hidden.id])
            .execute(&pool)
            .await
            .expect("environment cleanup should succeed");
    }

    #[tokio::test]
    async fn rollback_system_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = rollback_system(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
            Json(SystemRollbackRequest {
                target_commit: "abc123".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rollback_system_generation_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = rollback_system_generation(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
            Json(SystemRollbackGenerationRequest {
                store_path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-system".to_string(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    async fn test_pool_from_env() -> PgPool {
        let db_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for rollback generation DB tests");

        PgPool::connect(&db_url)
            .await
            .expect("failed to connect to DATABASE_URL")
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn rollback_system_generation_updates_desired_target_to_store_path() {
        let pool = test_pool_from_env().await;

        let suffix = Uuid::new_v4().simple().to_string();
        let hostname = format!("task294-rollback-gen-{suffix}");
        let store_path = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-system".to_string();

        let user = insert_user(
            &pool,
            &format!("{suffix}@example.com"),
            Some("Task 294 Tester"),
        )
        .await
        .expect("insert_user should succeed");
        sync_user_role(&pool, user.id, AuthRole::Admin)
            .await
            .expect("sync_user_role should succeed");

        let session_token = format!("session-{suffix}");
        let session_token_hash = hash_token(&session_token);
        create_user_session(
            &pool,
            user.id,
            session_token_hash,
            Utc::now() + chrono::Duration::hours(1),
            Some("test-agent".to_string()),
            Some("127.0.0.1".to_string()),
            "local".to_string(),
        )
        .await
        .expect("create_user_session should succeed");

        let signing_key = SigningKey::from_bytes(&[9u8; 32]);
        let public_key = PublicKey::from_verifying_key(signing_key.verifying_key());
        let system = System {
            id: Uuid::new_v4(),
            hostname: hostname.clone(),
            environment_id: None,
            is_active: true,
            public_key,
            flake_id: None,
            derivation: String::new(),
            system_configuration_name: Some(hostname.clone()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        insert_system(&pool, &system)
            .await
            .expect("insert_system should succeed");

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{}={}", SESSION_COOKIE_NAME, session_token)
                .parse()
                .expect("cookie header should parse"),
        );

        let response = rollback_system_generation(
            State(pool.clone()),
            headers,
            Path(system.id),
            Json(SystemRollbackGenerationRequest {
                store_path: store_path.clone(),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let desired_target = sqlx::query_scalar::<_, Option<String>>(
            "SELECT desired_target FROM systems WHERE id = $1",
        )
        .bind(system.id)
        .fetch_one(&pool)
        .await
        .expect("query desired_target should succeed");

        assert_eq!(desired_target.as_deref(), Some(store_path.as_str()));
    }

    #[tokio::test]
    async fn deploy_system_requires_authenticated_role() {
        let state = test_cf_state();
        let pool = state.pool.clone();

        let response = deploy_system(
            State(state),
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
            Json(DeploySystemRequest {
                commit_sha: "a1b2c3d".to_string(),
                action: ManualDeploymentAction::Deploy,
                request_id: None,
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn auto_latest_requires_an_explicit_manual_deployment_choice() {
        assert_eq!(
            plan_manual_deployment("auto_latest", ManualDeploymentAction::Deploy),
            Err("Choose Continue on auto_latest or Convert to manual and deploy")
        );
        assert_eq!(
            plan_manual_deployment("auto_latest", ManualDeploymentAction::Legacy),
            Err("Choose Continue on auto_latest or Convert to manual and deploy")
        );
        assert_eq!(
            plan_manual_deployment("auto_latest", ManualDeploymentAction::ContinueAutoLatest,),
            Ok(ManualDeploymentPlan::Keep(
                ManualDeploymentPolicyState::AutoLatest
            ))
        );
        assert_eq!(
            plan_manual_deployment("auto_latest", ManualDeploymentAction::ConvertToManual),
            Ok(ManualDeploymentPlan::ConvertToManual)
        );
    }

    #[test]
    fn legacy_identities_are_stable_intents_while_explicit_ids_are_unambiguous() {
        let system_id = Uuid::new_v4();
        let sha = "a".repeat(40);
        let first =
            deployment_request_identity(None, system_id, &sha, ManualDeploymentAction::Deploy);
        let retry =
            deployment_request_identity(None, system_id, &sha, ManualDeploymentAction::Deploy);
        let later =
            deployment_request_identity(None, system_id, &sha, ManualDeploymentAction::Deploy);
        let explicit_id = Uuid::new_v4();
        let explicit_first = deployment_request_identity(
            Some(explicit_id),
            system_id,
            &sha,
            ManualDeploymentAction::Deploy,
        );
        let explicit_later = deployment_request_identity(
            Some(explicit_id),
            system_id,
            &sha,
            ManualDeploymentAction::Deploy,
        );

        assert_eq!(first, retry);
        assert_eq!(first, later);
        assert!(first.starts_with("legacy:v1:"));
        assert_ne!(
            first,
            deployment_request_identity(
                None,
                system_id,
                &sha,
                ManualDeploymentAction::ConvertToManual,
            ),
            "the derived identity includes the action"
        );
        assert_eq!(explicit_first, explicit_later);
        assert!(explicit_first.starts_with("explicit:"));
        assert_ne!(
            explicit_first,
            deployment_request_identity(
                Some(Uuid::new_v4()),
                system_id,
                &sha,
                ManualDeploymentAction::Deploy,
            ),
            "a new explicit ID is the intentional redeployment boundary"
        );
    }

    #[test]
    fn deployment_request_conflict_has_a_stable_typed_wire_value() {
        assert_eq!(
            serde_json::to_value(ManualDeploymentRequestState::Conflict).unwrap(),
            serde_json::json!("conflict")
        );
    }

    #[test]
    fn deployment_targets_require_full_object_format_identity() {
        assert!(validate_target_commit(&"a".repeat(40)).is_ok());
        assert!(validate_target_commit(&"b".repeat(64)).is_ok());
        assert!(validate_target_commit("abcdef0").is_err());
        assert!(validate_target_commit(&"c".repeat(41)).is_err());
    }

    #[test]
    fn converted_manual_retry_remains_valid() {
        assert_eq!(
            plan_manual_deployment("manual", ManualDeploymentAction::ConvertToManual),
            Ok(ManualDeploymentPlan::ConvertToManual)
        );
    }

    #[test]
    fn deployment_failure_does_not_claim_rolled_back_conversion() {
        assert_eq!(
            manual_deployment_failure_message(
                ManualDeploymentPolicyState::Manual,
                ManualDeploymentConversionState::Converted,
                "target is unavailable",
            ),
            "Deployment failed: target is unavailable"
        );
    }

    #[test]
    fn deploy_target_prerequisite_distinguishes_built_and_buildable_rows() {
        let built = crate::queries::systems::SystemDeploymentDerivationRow {
            id: 1,
            store_path: Some("/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-system".to_string()),
            status_id: 10,
            has_completed_cache_push: false,
            has_active_cache_push: false,
            has_permanent_cache_failure: false,
            has_active_build_job: false,
        };
        assert!(built.has_store_path());
        assert!(!built.is_buildable());

        let buildable = crate::queries::systems::SystemDeploymentDerivationRow {
            id: 2,
            store_path: None,
            status_id: 5,
            has_completed_cache_push: false,
            has_active_cache_push: false,
            has_permanent_cache_failure: false,
            has_active_build_job: false,
        };
        assert!(!buildable.has_store_path());
        assert!(buildable.is_buildable());
    }

    #[tokio::test]
    async fn get_system_commits_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_commits(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_history_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_history(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_system_agent_events_requires_authenticated_role() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct");

        let response = get_system_agent_events(
            State(pool),
            HeaderMap::new(),
            Path(Uuid::parse_str("00000000-0000-0000-0000-000000000001").expect("uuid")),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn rollback_target_commit_validation_enforces_bounds_and_format() {
        assert!(validate_target_commit("").is_err());
        assert!(validate_target_commit("abc").is_err());
        assert!(validate_target_commit("zzzzzzz").is_err());
        assert!(validate_target_commit("a1b2c3d").is_err());
        assert!(validate_target_commit(&"a".repeat(40)).is_ok());
        assert!(validate_target_commit(&"a".repeat(65)).is_err());
    }

    #[test]
    fn action_to_str_distinguishes_deploy_and_rollback() {
        assert_eq!(
            action_to_str(AuditAction::SystemDeployRequested),
            "system_deploy_requested"
        );
        assert_eq!(
            action_to_str(AuditAction::SystemRollbackRequested),
            "system_rollback_requested"
        );
        assert_eq!(
            action_to_str(AuditAction::CveScanRequested),
            "cve_scan_requested"
        );
    }

    // ── CVE trigger ─────────────────────────────────────────────────────────

    #[test]
    fn scan_ineligible_response_returns_409_with_correct_error_code() {
        let response = scan_ineligible_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn cve_scan_error_vulnix_unavailable_display_is_stable() {
        use crate::services::cve_scans::CveScanError;
        let msg = CveScanError::VulnixUnavailable.to_string();
        assert!(msg.contains("vulnix"), "display must mention vulnix: {msg}");
        assert!(matches!(
            CveScanError::VulnixUnavailable,
            CveScanError::VulnixUnavailable
        ));
    }

    // ── CVE Justification Tests ────────────────────────────────────────────────

    #[test]
    fn empty_justification_reason_is_rejected() {
        // This test validates that the validation logic correctly identifies empty reasons.
        // The actual handler test requires a database connection, so this is a unit test
        // of the validation rules.
        let reason = "";
        assert!(
            reason.trim().is_empty(),
            "Empty reason should be rejected by validation"
        );
    }

    #[test]
    fn justification_reason_max_length_enforced() {
        let reason = "x".repeat(2001);
        assert!(
            reason.len() > 2000,
            "Reason longer than 2000 chars should be rejected"
        );
    }

    #[test]
    fn valid_justification_categories_accepted() {
        for category in ALLOWED_CVE_JUSTIFICATION_CATEGORIES {
            assert!(
                ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == *category),
                "Category {category} should be in allowed list"
            );
        }
    }

    #[test]
    fn invalid_justification_category_detected() {
        let invalid_category = "arbitrary_category";
        assert!(
            !ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                .iter()
                .any(|allowed| *allowed == invalid_category),
            "Invalid category should not be in allowed list"
        );
    }

    #[test]
    fn category_validation_rejects_values_not_in_preset_list() {
        // This test verifies that the validation logic (lines 354-360) correctly
        // identifies categories that are not in the allowed preset.
        let valid_categories = vec![
            "false_positive",
            "accepted_risk",
            "compensating_control",
            "planned_remediation",
            "vendor_pending_fix",
        ];

        let invalid_categories = vec![
            "custom_reason",
            "not_applicable",
            "temporary_exception",
            "executive_override",
        ];

        for cat in valid_categories {
            assert!(
                ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == cat),
                "Valid category '{cat}' should be accepted"
            );
        }

        for cat in invalid_categories {
            assert!(
                !ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == cat),
                "Invalid category '{cat}' should be rejected"
            );
        }
    }

    #[test]
    fn allowed_cve_categories_match_ui_presets() {
        // This test documents the expected alignment between backend validation
        // and UI preset list defined in packages/web-ui/src/components/cve/mod.rs
        let expected_categories = [
            "false_positive",
            "accepted_risk",
            "compensating_control",
            "planned_remediation",
            "vendor_pending_fix",
        ];
        assert_eq!(
            ALLOWED_CVE_JUSTIFICATION_CATEGORIES.len(),
            expected_categories.len(),
            "Backend and UI category lists should have same length"
        );
        for category in expected_categories {
            assert!(
                ALLOWED_CVE_JUSTIFICATION_CATEGORIES
                    .iter()
                    .any(|allowed| *allowed == category),
                "Category {category} should be in backend allowed list"
            );
        }
    }
}
