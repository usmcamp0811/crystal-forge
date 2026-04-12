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
use serde_json::Value;
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

/// Validate and normalize a Nix expression for custom policy checks.
///
/// This function ensures expressions use the correct variable scope by:
/// 1. Replacing standalone `config.` with `cfg.config.` (the correct scope in policy evaluation)
/// 2. Preserving `cfg.config.` if already correct
/// 3. Warning about potential issues
///
/// Returns the normalized expression or an error if validation fails.
fn validate_and_normalize_nix_expression(expr: &str) -> Result<String, (StatusCode, String)> {
    let trimmed = expr.trim();

    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Expression cannot be empty".to_string(),
        ));
    }

    // Check for common mistakes and auto-fix them
    let normalized = if trimmed.contains("config.") && !trimmed.contains("cfg.config.") {
        // Replace `config.` with `cfg.config.` but be careful not to replace `cfg.config.`
        // Use a simple regex-like replacement: replace `config.` with `cfg.config.` only when not preceded by `cfg.`
        let mut result = String::new();
        let mut chars = trimmed.chars().peekable();
        let mut last_three = String::new();

        while let Some(c) = chars.next() {
            result.push(c);
            last_three.push(c);
            if last_three.len() > 3 {
                last_three.remove(0);
            }

            // Check if we just wrote "config" and next char is "."
            if result.ends_with("config") && chars.peek() == Some(&'.') {
                // Check if it's preceded by "cfg."
                if !result.ends_with("cfg.config") {
                    // Insert "cfg." before "config"
                    let len = result.len();
                    result.insert_str(len - 6, "cfg.");
                }
            }
        }

        tracing::warn!(
            "Auto-corrected policy expression from 'config.' to 'cfg.config.': {} -> {}",
            trimmed,
            result
        );
        result
    } else {
        trimmed.to_string()
    };

    Ok(normalized)
}

fn validate_policy_config(
    policy_type: &str,
    config: &Value,
) -> Result<Value, (StatusCode, String)> {
    if config.is_null() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Policy config cannot be null".to_string(),
        ));
    }

    let obj = config.as_object().ok_or((
        StatusCode::BAD_REQUEST,
        "Policy config must be a JSON object".to_string(),
    ))?;

    if let Some(strict) = obj.get("strict") {
        if !strict.is_boolean() {
            return Err((
                StatusCode::BAD_REQUEST,
                "config.strict must be a boolean when provided".to_string(),
            ));
        }
    }

    let mut validated_config = config.clone();

    match policy_type {
        "require_cf_agent" => {
            if let Some(false) = obj.get("strict").and_then(|v| v.as_bool()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "require_cf_agent policy must enforce config.strict = true".to_string(),
                ));
            }
        }
        "require_packages" => {
            let packages = obj.get("packages").and_then(|v| v.as_array()).ok_or((
                StatusCode::BAD_REQUEST,
                "require_packages policy requires config.packages as a non-empty array".to_string(),
            ))?;

            if packages.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "require_packages policy requires at least one package".to_string(),
                ));
            }

            let all_valid = packages.iter().all(|entry| {
                entry
                    .as_str()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
            });
            if !all_valid {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "config.packages must contain only non-empty strings".to_string(),
                ));
            }
        }
        "custom_check" => {
            let has_expression = obj
                .get("expression")
                .and_then(|v| v.as_str())
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);

            let has_rules = obj
                .get("rules")
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);

            if !has_expression && !has_rules {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "custom_check policy requires either non-empty config.expression or non-empty config.rules[]".to_string(),
                ));
            }

            if has_expression && !has_rules {
                // Single-expression (legacy) path — normalize
                let expression = obj
                    .get("expression")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .unwrap();
                let normalized_expr = validate_and_normalize_nix_expression(expression)?;
                if let Some(config_obj) = validated_config.as_object_mut() {
                    config_obj.insert("expression".to_string(), Value::String(normalized_expr));
                }
            }

            if has_rules {
                // Multi-rule path — validate each rule
                let rules = obj.get("rules").and_then(|v| v.as_array()).unwrap();
                for (i, rule) in rules.iter().enumerate() {
                    let rule_obj = rule.as_object().ok_or((
                        StatusCode::BAD_REQUEST,
                        format!("config.rules[{}] must be an object", i),
                    ))?;

                    let expr = rule_obj
                        .get("expression")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .ok_or((
                            StatusCode::BAD_REQUEST,
                            format!("config.rules[{}].expression must be a non-empty string", i),
                        ))?;
                    validate_and_normalize_nix_expression(expr)?;

                    if rule_obj
                        .get("field_name")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                    {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            format!("config.rules[{}].field_name must be a non-empty string", i),
                        ));
                    }
                }

                // Validate mode if present
                if let Some(mode) = obj.get("mode") {
                    let mode_str = mode.as_str().ok_or((
                        StatusCode::BAD_REQUEST,
                        "config.mode must be a string (\"all\" or \"any\")".to_string(),
                    ))?;
                    if mode_str != "all" && mode_str != "any" {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "config.mode must be \"all\" or \"any\"".to_string(),
                        ));
                    }
                }
            }
        }
        "require_cve_check" => {
            // Validate by attempting deserialization into CveCheckConfig
            serde_json::from_value::<crate::models::deployment_policies::CveCheckConfig>(
                config.clone(),
            )
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("Invalid require_cve_check config: {}", e),
                )
            })?;

            // Validate when_no_scan string value if present
            if let Some(wns) = obj.get("when_no_scan").and_then(|v| v.as_str()) {
                if wns != "block" && wns != "skip" {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        "config.when_no_scan must be \"block\" or \"skip\"".to_string(),
                    ));
                }
            }
        }
        _ => {}
    }

    Ok(validated_config)
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

    // Get total count for pagination metadata
    let total = deployment_policies::count_deployment_policies(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to count deployment policies: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to count deployment policies".to_string(),
            )
        })?;

    // Fetch policies from database
    let policies =
        deployment_policies::list_deployment_policies(&state.pool, params.limit, params.offset)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list deployment policies: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to retrieve deployment policies".to_string(),
                )
            })?;

    Ok(Json(DeploymentPoliciesListResponse {
        policies,
        total: total as usize,
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
    let valid_types = ["require_cf_agent", "require_packages", "custom_check", "require_cve_check"];
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

    // Validate and normalize the config (may auto-fix expressions)
    request.config = validate_policy_config(&request.policy_type, &request.config)?;

    // Check if policy name already exists
    let name_exists =
        deployment_policies::check_policy_name_exists(&state.pool, &request.name, None)
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

    // Check for duplicate policy semantics (same type + same config)
    let content_exists = deployment_policies::check_policy_content_exists(
        &state.pool,
        &request.policy_type,
        &request.config,
        None,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to check duplicate policy content: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to validate policy content".to_string(),
        )
    })?;

    if content_exists {
        return Err((
            StatusCode::CONFLICT,
            "A policy with the same type and configuration already exists".to_string(),
        ));
    }

    // Core policy is always enabled
    if request.policy_type == "require_cf_agent" {
        request.enabled = Some(true);
        if let Some(config_obj) = request.config.as_object_mut() {
            config_obj.insert("strict".to_string(), Value::Bool(true));
        }
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

    // Core require_cf_agent policy is immutable except name/description updates.
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
        if request.config.is_some() {
            return Err((
                StatusCode::CONFLICT,
                "Core require_cf_agent policy config cannot be changed".to_string(),
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
        let valid_types = ["require_cf_agent", "require_packages", "custom_check", "require_cve_check"];
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

    let candidate_policy_type = request
        .policy_type
        .clone()
        .unwrap_or_else(|| existing.policy_type.clone());
    let mut candidate_config = request
        .config
        .clone()
        .unwrap_or_else(|| existing.config.clone());

    if request.policy_type.is_some() || request.config.is_some() {
        // Validate and normalize the config (may auto-fix expressions)
        candidate_config = validate_policy_config(&candidate_policy_type, &candidate_config)?;
        // Update the request with the normalized config
        request.config = Some(candidate_config.clone());
    }

    // Check for duplicate policy semantics (same type + same config)
    let content_exists = deployment_policies::check_policy_content_exists(
        &state.pool,
        &candidate_policy_type,
        &candidate_config,
        Some(&policy_id),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to check duplicate policy content: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to validate policy content".to_string(),
        )
    })?;

    if content_exists {
        return Err((
            StatusCode::CONFLICT,
            "A policy with the same type and configuration already exists".to_string(),
        ));
    }

    // Update policy
    let policy = deployment_policies::update_deployment_policy(&state.pool, &policy_id, &request)
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

    #[test]
    fn validate_policy_config_rejects_require_packages_without_packages() {
        let err = validate_policy_config("require_packages", &serde_json::json!({"strict": true}))
            .expect_err("missing packages must be rejected");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("config.packages"));
    }

    #[test]
    fn validate_policy_config_rejects_custom_check_without_expression() {
        let err = validate_policy_config("custom_check", &serde_json::json!({"strict": false}))
            .expect_err("missing expression must be rejected");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("config.expression"));
    }

    #[test]
    fn validate_policy_config_rejects_non_strict_require_cf_agent() {
        let err = validate_policy_config("require_cf_agent", &serde_json::json!({"strict": false}))
            .expect_err("strict false must be rejected");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("strict = true"));
    }

    #[test]
    fn validate_policy_config_accepts_valid_custom_check() {
        let result = validate_policy_config(
            "custom_check",
            &serde_json::json!({"expression": "cfg.config.services.ssh.enable", "strict": true}),
        )
        .expect("valid custom_check config must pass");

        // Verify expression is preserved when already correct
        assert_eq!(
            result.get("expression").and_then(|v| v.as_str()),
            Some("cfg.config.services.ssh.enable")
        );
    }

    #[test]
    fn validate_policy_config_auto_fixes_config_prefix() {
        let result = validate_policy_config(
            "custom_check",
            &serde_json::json!({"expression": "config.services.ssh.enable", "strict": true}),
        )
        .expect("should auto-fix config. to cfg.config.");

        // Verify expression was auto-corrected
        assert_eq!(
            result.get("expression").and_then(|v| v.as_str()),
            Some("cfg.config.services.ssh.enable"),
            "Expression should be auto-corrected from 'config.' to 'cfg.config.'"
        );
    }

    #[test]
    fn validate_policy_config_handles_complex_expression() {
        let result = validate_policy_config(
            "custom_check",
            &serde_json::json!({
                "expression": "!config.services.openssh.settings.PasswordAuthentication",
                "strict": false
            }),
        )
        .expect("should auto-fix complex expression");

        assert_eq!(
            result.get("expression").and_then(|v| v.as_str()),
            Some("!cfg.config.services.openssh.settings.PasswordAuthentication"),
            "Complex expressions should be auto-corrected"
        );
    }

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
    async fn test_list_includes_disabled_policies() {
        let pool = test_pool().await;

        // Create an enabled policy
        let enabled_request = CreateDeploymentPolicyRequest {
            name: "Enabled Test Policy".to_string(),
            description: Some("This policy is enabled".to_string()),
            policy_type: "custom_check".to_string(),
            config: serde_json::json!({"expression": "true"}),
            enabled: Some(true),
        };
        deployment_policies::create_deployment_policy(&pool, &enabled_request)
            .await
            .unwrap();

        // Create a disabled policy
        let disabled_request = CreateDeploymentPolicyRequest {
            name: "Disabled Test Policy".to_string(),
            description: Some("This policy is disabled".to_string()),
            policy_type: "custom_check".to_string(),
            config: serde_json::json!({"expression": "false"}),
            enabled: Some(false),
        };
        deployment_policies::create_deployment_policy(&pool, &disabled_request)
            .await
            .unwrap();

        // List all policies
        let policies = deployment_policies::list_deployment_policies(&pool, 100, 0)
            .await
            .unwrap();

        // Verify both policies are in the list
        let enabled_found = policies
            .iter()
            .any(|p| p.name == "Enabled Test Policy" && p.enabled);
        let disabled_found = policies
            .iter()
            .any(|p| p.name == "Disabled Test Policy" && !p.enabled);

        assert!(enabled_found, "Enabled policy should be in the list");
        assert!(disabled_found, "Disabled policy should be in the list");
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

        let updated =
            deployment_policies::update_deployment_policy(&pool, &policy_id, &update_request)
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

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_count_deployment_policies() {
        let pool = test_pool().await;

        // Get initial count
        let initial_count = deployment_policies::count_deployment_policies(&pool)
            .await
            .unwrap();

        // Create 3 test policies
        create_test_policy(&pool, "Count Test 1").await;
        create_test_policy(&pool, "Count Test 2").await;
        create_test_policy(&pool, "Count Test 3").await;

        // Verify count increased by 3
        let new_count = deployment_policies::count_deployment_policies(&pool)
            .await
            .unwrap();

        assert_eq!(new_count, initial_count + 3);
    }

    #[tokio::test]
    #[ignore = "requires live postgres"]
    async fn test_pagination_total_is_accurate() {
        let pool = test_pool().await;

        // Create multiple policies (more than one page)
        for i in 1..=5 {
            create_test_policy(&pool, &format!("Pagination Test {}", i)).await;
        }

        // Get total count
        let total = deployment_policies::count_deployment_policies(&pool)
            .await
            .unwrap();

        // Fetch with limit smaller than total
        let policies = deployment_policies::list_deployment_policies(&pool, 2, 0)
            .await
            .unwrap();

        // Verify we got a limited page but total represents all policies
        assert_eq!(policies.len(), 2, "Should return 2 policies per page");
        assert!(
            total >= 5,
            "Total should include all policies, not just the page size"
        );
    }
}
