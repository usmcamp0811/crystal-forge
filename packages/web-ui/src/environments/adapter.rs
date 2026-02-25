//! Environments adapter — API fetch with deterministic fallback.
//!
//! # Behaviour
//!
//! | Outcome               | Result                                      |
//! |-----------------------|---------------------------------------------|
//! | API returns 2xx       | Real data, no notice                        |
//! | API returns 401/403   | `redirect_to_login: true`                   |
//! | API 5xx / network err | Fallback mock data, notice shown            |
//! | Empty list from API   | Empty `items` vec (not fallback)            |
//!
//! Views MUST NOT call [`crate::api::client`] directly.
//! All HTTP interactions go through the functions in this module.

use uuid::Uuid;

use crate::api::client::{
    ApiClientError, create_environment, delete_environment, fetch_environments, update_environment,
};
use crate::api::models::{CreateEnvironmentRequest, EnvironmentSummary, UpdateEnvironmentRequest};
use crate::components::environments::EnvironmentItem;

// ─────────────────────────────────────────────────────────────────────────────
// Result Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result of loading the environments list.
#[derive(Debug, Clone)]
pub struct EnvironmentsLoadResult {
    /// Environments to display (real data, fallback, or empty).
    pub environments: Vec<EnvironmentItem>,
    /// Human-readable notice shown when using fallback data.
    pub notice: Option<String>,
    /// True when the API returned 401/403 — view should redirect to login.
    pub redirect_to_login: bool,
}

/// Result of loading environment names for dropdowns.
#[derive(Debug, Clone)]
pub struct EnvironmentNamesLoadResult {
    pub names: Vec<String>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Adapter Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the environments list from the backend, with fallback to deterministic mock data.
///
/// The `default_required_policy` is the ID of the "Require Crystal Forge Agent"
/// policy, which is assigned to all environments by default when creating new ones.
/// It is applied as the sole `required_policy_ids` for API-loaded environments
/// (the backend does not yet store policy requirements per environment).
pub async fn load_environments_with_fallback(
    default_required_policy: Uuid,
) -> EnvironmentsLoadResult {
    match fetch_environments().await {
        Ok(items) => {
            let environments = items
                .into_iter()
                .map(|e| api_to_environment_item(e, default_required_policy))
                .collect();

            EnvironmentsLoadResult {
                environments,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => EnvironmentsLoadResult {
            environments: fallback_environments(default_required_policy),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => EnvironmentsLoadResult {
            environments: fallback_environments(default_required_policy),
            notice: Some(format!(
                "Environments API unavailable, using deterministic fallback data: {error}"
            )),
            redirect_to_login: false,
        },
    }
}

/// Fetch environment names from backend for form dropdowns.
///
/// Falls back to deterministic names when the API is unavailable.
pub async fn load_environment_names_with_fallback() -> EnvironmentNamesLoadResult {
    match fetch_environments().await {
        Ok(items) => {
            let mut names: Vec<String> = items.into_iter().map(|e| e.name).collect();
            names.sort_by_key(|name| name.to_ascii_lowercase());
            names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            EnvironmentNamesLoadResult {
                names,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => EnvironmentNamesLoadResult {
            names: fallback_environment_names(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => EnvironmentNamesLoadResult {
            names: fallback_environment_names(),
            notice: Some(format!(
                "Environments API unavailable for system form, using fallback names: {error}"
            )),
            redirect_to_login: false,
        },
    }
}

fn fallback_environment_names() -> Vec<String> {
    vec![
        "production".to_string(),
        "staging".to_string(),
        "development".to_string(),
        "remote".to_string(),
    ]
}

/// Deterministic fallback environment list used when the API is unavailable.
pub fn fallback_environments(default_required_policy: Uuid) -> Vec<EnvironmentItem> {
    vec![
        EnvironmentItem {
            id: Uuid::from_u128(101),
            name: "production".to_string(),
            description: Some("Live fleet systems".to_string()),
            color_hex: "#0F766E".to_string(),
            system_count: 12,
            required_policy_ids: vec![default_required_policy, Uuid::from_u128(3)],
        },
        EnvironmentItem {
            id: Uuid::from_u128(102),
            name: "staging".to_string(),
            description: Some("Pre-production validation".to_string()),
            color_hex: "#B45309".to_string(),
            system_count: 2,
            required_policy_ids: vec![default_required_policy],
        },
        EnvironmentItem {
            id: Uuid::from_u128(103),
            name: "development".to_string(),
            description: Some("Workstations and local testing".to_string()),
            color_hex: "#2563EB".to_string(),
            system_count: 8,
            required_policy_ids: vec![default_required_policy],
        },
        EnvironmentItem {
            id: Uuid::from_u128(104),
            name: "remote".to_string(),
            description: Some("Remote unmanaged network".to_string()),
            color_hex: "#6B7280".to_string(),
            system_count: 0,
            required_policy_ids: vec![default_required_policy],
        },
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Mapping
// ─────────────────────────────────────────────────────────────────────────────

/// Convert an [`EnvironmentSummary`] API DTO into an [`EnvironmentItem`] UI type.
///
/// Color is derived from the environment name using a stable palette so that
/// well-known environment names (production, staging, development) get
/// consistent colours. Unknown names fall back to a neutral grey.
///
/// Policy requirements are not stored per environment in the current backend
/// schema; the default agent policy is used as a placeholder until that
/// feature is implemented.
pub fn api_to_environment_item(
    env: EnvironmentSummary,
    default_required_policy: Uuid,
) -> EnvironmentItem {
    let color_hex = color_for_name(&env.name);
    EnvironmentItem {
        id: env.id,
        name: env.name,
        description: env.description,
        color_hex,
        system_count: env.system_count as usize,
        required_policy_ids: vec![default_required_policy],
    }
}

/// Create a new environment via backend API.
pub async fn create_environment_via_api(
    name: String,
    description: Option<String>,
    is_active: bool,
    default_required_policy: Uuid,
) -> Result<EnvironmentItem, String> {
    let request = CreateEnvironmentRequest {
        name,
        description,
        is_active,
    };

    match create_environment(&request).await {
        Ok(env) => Ok(api_to_environment_item(env, default_required_policy)),
        Err(ApiClientError::Status { code: 401 | 403, .. }) => {
            Err("Authentication required. Please log in.".to_string())
        }
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {msg}")),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {msg}")),
    }
}

/// Delete an environment via backend API.
pub async fn delete_environment_via_api(environment_id: Uuid) -> Result<(), String> {
    match delete_environment(&environment_id).await {
        Ok(()) => Ok(()),
        Err(ApiClientError::Status { code: 401 | 403, .. }) => {
            Err("Authentication required. Please log in.".to_string())
        }
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {msg}")),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {msg}")),
    }
}

/// Update environment metadata via backend API.
pub async fn update_environment_via_api(
    environment_id: Uuid,
    name: String,
    description: Option<String>,
    default_required_policy: Uuid,
) -> Result<EnvironmentItem, String> {
    let request = UpdateEnvironmentRequest { name, description };

    match update_environment(&environment_id, &request).await {
        Ok(env) => Ok(api_to_environment_item(env, default_required_policy)),
        Err(ApiClientError::Status { code: 401 | 403, .. }) => {
            Err("Authentication required. Please log in.".to_string())
        }
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {msg}")),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {msg}")),
    }
}

/// Return a stable color for a well-known environment name, or a default.
fn color_for_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "production" | "prod" => "#0F766E".to_string(),  // teal
        "staging" | "stage" => "#B45309".to_string(),    // amber
        "development" | "dev" => "#2563EB".to_string(),  // blue
        "test" | "testing" => "#7C3AED".to_string(),     // violet
        "preprod" | "pre-prod" => "#9D174D".to_string(), // rose
        _ => "#6B7280".to_string(),                      // neutral grey
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status { code: 401 | 403, .. }
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_POLICY: Uuid = Uuid::from_u128(1);

    #[test]
    fn should_redirect_for_auth_errors() {
        assert!(should_redirect_to_login(&ApiClientError::Status {
            code: 401,
            body: "unauthorized".to_string(),
        }));
        assert!(should_redirect_to_login(&ApiClientError::Status {
            code: 403,
            body: "forbidden".to_string(),
        }));
    }

    #[test]
    fn should_not_redirect_for_server_or_network_errors() {
        assert!(!should_redirect_to_login(&ApiClientError::Status {
            code: 500,
            body: "internal server error".to_string(),
        }));
        assert!(!should_redirect_to_login(&ApiClientError::Network(
            "connection refused".to_string()
        )));
    }

    #[test]
    fn fallback_environments_is_deterministic() {
        let a = fallback_environments(DEFAULT_POLICY);
        let b = fallback_environments(DEFAULT_POLICY);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.name, y.name);
        }
    }

    #[test]
    fn fallback_environments_has_expected_entries() {
        let envs = fallback_environments(DEFAULT_POLICY);
        assert_eq!(envs.len(), 4);
        assert_eq!(envs[0].name, "production");
        assert_eq!(envs[1].name, "staging");
        assert_eq!(envs[2].name, "development");
        assert_eq!(envs[3].name, "remote");
    }

    #[test]
    fn api_to_environment_item_maps_known_names() {
        let summary = EnvironmentSummary {
            id: Uuid::from_u128(999),
            name: "production".to_string(),
            description: Some("Live fleet".to_string()),
            is_active: true,
            system_count: 6,
        };
        let item = api_to_environment_item(summary, DEFAULT_POLICY);
        assert_eq!(item.id, Uuid::from_u128(999));
        assert_eq!(item.name, "production");
        assert_eq!(item.color_hex, "#0F766E");
        assert_eq!(item.system_count, 6);
        assert_eq!(item.required_policy_ids, vec![DEFAULT_POLICY]);
    }

    #[test]
    fn api_to_environment_item_uses_fallback_color_for_unknown_name() {
        let summary = EnvironmentSummary {
            id: Uuid::from_u128(888),
            name: "my-custom-env".to_string(),
            description: None,
            is_active: true,
            system_count: 0,
        };
        let item = api_to_environment_item(summary, DEFAULT_POLICY);
        assert_eq!(item.color_hex, "#6B7280");
        assert!(item.description.is_none());
    }

    #[test]
    fn color_for_name_is_case_insensitive() {
        assert_eq!(color_for_name("PRODUCTION"), color_for_name("production"));
        assert_eq!(color_for_name("Staging"), color_for_name("staging"));
    }
}
