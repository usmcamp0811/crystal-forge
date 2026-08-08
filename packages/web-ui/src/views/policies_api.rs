//! API integration for the policies view.
//!
//! This module provides the adapter layer between the policies UI and the
//! backend API. Network and server errors are surfaced as `Err` so the view
//! can render an explicit error state. Mock policy data is never returned to
//! the caller (AC #34).

use std::collections::HashMap;

use uuid::Uuid;

use crate::api::client::{ApiClientError, fetch_deployment_policies};
use crate::api::models::DeploymentPolicyRecord;
use crate::components::policy::PolicyDefinition;
use crate::components::policy::PolicyRevisionSummary;

/// Result type for policy loading.
pub enum PolicyLoadResult {
    Ok(Vec<PolicyDefinition>),
    /// Server or network error — the caller must display an error state.
    /// Never returns mock data (AC #34).
    Err(String),
}

/// Fetch policies from the API.
///
/// Returns an explicit error on any failure; never falls back to mock data.
pub async fn load_policies() -> PolicyLoadResult {
    match fetch_deployment_policies(Some(100), Some(0)).await {
        Ok(response) => {
            let sys_counts = response.system_counts;
            let definitions = response
                .policies
                .into_iter()
                .map(|p| {
                    let count = sys_counts.get(&p.id).copied().unwrap_or(0);
                    policy_record_to_definition_with_count(p, count)
                })
                .collect();
            PolicyLoadResult::Ok(definitions)
        }
        Err(ApiClientError::Status { code, body }) => {
            PolicyLoadResult::Err(format!("Server returned {}: {}", code, body))
        }
        Err(ApiClientError::Network(msg)) => {
            PolicyLoadResult::Err(format!("Network error: {}", msg))
        }
        Err(ApiClientError::Deserialize(msg)) => {
            PolicyLoadResult::Err(format!("Deserialize error: {}", msg))
        }
    }
}

/// Convert a backend DeploymentPolicyRecord to a frontend PolicyDefinition.
fn policy_record_to_definition_with_count(
    record: DeploymentPolicyRecord,
    system_count: i64,
) -> PolicyDefinition {
    use crate::components::policy::PolicyFormat;

    let body = serde_json::to_string_pretty(&serde_json::json!({
        "policy_type": record.policy_type,
        "enabled": record.enabled,
        "config": record.config,
    }))
    .unwrap_or_else(|_| "{}".to_string());

    PolicyDefinition {
        id: record.id,
        lineage_id: record.id,
        version_id: record.current_version_id,
        revision: record.versions.first().map(|v| v.version.clone()),
        publication_state: record.versions.iter().find(|v| Some(v.id) == record.current_version_id).map(|v| v.publication_state.clone()),
        semantic_digest: record.versions.iter().find(|v| Some(v.id) == record.current_version_id).map(|v| v.semantic_digest.clone()),
        revisions: record.versions.into_iter().map(|v| PolicyRevisionSummary {
            id: v.id,
            version: v.version,
            publication_state: v.publication_state,
            trust_state: v.trust_state,
            semantic_digest: v.semantic_digest,
            created_at: v.created_at.to_rfc3339(),
            is_current_published: v.is_current_published,
            is_current_draft: v.is_current_draft,
            name: v.name,
            description: v.description,
            policy_type: v.policy_type,
            config: v.config,
            enabled: v.enabled,
        }).collect(),
        name: record.name,
        description: record
            .description
            .unwrap_or_else(|| "No description".to_string()),
        format: PolicyFormat::Json,
        body,
        policy_type: Some(record.policy_type),
        system_count,
    }
}
