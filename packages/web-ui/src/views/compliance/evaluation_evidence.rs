//! Read-only exact evaluation-check evidence in Compliance context.
//!
//! This receiver never queries current bundle membership or deployed evidence.
//! Full route identity is validated against the authorized pinned API response.
//! Missing or mismatched evidence fails closed, including after refresh.

use crate::api::{
    client::fetch_eval_attempt_policy_matrix,
    models::{EvalPolicyMatrixResponse, eval_policy_matrix::*},
};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel,
};
use crate::components::icon::{Icon, IconName};
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
    if !identity.matches_response(data) || snapshot.provenance != EvalMatrixProvenance::Captured {
        return None;
    }
    let mut bundles = snapshot.bundles.iter().filter(|b| {
        b.bundle_id == target.bundle_id && b.bundle_version_id == target.bundle_version_id
    });
    bundles.next()?;
    if bundles.next().is_some() {
        return None;
    }
    let mut positions = snapshot.columns.iter().enumerate().filter(|(_, p)| {
        p.policy_id == target.policy_id && p.policy_version_id == target.policy_version_id
    });
    let (position, _) = positions.next()?;
    if positions.next().is_some() {
        return None;
    }
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

    #[test]
    fn route_resolves_from_api_without_a_browser_handoff() {
        let (data, target) = fixture();
        let route = EvaluationEvidenceRoute::parse(
            1,
            target.evaluation.attempt_id,
            &target.bundle_id.to_string(),
            &target.bundle_version_id.to_string(),
            &target.system_id.to_string(),
            &target.policy_version_id.to_string(),
        )
        .unwrap();
        let resolved = route.resolve(&data).unwrap();
        assert_eq!(resolved, target);
        assert!(validate(&data, &resolved).is_some());
        let mut wrong_commit = route.clone();
        wrong_commit.commit_id += 1;
        assert!(wrong_commit.resolve(&data).is_none());
        let mut wrong_attempt = route.clone();
        wrong_attempt.attempt_id = Uuid::from_u128(99);
        assert!(wrong_attempt.resolve(&data).is_none());
        let mut wrong_lineage = data.clone();
        wrong_lineage.matrix.as_mut().unwrap().bundles[0].bundle_id = Uuid::from_u128(99);
        assert!(validate(&wrong_lineage, &route.resolve(&wrong_lineage).unwrap()).is_none());
        let mut no_contribution = data.clone();
        no_contribution.matrix.as_mut().unwrap().rows[0].policies[0]
            .memberships
            .clear();
        assert!(validate(&no_contribution, &route.resolve(&no_contribution).unwrap()).is_none());
        let mut legacy = data.clone();
        legacy.matrix.as_mut().unwrap().provenance = EvalMatrixProvenance::LegacyUnavailable;
        assert!(validate(&legacy, &route.resolve(&legacy).unwrap()).is_none());
    }

    #[test]
    fn duplicate_captured_versions_do_not_authorize_arbitrary_selection() {
        let (mut data, target) = fixture();
        let duplicate = data.matrix.as_ref().unwrap().columns[0].clone();
        data.matrix.as_mut().unwrap().columns.push(duplicate);
        assert!(validate(&data, &target).is_none());
        let (mut data, target) = fixture();
        let duplicate = data.matrix.as_ref().unwrap().bundles[0].clone();
        data.matrix.as_mut().unwrap().bundles.push(duplicate);
        assert!(validate(&data, &target).is_none());
    }

    #[test]
    fn pinned_identity_rejects_missing_malformed_and_retried_context() {
        let (data, target) = fixture();
        for index in 0..8 {
            let mut wrong = data.clone();
            let snapshot = wrong.matrix.as_mut().unwrap();
            match index {
                0 => snapshot.commit_sha = None,
                1 => snapshot.commit_sha = Some("latest".into()),
                2 => snapshot.commit_sha = Some("b".repeat(40)),
                3 => snapshot.evaluation_attempt_id = None,
                4 => snapshot.evaluation_attempt_id = Some(Uuid::nil()),
                5 => snapshot.evaluation_attempt_id = Some(Uuid::from_u128(99)),
                6 => snapshot.attempt_number = 0,
                _ => snapshot.attempt_number += 1,
            }
            assert!(
                !target.evaluation.matches_response(&wrong),
                "context {index}"
            );
        }
        let mut wrong = data.clone();
        wrong.commit_id += 1;
        assert!(!target.evaluation.matches_response(&wrong));
        wrong.matrix = None;
        assert!(!target.evaluation.matches_response(&wrong));
    }
}

/// Displays one exact captured check without manufacturing assessment IDs.
#[component]
pub(super) fn EvaluationEvidenceDrawer(
    commit_id: i32,
    attempt_id: uuid::Uuid,
    bundle: String,
    version: String,
    system: String,
    policy: String,
) -> Element {
    let requested =
        EvaluationEvidenceRoute::parse(commit_id, attempt_id, &bundle, &version, &system, &policy);
    let resource_target = requested.clone();
    let resource = use_resource(move || {
        let requested = resource_target.clone();
        async move {
            match requested {
                Some(target) => Some(
                    fetch_eval_attempt_policy_matrix(target.commit_id, target.attempt_id).await,
                ),
                None => None,
            }
        }
    });
    let nav = navigator();
    let close = move || {
        nav.push(Route::ComplianceView {
            bundle: String::new(),
            version: String::new(),
            system: String::new(),
            policy: String::new(),
            poam: String::new(),
            view: String::new(),
        });
    };
    rsx! {
        div { class: "fl-tray-backdrop", onclick: move |_| close() }
        aside { id: "compliance-evaluation-dialog", class: "fl-tray", role: "dialog", aria_modal: "true", aria_labelledby: "compliance-evaluation-title", tabindex: "-1", style: "width:min(900px,96vw);",
            onkeydown: move |event| if event.key() == Key::Escape { close(); },
            DialogFocusRestore {}
            DialogFocusSentinel { dialog_id: "compliance-evaluation-dialog".to_string(), boundary: DialogFocusBoundary::Last }
            header { class: "fl-tray-head",
                h2 { id: "compliance-evaluation-title", "Evaluation check evidence" }
                button { class: "btn-icon focus-ring", autofocus: true, aria_label: "Close evaluation evidence", onclick: move |_| close(), Icon { name: IconName::X, size: 16 } }
            }
            div { style: "padding:18px;overflow:auto;flex:1;",
            p { "Read-only captured evaluation evidence. This check does not establish framework requirement satisfaction or deployed compliance." }
            button { class: "btn btn-ghost focus-ring", onclick: move |_| { nav.push(Route::EvaluationsAttemptView { commit_id, attempt_id }); }, "Back to Evaluation" }
            match (&requested, &*resource.read()) {
                (Some(requested), Some(Some(Ok(data)))) => {
                    let target = requested.resolve(data);
                    if let Some((target, (snapshot, row, position))) = target.as_ref().and_then(|target| validate(data, target).map(|validated| (target, validated))) {
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
                _ => rsx! { p { role: "alert", "Evaluation evidence unavailable. The exact route is invalid, or the retained attempt is missing or unauthorized." } },
            }
            }
            DialogFocusSentinel { dialog_id: "compliance-evaluation-dialog".to_string(), boundary: DialogFocusBoundary::First }
        }
    }
}
