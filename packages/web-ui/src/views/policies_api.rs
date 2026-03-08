//! API integration for policies view with fallback to mock data.
//!
//! This module provides the adapter layer between the policies UI and the backend API,
//! with graceful fallback to mock data when the API is unavailable.

use uuid::Uuid;

use crate::api::client::{fetch_deployment_policies, ApiClientError};
use crate::api::models::DeploymentPolicyRecord;
use crate::components::policy::PolicyDefinition;

/// Fetch policies from the API with fallback to mock data.
///
/// Returns policies from the backend if available, otherwise returns
/// mock data to ensure the UI remains functional during development
/// or when the server is unavailable.
pub async fn load_policies_with_fallback() -> Vec<PolicyDefinition> {
    match fetch_deployment_policies(Some(100), Some(0)).await {
        Ok(response) => {
            web_sys::console::log_1(&format!("Loaded {} policies from API", response.policies.len()).into());
            response
                .policies
                .into_iter()
                .map(policy_record_to_definition)
                .collect()
        }
        Err(ApiClientError::Status { code, body }) => {
            // Only use mock fallback for server-side failures (5xx).
            // For auth/client errors (401/403/404/etc.), do not mask with mock data.
            if (500..600).contains(&code) {
                web_sys::console::warn_1(&format!("API returned {}: {} - falling back to mock data", code, body).into());
                mock_policies()
            } else {
                web_sys::console::warn_1(&format!("API returned {}: {} - showing empty policies", code, body).into());
                Vec::new()
            }
        }
        Err(ApiClientError::Network(msg)) => {
            web_sys::console::warn_1(&format!("Network error: {} - falling back to mock data", msg).into());
            mock_policies()
        }
        Err(ApiClientError::Deserialize(msg)) => {
            web_sys::console::error_1(&format!("Failed to deserialize API response: {} - showing empty policies", msg).into());
            Vec::new()
        }
    }
}

/// Convert a backend DeploymentPolicyRecord to a frontend PolicyDefinition.
fn policy_record_to_definition(record: DeploymentPolicyRecord) -> PolicyDefinition {
    use crate::components::policy::PolicyFormat;

    let body = serde_json::to_string_pretty(&serde_json::json!({
        "policy_type": record.policy_type,
        "enabled": record.enabled,
        "config": record.config,
    }))
    .unwrap_or_else(|_| "{}".to_string());
    
    PolicyDefinition {
        id: record.id,
        name: record.name,
        description: record.description.unwrap_or_else(|| "No description".to_string()),
        format: PolicyFormat::Json,
        body,
    }
}

/// Mock policies for fallback when API is unavailable.
fn mock_policies() -> Vec<PolicyDefinition> {
    use crate::components::policy::PolicyFormat;
    
    vec![
        PolicyDefinition {
            id: Uuid::from_u128(1),
            name: "Require Crystal Forge Agent".to_string(),
            description: "This policy ensures the Crystal Forge agent and client services are enabled on the target system.".to_string(),
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "require_crystal_forge_agent"
strict = true
"#.to_string(),
        },
        PolicyDefinition {
            id: Uuid::from_u128(2),
            name: "Require Firewall".to_string(),
            description: "Ensure firewall is enabled on all systems.".to_string(),
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "custom_check"
expression = "config.networking.firewall.enable"
description = "Firewall must be enabled"
strict = true
"#.to_string(),
        },
        PolicyDefinition {
            id: Uuid::from_u128(3),
            name: "Require SSH Key Auth".to_string(),
            description: "Require SSH key-only authentication (no passwords).".to_string(),
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "custom_check"
expression = "!config.services.openssh.settings.PasswordAuthentication"
description = "Password authentication must be disabled"
strict = false
"#.to_string(),
        },
        PolicyDefinition {
            id: Uuid::from_u128(4),
            name: "Require Auditd".to_string(),
            description: "Require audit daemon for security compliance.".to_string(),
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = "custom_check"
expression = "config.services.auditd.enable or false"
description = "Audit daemon should be enabled"
strict = false
"#.to_string(),
        },
    ]
}
