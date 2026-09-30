//! Converts a complete authorized register snapshot to OSCAL POA&M encodings.
//!
//! No observation, assessment, measured risk impact, or approval is derived
//! from decision justification or a plan's presence in the register.

use anyhow::{Result, ensure};
use chrono::{NaiveDate, Utc};
use uuid::Uuid;

use super::oscal_poam_export::{
    self, CveFindingLink, Document, Encodings, Entry, ScheduledCveTuple, Source,
};
use super::register_export_selection::RegisterExportSelection;
use crate::models::poam::PoamAssigneeView;
use crate::queries::acceptance_register::AcceptanceSource;

struct OwnedEntry {
    uuid: Uuid,
    source_id: String,
    title: String,
    description: String,
    scopes: Vec<String>,
    cve_ids: Vec<String>,
    scheduled_cve_tuples: Vec<ScheduledCveTuple>,
    cve_finding_links: Vec<CveFindingLink>,
    policy_finding_ids: Vec<Uuid>,
    target_date: Option<NaiveDate>,
    status: String,
    source_kind: String,
    human_id: Option<String>,
    rationale: Option<String>,
    accepted_by: Option<Uuid>,
    accepted_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    owner: Option<String>,
    assignee_user_id: Option<Uuid>,
    assignee_group_name: Option<String>,
    review_date: Option<NaiveDate>,
    review_due_at: Option<NaiveDate>,
    expires_at: Option<chrono::DateTime<Utc>>,
    retired_at: Option<chrono::DateTime<Utc>>,
    retired_by: Option<Uuid>,
    retirement_reason: Option<String>,
    replacement_poam_id: Option<Uuid>,
}

/// Writes all selected plans and decisions as one generated OSCAL document.
///
/// The caller must pass the result of the whole-scope repeatable-read selector
/// for the authenticated reader. Document metadata identifies this export;
/// its timestamp is not the creation or last modification of any source row.
/// Every source identity remains stable as an item UUID across exports.
///
/// # Errors
/// Fails closed on incomplete context, duplicate identities, or invalid source
/// fields. No partial encoding is returned.
pub fn write_authorized(selection: &RegisterExportSelection) -> Result<Encodings> {
    ensure!(
        selection.poams.len() + selection.acceptances.len() <= 1_000,
        "OSCAL export exceeds 1,000 source rows"
    );
    ensure!(
        selection.poam_context.len() == selection.poams.len()
            && selection.acceptance_context.len() == selection.acceptances.len(),
        "OSCAL export has incomplete source context"
    );
    let mut owned = Vec::with_capacity(selection.poams.len() + selection.acceptances.len());
    let mut identities = std::collections::HashSet::new();
    ensure!(
        selection
            .poam_context
            .iter()
            .map(|c| c.poam_id)
            .collect::<std::collections::HashSet<_>>()
            .len()
            == selection.poam_context.len()
            && selection
                .acceptance_context
                .iter()
                .map(|c| c.source_id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == selection.acceptance_context.len(),
        "OSCAL export has duplicate source context"
    );
    for row in &selection.poams {
        let plan = &row.summary;
        ensure!(identities.insert((0, plan.id)), "duplicate plan identity");
        let context = selection
            .poam_context
            .iter()
            .find(|context| context.poam_id == plan.id)
            .ok_or_else(|| anyhow::anyhow!("plan {} has no linked context", plan.id))?;
        // SECURITY: A register row can omit historical or hidden system links.
        // Never export a partially authorized plan as if it were complete.
        ensure!(
            selection.is_admin
                || (context
                    .system_ids
                    .iter()
                    .all(|id| row.system_ids.contains(id))
                    && context
                        .visibility_system_ids
                        .iter()
                        .all(|id| row.system_ids.contains(id))
                    && context
                        .environment_ids
                        .iter()
                        .all(|id| row.environment_ids.contains(id))),
            "plan {} has hidden linked context",
            plan.id
        );
        let mut scopes: Vec<_> = context
            .system_ids
            .iter()
            .map(|id| format!("system:{id}"))
            .chain(
                context
                    .environment_ids
                    .iter()
                    .map(|id| format!("environment:{id}")),
            )
            .collect();
        scopes.sort();
        scopes.dedup();
        // A CVE baseline can lack a scan; use the selector's explicit tuple
        // identity rather than treating every scan-less link as policy.
        let mut policy_finding_ids: Vec<_> = context
            .links
            .iter()
            .filter(|link| link.canonical_cve_id.is_none() && link.canonical_package_name.is_none())
            .map(|link| link.finding_id)
            .collect();
        policy_finding_ids.sort();
        policy_finding_ids.dedup();
        let cve_finding_links = context
            .links
            .iter()
            .filter_map(
                |link| match (&link.canonical_cve_id, &link.canonical_package_name) {
                    (Some(cve), Some(package)) => Some(Ok(CveFindingLink {
                        finding_id: link.finding_id,
                        canonical_cve_id: cve.clone(),
                        canonical_package_name: package.clone(),
                        baseline_scan_id: link.scan_id,
                    })),
                    (None, None) if link.scan_id.is_none() => None,
                    _ => Some(Err(anyhow::anyhow!(
                        "plan {} has incomplete CVE link",
                        plan.id
                    ))),
                },
            )
            .collect::<Result<Vec<_>>>()?;
        let (assignee_user_id, assignee_group_name) = match &plan.assignee {
            PoamAssigneeView::User { user_id, .. } => (Some(*user_id), None),
            PoamAssigneeView::OidcGroup { group_name, .. } => (None, Some(group_name.clone())),
            PoamAssigneeView::Unassigned | PoamAssigneeView::Legacy { .. } => (None, None),
        };
        owned.push(OwnedEntry {
            uuid: plan.id,
            source_id: plan.human_id.clone(),
            title: plan.title.clone(),
            description: plan.plan.clone(),
            scopes,
            cve_ids: context.cve_ids.clone(),
            scheduled_cve_tuples: context
                .scheduled_cve_tuples
                .iter()
                .map(|tuple| ScheduledCveTuple {
                    canonical_cve_id: tuple.canonical_cve_id.clone(),
                    canonical_package_name: tuple.canonical_package_name.clone(),
                })
                .collect(),
            cve_finding_links,
            policy_finding_ids,
            target_date: plan.target_date,
            status: plan.status.clone(),
            source_kind: "plan".into(),
            human_id: None,
            rationale: None,
            accepted_by: None,
            accepted_at: None,
            created_at: plan.created_at,
            updated_at: plan.updated_at,
            owner: (!plan.owner.trim().is_empty()).then(|| plan.owner.clone()),
            assignee_user_id,
            assignee_group_name,
            review_date: None,
            review_due_at: None,
            expires_at: None,
            retired_at: None,
            retired_by: None,
            retirement_reason: None,
            replacement_poam_id: None,
        });
    }
    for decision in &selection.acceptances {
        let (kind, source_kind, title, scopes, cve_ids) = match decision.source {
            AcceptanceSource::PolicyWaiver => (
                1,
                "policy-waiver",
                "Policy waiver",
                decision.system_id.map(|id| vec![format!("system:{id}")]),
                Vec::new(),
            ),
            AcceptanceSource::CveHost => (
                2,
                "cve-host-decision",
                "Host CVE decision",
                decision.system_id.map(|id| vec![format!("system:{id}")]),
                decision.canonical_cve_id.iter().cloned().collect(),
            ),
            AcceptanceSource::CveEnvironment => (
                3,
                "cve-environment-decision",
                "Environment CVE decision",
                decision
                    .environment_id
                    .map(|id| vec![format!("environment:{id}")]),
                decision.canonical_cve_id.iter().cloned().collect(),
            ),
        };
        ensure!(
            identities.insert((kind, decision.source_id)),
            "duplicate decision identity"
        );
        let context = selection
            .acceptance_context
            .iter()
            .find(|c| c.source_id == decision.source_id)
            .ok_or_else(|| {
                anyhow::anyhow!("decision {} has no scope context", decision.source_id)
            })?;
        ensure!(
            !context.scope_name.trim().is_empty(),
            "decision has no scope label"
        );
        let scopes = scopes.ok_or_else(|| anyhow::anyhow!("decision has no source scope"))?;
        owned.push(OwnedEntry {
            uuid: decision.source_id,
            source_id: format!("{}:{}", source_kind, decision.source_id),
            title: context.policy_name.as_deref().unwrap_or(title).to_owned(),
            description: decision.justification.clone(),
            scopes,
            cve_ids,
            scheduled_cve_tuples: vec![],
            cve_finding_links: vec![],
            policy_finding_ids: context.finding_id.into_iter().collect(),
            target_date: None,
            status: decision.status.clone(),
            source_kind: source_kind.into(),
            human_id: Some(decision.human_id.clone()),
            rationale: Some(decision.justification.clone()),
            accepted_by: decision.accepted_by,
            accepted_at: decision.accepted_at,
            created_at: decision.recorded_at,
            updated_at: decision.recorded_at,
            owner: None,
            assignee_user_id: None,
            assignee_group_name: None,
            review_date: decision.review_date,
            review_due_at: decision.review_due_at,
            expires_at: decision.expires_at,
            retired_at: decision.retired_at,
            retired_by: decision.retired_by,
            retirement_reason: decision.retirement_reason.clone(),
            replacement_poam_id: decision.replacement_poam_id,
        });
    }
    let entries: Vec<_> = owned
        .iter()
        .map(|row| Entry {
            uuid: row.uuid,
            source_id: &row.source_id,
            title: &row.title,
            description: &row.description,
            scopes: &row.scopes,
            cve_ids: &row.cve_ids,
            scheduled_cve_tuples: &row.scheduled_cve_tuples,
            cve_finding_links: &row.cve_finding_links,
            policy_finding_ids: &row.policy_finding_ids,
            target_date: row.target_date,
            technical_evidence: None,
            source: if let Some(rationale) = &row.rationale {
                Source::Acceptance {
                    human_id: row.human_id.as_deref(),
                    source_kind: &row.source_kind,
                    rationale,
                    accepted_by: row.accepted_by,
                    accepted_at: row.accepted_at,
                    recorded_at: row.created_at,
                    review_date: row.review_date,
                    review_due_at: row.review_due_at,
                    expires_at: row.expires_at,
                    retired_at: row.retired_at,
                    retired_by: row.retired_by,
                    retirement_reason: row.retirement_reason.as_deref(),
                    replacement_poam_id: row.replacement_poam_id,
                    status: &row.status,
                }
            } else {
                Source::Plan {
                    status: &row.status,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    owner: row.owner.as_deref(),
                    assignee_user_id: row.assignee_user_id,
                    assignee_group_name: row.assignee_group_name.as_deref(),
                }
            },
        })
        .collect();
    let generated_at = Utc::now();
    oscal_poam_export::write_poam(&Document {
        uuid: Uuid::new_v4(),
        title: "Crystal Forge authorized register export",
        version: "1",
        last_modified: generated_at,
        entries: &entries,
    })
}
