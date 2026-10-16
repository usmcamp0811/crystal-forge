//! Read-only exact evaluation-check evidence in Compliance context.
//!
//! This receiver never queries current bundle membership or deployed evidence.
//! A missing in-memory handoff or mismatched route is explicitly unavailable.

use crate::api::{
    client::fetch_eval_attempt_policy_matrix,
    models::{EvalPolicyMatrixResponse, eval_policy_matrix::*},
};
use crate::routes::Route;
use crate::state::evaluation_drawer::*;
use dioxus::prelude::*;

/// Validates every evidence identity and the actual per-system contribution.
fn validate<'a>(
    data: &'a EvalPolicyMatrixResponse,
    target: &EvaluationEvidence,
) -> Option<(&'a EvalPolicyMatrixSnapshot, &'a EvalMatrixRow, usize)> {
    let snapshot = data.matrix.as_ref()?;
    let identity = &target.evaluation;
    if identity.commit_sha.len() != 40
        || !identity.commit_sha.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    if data.commit_id != identity.commit_id
        || snapshot.provenance != EvalMatrixProvenance::Captured
        || snapshot.commit_sha.as_ref() != Some(&identity.commit_sha)
        || snapshot.evaluation_attempt_id != Some(identity.attempt_id)
        || snapshot.attempt_number != identity.attempt_number
    {
        return None;
    }
    snapshot.bundles.iter().find(|b| {
        b.bundle_id == target.bundle_id && b.bundle_version_id == target.bundle_version_id
    })?;
    let position = snapshot.columns.iter().position(|p| {
        p.policy_id == target.policy_id && p.policy_version_id == target.policy_version_id
    })?;
    let mut rows = snapshot
        .rows
        .iter()
        .filter(|r| r.system_id == Some(target.system_id));
    let row = rows.next()?;
    // Ambiguous duplicate system contexts do not authorize arbitrary selection.
    if rows.next().is_some() {
        return None;
    }
    row.policies.iter().find(|p| {
        p.policy_version_id == target.policy_version_id
            && p.memberships
                .iter()
                .any(|m| m.bundle_version_id == target.bundle_version_id)
    })?;
    Some((snapshot, row, position))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn fixture() -> (EvalPolicyMatrixResponse, EvaluationEvidence) {
        let id = |n| Uuid::from_u128(n);
        let target = EvaluationEvidence {
            evaluation: EvaluationIdentity {
                commit_id: 1,
                commit_sha: "a".repeat(40),
                attempt_id: id(1),
                attempt_number: 2,
            },
            system_id: id(2),
            bundle_id: id(3),
            bundle_version_id: id(4),
            policy_id: id(5),
            policy_version_id: id(6),
        };
        let data = serde_json::from_value(json!({"commit_id":1,"policies":[],"systems":[],"matrix":{
            "commit_sha":target.evaluation.commit_sha,"evaluation_attempt_id":id(1),"attempt_number":2,"provenance":"captured","empty_bundle_metadata_complete":true,
            "columns":[{"policy_id":id(5),"policy_version_id":id(6),"name":"Captured","version":"1"}],
            "bundles":[{"bundle_id":id(3),"bundle_version_id":id(4),"name":"Captured bundle","version":"1","framework":"F","framework_version":null}],
            "rows":[{"system_id":id(2),"system_name":"host","configuration_name":"config","resolution":"resolved","evaluation_digest":null,"cells":["unknown"],
                "policies":[{"policy_version_id":id(6),"report_only":false,"executable":true,"memberships":[{"bundle_version_id":id(4),"assignment_id":null,"assignment_version_id":null}]}]}]
        }})).unwrap();
        (data, target)
    }

    #[test]
    fn exact_evidence_accepts_unknown_outcomes_without_inventing_results() {
        let (data, target) = fixture();
        let (_, row, position) = validate(&data, &target).unwrap();
        assert_eq!(row.cells[position], EvalMatrixCell::Unknown);
        assert!(row.details.is_empty());
    }

    #[test]
    fn exact_evidence_rejects_each_mismatched_identity_and_membership() {
        let (data, target) = fixture();
        for index in 0..9 {
            let mut wrong = target.clone();
            match index {
                0 => wrong.evaluation.commit_id += 1,
                1 => wrong.evaluation.commit_sha = "b".repeat(40),
                2 => wrong.evaluation.attempt_id = Uuid::from_u128(99),
                3 => wrong.evaluation.attempt_number += 1,
                4 => wrong.system_id = Uuid::from_u128(99),
                5 => wrong.bundle_id = Uuid::from_u128(99),
                6 => wrong.bundle_version_id = Uuid::from_u128(99),
                7 => wrong.policy_id = Uuid::from_u128(99),
                _ => wrong.policy_version_id = Uuid::from_u128(99),
            }
            assert!(validate(&data, &wrong).is_none(), "identity {index}");
        }
        let mut absent = data.clone();
        absent.matrix.as_mut().unwrap().rows[0].policies[0]
            .memberships
            .clear();
        assert!(validate(&absent, &target).is_none());
        let mut ambiguous = data.clone();
        let row = ambiguous.matrix.as_ref().unwrap().rows[0].clone();
        ambiguous.matrix.as_mut().unwrap().rows.push(row);
        assert!(validate(&ambiguous, &target).is_none());
    }
}

/// Displays one exact captured check without manufacturing assessment IDs.
#[component]
pub(super) fn EvaluationEvidenceDrawer(
    bundle: String,
    version: String,
    system: String,
    policy: String,
) -> Element {
    let target = use_hook(|| {
        EVALUATION_EVIDENCE.peek().clone().filter(|target| {
            target.bundle_id.to_string() == bundle
                && target.bundle_version_id.to_string() == version
                && target.system_id.to_string() == system
                && target.policy_id.to_string() == policy
        })
    });
    let requested = target.clone();
    let resource = use_resource(move || {
        let requested = requested.clone();
        async move {
            match requested {
                Some(target) => Some(
                    fetch_eval_attempt_policy_matrix(
                        target.evaluation.commit_id,
                        target.evaluation.attempt_id,
                    )
                    .await,
                ),
                None => None,
            }
        }
    });
    let nav = navigator();
    let return_commit = target.as_ref().map(|t| t.evaluation.commit_id).or_else(|| {
        EVALUATION_RETURN
            .peek()
            .as_ref()
            .map(|r| r.identity.commit_id)
    });
    rsx! {
        section { class: "page-content", aria_label: "Compliance evaluation evidence",
            h1 { "Evaluation check evidence" }
            p { "Read-only captured evaluation evidence. This check does not establish framework requirement satisfaction or deployed compliance." }
            button { class: "btn btn-ghost focus-ring", onclick: move |_| { *EVALUATION_EVIDENCE.write() = None; if let Some(commit_id) = return_commit { nav.push(Route::EvaluationsCommitView { commit_id }); } else { nav.push(Route::EvaluationsView {}); } }, "Back to Evaluation" }
            match (&target, &*resource.read()) {
                (Some(target), Some(Some(Ok(data)))) => {
                    if let Some((snapshot, row, position)) = validate(data, target) {
                        let control = &snapshot.columns[position];
                        let outcome = row.cells.get(position).copied().unwrap_or(EvalMatrixCell::Unknown);
                        rsx! {
                            aside { class: "pm-expand pm-captured-details", role: "region", aria_label: "Exact evaluation evidence",
                                if snapshot.mock_evidence { p { role: "note", "Mock evaluation — simulated outcomes, not compliance evidence." } }
                                h2 { "{control.name} · {control.version}" }
                                p { "Commit ", code { "{target.evaluation.commit_sha}" }, " · attempt {target.evaluation.attempt_number} · ", code { "{target.evaluation.attempt_id}" } }
                            p { {format!("Attempt status: {}", snapshot.attempt_status.as_deref().unwrap_or("Unavailable"))} }
                                if let Some(started) = snapshot.started_at { p { "Started: {started}" } }
                                if let Some(completed) = snapshot.completed_at { p { "Completed: {completed}" } }
                                p { "System: {row.system_name} · {target.system_id}" }
                                p { "Bundle lineage {target.bundle_id} · version {target.bundle_version_id}" }
                                p { "Policy lineage {target.policy_id} · version {target.policy_version_id}" }
                                p { "Result: {outcome:?}" }
                            if !control.identifiers.is_empty() { p { {format!("Captured identifiers: {}", control.identifiers.join(", "))} } }
                                if let Some(description) = &control.description { p { "{description}" } }
                                if let Some(assertion) = &control.assertion { pre { "{assertion}" } }
                                if let Some(path) = &control.attribute_path { code { "{path}" } }
                                if row.policies.iter().any(|binding| binding.policy_version_id == target.policy_version_id && binding.executable) && matches!(outcome, EvalMatrixCell::Pass | EvalMatrixCell::Warn | EvalMatrixCell::Fail) {
                                    if let Some(Some(detail)) = row.details.get(position) { pre { style: "white-space:pre-wrap;overflow-wrap:anywhere", "{detail}" } }
                                } else { p { "Matching-attempt result details are unavailable." } }
                            }
                        }
                    } else { rsx! { p { role: "alert", "Evaluation evidence unavailable: exact captured context or membership does not match." } } }
                },
                (Some(_), None) => rsx! { p { role: "status", "Loading exact evaluation evidence…" } },
                _ => rsx! { p { role: "alert", "Evaluation evidence unavailable. The exact attempt may be missing, unauthorized, or the in-memory navigation context has expired." } },
            }
        }
    }
}
