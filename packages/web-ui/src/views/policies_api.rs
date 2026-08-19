//! API integration for the policies view.
//!
//! This module provides the adapter layer between the policies UI and the
//! backend API. Network and server errors are surfaced as `Err` so the view
//! can render an explicit error state. Mock policy data is never returned to
//! the caller (AC #34).

use uuid::Uuid;

use crate::api::client::{ApiClientError, fetch_deployment_policies, fetch_deployment_policy};
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

/// Fetch a complete policy lineage directly and select the exact version used
/// by a bundle coverage mapping. This deliberately bypasses the first-100
/// catalog page used by the policies list.
pub async fn load_policy_version(
    policy_id: Uuid,
    policy_version_id: Uuid,
) -> Result<PolicyDefinition, String> {
    let record = fetch_deployment_policy(&policy_id)
        .await
        .map_err(|error| error.to_string())?;
    let mut definition = policy_record_to_definition(record);
    if definition
        .revisions
        .iter()
        .any(|revision| revision.id == policy_version_id)
    {
        definition.version_id = Some(policy_version_id);
        Ok(definition)
    } else {
        Err(format!(
            "Policy version {policy_version_id} is not present in policy {policy_id}."
        ))
    }
}

/// Convert a freshly-created or freshly-fetched backend record to a
/// `PolicyDefinition` with a system count of 0.
///
/// Used after a successful policy create so the new policy is immediately
/// inserted into the local `policy_library` state without needing to
/// re-fetch the paginated first-100 list.
pub(crate) fn policy_record_to_definition(record: DeploymentPolicyRecord) -> PolicyDefinition {
    policy_record_to_definition_with_count(record, 0)
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

    // Extract SRG/CCI and classification from the current version before consuming `record.versions`.
    let current_version_id = record.current_version_id;
    let (
        current_srg_ids,
        current_cci_ids,
        current_category,
        current_framework,
        current_severity,
        current_control_family,
        current_cmmc_level,
        current_cis_section,
        current_rationale,
    ) = record
        .versions
        .iter()
        .find(|v| Some(v.id) == current_version_id)
        .map(|v| {
            (
                v.srg_ids.clone(),
                v.cci_ids.clone(),
                v.category.clone(),
                v.framework.clone(),
                v.severity.clone(),
                v.control_family.clone(),
                v.cmmc_level,
                v.cis_section.clone(),
                v.rationale.clone(),
            )
        })
        .unwrap_or_default();

    let revision = record.versions.first().map(|v| v.version.clone());
    let publication_state = record
        .versions
        .iter()
        .find(|v| Some(v.id) == current_version_id)
        .map(|v| v.publication_state.clone());
    let semantic_digest = record
        .versions
        .iter()
        .find(|v| Some(v.id) == current_version_id)
        .map(|v| v.semantic_digest.clone());

    let revisions: Vec<PolicyRevisionSummary> = record
        .versions
        .into_iter()
        .map(|v| PolicyRevisionSummary {
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
            srg_ids: v.srg_ids,
            cci_ids: v.cci_ids,
            category: v.category,
            framework: v.framework,
            severity: v.severity,
            control_family: v.control_family,
            cmmc_level: v.cmmc_level,
            cis_section: v.cis_section,
            rationale: v.rationale,
        })
        .collect();

    PolicyDefinition {
        id: record.id,
        lineage_id: record.id,
        version_id: current_version_id,
        revision,
        publication_state,
        semantic_digest,
        revisions,
        name: record.name,
        description: record
            .description
            .unwrap_or_else(|| "No description".to_string()),
        format: PolicyFormat::Json,
        body,
        policy_type: Some(record.policy_type),
        updated_at: record.updated_at.to_rfc3339(),
        system_count,
        srg_ids: current_srg_ids,
        cci_ids: current_cci_ids,
        category: current_category,
        framework: current_framework,
        severity: current_severity,
        control_family: current_control_family,
        cmmc_level: current_cmmc_level,
        cis_section: current_cis_section,
        rationale: current_rationale,
        mapped_requirement_count: 0,
        bundle_usage_count: 0,
    }
}
