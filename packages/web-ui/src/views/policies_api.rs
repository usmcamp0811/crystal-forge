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
    // Full advertised ordering key: `(name, id)`. The server sorts with
    // `ORDER BY name COLLATE "C" ASC, id ASC`; `C` collation is bytewise over
    // UTF-8, which is exactly Rust `String::Ord`, so comparing with Rust
    // ordering here is equivalent to the server's ordering.
    //
    // Both halves must be validated. Comparing only `name` would silently
    // accept a page boundary that reorders equal-named records, which is the
    // half of the contract the server's `id` tie-breaker exists to guarantee.
    let mut previous_key = None::<(String, Uuid)>;

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
            let key = (policy.name.clone(), policy.id);
            if let Some(previous) = previous_key.as_ref() {
                if key < *previous {
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
            previous_key = Some(key);
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
    let definition = policy_record_to_definition(record);
    policy_definition_for_revision(&definition, policy_version_id).ok_or_else(|| {
        format!("Policy version {policy_version_id} is not present in policy {policy_id}.")
    })
}

/// Project one exact revision of a lineage into a self-contained
/// `PolicyDefinition`.
///
/// Every editor entry point (catalog card/row, policy drawer, compliance
/// drawer) must hand the editor a single coherent version rather than a
/// lineage-current object with a different `version_id`. Classification,
/// enforcement config, evidence, and imported provenance are all taken from the
/// same revision.
pub(crate) fn policy_definition_for_revision(
    policy: &PolicyDefinition,
    revision_id: Uuid,
) -> Option<PolicyDefinition> {
    let revision = policy
        .revisions
        .iter()
        .find(|revision| revision.id == revision_id)?;

    let body = serde_json::to_string_pretty(&serde_json::json!({
        "policy_type": revision.policy_type,
        "enabled": revision.enabled,
        "config": revision.config,
    }))
    .unwrap_or_else(|_| "{}".to_string());

    Some(PolicyDefinition {
        id: policy.id,
        lineage_id: policy.lineage_id,
        version_id: Some(revision.id),
        revision: Some(revision.version.clone()),
        publication_state: Some(revision.publication_state.clone()),
        semantic_digest: Some(revision.semantic_digest.clone()),
        revisions: policy.revisions.clone(),
        name: revision.name.clone(),
        description: revision
            .description
            .clone()
            .unwrap_or_else(|| "No description".to_string()),
        format: policy.format,
        body,
        policy_type: Some(revision.policy_type.clone()),
        updated_at: policy.updated_at.clone(),
        system_count: policy.system_count,
        srg_ids: revision.srg_ids.clone(),
        cci_ids: revision.cci_ids.clone(),
        category: revision.category.clone(),
        framework: revision.framework.clone(),
        severity: revision.severity.clone(),
        control_family: revision.control_family.clone(),
        cmmc_level: revision.cmmc_level,
        cis_section: revision.cis_section.clone(),
        rationale: revision.rationale.clone(),
        // Usage counts belong to the lineage-current version; a historical or
        // freshly derived revision carries no authoritative count.
        mapped_requirement_count: if Some(revision.id) == policy.version_id {
            policy.mapped_requirement_count
        } else {
            0
        },
        bundle_usage_count: if Some(revision.id) == policy.version_id {
            policy.bundle_usage_count
        } else {
            0
        },
        evidence_specs: Some(revision.evidence_specs.clone()),
        provenance: revision.provenance.clone(),
    })
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
                provenance: Vec::new(),
            }],
            mapped_requirement_count: 0,
            bundle_usage_count: 0,
        }
    }

    /// P1 regression: the catalog conversion must hydrate the current
    /// version's evidence onto the top-level `PolicyDefinition`.
    ///
    /// `PolicyEditorModal` resolves the policy under edit out of
    /// `policy_library` by id, so a `None` here produced an empty evidence
    /// baseline on *every* edit path. The editor diffs against that baseline,
    /// so adding one evidence source to a policy that already had evidence
    /// emitted `Some([new])` and destroyed the existing set.
    ///
    /// Fails against `evidence_specs: None`.
    #[test]
    fn catalog_conversion_hydrates_current_version_evidence() {
        use crate::api::models::{EvidenceKind, EvidenceSpec};

        let id = Uuid::new_v4();
        let mut record = fixture_policy(id, "evidence policy".to_string(), None);
        let evidence_a = EvidenceSpec {
            kind: EvidenceKind::Command {
                cmd: "sshd -T".to_string(),
                expect: "permitrootlogin no".to_string(),
            },
            required_fields: Default::default(),
        };
        record.versions[0].evidence_specs = vec![evidence_a.clone()];

        let definition = policy_record_to_definition_with_count(record, 0);

        assert_eq!(
            definition.evidence_specs,
            Some(vec![evidence_a]),
            "catalog definition must carry the current version's evidence, \
             otherwise the editor baseline is empty and a save replaces \
             existing evidence instead of extending it"
        );
    }

    /// A current version that genuinely has no evidence must be
    /// distinguishable from "no resolvable current version".
    #[test]
    fn catalog_conversion_distinguishes_empty_evidence_from_unknown() {
        let id = Uuid::new_v4();

        // Current version resolves, and carries zero evidence.
        let empty = fixture_policy(id, "no evidence".to_string(), None);
        assert_eq!(
            policy_record_to_definition_with_count(empty, 0).evidence_specs,
            Some(Vec::new()),
            "a resolvable current version with no evidence must be Some([])"
        );

        // No current version pointer at all -> nothing is known.
        let mut unknown = fixture_policy(id, "unknown".to_string(), None);
        unknown.current_version_id = None;
        assert_eq!(
            policy_record_to_definition_with_count(unknown, 0).evidence_specs,
            None,
            "an unresolvable current version must stay None, not Some([])"
        );
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

    /// The production failure mode, reproduced at the client boundary.
    ///
    /// A database-collated `ORDER BY name` (e.g. `en_US.utf8`) emits
    /// `apple` before `Apple`, because that collation is case-insensitive.
    /// Rust `String::Ord` is bytewise, so it considers `"Apple" < "apple"`
    /// (`0x41 < 0x61`) and rejects the page. The server fix is
    /// `ORDER BY name COLLATE "C"`, which matches Rust byte ordering.
    #[tokio::test]
    async fn production_loader_rejects_locale_collated_name_order() {
        // Exactly what an en_US.utf8 database returns for these two names.
        let locale_ordered = vec![
            fixture_policy(Uuid::new_v4(), "apple policy".to_string(), None),
            fixture_policy(Uuid::new_v4(), "Apple policy".to_string(), None),
        ];

        let result = load_policies_with(move |_offset| {
            let locale_ordered = locale_ordered.clone();
            async move { Ok(page(0, 2, locale_ordered)) }
        })
        .await;

        assert!(
            matches!(
                result,
                PolicyLoadResult::Err(ref msg) if msg.contains("deterministically ordered")
            ),
            "locale-collated name order must be rejected by the client contract, got: {result:?}"
        );
    }

    /// The same two names in `C`/Rust byte order must be accepted. This is
    /// the post-fix server behaviour.
    #[tokio::test]
    async fn production_loader_accepts_c_collated_name_order() {
        let c_ordered = vec![
            fixture_policy(Uuid::new_v4(), "Apple policy".to_string(), None),
            fixture_policy(Uuid::new_v4(), "apple policy".to_string(), None),
        ];

        let result = load_policies_with(move |_offset| {
            let c_ordered = c_ordered.clone();
            async move { Ok(page(0, 2, c_ordered)) }
        })
        .await;

        let PolicyLoadResult::Ok(loaded) = result else {
            panic!("C-collated (Rust byte) name order must be accepted, got: {result:?}");
        };
        assert_eq!(loaded.len(), 2);
    }

    /// Validates the half of the ordering contract the client previously
    /// ignored: for equal names the server promises ascending `id`, so a
    /// page that emits descending `id` within a name tie must be rejected.
    ///
    /// Before widening the validator to the full `(name, id)` key this case
    /// passed silently, because `name < previous` is false for equal names.
    #[tokio::test]
    async fn production_loader_rejects_tied_names_with_descending_ids() {
        let shared = "shared policy name".to_string();
        let mut ids = [Uuid::new_v4(), Uuid::new_v4()];
        ids.sort();
        // Emit the higher id first — violates the advertised `id ASC`.
        let descending = vec![
            fixture_policy(ids[1], shared.clone(), None),
            fixture_policy(ids[0], shared.clone(), None),
        ];

        let result = load_policies_with(move |_offset| {
            let descending = descending.clone();
            async move { Ok(page(0, 2, descending)) }
        })
        .await;

        assert!(
            matches!(
                result,
                PolicyLoadResult::Err(ref msg) if msg.contains("deterministically ordered")
            ),
            "tied names with descending ids must be rejected, got: {result:?}"
        );
    }

    /// Tied names in ascending `id` order satisfy the contract.
    #[tokio::test]
    async fn production_loader_accepts_tied_names_with_ascending_ids() {
        let shared = "shared policy name".to_string();
        let mut ids = [Uuid::new_v4(), Uuid::new_v4()];
        ids.sort();
        let ascending = vec![
            fixture_policy(ids[0], shared.clone(), None),
            fixture_policy(ids[1], shared.clone(), None),
        ];

        let result = load_policies_with(move |_offset| {
            let ascending = ascending.clone();
            async move { Ok(page(0, 2, ascending)) }
        })
        .await;

        let PolicyLoadResult::Ok(loaded) = result else {
            panic!("tied names with ascending ids must be accepted, got: {result:?}");
        };
        assert_eq!(loaded.len(), 2);
    }

    /// Ordering must also hold *across* a page boundary, not just within a page.
    #[tokio::test]
    async fn production_loader_rejects_out_of_order_across_page_boundary() {
        let first_page = vec![
            fixture_policy(Uuid::new_v4(), "b policy".to_string(), None),
            fixture_policy(Uuid::new_v4(), "c policy".to_string(), None),
        ];
        // Regresses below the last name of page 1.
        let second_page = vec![fixture_policy(Uuid::new_v4(), "a policy".to_string(), None)];

        let result = load_policies_with(move |offset| {
            let first_page = first_page.clone();
            let second_page = second_page.clone();
            async move {
                Ok(if offset == 0 {
                    page(0, 3, first_page)
                } else {
                    page(2, 3, second_page)
                })
            }
        })
        .await;

        assert!(
            matches!(
                result,
                PolicyLoadResult::Err(ref msg) if msg.contains("deterministically ordered")
            ),
            "ordering violation across a page boundary must be rejected, got: {result:?}"
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

    // Hydrate the current version's evidence set onto the catalog-level
    // definition.
    //
    // `PolicyEditorModal` resolves the policy being edited by id out of
    // `policy_library` (the catalog), regardless of which `PolicyDefinition`
    // the caller handed to `on_edit`. Leaving this `None` therefore gave the
    // editor an empty evidence baseline on *every* edit path, including the
    // drawer path that carefully reconstructs the selected revision.
    //
    // The editor diffs the edited evidence against that baseline to decide
    // between `None` (preserve) and `Some(..)` (replace). An empty baseline
    // turned "not loaded" into "was empty", so adding one evidence source to a
    // policy that already had evidence issued `Some([new])` and destroyed the
    // existing set.
    //
    // The `Option` distinction is meaningful and preserved here:
    //   `None`     -> no resolvable current version; nothing is known.
    //   `Some([])` -> current version genuinely carries no evidence.
    let current_evidence_specs = record
        .versions
        .iter()
        .find(|v| Some(v.id) == current_version_id)
        .map(|v| v.evidence_specs.clone());

    // Authoritative imported-origin provenance for the current version. Empty
    // for policies authored in Crystal Forge; never derived from the name or
    // any display string.
    let current_provenance = record
        .versions
        .iter()
        .find(|v| Some(v.id) == current_version_id)
        .map(|v| v.provenance.clone())
        .unwrap_or_default();

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
            provenance: v.provenance,
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
        evidence_specs: current_evidence_specs,
        provenance: current_provenance,
    }
}
