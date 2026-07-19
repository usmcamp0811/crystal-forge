//! Policy card component for displaying policy definitions.

use dioxus::prelude::*;
use uuid::Uuid;

use super::types::{
    is_core_policy, is_policy_enabled, normalized_policy_type, policy_category,
    policy_rule_summaries, PolicyDefinition,
};

/// Card component for displaying a policy definition with design-parity rule summaries.
#[component]
pub fn PolicyCard(
    policy: PolicyDefinition,
    on_edit: EventHandler<PolicyDefinition>,
    on_delete: EventHandler<Uuid>,
    #[props(default = false)] highlighted: bool,
) -> Element {
    let category = policy_category(&policy);
    let rules = policy_rule_summaries(&policy);
    let is_core = is_core_policy(&policy);
    let enabled = is_policy_enabled(&policy);
    let policy_type = normalized_policy_type(&policy);
    let type_label = if is_core { "built-in" } else { "custom" };
    let type_chip = if is_core {
        "chip chip-info"
    } else {
        "chip chip-healthy"
    };
    let category_color = category.color();
    let opacity = if enabled { "1" } else { "0.72" };
    let policy_for_edit = policy.clone();
    let policy_id = policy.id;

    rsx! {
        div {
            class: "sys-card",
            style: if highlighted {
                format!(
                    "--status-color: {category_color}; opacity: {opacity}; box-shadow: inset 0 0 0 1px color-mix(in oklab, {category_color} 55%, transparent), 0 0 0 2px color-mix(in oklab, {category_color} 22%, transparent); background: color-mix(in oklab, {category_color} 7%, var(--cf-card-bg));"
                )
            } else {
                format!("--status-color: {category_color}; opacity: {opacity};")
            },
            "data-policy-card": "true",
            "data-policy-name": "{policy.name}",
            div { class: "status-rail" }

            div { class: "sys-card-head",
                div { class: "sys-title",
                    div { class: "sys-hostname",
                        svg {
                            width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                            polyline { points: "14 2 14 8 20 8" }
                        }
                        span { "{policy.name}" }
                    }
                    div { class: "text-[11px] text-gray-400 line-clamp-2", "{policy.description}" }
                }
                div { style: "display:flex;flex-direction:column;align-items:flex-end;gap:5px;flex-shrink:0;",
                    span { class: "chip", style: "color: {category_color}; background: color-mix(in oklab, {category_color} 14%, transparent);", "{category.short_label()}" }
                    span { class: "{type_chip}", "{type_label}" }
                    if policy_type == "require_cve_check" {
                        span { class: "chip", style: "color:#fbbf24;background:rgba(251,191,36,0.14);", "CVE gate" }
                    }
                    if is_core {
                        span { class: "chip chip-info", "protected" }
                    }
                    if !enabled {
                        span { class: "chip chip-unknown", "disabled" }
                    }
                }
            }

            div {
                div { class: "sys-kv-key", "Rules" }
                div { class: "flex flex-col gap-1.5",
                    if rules.is_empty() {
                        div { class: "text-[11px] italic text-gray-500", "No automated rules — operator approves directly." }
                    } else {
                        for rule in rules.iter().take(4) {
                            div { class: "flex items-start gap-2 text-[11px] text-gray-200",
                                svg {
                                    class: "mt-0.5 shrink-0", width: "10", height: "10", view_box: "0 0 24 24", fill: "none", stroke: "#34d399", stroke_width: "3", stroke_linecap: "round", stroke_linejoin: "round",
                                    polyline { points: "20 6 9 17 4 12" }
                                }
                                span { "{rule.label}" }
                            }
                        }
                    }
                    if rules.len() > 4 {
                        div { class: "text-[11px] text-gray-500", "+ {rules.len() - 4} more rules" }
                    }
                }
            }

            div { class: "sys-card-foot",
                div { class: "flex items-center gap-2 text-[11px] text-gray-500",
                    svg { width: "11", height: "11", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        rect { x: "3", y: "4", width: "18", height: "8", rx: "2" }
                        rect { x: "3", y: "14", width: "18", height: "6", rx: "2" }
                    }
                    span { class: "mono font-semibold text-gray-300", "0" }
                    span { "systems use this" }
                }
                if is_core {
                    span { class: "text-xs text-emerald-300", "Always on" }
                } else {
                    div { class: "flex items-center gap-2",
                        button {
                            class: "btn btn-subtle focus-ring xs",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_edit.call(policy_for_edit.clone());
                            },
                            "Edit"
                        }
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            style: "color:#f87171;border-color:rgba(248,113,113,0.3);",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_delete.call(policy_id);
                            },
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}
