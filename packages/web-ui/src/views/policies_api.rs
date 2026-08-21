//! API integration for the policies view.
//!
//! This module provides the adapter layer between the policies UI and the
//! backend API. Network and server errors are surfaced as `Err` so the view
//! can render an explicit error state. Mock policy data is never returned to
//! the caller (AC #34).

use std::{collections::HashMap, future::Future};

use uuid::Uuid;

use crate::api::client::{ApiClientError, fetch_deployment_policies, fetch_deployment_policy};
use crate::api::models::{DeploymentPoliciesListResponse, DeploymentPolicyRecord};
use crate::components::policy::PolicyDefinition;
use crate::components::policy::PolicyRevisionSummary;

/// Result type for policy loading.
#[derive(Debug)]
pub enum PolicyLoadResult {
    Ok(Vec<PolicyDefinition>),
    /// Server or network error — the caller must display an error state.
    /// Never returns mock data (AC #34).
    Err(String),
}

const POLICY_PAGE_SIZE: i64 = 100;

async fn load_all_policy_pages<F, Fut>(
    mut fetch_page: F,
) -> Result<(Vec<DeploymentPolicyRecord>, HashMap<Uuid, i64>), ApiClientError>
where
    F: FnMut(i64) -> Fut,
    Fut: Future<Output = Result<DeploymentPoliciesListResponse, ApiClientError>>,
{
    let mut offset = 0_i64;
    let mut records = Vec::new();
    let mut system_counts = HashMap::new();
    let mut seen_ids = std::collections::HashSet::new();
    let mut expected_total = None;
    let mut previous_name = None::<String>;

    loop {
        let response = fetch_page(offset).await?;
        if response.offset != offset {
            return Err(ApiClientError::Deserialize(format!(
                "policy page offset mismatch: requested {offset}, received {}",
                response.offset
            )));
        }
        if let Some(total) = expected_total {
            if total != response.total {
                return Err(ApiClientError::Deserialize(
                    "policy page total changed during load".to_string(),
                ));
            }
        } else {
            expected_total = Some(response.total);
        }

        for policy in &response.policies {
            if let Some(previous) = previous_name.as_ref() {
                if policy.name < *previous {
                    return Err(ApiClientError::Deserialize(
                        "policy pages are not deterministically ordered".to_string(),
                    ));
                }
            }
            if !seen_ids.insert(policy.id) {
                return Err(ApiClientError::Deserialize(format!(
                    "duplicate policy id returned: {}",
                    policy.id
                )));
            }
            previous_name = Some(policy.name.clone());
        }

        let page_len = response.policies.len() as i64;
        system_counts.extend(response.system_counts);
        records.extend(response.policies);
        offset += page_len;
        let total = expected_total.unwrap_or_default();

        if records.len() == total {
            break;
        }
        if page_len == 0 || records.len() > total {
            return Err(ApiClientError::Deserialize(format!(
                "policy page load ended with {} records, expected {total}",
                records.len()
            )));
        }
    }

    Ok((records, system_counts))
}

async fn load_policies_with<F, Fut>(fetch_page: F) -> PolicyLoadResult
where
    F: FnMut(i64) -> Fut,
    Fut: Future<Output = Result<DeploymentPoliciesListResponse, ApiClientError>>,
{
    let result = load_all_policy_pages(fetch_page).await;
    let (records, system_counts) = match result {
        Ok(result) => result,
        Err(ApiClientError::Status { code, body }) => {
            return PolicyLoadResult::Err(format!("Server returned {}: {}", code, body));
        }
        Err(ApiClientError::Network(msg)) => {
            return PolicyLoadResult::Err(format!("Network error: {}", msg));
        }
        Err(ApiClientError::Deserialize(msg)) => {
            return PolicyLoadResult::Err(format!("Deserialize error: {}", msg));
        }
    };

    let definitions = records
        .into_iter()
        .map(|p| {
            let count = system_counts.get(&p.id).copied().unwrap_or(0);
            policy_record_to_definition_with_count(p, count)
        })
        .collect();
    PolicyLoadResult::Ok(definitions)
}

/// Fetch policies from the API.
///
/// Returns an explicit error on any failure; never falls back to mock data.
pub async fn load_policies() -> PolicyLoadResult {
    load_policies_with(|offset| async move {
        fetch_deployment_policies(Some(POLICY_PAGE_SIZE), Some(offset)).await
    })
    .await
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::Utc;

    use super::*;
    use crate::api::models::DeploymentPolicyVersionSummary;

    fn fixture_policy(id: Uuid, name: String, category: Option<&str>) -> DeploymentPolicyRecord {
        let version_id = Uuid::new_v4();
        DeploymentPolicyRecord {
            id,
            name: name.clone(),
            description: None,
            policy_type: "custom_check".to_string(),
            config: serde_json::json!({"expression": "true"}),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            current_version_id: Some(version_id),
            versions: vec![DeploymentPolicyVersionSummary {
                id: version_id,
                policy_id: id,
                version: "1.0.0".to_string(),
                publication_state: "draft".to_string(),
                trust_state: "trusted".to_string(),
                semantic_digest: format!("digest-{id}"),
                created_at: Utc::now(),
                published_at: None,
                derived_from_version_id: None,
                is_current_published: false,
                is_current_draft: true,
                name,
                description: None,
                policy_type: "custom_check".to_string(),
                config: serde_json::json!({"expression": "true"}),
                enabled: true,
                srg_ids: Vec::new(),
                cci_ids: Vec::new(),
                category: category.map(str::to_string),
                framework: None,
                severity: None,
                control_family: None,
                cmmc_level: None,
                cis_section: None,
                rationale: None,
                created_by: None,
                created_by_display: None,
                evidence_specs: Vec::new(),
            }],
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        }
    }

    fn page(
        offset: i64,
        total: usize,
        policies: Vec<DeploymentPolicyRecord>,
    ) -> DeploymentPoliciesListResponse {
        DeploymentPoliciesListResponse {
            policies,
            total,
            limit: POLICY_PAGE_SIZE,
            offset,
            system_counts: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn production_loader_fetches_and_classifies_later_policy_page() {
        let mut policies = Vec::new();
        for index in 0..105 {
            policies.push(fixture_policy(
                Uuid::new_v4(),
                format!("security-{index:03}"),
                Some("security"),
            ));
        }
        let platform_ids: Vec<Uuid> = (0..5)
            .map(|index| {
                let id = Uuid::new_v4();
                policies.push(fixture_policy(id, format!("zzz-platform-{index:03}"), None));
                id
            })
            .collect();

        let first_page = policies[..100].to_vec();
        let second_page = policies[100..].to_vec();
        let result = load_policies_with(move |offset| {
            let first_page = first_page.clone();
            let second_page = second_page.clone();
            async move {
                Ok(if offset == 0 {
                    page(0, 110, first_page)
                } else {
                    page(100, 110, second_page)
                })
            }
        })
        .await;

        let PolicyLoadResult::Ok(loaded) = result else {
            panic!("expected complete policy catalog");
        };
        assert_eq!(loaded.len(), 110);
        let unique_ids: std::collections::HashSet<Uuid> =
            loaded.iter().map(|policy| policy.id).collect();
        assert_eq!(unique_ids.len(), 110);
        assert_eq!(
            loaded
                .iter()
                .filter(|p| p.category.as_deref() == Some("security"))
                .count(),
            105
        );
        assert_eq!(loaded.iter().filter(|p| p.category.is_none()).count(), 5);
        for id in platform_ids {
            assert!(loaded.iter().any(|policy| policy.id == id));
        }
        assert!(
            loaded
                .iter()
                .any(|policy| policy.name == "zzz-platform-004")
        );
    }

    #[tokio::test]
    async fn production_loader_rejects_later_page_failure_without_partial_success() {
        let first_page: Vec<DeploymentPolicyRecord> = (0..100)
            .map(|index| {
                fixture_policy(
                    Uuid::new_v4(),
                    format!("security-{index:03}"),
                    Some("security"),
                )
            })
            .collect();
        let result = load_policies_with(move |offset| {
            let first_page = first_page.clone();
            async move {
                if offset == 0 {
                    Ok(page(0, 110, first_page))
                } else {
                    Err(ApiClientError::Status {
                        code: 503,
                        body: "later page unavailable".to_string(),
                    })
                }
            }
        })
        .await;

        assert!(matches!(result, PolicyLoadResult::Err(message) if message.contains("503")));
    }

    /// Regression: two policies sharing the same name must not cause
    /// `"policy pages are not deterministically ordered"`.  With
    /// `ORDER BY name ASC, id ASC` the server always returns the same
    /// id-sequence for tied names.  This test feeds the client two pages
    /// that are already correctly ordered by (name, id) and verifies the
    /// loader accepts them.  A previous bug (`ORDER BY name ASC` alone)
    /// made the page boundary unstable, which triggered the client-side
    /// deterministic-page validation.
    #[tokio::test]
    async fn production_loader_accepts_pages_with_tied_names() {
        let shared_name = "shared-policy-name".to_string();

        // Simulate 4 policies with the same name but distinct UUIDs,
        // pre-sorted by (name, id) — exactly what the fixed query returns.
        let mut tied: Vec<DeploymentPolicyRecord> = (0..4)
            .map(|i| {
                let id = Uuid::new_v4();
                let mut p = fixture_policy(id, shared_name.clone(), None);
                // Ensure the ID sorts after the others so the tie-break is exercised.
                p.name = shared_name.clone();
                p
            })
            .collect();
        tied.sort_by(|a, b| (a.name.clone(), a.id).cmp(&(b.name.clone(), b.id)));

        let first_page = tied[..2].to_vec();
        let second_page = tied[2..].to_vec();

        let result = load_policies_with(move |offset| {
            let first_page = first_page.clone();
            let second_page = second_page.clone();
            async move {
                Ok(if offset == 0 {
                    page(0, 4, first_page)
                } else {
                    page(2, 4, second_page)
                })
            }
        })
        .await;

        let PolicyLoadResult::Ok(loaded) = result else {
            panic!(
                "pages with tied names must not fail deterministic-page validation: {:?}",
                result
            );
        };
        assert_eq!(loaded.len(), 4);
        let unique_ids: std::collections::HashSet<Uuid> =
            loaded.iter().map(|policy| policy.id).collect();
        assert_eq!(unique_ids.len(), 4, "no duplicate policy IDs");
    }

    /// Verify that the deterministic-page validation rejects out-of-order
    /// pages — the exact scenario that caused the production regression.
    #[tokio::test]
    async fn production_loader_rejects_out_of_order_pages() {
        let mut policies: Vec<DeploymentPolicyRecord> = (0..5)
            .map(|i| fixture_policy(Uuid::new_v4(), format!("policy-{i:03}"), None))
            .collect();
        // Reverse the first page to simulate non-deterministic ordering.
        policies[..3].reverse();

        let first_page = policies[..3].to_vec();
        let second_page = policies[3..].to_vec();

        let result = load_policies_with(move |offset| {
            let first_page = first_page.clone();
            let second_page = second_page.clone();
            async move {
                Ok(if offset == 0 {
                    page(0, 5, first_page)
                } else {
                    page(3, 5, second_page)
                })
            }
        })
        .await;

        assert!(
            matches!(
                result,
                PolicyLoadResult::Err(ref msg) if msg.contains("deterministically ordered")
            ),
            "out-of-order pages must be rejected, got: {result:?}"
        );
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
            created_by: v.created_by,
            created_by_display: v.created_by_display,
            evidence_specs: v.evidence_specs,
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
         mapped_requirement_count: record.mapped_requirement_count,
         bundle_usage_count: record.bundle_usage_count,
         evidence_specs: None,
     }
}
