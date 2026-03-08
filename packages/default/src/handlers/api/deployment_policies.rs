//! API handlers for deployment policy management (CRUD operations).
//!
//! This module provides REST endpoints for managing deployment policies with RBAC enforcement:
//! - GET endpoints: Available to all authenticated users (Admin/Operator/Viewer)
//! - POST/PUT endpoints: Available to Admin and Operator roles only
//! - DELETE endpoint: Available to Admin role only

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::extractors::{RequireAdmin, RequireAuth, RequireOperator};
use crate::handlers::agent_request::CFState;
use crate::models::deployment_policies::{
    CreateDeploymentPolicyRequest, DeploymentPolicyRecord, UpdateDeploymentPolicyRequest,
};
use crate::queries::deployment_policies;

// =============================================================================
// Response Models
// =============================================================================

#[derive(Debug, Serialize)]
pub struct DeploymentPoliciesListResponse {
    pub policies: Vec<DeploymentPolicyRecord>,
    pub total: usize,
    pub limit: i64,
    pub offset: i64,
}

// =============================================================================
// Query Parameters
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

impl PaginationParams {
    /// Validate and normalize pagination parameters
    fn validate(&self) -> Result<(), (StatusCode, String)> {
        if self.limit < 1 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Limit must be at least 1".to_string(),
            ));
        }
        if self.limit > 1000 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Limit cannot exceed 1000".to_string(),
            ));
        }
        if self.offset < 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Offset cannot be negative".to_string(),
            ));
        }
        Ok(())
    }
}

// =============================================================================
// CRUD Endpoints
// =============================================================================

/// GET /api/v1/deployment-policies - List deployment policies with pagination
///
/// Available to all authenticated users (Admin/Operator/Viewer).
///
/// Query parameters:
/// - limit: Maximum number of policies to return (default 100, max 1000)
/// - offset: Number of policies to skip (default 0)
pub async fn list_deployment_policies(
    RequireAuth(_user): RequireAuth,
    State(state): State<CFState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<DeploymentPoliciesListResponse>, (StatusCode, String)> {
    // Validate pagination parameters
    params.validate()?;

    // Fetch policies from database
    let policies = deployment_policies::list_deployment_policies(
        &state.pool,
        params.limit,
        params.offset,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list deployment policies: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to retrieve deployment policies".to_string(),
        )
    })?;

    let total = policies.len();

    Ok(Json(DeploymentPoliciesListResponse {
        policies,
        total,
        limit: params.limit,
        offset: params.offset,
    }))
}

/// GET /api/v1/deployment-policies/:id - Get deployment policy details
///
/// Available to all authenticated users (Admin/Operator/Viewer).
///
/// Returns 404 if the policy does not exist.
pub async fn get_deployment_policy(
    RequireAuth(_user): RequireAuth,
    State(state): State<CFState>,
    Path(policy_id): Path<Uuid>,
) -> Result<Json<DeploymentPolicyRecord>, (StatusCode, String)> {
    let policy = deployment_policies::get_deployment_policy_by_id(&state.pool, &policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch deployment policy {}: {}", policy_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to retrieve deployment policy".to_string(),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Deployment policy not found".to_string(),
        ))?;

    Ok(Json(policy))
}

/// POST /api/v1/deployment-policies - Create a new deployment policy
///
/// Available to Admin and Operator roles only.
///
/// Request body:
/// ```json
/// {
///   "name": "policy-name",
///   "description": "Policy description",
///   "policy_type": "require_cf_agent|require_packages|custom_check",
///   "config": { "strict": true, ... },
///   "enabled": true
/// }
/// ```
///
/// Returns 400 for validation errors.
/// Returns 409 if a policy with the same name already exists.
pub async fn create_deployment_policy(
    RequireOperator(_user): RequireOperator,
    State(state): State<CFState>,
    Json(request): Json<CreateDeploymentPolicyRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut request = request;

    // Input validation
    if request.name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Policy name cannot be empty".to_string(),
        ));
    }
    if request.name.len() > 255 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Policy name too long (max 255 characters)".to_string(),
        ));
    }

    // Validate policy_type
    let valid_types = ["require_cf_agent", "require_packages", "custom_check"];
    if !valid_types.contains(&request.policy_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid policy_type '{}'. Must be one of: {}",
                request.policy_type,
                valid_types.join(", ")
            ),
        ));
    }

    // Validate config is valid JSON (already parsed by serde, but check it's not null)
    if request.config.is_null() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Policy config cannot be null".to_string(),
        ));
    }

    // Check if policy name already exists
    let name_exists = deployment_policies::check_policy_name_exists(&state.pool, &request.name, None)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check policy name existence: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to validate policy name".to_string(),
            )
        })?;

    if name_exists {
        return Err((
            StatusCode::CONFLICT,
            format!("A policy named '{}' already exists", request.name),
        ));
    }

    // Core policy is always enabled
    if request.policy_type == "require_cf_agent" {
        request.enabled = Some(true);
    }

    // Create policy
    let policy = deployment_policies::create_deployment_policy(&state.pool, &request)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create deployment policy: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create deployment policy".to_string(),
            )
        })?;

    Ok((StatusCode::CREATED, Json(policy)))
}

/// PUT /api/v1/deployment-policies/:id - Update an existing deployment policy
///
/// Available to Admin and Operator roles only.
///
/// Request body (all fields optional):
/// ```json
/// {
///   "name": "new-name",
///   "description": "Updated description",
///   "policy_type": "custom_check",
///   "config": { "strict": false },
///   "enabled": false
/// }
/// ```
///
/// Returns 400 for validation errors.
/// Returns 404 if the policy does not exist.
/// Returns 409 if the new name conflicts with an existing policy.
pub async fn update_deployment_policy(
    RequireOperator(_user): RequireOperator,
    State(state): State<CFState>,
    Path(policy_id): Path<Uuid>,
    Json(request): Json<UpdateDeploymentPolicyRequest>,
) -> Result<Json<DeploymentPolicyRecord>, (StatusCode, String)> {
    let mut request = request;

    let existing = deployment_policies::get_deployment_policy_by_id(&state.pool, &policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch deployment policy {}: {}", policy_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to retrieve deployment policy".to_string(),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Deployment policy not found".to_string(),
        ))?;

    // Core require_cf_agent policy is immutable except name/description/config updates.
    if existing.policy_type == "require_cf_agent" {
        if let Some(ref policy_type) = request.policy_type {
            if policy_type != "require_cf_agent" {
                return Err((
                    StatusCode::CONFLICT,
                    "Core require_cf_agent policy type cannot be changed".to_string(),
                ));
            }
        }
        if request.enabled == Some(false) {
            return Err((
                StatusCode::CONFLICT,
                "Core require_cf_agent policy is always enabled".to_string(),
            ));
        }
        request.enabled = Some(true);
    }

    // Validate name if provided
    if let Some(ref name) = request.name {
        if name.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Policy name cannot be empty".to_string(),
            ));
        }
        if name.len() > 255 {
            return Err((
                StatusCode::BAD_REQUEST,
                "Policy name too long (max 255 characters)".to_string(),
            ));
        }

        // Check for name conflicts (excluding current policy)
        let name_exists =
            deployment_policies::check_policy_name_exists(&state.pool, name, Some(&policy_id))
                .await
                .map_err(|e| {
                    tracing::error!("Failed to check policy name existence: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to validate policy name".to_string(),
                    )
                })?;

        if name_exists {
            return Err((
                StatusCode::CONFLICT,
                format!("A policy named '{}' already exists", name),
            ));
        }
    }

    // Validate policy_type if provided
    if let Some(ref policy_type) = request.policy_type {
        let valid_types = ["require_cf_agent", "require_packages", "custom_check"];
        if !valid_types.contains(&policy_type.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Invalid policy_type '{}'. Must be one of: {}",
                    policy_type,
                    valid_types.join(", ")
                ),
            ));
        }
    }

    // Validate config if provided
    if let Some(ref config) = request.config {
        if config.is_null() {
            return Err((
                StatusCode::BAD_REQUEST,
                "Policy config cannot be null".to_string(),
            ));
        }
    }

    // Update policy
    let policy =
        deployment_policies::update_deployment_policy(&state.pool, &policy_id, &request)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update deployment policy {}: {}", policy_id, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to update deployment policy".to_string(),
                )
            })?
            .ok_or((
                StatusCode::NOT_FOUND,
                "Deployment policy not found".to_string(),
            ))?;

    Ok(Json(policy))
}

/// DELETE /api/v1/deployment-policies/:id - Delete a deployment policy
///
/// Available to Admin role only.
///
/// Returns 404 if the policy does not exist.
/// Returns 409 if the policy is currently assigned to any environments or systems.
pub async fn delete_deployment_policy(
    RequireAdmin(_user): RequireAdmin,
    State(state): State<CFState>,
    Path(policy_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let existing = deployment_policies::get_deployment_policy_by_id(&state.pool, &policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch deployment policy {}: {}", policy_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to retrieve deployment policy".to_string(),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            "Deployment policy not found".to_string(),
        ))?;

    if existing.policy_type == "require_cf_agent" {
        return Err((
            StatusCode::CONFLICT,
            "Core require_cf_agent policy cannot be deleted".to_string(),
        ));
    }

    // Check if policy is in use
    let in_use = deployment_policies::check_policy_in_use(&state.pool, &policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to check if policy {} is in use: {}", policy_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to validate policy deletion".to_string(),
            )
        })?;

    if in_use {
        return Err((
            StatusCode::CONFLICT,
            "Cannot delete policy: it is currently assigned to one or more environments or systems"
                .to_string(),
        ));
    }

    // Delete policy
    let deleted = deployment_policies::delete_deployment_policy(&state.pool, &policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete deployment policy {}: {}", policy_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete deployment policy".to_string(),
            )
        })?;

    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            "Deployment policy not found".to_string(),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::db::test_pool;
    use sqlx::PgPool;

    async fn create_test_policy(pool: &PgPool, name: &str) -> Uuid {
        let request = CreateDeploymentPolicyRequest {
            name: name.to_string(),
            description: Some("Test policy".to_string()),
            policy_type: "custom_check".to_string(),
            config: serde_json::json!({"expression": "true"}),
            enabled: Some(true),
        };
        
        deployment_policies::create_deployment_policy(pool, &request)
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_list_deployment_policies_empty() {
        let pool = test_pool().await;
        let policies = deployment_policies::list_deployment_policies(&pool, 100, 0)
            .await
            .unwrap();
        
        // May have seed data, just verify it doesn't error
        assert!(policies.len() >= 0);
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_create_deployment_policy() {
        let pool = test_pool().await;
        let request = CreateDeploymentPolicyRequest {
            name: "Test Policy".to_string(),
            description: Some("Test description".to_string()),
            policy_type: "custom_check".to_string(),
            config: serde_json::json!({"expression": "config.services.ssh.enable"}),
            enabled: Some(true),
        };

        let policy = deployment_policies::create_deployment_policy(&pool, &request)
            .await
            .unwrap();

        assert_eq!(policy.name, "Test Policy");
        assert_eq!(policy.policy_type, "custom_check");
        assert_eq!(policy.enabled, true);
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_get_deployment_policy_by_id() {
        let pool = test_pool().await;
        let policy_id = create_test_policy(&pool, "Get Test Policy").await;

        let policy = deployment_policies::get_deployment_policy_by_id(&pool, &policy_id)
            .await
            .unwrap()
            .expect("Policy should exist");

        assert_eq!(policy.id, policy_id);
        assert_eq!(policy.name, "Get Test Policy");
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_update_deployment_policy() {
        let pool = test_pool().await;
        let policy_id = create_test_policy(&pool, "Original Name").await;

        let update_request = UpdateDeploymentPolicyRequest {
            name: Some("Updated Name".to_string()),
            description: Some("Updated description".to_string()),
            policy_type: None,
            config: None,
            enabled: Some(false),
        };

        let updated = deployment_policies::update_deployment_policy(&pool, &policy_id, &update_request)
            .await
            .unwrap()
            .expect("Policy should exist after update");

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.description, Some("Updated description".to_string()));
        assert_eq!(updated.enabled, false);
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_delete_deployment_policy() {
        let pool = test_pool().await;
        let policy_id = create_test_policy(&pool, "To Be Deleted").await;

        let deleted = deployment_policies::delete_deployment_policy(&pool, &policy_id)
            .await
            .unwrap();

        assert!(deleted);

        // Verify it's actually deleted
        let policy = deployment_policies::get_deployment_policy_by_id(&pool, &policy_id)
            .await
            .unwrap();

        assert!(policy.is_none());
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_duplicate_name_prevention() {
        let pool = test_pool().await;
        create_test_policy(&pool, "Duplicate Test").await;

        let request = CreateDeploymentPolicyRequest {
            name: "Duplicate Test".to_string(),
            description: Some("This should fail".to_string()),
            policy_type: "custom_check".to_string(),
            config: serde_json::json!({"expression": "true"}),
            enabled: Some(true),
        };

        let result = deployment_policies::create_deployment_policy(&pool, &request).await;
        
        // Should fail due to duplicate name
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_check_policy_in_use() {
        let pool = test_pool().await;
        let policy_id = create_test_policy(&pool, "Usage Test").await;

        let in_use = deployment_policies::check_policy_in_use(&pool, &policy_id)
            .await
            .unwrap();

        // Policy not assigned to any environment/system yet
        assert!(!in_use);
    }
}
