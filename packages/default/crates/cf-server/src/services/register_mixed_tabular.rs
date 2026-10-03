//! Converts a fully authorized mixed register selection to CSV and XLSX.
//!
//! All names, finding links, and baseline scans come from the selector's
//! repeatable-read snapshot. This converter never reads the database again.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, ensure};
use uuid::Uuid;

use super::register_export_selection::RegisterExportSelection;
use super::register_tabular_export::{
    self as tabular, AcceptanceKind, Entry, Evidence, Scope, Snapshot, Source,
};
use crate::queries::acceptance_register::AcceptanceSource;

const NO_LINK: &str = "No source-linked finding or scan recorded";

struct Owned<'a> {
    uuid: Uuid,
    source_id: String,
    title: String,
    description: &'a str,
    status: String,
    scopes: Vec<Scope<'a>>,
    cve_id: Option<&'a str>,
    source: Source,
    evidence: Vec<(Option<Uuid>, Option<Uuid>, String)>,
}

/// Writes both formats from one complete authorized POA&M and decision selection.
///
/// The caller must use the whole-scope [`RegisterExportSelection`] for the
/// authenticated actor. Plan scopes include each recorded link and assignment
/// scope, with no host-environment inference. The single-CVE column is set
/// only when all linked and scheduled tuples share one canonical CVE ID.
/// Scheduled tuples without a finding appear as labeled, scan-less evidence.
/// Missing scan and finding facts stay blank with an explicit evidence note.
///
/// # Errors
/// Returns an error for incomplete, hidden, or duplicate context, invalid
/// source fields, or serialization limits. No partial download is returned.
pub fn write_authorized(selection: &RegisterExportSelection) -> Result<tabular::Exports> {
    ensure!(
        selection.poams.len() + selection.acceptances.len()
            <= super::register_export_selection::MAX_AUTHORIZED_EXPORT_ROWS,
        "mixed register source limit exceeded"
    );
    let plans = selection
        .poam_context
        .iter()
        .map(|context| (context.poam_id, context))
        .collect::<HashMap<_, _>>();
    let decisions = selection
        .acceptance_context
        .iter()
        .map(|context| (context.source_id, context))
        .collect::<HashMap<_, _>>();
    ensure!(
        plans.len() == selection.poams.len() && decisions.len() == selection.acceptances.len(),
        "mixed register has incomplete or duplicate context"
    );
    let mut seen = HashSet::new();
    let mut owned = Vec::with_capacity(selection.poams.len() + selection.acceptances.len());
    for row in &selection.poams {
        let plan = &row.summary;
        ensure!(seen.insert(plan.id), "duplicate mixed register source UUID");
        let context = plans
            .get(&plan.id)
            .ok_or_else(|| anyhow::anyhow!("plan {} has no authorized source context", plan.id))?;
        // SECURITY: Page-scoped presentation may omit retired or hidden links.
        // An incomplete source cannot be represented as a complete export.
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
            "plan has hidden source context"
        );
        ensure!(
            context.system_ids.len() == context.system_names.len()
                && context.environment_ids.len() == context.environment_names.len(),
            "plan has incomplete scope names"
        );
        let scopes = context
            .system_ids
            .iter()
            .zip(&context.system_names)
            .map(|(&id, name)| Scope::System { id, name })
            .chain(
                context
                    .environment_ids
                    .iter()
                    .zip(&context.environment_names)
                    .map(|(&id, name)| Scope::Environment { id, name }),
            )
            .collect();
        let mut evidence: Vec<_> = context
            .links
            .iter()
            .map(|link| {
                (
                    Some(link.finding_id),
                    link.scan_id,
                    link.description.clone(),
                )
            })
            .collect();
        // A schedule is a decision about an exact tuple, not a scan or finding.
        for tuple in context.scheduled_cve_tuples.iter() {
            if !context.links.iter().any(|link| {
                link.canonical_cve_id.as_deref() == Some(tuple.canonical_cve_id.as_str())
                    && link.canonical_package_name.as_deref()
                        == Some(tuple.canonical_package_name.as_str())
            }) {
                evidence.push((None, None, format!("{} / {} (scheduled environment disposition; no source-linked finding or scan recorded)", tuple.canonical_cve_id, tuple.canonical_package_name)));
            }
        }
        if evidence.is_empty() {
            evidence.push((None, None, NO_LINK.into()));
        }
        let cve_id = (context.cve_ids.len() == 1).then(|| context.cve_ids[0].as_str());
        owned.push(Owned {
            uuid: plan.id,
            source_id: plan.human_id.clone(),
            title: plan.title.clone(),
            description: &plan.plan,
            status: plan.status.clone(),
            scopes,
            cve_id,
            source: Source::Plan {
                target_date: plan.target_date,
            },
            evidence,
        });
    }
    for row in &selection.acceptances {
        ensure!(
            seen.insert(row.source_id),
            "duplicate mixed register source UUID"
        );
        let context = decisions.get(&row.source_id).ok_or_else(|| {
            anyhow::anyhow!(
                "decision {} has no authorized source context",
                row.source_id
            )
        })?;
        let (title, scope, kind) = match row.source {
            AcceptanceSource::PolicyWaiver => {
                ensure!(
                    row.finding_id.is_some() && row.finding_id == context.finding_id,
                    "waiver has incomplete finding context"
                );
                (
                    context.policy_name.clone().unwrap_or_default(),
                    Scope::System {
                        id: row
                            .system_id
                            .ok_or_else(|| anyhow::anyhow!("waiver has no system"))?,
                        name: &context.scope_name,
                    },
                    AcceptanceKind::Policy,
                )
            }
            AcceptanceSource::CveHost => (
                format!(
                    "{} / {}",
                    row.canonical_cve_id.as_deref().unwrap_or(""),
                    row.canonical_package_name.as_deref().unwrap_or("")
                ),
                Scope::System {
                    id: row
                        .system_id
                        .ok_or_else(|| anyhow::anyhow!("decision has no system"))?,
                    name: &context.scope_name,
                },
                AcceptanceKind::Cve,
            ),
            AcceptanceSource::CveEnvironment => (
                format!(
                    "{} / {}",
                    row.canonical_cve_id.as_deref().unwrap_or(""),
                    row.canonical_package_name.as_deref().unwrap_or("")
                ),
                Scope::Environment {
                    id: row
                        .environment_id
                        .ok_or_else(|| anyhow::anyhow!("decision has no environment"))?,
                    name: &context.scope_name,
                },
                AcceptanceKind::Cve,
            ),
        };
        ensure!(
            match row.source {
                AcceptanceSource::PolicyWaiver => row.environment_id.is_none(),
                AcceptanceSource::CveHost =>
                    row.environment_id.is_none() && row.finding_id.is_none(),
                AcceptanceSource::CveEnvironment =>
                    row.system_id.is_none() && row.finding_id.is_none(),
            },
            "decision has ambiguous source scope"
        );
        let review = match row.source {
            AcceptanceSource::PolicyWaiver => row.review_due_at,
            _ => row.review_date,
        };
        let status = if row.retired_at.is_some() {
            if row.replacement_poam_id.is_some() {
                "converted (retired accepted)".to_owned()
            } else {
                "retired accepted".to_owned()
            }
        } else if row.source == AcceptanceSource::PolicyWaiver && row.replacement_poam_id.is_some()
        {
            "converted (revoked)".to_owned()
        } else {
            row.status.clone()
        };
        let evidence = vec![(
            context.finding_id,
            None,
            if context.finding_id.is_some() {
                "Policy finding"
            } else {
                NO_LINK
            }
            .to_owned(),
        )];
        owned.push(Owned {
            uuid: row.source_id,
            source_id: row.source_id.to_string(),
            title,
            description: &row.justification,
            status,
            scopes: vec![scope],
            cve_id: row.canonical_cve_id.as_deref(),
            source: Source::Acceptance {
                kind,
                review_deadline: review,
                authorization_expiry: row.expires_at,
                human_id: Some(row.human_id.clone()),
            },
            evidence,
        });
    }
    let evidence: Vec<Vec<_>> = owned
        .iter()
        .map(|record| {
            record
                .evidence
                .iter()
                .map(|(finding_id, scan_id, description)| Evidence {
                    finding_id: *finding_id,
                    scan_id: *scan_id,
                    description,
                })
                .collect()
        })
        .collect();
    let entries = owned
        .iter()
        .zip(&evidence)
        .map(|(record, evidence)| Entry {
            uuid: record.uuid,
            source_id: &record.source_id,
            title: &record.title,
            description: record.description,
            status: &record.status,
            scope: match record.scopes.as_slice() {
                [] => Scope::Unspecified,
                [Scope::System { id, name }] => Scope::System { id: *id, name },
                [Scope::Environment { id, name }] => Scope::Environment { id: *id, name },
                scopes => Scope::Multiple { scopes },
            },
            cve_id: record.cve_id,
            source: match &record.source {
                Source::Plan { target_date } => Source::Plan {
                    target_date: *target_date,
                },
                Source::Acceptance {
                    kind,
                    review_deadline,
                    authorization_expiry,
                    human_id,
                } => Source::Acceptance {
                    kind: match kind {
                        AcceptanceKind::Policy => AcceptanceKind::Policy,
                        AcceptanceKind::Cve => AcceptanceKind::Cve,
                    },
                    review_deadline: *review_deadline,
                    authorization_expiry: *authorization_expiry,
                    human_id: human_id.clone(),
                },
            },
            evidence,
        })
        .collect::<Vec<_>>();
    tabular::write_register(&Snapshot { entries: &entries })
}
