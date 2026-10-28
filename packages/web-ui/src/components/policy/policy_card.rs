//! Policy card component for displaying policy definitions.

use super::types::{
    PolicyDefinition, is_core_policy, is_policy_enabled, is_policy_version_editable,
    policy_category, policy_rule_summaries,
};
use crate::components::{Icon, IconName};
use dioxus::prelude::*;

const COLLAPSIBLE_DESCRIPTION_CHARS: usize = 120;

fn description_is_expandable(description: &str) -> bool {
    description.chars().count() > COLLAPSIBLE_DESCRIPTION_CHARS || description.lines().count() > 2
}

/// Renders one policy catalog card with enforcement and mapping summaries.
///
/// The parent owns catalog selection, exact-version navigation, and edit
/// authority. The card owns only description expansion and interaction wiring.
#[component]
pub fn PolicyCard(
    policy: PolicyDefinition,
    on_open: EventHandler<PolicyDefinition>,
    on_open_revisions: EventHandler<PolicyDefinition>,
    on_edit: EventHandler<PolicyDefinition>,
    #[props(default = false)] selection_mode: bool,
    #[props(default = false)] selected: bool,
    #[props(default)] on_toggle_select: EventHandler<bool>,
    /// Fired for a plain or Shift-modified click on the card body (outside
    /// the checkbox/action buttons). The parent inspects
    /// `evt.modifiers().shift()` to decide between a single toggle and a
    /// Shift-range selection.
    #[props(default)]
    on_row_click: EventHandler<MouseEvent>,
    #[props(default = false)] highlighted: bool,
) -> Element {
    let mut description_expanded = use_signal(|| false);
    let rules = policy_rule_summaries(&policy);
    let is_core = is_core_policy(&policy);
    let enabled = is_policy_enabled(&policy);
    let is_editable = is_policy_version_editable(&policy);
    let category = policy_category(&policy);
    let rail_color = if enabled { category.color() } else { "#6b7280" };
    let severity_label = policy.severity.as_deref().and_then(|severity| {
        match severity.to_ascii_lowercase().as_str() {
            "high" => Some(("CAT I", "var(--cf-policy-red)")),
            "medium" => Some(("CAT II", "var(--cf-policy-amber)")),
            "low" => Some(("CAT III", "var(--cf-policy-blue)")),
            _ => None,
        }
    });
    let type_label = if is_core { "built-in" } else { "custom" };
    let type_chip = if is_core {
        "chip chip-info"
    } else {
        "chip chip-healthy"
    };
    let opacity = if enabled { "1" } else { "0.72" };
    let policy_for_open = policy.clone();
    let policy_for_keyboard = policy.clone();
    let policy_for_edit = policy.clone();
    let policy_for_revisions = policy.clone();

    rsx! {
        div {
            class: if selected { "sys-card policy-card selected" } else { "sys-card policy-card" },
            role: "group",
            tabindex: "0",
            aria_label: if selection_mode { format!("Select {}", policy.name) } else { format!("Open {}", policy.name) },
            onclick: move |evt| {
                if selection_mode
                    || evt.modifiers().shift()
                    || evt.modifiers().ctrl()
                    || evt.modifiers().meta()
                {
                    on_row_click.call(evt);
                } else {
                    on_open.call(policy_for_open.clone());
                }
            },
            onkeydown: move |event| {
                let key = event.key();
                if key == Key::Enter || matches!(key, Key::Character(ref value) if value == " ") {
                    event.prevent_default();
                    if selection_mode {
                        on_toggle_select.call(!selected);
                    } else {
                        on_open.call(policy_for_keyboard.clone());
                    }
                }
            },
            style: if highlighted {
                format!("--status-color: {rail_color}; opacity: {opacity}; box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--cf-brand-purple) 55%, transparent), 0 0 0 2px color-mix(in oklab, var(--cf-brand-purple) 22%, transparent); background: color-mix(in oklab, var(--cf-brand-purple) 7%, var(--cf-card-bg));")
            } else {
                format!("--status-color: {rail_color}; opacity: {opacity}; cursor: pointer;")
            },
            "data-policy-card": "true",
            "data-policy-id": "{policy.id}",
            "data-policy-name": "{policy.name}",
            div { class: "status-rail" }

            div { class: "sys-card-head",
                div { class: "sys-title",
                    div { class: "sys-hostname",
                        if selection_mode {
                            input {
                                r#type: "checkbox",
                                class: "focus-ring",
                                checked: selected,
                                aria_label: "Select {policy.name}",
                                onclick: move |event| event.stop_propagation(),
                                onkeydown: move |event| event.stop_propagation(),
                                onchange: move |event| on_toggle_select.call(event.checked()),
                            }
                        }
                        svg {
                            width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                            polyline { points: "14 2 14 8 20 8" }
                        }
                        span { "{policy.name}" }
                    }
                    div {
                        class: if description_expanded() { "policy-card-description policy-card-description-expanded" } else { "policy-card-description" },
                        title: "{policy.description}",
                        "{policy.description}"
                    }
                    if description_is_expandable(&policy.description) {
                        button {
                            r#type: "button",
                            class: "policy-card-description-toggle focus-ring",
                            aria_expanded: if description_expanded() { "true" } else { "false" },
                            onclick: move |event| {
                                event.stop_propagation();
                                description_expanded.toggle();
                            },
                            onkeydown: move |event| event.stop_propagation(),
                            if description_expanded() { "Show less" } else { "Show more" }
                        }
                    }
                }
                div { style: "display:flex;flex-direction:column;align-items:flex-end;gap:5px;flex-shrink:0;",
                    span { class: "{type_chip}", "{type_label}" }
                    if policy.revisions.len() > 1 {
                        if let Some(state) = policy.publication_state.as_ref() {
                        span { class: "chip chip-unknown", "{state}" }
                        }
                    }
                    if is_core {
                        span { class: "chip chip-info", "protected" }
                    }
                    if !enabled {
                        span { class: "chip chip-unknown", "disabled" }
                    }
                    if policy.category.as_deref().is_some_and(|category| category.eq_ignore_ascii_case("security")) {
                        if let Some((severity_label, severity_color)) = severity_label {
                            span { class: "chip", style: "font-size:9px;color:{severity_color};background:color-mix(in oklab, {severity_color} 14%, transparent);", "{severity_label}" }
                        }
                    }
                }
            }

            div {
                div { style: "font-size:10px;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);font-weight:600;margin-bottom:6px;", "Enforcement" }
                div { class: "flex flex-col gap-1.5",
                    if rules.is_empty() {
                        div { style: "font-size:11px;font-style:italic;color:var(--cf-text-muted);", "No enforcement defined." }
                    } else {
                        for rule in rules.iter() {
                            div { class: "flex items-start gap-2", style: "font-size:11px;color:var(--cf-text-primary);",
                                svg {
                                    class: "mt-0.5 shrink-0", width: "10", height: "10", view_box: "0 0 24 24", fill: "none", stroke: "var(--cf-policy-emerald)", stroke_width: "3", stroke_linecap: "round", stroke_linejoin: "round",
                                    polyline { points: "20 6 9 17 4 12" }
                                }
                                span { class: "policy-card-rule-summary", title: "{rule.label}", "{rule.label}" }
                            }
                        }
                    }
                }
            }

            if policy.mapped_requirement_count > 0 || policy.bundle_usage_count > 0 {
                div { style: "display:flex;flex-wrap:wrap;gap:5px;",
                    if policy.mapped_requirement_count > 0 {
                        {
                            let req_plural = if policy.mapped_requirement_count == 1 { "" } else { "s" };
                            rsx! {
                                span { class: "chip chip-info", style: "font-size:9.5;",
                                    "{policy.mapped_requirement_count} mapped requirement{req_plural}"
                                }
                            }
                        }
                    }
                    if policy.bundle_usage_count > 0 {
                        {
                            let bundle_plural = if policy.bundle_usage_count == 1 { "" } else { "s" };
                            rsx! {
                                span { class: "chip chip-unknown", style: "font-size:9.5;",
                                    "used by {policy.bundle_usage_count} bundle{bundle_plural}"
                                }
                            }
                        }
                    }
                }
            }

            div { class: "sys-card-foot",
                    div { class: "flex items-center gap-2", style: "font-size:11px;color:var(--cf-text-muted);",
                    svg { width: "11", height: "11", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        rect { x: "3", y: "4", width: "18", height: "8", rx: "2" }
                        rect { x: "3", y: "14", width: "18", height: "6", rx: "2" }
                    }
                    span { class: "mono", style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "{policy.system_count}" }
                    span { "systems use this" }
                }
                if is_core {
                    span { style: "font-size:12px;color:var(--cf-policy-emerald);", "Always on" }
                } else if !selection_mode && is_editable {
                    div { class: "flex items-center gap-2",
                        button {
                            class: "btn btn-subtle focus-ring xs",
                            "data-testid": "policy-card-edit",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                on_edit.call(policy_for_edit.clone());
                            },
                            onkeydown: move |event| event.stop_propagation(),
                            Icon { name: IconName::Gear, size: 12 } "Edit"
                        }
                    }
                } else if !selection_mode {
                    span { class: "chip chip-unknown", "read-only" }
                }
            }
            if !selection_mode && policy.revisions.len() > 1 {
                button {
                    class: "policy-card-revisions focus-ring",
                    onclick: move |event| {
                        event.stop_propagation();
                        on_open_revisions.call(policy_for_revisions.clone());
                    },
                    onkeydown: move |event| event.stop_propagation(),
                    span { "{policy.revisions.len()} revisions" }
                    span { "›" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::description_is_expandable;

    #[test]
    fn descriptions_expand_only_when_the_card_clamp_can_hide_content() {
        assert!(!description_is_expandable("Short policy description."));
        assert!(description_is_expandable(
            "first line\nsecond line\nthird line"
        ));
        assert!(description_is_expandable(&"x".repeat(121)));
    }
}
