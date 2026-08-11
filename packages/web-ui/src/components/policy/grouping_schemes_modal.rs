use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{
    create_compliance_grouping_scheme, delete_compliance_grouping_scheme,
    update_compliance_grouping_scheme,
};
use crate::api::models::{
    ComplianceGroupingScheme, ComplianceGroupingSchemeGroup, ComplianceGroupingSchemeRequest,
};

use super::PolicyDefinition;

#[component]
pub fn GroupingSchemesModal(
    schemes: Vec<ComplianceGroupingScheme>,
    policies: Vec<PolicyDefinition>,
    selected_scheme_id: Option<Uuid>,
    on_close: EventHandler<()>,
    on_select: EventHandler<Option<Uuid>>,
    on_changed: EventHandler<Vec<ComplianceGroupingScheme>>,
) -> Element {
    let initial = selected_scheme_id.and_then(|id| schemes.iter().find(|scheme| scheme.id == id).cloned());
    let mut selected_id = use_signal(|| initial.as_ref().map(|scheme| scheme.id));
    let mut name = use_signal(|| initial.as_ref().map(|scheme| scheme.name.clone()).unwrap_or_default());
    let mut description = use_signal(|| initial.as_ref().and_then(|scheme| scheme.description.clone()).unwrap_or_default());
    let mut groups = use_signal(|| initial.map(|scheme| scheme.groups).unwrap_or_else(|| vec![blank_group(1)]));
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let delete_schemes = schemes.clone();
    let save_schemes = schemes.clone();

    rsx! {
        div { class: "modal-backdrop cf-modal-overlay-z50", onclick: move |_| on_close.call(()),
            div { class: "modal", role: "dialog", "aria-modal": "true", "aria-labelledby": "grouping-schemes-title", style: "width:min(920px,96vw);max-height:92vh;", tabindex: "-1", onclick: |event| event.stop_propagation(), onkeydown: move |event| if event.key() == Key::Escape { on_close.call(()) },
                div { class: "modal-head",
                    div {
                        h2 { id: "grouping-schemes-title", "Manage groupings" }
                        p { "Server-backed grouping schemes for security controls." }
                    }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| on_close.call(()), "Close" }
                }
                div { class: "modal-body", style: "overflow:auto;",
                    div { style: "display:grid;grid-template-columns:minmax(180px,240px) minmax(0,1fr);gap:18px;",
                        div { style: "display:grid;gap:6px;align-content:start;",
                            button { class: "btn btn-ghost xs focus-ring", onclick: move |_| { selected_id.set(None); name.set(String::new()); description.set(String::new()); groups.set(vec![blank_group(1)]); error.set(None); }, "New scheme" }
                            for scheme in schemes.iter().cloned() {
                                {
                                    let id = scheme.id;
                                    let active = selected_id() == Some(id);
                                    rsx! { button { class: if active { "policy-picker-row focus-ring" } else { "policy-picker-row focus-ring" }, style: if active { "border-color:var(--cf-accent);" } else { "" }, onclick: move |_| { selected_id.set(Some(scheme.id)); name.set(scheme.name.clone()); description.set(scheme.description.clone().unwrap_or_default()); groups.set(scheme.groups.clone()); error.set(None); }, "{scheme.name}" } }
                                }
                            }
                        }
                        div { style: "min-width:0;",
                            label { class: "form-field", "Scheme name", input { class: "input focus-ring", value: "{name}", oninput: move |event| name.set(event.value()) } }
                            label { class: "form-field", "Description", textarea { class: "input focus-ring", value: "{description}", oninput: move |event| description.set(event.value()) } }
                            h3 { class: "modal-section-title", "Groups" }
                            for (index, group) in groups.read().iter().cloned().enumerate() {
                                div { key: "{group.id}", class: "policy-picker-row", style: "display:grid;gap:8px;align-items:stretch;",
                                    div { style: "display:flex;gap:8px;justify-content:space-between;", strong { "Group {index + 1}" }, button { class: "btn btn-ghost xs focus-ring", onclick: move |_| { groups.write().remove(index); }, "Remove" } }
                                    label { class: "form-field", "Group ID", input { class: "input focus-ring mono", value: "{group.id}", oninput: move |event| groups.write()[index].id = event.value() } }
                                    label { class: "form-field", "Name", input { class: "input focus-ring", value: "{group.name}", oninput: move |event| groups.write()[index].name = event.value() } }
                                    label { class: "form-field", "Description", input { class: "input focus-ring", value: "{group.description.clone().unwrap_or_default()}", oninput: move |event| groups.write()[index].description = nonempty(event.value()) } }
                                    label { class: "form-field", "Match text", input { class: "input focus-ring", placeholder: "Matches name, metadata, CCI, SRG, severity, or CIS section", value: "{group.query}", oninput: move |event| groups.write()[index].query = event.value() } }
                                    details { summary { "Pinned and excluded policy lineages" }, div { class: "policy-picker", style: "margin-top:8px;",
                                        for policy in policies.iter().filter(|policy| policy.category.as_deref().is_some_and(|category| category.eq_ignore_ascii_case("security"))).cloned() {
                                            {
                                                let policy_id = policy.id;
                                                let pinned = group.pinned_policy_ids.contains(&policy_id);
                                                let excluded = group.excluded_policy_ids.contains(&policy_id);
                                                rsx! { div { class: "policy-picker-row", label { input { r#type: "checkbox", checked: pinned, disabled: excluded, onchange: move |event| toggle_id(&mut groups.write()[index].pinned_policy_ids, policy_id, event.checked()) }, " Pin {policy.name}" } label { input { r#type: "checkbox", checked: excluded, onchange: move |event| toggle_id(&mut groups.write()[index].excluded_policy_ids, policy_id, event.checked()) }, " Exclude" } } }
                                            }
                                        }
                                    } }
                                }
                            }
                            button { class: "btn btn-ghost xs focus-ring", onclick: move |_| { let next = groups.read().len() + 1; groups.write().push(blank_group(next)); }, "Add group" }
                        }
                    }
                    if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                }
                div { class: "modal-foot",
                    if selected_id().is_some() { button { class: "btn btn-ghost xs focus-ring", disabled: busy(), onclick: move |_| { let id = selected_id().unwrap(); let mut error = error; let mut busy = busy; let mut selected_id = selected_id; let mut schemes = delete_schemes.clone(); spawn(async move { busy.set(true); match delete_compliance_grouping_scheme(&id).await { Ok(()) => { schemes.retain(|scheme| scheme.id != id); on_changed.call(schemes); on_select.call(None); selected_id.set(None); busy.set(false); }, Err(err) => { error.set(Some(err.to_string())); busy.set(false); } } }); }, "Delete" } }
                    button { class: "btn btn-primary focus-ring", disabled: busy(), onclick: move |_| { let request = ComplianceGroupingSchemeRequest { name: name(), description: nonempty(description()), groups: groups() }; let id = selected_id(); let mut error = error; let mut busy = busy; let mut selected_id = selected_id; let mut schemes = save_schemes.clone(); spawn(async move { busy.set(true); let result = match id { Some(id) => update_compliance_grouping_scheme(&id, &request).await, None => create_compliance_grouping_scheme(&request).await }; match result { Ok(scheme) => { if let Some(index) = schemes.iter().position(|value| value.id == scheme.id) { schemes[index] = scheme.clone(); } else { schemes.push(scheme.clone()); } selected_id.set(Some(scheme.id)); on_select.call(Some(scheme.id)); on_changed.call(schemes); busy.set(false); }, Err(err) => { error.set(Some(err.to_string())); busy.set(false); } } }); }, if busy() { "Saving..." } else { "Save scheme" } }
                }
            }
        }
    }
}

fn blank_group(number: usize) -> ComplianceGroupingSchemeGroup {
    ComplianceGroupingSchemeGroup { id: format!("group-{number}"), name: format!("Group {number}"), description: None, query: String::new(), pinned_policy_ids: Vec::new(), excluded_policy_ids: Vec::new() }
}

fn nonempty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn toggle_id(ids: &mut Vec<Uuid>, id: Uuid, checked: bool) {
    if checked && !ids.contains(&id) { ids.push(id); }
    if !checked { ids.retain(|value| *value != id); }
}
