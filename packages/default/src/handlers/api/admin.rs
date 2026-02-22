use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::api::models::{
    AdminUserSummary, ApiError, AuditAction, AuditEvent, IdentitySource, OidcGroupMapping,
    PaginatedResponse, Role,
};
use crate::handlers::api::rbac::require_admin as require_admin_user;
use crate::models::auth_identity::AuthRole;
use crate::queries::auth_identity::{assign_role_to_user, get_user_roles};
use crate::queries::users::insert_user;

#[derive(Debug, sqlx::FromRow)]
struct AdminUserRow {
    id: Uuid,
    username: String,
    email: String,
    is_active: bool,
    has_external_identity: bool,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct AuditRoleRow {
    created_at: DateTime<Utc>,
    actor_email: Option<String>,
    role: AuthRole,
    target_email: String,
}

#[derive(Debug, sqlx::FromRow)]
struct AuditSessionRow {
    invalidated_at: DateTime<Utc>,
    user_email: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct AdminAuditEventRow {
    created_at: DateTime<Utc>,
    actor_identifier: Option<String>,
    action: String,
    target: String,
    request_origin: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentMembershipRow {
    environment_name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentLookupRow {
    id: Uuid,
    name: String,
}

#[derive(Debug, sqlx::FromRow)]
struct OidcMappingRow {
    id: Uuid,
    group_name: String,
    role: Option<AuthRole>,
    environments: Vec<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminUserRequest {
    pub email: String,
    pub display_name: Option<String>,
    pub role: Role,
    #[serde(default)]
    pub environments: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAdminUserRequest {
    pub role: Option<Role>,
    pub enabled: Option<bool>,
    pub environments: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpsertOidcMappingRequest {
    pub group_name: String,
    pub role: Option<Role>,
    #[serde(default)]
    pub environments: Vec<String>,
}

pub async fn list_users(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    let Some(_admin_user) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    let rows = match sqlx::query_as::<_, AdminUserRow>(
        "SELECT u.id,
                u.username,
                u.email,
                u.is_active,
                EXISTS(SELECT 1 FROM external_identities ei WHERE ei.user_id = u.id) AS has_external_identity,
                u.updated_at
         FROM users u
         ORDER BY u.updated_at DESC",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to list admin users"),
    };

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let role = match get_user_roles(&pool, row.id).await {
            Ok(roles) => highest_role(roles.into_iter().map(|r| r.role).collect()),
            Err(_) => None,
        };

        let environments = load_user_environments(&pool, row.id)
            .await
            .unwrap_or_default();

        result.push(AdminUserSummary {
            id: row.id.to_string(),
            identifier: if row.username.trim().is_empty() {
                row.email
            } else {
                format!("{} ({})", row.username, row.email)
            },
            identity_source: if row.has_external_identity {
                IdentitySource::OidcDerived
            } else {
                IdentitySource::LocalManaged
            },
            role,
            enabled: row.is_active,
            environments,
            updated_at: row.updated_at,
        });
    }

    (StatusCode::OK, Json(result)).into_response()
}

pub async fn list_oidc_mappings(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(_admin_user) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    let rows = match sqlx::query_as::<_, OidcMappingRow>(
        "SELECT id, group_name, role, environments, updated_at
         FROM oidc_group_mappings
         ORDER BY group_name ASC",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to list OIDC mappings"),
    };

    let mappings = rows.into_iter().map(to_oidc_mapping).collect::<Vec<_>>();
    (StatusCode::OK, Json(mappings)).into_response()
}

pub async fn upsert_oidc_mapping(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<UpsertOidcMappingRequest>,
) -> impl IntoResponse {
    let Some(admin_user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    let group_name = match normalize_oidc_group_name(&payload.group_name) {
        Ok(value) => value,
        Err(message) => return bad_request(&message),
    };

    let environments = match normalize_environment_names_with_duplicate_check(&payload.environments) {
        Ok(value) => value,
        Err(message) => return bad_request(&message),
    };

    if let Err(message) = validate_environment_names_exist(&pool, &environments).await {
        return bad_request(&message);
    }

    let role = payload.role.map(role_to_auth_role);

    let row = match sqlx::query_as::<_, OidcMappingRow>(
        "INSERT INTO oidc_group_mappings (group_name, role, environments)
         VALUES ($1, $2, $3)
         ON CONFLICT (group_name)
         DO UPDATE SET role = EXCLUDED.role, environments = EXCLUDED.environments
         RETURNING id, group_name, role, environments, updated_at",
    )
    .bind(&group_name)
    .bind(role)
    .bind(&environments)
    .fetch_one(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to save OIDC mapping"),
    };

    if record_admin_audit_event(
        &pool,
        admin_user_id,
        AuditAction::OidcMappingChanged,
        format!("group:{}", row.group_name),
        extract_request_origin(&headers),
        serde_json::json!({ "role": row.role, "environments": row.environments }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    (StatusCode::OK, Json(to_oidc_mapping(row))).into_response()
}

pub async fn delete_oidc_mapping(
    State(pool): State<PgPool>,
    Path(mapping_id): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(admin_user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    let mapping_id = match Uuid::parse_str(&mapping_id) {
        Ok(value) => value,
        Err(_) => return bad_request("Mapping id must be a valid UUID"),
    };

    let deleted = match sqlx::query_as::<_, OidcMappingRow>(
        "DELETE FROM oidc_group_mappings
         WHERE id = $1
         RETURNING id, group_name, role, environments, updated_at",
    )
    .bind(mapping_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to delete OIDC mapping"),
    };

    let Some(mapping) = deleted else {
        return not_found("OIDC mapping not found");
    };

    if record_admin_audit_event(
        &pool,
        admin_user_id,
        AuditAction::OidcMappingChanged,
        format!("group:{}", mapping.group_name),
        extract_request_origin(&headers),
        serde_json::json!({ "deleted": true }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    StatusCode::NO_CONTENT.into_response()
}

pub async fn create_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<CreateAdminUserRequest>,
) -> impl IntoResponse {
    let Some(admin_user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };
    let request_origin = extract_request_origin(&headers);

    let email = payload.email.trim().to_ascii_lowercase();
    if !is_valid_email(&email) {
        return bad_request("Email must be a valid address");
    }

    let exists = match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count > 0,
        Err(_) => return internal_error("Failed to validate user email"),
    };
    if exists {
        return conflict("Email already exists");
    }

    let user = match insert_user(&pool, &email, payload.display_name.as_deref()).await {
        Ok(user) => user,
        Err(_) => return internal_error("Failed to create user"),
    };

    if set_user_primary_role(&pool, user.id, payload.role, admin_user_id)
        .await
        .is_err()
    {
        return internal_error("Failed to assign role");
    }

    if let Err(message) =
        sync_user_environments(&pool, user.id, admin_user_id, &payload.environments).await
    {
        return bad_request(&message);
    }

    if record_admin_audit_event(
        &pool,
        admin_user_id,
        AuditAction::UserCreated,
        format!("{} ({})", email, user.id),
        request_origin.clone(),
        serde_json::json!({ "display_name": payload.display_name }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    if record_admin_audit_event(
        &pool,
        admin_user_id,
        AuditAction::UserRoleAssigned,
        format!("{} ({})", email, user.id),
        request_origin.clone(),
        serde_json::json!({ "role": payload.role }),
    )
    .await
    .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    if !payload.environments.is_empty()
        && record_admin_audit_event(
            &pool,
            admin_user_id,
            AuditAction::UserEnvironmentMembershipUpdated,
            format!("{} ({})", email, user.id),
            request_origin,
            serde_json::json!({ "environments": payload.environments }),
        )
        .await
        .is_err()
    {
        return internal_error("Failed to write audit event");
    }

    match fetch_admin_user_summary(&pool, user.id).await {
        Ok(summary) => (StatusCode::CREATED, Json(summary)).into_response(),
        Err(_) => internal_error("Failed to load created user"),
    }
}

pub async fn update_user(
    State(pool): State<PgPool>,
    Path(user_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpdateAdminUserRequest>,
) -> impl IntoResponse {
    let Some(admin_user_id) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };
    let request_origin = extract_request_origin(&headers);

    let target_user_id = match Uuid::parse_str(&user_id) {
        Ok(value) => value,
        Err(_) => return bad_request("User id must be a valid UUID"),
    };

    let current = match sqlx::query_as::<_, AdminUserRow>(
        "SELECT id, username, email, is_active, updated_at FROM users WHERE id = $1",
    )
    .bind(target_user_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(value)) => value,
        Ok(None) => return not_found("User not found"),
        Err(_) => return internal_error("Failed to load user"),
    };

    let roles = match get_user_roles(&pool, target_user_id).await {
        Ok(value) => value,
        Err(_) => return internal_error("Failed to load user roles"),
    };
    let had_admin = roles.iter().any(|role| role.role == AuthRole::Admin);
    let current_role = highest_role(roles.iter().map(|role| role.role).collect());
    let prior_environments = load_user_environments(&pool, target_user_id)
        .await
        .unwrap_or_default();
    let mut audit_events: Vec<(AuditAction, serde_json::Value)> = vec![];

    if let Some(enabled) = payload.enabled {
        if should_block_last_admin_disable(enabled, current.is_active, had_admin) {
            match enabled_admin_count(&pool).await {
                Ok(1) => return conflict("Cannot disable the last enabled admin"),
                Ok(_) => {}
                Err(_) => return internal_error("Failed to validate admin guardrail"),
            }
        }

        if sqlx::query("UPDATE users SET is_active = $1 WHERE id = $2")
            .bind(enabled)
            .bind(target_user_id)
            .execute(&pool)
            .await
            .is_err()
        {
            return internal_error("Failed to update user status");
        }

        if enabled != current.is_active {
            audit_events.push((
                if enabled {
                    AuditAction::UserEnabled
                } else {
                    AuditAction::UserDisabled
                },
                serde_json::json!({ "enabled": enabled }),
            ));
        }
    }

    if let Some(role) = payload.role {
        if should_block_final_admin_role_removal(role, had_admin, current.is_active) {
            match enabled_admin_count(&pool).await {
                Ok(1) => return conflict("Cannot remove the final admin role assignment"),
                Ok(_) => {}
                Err(_) => return internal_error("Failed to validate admin guardrail"),
            }
        }

        if set_user_primary_role(&pool, target_user_id, role, admin_user_id)
            .await
            .is_err()
        {
            return internal_error("Failed to update user role");
        }

        if Some(role) != current_role {
            audit_events.push((
                AuditAction::UserRoleAssigned,
                serde_json::json!({ "from": current_role, "to": role }),
            ));
        }
    }

    if let Some(environments) = payload.environments {
        if let Err(message) =
            sync_user_environments(&pool, target_user_id, admin_user_id, &environments).await
        {
            return bad_request(&message);
        }

        let mut after = environments
            .iter()
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        after.sort();
        let mut before = prior_environments.clone();
        before.sort();
        if before != after {
            audit_events.push((
                AuditAction::UserEnvironmentMembershipUpdated,
                serde_json::json!({ "from": prior_environments, "to": environments }),
            ));
        }
    }

    if !audit_events.is_empty() {
        let actor = fetch_user_identifier(&pool, admin_user_id)
            .await
            .unwrap_or_else(|| admin_user_id.to_string());
        let target = format!("{} ({})", current.email, current.id);

        if record_admin_audit_event(
            &pool,
            admin_user_id,
            AuditAction::UserUpdated,
            target.clone(),
            request_origin.clone(),
            serde_json::json!({
                "actor": actor,
                "change_count": audit_events.len(),
            }),
        )
        .await
        .is_err()
        {
            return internal_error("Failed to write audit event");
        }

        for (action, metadata) in audit_events {
            if record_admin_audit_event(
                &pool,
                admin_user_id,
                action,
                target.clone(),
                request_origin.clone(),
                serde_json::json!({
                    "actor": actor,
                    "changes": metadata,
                }),
            )
            .await
            .is_err()
            {
                return internal_error("Failed to write audit event");
            }
        }
    }

    match fetch_admin_user_summary(&pool, target_user_id).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(_) => internal_error("Failed to load updated user"),
    }
}

pub async fn list_audit_events(
    State(pool): State<PgPool>,
    Query(params): Query<AuditEventsQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(_admin_user) = require_admin_user(&pool, &headers).await else {
        return forbidden();
    };

    let mut events: Vec<AuditEvent> = vec![];

    let admin_events = match sqlx::query_as::<_, AdminAuditEventRow>(
        "SELECT created_at, actor_identifier, action, target, request_origin
         FROM admin_audit_events
         ORDER BY created_at DESC
         LIMIT 300",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return internal_error("Failed to load audit events"),
    };

    for row in admin_events {
        let Some(action) = parse_audit_action(&row.action) else {
            continue;
        };
        events.push(AuditEvent {
            timestamp: row.created_at,
            actor: row.actor_identifier,
            action,
            target: row.target,
            source: row
                .request_origin
                .unwrap_or_else(|| "admin_audit_events".to_string()),
        });
    }

    let role_events = match sqlx::query_as::<_, AuditRoleRow>(
        "SELECT ura.created_at,
                actor.email AS actor_email,
                ura.role,
                target.email AS target_email
         FROM user_role_assignments ura
         LEFT JOIN users actor ON actor.id = ura.granted_by_user_id
         JOIN users target ON target.id = ura.user_id
         ORDER BY ura.created_at DESC
         LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return internal_error("Failed to load audit events"),
    };

    for row in role_events {
        events.push(AuditEvent {
            timestamp: row.created_at,
            actor: row.actor_email,
            action: AuditAction::UserRoleAssigned,
            target: format!("{} -> {:?}", row.target_email, row.role),
            source: "user_role_assignments".to_string(),
        });
    }

    let session_events = match sqlx::query_as::<_, AuditSessionRow>(
        "SELECT us.invalidated_at, u.email AS user_email
         FROM user_sessions us
         LEFT JOIN users u ON u.id = us.user_id
         WHERE us.invalidated_at IS NOT NULL
         ORDER BY us.invalidated_at DESC
         LIMIT 100",
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return internal_error("Failed to load audit events"),
    };

    for row in session_events {
        events.push(AuditEvent {
            timestamp: row.invalidated_at,
            actor: row.user_email.clone(),
            action: AuditAction::SessionInvalidated,
            target: row.user_email.unwrap_or_else(|| "unknown user".to_string()),
            source: "user_sessions".to_string(),
        });
    }

    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let filtered = match apply_audit_filters(events, &params) {
        Ok(value) => value,
        Err(message) => return bad_request(&message),
    };

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(25).clamp(1, 200);

    let paginated = paginate_events(filtered, page, per_page);

    (StatusCode::OK, Json(paginated)).into_response()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuditEventsQuery {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

fn apply_audit_filters(
    mut events: Vec<AuditEvent>,
    params: &AuditEventsQuery,
) -> Result<Vec<AuditEvent>, String> {
    let actor_filter = params
        .actor
        .as_ref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    let action_filter = params
        .action
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_action_filter)
        .transpose()?
        .flatten();

    let from = params
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_rfc3339)
        .transpose()?;

    let to = params
        .to
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_rfc3339)
        .transpose()?;

    if let (Some(start), Some(end)) = (from, to) {
        if start > end {
            return Err("`from` must be less than or equal to `to`".to_string());
        }
    }

    events.retain(|event| {
        if let Some(actor) = &actor_filter {
            let event_actor = event
                .actor
                .as_deref()
                .unwrap_or("system")
                .to_ascii_lowercase();
            if !event_actor.contains(actor) {
                return false;
            }
        }

        if let Some(action) = action_filter {
            if event.action != action {
                return false;
            }
        }

        if let Some(start) = from {
            if event.timestamp < start {
                return false;
            }
        }

        if let Some(end) = to {
            if event.timestamp > end {
                return false;
            }
        }

        true
    });

    Ok(events)
}

fn parse_action_filter(value: &str) -> Result<Option<AuditAction>, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(None);
    }

    parse_audit_action(&normalized)
        .map(Some)
        .ok_or_else(|| format!("invalid action `{value}`"))
}

fn parse_audit_action(value: &str) -> Option<AuditAction> {
    match value {
        "user_created" => Some(AuditAction::UserCreated),
        "user_updated" => Some(AuditAction::UserUpdated),
        "user_enabled" => Some(AuditAction::UserEnabled),
        "user_disabled" => Some(AuditAction::UserDisabled),
        "user_role_assigned" => Some(AuditAction::UserRoleAssigned),
        "user_environment_membership_updated" => {
            Some(AuditAction::UserEnvironmentMembershipUpdated)
        }
        "oidc_mapping_changed" => Some(AuditAction::OidcMappingChanged),
        "session_invalidated" => Some(AuditAction::SessionInvalidated),
        _ => None,
    }
}

fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| format!("invalid RFC3339 timestamp `{value}`"))
}

fn paginate_events(
    events: Vec<AuditEvent>,
    page: i64,
    per_page: i64,
) -> PaginatedResponse<AuditEvent> {
    let total = events.len() as i64;
    let start_index = ((page - 1) * per_page).max(0) as usize;
    let items = if start_index >= events.len() {
        vec![]
    } else {
        events
            .into_iter()
            .skip(start_index)
            .take(per_page as usize)
            .collect()
    };

    PaginatedResponse {
        items,
        total,
        page,
        per_page,
    }
}

fn highest_role(roles: Vec<AuthRole>) -> Option<Role> {
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

async fn fetch_admin_user_summary(pool: &PgPool, user_id: Uuid) -> Result<AdminUserSummary, ()> {
    let row = sqlx::query_as::<_, AdminUserRow>(
        "SELECT u.id,
                u.username,
                u.email,
                u.is_active,
                EXISTS(SELECT 1 FROM external_identities ei WHERE ei.user_id = u.id) AS has_external_identity,
                u.updated_at
         FROM users u
         WHERE u.id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|_| ())?;

    let roles = get_user_roles(pool, user_id).await.map_err(|_| ())?;
    let role = highest_role(roles.into_iter().map(|r| r.role).collect());
    let environments = load_user_environments(pool, user_id)
        .await
        .map_err(|_| ())?;

    Ok(AdminUserSummary {
        id: row.id.to_string(),
        identifier: if row.username.trim().is_empty() {
            row.email
        } else {
            format!("{} ({})", row.username, row.email)
        },
        identity_source: if row.has_external_identity {
            IdentitySource::OidcDerived
        } else {
            IdentitySource::LocalManaged
        },
        role,
        enabled: row.is_active,
        environments,
        updated_at: row.updated_at,
    })
}

async fn load_user_environments(pool: &PgPool, user_id: Uuid) -> Result<Vec<String>, ()> {
    sqlx::query_as::<_, EnvironmentMembershipRow>(
        "SELECT e.name AS environment_name
         FROM user_environment_memberships uem
         JOIN environments e ON e.id = uem.environment_id
         WHERE uem.user_id = $1
         ORDER BY e.name ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row.environment_name).collect())
    .map_err(|_| ())
}

async fn enabled_admin_count(pool: &PgPool) -> Result<i64, ()> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT u.id)
         FROM users u
         JOIN user_role_assignments ura ON ura.user_id = u.id
         WHERE u.is_active = TRUE AND ura.role = 'admin'",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| ())
}

async fn set_user_primary_role(
    pool: &PgPool,
    user_id: Uuid,
    role: Role,
    granted_by_user_id: Uuid,
) -> Result<(), ()> {
    sqlx::query("DELETE FROM user_role_assignments WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|_| ())?;

    assign_role_to_user(
        pool,
        user_id,
        role_to_auth_role(role),
        Some(granted_by_user_id),
    )
    .await
    .map_err(|_| ())?;

    Ok(())
}

async fn sync_user_environments(
    pool: &PgPool,
    user_id: Uuid,
    assigned_by_user_id: Uuid,
    environments: &[String],
) -> Result<(), String> {
    let normalized: Vec<String> = environments
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "Failed to start membership update".to_string())?;

    sqlx::query("DELETE FROM user_environment_memberships WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "Failed to update environment memberships".to_string())?;

    if normalized.is_empty() {
        tx.commit()
            .await
            .map_err(|_| "Failed to commit environment memberships".to_string())?;
        return Ok(());
    }

    let resolved = sqlx::query_as::<_, EnvironmentLookupRow>(
        "SELECT id, name FROM environments WHERE name = ANY($1)",
    )
    .bind(&normalized)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| "Failed to validate environments".to_string())?;

    if resolved.len() != normalized.len() {
        let known = resolved
            .iter()
            .map(|row| row.name.clone())
            .collect::<BTreeSet<_>>();
        let missing = normalized
            .iter()
            .filter(|name| !known.contains((*name).as_str()))
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!("Unknown environment(s): {}", missing.join(", ")));
    }

    for env in resolved {
        sqlx::query(
            "INSERT INTO user_environment_memberships (user_id, environment_id, assigned_by_user_id)
             VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(env.id)
        .bind(assigned_by_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "Failed to insert environment membership".to_string())?;
    }

    tx.commit()
        .await
        .map_err(|_| "Failed to commit environment memberships".to_string())?;
    Ok(())
}

fn normalize_oidc_group_name(value: &str) -> Result<String, String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err("Group name is required".to_string());
    }

    if normalized.len() > 128 {
        return Err("Group name must be 128 characters or fewer".to_string());
    }

    let valid = normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'));
    if !valid {
        return Err(
            "Group name may only contain letters, numbers, '-', '_', '.', ':', '/'".to_string(),
        );
    }

    Ok(normalized)
}

fn normalize_environment_names_with_duplicate_check(
    values: &[String],
) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    let mut duplicates = BTreeSet::new();

    for value in values {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            continue;
        }

        if !seen.insert(normalized.clone()) {
            duplicates.insert(normalized);
        }
    }

    if !duplicates.is_empty() {
        return Err(format!(
            "Duplicate environment(s): {}",
            duplicates.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    Ok(seen.into_iter().collect())
}

async fn validate_environment_names_exist(pool: &PgPool, names: &[String]) -> Result<(), String> {
    if names.is_empty() {
        return Ok(());
    }

    let resolved = sqlx::query_as::<_, EnvironmentLookupRow>(
        "SELECT id, name FROM environments WHERE name = ANY($1)",
    )
    .bind(names)
    .fetch_all(pool)
    .await
    .map_err(|_| "Failed to validate environments".to_string())?;

    if resolved.len() == names.len() {
        return Ok(());
    }

    let known = resolved
        .into_iter()
        .map(|row| row.name)
        .collect::<BTreeSet<_>>();
    let missing = names
        .iter()
        .filter(|name| !known.contains((*name).as_str()))
        .cloned()
        .collect::<Vec<_>>();

    Err(format!("Unknown environment(s): {}", missing.join(", ")))
}

fn role_to_auth_role(role: Role) -> AuthRole {
    match role {
        Role::Admin => AuthRole::Admin,
        Role::Operator => AuthRole::Operator,
        Role::Viewer => AuthRole::Viewer,
    }
}

fn action_to_str(action: AuditAction) -> &'static str {
    match action {
        AuditAction::UserCreated => "user_created",
        AuditAction::UserUpdated => "user_updated",
        AuditAction::UserEnabled => "user_enabled",
        AuditAction::UserDisabled => "user_disabled",
        AuditAction::UserRoleAssigned => "user_role_assigned",
        AuditAction::UserEnvironmentMembershipUpdated => "user_environment_membership_updated",
        AuditAction::OidcMappingChanged => "oidc_mapping_changed",
        AuditAction::SessionInvalidated => "session_invalidated",
    }
}

fn to_oidc_mapping(row: OidcMappingRow) -> OidcGroupMapping {
    OidcGroupMapping {
        id: row.id.to_string(),
        group_name: row.group_name,
        role: row.role.map(auth_role_to_role),
        environments: row.environments,
        updated_at: row.updated_at,
    }
}

fn auth_role_to_role(role: AuthRole) -> Role {
    match role {
        AuthRole::Admin => Role::Admin,
        AuthRole::Operator => Role::Operator,
        AuthRole::Viewer => Role::Viewer,
    }
}

async fn fetch_user_identifier(pool: &PgPool, user_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

async fn record_admin_audit_event(
    pool: &PgPool,
    actor_user_id: Uuid,
    action: AuditAction,
    target: String,
    request_origin: Option<String>,
    metadata: serde_json::Value,
) -> Result<(), ()> {
    let actor_identifier = fetch_user_identifier(pool, actor_user_id)
        .await
        .unwrap_or_else(|| actor_user_id.to_string());

    sqlx::query(
        "INSERT INTO admin_audit_events (actor_user_id, actor_identifier, action, target, request_origin, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor_user_id)
    .bind(actor_identifier)
    .bind(action_to_str(action))
    .bind(target)
    .bind(request_origin)
    .bind(metadata)
    .execute(pool)
    .await
    .map_err(|_| ())?;

    Ok(())
}

fn extract_request_origin(headers: &HeaderMap) -> Option<String> {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if forwarded.is_some() {
        return forwarded;
    }

    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn should_block_last_admin_disable(
    enabled: bool,
    is_currently_active: bool,
    had_admin: bool,
) -> bool {
    !enabled && is_currently_active && had_admin
}

fn should_block_final_admin_role_removal(
    next_role: Role,
    had_admin: bool,
    is_currently_active: bool,
) -> bool {
    next_role != Role::Admin && had_admin && is_currently_active
}

fn is_valid_email(value: &str) -> bool {
    if value.is_empty() || value.len() > 255 {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.')
}

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Admin role required".to_string(),
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

fn conflict(message: &str) -> axum::response::Response {
    (
        StatusCode::CONFLICT,
        Json(ApiError {
            error: "conflict".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn not_found(message: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "not_found".to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use chrono::TimeZone;
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn highest_role_prefers_admin_then_operator_then_viewer() {
        assert_eq!(highest_role(vec![AuthRole::Viewer]), Some(Role::Viewer));
        assert_eq!(
            highest_role(vec![AuthRole::Viewer, AuthRole::Operator]),
            Some(Role::Operator)
        );
        assert_eq!(
            highest_role(vec![AuthRole::Viewer, AuthRole::Admin]),
            Some(Role::Admin)
        );
        assert_eq!(highest_role(vec![]), None);
    }

    #[test]
    fn has_admin_role_only_accepts_admin_role() {
        assert!(crate::handlers::api::rbac::has_admin_role(&[AuthRole::Admin]));
        assert!(!crate::handlers::api::rbac::has_admin_role(&[AuthRole::Operator]));
        assert!(!crate::handlers::api::rbac::has_admin_role(&[AuthRole::Viewer]));
        assert!(!crate::handlers::api::rbac::has_admin_role(&[
            AuthRole::Viewer,
            AuthRole::Operator,
        ]));
    }

    fn event(timestamp: DateTime<Utc>, actor: Option<&str>, action: AuditAction) -> AuditEvent {
        AuditEvent {
            timestamp,
            actor: actor.map(str::to_string),
            action,
            target: "target".to_string(),
            source: "source".to_string(),
        }
    }

    #[test]
    fn apply_audit_filters_filters_by_actor_and_action() {
        let events = vec![
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 10, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("admin@example.com"),
                AuditAction::UserRoleAssigned,
            ),
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 11, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("operator@example.com"),
                AuditAction::SessionInvalidated,
            ),
        ];

        let params = AuditEventsQuery {
            actor: Some("admin".to_string()),
            action: Some("user_role_assigned".to_string()),
            ..Default::default()
        };

        let filtered = apply_audit_filters(events, &params).expect("filter succeeds");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].action, AuditAction::UserRoleAssigned);
    }

    #[test]
    fn apply_audit_filters_rejects_invalid_range() {
        let params = AuditEventsQuery {
            from: Some("2026-02-21T12:00:00Z".to_string()),
            to: Some("2026-02-21T10:00:00Z".to_string()),
            ..Default::default()
        };

        let err = apply_audit_filters(vec![], &params).expect_err("range should fail");
        assert!(err.contains("from"));
    }

    #[test]
    fn paginate_events_returns_expected_window() {
        let events = vec![
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 10, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("a@example.com"),
                AuditAction::UserRoleAssigned,
            ),
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 11, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("b@example.com"),
                AuditAction::SessionInvalidated,
            ),
            event(
                Utc.with_ymd_and_hms(2026, 2, 21, 12, 0, 0)
                    .single()
                    .expect("valid timestamp"),
                Some("c@example.com"),
                AuditAction::SessionInvalidated,
            ),
        ];

        let page = paginate_events(events, 2, 2);
        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 1);
    }

    #[test]
    fn validates_email_format() {
        assert!(is_valid_email("admin@example.com"));
        assert!(!is_valid_email(""));
        assert!(!is_valid_email("missing-at"));
        assert!(!is_valid_email("missing-domain@"));
    }

    #[test]
    fn blocks_last_admin_disable_only_for_active_admin() {
        assert!(should_block_last_admin_disable(true, true, true) == false);
        assert!(should_block_last_admin_disable(false, true, true));
        assert!(should_block_last_admin_disable(false, false, true) == false);
        assert!(should_block_last_admin_disable(false, true, false) == false);
    }

    #[test]
    fn blocks_final_admin_role_removal_only_when_demoting_active_admin() {
        assert!(should_block_final_admin_role_removal(Role::Admin, true, true) == false);
        assert!(should_block_final_admin_role_removal(
            Role::Operator,
            true,
            true
        ));
        assert!(should_block_final_admin_role_removal(Role::Viewer, true, false) == false);
        assert!(should_block_final_admin_role_removal(Role::Viewer, false, true) == false);
    }

    #[test]
    fn normalize_oidc_group_name_requires_allowed_characters() {
        assert_eq!(
            normalize_oidc_group_name(" Team:Platform/Admin ").expect("valid group"),
            "team:platform/admin"
        );
        assert!(normalize_oidc_group_name("").is_err());
        assert!(normalize_oidc_group_name("engineering team").is_err());
    }

    #[test]
    fn normalize_environment_names_rejects_duplicates() {
        let err = normalize_environment_names_with_duplicate_check(&[
            "prod".to_string(),
            "PROD".to_string(),
            "staging".to_string(),
        ])
        .expect_err("duplicate should fail");

        assert!(err.contains("Duplicate environment(s): prod"));
    }

    #[test]
    fn normalize_environment_names_trims_and_sorts() {
        let values = normalize_environment_names_with_duplicate_check(&[
            " staging ".to_string(),
            "prod".to_string(),
            "".to_string(),
        ])
        .expect("normalization should succeed");

        assert_eq!(values, vec!["prod".to_string(), "staging".to_string()]);
    }

    #[test]
    fn extract_request_origin_prefers_forwarded_over_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.5".parse().expect("valid header"));
        headers.insert("x-real-ip", "203.0.113.10".parse().expect("valid header"));

        assert_eq!(
            extract_request_origin(&headers),
            Some("198.51.100.5".to_string())
        );
    }

    #[test]
    fn extract_request_origin_falls_back_to_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.10".parse().expect("valid header"));

        assert_eq!(
            extract_request_origin(&headers),
            Some("203.0.113.10".to_string())
        );
    }

    #[test]
    fn action_to_str_covers_admin_audit_variants() {
        assert_eq!(action_to_str(AuditAction::UserCreated), "user_created");
        assert_eq!(action_to_str(AuditAction::UserUpdated), "user_updated");
        assert_eq!(action_to_str(AuditAction::UserEnabled), "user_enabled");
        assert_eq!(action_to_str(AuditAction::UserDisabled), "user_disabled");
        assert_eq!(
            action_to_str(AuditAction::UserRoleAssigned),
            "user_role_assigned"
        );
        assert_eq!(
            action_to_str(AuditAction::UserEnvironmentMembershipUpdated),
            "user_environment_membership_updated"
        );
        assert_eq!(
            action_to_str(AuditAction::OidcMappingChanged),
            "oidc_mapping_changed"
        );
        assert_eq!(
            action_to_str(AuditAction::SessionInvalidated),
            "session_invalidated"
        );
    }

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://postgres:postgres@localhost/cf_test")
            .expect("lazy pool should construct")
    }

    #[tokio::test]
    async fn create_user_requires_admin_session() {
        let response = create_user(
            State(lazy_pool()),
            HeaderMap::new(),
            Json(CreateAdminUserRequest {
                email: "new-user@example.com".to_string(),
                display_name: Some("New User".to_string()),
                role: Role::Viewer,
                environments: vec![],
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn update_user_requires_admin_session() {
        let response = update_user(
            State(lazy_pool()),
            Path(Uuid::new_v4().to_string()),
            HeaderMap::new(),
            Json(UpdateAdminUserRequest {
                role: Some(Role::Operator),
                enabled: Some(true),
                environments: Some(vec![]),
            }),
        )
        .await
        .into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn parses_extended_audit_actions() {
        assert_eq!(
            parse_audit_action("user_created"),
            Some(AuditAction::UserCreated)
        );
        assert_eq!(
            parse_audit_action("user_updated"),
            Some(AuditAction::UserUpdated)
        );
        assert_eq!(
            parse_audit_action("user_environment_membership_updated"),
            Some(AuditAction::UserEnvironmentMembershipUpdated)
        );
        assert_eq!(
            parse_audit_action("oidc_mapping_changed"),
            Some(AuditAction::OidcMappingChanged)
        );
        assert_eq!(parse_audit_action("unknown"), None);
    }
}
