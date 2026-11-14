//! System Detail batch triage: one host-override decision for selected pairs.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use chrono::{Duration, Utc};
use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::views::cves::batch_assignee;
use crate::views::poam_api::{self, PoamApiError};

/// Describes one selected System Detail row shown in the batch dialog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SystemBatchRow {
    /// Canonical CVE ID.
    pub cve_id: String,
    /// Canonical package name.
    pub package: String,
    /// Lowercase severity label used for the dot color and default date.
    pub severity: String,
    /// Server effective triage state, `accepted` or `scheduled`; `None` is OPEN.
    pub triage_state: Option<String>,
}

/// Returns the default patch window in days for the most severe row.
fn default_patch_days(rows: &[SystemBatchRow]) -> i64 {
    if rows.iter().any(|row| row.severity == "critical") {
        14
    } else if rows.iter().any(|row| row.severity == "high") {
        30
    } else {
        56
    }
}

/// Applies one decision to the selected pairs on one host.
///
/// The request writes host overrides only. Rows whose effective state is
/// `scheduled` are always skipped by the server; the dialog says so before
/// submission instead of guessing the exact server outcome for other rows.
#[component]
pub(crate) fn SystemCveBatchTriageDialog(
    system_id: Uuid,
    hostname: String,
    rows: Vec<SystemBatchRow>,
    on_close: EventHandler<()>,
    on_success: EventHandler<poam_api::FleetCveBatchTriageResponse>,
) -> Element {
    let default_date =
        (Utc::now().date_naive() + Duration::days(default_patch_days(&rows))).to_string();
    let mut catalog = use_signal(|| None::<Result<poam_api::PoamAssigneeCatalog, String>>);
    let mut action = use_signal(|| "accepted");
    let mut skip_existing = use_signal(|| true);
    let mut grouping = use_signal(|| poam_api::CveBatchScheduleGrouping::One);
    let mut assignee = use_signal(String::new);
    let mut target_date = use_signal(move || default_date);
    let mut plan = use_signal(String::new);
    let mut milestones = use_signal(|| true);
    let mut justification = use_signal(String::new);
    let mut review_date = use_signal(String::new);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let component_active = use_hook(|| Rc::new(Cell::new(true)));
    {
        let component_active = component_active.clone();
        use_drop(move || component_active.set(false));
    }
    use_effect({
        let component_active = component_active.clone();
        move || {
            let component_active = component_active.clone();
            spawn(async move {
                let result = poam_api::fetch_assignee_catalog()
                    .await
                    .map_err(|err| err.to_string());
                if component_active.get() {
                    catalog.set(Some(result));
                }
            });
        }
    });

    let packages = rows
        .iter()
        .map(|row| row.package.clone())
        .collect::<BTreeSet<_>>();
    let scheduled = rows
        .iter()
        .filter(|row| row.triage_state.as_deref() == Some("scheduled"))
        .count();
    let accepted = rows
        .iter()
        .filter(|row| row.triage_state.as_deref() == Some("accepted"))
        .count();
    let candidates = rows.len() - scheduled;
    let valid_assignee = catalog
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|catalog| batch_assignee(&assignee(), catalog));
    let valid_date = target_date().parse::<chrono::NaiveDate>().ok();
    let valid_review = if review_date().is_empty() {
        Some(None)
    } else {
        review_date().parse::<chrono::NaiveDate>().ok().map(Some)
    };
    let can_apply = !pending()
        && candidates > 0
        && if action() == "accepted" {
            justification().trim().len() >= 10 && valid_review.is_some()
        } else {
            valid_assignee.is_some() && valid_date.is_some()
        };
    let plan_count = match grouping() {
        poam_api::CveBatchScheduleGrouping::PerPackage => packages.len(),
        _ => 1,
    };
    let submit_rows = rows.clone();
    let submit = move |_: MouseEvent| {
        if !can_apply {
            return;
        }
        let disposition = if action() == "accepted" {
            poam_api::CveBatchDisposition::AcceptRisk {
                justification: justification().trim().into(),
                review_date: valid_review.flatten(),
            }
        } else {
            let (Some(assignee), Some(target_date)) = (valid_assignee.clone(), valid_date) else {
                return;
            };
            poam_api::CveBatchDisposition::SchedulePatch {
                grouping: grouping(),
                assignee,
                target_date,
                plan: plan().trim().into(),
                default_milestones: milestones(),
            }
        };
        let request = poam_api::SystemCveBatchTriageRequest {
            pairs: submit_rows
                .iter()
                .map(|row| poam_api::CveBatchPairIdentity {
                    cve_id: row.cve_id.clone(),
                    canonical_package_name: row.package.clone(),
                })
                .collect(),
            skip_existing: skip_existing(),
            disposition,
        };
        pending.set(true);
        error.set(None);
        spawn(async move {
            match poam_api::triage_system_cve_batch(system_id, &request).await {
                Ok(response) => on_success.call(response),
                Err(PoamApiError::Server(server)) if server.status == 409 => {
                    error.set(Some(format!(
                        "Evidence changed: {} Refresh the inventory and retry; no changes were applied.",
                        server.message
                    )));
                }
                Err(err) => error.set(Some(format!("Batch triage was not applied: {err}"))),
            }
            pending.set(false);
        });
    };
    rsx! {
        DialogFocusRestore {}
        DialogInitialFocus { dialog_id: "system-cve-batch-dialog" }
        button { class: "modal-backdrop cve-batch-backdrop", tabindex: "-1", aria_label: "Close batch triage", onclick: move |_| if !pending() { on_close.call(()) } }
        div { id: "system-cve-batch-dialog", class: "modal cve-batch-modal", role: "dialog", aria_modal: "true", aria_label: "Batch triage", tabindex: "-1", "data-testid": "system-cve-batch-dialog",
            onkeydown: move |event| if event.key() == Key::Escape && !pending() { event.stop_propagation(); on_close.call(()); },
            DialogFocusSentinel { dialog_id: "system-cve-batch-dialog", boundary: DialogFocusBoundary::Last }
            div { class: "modal-head cve-batch-head",
                div {
                    h2 { "Triage {rows.len()} CVEs together" }
                    p { "One decision for the group on {hostname} only. Each CVE gets its own host decision, same as triaging it alone. The environment default and other hosts do not change." }
                }
                button { class: "btn-icon focus-ring", aria_label: "Close batch triage", autofocus: true, disabled: pending(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
            }
            div { class: "modal-body cve-batch-body",
                if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                div { class: "cve-batch-list",
                    for package in &packages {
                        div { class: "cve-batch-pkg", strong { class: "mono", "{package}" }
                            div { class: "cve-batch-ids",
                                for row in rows.iter().filter(|row| &row.package == package) {
                                    span { class: if row.triage_state.as_deref() == Some("scheduled") { "cve-batch-id skip" } else { "cve-batch-id" },
                                        span { class: "cve-batch-dot", "data-sev": "{row.severity}" }
                                        span { class: "mono", "{row.cve_id}" }
                                        if let Some(state) = &row.triage_state { small { class: "cve-batch-existing", "{state}" } }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "field", span { "Disposition" }
                    div { class: "seg",
                        for (value, label) in [("accepted", "Accept risk"), ("scheduled", "Schedule patch")] {
                            button { class: if action() == value { "active" } else { "" }, onclick: move |_| action.set(value), "{label}" }
                        }
                    }
                }
                if scheduled > 0 {
                    p { class: "help", role: "status", "{scheduled} already scheduled — always skipped. Change a scheduled decision from its own row." }
                }
                if accepted > 0 {
                    label { class: "poam-check",
                        input { r#type: "checkbox", checked: skip_existing(), onchange: move |event| skip_existing.set(event.checked()) }
                        span { "Skip CVEs already accepted on this host — uncheck to overwrite their decision." }
                    }
                }
                if action() == "accepted" {
                    div { class: "cve-batch-fields cve-batch-waiver",
                        strong { class: "cve-batch-section-title", Icon { name: IconName::Check, size: 12 } " Waiver · {candidates} CVEs" }
                        label { class: "field", span { "Justification · required, applied to every CVE in the group" }
                            textarea { class: "input focus-ring", rows: 2, placeholder: "Why is this acceptable on this host / what is the compensating control?", value: "{justification}", oninput: move |event| justification.set(event.value()) }
                        }
                        label { class: "field cve-batch-review", span { "Review date · optional" }
                            input { class: "input focus-ring", r#type: "date", value: "{review_date}", oninput: move |event| review_date.set(event.value()) }
                        }
                    }
                } else {
                    div { class: "cve-batch-fields cve-batch-poam",
                        strong { class: "cve-batch-section-title", Icon { name: IconName::Plus, size: 12 } " POA&M · {candidates} CVEs" }
                        if packages.len() > 1 {
                            div { class: "field", span { "Group into" }
                                div { class: "seg",
                                    button { class: if grouping() != poam_api::CveBatchScheduleGrouping::PerPackage { "active" } else { "" }, onclick: move |_| grouping.set(poam_api::CveBatchScheduleGrouping::One), "One POA&M" }
                                    button { class: if grouping() == poam_api::CveBatchScheduleGrouping::PerPackage { "active" } else { "" }, onclick: move |_| grouping.set(poam_api::CveBatchScheduleGrouping::PerPackage), "One per package ({packages.len()})" }
                                }
                            }
                        }
                        div { class: "cve-batch-two-col",
                            label { class: "field", span { "Owner" }
                                select { class: "input focus-ring", value: "{assignee}", disabled: catalog.read().is_none(), onchange: move |event| assignee.set(event.value()),
                                    option { value: "", "Select a user or group" }
                                    if let Some(Ok(catalog)) = &*catalog.read() {
                                        optgroup { label: "People", for person in &catalog.people { option { value: "user:{person.user_id}", "{person.label}" } } }
                                        optgroup { label: "Groups", for group in &catalog.groups { option { value: "group:{group.group_name}", "{group.group_name}" } } }
                                    }
                                }
                                if let Some(Err(message)) = &*catalog.read() { small { role: "alert", "Assignees unavailable: {message}" } }
                            }
                            label { class: "field", span { "Target completion" }
                                input { class: "input focus-ring", r#type: "date", value: "{target_date}", oninput: move |event| target_date.set(event.value()) }
                            }
                        }
                        label { class: "field", span { "Remediation plan · optional now, expected before review" }
                            textarea { class: "input focus-ring", rows: 2, placeholder: "Upgrade to patched releases, roll out to this host, and verify the scan clears", value: "{plan}", oninput: move |event| plan.set(event.value()) }
                        }
                        label { class: "poam-check",
                            input { r#type: "checkbox", checked: milestones(), onchange: move |event| milestones.set(event.checked()) }
                            span { "Start from standard patch milestones." }
                        }
                    }
                }
            }
            div { class: "modal-foot",
                span { class: "cve-batch-summary",
                    if candidates == 0 { "Every selected CVE is already scheduled" }
                    else if action() == "scheduled" { "creates {plan_count} POA&M(s) · up to {candidates} CVEs · {hostname}" }
                    else { "up to {candidates} CVEs · {hostname}" }
                }
                button { class: "btn btn-ghost focus-ring", disabled: pending(), onclick: move |_| on_close.call(()), "Cancel" }
                button { class: "btn btn-primary focus-ring", "data-testid": "system-cve-batch-apply", disabled: !can_apply, onclick: submit,
                    Icon { name: IconName::Check, size: 13 }
                    if pending() { "Applying…" } else { " Apply" }
                }
            }
            DialogFocusSentinel { dialog_id: "system-cve-batch-dialog", boundary: DialogFocusBoundary::First }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SystemBatchRow, default_patch_days};

    fn row(severity: &str) -> SystemBatchRow {
        SystemBatchRow {
            cve_id: "CVE-2026-1".into(),
            package: "openssl".into(),
            severity: severity.into(),
            triage_state: None,
        }
    }

    #[test]
    fn default_patch_window_follows_most_severe_row() {
        assert_eq!(default_patch_days(&[row("low"), row("critical")]), 14);
        assert_eq!(default_patch_days(&[row("high"), row("medium")]), 30);
        assert_eq!(default_patch_days(&[row("low")]), 56);
    }
}
