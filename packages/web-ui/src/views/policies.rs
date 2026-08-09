//! Policies view — global policy management for deployment rules.

use dioxus::prelude::*;
use std::collections::HashSet;
use uuid::Uuid;

use crate::api::client::{delete_deployment_policy, export_policy_versions};
use crate::components::io_menu::{IOMenu, IOMenuItem};
use crate::components::layout::Card;
use crate::components::policy::{
    POLICY_CATEGORIES, PolicyCard, PolicyCategory, PolicyDefinition, PolicyEditorModal,
    PolicyFormat, is_core_policy, is_policy_version_editable, normalized_policy_type,
    policy_category,
};
use crate::state::navigation_focus::{FocusTarget, NavigationFocus};
use crate::state::{app_state::AppState, auth};
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
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();
    let mut policy_library: Signal<Vec<PolicyDefinition>> = use_signal(Vec::new);
    let mut policies_load_error: Signal<Option<String>> = use_signal(|| None);
    let mut show_editor = use_signal(|| false);
    let mut show_import = use_signal(|| false);
    let mut drawer_policy = use_signal(|| None::<PolicyDefinition>);
    let mut drawer_revisions = use_signal(|| false);

    use_effect(move || {
        spawn(async move {
            match policies_api::load_policies().await {
                policies_api::PolicyLoadResult::Ok(p) => {
                    policies_load_error.set(None);
                    policy_library.set(p);
                }
                policies_api::PolicyLoadResult::Err(e) => {
                    policies_load_error.set(Some(e));
                }
            }
        });
    });

    let mut editing_policy_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_body = use_signal(String::new);
    let mut edit_format = use_signal(|| PolicyFormat::Json);
    // SRG/CCI mapping fields — seeded from the current policy on edit,
    // blank on create. These are persisted to compliance_metadata.
    let mut edit_srg_ids = use_signal(String::new);
    let mut edit_cci_ids = use_signal(String::new);
    let mut search_query = use_signal(String::new);
    let mut category_filter = use_signal(|| "all".to_string());
    let mut type_filter = use_signal(|| "all".to_string());
    let mut delete_confirm: Signal<Option<Uuid>> = use_signal(|| None);
    let mut delete_busy = use_signal(|| false);
    let mut delete_error: Signal<Option<String>> = use_signal(|| None);
    let mut focused_policy_name = use_signal(|| None::<String>);
    let mut policies_loaded = use_signal(|| false);
    let mut pending_policy_focus = use_signal(|| None::<NavigationFocus>);
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let mut selection_mode = use_signal(|| false);
    let mut selected_policy_ids = use_signal(HashSet::<Uuid>::new);
    let mut export_error = use_signal(|| None::<String>);

    let export_single_policy = {
        let mut export_error = export_error;
        move |(policy, format): (PolicyDefinition, String)| {
            let Some(version_id) = policy.version_id else {
                export_error.set(Some(
                    "This policy has no portable version available to export".to_string(),
                ));
                return;
            };
            spawn(async move {
                match export_policy_versions(&[version_id], &format).await {
                    Ok(body) => {
                        let filename = format!("{}.{}", sanitize_filename(&policy.name), format);
                        if let Err(error) = crate::export::trigger_download(
                            &filename,
                            if format == "json" {
                                "application/json"
                            } else {
                                "application/toml"
                            },
                            &body,
                        ) {
                            export_error.set(Some(error));
                        }
                    }
                    Err(error) => export_error.set(Some(error.to_string())),
                }
            });
        }
    };

    // Track when policies finish loading, so we can retry the focus match.
    use_effect(move || {
        let snapshot_empty = policy_library.read().is_empty();
        if !snapshot_empty && !policies_loaded() {
            policies_loaded.set(true);
        }
        // If there's a pending focus and policies are now loaded, re-fire the focus effect.
        if policies_loaded() {
            let pending = pending_policy_focus.read().clone();
            if let Some(pf) = pending {
                navigation_focus.set(Some(pf));
                pending_policy_focus.set(None);
            }
        }
    });

    // Normalize matrix-API identifiers to canonical policy types.  The eval
    // policy matrix returns internal keys such as "cf.agent_enabled" which do
    // not match any policy_type, display name, or normalized form.
    fn canonical_policy_type(name: &str) -> &str {
        match name {
            "cf.agent_enabled" => "require_cf_agent",
            other => other,
        }
    }

    use_effect(move || {
        let Some(focus) = navigation_focus() else {
            return;
        };
        if focus.target != FocusTarget::Policies {
            return;
        }

        let Some(raw_name) = focus.policy_name.clone() else {
            navigation_focus.set(None);
            return;
        };

        let policy_name = canonical_policy_type(&raw_name).to_string();
        let policy_snapshot = policy_library.read();
        let loaded = policies_loaded();
        let search_name = policy_name.to_ascii_lowercase();
        let matched = policy_snapshot.iter().find(|policy| {
            // Match against display name.
            if policy.name.eq_ignore_ascii_case(&policy_name)
                || policy.name.to_ascii_lowercase().contains(&search_name)
            {
                return true;
            }
            // Match against the policy_type field (e.g. "require_crystal_forge_agent").
            if let Some(ref pt) = policy.policy_type {
                if pt.eq_ignore_ascii_case(&policy_name)
                    || pt.to_ascii_lowercase().contains(&search_name)
                {
                    return true;
                }
            }
            // Match against the normalized policy type.
            if normalized_policy_type(policy)
                .to_ascii_lowercase()
                .contains(&search_name)
            {
                return true;
            }
            false
        });

        if let Some(policy) = matched {
            category_filter.set("all".to_string());
            type_filter.set("all".to_string());
            search_query.set(String::new());
            focused_policy_name.set(Some(policy.name.clone()));
            drawer_policy.set(Some(policy.clone()));

            #[cfg(target_arch = "wasm32")]
            {
                let target_name = policy.name.clone();
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(10).await;
                    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                        let selector = format!(
                            r#"[data-policy-name=\"{}\"]"#,
                            target_name.replace('"', "\\\"")
                        );
                        if let Ok(Some(element)) = document.query_selector(&selector) {
                            element.scroll_into_view();
                        }
                    }
                });
            }
            navigation_focus.set(None);
        } else if loaded {
            // Policies are loaded but no match found — genuinely missing.
            navigation_focus.set(None);
        } else {
            // Policies not yet loaded — save focus and retry after load.
            pending_policy_focus.set(Some(focus));
            navigation_focus.set(None);
        }
    });

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
    let mut selected_version_ids: Vec<Uuid> = selected_policy_ids
        .read()
        .iter()
        .filter_map(|id| all_policies.iter().find(|policy| policy.id == *id))
        .filter_map(|policy| policy.version_id)
        .collect();
    selected_version_ids.sort();
    let mut custom_version_ids: Vec<Uuid> = all_policies
        .iter()
        .filter(|policy| !is_core_policy(policy))
        .filter_map(|policy| policy.version_id)
        .collect();
    custom_version_ids.sort();

    rsx! {
        div { class: "space-y-4",
            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Policies" }
                    p { class: "page-subtitle",
                        "Criteria a system must satisfy to deploy · {built_in_count} built-in · {custom_count} custom"
                    }
                }
                div { style: "display:flex; gap:8px; align-items:center;",
                    // Shared Import / Export menu (AC #32, #35)
                    IOMenu {
                        trigger_label: "Import / Export".to_string(),
                        trigger_class: "focus-ring".to_string(),
                        id: "policies-io".to_string(),
                        items: vec![
                            if is_admin_user {
                                IOMenuItem::action("Import policies…")
                            } else {
                                IOMenuItem::disabled("Import policies…", "Administrator permission required")
                            },
                            IOMenuItem::Separator,
                            if custom_version_ids.is_empty() {
                                IOMenuItem::disabled("Export all custom policies (JSON)", "No exportable custom policies")
                            } else {
                                IOMenuItem::action("Export all custom policies (JSON)")
                            },
                            if custom_version_ids.is_empty() {
                                IOMenuItem::disabled("Export all custom policies (TOML)", "No exportable custom policies")
                            } else {
                                IOMenuItem::action("Export all custom policies (TOML)")
                            },
                            IOMenuItem::Separator,
                            IOMenuItem::action("Select policies to export"),
                            if selected_version_ids.is_empty() {
                                IOMenuItem::disabled("Export selected policies (JSON)", "Select at least one policy")
                            } else {
                                IOMenuItem::action("Export selected policies (JSON)")
                            },
                            if selected_version_ids.is_empty() {
                                IOMenuItem::disabled("Export selected policies (TOML)", "Select at least one policy")
                            } else {
                                IOMenuItem::action("Export selected policies (TOML)")
                            },
                        ],
                        on_action: move |idx: usize| {
                            let ids = if idx == 1 || idx == 2 {
                                custom_version_ids.clone()
                            } else {
                                selected_version_ids.clone()
                            };
                            match idx {
                                0 => show_import.set(true),
                                1 | 4 => {
                                    let mut export_error = export_error;
                                    let mut selected_policy_ids = selected_policy_ids;
                                    spawn(async move {
                                        match export_policy_versions(&ids, "json").await {
                                            Ok(body) => {
                                                let filename = if ids.len() == 1 { "policy.json" } else { "policies.json" };
                                                if let Err(error) = crate::export::trigger_download(filename, "application/json", &body) {
                                                    export_error.set(Some(error));
                                                } else {
                                                    selected_policy_ids.clear();
                                                    selection_mode.set(false);
                                                }
                                            }
                                            Err(error) => export_error.set(Some(error.to_string())),
                                        }
                                    });
                                }
                                2 | 5 => {
                                    let mut export_error = export_error;
                                    let mut selected_policy_ids = selected_policy_ids;
                                    spawn(async move {
                                        match export_policy_versions(&ids, "toml").await {
                                            Ok(body) => {
                                                let filename = if ids.len() == 1 { "policy.toml" } else { "policies.toml" };
                                                if let Err(error) = crate::export::trigger_download(filename, "application/toml", &body) {
                                                    export_error.set(Some(error));
                                                } else {
                                                    selected_policy_ids.clear();
                                                    selection_mode.set(false);
                                                }
                                            }
                                            Err(error) => export_error.set(Some(error.to_string())),
                                        }
                                    });
                                }
                                3 => selection_mode.set(true),
                                _ => {}
                            }
                        },
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| {
                            editing_policy_id.set(None);
                            edit_name.set(String::new());
                            edit_description.set(String::new());
                            edit_body.set(POLICY_JSON_TEMPLATE.to_string());
                            edit_format.set(PolicyFormat::Json);
                            edit_srg_ids.set(String::new());
                            edit_cci_ids.set(String::new());
                            show_editor.set(true);
                        },
                        svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            path { d: "M12 5v14M5 12h14" }
                        }
                        " New custom policy"
                    }
                }
            }

            // Show an explicit error when the policy API fails (AC #34).
            if let Some(ref err) = *policies_load_error.read() {
                div { class: "sd-callout sd-callout-danger",
                    "Failed to load policies: {err}"
                }
            }
            if let Some(ref err) = *export_error.read() {
                div { class: "sd-callout sd-callout-danger", role: "alert",
                    "Policy export failed: {err}"
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| export_error.set(None), "Dismiss" }
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

            if selection_mode() {
                div { class: "sd-callout sd-callout-info", role: "status",
                    span { "Export selection mode: {selected_policy_ids.read().len()} selected" }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| {
                            selection_mode.set(false);
                            selected_policy_ids.clear();
                        },
                        "Done"
                    }
                }
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
                            } else if policies_load_error.read().is_some() {
                                p { class: "text-gray-400 mb-2", "Policy data is unavailable." }
                                p { class: "text-sm text-gray-500", "Resolve the management API error above and retry." }
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
                    section { class: "pol-group",
                        div { class: "pol-group-head",
                            span { class: "pol-group-icon", style: "background:color-mix(in oklab, {category.color()} 16%, transparent);color:{category.color()};",
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M9 12l2 2 4-4" }
                                    path { d: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                                }
                            }
                            div { style: "min-width:0;",
                                h2 { class: "pol-group-title", "{category.label()} " span { class: "pol-group-count", "{items.len()}" } }
                                div { class: "pol-group-blurb", "{category.blurb()}" }
                            }
                        }

                        div { class: "cards-grid",
                            for policy in items.iter().cloned() {
                                PolicyCard {
                                    key: "{policy.id}",
                                    policy: policy.clone(),
                                    on_open: move |p: PolicyDefinition| {
                                        drawer_revisions.set(false);
                                        drawer_policy.set(Some(p));
                                    },
                                    on_open_revisions: move |p: PolicyDefinition| {
                                        drawer_revisions.set(true);
                                        drawer_policy.set(Some(p));
                                    },
                                    highlighted: focused_policy_name.read().as_ref() == Some(&policy.name),
                                    on_edit: move |p: PolicyDefinition| {
                                        editing_policy_id.set(Some(p.id));
                                        edit_name.set(p.name.clone());
                                        edit_description.set(p.description.clone());
                                        edit_body.set(p.body.clone());
                                        edit_format.set(p.format);
                                        edit_srg_ids.set(p.srg_ids.join(", "));
                                        edit_cci_ids.set(p.cci_ids.join(", "));
                                        show_editor.set(true);
                                    },
                                    on_delete: move |id: Uuid| {
                                        delete_confirm.set(Some(id));
                                    },
                                    selection_mode: selection_mode(),
                                    selected: selected_policy_ids.read().contains(&policy.id),
                                    on_toggle_select: move |selected: bool| {
                                        let mut ids = selected_policy_ids.write();
                                        if selected { ids.insert(policy.id); } else { ids.remove(&policy.id); }
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
                    edit_srg_ids: edit_srg_ids.clone(),
                    edit_cci_ids: edit_cci_ids.clone(),
                    policy_library: policy_library.clone(),
                    on_close: move || show_editor.set(false),
                }
            }

            if show_import() {
                crate::components::policy::PolicyInterchangeModal {
                    on_close: move |_| show_import.set(false),
                    on_success: move |_| {
                        let mut policy_library = policy_library;
                        let mut policies_load_error = policies_load_error;
                        spawn(async move {
                            match policies_api::load_policies().await {
                                policies_api::PolicyLoadResult::Ok(policies) => {
                                    policy_library.set(policies);
                                    policies_load_error.set(None);
                                }
                                policies_api::PolicyLoadResult::Err(error) => {
                                    policies_load_error.set(Some(format!("Import succeeded, but refresh failed: {error}")));
                                }
                            }
                        });
                    },
                }
            }

            if let Some(policy) = drawer_policy.read().clone() {
                PolicyDrawer {
                    policy,
                    is_admin: is_admin_user,
                    initial_revisions: drawer_revisions(),
                    on_close: move |_| drawer_policy.set(None),
                    on_export: export_single_policy,
                    on_edit: move |policy: PolicyDefinition| {
                        drawer_policy.set(None);
                        editing_policy_id.set(Some(policy.id));
                        edit_name.set(policy.name.clone());
                        edit_description.set(policy.description.clone());
                        edit_body.set(policy.body.clone());
                        edit_format.set(policy.format);
                        edit_srg_ids.set(policy.srg_ids.join(", "));
                        edit_cci_ids.set(policy.cci_ids.join(", "));
                        show_editor.set(true);
                    },
                }
            }

            if let Some(id) = *delete_confirm.read() {
                DeleteConfirmModal {
                    policy_id: id,
                    policy_name: policy_library.read().iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default(),
                    busy: *delete_busy.read(),
                    error: delete_error.read().clone(),
                    on_confirm: move |_| {
                        let mut policy_library = policy_library;
                        let mut delete_confirm = delete_confirm;
                        let mut delete_busy = delete_busy;
                        let mut delete_error = delete_error;
                        delete_error.set(None);
                        delete_busy.set(true);
                        spawn(async move {
                            match delete_deployment_policy(&id).await {
                                Ok(()) => {
                                    match policies_api::load_policies().await {
                                        policies_api::PolicyLoadResult::Ok(p) => {
                                            policies_load_error.set(None);
                                            policy_library.set(p);
                                        }
                                        policies_api::PolicyLoadResult::Err(e) => {
                                            policies_load_error.set(Some(e));
                                        }
                                    }
                                    delete_busy.set(false);
                                    delete_confirm.set(None);
                                }
                                Err(error) => {
                                    web_sys::console::error_1(&format!("Failed to delete policy: {error}").into());
                                    delete_busy.set(false);
                                    delete_error.set(Some(match error {
                                        crate::api::client::ApiClientError::Status { code, body } => {
                                            format!("Delete failed (HTTP {code}): {body}")
                                        }
                                        other => format!("Failed to delete policy: {other}"),
                                    }));
                                }
                            }
                        });
                    },
                    on_cancel: move |_| {
                        if !*delete_busy.read() {
                            delete_error.set(None);
                            delete_confirm.set(None);
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn PolicyDrawer(
    policy: PolicyDefinition,
    is_admin: bool,
    initial_revisions: bool,
    on_close: EventHandler<MouseEvent>,
    on_export: EventHandler<(PolicyDefinition, String)>,
    on_edit: EventHandler<PolicyDefinition>,
) -> Element {
    let mut selected_version_id = use_signal(|| policy.version_id);
    let mut show_revisions = use_signal(|| initial_revisions);
    let policy_version_id = policy.version_id;
    use_effect(move || selected_version_id.set(policy_version_id));
    use_effect(move || show_revisions.set(initial_revisions));

    let selected_revision = selected_version_id()
        .and_then(|id| policy.revisions.iter().find(|revision| revision.id == id));
    let displayed_policy = selected_revision.map_or_else(
        || policy.clone(),
        |revision| {
            let body = serde_json::to_string_pretty(&serde_json::json!({
                "policy_type": revision.policy_type,
                "enabled": revision.enabled,
                "config": revision.config,
            }))
            .unwrap_or_else(|_| "{}".to_string());
            PolicyDefinition {
                id: policy.id,
                lineage_id: policy.lineage_id,
                version_id: Some(revision.id),
                revision: Some(revision.version.clone()),
                publication_state: Some(revision.publication_state.clone()),
                semantic_digest: Some(revision.semantic_digest.clone()),
                revisions: policy.revisions.clone(),
                name: revision.name.clone(),
                description: revision
                    .description
                    .clone()
                    .unwrap_or_else(|| "No description".to_string()),
                format: policy.format,
                body,
                policy_type: Some(revision.policy_type.clone()),
                updated_at: policy.updated_at.clone(),
                system_count: policy.system_count,
                // Use the selected revision's exact mappings (not the lineage current).
                srg_ids: revision.srg_ids.clone(),
                cci_ids: revision.cci_ids.clone(),
                category: revision.category.clone(),
                framework: revision.framework.clone(),
                severity: revision.severity.clone(),
                control_family: revision.control_family.clone(),
                cmmc_level: revision.cmmc_level,
                cis_section: revision.cis_section.clone(),
                rationale: revision.rationale.clone(),
            }
        },
    );
    let category = policy_category(&displayed_policy);
    let rules = crate::components::policy::policy_rule_summaries(&displayed_policy);
    let is_core = is_core_policy(&displayed_policy);
    let is_editable = is_policy_version_editable(&displayed_policy);
    let policy_for_edit = displayed_policy.clone();
    let mut busy = use_signal(|| false);
    let mut action_status = use_signal(|| None::<String>);

    let version_id = displayed_policy.version_id;
    let revision_count = policy.revisions.len();
    let type_display = normalized_policy_type(&displayed_policy).replace('_', " ");
    let modified_at = selected_revision
        .map(|revision| revision.created_at.clone())
        .unwrap_or_else(|| displayed_policy.updated_at.clone());

    rsx! {
        div {
            class: "fl-tray-backdrop",
            onclick: move |evt| on_close.call(evt),
        }
        aside {
            class: "fl-tray",
            role: "dialog",
            "aria-label": "Policy detail",
            header {
                class: "fl-tray-head",
                div {
                    style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                    svg { width: "18", height: "18", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "flex-shrink:0;color:{category.color()};",
                        path { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }
                        polyline { points: "14 2 14 8 20 8" }
                    }
                    div { style: "min-width: 0;",
                        div {
                            style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                             span { class: "mono", style: "font-weight: 700; font-size: 15px;", "{displayed_policy.name}" }
                            span {
                                class: "chip",
                                style: "color: {category.color()}; background: color-mix(in oklab, {category.color()} 14%, transparent);",
                                "{category.short_label()}"
                            }
                            if is_core {
                                span { class: "chip chip-info", "built-in" }
                            } else {
                                span { class: "chip chip-healthy", "custom" }
                            }
                        }
                        div {
                            style: "font-size: 12px; color: var(--cf-text-muted); margin-top: 4px;",
                             "{displayed_policy.description}"
                        }
                    }
                }
                div { style: "display: flex; gap: 6px; align-items: center;",
                    if !is_core && is_editable {
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| on_edit.call(policy_for_edit.clone()),
                            "Edit"
                        }
                    }
                    IOMenu {
                        id: "policy-drawer-export".to_string(),
                        trigger_label: "Export".to_string(),
                        trigger_class: "btn btn-ghost focus-ring xs".to_string(),
                        items: vec![IOMenuItem::action("JSON"), IOMenuItem::action("TOML")],
                        on_action: {
                            let policy = displayed_policy.clone();
                            move |index| on_export.call((policy.clone(), if index == 0 { "json".to_string() } else { "toml".to_string() }))
                        },
                    }
                    // ── Lifecycle controls (admin-only, requires version ID) ─
                    if is_admin && !is_core && version_id.is_some() {
                        {
                            let vid = version_id.unwrap();
                            let pid = policy.id;
                            rsx! {
                                if let Some(status) = action_status.read().as_ref() {
                                    span { class: "chip chip-info", style: "font-size:10px;", "{status}" }
                                } else if !*busy.read() && is_editable {
                                    button {
                                        class: "btn btn-ghost focus-ring xs",
                                        title: "Mark this policy version as trusted",
                                        onclick: {
                                            move |_| {
                                                busy.set(true);
                                                let v = vid;
                                                spawn(async move {
                                                    match crate::api::client::trust_policy_version(
                                                        &v,
                                                        &crate::api::models::TrustPolicyVersionRequest { trusted: true, review_note: None },
                                                    ).await {
                                                        Ok(_) => { busy.set(false); action_status.set(Some("Trusted".into())); }
                                                        Err(e) => { busy.set(false); action_status.set(Some(format!("Error: {e}"))); }
                                                    }
                                                });
                                            }
                                        },
                                        "Trust"
                                    }
                                    button {
                                        class: "btn btn-primary focus-ring xs",
                                        title: "Publish this draft as an immutable accepted version",
                                        onclick: {
                                            move |_| {
                                                busy.set(true);
                                                let v = vid;
                                                spawn(async move {
                                                    match crate::api::client::publish_policy_version(&v).await {
                                                        Ok(_) => { busy.set(false); action_status.set(Some("Published".into())); }
                                                        Err(e) => { busy.set(false); action_status.set(Some(format!("Error: {e}"))); }
                                                    }
                                                });
                                            }
                                        },
                                        "Publish"
                                    }
                                }
                                if !is_editable {
                                    button {
                                        class: "btn btn-ghost focus-ring xs",
                                        disabled: *busy.read(),
                                        title: "Create a new draft from this accepted version",
                                        onclick: {
                                            move |_| {
                                                busy.set(true);
                                                spawn(async move {
                                                    match crate::api::client::create_policy_draft(&pid).await {
                                                        Ok(_) => { busy.set(false); action_status.set(Some("Draft created".into())); }
                                                        Err(e) => { busy.set(false); action_status.set(Some(format!("Error: {e}"))); }
                                                    }
                                                });
                                            }
                                        },
                                        "Create draft"
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        onclick: move |evt| on_close.call(evt),
                        title: "Close",
                        crate::components::icon::Icon { name: crate::components::icon::IconName::X, size: 16 }
                    }
                }
            }
            div { class: "ed-stats",
                div { class: "ed-stat", div { class: "ed-stat-label", "Systems" } div { class: "ed-stat-val", "{displayed_policy.system_count}" } }
                div { class: "ed-stat", div { class: "ed-stat-label", "Rules" } div { class: "ed-stat-val", "{rules.len()}" } }
                div { class: "ed-stat", div { class: "ed-stat-label", "Type" } div { class: "ed-stat-val", style: "font-size:12px;", "{type_display}" } }
                div { class: "ed-stat", div { class: "ed-stat-label", "Modified" } div { class: "ed-stat-val", style: "font-size:12px;", "{modified_at}" } }
                div { class: "ed-stat", div { class: "ed-stat-label", "Owner" } div { class: "ed-stat-val", "—" } }
            }
            if revision_count > 1 {
                div { class: "policy-drawer-tabs",
                    button { class: if !show_revisions() { "btn btn-ghost xs focus-ring active" } else { "btn btn-ghost xs focus-ring" }, onclick: move |_| show_revisions.set(false), "Details" }
                    button { class: if show_revisions() { "btn btn-ghost xs focus-ring active" } else { "btn btn-ghost xs focus-ring" }, onclick: move |_| show_revisions.set(true), "Revisions · {revision_count}" }
                }
            }
            div {
                class: "ed-body policy-drawer-body",
                if show_revisions() && revision_count > 1 {
                    div { class: "policy-revision-list",
                        for revision in policy.revisions.iter().cloned() {
                            {
                                let revision_id = revision.id;
                                let selected = selected_version_id() == Some(revision_id);
                                rsx! {
                                    button {
                                        class: if selected { "policy-revision-row selected focus-ring" } else { "policy-revision-row focus-ring" },
                                        onclick: move |_| selected_version_id.set(Some(revision_id)),
                                        div {
                                            div { class: "mono", style: "font-weight:700;", "v{revision.version}" }
                                            if let Some(description) = revision.description.as_ref() { div { style: "margin-top:4px;font-size:12px;color:var(--cf-text-secondary);", "{description}" } }
                                            div { style: "margin-top:6px;font-size:11px;color:var(--cf-text-muted);", "Created {revision.created_at}" }
                                        }
                                        div { style: "display:flex;flex-wrap:wrap;justify-content:flex-end;gap:5px;",
                                            span { class: "chip chip-unknown", "{revision.publication_state}" }
                                            if revision.is_current_draft { span { class: "chip chip-info", "current draft" } }
                                            if revision.is_current_published { span { class: "chip chip-healthy", "current" } }
                                            if selected { span { class: "chip", "selected" } }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if !displayed_policy.srg_ids.is_empty() || !displayed_policy.cci_ids.is_empty() {
                        div {
                            h3 { class: "policy-drawer-section-title", "SRG / CCI mapping" }
                            div { style: "display: flex; flex-wrap: wrap; gap: 5px;",
                                for srg in displayed_policy.srg_ids.iter() { span { class: "chip mono policy-mapping-srg", "{srg}" } }
                                for cci in displayed_policy.cci_ids.iter() { span { class: "chip mono policy-mapping-cci", "{cci}" } }
                            }
                        }
                    }
                    div {
                        h3 { class: "policy-drawer-section-title", "Rules" }
                        if rules.is_empty() {
                            div { class: "sd-callout sd-callout-info", "No automated rules are configured for this policy." }
                        } else {
                            div { style: "display:flex;flex-direction:column;gap:8px;",
                                for rule in rules {
                                    div { class: "policy-rule-card",
                                        div { class: "policy-rule-title", "{rule.label}" }
                                        div { class: "policy-rule-kind", "kind: {rule.kind}" }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        h3 { class: "policy-drawer-section-title", "Systems using this policy · {displayed_policy.system_count}" }
                        p { style: "margin:0;font-size:12px;color:var(--cf-text-muted);line-height:1.5;", "{displayed_policy.system_count} systems currently use this policy. Detailed system membership is not available from this endpoint." }
                    }
                    details { class: "policy-definition",
                        summary { "Definition" }
                        pre { class: "mono", "{displayed_policy.body}" }
                    }
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

    if query.trim().is_empty() {
        return true;
    }

    let q = query.to_lowercase();

    if policy.name.to_lowercase().contains(&q) {
        return true;
    }
    if policy.description.to_lowercase().contains(&q) {
        return true;
    }

    // Search across SRG/CCI mappings on ALL revisions (including historical).
    // This ensures a search for "CCI-000205" or "000205" finds the lineage even
    // when the mapping only existed on an older version.
    for revision in &policy.revisions {
        for srg in &revision.srg_ids {
            if srg.to_lowercase().contains(&q) {
                return true;
            }
        }
        for cci in &revision.cci_ids {
            if cci.to_lowercase().contains(&q) {
                return true;
            }
        }
    }
    // Also search the current lineage-level srg/cci (which may be derived from
    // the current version if revisions list is empty for some reason).
    for srg in &policy.srg_ids {
        if srg.to_lowercase().contains(&q) {
            return true;
        }
    }
    for cci in &policy.cci_ids {
        if cci.to_lowercase().contains(&q) {
            return true;
        }
    }

    false
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

fn sanitize_filename(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let mut compact = String::with_capacity(sanitized.len());
    for character in sanitized.chars() {
        if character == '-' && compact.ends_with('-') {
            continue;
        }
        compact.push(character);
    }
    let trimmed = compact.trim_matches('-');
    if trimmed.is_empty() {
        "policy".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod interchange_tests {
    use super::sanitize_filename;

    #[test]
    fn export_filename_is_stable_and_safe() {
        assert_eq!(
            sanitize_filename("Firewall enabled / prod"),
            "Firewall-enabled-prod"
        );
        assert_eq!(sanitize_filename("  "), "policy");
        assert_eq!(sanitize_filename("bundle.v1"), "bundle.v1");
    }
}

#[component]
fn DeleteConfirmModal(
    policy_id: Uuid,
    policy_name: String,
    busy: bool,
    error: Option<String>,
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
                h3 { class: "text-lg font-semibold text-white text-center mb-2", "Delete draft policy?" }
                p { class: "text-sm {theme::text::SECONDARY} text-center mb-6",
                    "This permanently removes the unpublished policy "
                    span { class: "font-medium text-white", "{policy_name}" }
                    " and its draft history. This cannot be undone."
                }
                if let Some(error) = error {
                    div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;", "{error}" }
                }
                div { class: "flex gap-3",
                    button { class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white", disabled: busy, onclick: move |_| on_cancel.call(()), "Cancel" }
                    button { class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white", disabled: busy, onclick: move |_| on_confirm.call(()), if busy { "Deleting…" } else { "Delete" } }
                }
            }
        }
    }
}
