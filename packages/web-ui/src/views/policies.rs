//! Policies view — global policy management for deployment rules.

use dioxus::prelude::*;
use std::collections::HashSet;
use uuid::Uuid;

use crate::Route;
use crate::api::client::{
    delete_deployment_policy, export_policy_versions, fetch_compliance_grouping_schemes,
    fetch_policy_requirement_mappings, fetch_policy_version_usage,
};
use crate::api::models::{ComplianceGroupingScheme, PolicyMappingRow, PolicyVersionUsageResponse};
use crate::components::io_menu::{IOMenu, IOMenuItem};
use crate::components::layout::Card;
use crate::components::policy::{
    GroupingSchemesModal, PolicyCard, PolicyCategory, PolicyDefinition, PolicyEditorModal,
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
    let mut show_grouping_schemes = use_signal(|| false);
    let mut grouping_schemes: Signal<Vec<ComplianceGroupingScheme>> = use_signal(Vec::new);
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
    let mut domain = use_signal(|| "platform".to_string());
    let mut security_grouping = use_signal(|| "control-family".to_string());
    let mut type_filter = use_signal(|| "all".to_string());
    let mut delete_confirm: Signal<Option<Uuid>> = use_signal(|| None);
    let mut delete_busy = use_signal(|| false);
    let mut delete_error: Signal<Option<String>> = use_signal(|| None);
    let mut delete_eligibility: Signal<Option<crate::api::models::DeletionEligibility>> =
        use_signal(|| None);
    let mut delete_eligibility_loading = use_signal(|| false);
    let mut focused_policy_name = use_signal(|| None::<String>);
    let mut policies_loaded = use_signal(|| false);
    let mut pending_policy_focus = use_signal(|| None::<NavigationFocus>);
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let is_authenticated = auth::is_authenticated(&app_state.read().auth);
    let mut selection_mode = use_signal(|| false);
    let mut selected_policy_ids = use_signal(HashSet::<Uuid>::new);
    let mut export_error = use_signal(|| None::<String>);

    use_effect(move || {
        if !is_authenticated {
            return;
        }
        spawn(async move {
            // Grouping schemes are optional presentation data. A load failure must
            // leave built-in grouping available.
            if let Ok(schemes) = fetch_compliance_grouping_schemes().await {
                grouping_schemes.set(schemes);
            }
        });
    });

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

    let domain_policies = all_policies
        .iter()
        .filter(|policy| policy_domain(policy) == domain())
        .cloned()
        .collect::<Vec<_>>();
    let filtered_policies = domain_policies
        .iter()
        .filter(|policy| {
            policy_matches_filters(
                policy,
                &query,
                if domain() == "platform" {
                    &selected_category
                } else {
                    "all"
                },
                &selected_type,
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let filtered_count = filtered_policies.len();
    let filtered_label = if filtered_count == 1 {
        "policy"
    } else {
        "policies"
    };
    let has_filters = (domain() == "platform" && selected_category != "all")
        || selected_type != "all"
        || !query.is_empty();
    let platform_category_counts = platform_category_counts(&all_policies);
    let current_grouping_schemes = grouping_schemes.read().clone();
    let grouped_policies = grouped_policies(
        &filtered_policies,
        &domain(),
        &security_grouping(),
        &current_grouping_schemes,
    );
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

            div { class: "pol-domain-tabs", role: "tablist", "aria-label": "Policy domain",
                for (domain_id, label, count, color, blurb) in [
                    ("platform", "Platform", all_policies.iter().filter(|policy| policy_domain(policy) == "platform").count(), "#60a5fa", "Deployment modes, pipeline gates, and rollout controls."),
                    ("security", "Security controls", all_policies.iter().filter(|policy| policy_domain(policy) == "security").count(), "#f87171", "Framework-owned controls for security and compliance."),
                ] {
                    button {
                        key: "{domain_id}",
                        role: "tab",
                        "aria-selected": "{domain() == domain_id}",
                        class: if domain() == domain_id { "pol-domain-tab active focus-ring" } else { "pol-domain-tab focus-ring" },
                        title: "{blurb}",
                        style: "--dc:{color};",
                        onclick: move |_| {
                            domain.set(domain_id.to_string());
                        },
                        "{label} " span { class: "pol-domain-tab-count", "{count}" }
                    }
                }
            }

            div { class: "pol-group-toolbar",
                span { class: "pol-group-toolbar-label", if domain() == "platform" { "Category" } else { "Grouping" } }
                if domain() == "platform" {
                    div { class: "seg",
                        button {
                            class: if selected_category == "all" { "active" } else { "" },
                            onclick: move |_| category_filter.set("all".to_string()),
                            "all"
                        }
                        for (category, count) in platform_category_counts.iter().copied() {
                            button {
                                key: "seg-{category.id()}",
                                class: if selected_category == category.id() { "active" } else { "" },
                                title: "{category.blurb()}",
                                onclick: move |_| category_filter.set(category.id().to_string()),
                                span { style: "display:inline-flex;align-items:center;gap:5px;",
                                    span { style: "width:6px;height:6px;border-radius:50%;background:{category.color()};flex-shrink:0;" }
                                    "{category.short_label()} " span { class: "mono", style: "opacity:.6;font-size:10.5px;", "{count}" }
                                }
                            }
                        }
                    }
                } else {
                    select {
                        class: "input focus-ring filter-select",
                        style: "width:auto;font-size:12px;padding:6px 28px 6px 10px;",
                        value: "{security_grouping}",
                        onchange: move |event| security_grouping.set(event.value()),
                        option { value: "control-family", "NIST 800-53 family" }
                        option { value: "severity", "STIG severity (CAT)" }
                        option { value: "cci", "CCI (Control Correlation Identifier)" }
                        option { value: "srg-category", "SRG category" }
                        option { value: "cmmc-level", "CMMC 2.0 level" }
                        option { value: "cis-section", "CIS Benchmark section" }
                        option { value: "remediation", "Remediation status" }
                        option { value: "flat", "Flat list (no grouping)" }
                        for scheme in current_grouping_schemes.iter() {
                            option { value: "custom:{scheme.id}", "{scheme.name}" }
                        }
                    }
                }
                if domain() == "platform" {
                    button { class: "btn btn-ghost focus-ring xs", style: "margin-left:auto;visibility:hidden;", tabindex: "-1", "Manage groupings" }
                } else if is_admin_user {
                    button { class: "btn btn-ghost focus-ring xs", style: "margin-left:auto;", onclick: move |_| show_grouping_schemes.set(true), "Manage groupings" }
                } else {
                    span { style: "margin-left:auto;" }
                }
            }

            div { class: "filterbar", style: "min-height:42px;",
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
                                svg { width: "48", height: "48", class: "mx-auto text-gray-600 mb-4", style: "display:block;width:48px;height:48px;max-width:48px;max-height:48px;", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1.5", d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                                }
                                p { class: "text-gray-400 mb-2", "No policies yet" }
                                p { class: "text-sm text-gray-500", "Create your first custom policy to get started." }
                            }
                        }
                    }
                }
            } else {
                for group in grouped_policies.iter() {
                    section { class: "pol-group",
                        div { class: "pol-group-head",
                            span { class: "pol-group-icon", style: "background:color-mix(in oklab, {group.color} 16%, transparent);color:{group.color};",
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M9 12l2 2 4-4" }
                                    path { d: "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                                }
                            }
                            div { style: "min-width:0;",
                                h2 { class: "pol-group-title", "{group.label} " span { class: "pol-group-count", "{group.items.len()}" } }
                                div { class: "pol-group-blurb", "{group.blurb}" }
                            }
                        }

                        div { class: "cards-grid",
                            for policy in group.items.iter().cloned() {
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
                                        // Fetch deletion eligibility before showing the dialog.
                                        delete_eligibility.set(None);
                                        delete_error.set(None);
                                        delete_eligibility_loading.set(true);
                                        delete_confirm.set(Some(id));
                                        spawn(async move {
                                            match crate::api::client::fetch_policy_deletion_eligibility(&id).await {
                                                Ok(result) => delete_eligibility.set(Some(result)),
                                                Err(_) => {}
                                            }
                                            delete_eligibility_loading.set(false);
                                        });
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

            if show_grouping_schemes() {
                GroupingSchemesModal {
                    schemes: current_grouping_schemes.clone(),
                    policies: all_policies.clone(),
                    selected_scheme_id: security_grouping().strip_prefix("custom:").and_then(|id| Uuid::parse_str(id).ok()),
                    on_close: move |_| show_grouping_schemes.set(false),
                    on_select: move |id: Option<Uuid>| security_grouping.set(id.map(|id| format!("custom:{id}")).unwrap_or_else(|| "control-family".to_string())),
                    on_changed: move |schemes| grouping_schemes.set(schemes),
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
                    eligibility_loading: *delete_eligibility_loading.read(),
                    eligibility: delete_eligibility.read().clone(),
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
pub fn PolicyDrawer(
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
    let mut mappings: Signal<Vec<PolicyMappingRow>> = use_signal(Vec::new);
    let mut mappings_loading = use_signal(|| false);
    let mut mappings_error: Signal<Option<String>> = use_signal(|| None);
    let mut loaded_mapping_version: Signal<Option<Uuid>> = use_signal(|| None);
    let mut mapping_request_generation = use_signal(|| 0_u64);
    let mut usage: Signal<Option<PolicyVersionUsageResponse>> = use_signal(|| None);
    let mut usage_loading = use_signal(|| false);
    let mut usage_error: Signal<Option<String>> = use_signal(|| None);
    let mut loaded_usage_version: Signal<Option<Uuid>> = use_signal(|| None);
    let mut usage_request_generation = use_signal(|| 0_u64);

    use_effect(move || {
        let requested_version = selected_version_id();
        let generation = *mapping_request_generation.peek() + 1;
        mapping_request_generation.set(generation);
        loaded_mapping_version.set(None);
        mappings_error.set(None);

        let Some(requested_version) = requested_version else {
            mappings.set(Vec::new());
            mappings_loading.set(false);
            return;
        };

        mappings.set(Vec::new());
        mappings_loading.set(true);
        spawn(async move {
            let result = fetch_policy_requirement_mappings(&requested_version).await;
            if mapping_request_generation() != generation
                || selected_version_id() != Some(requested_version)
            {
                return;
            }
            mappings_loading.set(false);
            match result {
                Ok(value) => {
                    mappings.set(value);
                    loaded_mapping_version.set(Some(requested_version));
                }
                Err(error) => {
                    mappings_error.set(Some(format!("Failed to load normalized mappings: {error}")))
                }
            }
        });
    });

    use_effect(move || {
        let requested_version = selected_version_id();
        let generation = *usage_request_generation.peek() + 1;
        usage_request_generation.set(generation);
        loaded_usage_version.set(None);
        usage_error.set(None);

        let Some(requested_version) = requested_version else {
            usage.set(None);
            usage_loading.set(false);
            return;
        };

        usage.set(None);
        usage_loading.set(true);
        spawn(async move {
            let result = fetch_policy_version_usage(&requested_version).await;
            if usage_request_generation() != generation
                || selected_version_id() != Some(requested_version)
            {
                return;
            }
            usage_loading.set(false);
            match result {
                Ok(value) => {
                    usage.set(Some(value));
                    loaded_usage_version.set(Some(requested_version));
                }
                Err(error) => {
                    usage_error.set(Some(format!("Failed to load policy usage: {error}")))
                }
            }
        });
    });

    let mapping_groups = grouped_policy_mappings(&mappings.read());
    let usage_snapshot = usage.read().clone();
    let resolved_system_count = usage_snapshot
        .as_ref()
        .map(|usage| {
            usage
                .systems
                .iter()
                .map(|system| system.system_id)
                .collect::<HashSet<_>>()
                .len()
        })
        .unwrap_or(0);

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
                div { class: "ed-stat", div { class: "ed-stat-label", "Systems" } div { class: "ed-stat-val", if usage_loading() { "—" } else { "{resolved_system_count}" } } }
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
                    if let Some(rationale) = displayed_policy.rationale.as_deref().filter(|value| !value.trim().is_empty()) {
                        section {
                            h3 { class: "policy-drawer-section-title", "Rationale" }
                            div { style: "font-size:13px;color:var(--cf-text-primary);line-height:1.5;", "{rationale}" }
                        }
                    }
                    section {
                        h3 { class: "policy-drawer-section-title", "Mapped Requirements · {mappings.read().len()}" }
                        if mappings_loading() {
                            div { style: "font-size:12px;color:var(--cf-text-muted);", "Loading normalized requirement mappings…" }
                        } else if let Some(error) = mappings_error.read().clone() {
                            div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{error}" }
                        } else if loaded_mapping_version() != displayed_policy.version_id {
                            div { style: "font-size:12px;color:var(--cf-text-muted);", "Loading normalized requirement mappings…" }
                        } else if mapping_groups.is_empty() {
                            div { class: "sd-callout sd-callout-info", style: "margin-bottom:10px;", "This policy is not currently mapped to an external compliance requirement. It can still be used as an operational or custom policy." }
                        } else {
                            div { style: "display:flex;flex-direction:column;gap:12px;",
                                for (framework_name, framework_version, group) in mapping_groups.iter() {
                                    div {
                                        div { style: "font-size:11.5px;font-weight:700;color:var(--cf-text-primary);margin-bottom:6px;", "{framework_name}" span { style: "color:var(--cf-text-muted);font-weight:400;", " · {framework_version}" } }
                                        div { style: "display:flex;flex-direction:column;gap:6px;",
                                            for row in group.iter() {
                                                div { key: "{row.id}", style: "padding:9px 11px;background:var(--cf-subtle-bg);border-radius:8px;border:1px solid var(--cf-divider);",
                                                    div { style: "display:flex;justify-content:space-between;gap:8px;",
                                                        span { class: "mono", style: "font-size:12px;font-weight:600;", "{row.requirement_external_id}" }
                                                        span { style: "font-size:9.5px;color:var(--cf-text-muted);", "{mapping_provenance_label(&row.provenance)}" }
                                                    }
                                                    if let Some(title) = row.requirement_title.as_deref() { div { style: "font-size:11.5px;color:var(--cf-text-secondary);margin:2px 0 5px;", "{title}" } }
                                                    div { style: "font-size:11px;display:flex;gap:6px;align-items:center;",
                                                        span { style: "font-weight:600;color:var(--cf-text-primary);", "{mapping_relationship_label(&row.relationship)}" }
                                                        span { style: "color:var(--cf-text-muted);", if row.coverage == "full" { "· Full coverage" } else { "· Partial coverage" } }
                                                    }
                                                    if let Some(rationale) = row.rationale.as_deref().filter(|value| !value.trim().is_empty()) { div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:5px;line-height:1.4;", "{rationale}" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if displayed_policy.category.as_deref().is_some_and(|category| category.eq_ignore_ascii_case("security")) {
                        div {
                            h3 { class: "policy-drawer-section-title", "Source / imported metadata" }
                            div { style: "display: flex; flex-wrap: wrap; gap: 5px;",
                                if let Some(framework) = displayed_policy.framework.as_deref().filter(|value| !value.trim().is_empty()) { span { class: "chip chip-info", "{framework}" } }
                                if let Some(severity) = displayed_policy.severity.as_deref().filter(|value| !value.trim().is_empty()) { span { class: "chip chip-unknown", "{severity}" } }
                                if let Some(family) = displayed_policy.control_family.as_deref().filter(|value| !value.trim().is_empty()) { span { class: "chip mono", "{family}" } }
                                if let Some(level) = displayed_policy.cmmc_level { span { class: "chip", "CMMC Level {level}" } }
                                if let Some(section) = displayed_policy.cis_section.as_deref().filter(|value| !value.trim().is_empty()) { span { class: "chip mono", "CIS {section}" } }
                            }
                        }
                    }
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
                    section {
                        h3 { class: "policy-drawer-section-title", "Used by bundles" }
                        if usage_loading() || loaded_usage_version() != displayed_policy.version_id {
                            div { style: "font-size:12px;color:var(--cf-text-muted);", "Loading exact-version usage…" }
                        } else if let Some(error) = usage_error.read().as_ref() {
                            div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{error}" }
                        } else if let Some(usage) = usage_snapshot.as_ref() {
                            if usage.bundle_versions.is_empty() {
                                div { class: "sd-callout sd-callout-info", "This exact policy version is not selected by any bundle revision." }
                            } else {
                                div { style: "display:flex;flex-direction:column;gap:6px;",
                                    for bundle in usage.bundle_versions.iter() {
                                        div { key: "{bundle.bundle_version_id}", style: "display:flex;justify-content:space-between;gap:10px;padding:8px 10px;border:1px solid var(--cf-divider);border-radius:8px;background:var(--cf-subtle-bg);",
                                            div {
                                                div { style: "font-size:12px;font-weight:600;", "{bundle.bundle_name}" }
                                                div { class: "mono", style: "font-size:10px;color:var(--cf-text-muted);margin-top:2px;", "Revision {bundle.bundle_version} · policy order {bundle.policy_order}" }
                                            }
                                            div { style: "display:flex;gap:4px;align-items:center;",
                                                span { class: "chip chip-unknown", "{bundle.publication_state}" }
                                                if bundle.is_current_published { span { class: "chip chip-healthy", "current" } }
                                                if bundle.is_current_draft { span { class: "chip chip-info", "draft" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    section {
                        h3 { class: "policy-drawer-section-title", "Systems using this version · {resolved_system_count}" }
                        if usage_loading() || loaded_usage_version() != displayed_policy.version_id {
                            div { style: "font-size:12px;color:var(--cf-text-muted);", "Loading resolved system membership…" }
                        } else if usage_error.read().is_none() {
                            if let Some(usage) = usage_snapshot.as_ref() {
                                if usage.systems.is_empty() {
                                    div { class: "sd-callout sd-callout-info", "No active bundle assignment currently resolves this exact policy version onto a system." }
                                } else {
                                    div { style: "display:flex;flex-direction:column;gap:6px;",
                                        for system in usage.systems.iter() {
                                            {
                                                let environment = system.environment.as_deref().unwrap_or("No environment");
                                                rsx! { Link { key: "{system.bundle_version_id}-{system.system_id}", class: "policy-revision-row focus-ring", to: Route::SystemDetailView { id: system.system_id.to_string() },
                                                    div {
                                                        div { class: "mono", style: "font-weight:700;", "{system.hostname}" }
                                                        div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:3px;", "{environment} · {system.bundle_name} rev {system.bundle_version}" }
                                                    }
                                                    div { style: "display:flex;gap:4px;flex-wrap:wrap;justify-content:flex-end;",
                                                        span { class: "chip chip-info", "{system.source}" }
                                                        span { class: "chip chip-neutral", "{system.enforcement_mode}" }
                                                    }
                                                }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
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

fn mapping_relationship_label(relationship: &str) -> &str {
    match relationship {
        "implements" => "Implements",
        "supports" => "Supports",
        "provides_evidence_for" => "Provides evidence for",
        other => other,
    }
}

fn mapping_provenance_label(provenance: &str) -> &str {
    match provenance {
        "manual" => "Manual mapping",
        "imported" => "Imported",
        "inherited" => "Inherited",
        "inferred" => "Inferred",
        "suggested" => "Suggested",
        other => other,
    }
}

fn grouped_policy_mappings(
    rows: &[PolicyMappingRow],
) -> Vec<(String, String, Vec<PolicyMappingRow>)> {
    let mut groups: Vec<(String, String, Vec<PolicyMappingRow>)> = Vec::new();
    for row in rows {
        if let Some((_, _, group)) = groups.iter_mut().find(|(framework, version, _)| {
            *framework == row.framework_name && *version == row.framework_version
        }) {
            group.push(row.clone());
        } else {
            groups.push((
                row.framework_name.clone(),
                row.framework_version.clone(),
                vec![row.clone()],
            ));
        }
    }
    groups
}

fn policy_matches_filters(
    policy: &PolicyDefinition,
    query: &str,
    selected_category: &str,
    selected_type: &str,
) -> bool {
    if selected_category != "all" && platform_category(policy).id() != selected_category {
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

    if metadata_matches(
        &policy.name,
        &policy.description,
        policy.framework.as_deref(),
        &policy.srg_ids,
        &policy.cci_ids,
        policy.control_family.as_deref(),
        policy.cis_section.as_deref(),
        policy.severity.as_deref(),
        &q,
    ) {
        return true;
    }

    // The endpoint returns one current definition per lineage. Match revision
    // summaries too so metadata removed from the current version remains searchable.
    for revision in &policy.revisions {
        if metadata_matches(
            &revision.name,
            revision.description.as_deref().unwrap_or_default(),
            revision.framework.as_deref(),
            &revision.srg_ids,
            &revision.cci_ids,
            revision.control_family.as_deref(),
            revision.cis_section.as_deref(),
            revision.severity.as_deref(),
            &q,
        ) {
            return true;
        }
    }

    false
}

#[allow(clippy::too_many_arguments)]
fn metadata_matches(
    name: &str,
    description: &str,
    framework: Option<&str>,
    srg_ids: &[String],
    cci_ids: &[String],
    control_family: Option<&str>,
    cis_section: Option<&str>,
    severity: Option<&str>,
    query: &str,
) -> bool {
    [
        Some(name),
        Some(description),
        framework,
        control_family,
        cis_section,
        severity,
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(query))
        || srg_ids
            .iter()
            .chain(cci_ids)
            .any(|value| value.to_lowercase().contains(query))
}

fn policy_domain(policy: &PolicyDefinition) -> &'static str {
    if policy
        .category
        .as_deref()
        .is_some_and(|category| category.eq_ignore_ascii_case("security"))
    {
        "security"
    } else {
        "platform"
    }
}

fn platform_category(policy: &PolicyDefinition) -> PolicyCategory {
    match policy
        .category
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pipeline") => PolicyCategory::Pipeline,
        Some("rollout") => PolicyCategory::Rollout,
        _ => PolicyCategory::Deployment,
    }
}

fn platform_category_counts(policies: &[PolicyDefinition]) -> Vec<(PolicyCategory, usize)> {
    [
        PolicyCategory::Deployment,
        PolicyCategory::Pipeline,
        PolicyCategory::Rollout,
    ]
    .into_iter()
    .map(|category| {
        let count = policies
            .iter()
            .filter(|policy| {
                policy_domain(policy) == "platform" && platform_category(policy) == category
            })
            .count();
        (category, count)
    })
    .collect()
}

#[derive(Clone)]
struct PolicyGroup {
    label: String,
    blurb: String,
    color: &'static str,
    items: Vec<PolicyDefinition>,
}

fn grouped_policies(
    policies: &[PolicyDefinition],
    domain: &str,
    grouping: &str,
    custom_schemes: &[ComplianceGroupingScheme],
) -> Vec<PolicyGroup> {
    if domain == "platform" {
        return [
            PolicyCategory::Deployment,
            PolicyCategory::Pipeline,
            PolicyCategory::Rollout,
        ]
        .into_iter()
        .filter_map(|category| {
            let items = policies
                .iter()
                .filter(|policy| platform_category(policy) == category)
                .cloned()
                .collect::<Vec<_>>();
            (!items.is_empty()).then(|| PolicyGroup {
                label: category.label().to_string(),
                blurb: category.blurb().to_string(),
                color: category.color(),
                items,
            })
        })
        .collect();
    }

    if grouping == "flat" {
        return (!policies.is_empty())
            .then(|| PolicyGroup {
                label: "All security controls".to_string(),
                blurb: "Every control in this domain, ungrouped.".to_string(),
                color: PolicyCategory::Security.color(),
                items: policies.to_vec(),
            })
            .into_iter()
            .collect();
    }

    if let Some(scheme_id) = grouping
        .strip_prefix("custom:")
        .and_then(|id| Uuid::parse_str(id).ok())
    {
        if let Some(scheme) = custom_schemes.iter().find(|scheme| scheme.id == scheme_id) {
            return grouped_by_custom_scheme(policies, scheme);
        }
    }

    let mut groups: Vec<PolicyGroup> = Vec::new();
    for policy in policies {
        let (label, blurb) = security_group_label(policy, grouping);
        if let Some(group) = groups.iter_mut().find(|group| group.label == label) {
            group.items.push(policy.clone());
        } else {
            groups.push(PolicyGroup {
                label,
                blurb,
                color: PolicyCategory::Security.color(),
                items: vec![policy.clone()],
            });
        }
    }
    groups
}

fn grouped_by_custom_scheme(
    policies: &[PolicyDefinition],
    scheme: &ComplianceGroupingScheme,
) -> Vec<PolicyGroup> {
    let mut assigned = HashSet::new();
    let mut groups = Vec::new();

    for group in &scheme.groups {
        let items = policies
            .iter()
            .filter(|policy| !assigned.contains(&policy.id))
            .filter(|policy| custom_group_matches(policy, group))
            .cloned()
            .collect::<Vec<_>>();
        assigned.extend(items.iter().map(|policy| policy.id));
        if !items.is_empty() {
            groups.push(PolicyGroup {
                label: group.name.clone(),
                blurb: group
                    .description
                    .clone()
                    .unwrap_or_else(|| "Custom grouping scheme.".to_string()),
                color: PolicyCategory::Security.color(),
                items,
            });
        }
    }

    let ungrouped = policies
        .iter()
        .filter(|policy| !assigned.contains(&policy.id))
        .cloned()
        .collect::<Vec<_>>();
    if !ungrouped.is_empty() {
        groups.push(PolicyGroup {
            label: "Ungrouped".to_string(),
            blurb: "No custom group matched this control.".to_string(),
            color: PolicyCategory::Security.color(),
            items: ungrouped,
        });
    }
    groups
}

fn custom_group_matches(
    policy: &PolicyDefinition,
    group: &crate::api::models::ComplianceGroupingSchemeGroup,
) -> bool {
    // Exclusions take precedence, including over an explicit pin.
    if group.excluded_policy_ids.contains(&policy.id) {
        return false;
    }
    if group.pinned_policy_ids.contains(&policy.id) {
        return true;
    }
    let query = group.query.trim().to_ascii_lowercase();
    !query.is_empty()
        && metadata_matches(
            &policy.name,
            &policy.description,
            policy.framework.as_deref(),
            &policy.srg_ids,
            &policy.cci_ids,
            policy.control_family.as_deref(),
            policy.cis_section.as_deref(),
            policy.severity.as_deref(),
            &query,
        )
}

fn security_group_label(policy: &PolicyDefinition, grouping: &str) -> (String, String) {
    match grouping {
        "control-family" => policy
            .control_family
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| (value.to_string(), "NIST 800-53 control family.".to_string()))
            .unwrap_or_else(|| {
                (
                    "Ungrouped".to_string(),
                    "No NIST family tag is set.".to_string(),
                )
            }),
        "severity" => policy
            .severity
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                (
                    format!("CAT {}", value.to_ascii_uppercase()),
                    "STIG severity.".to_string(),
                )
            })
            .unwrap_or_else(|| {
                (
                    "Unrated".to_string(),
                    "No severity is assigned.".to_string(),
                )
            }),
        "cci" => policy
            .cci_ids
            .first()
            .cloned()
            .map(|value| (value, "Control Correlation Identifier.".to_string()))
            .unwrap_or_else(|| {
                (
                    "Unmapped".to_string(),
                    "No CCI mapping is assigned.".to_string(),
                )
            }),
        "srg-category" => policy
            .srg_ids
            .first()
            .and_then(|id| id.split('-').nth(1))
            .filter(|value| !value.trim().is_empty())
            .map(|value| (value.to_ascii_uppercase(), "SRG category.".to_string()))
            .unwrap_or_else(|| {
                (
                    "Unmapped".to_string(),
                    "No SRG mapping is assigned.".to_string(),
                )
            }),
        "cmmc-level" => policy
            .cmmc_level
            .map(|level| {
                (
                    format!("Level {level}"),
                    "Explicit CMMC maturity level.".to_string(),
                )
            })
            .unwrap_or_else(|| {
                (
                    "Unrated".to_string(),
                    "No explicit CMMC level is assigned.".to_string(),
                )
            }),
        "cis-section" => policy
            .cis_section
            .as_deref()
            .and_then(|section| section.split('.').next())
            .filter(|value| !value.trim().is_empty())
            .map(|section| {
                (
                    format!("Section {section}"),
                    "CIS Benchmark section.".to_string(),
                )
            })
            .unwrap_or_else(|| {
                (
                    "Unmapped".to_string(),
                    "No CIS section is assigned.".to_string(),
                )
            }),
        "remediation" => remediation_status(policy),
        _ => (
            "Ungrouped".to_string(),
            "No grouping is selected.".to_string(),
        ),
    }
}

fn remediation_status(policy: &PolicyDefinition) -> (String, String) {
    let config = serde_json::from_str::<serde_json::Value>(&policy.body)
        .ok()
        .and_then(|value| value.get("config").cloned().or(Some(value)))
        .unwrap_or(serde_json::Value::Null);
    remediation_status_from_config(&config)
}

fn remediation_status_from_config(config: &serde_json::Value) -> (String, String) {
    let kinds = config
        .get("rules")
        .and_then(|rules| rules.as_array())
        .map(|rules| {
            rules
                .iter()
                .filter_map(|rule| rule.get("kind").and_then(|kind| kind.as_str()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !kinds.is_empty() && kinds.iter().all(|kind| *kind == "nixos_option") {
        (
            "Automated".to_string(),
            "All rules are declarative NixOS options.".to_string(),
        )
    } else if kinds
        .iter()
        .any(|kind| matches!(*kind, "nixos_option" | "custom_eval"))
    {
        (
            "Semi-automated".to_string(),
            "NixOS or custom evaluation requires a reviewed fix.".to_string(),
        )
    } else {
        (
            "Manual".to_string(),
            "No automatable mechanism is configured.".to_string(),
        )
    }
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
    use super::{custom_group_matches, remediation_status_from_config, sanitize_filename};
    use crate::api::models::ComplianceGroupingSchemeGroup;
    use crate::components::policy::{PolicyDefinition, PolicyFormat};
    use uuid::Uuid;

    fn security_policy() -> PolicyDefinition {
        PolicyDefinition {
            id: Uuid::new_v4(),
            lineage_id: Uuid::new_v4(),
            version_id: None,
            revision: None,
            publication_state: None,
            semantic_digest: None,
            revisions: Vec::new(),
            name: "SSH hardening".to_string(),
            description: "Require a secure SSH configuration".to_string(),
            format: PolicyFormat::Json,
            body: "{}".to_string(),
            policy_type: Some("custom_check".to_string()),
            updated_at: String::new(),
            system_count: 0,
            srg_ids: vec!["SRG-OS-000001".to_string()],
            cci_ids: vec!["CCI-000001".to_string()],
            category: Some("security".to_string()),
            framework: Some("NIST 800-53".to_string()),
            severity: Some("high".to_string()),
            control_family: Some("AC".to_string()),
            cmmc_level: None,
            cis_section: Some("5.2".to_string()),
            rationale: None,
        }
    }

    #[test]
    fn export_filename_is_stable_and_safe() {
        assert_eq!(
            sanitize_filename("Firewall enabled / prod"),
            "Firewall-enabled-prod"
        );
        assert_eq!(sanitize_filename("  "), "policy");
        assert_eq!(sanitize_filename("bundle.v1"), "bundle.v1");
    }

    #[test]
    fn remediation_status_uses_only_rule_mechanisms() {
        assert_eq!(
            remediation_status_from_config(&serde_json::json!({
                "rules": [{ "kind": "nixos_option" }]
            }))
            .0,
            "Automated"
        );
        assert_eq!(
            remediation_status_from_config(&serde_json::json!({
                "rules": [{ "kind": "nixos_option" }, { "kind": "custom_eval" }]
            }))
            .0,
            "Semi-automated"
        );
        assert_eq!(
            remediation_status_from_config(&serde_json::json!({
                "rules": [{ "kind": "packages_installed" }]
            }))
            .0,
            "Manual"
        );
    }

    #[test]
    fn custom_group_query_searches_security_metadata() {
        let policy = security_policy();
        let group = ComplianceGroupingSchemeGroup {
            id: "ssh".to_string(),
            name: "SSH".to_string(),
            description: None,
            query: "cci-000001".to_string(),
            pinned_policy_ids: Vec::new(),
            excluded_policy_ids: Vec::new(),
        };
        assert!(custom_group_matches(&policy, &group));
    }

    #[test]
    fn custom_group_exclusion_wins_over_pin() {
        let policy = security_policy();
        let group = ComplianceGroupingSchemeGroup {
            id: "exception".to_string(),
            name: "Exception".to_string(),
            description: None,
            query: "unrelated".to_string(),
            pinned_policy_ids: vec![policy.id],
            excluded_policy_ids: vec![policy.id],
        };
        assert!(!custom_group_matches(&policy, &group));
    }
}

#[component]
fn DeleteConfirmModal(
    policy_id: Uuid,
    policy_name: String,
    busy: bool,
    eligibility_loading: bool,
    eligibility: Option<crate::api::models::DeletionEligibility>,
    error: Option<String>,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let _ = policy_id;
    let can_delete = eligibility.as_ref().map(|e| e.eligible).unwrap_or(false);
    let permanently_blocked = eligibility
        .as_ref()
        .map(|e| e.permanently_blocked())
        .unwrap_or(false);

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "modal",
                style: "width:min(520px,96vw);max-height:92vh;",
                onclick: |evt| evt.stop_propagation(),

                div { class: "modal-head",
                    h2 { style: "display:flex;align-items:center;gap:8px;",
                        span { style: "color:#f87171;display:inline-flex;",
                            crate::components::icon::Icon { name: crate::components::icon::IconName::Trash, size: 15 }
                        }
                        if permanently_blocked { "Policy cannot be permanently deleted" }
                        else if !can_delete && eligibility.is_some() { "Remove references before deleting" }
                        else { "Delete draft policy?" }
                    }
                    p {
                        if permanently_blocked {
                            "{policy_name} has published or immutable compliance history that Crystal Forge retains for auditability."
                        } else if !can_delete && eligibility.is_some() {
                            "{policy_name} is still referenced by unpublished draft content. Remove those references first."
                        } else if can_delete {
                            "This policy has never been published and has no retained compliance history."
                        } else {
                            "Checking deletion eligibility…"
                        }
                    }
                }

                div { class: "modal-body",
                    if eligibility_loading {
                        div { style: "color:var(--cf-text-muted);font-size:13px;", "Checking deletion eligibility…" }
                    } else if let Some(elig) = eligibility.as_ref() {
                        if !elig.eligible {
                            div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;",
                                crate::components::icon::Icon { name: crate::components::icon::IconName::Warn, size: 13 }
                                div { style: "font-size:12px;",
                                    for blocker in elig.blockers.iter() {
                                        p { style: "margin:0 0 4px;font-weight:600;", "{blocker.message}" }
                                    }
                                    if permanently_blocked {
                                        p { style: "margin:6px 0 0;color:var(--cf-text-muted);font-size:11px;",
                                            "Remove this policy from future drafts and active assignments. Historical references will remain."
                                        }
                                    }
                                }
                            }
                        } else {
                            div { style: "font-size:13px;color:var(--cf-text-muted);margin-bottom:12px;",
                                "Permanently deleting "
                                span { style: "font-weight:600;color:var(--cf-text-primary);", "{policy_name}" }
                                " removes its unpublished policy versions and mutable import/source mappings. "
                                "This cannot be undone."
                            }
                        }
                    }

                    if let Some(error) = error {
                        div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;", "{error}" }
                    }
                }

                div { class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        disabled: busy,
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    if !permanently_blocked && can_delete {
                        button {
                            class: "btn focus-ring",
                            style: "background:#dc2626;color:white;",
                            disabled: busy,
                            onclick: move |_| on_confirm.call(()),
                            crate::components::icon::Icon { name: crate::components::icon::IconName::Trash, size: 13 }
                            if busy { " Deleting…" } else { " Delete permanently" }
                        }
                    }
                }
            }
        }
    }
}
