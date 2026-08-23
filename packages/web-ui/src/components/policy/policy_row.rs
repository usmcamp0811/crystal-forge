//! Table-row rendering of a policy definition — the "Table" view mode
//! counterpart to `PolicyCard` (TASK-433 Phase 1 — policy catalog scaling).
//!
//! Preserves the same selection/edit/open semantics as `PolicyCard` so cards
//! and table rows are interchangeable views over identical policy state.

use dioxus::prelude::*;
use uuid::Uuid;

use super::types::{
    PolicyDefinition, is_core_policy, is_policy_enabled, is_policy_version_editable,
    policy_category, policy_rule_summaries,
};

/// A single row in the policy catalog table view.
#[component]
pub fn PolicyRow(
    policy: PolicyDefinition,
    on_open: EventHandler<PolicyDefinition>,
    on_edit: EventHandler<PolicyDefinition>,
    on_delete: EventHandler<Uuid>,
    #[props(default = false)] selection_mode: bool,
    #[props(default = false)] selected: bool,
    #[props(default)] on_toggle_select: EventHandler<bool>,
    /// Fired for a plain or Shift-modified click on the row body (outside the
    /// checkbox/action buttons). The parent inspects `evt.modifiers().shift()`
    /// to decide between a single toggle and a Shift-range selection.
    #[props(default)]
    on_row_click: EventHandler<MouseEvent>,
    #[props(default = false)] highlighted: bool,
) -> Element {
    let rules = policy_rule_summaries(&policy);
    let is_core = is_core_policy(&policy);
    let enabled = is_policy_enabled(&policy);
    let is_editable = is_policy_version_editable(&policy);
    let category = policy_category(&policy);
    let severity_label = policy.severity.as_deref().and_then(|severity| {
        match severity.to_ascii_lowercase().as_str() {
            "high" => Some(("CAT I", "#f87171")),
            "medium" => Some(("CAT II", "#fbbf24")),
            "low" => Some(("CAT III", "#60a5fa")),
            _ => None,
        }
    });
    let type_label = if is_core { "built-in" } else { "custom" };
    let type_chip = if is_core {
        "chip chip-info"
    } else {
        "chip chip-healthy"
    };
    let policy_for_open = policy.clone();
    let policy_for_edit = policy.clone();
    let policy_id = policy.id;

    rsx! {
        tr {
            class: if selected { "selectable selected" } else { "selectable" },
            "data-policy-row": "true",
            "data-policy-id": "{policy.id}",
            "data-policy-name": "{policy.name}",
            style: if highlighted { "box-shadow: inset 2px 0 0 var(--cf-brand-purple);" } else { "" },
            onclick: move |evt| {
                if selection_mode || evt.modifiers().shift() {
                    on_row_click.call(evt);
                } else {
                    on_open.call(policy_for_open.clone());
                }
            },
            td { style: "width:28px;",
                if selection_mode {
                    input {
                        r#type: "checkbox",
                        class: "focus-ring",
                        checked: selected,
                        aria_label: "Select {policy.name}",
                        onclick: move |event| event.stop_propagation(),
                        onchange: move |event| on_toggle_select.call(event.checked()),
                    }
                }
            }
            td {
                div { style: "display:flex;flex-direction:column;gap:2px;min-width:0;",
                    span { class: "mono", style: "font-weight:600;font-size:12.5px;color:{category.color()};", "{policy.name}" }
                    span { style: "font-size:11px;color:var(--cf-text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:360px;", title: "{policy.description}", "{policy.description}" }
                }
            }
            td { span { class: "{type_chip}", "{type_label}" } }
            td {
                if let Some((label, color)) = severity_label {
                    span { class: "chip", style: "font-size:9px;color:{color};background:color-mix(in oklab, {color} 14%, transparent);", "{label}" }
                } else {
                    span { style: "color:var(--cf-text-muted);font-size:11px;", "—" }
                }
            }
            td { class: "mono", style: "text-align:right;", "{policy.mapped_requirement_count}" }
            td { class: "mono", style: "text-align:right;", "{policy.system_count}" }
            td { style: "text-align:right;color:var(--cf-text-muted);font-size:11px;",
                if !enabled { span { class: "chip chip-unknown", "disabled" } }
                else if rules.is_empty() { "no automated rules" } else { "{rules.len()} rule(s)" }
            }
            td { class: "row-actions", onclick: move |evt| evt.stop_propagation(),
                if is_core {
                    span { class: "chip chip-info", "protected" }
                } else if !selection_mode && is_editable {
                    button {
                        class: "btn btn-subtle focus-ring xs",
                        onclick: move |_| on_edit.call(policy_for_edit.clone()),
                        "Edit"
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        style: "color:#f87171;border-color:rgba(248,113,113,0.3);margin-left:6px;",
                        onclick: move |_| on_delete.call(policy_id),
                        "Delete"
                    }
                } else {
                    span { class: "chip chip-unknown", "read-only" }
                }
            }
        }
    }
}
