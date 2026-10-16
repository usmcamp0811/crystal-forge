//! Opens exact receivers only after validating server-projected attempt context.

use crate::api::models::eval_policy_matrix::*;
use crate::routes::Route;
use crate::state::evaluation_drawer::*;
use dioxus::prelude::*;

/// Validates a drawer summary against the represented matrix attempt.
pub(super) fn identity(
    snapshot: &EvalPolicyMatrixSnapshot,
    seed: &EvaluationReturn,
) -> Option<EvaluationIdentity> {
    let sha = snapshot.commit_sha.as_ref()?;
    if sha != &seed.identity.commit_sha
        || sha.len() != 40
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        || snapshot.attempt_number != seed.identity.attempt_number
    {
        return None;
    }
    Some(EvaluationIdentity {
        commit_id: seed.identity.commit_id,
        commit_sha: sha.clone(),
        attempt_id: snapshot.evaluation_attempt_id?,
        attempt_number: snapshot.attempt_number,
    })
}

/// Opens Config at the captured system UUID and full commit SHA.
#[component]
pub(super) fn OpenSystem(
    snapshot: EvalPolicyMatrixSnapshot,
    system_id: Option<uuid::Uuid>,
) -> Element {
    let seed = use_context::<Signal<EvaluationReturn>>();
    let exact = identity(&snapshot, &seed.read());
    let nav = navigator();
    rsx! { button { class: "btn btn-ghost xs focus-ring", disabled: exact.is_none() || system_id.is_none(),
    id: "evaluation-open-system-{system_id:?}",
    title: "Inspect this system at the exact evaluated commit",
    onclick: move |_| {
        let (Some(exact), Some(system_id)) = (exact.clone(), system_id) else { return; };
        let mut saved = seed(); saved.identity = exact.clone(); saved.system = Some(system_id);
        spawn(async move {
            saved.dom = capture_dom().await;
            *EVALUATION_RETURN.write() = Some(saved);
            nav.push(Route::SystemDetailView { id: system_id.to_string(), tab: "config".into(), poam: String::new(), config_mode: "commit".into(), revision: exact.commit_sha, generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() });
        });
    }, "Open System" } }
}

/// Offers one exact evidence action per captured contributing bundle.
#[component]
pub(super) fn OpenEvidence(
    snapshot: EvalPolicyMatrixSnapshot,
    system_id: Option<uuid::Uuid>,
    policy: EvalPolicyColumn,
    bundle: EvalBundleColumn,
) -> Element {
    let seed = use_context::<Signal<EvaluationReturn>>();
    let exact = identity(&snapshot, &seed.read());
    let nav = navigator();
    rsx! { button { class: "btn btn-ghost xs focus-ring", disabled: exact.is_none() || system_id.is_none(),
    id: "evaluation-evidence-{system_id:?}-{policy.policy_version_id}-{bundle.bundle_version_id}",
    onclick: move |_| {
        let (Some(evaluation), Some(system_id)) = (exact.clone(), system_id) else { return; };
        let evidence = EvaluationEvidence { evaluation: evaluation.clone(), system_id, bundle_id: bundle.bundle_id, bundle_version_id: bundle.bundle_version_id, policy_id: policy.policy_id, policy_version_id: policy.policy_version_id };
        let mut saved = seed(); saved.identity = evaluation; saved.system = None;
        spawn(async move {
            saved.dom = capture_dom().await;
            *EVALUATION_RETURN.write() = Some(saved);
            *EVALUATION_EVIDENCE.write() = Some(evidence);
            nav.push(Route::ComplianceView { bundle: bundle.bundle_id.to_string(), version: bundle.bundle_version_id.to_string(), system: system_id.to_string(), policy: policy.policy_id.to_string(), poam: String::new(), view: "evaluation-evidence".into() });
        });
    }, "View Evidence · {bundle.name} · {bundle.version}" } }
}
