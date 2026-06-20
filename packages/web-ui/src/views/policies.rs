//! Policies view — global policy management for deployment rules.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::delete_deployment_policy;
use crate::components::layout::Card;
use crate::components::policy::{
    POLICY_CATEGORIES, PolicyCard, PolicyCategory, PolicyDefinition, PolicyEditorModal,
    PolicyFormat, is_core_policy, policy_category,
};
use crate::theme;
use crate::views::policies_api;

const POLICY_JSON_TEMPLATE: &str = r#"{
  "policy_type": "custom_check",
  "config": {
    "expression": "config.networking.firewall.enable",
    "description": "Firewall must be enabled",
    "strict": true
  }
}"#;

/// The policies page for global policy management.
#[component]
pub fn PoliciesView() -> Element {
    let mut policy_library: Signal<Vec<PolicyDefinition>> = use_signal(Vec::new);
    let mut show_editor = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            let policies = policies_api::load_policies_with_fallback().await;
            policy_library.set(policies);
        });
    });

    let mut editing_policy_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_body = use_signal(String::new);
    let mut edit_format = use_signal(|| PolicyFormat::Json);
    let mut search_query = use_signal(String::new);
    let mut category_filter = use_signal(|| "all".to_string());
    let mut type_filter = use_signal(|| "all".to_string());
    let mut delete_confirm: Signal<Option<Uuid>> = use_signal(|| None);

    let query = search_query.read().to_lowercase();
    let selected_category = category_filter.read().clone();
    let selected_type = type_filter.read().clone();
    let all_policies = policy_library.read().clone();
    let policy_count = all_policies.len();
    let built_in_count = all_policies
        .iter()
        .filter(|policy| is_core_policy(policy))
        .count();
    let custom_count = policy_count.saturating_sub(built_in_count);

    let filtered_policies = all_policies
        .iter()
        .filter(|policy| policy_matches_filters(policy, &query, &selected_category, &selected_type))
        .cloned()
        .collect::<Vec<_>>();
    let filtered_count = filtered_policies.len();
    let filtered_label = if filtered_count == 1 {
        "policy"
    } else {
        "policies"
    };
    let has_filters = selected_category != "all" || selected_type != "all" || !query.is_empty();
    let category_counts = category_counts(&all_policies);
    let grouped_policies = grouped_policies(&filtered_policies);

    rsx! {
        div { class: "space-y-4",
            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Policies" }
                    p { class: "page-subtitle",
                        "Criteria a system must satisfy to deploy · {built_in_count} built-in · {custom_count} custom"
                    }
                }
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| {
                        editing_policy_id.set(None);
                        edit_name.set(String::new());
                        edit_description.set(String::new());
                        edit_body.set(POLICY_JSON_TEMPLATE.to_string());
                        edit_format.set(PolicyFormat::Json);
                        show_editor.set(true);
                    },
                    svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M12 5v14M5 12h14" }
                    }
                    " New custom policy"
                }
            }

            div { class: "stat-strip pol-cat-strip",
                for (category, count) in category_counts.iter().copied() {
                    button {
                        key: "{category.id()}",
                        class: "stat pol-cat-stat focus-ring",
                        title: "{category.blurb()}",
                        style: if selected_category == category.id() {
                            "background: color-mix(in oklab, {category.color()} 14%, transparent); box-shadow: inset 0 0 0 1px color-mix(in oklab, {category.color()} 45%, transparent);"
                        } else {
                            ""
                        },
                        onclick: move |_| {
                            if category_filter.read().as_str() == category.id() {
                                category_filter.set("all".to_string());
                            } else {
                                category_filter.set(category.id().to_string());
                            }
                        },
                        span { class: "stat-accent", style: "--stat-color: {category.color()};" }
                        div { class: "stat-label", "{category.label()}" }
                        div { class: "stat-value", style: "color: {category.color()};", "{count}" }
                        div { class: "stat-meta", "{category.blurb()}" }
                    }
                }
            }

            div { class: "filterbar",
                div { class: "filter-search", style: "max-width:280px;",
                    svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.3-4.3" }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search policies…",
                        value: "{search_query}",
                        oninput: move |event| search_query.set(event.value()),
                    }
                }

                div { class: "seg",
                    button {
                        class: if selected_category == "all" { "active" } else { "" },
                        onclick: move |_| category_filter.set("all".to_string()),
                        "all"
                    }
                    for category in POLICY_CATEGORIES.iter().copied() {
                        button {
                            key: "seg-{category.id()}",
                            class: if selected_category == category.id() { "active" } else { "" },
                            title: "{category.blurb()}",
                            onclick: move |_| category_filter.set(category.id().to_string()),
                            span { style: "display:inline-flex;align-items:center;gap:5px;",
                                span { style: "width:6px;height:6px;border-radius:50%;background:{category.color()};flex-shrink:0;" }
                                "{category.short_label()}"
                            }
                        }
                    }
                }

                select {
                    class: "input focus-ring filter-select",
                    style: "width:auto;font-size:12px;padding:6px 28px 6px 10px;",
                    value: "{type_filter}",
                    onchange: move |event| type_filter.set(event.value()),
                    option { value: "all", "All types" }
                    option { value: "builtin", "Built-in" }
                    option { value: "custom", "Custom" }
                }

                if has_filters {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        onclick: move |_| {
                            category_filter.set("all".to_string());
                            type_filter.set("all".to_string());
                            search_query.set(String::new());
                        },
                        "Clear"
                    }
                }

                span { class: "filter-count", "{filtered_count} {filtered_label}" }
            }

            if grouped_policies.is_empty() {
                Card {
                    children: rsx! {
                        div { class: "text-center py-12",
                            if has_filters {
                                svg { width: "20", height: "20", class: "mx-auto text-gray-600 mb-2", style: "opacity:0.5;", fill: "none", stroke: "currentColor", stroke_width: "2", view_box: "0 0 24 24",
                                    circle { cx: "11", cy: "11", r: "8" }
                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "m21 21-4.35-4.35" }
                                }
                                p { class: "text-gray-400 mb-2", "No policies match these filters." }
                                p { class: "text-sm text-gray-500", "Clear the filters or try a different search." }
                            } else {
                                svg { class: "w-12 h-12 mx-auto text-gray-600 mb-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1.5", d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                                }
                                p { class: "text-gray-400 mb-2", "No policies yet" }
                                p { class: "text-sm text-gray-500", "Create your first custom policy to get started." }
                            }
                        }
                    }
                }
            } else {
                for (category, items) in grouped_policies.iter() {
                    section { class: "pol-group space-y-3",
                        div { class: "pol-group-head flex items-start gap-3",
                            span { class: "pol-group-icon", style: "width:28px;height:28px;border-radius:8px;display:grid;place-items:center;background:color-mix(in oklab, {category.color()} 16%, transparent);color:{category.color()};",
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M9 12l2 2 4-4" }
                                    path { d: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                                }
                            }
                            div { style: "min-width:0;",
                                h2 { class: "text-sm font-semibold text-white", "{category.label()} " span { class: "text-xs text-gray-500", "{items.len()}" } }
                                div { class: "text-xs text-gray-500", "{category.blurb()}" }
                            }
                        }

                        div { class: "cards-grid",
                            for policy in items.iter().cloned() {
                                PolicyCard {
                                    key: "{policy.id}",
                                    policy: policy.clone(),
                                    on_edit: move |p: PolicyDefinition| {
                                        editing_policy_id.set(Some(p.id));
                                        edit_name.set(p.name.clone());
                                        edit_description.set(p.description.clone());
                                        edit_body.set(p.body.clone());
                                        edit_format.set(p.format);
                                        show_editor.set(true);
                                    },
                                    on_delete: move |id: Uuid| {
                                        delete_confirm.set(Some(id));
                                    },
                                }
                            }
                        }
                    }
                }
            }

            if *show_editor.read() {
                PolicyEditorModal {
                    editing_policy_id: editing_policy_id.clone(),
                    edit_name: edit_name.clone(),
                    edit_description: edit_description.clone(),
                    edit_body: edit_body.clone(),
                    edit_format: edit_format.clone(),
                    policy_library: policy_library.clone(),
                    on_close: move || show_editor.set(false),
                }
            }

            if let Some(id) = *delete_confirm.read() {
                DeleteConfirmModal {
                    policy_id: id,
                    policy_name: policy_library.read().iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default(),
                    on_confirm: move |_| {
                        let mut policy_library = policy_library;
                        let mut delete_confirm = delete_confirm;
                        spawn(async move {
                            match delete_deployment_policy(&id).await {
                                Ok(()) => {
                                    let latest = policies_api::load_policies_with_fallback().await;
                                    policy_library.set(latest);
                                }
                                Err(error) => {
                                    web_sys::console::error_1(&format!("Failed to delete policy: {error}").into());
                                }
                            }
                            delete_confirm.set(None);
                        });
                    },
                    on_cancel: move |_| delete_confirm.set(None),
                }
            }
        }
    }
}

fn policy_matches_filters(
    policy: &PolicyDefinition,
    query: &str,
    selected_category: &str,
    selected_type: &str,
) -> bool {
    if selected_category != "all" && policy_category(policy).id() != selected_category {
        return false;
    }

    if selected_type == "builtin" && !is_core_policy(policy) {
        return false;
    }

    if selected_type == "custom" && is_core_policy(policy) {
        return false;
    }

    query.trim().is_empty()
        || policy.name.to_lowercase().contains(query)
        || policy.description.to_lowercase().contains(query)
}

fn category_counts(policies: &[PolicyDefinition]) -> Vec<(PolicyCategory, usize)> {
    POLICY_CATEGORIES
        .iter()
        .copied()
        .map(|category| {
            let count = policies
                .iter()
                .filter(|policy| policy_category(policy) == category)
                .count();
            (category, count)
        })
        .collect()
}

fn grouped_policies(policies: &[PolicyDefinition]) -> Vec<(PolicyCategory, Vec<PolicyDefinition>)> {
    POLICY_CATEGORIES
        .iter()
        .copied()
        .filter_map(|category| {
            let items = policies
                .iter()
                .filter(|policy| policy_category(policy) == category)
                .cloned()
                .collect::<Vec<_>>();
            if items.is_empty() {
                None
            } else {
                Some((category, items))
            }
        })
        .collect()
}

#[component]
fn DeleteConfirmModal(
    policy_id: Uuid,
    policy_name: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let _ = policy_id;

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-28",
                onclick: |evt| evt.stop_propagation(),
                div { class: "flex justify-center mb-4",
                    div { class: "w-12 h-12 rounded-full bg-red-500/20 flex items-center justify-center",
                        svg { class: "w-6 h-6 text-red-400", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" }
                        }
                    }
                }
                h3 { class: "text-lg font-semibold text-white text-center mb-2", "Delete Policy?" }
                p { class: "text-sm {theme::text::SECONDARY} text-center mb-6",
                    "Are you sure you want to delete "
                    span { class: "font-medium text-white", "{policy_name}" }
                    "? This action cannot be undone."
                }
                div { class: "flex gap-3",
                    button { class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white", onclick: move |_| on_cancel.call(()), "Cancel" }
                    button { class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white", onclick: move |_| on_confirm.call(()), "Delete" }
                }
            }
        }
    }
}
