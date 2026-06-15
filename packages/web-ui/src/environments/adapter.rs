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

use std::collections::HashMap;
use uuid::Uuid;

use crate::api::client::{
    create_environment, delete_environment, fetch_environment_policies,
    fetch_environment_policies_map, fetch_environments, fetch_policies, update_environment,
    update_environment_policies, ApiClientError,
};
use crate::api::models::{CreateEnvironmentRequest, EnvironmentSummary, UpdateEnvironmentRequest};
use crate::components::environments::{
    policy_library as fallback_policy_library, EnvironmentCacheSummary,
    EnvironmentDeploymentPolicy, EnvironmentHealthBreakdown, EnvironmentItem, PolicyOption,
};

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

/// Result of loading environment names + colors.
#[derive(Debug, Clone)]
pub struct EnvironmentColorsLoadResult {
    pub colors: Vec<(String, String)>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

#[derive(Debug, Clone)]
pub struct PoliciesLoadResult {
    pub policies: Vec<PolicyOption>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Adapter Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the environments list from the backend, with fallback to deterministic mock data.
///
/// The `default_required_policy` is used only for fallback data and create flows.
pub async fn load_environments_with_fallback(
    default_required_policy: Uuid,
) -> EnvironmentsLoadResult {
    match (
        fetch_environments().await,
        fetch_environment_policies_map().await,
    ) {
        (Ok(items), Ok(policy_map_entries)) => {
            let policy_map: HashMap<Uuid, Vec<Uuid>> = policy_map_entries
                .into_iter()
                .map(|entry| (entry.environment_id, entry.required_policy_ids))
                .collect();

            let environments = items
                .into_iter()
                .map(|env| {
                    let required_policy_ids = policy_map.get(&env.id).cloned().unwrap_or_default();
                    api_to_environment_item(env, required_policy_ids)
                })
                .collect();

            EnvironmentsLoadResult {
                environments,
                notice: None,
                redirect_to_login: false,
            }
        }
        (Err(error), _) | (_, Err(error)) if should_redirect_to_login(&error) => {
            EnvironmentsLoadResult {
                environments: Vec::new(),
                notice: None,
                redirect_to_login: true,
            }
        }
        (Err(error), _) | (_, Err(error)) => EnvironmentsLoadResult {
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

/// Fetch environment names and color hex values from backend.
///
/// Falls back to deterministic environment palette when the API is unavailable.
pub async fn load_environment_colors_with_fallback() -> EnvironmentColorsLoadResult {
    match fetch_environments().await {
        Ok(items) => {
            let mut colors: Vec<(String, String)> =
                items.into_iter().map(|e| (e.name, e.color_hex)).collect();
            colors.sort_by_key(|(name, _)| name.to_ascii_lowercase());
            colors.dedup_by(|(a, _), (b, _)| a.eq_ignore_ascii_case(b));

            EnvironmentColorsLoadResult {
                colors,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => EnvironmentColorsLoadResult {
            colors: fallback_environment_color_pairs(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => EnvironmentColorsLoadResult {
            colors: fallback_environment_color_pairs(),
            notice: Some(format!(
                "Environments API unavailable for color mapping, using fallback palette: {error}"
            )),
            redirect_to_login: false,
        },
    }
}

/// Fetch policy options from backend for environment requirements modal.
pub async fn load_policies_with_fallback() -> PoliciesLoadResult {
    match fetch_policies().await {
        Ok(items) => {
            let mut policies: Vec<PolicyOption> = items
                .into_iter()
                .filter(|p| p.enabled)
                .map(|p| PolicyOption {
                    id: p.id,
                    name: p.name,
                    description: p.description.unwrap_or_default(),
                })
                .collect();
            policies.sort_by_key(|p| p.name.to_ascii_lowercase());

            PoliciesLoadResult {
                policies,
                notice: None,
                redirect_to_login: false,
            }
        }
        Err(error) if should_redirect_to_login(&error) => PoliciesLoadResult {
            policies: fallback_policy_library(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => PoliciesLoadResult {
            policies: fallback_policy_library(),
            notice: Some(format!(
                "Policies API unavailable, using fallback policy list: {error}"
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

fn fallback_environment_color_pairs() -> Vec<(String, String)> {
    fallback_environments(Uuid::from_u128(1))
        .into_iter()
        .map(|env| (env.name, env.color_hex))
        .collect()
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
            health: EnvironmentHealthBreakdown {
                active: 12,
                healthy: 9,
                warning: 2,
                critical: 1,
                offline: 0,
            },
            cve_critical_high: 7,
            flake_names: vec!["infrastructure".to_string(), "edge".to_string()],
            default_policy: Some(EnvironmentDeploymentPolicy::Manual),
            cache: Some(EnvironmentCacheSummary {
                name: "prod-cache".to_string(),
                url: "s3://crystal-forge-prod-cache".to_string(),
                cache_type: "s3".to_string(),
                status: "healthy".to_string(),
            }),
            auto_sync: Some(true),
            requires_approval: Some(true),
            is_production: Some(true),
            role_assignment_count: Some(4),
        },
        EnvironmentItem {
            id: Uuid::from_u128(102),
            name: "staging".to_string(),
            description: Some("Pre-production validation".to_string()),
            color_hex: "#B45309".to_string(),
            system_count: 2,
            required_policy_ids: vec![default_required_policy],
            health: EnvironmentHealthBreakdown {
                active: 2,
                healthy: 1,
                warning: 1,
                critical: 0,
                offline: 0,
            },
            cve_critical_high: 2,
            flake_names: vec!["infrastructure".to_string()],
            default_policy: Some(EnvironmentDeploymentPolicy::Manual),
            cache: Some(EnvironmentCacheSummary {
                name: "staging-cache".to_string(),
                url: "s3://crystal-forge-staging-cache".to_string(),
                cache_type: "s3".to_string(),
                status: "healthy".to_string(),
            }),
            auto_sync: Some(true),
            requires_approval: Some(true),
            is_production: Some(false),
            role_assignment_count: Some(5),
        },
        EnvironmentItem {
            id: Uuid::from_u128(103),
            name: "development".to_string(),
            description: Some("Workstations and local testing".to_string()),
            color_hex: "#2563EB".to_string(),
            system_count: 8,
            required_policy_ids: vec![default_required_policy],
            health: EnvironmentHealthBreakdown {
                active: 8,
                healthy: 6,
                warning: 1,
                critical: 0,
                offline: 1,
            },
            cve_critical_high: 1,
            flake_names: vec!["workstations".to_string(), "lab".to_string()],
            default_policy: Some(EnvironmentDeploymentPolicy::AutoLatest),
            cache: Some(EnvironmentCacheSummary {
                name: "dev-attic".to_string(),
                url: "attic://cf-attic.dev/dev".to_string(),
                cache_type: "attic".to_string(),
                status: "warning".to_string(),
            }),
            auto_sync: Some(true),
            requires_approval: Some(false),
            is_production: Some(false),
            role_assignment_count: Some(7),
        },
        EnvironmentItem {
            id: Uuid::from_u128(104),
            name: "remote".to_string(),
            description: Some("Remote unmanaged network".to_string()),
            color_hex: "#6B7280".to_string(),
            system_count: 0,
            required_policy_ids: vec![default_required_policy],
            health: EnvironmentHealthBreakdown::default(),
            cve_critical_high: 0,
            flake_names: Vec::new(),
            default_policy: Some(EnvironmentDeploymentPolicy::Pinned),
            cache: None,
            auto_sync: Some(false),
            requires_approval: Some(false),
            is_production: Some(false),
            role_assignment_count: Some(1),
        },
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Mapping
// ─────────────────────────────────────────────────────────────────────────────

/// Convert an [`EnvironmentSummary`] API DTO into an [`EnvironmentItem`] UI type.
///
/// Color comes directly from the backend-provided `color_hex` field.
/// Policy requirements are provided by the policies endpoint and persisted server-side.
pub fn api_to_environment_item(
    env: EnvironmentSummary,
    required_policy_ids: Vec<Uuid>,
) -> EnvironmentItem {
    EnvironmentItem {
        id: env.id,
        name: env.name,
        description: env.description,
        color_hex: env.color_hex,
        system_count: env.system_count as usize,
        required_policy_ids,
        health: EnvironmentHealthBreakdown {
            active: env.rollup.active_system_count.max(0) as usize,
            healthy: env.rollup.healthy.max(0) as usize,
            warning: env.rollup.warning.max(0) as usize,
            critical: env.rollup.critical.max(0) as usize,
            offline: env.rollup.offline.max(0) as usize,
        },
        cve_critical_high: env.rollup.cve_critical_high.max(0) as usize,
        flake_names: env.rollup.flakes,
        // TASK-359..TASK-362 placeholders are intentionally not populated for
        // successful API responses. Rendering these as Some(..) would fabricate
        // operational state and make non-persisted edits appear successful.
        default_policy: None,
        cache: None,
        auto_sync: None,
        requires_approval: None,
        is_production: None,
        role_assignment_count: None,
    }
}

/// Create a new environment via backend API.
pub async fn create_environment_via_api(
    name: String,
    description: Option<String>,
    color_hex: String,
    is_active: bool,
    default_required_policy: Uuid,
) -> Result<EnvironmentItem, String> {
    let request = CreateEnvironmentRequest {
        name,
        description,
        color_hex,
        is_active,
    };

    match create_environment(&request).await {
        Ok(env) => Ok(api_to_environment_item(env, vec![default_required_policy])),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {msg}")),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {msg}")),
    }
}

/// Delete an environment via backend API.
pub async fn delete_environment_via_api(environment_id: Uuid) -> Result<(), String> {
    match delete_environment(&environment_id).await {
        Ok(()) => Ok(()),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
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
    color_hex: String,
    default_required_policy: Uuid,
) -> Result<EnvironmentItem, String> {
    let request = UpdateEnvironmentRequest {
        name,
        description,
        color_hex,
    };

    match update_environment(&environment_id, &request).await {
        Ok(env) => {
            let required_policy_ids = match fetch_environment_policies(&environment_id).await {
                Ok(details) => details.required_policy_ids,
                Err(_) => vec![default_required_policy],
            };
            Ok(api_to_environment_item(env, required_policy_ids))
        }
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {msg}")),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {msg}")),
    }
}

/// Update environment required policies via backend API.
pub async fn update_environment_policies_via_api(
    environment_id: Uuid,
    required_policy_ids: Vec<Uuid>,
) -> Result<(), String> {
    match update_environment_policies(&environment_id, &required_policy_ids).await {
        Ok(_response) => Ok(()),
        Err(ApiClientError::Status {
            code: 401 | 403, ..
        }) => Err("Authentication required. Please log in.".to_string()),
        Err(ApiClientError::Status { body, .. }) => Err(body),
        Err(ApiClientError::Network(msg)) => Err(format!("Network error: {msg}")),
        Err(ApiClientError::Deserialize(msg)) => Err(format!("Invalid response: {msg}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status {
            code: 401 | 403,
            ..
        }
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
            color_hex: "#0F766E".to_string(),
            is_active: true,
            system_count: 6,
            rollup: crate::api::models::EnvironmentRollup {
                active_system_count: 5,
                healthy: 4,
                warning: 1,
                critical: 1,
                offline: 0,
                cve_critical_high: 9,
                flakes: vec!["infra".to_string(), "edge".to_string()],
            },
        };
        let item = api_to_environment_item(summary, vec![DEFAULT_POLICY]);
        assert_eq!(item.id, Uuid::from_u128(999));
        assert_eq!(item.name, "production");
        assert_eq!(item.color_hex, "#0F766E");
        assert_eq!(item.system_count, 6);
        assert_eq!(item.health.active, 5);
        assert_eq!(item.health.healthy, 4);
        assert_eq!(item.health.warning, 1);
        assert_eq!(item.health.critical, 1);
        assert_eq!(item.cve_critical_high, 9);
        assert_eq!(item.flake_names, vec!["infra", "edge"]);
        assert_eq!(item.cache, None);
        assert_eq!(item.default_policy, None);
        assert_eq!(item.auto_sync, None);
        assert_eq!(item.requires_approval, None);
        assert_eq!(item.is_production, None);
        assert_eq!(item.required_policy_ids, vec![DEFAULT_POLICY]);
    }

    #[test]
    fn api_to_environment_item_uses_fallback_color_for_unknown_name() {
        let summary = EnvironmentSummary {
            id: Uuid::from_u128(888),
            name: "my-custom-env".to_string(),
            description: None,
            color_hex: "#123456".to_string(),
            is_active: true,
            system_count: 0,
            rollup: Default::default(),
        };
        let item = api_to_environment_item(summary, vec![DEFAULT_POLICY]);
        assert_eq!(item.color_hex, "#123456");
        assert!(item.description.is_none());
    }
}
