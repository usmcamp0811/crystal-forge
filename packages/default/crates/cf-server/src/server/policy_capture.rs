//! Converts actual loader inputs into non-executable historical metadata.
//!
//! Applicable contexts include empty bundles. Only authoritative resolver
//! contributions create policy membership edges. Both come from the same
//! resolver work items before execution, never today's historical reconstruction.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, ensure};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::api::eval_policy_matrix::*;
use crate::compliance::resolver::{
    AssignmentMode, EffectivePolicy, ResolutionOutcome, ResolvedBundleContext,
};
use crate::models::deployment_policies::DeploymentPolicyRecord;

/// Creates an empty capture accumulator without an assigned attempt identity.
pub(super) fn new_snapshot() -> EvalPolicyMatrixSnapshot {
    EvalPolicyMatrixSnapshot {
        evaluation_attempt_id: None,
        attempt_number: 0,
        commit_sha: None,
        attempt_status: None,
        started_at: None,
        completed_at: None,
        mock_evidence: false,
        provenance: EvalMatrixProvenance::Captured,
        columns: Vec::new(),
        bundles: Vec::new(),
        empty_bundle_metadata_complete: false,
        rows: Vec::new(),
    }
}

/// Records every planned system, including empty and unresolved policy sets.
///
/// Copies only the explicit classification joined by the loader in its RR
/// transaction. Names, outcomes, and missing deployment state provide no
/// classification authority. Shared configurations retain per-system values.
pub(super) fn initialize_rows(
    capture: &mut EvalPolicyMatrixSnapshot,
    systems: &[(Uuid, String, String, Option<bool>)],
    outcomes: &HashMap<(Option<Uuid>, Uuid), ResolutionOutcome>,
) {
    capture.rows = systems
        .iter()
        .map(|(id, config, name, greenfield)| EvalMatrixRow {
            greenfield: *greenfield,
            system_id: Some(*id),
            system_name: name.clone(),
            configuration_name: config.clone(),
            resolution: match outcomes.get(&(None, *id)) {
                Some(ResolutionOutcome::Resolved(_)) => EvalMatrixResolution::Resolved,
                Some(ResolutionOutcome::Conflict(_)) => EvalMatrixResolution::Conflict,
                None => EvalMatrixResolution::Unavailable,
            },
            evaluation_digest: None,
            assigned_bundles: Vec::new(),
            policies: Vec::new(),
            cells: Vec::new(),
            details: Vec::new(),
        })
        .collect();
}

/// Marks resolved peers whose configuration gates were skipped by a conflict.
pub(super) fn mark_skipped_configurations(
    capture: &mut EvalPolicyMatrixSnapshot,
    configurations: &BTreeSet<String>,
) {
    for row in &mut capture.rows {
        if configurations.contains(&row.configuration_name)
            && row.resolution == EvalMatrixResolution::Resolved
        {
            row.resolution = EvalMatrixResolution::Skipped;
        }
    }
}

/// Loads runtime records without changing the loader's existing enabled semantics.
///
/// Runtime names and enabled state come from mutable lineages in the loader's
/// snapshot. Public capture labels use immutable version names instead.
///
/// # Errors
/// Returns an error when PostgreSQL cannot load or decode policy records.
pub(super) async fn load_records(
    tx: &mut Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, DeploymentPolicyRecord>> {
    #[derive(sqlx::FromRow)]
    struct VersionRecord {
        version_id: Uuid,
        #[sqlx(flatten)]
        record: DeploymentPolicyRecord,
    }
    let rows = sqlx::query_as::<_, VersionRecord>(
        "SELECT pv.id AS version_id, dp.id, dp.name, dp.description,
                pv.policy_type, pv.config, dp.enabled, dp.created_at, dp.updated_at
         FROM deployment_policy_versions pv JOIN deployment_policies dp ON dp.id = pv.policy_id
         WHERE pv.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().map(|r| (r.version_id, r.record)).collect())
}

/// Holds exact-version metadata loaded only for this resolution transaction.
///
/// These maps avoid per-system metadata queries. They never cache mutable
/// catalog pointers across transactions.
pub(super) struct CaptureMetadata {
    /// Exact-version display labels from this loader transaction only.
    pub(super) policies: HashMap<Uuid, EvalPolicyColumn>,
    bundles: HashMap<Uuid, EvalBundleColumn>,
    assignments: HashMap<Uuid, (Uuid, Uuid)>,
}

/// Loads version labels in bulk for resolver-exported assignment snapshots.
///
/// # Errors
/// Returns an error when PostgreSQL cannot load or decode referenced metadata.
pub(super) async fn load_metadata(
    tx: &mut Transaction<'_, Postgres>,
    policies: &[Uuid],
    contexts: &HashMap<(Option<Uuid>, Uuid), Vec<ResolvedBundleContext>>,
) -> Result<CaptureMetadata> {
    let mapped_ids = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT m.policy_version_id, r.external_id
         FROM policy_requirement_mappings m
         JOIN compliance_requirement_versions r ON r.id = m.requirement_version_id
         WHERE m.policy_version_id = ANY($1) AND m.trust_state = 'trusted'
         ORDER BY m.policy_version_id, r.external_id",
    )
    .bind(policies)
    .fetch_all(&mut **tx)
    .await?;
    let mut requirement_ids: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (id, external_id) in mapped_ids {
        requirement_ids.entry(id).or_default().push(external_id);
    }
    let policies = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            Option<String>,
            serde_json::Value,
        ),
    >(
        "SELECT id, policy_id, name, version, description, compliance_metadata
         FROM deployment_policy_versions WHERE id = ANY($1)",
    )
    .bind(policies)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(
        |(policy_version_id, policy_id, name, version, description, metadata)| {
            (
                policy_version_id,
                EvalPolicyColumn {
                    policy_version_id,
                    policy_id,
                    name,
                    version,
                    identifiers: display_identifiers(
                        &metadata,
                        requirement_ids
                            .remove(&policy_version_id)
                            .unwrap_or_default(),
                    ),
                    description,
                    // No canonical display-only assertion/path exists. Executable
                    // config and XCCDF check/fix bodies must not fill this gap.
                    assertion: None,
                    attribute_path: None,
                },
            )
        },
    )
    .collect();
    // Use identities exported by the resolver's authoritative query. Do not
    // resolve assignment pointers again or derive contexts from policy unions.
    let assignments = contexts
        .values()
        .flatten()
        .filter_map(|context| {
            context
                .assignment_id
                .zip(context.assignment_version_id)
                .map(|(id, version)| (id, (version, context.bundle_version_id)))
        })
        .collect();
    let bundle_ids: Vec<_> = contexts
        .values()
        .flatten()
        .map(|c| c.bundle_version_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let bundles = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, Option<String>)>(
        "SELECT id, bundle_id, name, version, framework, framework_version
         FROM compliance_bundle_versions WHERE id = ANY($1)",
    )
    .bind(&bundle_ids)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .map(
        |(bundle_version_id, bundle_id, name, version, framework, framework_version)| {
            (
                bundle_version_id,
                EvalBundleColumn {
                    bundle_version_id,
                    bundle_id,
                    name,
                    version,
                    framework,
                    framework_version,
                },
            )
        },
    )
    .collect();
    Ok(CaptureMetadata {
        policies,
        bundles,
        assignments,
    })
}

// SECURITY: This whitelist follows XCCDF ImportedPolicy::build_compliance_metadata
// and the policy CRUD classification metadata. Never serialize generic metadata,
// identifiers objects, checks, fixes, config, expressions, or opaque source XML.
// Mapped labels come only from trusted exact-version requirement external IDs in
// the loader's RR snapshot. They label this policy; they are not extra controls.
fn display_identifiers(metadata: &serde_json::Value, mapped: Vec<String>) -> Vec<String> {
    let mut ids: BTreeSet<String> = mapped
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .collect();
    for key in ["source_rule_id", "source_group_id", "cis_section"] {
        if let Some(value) = metadata.get(key).and_then(serde_json::Value::as_str) {
            if !value.trim().is_empty() {
                ids.insert(value.to_owned());
            }
        }
    }
    for key in ["srg_ids", "cci_ids"] {
        if let Some(values) = metadata.get(key).and_then(serde_json::Value::as_array) {
            ids.extend(
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .filter(|id| !id.trim().is_empty())
                    .map(str::to_owned),
            );
        }
    }
    ids.into_iter().collect()
}

/// Records complete per-system applicability independently of eligible controls.
///
/// # Errors
/// Returns an error for missing bundle metadata or invalid context identities.
pub(super) fn bind_contexts(
    capture: &mut EvalPolicyMatrixSnapshot,
    metadata: &CaptureMetadata,
    contexts: &HashMap<(Option<Uuid>, Uuid), Vec<ResolvedBundleContext>>,
) -> Result<()> {
    let mut complete = true;
    for row in &mut capture.rows {
        let Some(contexts) = row.system_id.and_then(|id| contexts.get(&(None, id))) else {
            complete = false;
            continue;
        };
        for context in contexts {
            ensure!(
                context.virtual_baseline == context.assignment_id.is_none()
                    && context.virtual_baseline == context.assignment_version_id.is_none(),
                "invalid resolver bundle context identity"
            );
            row.assigned_bundles.push(EvalBundleMembership {
                bundle_version_id: context.bundle_version_id,
                assignment_id: context.assignment_id,
                assignment_version_id: context.assignment_version_id,
                source: if context.virtual_baseline {
                    EvalBundleSource::VirtualBaseline
                } else {
                    EvalBundleSource::Assignment
                },
            });
            if !capture
                .bundles
                .iter()
                .any(|b| b.bundle_version_id == context.bundle_version_id)
            {
                capture.bundles.push(
                    metadata
                        .bundles
                        .get(&context.bundle_version_id)
                        .context("capture applicable bundle metadata missing")?
                        .clone(),
                );
            }
        }
        row.assigned_bundles
            .sort_by_key(|m| (m.bundle_version_id, m.assignment_version_id));
        row.assigned_bundles
            .dedup_by_key(|m| (m.bundle_version_id, m.assignment_version_id));
    }
    capture.bundles.sort_by_key(|b| b.bundle_version_id);
    capture.empty_bundle_metadata_complete = complete;
    Ok(())
}

/// Records one eligible policy and its authoritative memberships on one system.
///
/// # Errors
/// Returns an error when version metadata, assignment identity, or the planned
/// system is missing or inconsistent with the resolver's snapshot.
pub(super) fn bind_policy(
    metadata: &CaptureMetadata,
    capture: &mut EvalPolicyMatrixSnapshot,
    system_id: Uuid,
    policy: &EffectivePolicy,
    executable: bool,
) -> Result<()> {
    if !capture
        .columns
        .iter()
        .any(|c| c.policy_version_id == policy.policy_version_id)
    {
        capture.columns.push(
            metadata
                .policies
                .get(&policy.policy_version_id)
                .context("capture policy version metadata missing")?
                .clone(),
        );
    }
    let mut memberships = Vec::new();
    for source in policy.provenance.iter().filter(|p| p.authoritative) {
        let Some(bundle_version_id) = source.bundle_version_id else {
            continue;
        };
        let membership = if source.scope_type.as_deref() == Some("virtual_baseline") {
            // COMPATIBILITY: The resolver uses nil IDs internally for virtual
            // work items. Public metadata uses no fabricated assignment UUID.
            ensure!(
                source.assignment_id.is_none_or(|id| id.is_nil()),
                "virtual baseline has a persisted assignment identity"
            );
            ensure!(
                capture
                    .rows
                    .iter()
                    .find(|r| r.system_id == Some(system_id))
                    .is_some_and(|r| r.assigned_bundles.iter().any(|m| {
                        m.bundle_version_id == bundle_version_id
                            && m.source == EvalBundleSource::VirtualBaseline
                    })),
                "virtual baseline lacks an applicable resolver context"
            );
            EvalBundleMembership {
                bundle_version_id,
                assignment_id: None,
                assignment_version_id: None,
                source: EvalBundleSource::VirtualBaseline,
            }
        } else {
            let assignment_id = source
                .assignment_id
                .context("bundle contribution lacks assignment identity")?;
            let (assignment_version_id, assigned_bundle) = metadata
                .assignments
                .get(&assignment_id)
                .copied()
                .context("capture exact resolver assignment version missing")?;
            ensure!(
                assigned_bundle == bundle_version_id,
                "capture resolver assignment version mismatch"
            );
            EvalBundleMembership {
                bundle_version_id,
                assignment_id: Some(assignment_id),
                assignment_version_id: Some(assignment_version_id),
                source: EvalBundleSource::Assignment,
            }
        };
        memberships.push(membership);
        if !capture
            .bundles
            .iter()
            .any(|b| b.bundle_version_id == bundle_version_id)
        {
            capture.bundles.push(
                metadata
                    .bundles
                    .get(&bundle_version_id)
                    .context("capture bundle version metadata missing")?
                    .clone(),
            );
        }
    }
    memberships.sort_by_key(|m| (m.bundle_version_id, m.assignment_version_id));
    memberships.dedup_by_key(|m| (m.bundle_version_id, m.assignment_version_id));
    let row = capture
        .rows
        .iter_mut()
        .find(|r| r.system_id == Some(system_id))
        .context("capture planned system missing")?;
    row.policies.push(EvalPolicyBinding {
        policy_version_id: policy.policy_version_id,
        report_only: policy.effective_mode == AssignmentMode::ReportOnly,
        executable,
        memberships,
    });
    capture.columns.sort_by_key(|p| p.policy_version_id);
    capture.bundles.sort_by_key(|b| b.bundle_version_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::resolver::{
        AssignmentTarget, EffectivePolicySet, EffectivePolicySource, PolicySpecificity,
        ProvenanceEntry,
    };

    #[test]
    fn display_metadata_whitelist_excludes_executable_and_arbitrary_payloads() {
        let ids = display_identifiers(
            &serde_json::json!({
                "source_rule_id": "SV-123r1_rule", "source_group_id": "V-123",
                "cis_section": "5.2.3", "srg_ids": ["SRG-OS-123", 7],
                "cci_ids": ["CCI-000001", ""],
                "name": "made-up-ID", "identifiers": [{"value": "unreviewed"}],
                "expression": "secret-expression", "assertion": "unsafe-source",
                "attribute_path": "config.secret", "checks": [{"content": "secret-check"}],
                "config": {"secret": "secret-config"}, "overlays": ["secret-overlay"]
            }),
            vec!["REQ-1".into(), "V-123".into()],
        );
        assert_eq!(
            ids,
            vec![
                "5.2.3",
                "CCI-000001",
                "REQ-1",
                "SRG-OS-123",
                "SV-123r1_rule",
                "V-123"
            ]
        );
        assert!(
            display_identifiers(
                &serde_json::json!({"source_rule_id": [], "srg_ids": "bad-type"}),
                vec![]
            )
            .is_empty()
        );
    }

    #[test]
    fn authoritative_overlap_uses_version_metadata_and_omits_superseded_sources_and_config() {
        let policy_id = Uuid::new_v4();
        let version = Uuid::new_v4();
        let system = Uuid::new_v4();
        let bundle_ids = [Uuid::new_v4(), Uuid::new_v4()];
        let assignment_ids = [Uuid::new_v4(), Uuid::new_v4()];
        let assignment_versions = [Uuid::new_v4(), Uuid::new_v4()];
        let source = |assignment_id, bundle_version_id, authoritative| ProvenanceEntry {
            source: EffectivePolicySource::Baseline,
            specificity: PolicySpecificity::System,
            assignment_id: Some(assignment_id),
            bundle_id: Some(Uuid::new_v4()),
            bundle_version_id: Some(bundle_version_id),
            scope_type: Some("system".into()),
            enforcement_mode: "report_only".into(),
            authoritative,
        };
        let policy = EffectivePolicy {
            policy_version_id: version,
            policy_lineage_id: policy_id,
            policy_type: "custom_check".into(),
            source: EffectivePolicySource::Baseline,
            specificity: PolicySpecificity::System,
            baseline_order: Some(0),
            addition_order: None,
            overrides: vec![],
            effective_config: serde_json::json!({"expression": "sensitive-executable-content"}),
            assignment_mode: AssignmentMode::ReportOnly,
            effective_mode: AssignmentMode::ReportOnly,
            provenance: vec![
                source(assignment_ids[0], bundle_ids[0], true),
                source(assignment_ids[1], bundle_ids[1], true),
                source(Uuid::new_v4(), Uuid::new_v4(), false),
            ],
        };
        let metadata = CaptureMetadata {
            policies: HashMap::from([(
                version,
                EvalPolicyColumn {
                    policy_version_id: version,
                    policy_id,
                    name: "Exact version name".into(),
                    version: "1".into(),
                    identifiers: vec![],
                    description: None,
                    assertion: None,
                    attribute_path: None,
                },
            )]),
            bundles: bundle_ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        EvalBundleColumn {
                            bundle_version_id: *id,
                            bundle_id: Uuid::new_v4(),
                            name: "Version bundle".into(),
                            version: "2".into(),
                            framework: "Version framework".into(),
                            framework_version: None,
                        },
                    )
                })
                .collect(),
            assignments: (0..2)
                .map(|i| (assignment_ids[i], (assignment_versions[i], bundle_ids[i])))
                .collect(),
        };
        let mut capture = new_snapshot();
        capture.rows.push(EvalMatrixRow {
            greenfield: None,
            system_id: Some(system),
            system_name: "host".into(),
            configuration_name: "configuration".into(),
            resolution: EvalMatrixResolution::Resolved,
            evaluation_digest: None,
            policies: vec![],
            cells: vec![],
            details: vec![],
            assigned_bundles: vec![],
        });
        bind_policy(&metadata, &mut capture, system, &policy, true).unwrap();
        assert_eq!(capture.columns.len(), 1);
        assert_eq!(capture.columns[0].name, "Exact version name");
        assert_eq!(capture.bundles.len(), 2);
        let binding = &capture.rows[0].policies[0];
        assert!(binding.report_only);
        assert_eq!(binding.memberships.len(), 2);
        assert!(binding.memberships.iter().all(|m| {
            m.assignment_version_id
                .is_some_and(|id| assignment_versions.contains(&id))
        }));
        let json = serde_json::to_string(&capture).unwrap();
        assert!(!json.contains("sensitive-executable-content"));
        assert!(!json.contains("effective_config"));
        assert!(!json.contains("expression"));
    }

    #[test]
    fn successful_empty_conflicted_skipped_and_missing_rows_remain_distinct() {
        let ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let systems = vec![
            (ids[0], "empty".into(), "empty-host".into(), Some(true)),
            (ids[1], "shared".into(), "conflict-host".into(), Some(false)),
            (ids[2], "shared".into(), "skipped-host".into(), None),
            (ids[3], "missing".into(), "missing-host".into(), None),
        ];
        let empty = |id| {
            ResolutionOutcome::Resolved(EffectivePolicySet {
                bundle_version_id: Uuid::nil(),
                assignment_id: None,
                target: AssignmentTarget::System { system_id: id },
                policies: vec![],
                effective_set_digest: "empty".into(),
                warnings: vec![],
            })
        };
        let outcomes = HashMap::from([
            ((None, ids[0]), empty(ids[0])),
            ((None, ids[1]), ResolutionOutcome::Conflict(vec![])),
            ((None, ids[2]), empty(ids[2])),
        ]);
        let mut capture = new_snapshot();
        initialize_rows(&mut capture, &systems, &outcomes);
        mark_skipped_configurations(&mut capture, &BTreeSet::from(["shared".into()]));
        assert_eq!(capture.rows.len(), 4);
        assert_eq!(
            capture
                .rows
                .iter()
                .map(|r| r.resolution)
                .collect::<Vec<_>>(),
            vec![
                EvalMatrixResolution::Resolved,
                EvalMatrixResolution::Conflict,
                EvalMatrixResolution::Skipped,
                EvalMatrixResolution::Unavailable,
            ]
        );
        assert!(!capture.empty_bundle_metadata_complete);
        assert_eq!(
            capture
                .rows
                .iter()
                .map(|r| r.greenfield)
                .collect::<Vec<_>>(),
            vec![Some(true), Some(false), None, None]
        );
        assert!(capture.rows.iter().all(|r| r.policies.is_empty()));
    }
}
