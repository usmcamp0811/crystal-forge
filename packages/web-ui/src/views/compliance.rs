use dioxus::prelude::*;

use crate::api::client::{
    create_bundle_draft, create_compliance_assignment, create_compliance_bundle,
    delete_compliance_bundle, fetch_bundle_requirement_coverage,
    fetch_bundle_version_policy_membership, fetch_bundle_version_requirement_membership,
    fetch_compliance_bundle_systems, fetch_compliance_bundles, fetch_compliance_system_evidence,
    fetch_compliance_framework_versions, fetch_compliance_frameworks, fetch_environments,
    fetch_framework_mapped_policy_versions, search_requirements,
    fetch_policies, fetch_systems, import_xccdf, preview_compliance_assignment, preview_xccdf,
    publish_bundle_version, trust_bundle_version, update_compliance_bundle,
};
use crate::api::models::{
    BundleCoverageReport, BundleCoverageRow, ComplianceBundleSummary, ComplianceBundleSystemsResponse,
    ComplianceEvidenceResponse, CreateAssignmentRequest, CreateBundleDraftRequest,
    CreateComplianceBundleRequest, ComplianceFrameworkSummary, ComplianceFrameworkVersionSummary,
    DeploymentPolicySummary, EnvironmentSummary, RequirementVersionSummary,
    ImportedBundlePlan, ImportedCustomCheck, ImportedCustomCheckRule, ImportedEvidenceRequirement,
    ImportedPolicyCustomization, PolicyValueOverride, PublishBundleVersionRequest,
    RequirementCoverage, SortOrder, SystemSummary, SystemsListParams,
    TrustBundleVersionRequest, UpdateAssignmentRequest, UpdateComplianceBundleRequest,
    XccdfImportPlan, XccdfImportResponse, XccdfPreviewResponse, XccdfRuleImportAction,
};
use crate::components::compliance::{
    BundleCatalog, BundleHeader, EvidenceDrawer, ImportReview, RefinePolicyStep,
    RefinedPolicyDraft, RefinedRuleAction, RefinedStigRule, ScoreStrip, SourceCheck,
    SourceCheckBodyPart, SourceStigRule, SystemsMatrix, action_to_import, mapping_semantics_for,
};
use crate::components::icon::{Icon, IconName};
use crate::components::io_menu::{IOMenu, IOMenuItem};
use crate::components::loading::DashboardLoadingSpinner;
use crate::export::{
    ExportPayload, build_cf_json, build_csv, build_oscal, build_sarif, download_print_html,
    trigger_download,
};
use crate::state::{app_state::AppState, auth};

#[component]
pub fn ComplianceView() -> Element {
    // ── RBAC ─────────────────────────────────────────────────────────────────
    // Read-only compliance browsing is available to all authenticated users.
    // Bundle management (create / edit / delete) and Import STIG are restricted
    // to admins, matching the backend RBAC on POST/PUT/DELETE endpoints.
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let is_admin = auth::is_admin(&auth_context);

    // `fetch_started` prevents the effect from re-firing; `loaded` becomes true
    // only after the bundle fetch completes so we never show the empty state
    // while a request is in flight.
    let mut fetch_started = use_signal(|| false);
    let mut loaded = use_signal(|| false);
    let mut bundles = use_signal(Vec::<ComplianceBundleSummary>::new);
    let mut load_error = use_signal(|| None::<String>);
    let mut selected_bundle_id = use_signal(|| None::<uuid::Uuid>);
    // Separate Ok/Err state for systems so failures are surfaced, not swallowed.
    let mut systems = use_signal(|| None::<ComplianceBundleSystemsResponse>);
    let mut systems_error = use_signal(|| None::<String>);
    let mut systems_loading = use_signal(|| false);
    // Separate Ok/Err state for evidence.
    let mut evidence = use_signal(|| None::<ComplianceEvidenceResponse>);
    let mut evidence_error = use_signal(|| None::<String>);
    let mut show_export = use_signal(|| false);
    let mut show_new_bundle = use_signal(|| false);
    let mut show_edit_bundle = use_signal(|| false);
    let mut show_import_stig = use_signal(|| false);
    let mut import_mode_stig = use_signal(|| true); // true = STIG/XCCDF, false = CF-native bundle
    let mut version_action_busy = use_signal(|| false);
    let mut version_action_error = use_signal(|| None::<String>);
    let mut policies = use_signal(Vec::<DeploymentPolicySummary>::new);
    // Requirement coverage for the selected bundle version.
    let mut coverage_report: Signal<Option<BundleCoverageReport>> = use_signal(|| None);
    let mut coverage_expanded = use_signal(|| false);
    let mut environments = use_signal(Vec::<EnvironmentSummary>::new);
    let mut sys_filter = use_signal(|| "all".to_string());
    let mut selected_export_version_id = use_signal(|| None::<uuid::Uuid>);
    let mut export_version_pointers = use_signal(|| (None::<uuid::Uuid>, None::<uuid::Uuid>));
    let mut show_assignment = use_signal(|| false);

    // Generation counters guard against stale async responses overwriting the
    // state of a subsequently-selected bundle or system.  Each spawn captures
    // the current generation before going async; on completion it only commits
    // state if the captured generation still matches the live counter.
    // This covers every spawn site uniformly: initial load, selection,
    // Retry, create, update, and delete callbacks.
    let mut systems_gen = use_signal(|| 0u32);
    let mut evidence_gen = use_signal(|| 0u32);

    // Helper closures (moved into the component body) that bump a generation,
    // spawn the fetch, and only write state when the generation is current.
    // We express them as plain closures captured by the rsx below.

    // Spawn a systems fetch for `bundle_id`.  Increments `systems_gen` and
    // clears existing systems/error state before returning the new generation
    // so the caller can pass it into the async block.
    let mut start_systems_fetch = move |bundle_id: uuid::Uuid, version_id: Option<uuid::Uuid>| {
        let gen_id = *systems_gen.read() + 1;
        systems_gen.set(gen_id);
        systems.set(None);
        systems_error.set(None);
        systems_loading.set(true);
        spawn(async move {
            match fetch_compliance_bundle_systems(&bundle_id, version_id.as_ref()).await {
                Ok(resp) => {
                    if *systems_gen.read() == gen_id {
                        systems.set(Some(resp));
                        systems_error.set(None);
                        systems_loading.set(false);
                    }
                }
                Err(err) => {
                    if *systems_gen.read() == gen_id {
                        systems_error.set(Some(err.to_string()));
                        systems_loading.set(false);
                    }
                }
            }
        });
    };

    use_effect(move || {
        if *fetch_started.read() {
            return;
        }
        fetch_started.set(true);
        spawn(async move {
            match fetch_compliance_bundles().await {
                Ok(items) => {
                    let first_id = items.first().map(|b| b.id);
                    let first_version_id = items.first().and_then(|b| {
                        b.current_published_version_id
                            .or(b.current_draft_version_id)
                    });
                    bundles.set(items);
                    selected_bundle_id.set(first_id);
                    // loaded = true before the systems fetch so the bundle list
                    // renders immediately; systems has its own loading indicator.
                    loaded.set(true);
                    if let Some(bundle_id) = first_id {
                        start_systems_fetch(bundle_id, first_version_id);
                    }
                }
                Err(err) => {
                    load_error.set(Some(err.to_string()));
                    loaded.set(true);
                }
            }
            // Policies/environments are non-critical; silent failure is acceptable
            // since they only populate the New/Edit bundle modal pickers.
            if let Ok(items) = fetch_policies().await {
                policies.set(items);
            }
            if let Ok(items) = fetch_environments().await {
                environments.set(items);
            }
        });
    });

    let selected_bundle = selected_bundle_id
        .read()
        .and_then(|id| bundles.read().iter().find(|b| b.id == id).cloned());

    let on_select_bundle = move |bundle_id: uuid::Uuid| {
        selected_bundle_id.set(Some(bundle_id));
        evidence.set(None);
        evidence_error.set(None);
        // Bump evidence_gen so any in-flight evidence fetch for the old bundle
        // is invalidated even though we already cleared `evidence`.
        let eg = *evidence_gen.read() + 1;
        evidence_gen.set(eg);
        sys_filter.set("all".to_string());
        let version_id = bundles
            .read()
            .iter()
            .find(|bundle| bundle.id == bundle_id)
            .and_then(|bundle| {
                bundle
                    .current_published_version_id
                    .or(bundle.current_draft_version_id)
            });
        start_systems_fetch(bundle_id, version_id);
    };

    use_effect(move || {
        let bundle_id = *selected_bundle_id.read();
        let bundles_snapshot = bundles.read().clone();
        let pointers = bundle_id
            .and_then(|bid| bundles_snapshot.iter().find(|b| b.id == bid))
            .map(|bundle| {
                (
                    bundle.current_published_version_id,
                    bundle.current_draft_version_id,
                )
            })
            .unwrap_or((None, None));
        if *export_version_pointers.read() == pointers {
            return;
        }
        export_version_pointers.set(pointers);

        // Preserve an explicit choice while it remains available. If the
        // selected bundle is refreshed or published and the old version is no
        // longer one of the current pointers, prefer published, then draft.
        let current = *selected_export_version_id.read();
        let version_exists = bundle_id
            .and_then(|bid| bundles_snapshot.iter().find(|bundle| bundle.id == bid))
            .is_some_and(|bundle| {
                current.is_some_and(|id| bundle.versions.iter().any(|v| v.id == id))
            });
        let next = if version_exists {
            current
        } else {
            pointers.0.or(pointers.1).or_else(|| {
                bundle_id.and_then(|bid| {
                    bundles_snapshot
                        .iter()
                        .find(|bundle| bundle.id == bid)
                        .and_then(|bundle| bundle.versions.first().map(|v| v.id))
                })
            })
        };
        selected_export_version_id.set(next);
    });

    let on_evidence = move |system_id: uuid::Uuid| {
        if let Some(bundle_id) = *selected_bundle_id.read() {
            evidence.set(None);
            evidence_error.set(None);
            let gen_id = *evidence_gen.read() + 1;
            evidence_gen.set(gen_id);
            let version_id = *selected_export_version_id.read();
            spawn(async move {
                match fetch_compliance_system_evidence(&bundle_id, &system_id, version_id.as_ref())
                    .await
                {
                    Ok(resp) => {
                        if *evidence_gen.read() == gen_id {
                            evidence.set(Some(resp));
                        }
                    }
                    Err(err) => {
                        if *evidence_gen.read() == gen_id {
                            evidence_error.set(Some(err.to_string()));
                        }
                    }
                }
            });
        }
    };

    rsx! {
        div { style: "display:flex;flex-direction:column;gap:16px;",
            // ── Page head ──────────────────────────────────────────────────
            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Compliance" }
                    p { class: "page-subtitle",
                        "Walk through compliance bundles, review per-control evidence, export for auditors."
                    }
                }
                div { style: "display:flex;gap:8px;align-items:center;",
                    // Admin-only bundle management
                    if is_admin {
                        button {
                            class: "btn btn-primary focus-ring",
                            onclick: move |_| show_new_bundle.set(true),
                            Icon { name: IconName::Plus, size: 14 }
                            " New bundle"
                        }
                    }
                    // Shared Import / Export menu (AC #25)
                    IOMenu {
                        trigger_label: "Import / Export".to_string(),
                        trigger_class: "focus-ring".to_string(),
                        items: {
                            let mut items = vec![];
                            if is_admin {
                                items.push(IOMenuItem::action_with_icon(
                                    "Import STIG or XCCDF (.xml/.zip)",
                                    IconName::Download,
                                ));
                                // Import CF bundle: uses the same import flow as foreign XCCDF.
                                items.push(IOMenuItem::action_with_icon(
                                    "Import Crystal Forge bundle (.xml)",
                                    IconName::Download,
                                ));
                                items.push(IOMenuItem::Separator);
                            }
                            // Export XCCDF: enabled when a bundle version is selected.
                            let bundle_selected = selected_bundle_id.read().is_some();
                            let has_version = selected_export_version_id.read().is_some();
                            let export_label = selected_bundle.as_ref().and_then(|bundle| {
                                let selected = *selected_export_version_id.read();
                                if selected == bundle.current_published_version_id {
                                    bundle.current_published_version.as_ref().map(|version| {
                                        format!("Export XCCDF: {version} published")
                                    })
                                } else if selected == bundle.current_draft_version_id {
                                    bundle.current_draft_version.as_ref().map(|version| {
                                        format!("Export XCCDF: {version} draft")
                                    })
                                } else {
                                    None
                                }
                            }).unwrap_or_else(|| "Export XCCDF".to_string());
                            items.push(if bundle_selected && has_version {
                                IOMenuItem::action_with_icon(
                                    export_label,
                                    IconName::Download,
                                )
                            } else if bundle_selected {
                                IOMenuItem::disabled(
                                    "Export this bundle (XCCDF .xml)",
                                    "No published or draft version available",
                                )
                            } else {
                                IOMenuItem::disabled(
                                    "Export this bundle (XCCDF .xml)",
                                    "Select a bundle first",
                                )
                            });
                            items.push(IOMenuItem::action_with_icon(
                                "Export evidence report…",
                                IconName::Download,
                            ));
                            items
                        },
                        on_action: move |idx: usize| {
                            if is_admin {
                                match idx {
                                    0 => {
                                        import_mode_stig.set(true);
                                        show_import_stig.set(true);
                                    }
                                    1 => {
                                        import_mode_stig.set(false);
                                        show_import_stig.set(true);
                                    }
                                    2 => {
                                        // Export XCCDF: trigger a download of the selected bundle version.
                                        if let Some(vid) = *selected_export_version_id.read() {
                                            let url = format!(
                                                "{}/api/v1/compliance/bundle-versions/{}/xccdf",
                                                crate::api::client::base_url(),
                                                vid
                                            );
                                            if let Some(win) = web_sys::window() {
                                                let _ = win.location().set_href(&url);
                                            }
                                        }
                                    }
                                    3 => show_export.set(true),
                                    _ => {}
                                }
                            } else {
                                match idx {
                                    0 => {
                                        if let Some(vid) = *selected_export_version_id.read() {
                                            let url = format!(
                                                "{}/api/v1/compliance/bundle-versions/{}/xccdf",
                                                crate::api::client::base_url(),
                                                vid
                                            );
                                            if let Some(win) = web_sys::window() {
                                                let _ = win.location().set_href(&url);
                                            }
                                        }
                                    }
                                    1 => show_export.set(true),
                                    _ => {}
                                }
                            }
                        },
                    }
                }
            }

            // ── Body ───────────────────────────────────────────────────────
            if let Some(error) = load_error.read().as_ref() {
                div { class: "sd-callout sd-callout-danger",
                    Icon { name: IconName::X, size: 13 }
                    div { "Failed to load compliance bundles: {error}" }
                }
            } else if !*loaded.read() {
                DashboardLoadingSpinner { label: "Loading compliance…".to_string() }
            } else if bundles.read().is_empty() {
                EmptyComplianceState {
                    is_admin,
                    on_new: move |_| show_new_bundle.set(true),
                }
            } else {
                div {
                    style: "display:grid;grid-template-columns:320px 1fr;gap:16px;align-items:start;",
                    // Left rail: catalog
                     BundleCatalog {
                         bundles: bundles.read().clone(),
                         selected_id: *selected_bundle_id.read(),
                         on_select: on_select_bundle,
                         selected_version_id: *selected_export_version_id.read(),
                         on_select_version: move |version_id| {
                             selected_export_version_id.set(Some(version_id));
                             if let Some(bundle_id) = *selected_bundle_id.read() {
                                 start_systems_fetch(bundle_id, Some(version_id));
                             }
                         },
                     }
                    // Right: bundle content
                    if let Some(bundle) = selected_bundle {
                        div { style: "display:flex;flex-direction:column;gap:14px;min-width:0;",
                             BundleHeader {
                                 bundle: bundle.clone(),
                                 on_edit: move |_| show_edit_bundle.set(true),
                                 is_admin,
                             }
                             XccdfVersionSelector {
                                 bundle: bundle.clone(),
                                 selected_version_id: *selected_export_version_id.read(),
                                   on_select: move |version_id| {
                                       selected_export_version_id.set(version_id);
                                       if let Some(bundle_id) = *selected_bundle_id.read() {
                                           start_systems_fetch(bundle_id, version_id);
                                       }
                                   },
                             }
                             // ── Version lifecycle actions (admin-only) ─
                             if is_admin {
                                 BundleVersionActions {
                                     bundle: bundle.clone(),
                                     selected_version_id: *selected_export_version_id.read(),
                                     busy: *version_action_busy.read(),
                                     error: version_action_error.read().clone(),
                                     on_trust: move |version_id: uuid::Uuid| {
                                         version_action_busy.set(true);
                                         version_action_error.set(None);
                                         spawn(async move {
                                             match trust_bundle_version(
                                                 &version_id,
                                                 &TrustBundleVersionRequest { trusted: true, review_note: None },
                                             ).await {
                                                 Ok(_) => {
                                                     version_action_busy.set(false);
                                                     if let Ok(items) = fetch_compliance_bundles().await {
                                                         bundles.set(items);
                                                     }
                                                 }
                                                 Err(err) => {
                                                     version_action_busy.set(false);
                                                     version_action_error.set(Some(format!("Trust failed: {err}")));
                                                 }
                                             }
                                         });
                                     },
                                     on_publish: move |version_id: uuid::Uuid| {
                                         version_action_busy.set(true);
                                         version_action_error.set(None);
                                         spawn(async move {
                                             match publish_bundle_version(
                                                 &version_id,
                                                 &PublishBundleVersionRequest {
                                                     auto_publish_draft_policies: Some(true),
                                                     expected_semantic_digest: None,
                                                 },
                                             ).await {
                                                 Ok(_) => {
                                                     version_action_busy.set(false);
                                                     if let Ok(items) = fetch_compliance_bundles().await {
                                                         bundles.set(items);
                                                     }
                                                 }
                                                 Err(err) => {
                                                     version_action_busy.set(false);
                                                     version_action_error.set(Some(format!("Publish failed: {err}")));
                                                 }
                                             }
                                         });
                                     },
                                     on_create_draft: move |bundle_id: uuid::Uuid| {
                                         version_action_busy.set(true);
                                         version_action_error.set(None);
                                         spawn(async move {
                                             match create_bundle_draft(
                                                 &bundle_id,
                                                 &CreateBundleDraftRequest { new_version: None },
                                             ).await {
                                                 Ok(draft) => {
                                                     version_action_busy.set(false);
                                                     selected_export_version_id.set(Some(draft.version_id));
                                                     if let Ok(items) = fetch_compliance_bundles().await {
                                                         bundles.set(items);
                                                     }
                                                 }
                                                 Err(err) => {
                                                     version_action_busy.set(false);
                                                     version_action_error.set(Some(format!("Draft creation failed: {err}")));
                                                 }
                                             }
                                         });
                                     },
                                 }
                             }
                             if let Some(err) = systems_error.read().as_ref() {
                                div { class: "sd-callout sd-callout-danger",
                                    Icon { name: IconName::X, size: 13 }
                                    div { style: "font-size:12px;display:flex;flex-direction:column;gap:6px;",
                                        div { "Failed to load systems: {err}" }
                                        button {
                                            class: "btn btn-ghost focus-ring xs",
                                            style: "width:fit-content;",
                            onclick: move |_| {
                             if let Some(bid) = *selected_bundle_id.read() {
                                     start_systems_fetch(bid, *selected_export_version_id.read());
                                }
                            },
                                            Icon { name: IconName::Sync, size: 11 }
                                            " Retry"
                                        }
                                    }
                                }
                            } else if *systems_loading.read() {
                                div { class: "sd-callout sd-callout-info",
                                    Icon { name: IconName::Shield, size: 13 }
                                    div { style: "font-size:12px;", "Loading systems rollup…" }
                                }
                             } else if let Some(resp) = systems.read().as_ref() {
                                  ScoreStrip { totals: resp.totals.clone() }
                                  SystemsMatrix {
                                     systems: resp.systems.clone(),
                                     on_evidence,
                                     filter: sys_filter.read().clone(),
                                      on_filter: move |f| sys_filter.set(f),
                                  }
                                  // ── Requirement coverage card ───────────────
                                  if let Some(vid) = *selected_export_version_id.read() {
                                      {
                                          // Lazy-load coverage when the version changes.
                                          if coverage_report.read().as_ref().map(|r| r.bundle_version_id) != Some(vid) {
                                              spawn(async move {
                                                  if let Ok(report) = fetch_bundle_requirement_coverage(&vid).await {
                                                      coverage_report.set(Some(report));
                                                  }
                                              });
                                          }
                                          rsx! { div {} }
                                      }
                                      if let Some(report) = coverage_report.read().clone() {
                                          RequirementCoverageCard { report, expanded: coverage_expanded }
                                      }
                                  }
                                  // Administrative assignment controls stay below operational posture.
                                  if is_admin {
                                     if let Some(vid) = *selected_export_version_id.read() {
                                          button {
                                              class: "btn btn-primary focus-ring",
                                              onclick: move |_| show_assignment.set(true),
                                              "Assign bundle"
                                          }
                                          if *show_assignment.read() {
                                              AssignmentCreatePanel {
                                                   bundle: bundle.clone(),
                                                   bundle_version_id: vid,
                                                   environments: environments.read().clone(),
                                                   policies: policies.read().clone(),
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

        // ── Evidence drawer ────────────────────────────────────────────────
        if let Some(ev) = evidence.read().as_ref() {
            EvidenceDrawer {
                evidence: ev.clone(),
                bundle_name: selected_bundle_id.read()
                    .and_then(|id| bundles.read().iter().find(|b| b.id == id).map(|b| b.name.clone()))
                    .unwrap_or_default(),
                on_close: move |_| { evidence.set(None); evidence_error.set(None); },
            }
        } else if let Some(err) = evidence_error.read().as_ref() {
            // Evidence fetch failed: show an overlay error so the action
            // doesn't silently appear to do nothing.
            div {
                class: "fl-tray-backdrop",
                onclick: move |_| evidence_error.set(None),
            }
            aside {
                class: "fl-tray",
                style: "width:min(480px,96vw);",
                header { class: "fl-tray-head",
                    span { style: "font-weight:600;", "Failed to load evidence" }
                    button {
                        class: "btn-icon focus-ring",
                        onclick: move |_| evidence_error.set(None),
                        Icon { name: IconName::X, size: 16 }
                    }
                }
                div { style: "padding:20px;display:flex;flex-direction:column;gap:12px;",
                    div { class: "sd-callout sd-callout-danger",
                        Icon { name: IconName::X, size: 13 }
                        div { style: "font-size:12px;", "{err}" }
                    }
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| evidence_error.set(None),
                        "Dismiss"
                    }
                }
            }
        }

        // ── Export modal ────────────────────────────────────────────────────
        if *show_export.read() {
            ExportModal {
                bundles: bundles.read().clone(),
                selected_bundle: selected_bundle_id.read()
                    .and_then(|id| bundles.read().iter().find(|b| b.id == id).cloned()),
                systems_resp: systems.read().clone(),
                environments: environments.read().clone(),
                on_close: move |_| show_export.set(false),
            }
        }

        // ── New bundle modal (admin-only) ──────────────────────────────────
        if is_admin && *show_new_bundle.read() {
            NewBundleModal {
                policies: policies.read().clone(),
                bundles: bundles.read().clone(),
                environments: environments.read().clone(),
                on_close: move |_| show_new_bundle.set(false),
                on_created: move |bundle: ComplianceBundleSummary| {
                    let id = bundle.id;
                    let version_id = bundle.current_published_version_id.or(bundle.current_draft_version_id);
                    let mut next = bundles.read().clone();
                    next.push(bundle);
                    bundles.set(next);
                    selected_bundle_id.set(Some(id));
                    evidence.set(None);
                    evidence_error.set(None);
                    let eg = *evidence_gen.read() + 1;
                    evidence_gen.set(eg);
                    show_new_bundle.set(false);
                    start_systems_fetch(id, version_id);
                },
            }
        }

        // ── Edit bundle modal (admin-only) ─────────────────────────────────
        if is_admin && *show_edit_bundle.read() {
            if let Some(bundle) = selected_bundle_id.read()
                .and_then(|id| bundles.read().iter().find(|b| b.id == id).cloned())
            {
                EditBundleModal {
                    bundle,
                    policies: policies.read().clone(),
                    bundles: bundles.read().clone(),
                    environments: environments.read().clone(),
                    on_close: move |_| show_edit_bundle.set(false),
                    on_saved: move |updated: ComplianceBundleSummary| {
                        let id = updated.id;
                        let version_id = updated.current_published_version_id.or(updated.current_draft_version_id);
                        let mut next = bundles.read().clone();
                        if let Some(pos) = next.iter().position(|b| b.id == id) {
                            next[pos] = updated;
                        }
                        bundles.set(next);
                        show_edit_bundle.set(false);
                        start_systems_fetch(id, version_id);
                    },
                    on_deleted: move |deleted_id: uuid::Uuid| {
                        let mut next = bundles.read().clone();
                        next.retain(|b| b.id != deleted_id);
                        let next_bundle = next.first().cloned();
                        let next_id = next_bundle.as_ref().map(|b| b.id);
                        bundles.set(next);
                        selected_bundle_id.set(next_id);
                        evidence.set(None);
                        evidence_error.set(None);
                        let eg = *evidence_gen.read() + 1;
                        evidence_gen.set(eg);
                        show_edit_bundle.set(false);
                        if let Some(bundle) = next_bundle {
                            start_systems_fetch(
                                bundle.id,
                                bundle.current_published_version_id.or(bundle.current_draft_version_id),
                            );
                        }
                    },
                }
            }
        }

        // ── Import STIG modal (admin-only) ────────────────────────────────
        if is_admin && *show_import_stig.read() {
            ImportStigModal {
                environments: environments.read().clone(),
                is_stig_import: *import_mode_stig.read(),
                on_close: move |_| show_import_stig.set(false),
                on_success: move |_| {
                    // Refresh the bundle catalog after a successful import.
                    spawn(async move {
                        if let Ok(items) = fetch_compliance_bundles().await {
                            let first_id = items.first().map(|b| b.id);
                            bundles.set(items);
                            if let Some(id) = first_id {
                                selected_bundle_id.set(Some(id));
                            }
                        }
                    });
                },
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct XccdfVersionSelectorProps {
    bundle: ComplianceBundleSummary,
    selected_version_id: Option<uuid::Uuid>,
    on_select: EventHandler<Option<uuid::Uuid>>,
}

#[component]
fn XccdfVersionSelector(props: XccdfVersionSelectorProps) -> Element {
    let bundle = &props.bundle;
    let selected = props.selected_version_id;
    let has_version = !bundle.versions.is_empty()
        || bundle.current_published_version_id.is_some()
        || bundle.current_draft_version_id.is_some();

    rsx! {
        if has_version {
            div {
                class: "sd-callout sd-callout-info",
                style: "display:flex;align-items:center;gap:10px;",
                label { style: "font-size:12px;font-weight:600;", "Selected XCCDF revision" }
                select {
                    class: "input focus-ring",
                    style: "width:auto;min-width:260px;",
                    value: selected.map(|id| id.to_string()).unwrap_or_default(),
                    onchange: move |event| {
                        props.on_select.call(uuid::Uuid::parse_str(&event.value()).ok());
                    },
                    for version in bundle.versions.iter() {
                        option {
                            value: "{version.id}",
                            selected: selected == Some(version.id),
                            "{version.version} · {version.publication_state}"
                            if version.is_current_published { " · Current" }
                            if version.is_current_draft { " · Draft" }
                        }
                    }
                }
            }
        } else {
            div {
                class: "sd-callout sd-callout-warning",
                style: "font-size:12px;",
                "No published or draft version is available for XCCDF export."
            }
        }
    }
}

// ─── Bundle version lifecycle actions (trust / publish / draft) ─────────────

#[derive(Props, Clone, PartialEq)]
struct BundleVersionActionsProps {
    bundle: ComplianceBundleSummary,
    selected_version_id: Option<uuid::Uuid>,
    busy: bool,
    error: Option<String>,
    on_trust: EventHandler<uuid::Uuid>,
    on_publish: EventHandler<uuid::Uuid>,
    on_create_draft: EventHandler<uuid::Uuid>,
}

#[component]
fn BundleVersionActions(props: BundleVersionActionsProps) -> Element {
    let bundle = &props.bundle;
    let vid = props.selected_version_id;

    // Determine the publication state of the selected version.
    let is_draft_selected = vid == bundle.current_draft_version_id;
    let is_published_selected = vid == bundle.current_published_version_id;

    if vid.is_none() {
        return rsx! {};
    }

    rsx! {
        div { class: "card", style: "padding:14px 16px;display:flex;flex-direction:column;gap:12px;",
            div { style: "font-size:11px;font-weight:700;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);",
                "Version actions"
            }

            if let Some(err) = &props.error {
                div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
            }

            div { style: "display:flex;flex-wrap:wrap;gap:8px;",
                // ── Trust / untrust (draft versions only) ──────────────
                if is_draft_selected {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        disabled: props.busy,
                        onclick: {
                            let vid = vid.unwrap();
                            move |_| props.on_trust.call(vid)
                        },
                        Icon { name: IconName::Check, size: 12 }
                        " Mark trusted"
                    }
                }

                // ── Publish (draft → accepted) ─────────────────────────
                if is_draft_selected {
                    button {
                        class: "btn btn-primary focus-ring xs",
                        disabled: props.busy,
                        title: "Publish this draft as an immutable accepted version",
                        onclick: {
                            let vid = vid.unwrap();
                            move |_| props.on_publish.call(vid)
                        },
                        if props.busy { "Publishing…" } else { "Publish version" }
                    }
                }

                // ── Create draft (from accepted version) ───────────────
                if is_published_selected {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        disabled: props.busy,
                        title: "Create a new mutable draft derived from this accepted version",
                        onclick: {
                            let bid = bundle.id;
                            move |_| props.on_create_draft.call(bid)
                        },
                        if props.busy { "Creating…" } else { "Create draft" }
                    }

                    div { class: "sd-callout sd-callout-info", style: "font-size:12px;width:100%;",
                        Icon { name: IconName::Shield, size: 12 }
                        "This is an accepted (immutable) version. Edit by creating a new draft."
                    }
                }
            }
        }
    }
}

// ─── Empty state ─────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct EmptyComplianceStateProps {
    on_new: EventHandler<()>,
    #[props(default = false)]
    is_admin: bool,
}

#[component]
fn EmptyComplianceState(props: EmptyComplianceStateProps) -> Element {
    rsx! {
        div { class: "empty",
            h3 { "No compliance bundles" }
            if props.is_admin {
                div {
                    "Create a bundle by grouping deployment policies into a reviewable compliance standard."
                }
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| props.on_new.call(()),
                    Icon { name: IconName::Plus, size: 14 }
                    " New bundle"
                }
            } else {
                div {
                    "No compliance bundles have been configured. Contact an administrator to set up compliance bundles."
                }
            }
        }
    }
}

// ─── Assignment creation panel ───────────────────────────────────────────────

fn parse_uuid_list(value: &str) -> Result<Vec<uuid::Uuid>, ()> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| uuid::Uuid::parse_str(item).map_err(|_| ()))
        .collect()
}

/// Compact panel for creating a bundle assignment for a specific published version.
#[derive(Props, Clone, PartialEq)]
struct AssignmentCreatePanelProps {
    bundle: ComplianceBundleSummary,
    bundle_version_id: uuid::Uuid,
    environments: Vec<EnvironmentSummary>,
    policies: Vec<DeploymentPolicySummary>,
}

#[component]
fn AssignmentCreatePanel(props: AssignmentCreatePanelProps) -> Element {
    let mut scope_type = use_signal(|| "environment".to_string());
    let mut scope_id = use_signal(|| String::new());
    let mut system_search = use_signal(String::new);
    let mut enforcement_mode = use_signal(|| "enforce".to_string());
    let mut exclusions = use_signal(Vec::<uuid::Uuid>::new);
    let mut additions = use_signal(Vec::<uuid::Uuid>::new);
    let mut busy = use_signal(|| false);
    let mut success = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut preview = use_signal(|| None::<crate::api::models::EffectivePolicySetResponse>);
    let mut previewed_request = use_signal(|| None::<CreateAssignmentRequest>);
    let mut preview_busy = use_signal(|| false);

    let membership = use_resource({
        let bundle_version_id = props.bundle_version_id;
        move || async move { fetch_bundle_version_policy_membership(&bundle_version_id).await }
    });

    let systems = use_resource({
        let scope_type = scope_type;
        let system_search = system_search;
        move || {
            let scope_type = scope_type.read().clone();
            let search = system_search.read().trim().to_string();
            async move {
                if scope_type != "system" {
                    return Ok(Vec::<SystemSummary>::new());
                }
                fetch_systems(&SystemsListParams {
                    page: Some(1),
                    per_page: Some(200),
                    search: (!search.is_empty()).then_some(search),
                    health_status: None,
                    deployment_status: None,
                    environment: None,
                    sort_by: Some("hostname".to_string()),
                    sort_order: Some(SortOrder::Asc),
                })
                .await
                .map(|response| response.items)
            }
        }
    });

    let selected_version = props
        .bundle
        .versions
        .iter()
        .find(|version| version.id == props.bundle_version_id);
    let revision_is_current = selected_version
        .is_some_and(|version| version.is_current_published || version.is_current_draft);
    let revision_label = selected_version
        .map(|version| version.version.clone())
        .unwrap_or_else(|| props.bundle.version.clone());
    let revision_state = selected_version
        .map(|version| version.publication_state.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let request = move || {
        let scope_id = uuid::Uuid::parse_str(scope_id.read().trim()).ok()?;
        Some(CreateAssignmentRequest {
            bundle_version_id: props.bundle_version_id,
            scope_type: scope_type.read().clone(),
            scope_id,
            enforcement_mode: Some(enforcement_mode.read().clone()),
            exclusions: (!exclusions.read().is_empty()).then_some(exclusions.read().clone()),
            additions: (!additions.read().is_empty()).then_some(additions.read().clone()),
            value_overrides: None,
        })
    };

    let current_request = request();
    let can_preview = current_request.is_some() && !*preview_busy.read() && !*busy.read();
    let can_submit = current_request.is_some()
        && previewed_request.read().as_ref() == current_request.as_ref()
        && preview.read().is_some()
        && !*busy.read();
    let created_scope_id = uuid::Uuid::parse_str(scope_id.read().trim()).ok();
    let created_scope_type = scope_type.read().clone();
    let exact_members = membership
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();

    rsx! {
        div { class: "card", style: "padding:14px 16px;display:flex;flex-direction:column;gap:12px;",
                 div { style: "font-size:11px;font-weight:700;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);",
                "Assign bundle revision {revision_label}"
            }
            div { style: "font-size:10px;color:var(--cf-text-muted);",
                "Exact revision: " span { class: "mono", "{props.bundle_version_id}" }
                " · {revision_state}"
            }
            if !revision_is_current {
                div { class: "sd-callout sd-callout-warning", style: "font-size:11px;",
                    Icon { name: IconName::Warn, size: 13 }
                    "This is a non-current bundle revision. The assignment will use this exact revision, not the current pointer."
                }
            }

            if *success.read() {
                div { class: "sd-callout sd-callout-success", style: "font-size:12px;",
                    Icon { name: IconName::Check, size: 13 }
                    "Assignment created. The effective policy set is now active for the selected scope."
                }
                if let Some(created_scope_id) = created_scope_id {
                    AssignmentListPanel {
                        scope_type: created_scope_type.clone(),
                        scope_id: created_scope_id,
                    }
                }
            } else {
                if let Some(err) = error.read().as_ref() {
                    div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
                }

                div { style: "display:grid;grid-template-columns:1fr 1fr;gap:10px;",
                    // Scope type
                    div { class: "field",
                        label { "Scope type" }
                        select {
                            class: "input focus-ring",
                            value: "{scope_type.read()}",
                            onchange: move |e| {
                                scope_type.set(e.value());
                                scope_id.set(String::new());
                                preview.set(None);
                                previewed_request.set(None);
                            },
                            option { value: "environment", "Environment" }
                            option { value: "system", "System" }
                        }
                    }

                    // Enforcement mode
                    div { class: "field",
                        label { "Enforcement mode" }
                        select {
                            class: "input focus-ring",
                            value: "{enforcement_mode.read()}",
                            onchange: move |e| {
                                enforcement_mode.set(e.value());
                                preview.set(None);
                                previewed_request.set(None);
                            },
                            option { value: "enforce", "Enforce (default)" }
                            option { value: "report_only", "Report only" }
                        }
                    }
                }
                div { style: "display:grid;grid-template-columns:1fr 1fr;gap:10px;",
                    div { class: "field",
                        label { "Exclude baseline policies" }
                        div { style: "display:flex;flex-direction:column;gap:5px;max-height:130px;overflow:auto;",
                            if exact_members.is_empty() {
                                div { style: "font-size:11px;color:var(--cf-text-muted);", "Loading revision policies…" }
                            } else {
                                for member in exact_members.iter() {
                                    {
                                        let version_id = member.policy_version_id;
                                        let name = member.name.clone();
                                        rsx! {
                                            label { style: "display:flex;gap:6px;align-items:center;font-size:11px;",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: exclusions.read().contains(&version_id),
                                                    onchange: move |event| {
                                                        if event.checked() {
                                                            exclusions.with_mut(|ids| { if !ids.contains(&version_id) { ids.push(version_id); } });
                                                        } else {
                                                            exclusions.with_mut(|ids| ids.retain(|id| *id != version_id));
                                                        }
                                                        preview.set(None);
                                                        previewed_request.set(None);
                                                    },
                                                }
                                                "{name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "field",
                        label { "Add policies" }
                        div { style: "display:flex;flex-direction:column;gap:5px;max-height:130px;overflow:auto;",
                            for policy in props.policies.iter() {
                                if let Some(version_id) = policy.version_id {
                                    label { style: "display:flex;gap:6px;align-items:center;font-size:11px;",
                                        input {
                                            r#type: "checkbox",
                                            checked: additions.read().contains(&version_id),
                                            onchange: move |event| {
                                                if event.checked() {
                                                    additions.with_mut(|ids| { if !ids.contains(&version_id) { ids.push(version_id); } });
                                                } else {
                                                    additions.with_mut(|ids| ids.retain(|id| *id != version_id));
                                                }
                                                preview.set(None);
                                                previewed_request.set(None);
                                            },
                                        }
                                        "{policy.name}"
                                    }
                                }
                            }
                        }
                        if props.policies.iter().all(|policy| policy.version_id.is_none()) {
                            div { style: "font-size:11px;color:var(--cf-text-muted);", "No versioned policies available." }
                        }
                    }
                }

                // Environment picker (when scope is environment)
                if *scope_type.read() == "environment" {
                    div { class: "field",
                        label { "Environment" }
                        select {
                            class: "input focus-ring",
                            value: "{scope_id.read()}",
                            onchange: move |e| {
                                scope_id.set(e.value());
                                preview.set(None);
                                previewed_request.set(None);
                            },
                            option { value: "", "Select an environment…" }
                            for env in &props.environments {
                                option { value: "{env.id}", "{env.name}" }
                            }
                        }
                    }
                } else {
                    div { class: "field",
                        label { "System" }
                        input {
                            class: "input focus-ring",
                            placeholder: "Search by hostname",
                            value: "{system_search.read()}",
                            oninput: move |e| system_search.set(e.value()),
                        }
                        select {
                            class: "input focus-ring",
                            value: "{scope_id.read()}",
                            onchange: move |e| {
                                scope_id.set(e.value());
                                preview.set(None);
                                previewed_request.set(None);
                            },
                            option { value: "", "Select a system…" }
                            match systems.read().as_ref() {
                                Some(Ok(items)) => rsx! {
                                    for system in items {
                                        option { value: "{system.id}", "{system.hostname}" }
                                    }
                                },
                                Some(Err(_)) => rsx! { option { value: "", "Unable to load systems" } },
                                None => rsx! { option { value: "", "Loading systems…" } },
                            }
                        }
                    }
                }

                button {
                    class: "btn btn-ghost focus-ring xs",
                    disabled: !can_preview,
                    style: if !can_preview { "opacity:0.5;cursor:not-allowed;" } else { "" },
                    onclick: move |_| {
                        let Some(req) = request() else { return; };
                        preview_busy.set(true);
                        error.set(None);
                        spawn(async move {
                            match preview_compliance_assignment(&req).await {
                                Ok(value) => {
                                    preview.set(Some(value));
                                    previewed_request.set(Some(req));
                                }
                                Err(err) => error.set(Some(format!("Preview failed: {err}"))),
                            }
                            preview_busy.set(false);
                        });
                    },
                    if *preview_busy.read() { "Previewing…" } else { "Preview effective set" }
                }
                if let Some(value) = preview.read().as_ref() {
                    div { class: "sd-callout sd-callout-info", style: "font-size:11px;",
                        "Preview: {value.policies.len()} effective policies · digest "
                        span { class: "mono", "{value.effective_set_digest}" }
                        if !value.warnings.is_empty() {
                            div { "Warnings: {value.warnings.join(\"; \")}" }
                        }
                    }
                }
                button {
                    class: "btn btn-primary focus-ring xs",
                    disabled: !can_submit,
                    style: if !can_submit { "opacity:0.5;cursor:not-allowed;" } else { "" },
                    onclick: move |_| {
                        if !can_submit { return; }
                         let Some(req) = request() else { return; };
                         busy.set(true);
                        error.set(None);
                        spawn(async move {
                            match create_compliance_assignment(&req).await {
                                Ok(_) => {
                                    busy.set(false);
                                    success.set(true);
                                }
                                Err(err) => {
                                    busy.set(false);
                                    error.set(Some(format!("Assignment failed: {err}")));
                                }
                            }
                        });
                    },
                    if *busy.read() { "Creating…" } else { "Create assignment" }
                }
            }
        }
    }
}

/// Panel listing and managing existing assignments for the selected scope.
#[derive(Props, Clone, PartialEq)]
struct AssignmentListPanelProps {
    scope_type: String,
    scope_id: uuid::Uuid,
}

#[component]
fn AssignmentListPanel(props: AssignmentListPanelProps) -> Element {
    use crate::api::client::{
        compliance_assignment_xccdf_url, delete_compliance_assignment,
        fetch_environment_assignments, fetch_system_assignments,
    };

    let mut assignments = use_signal(Vec::<crate::api::models::AssignmentResponse>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut fetched = use_signal(|| false);
    let mut effective_preview =
        use_signal(|| None::<crate::api::models::EffectivePolicySetResponse>);
    let mut preview_loading = use_signal(|| false);

    if !*fetched.read() {
        fetched.set(true);
        loading.set(true);
        let scope_type = props.scope_type.clone();
        let scope_id = props.scope_id;
        spawn(async move {
            let result = if scope_type == "environment" {
                fetch_environment_assignments(&scope_id).await
            } else {
                fetch_system_assignments(&scope_id).await
            };
            match result {
                Ok(list) => assignments.set(list),
                Err(err) => error.set(Some(err.to_string())),
            }
            loading.set(false);
        });
    }

    if *loading.read() {
        return rsx! { DashboardLoadingSpinner {} };
    }

    let list = assignments.read().clone();
    if list.is_empty() {
        return rsx! {
            div { class: "card", style: "padding:10px 14px;",
                div { style: "font-size:11px;color:var(--cf-text-muted);",
                    "No assignments for this scope."
                }
            }
        };
    }

    rsx! {
        for assignment in list.iter() {
            {
                let assignment_id = assignment.id;
                let current_mode = assignment.enforcement_mode.clone();
                let current_version = assignment.current_version_id;
                let current_exclusions_text = assignment
                    .exclusions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let current_additions_text = assignment
                    .additions
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let current_overrides_text = serde_json::to_string(&assignment.value_overrides)
                    .unwrap_or_else(|_| "[]".to_string());
                let mut deleting = use_signal(|| false);
                let mut editing = use_signal(|| false);
                let mut edit_mode = use_signal(|| current_mode.clone());
                let mut edit_exclusions = use_signal(|| {
                    assignment
                        .exclusions
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                let mut edit_additions = use_signal(|| {
                    assignment
                        .additions
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                let mut edit_overrides = use_signal(|| current_overrides_text.clone());
                let mut edit_busy = use_signal(|| false);
                let mut edit_error = use_signal(|| None::<String>);
                let edits_dirty = *edit_mode.read() != current_mode
                    || *edit_exclusions.read() != current_exclusions_text
                    || *edit_additions.read() != current_additions_text
                    || *edit_overrides.read() != current_overrides_text;
                rsx! {
                    div { class: "card", style: "padding:10px 14px;display:flex;flex-direction:column;gap:6px;",
                        div { style: "display:flex;justify-content:space-between;align-items:center;",
                            div { style: "font-size:12px;font-weight:600;",
                                "Bundle version "
                                span { class: "mono", style: "font-size:10px;", "{assignment.bundle_version_id}" }
                                " · {assignment.enforcement_mode}"
                            }
                            div { style: "display:flex;gap:4px;",
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    style: "font-size:10px;",
                                    disabled: *deleting.read() || *editing.read(),
                                    onclick: move |_| {
                                        let url = compliance_assignment_xccdf_url(&assignment_id);
                                        if let Some(window) = web_sys::window() {
                                            let _ = window.location().set_href(&url);
                                        }
                                    },
                                    "Export effective assignment (XCCDF)"
                                }
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    style: "font-size:10px;",
                                    disabled: *deleting.read() || *editing.read(),
                                    onclick: move |_| editing.set(true),
                                    "Edit mode"
                                }
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    style: "font-size:10px;color:var(--cf-text-muted);",
                                    disabled: *deleting.read(),
                                    onclick: {
                                        let a_id = assignment_id;
                                        move |_| {
                                            deleting.set(true);
                                            spawn(async move {
                                                let _ = delete_compliance_assignment(&a_id).await;
                                                assignments.with_mut(|list| list.retain(|a| a.id != a_id));
                                            });
                                        }
                                    },
                                    if *deleting.read() { "Deactivating…" } else { "Deactivate" }
                                }
                            }
                        }
                        div { style: "font-size:10px;color:var(--cf-text-muted);",
                            "scope: {assignment.scope_type}:{assignment.scope_id}"
                            " · version: " span { class: "mono", "{assignment.current_version_id}" }
                        }
                        if *editing.read() {
                            if let Some(err) = edit_error.read().as_ref() {
                                div { class: "sd-callout sd-callout-danger", style: "font-size:11px;", "{err}" }
                            }
                            div { style: "display:flex;gap:6px;align-items:center;",
                                select {
                                    class: "input xs",
                                    style: "flex:1;",
                                    value: "{edit_mode.read()}",
                                    onchange: move |e| edit_mode.set(e.value()),
                                    option { value: "enforce", "Enforce" }
                                    option { value: "report_only", "Report only" }
                                }
                                input {
                                    class: "input xs mono",
                                    style: "flex:1;",
                                    placeholder: "excluded version UUIDs",
                                    value: "{edit_exclusions.read()}",
                                    oninput: move |e| edit_exclusions.set(e.value()),
                                }
                                input {
                                    class: "input xs mono",
                                    style: "flex:1;",
                                    placeholder: "added version UUIDs",
                                    value: "{edit_additions.read()}",
                                    oninput: move |e| edit_additions.set(e.value()),
                                }
                                textarea {
                                    class: "input xs mono",
                                    style: "flex:1;",
                                    rows: "2",
                                    placeholder: "typed overrides JSON",
                                    value: "{edit_overrides.read()}",
                                    oninput: move |e| edit_overrides.set(e.value()),
                                }
                                button {
                                    class: "btn btn-primary xs focus-ring",
                                    style: "font-size:10px;",
                                    disabled: *edit_busy.read() || !edits_dirty,
                                    onclick: {
                                        let a_id = assignment_id;
                                        let cm = current_version;
                                        let scope_type = props.scope_type.clone();
                                        let scope_id = props.scope_id;
                                        let exclusions_text = edit_exclusions.read().clone();
                                        let additions_text = edit_additions.read().clone();
                                        let overrides_text = edit_overrides.read().clone();
                                        move |_| {
                                            let Ok(exclusions) = parse_uuid_list(&exclusions_text) else {
                                                edit_error.set(Some("Exclusions must be comma-separated UUIDs".to_string()));
                                                return;
                                            };
                                            let Ok(additions) = parse_uuid_list(&additions_text) else {
                                                edit_error.set(Some("Additions must be comma-separated UUIDs".to_string()));
                                                return;
                                            };
                                            let Ok(value_overrides) = serde_json::from_str::<Vec<PolicyValueOverride>>(&overrides_text) else {
                                                edit_error.set(Some("Overrides must be a JSON array of typed values".to_string()));
                                                return;
                                            };
                                            edit_busy.set(true);
                                            edit_error.set(None);
                                            let request = UpdateAssignmentRequest {
                                                expected_version_id: cm,
                                                enforcement_mode: Some((*edit_mode.read()).clone()),
                                                exclusions: Some(exclusions),
                                                additions: Some(additions),
                                                value_overrides: Some(value_overrides),
                                            };
                                            let st = scope_type.clone();
                                            let si = scope_id;
                                            spawn(async move {
                                                match crate::api::client::update_compliance_assignment(&a_id, &request).await {
                                                    Ok(_) => {
                                                        edit_busy.set(false);
                                                        editing.set(false);
                                                        spawn(async move {
                                                            let result = if st == "environment" {
                                                                fetch_environment_assignments(&si).await
                                                            } else {
                                                                fetch_system_assignments(&si).await
                                                            };
                                                            if let Ok(list) = result {
                                                                assignments.set(list);
                                                            }
                                                        });
                                                    }
                                                    Err(e) => {
                                                        edit_busy.set(false);
                                                        edit_error.set(Some(format!("Update failed: {e}")));
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    if *edit_busy.read() { "Saving…" } else { "Save" }
                                }
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    style: "font-size:10px;",
                                    disabled: *edit_busy.read(),
                                    onclick: move |_| editing.set(false),
                                    "Cancel"
                                }
                            }
                        }
                        button {
                            class: "btn btn-ghost xs focus-ring",
                            disabled: *preview_loading.read(),
                            onclick: {
                                let bundle_version_id = assignment.bundle_version_id;
                                let scope_type = assignment.scope_type.clone();
                                let scope_id = assignment.scope_id;
                                let mode = edit_mode.read().clone();
                                let exclusions_text = edit_exclusions.read().clone();
                                let additions_text = edit_additions.read().clone();
                                let overrides_text = edit_overrides.read().clone();
                                move |_| {
                                    let Ok(exclusions) = parse_uuid_list(&exclusions_text) else {
                                        edit_error.set(Some("Exclusions must be comma-separated UUIDs".to_string()));
                                        return;
                                    };
                                    let Ok(additions) = parse_uuid_list(&additions_text) else {
                                        edit_error.set(Some("Additions must be comma-separated UUIDs".to_string()));
                                        return;
                                    };
                                    let Ok(value_overrides) = serde_json::from_str::<Vec<PolicyValueOverride>>(&overrides_text) else {
                                        edit_error.set(Some("Overrides must be a JSON array of typed values".to_string()));
                                        return;
                                    };
                                    let request = CreateAssignmentRequest {
                                        bundle_version_id,
                                        scope_type: scope_type.clone(),
                                        scope_id,
                                        enforcement_mode: Some(mode.clone()),
                                        exclusions: Some(exclusions),
                                        additions: Some(additions),
                                        value_overrides: Some(value_overrides),
                                    };
                                    preview_loading.set(true);
                                    spawn(async move {
                                        match preview_compliance_assignment(&request).await {
                                            Ok(value) => effective_preview.set(Some(value)),
                                            Err(error) => edit_error.set(Some(format!("Preview failed: {error}"))),
                                        }
                                        preview_loading.set(false);
                                    });
                                }
                            },
                            if *preview_loading.read() { "Previewing…" } else { "Preview unsaved effective set" }
                        }
                        if let Some(preview) = effective_preview.read().as_ref() {
                            div { class: "sd-callout sd-callout-info", style: "font-size:10px;",
                                "Effective set: {preview.policies.len()} policies · digest "
                                span { class: "mono", "{preview.effective_set_digest}" }
                                if !preview.warnings.is_empty() {
                                    div { "Warnings: {preview.warnings.join(\"; \" )}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─── Import STIG modal ────────────────────────────────────────────────────────
//
// Implements the 4-step design reference (upload → review → commit → done).
// Uses the real /api/v1/compliance/xccdf/preview and /xccdf/import endpoints.

/// Local rule selection state derived from the XCCDF preview response.
#[derive(Clone, PartialEq)]
struct StigRule {
    rule_id: String,
    vulnerability_id: String,
    source_description: String,
    group_id: String,
    severity: String, // "high" | "medium" | "low"
    title: String,
    fixtext: String,
    check: String,
    srg: String,
    srg_ids: Vec<String>,
    cci_ids: Vec<String>,
    checks: Vec<SourceCheck>,
    references: Vec<String>,
    platforms: Vec<String>,
    selected: bool,
    is_native: bool,
    action: String,
    local_name: String,
    local_description: String,
    implementation_note: String,
    assertion_mode: String,
    assertions: Vec<ImportedCustomCheckRule>,
    evidence_requirements: Vec<ImportedEvidenceRequirement>,
    mapped_policy_version_id: Option<uuid::Uuid>,
}

/// Convert the server's ordered source check parts without changing the XCCDF
/// representation held by the import plan.
fn source_check_body_parts(check: &serde_json::Value) -> Vec<SourceCheckBodyPart> {
    let parts = check.get("body_parts").and_then(|value| value.as_array());
    let Some(parts) = parts else {
        return check
            .get("inline_content")
            .or_else(|| check.get("content"))
            .and_then(|value| value.as_str())
            .map(|content| vec![SourceCheckBodyPart::Inline(content.to_string())])
            .unwrap_or_default();
    };

    parts
        .iter()
        .filter_map(
            |part| match part.get("type").and_then(|value| value.as_str()) {
                Some("inline") => part
                    .get("content")
                    .or_else(|| part.get("preview"))
                    .and_then(|value| value.as_str())
                    .map(|content| SourceCheckBodyPart::Inline(content.to_string())),
                Some("reference") => {
                    part.get("href")
                        .and_then(|value| value.as_str())
                        .map(|href| SourceCheckBodyPart::Reference {
                            href: href.to_string(),
                            name: part
                                .get("name")
                                .and_then(|value| value.as_str())
                                .map(str::to_string),
                        })
                }
                _ => None,
            },
        )
        .collect()
}

fn rules_from_preview(preview: &XccdfPreviewResponse) -> Vec<StigRule> {
    preview
        .rules
        .iter()
        .map(|r| {
            // Build a summary of identifiers for display
            let identifier_values = r
                .identifiers
                .iter()
                .filter_map(|i| {
                    i.get("value")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>();
            let vulnerability_id = identifier_values
                .iter()
                .find(|value| value.starts_with("V-"))
                .cloned()
                .unwrap_or_default();
            let srg_ids = identifier_values
                .iter()
                .filter(|value| value.starts_with("SRG-"))
                .cloned()
                .collect::<Vec<_>>();
            let cci_ids = identifier_values
                .iter()
                .filter(|value| value.starts_with("CCI-"))
                .cloned()
                .collect::<Vec<_>>();
            let srg = srg_ids.first().cloned().unwrap_or_default();

            let check_summary = r
                .checks
                .first()
                .map(|c| {
                    let sys = c.get("system").and_then(|v| v.as_str()).unwrap_or("");
                    sys.to_string()
                })
                .unwrap_or_default();

            let checks = r
                .checks
                .iter()
                .map(|check| SourceCheck {
                    system: check
                        .get("system")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    selector: check
                        .get("selector")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    references: check
                        .get("references")
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| item.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    body_parts: source_check_body_parts(check),
                })
                .collect::<Vec<_>>();

            // Use "content" (full text) if available; fall back to "preview" for
            // backward compatibility with server responses that only had truncated text.
            let fix_text = r
                .fix
                .as_ref()
                .and_then(|f| {
                    f.get("content")
                        .or_else(|| f.get("preview"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("")
                .to_string();

            // Foreign source remains non-executable. The server may provide
            // conservative structured suggestions from recognized fix literals.
            let assertions: Vec<ImportedCustomCheckRule> = r
                .inferred_assertions
                .iter()
                .filter_map(|a| {
                    Some(ImportedCustomCheckRule {
                        field_name: a.get("option_path")?.as_str()?.replace('.', "_"),
                        expression: a.get("nix_expression")?.as_str()?.to_string(),
                        description: a
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Inferred from official fix; review before importing.")
                            .to_string(),
                        strict: true,
                    })
                })
                .collect();
            let default_action = if r.is_native || !assertions.is_empty() {
                "native"
            } else {
                "unbound"
            };

            StigRule {
                rule_id: r.id.clone(),
                vulnerability_id,
                source_description: r.description.clone().unwrap_or_default(),
                group_id: r.group_id.clone().unwrap_or_default(),
                severity: r.severity.as_deref().unwrap_or("medium").to_string(),
                title: r.title.as_deref().unwrap_or(&r.id).to_string(),
                fixtext: fix_text,
                check: check_summary,
                srg,
                srg_ids,
                cci_ids,
                checks,
                references: r
                    .references
                    .iter()
                    .filter_map(|reference| {
                        reference
                            .get("href")
                            .or_else(|| reference.get("value"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    })
                    .collect(),
                platforms: r.platforms.clone(),
                selected: true,
                is_native: r.is_native,
                action: default_action.to_string(),
                local_name: r.title.as_deref().unwrap_or(&r.id).to_string(),
                local_description: r.description.clone().unwrap_or_default(),
                implementation_note: String::new(),
                assertion_mode: "all".to_string(),
                assertions,
                evidence_requirements: Vec::new(),
                mapped_policy_version_id: None,
            }
        })
        .collect()
}

fn sev_color(sev: &str) -> &'static str {
    match sev {
        "high" => "#f87171",
        "medium" => "#fbbf24",
        _ => "#60a5fa",
    }
}
fn sev_cat(sev: &str) -> &'static str {
    match sev {
        "high" => "CAT I",
        "medium" => "CAT II",
        _ => "CAT III",
    }
}
fn sev_label(sev: &str) -> &'static str {
    match sev {
        "high" => "High",
        "medium" => "Medium",
        _ => "Low",
    }
}

fn human_document_class(value: Option<&str>) -> &'static str {
    match value.unwrap_or_default() {
        "foreign" | "foreign_xccdf" => "Foreign XCCDF",
        "cf_native_exact" => "CF-native exact",
        "cf_native" => "CF-native",
        _ => "XCCDF document",
    }
}

fn human_fidelity(value: Option<&str>) -> &'static str {
    match value.unwrap_or_default() {
        "preserved_opaque" => "Preserved as opaque",
        "native_exact" => "Native exact",
        "normalized_complete" => "Normalized complete",
        "degraded" => "Degraded",
        _ => "Not classified",
    }
}

fn reconciliation_match_label(match_type: &str) -> &'static str {
    match match_type {
        "authoritative_mapping" => "Existing mapping",
        "inherited_mapping" => "Inherited mapping",
        "exact_technical_match" => "Exact technical match",
        "related_mapping" => "Related candidate",
        "fuzzy_similarity" => "Similarity candidate",
        _ => "Candidate",
    }
}

fn reconciliation_match_class(match_type: &str) -> &'static str {
    match match_type {
        "authoritative_mapping" | "inherited_mapping" | "exact_technical_match" => "chip chip-success",
        "related_mapping" | "fuzzy_similarity" => "chip chip-warning",
        _ => "chip chip-unknown",
    }
}

fn shared_reconciliation_action_label(action: &str) -> &'static str {
    match action {
        "reuse_existing" => "Reuse existing",
        "create_shared" => "Create one policy",
        _ => "Review individually",
    }
}

fn shared_reconciliation_requirement_keys(keys: &[String]) -> String {
    keys.join(", ")
}

fn import_action_from_rule(rule: &StigRule) -> XccdfRuleImportAction {
    let customization = ImportedPolicyCustomization {
        policy_name: Some(rule.local_name.clone()),
        policy_description: Some(rule.local_description.clone()),
        implementation_note: (!rule.implementation_note.trim().is_empty())
            .then(|| rule.implementation_note.clone()),
        policy_severity: Some(rule.severity.clone()),
        policy_rationale: (!rule.fixtext.trim().is_empty()).then(|| rule.fixtext.clone()),
    };
    match rule.action.as_str() {
        "native" => XccdfRuleImportAction::CreateNativeCustom {
            rule_id: rule.rule_id.clone(),
            customization,
            custom_check: ImportedCustomCheck {
                mode: rule.assertion_mode.clone(),
                rules: rule.assertions.clone(),
            },
            evidence_requirements: rule.evidence_requirements.clone(),
        },
        "manual" => XccdfRuleImportAction::CreateManual {
            rule_id: rule.rule_id.clone(),
            customization,
            evidence_requirements: rule.evidence_requirements.clone(),
        },
        "opaque" => XccdfRuleImportAction::PreserveOpaque {
            rule_id: rule.rule_id.clone(),
            customization,
        },
        "exclude" => XccdfRuleImportAction::Exclude {
            rule_id: rule.rule_id.clone(),
        },
        _ => XccdfRuleImportAction::CreateUnbound {
            rule_id: rule.rule_id.clone(),
            customization,
        },
    }
}

fn refined_rules_from_rules(rules: &[StigRule]) -> Vec<RefinedStigRule> {
    rules.iter().enumerate().map(|(rule_order, rule)| RefinedStigRule {
        source: SourceStigRule {
            rule_id: rule.rule_id.clone(),
            group_id: (!rule.group_id.is_empty()).then(|| rule.group_id.clone()),
            stig_id: (!rule.vulnerability_id.is_empty()).then(|| rule.vulnerability_id.clone()),
            title: Some(rule.title.clone()),
            description: (!rule.source_description.is_empty()).then(|| rule.source_description.clone()),
            source_severity: Some(rule.severity.clone()),
            fix_text: (!rule.fixtext.is_empty()).then(|| rule.fixtext.clone()),
            checks: if rule.checks.is_empty() && !rule.check.is_empty() {
                vec![SourceCheck { system: String::new(), selector: None, references: Vec::new(), body_parts: vec![SourceCheckBodyPart::Inline(rule.check.clone())] }]
            } else {
                rule.checks.clone()
            },
            identifiers: std::iter::once(rule.vulnerability_id.clone())
                .chain(rule.srg_ids.iter().cloned())
                .chain(rule.cci_ids.iter().cloned())
                .filter(|identifier| !identifier.is_empty())
                .collect(),
            references: rule.references.clone(),
            platforms: rule.platforms.clone(),
            rule_order,
        },
        draft: RefinedPolicyDraft {
            local_name: rule.local_name.clone(),
            local_description: rule.local_description.clone(),
            local_severity: rule.severity.clone(),
            local_rationale: rule.fixtext.clone(),
            implementation_note: rule.implementation_note.clone(),
            action: match rule.action.as_str() { "native" => RefinedRuleAction::Native, "manual" => RefinedRuleAction::Manual, "opaque" => RefinedRuleAction::Opaque, _ => RefinedRuleAction::Unbound },
            assertion_mode: rule.assertion_mode.clone(),
            assertions: rule.assertions.iter().map(|assertion| crate::components::compliance::refine_policy::PolicyAssertionDraft::CustomExpression { field_name: assertion.field_name.clone(), expression: assertion.expression.clone(), failure_message: assertion.description.clone(), strict: assertion.strict }).collect(),
            evidence_requirements: rule.evidence_requirements.iter().filter_map(|evidence| match evidence { ImportedEvidenceRequirement::Command { command, expected_output } => Some(crate::components::compliance::refine_policy::EvidenceRequirementDraft::Command { command: command.clone(), expected_output: expected_output.clone() }), ImportedEvidenceRequirement::Attestation { description } => Some(crate::components::compliance::refine_policy::EvidenceRequirementDraft::Attestation { description: description.clone() }), _ => None }).collect(),
        },
        selected: rule.selected,
        mapping_relationship: None,
        mapping_coverage: None,
        mapping_rationale: None,
        candidate_options: vec![],
        selected_candidate: None,
    }).collect()
}

#[derive(Props, Clone, PartialEq)]
struct ImportStigModalProps {
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
    is_stig_import: bool,
}

#[component]
fn ImportStigModal(props: ImportStigModalProps) -> Element {
    // step: "upload" | "review" | "refine" | "committing" | "done"
    let mut step = use_signal(|| "upload".to_string());
    let mut rules = use_signal(|| Vec::<StigRule>::new());
    let mut refined_rules = use_signal(|| Vec::<RefinedStigRule>::new());
    let mut mapping_semantics = use_signal(
        std::collections::HashMap::<String, crate::api::models::ImportedMappingSemantics>::new,
    );
    let mut bundle_name = use_signal(String::new);
    let mut file_name = use_signal(String::new);
    let mut bench_title = use_signal(String::new);
    let mut bench_ver = use_signal(String::new);
    let mut selected_envs = use_signal(|| Vec::<String>::new());
    let mut cursor = use_signal(|| 0usize);
    // Source rule IDs shown in the refine walkthrough.  All selected controls
    // remain in the import plan; this only narrows the human-review path.
    let mut refine_rule_ids = use_signal(Vec::<String>::new);
    let mut parse_error = use_signal(|| Option::<String>::None);
    // done-step summary
    let mut done_total = use_signal(|| 0usize);

    // Real API state
    let mut file_bytes = use_signal(|| Vec::<u8>::new());
    let mut preview_response = use_signal(|| None::<XccdfPreviewResponse>);
    let mut import_result = use_signal(|| None::<XccdfImportResponse>);
    let mut previewing = use_signal(|| false);
    let mut committing = use_signal(|| false);
    let mut import_error = use_signal(|| None::<String>);

    let all_env_names: Vec<String> = props.environments.iter().map(|e| e.name.clone()).collect();

    // Derived counts
    let selected_rules: Vec<StigRule> = rules
        .read()
        .iter()
        .filter(|r| r.selected)
        .cloned()
        .collect();
    let sel_count = selected_rules.len();
    let total_count = rules.read().len();

    let counts: Vec<(&'static str, usize, usize)> = ["high", "medium", "low"]
        .iter()
        .map(|&s| {
            let n = rules.read().iter().filter(|r| r.severity == s).count();
            let sel = rules
                .read()
                .iter()
                .filter(|r| r.severity == s && r.selected)
                .count();
            (s, n, sel)
        })
        .collect();

    // can_advance: need at least one rule selected and a bundle name.
    // Env selection is only required when environments actually exist — if the
    // server has no environments yet the user can still proceed.
    let can_advance = sel_count > 0
        && !bundle_name.read().trim().is_empty()
        && (props.environments.is_empty() || !selected_envs.read().is_empty());
    let reconciliation_rows = preview_response
        .read()
        .as_ref()
        .and_then(|preview| preview.foreign_stig_reconciliation.as_ref())
        .map(|reconciliation| reconciliation.requirements.clone())
        .unwrap_or_default();
    let selected_rule_ids: std::collections::HashSet<String> = selected_rules
        .iter()
        .map(|rule| rule.rule_id.clone())
        .collect();
    let reconciliation_rows: Vec<_> = reconciliation_rows
        .into_iter()
        .filter(|row| selected_rule_ids.contains(&row.rule_id))
        .collect();
    let shared_groups = preview_response
        .read()
        .as_ref()
        .and_then(|preview| preview.foreign_stig_reconciliation.as_ref())
        .map(|reconciliation| reconciliation.shared_implementation_groups.clone())
        .unwrap_or_default();
    let framework_reconciliation = preview_response
        .read()
        .as_ref()
        .and_then(|preview| preview.foreign_stig_reconciliation.as_ref())
        .map(|reconciliation| reconciliation.framework.clone());
    let framework_state_label = framework_reconciliation
        .as_ref()
        .map(|framework| {
            if framework.state == "exact_release" {
                "Exact release"
            } else {
                "Release requires review"
            }
        });
    let attention_rule_ids: Vec<String> = reconciliation_rows
        .iter()
        .filter(|row| !row.auto_resolvable || row.state == "identity_conflict")
        .map(|row| row.rule_id.clone())
        .collect();
    let reused_count = reconciliation_rows
        .iter()
        .filter(|row| !row.candidates.is_empty())
        .count();
    let authoritative_count = reconciliation_rows
        .iter()
        .filter(|row| row.candidates.iter().any(|candidate| candidate.match_type == "authoritative_mapping"))
        .count();
    let inherited_count = reconciliation_rows
        .iter()
        .filter(|row| row.candidates.iter().any(|candidate| candidate.match_type == "inherited_mapping"))
        .count();
    let exact_count = reconciliation_rows
        .iter()
        .filter(|row| row.candidates.iter().any(|candidate| candidate.match_type == "exact_technical_match"))
        .count();
    let ready_count = reconciliation_rows
        .iter()
        .filter(|row| row.candidates.is_empty() && row.auto_resolvable)
        .count();
    let review_count = if attention_rule_ids.is_empty() {
        sel_count
    } else {
        attention_rule_ids.len()
    };
    let refine_all_rule_ids: Vec<String> = selected_rules
        .iter()
        .map(|rule| rule.rule_id.clone())
        .collect();
    let fallback_review_rule_ids = refine_all_rule_ids.clone();

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| props.on_close.call(()),
            div {
                 class: "modal import-workflow-modal",
                style: "width:min(720px,97vw);max-height:92vh;display:flex;flex-direction:column;",
                onclick: move |e| e.stop_propagation(),

                // ══════════════════════════════════════════════════════
                // STEP: upload
                // ══════════════════════════════════════════════════════
                if *step.read() == "upload" {
                    div { class: "modal-head",
                        h2 { style: "display:flex;align-items:center;gap:8px;",
                            span { style: "display:inline-flex;transform:rotate(180deg);",
                                Icon { name: IconName::Download, size: 14 }
                            }
                            if props.is_stig_import {
                                "Import STIG / XCCDF"
                            } else {
                                "Import Crystal Forge bundle"
                            }
                        }
                        p {
                            if props.is_stig_import {
                                "Upload a DISA XCCDF benchmark (.xml or .zip). Crystal Forge parses each rule into a policy and assembles them into a compliance bundle."
                            } else {
                                "Upload a Crystal Forge XCCDF bundle export (.xml). Existing policy versions are reused; new ones are created as drafts."
                            }
                        }
                    }
                    div { class: "modal-body",
                        if *previewing.read() {
                            div { style: "text-align:center;padding:32px 0;",
                                DashboardLoadingSpinner {}
                                div { style: "font-size:13px;color:var(--cf-text-muted);margin-top:12px;",
                                    "Parsing XCCDF document…"
                                }
                            }
                        } else {
                            // ── File input ────────────────────────────────
                            label {
                                class: "focus-ring",
                                style: "display:block;border:2px dashed var(--cf-divider);background:var(--cf-card-bg);\
                                        border-radius:12px;padding:38px 20px;text-align:center;cursor:pointer;",
                                input {
                                    r#type: "file",
                                    accept: ".xml,.zip",
                                    style: "display:none;",
                                    onchange: move |event| {
                                        let mut parse_error = parse_error;
                                        let mut file_bytes = file_bytes;
                                        let mut file_name = file_name;
                                        let mut preview_response = preview_response;
                                        let mut rules = rules;
                                        let mut bundle_name = bundle_name;
                                        let mut bench_title = bench_title;
                                        let mut bench_ver = bench_ver;
                                        let mut selected_envs = selected_envs;
                                        let mut previewing = previewing;
                                        let mut step = step;
                                        let all_env_names = all_env_names.clone();

                                        parse_error.set(None);
                                        let files = event.files();
                                        if let Some(file) = files.into_iter().next() {
                                            let fname = file.name();
                                            previewing.set(true);
                                            spawn(async move {
                                                match file.read_bytes().await {
                                                    Ok(bytes) => {
                                                        let bytes_vec = bytes.to_vec();
                                                        match preview_xccdf(&bytes_vec, &fname).await {
                                                            Ok(resp) => {
                                                                let bm_title = resp.benchmark.as_ref()
                                                                    .and_then(|b| b.title.clone())
                                                                    .unwrap_or_else(|| fname.clone());
                                                                let bm_ver = resp.benchmark.as_ref()
                                                                    .and_then(|b| b.version.clone())
                                                                    .unwrap_or_default();
                                                                 // Branch on document class
                                                                 let is_cf_native = resp.document_class.as_deref().unwrap_or("") == "cfnativeexact";
                                                                 
                                                                 bundle_name.set(bm_title.clone());
                                                                 bench_title.set(bm_title);
                                                                 bench_ver.set(bm_ver);
                                                                 file_name.set(fname.clone());
                                                                 file_bytes.set(bytes_vec);
                                                                  let foreign_reconciliation = resp.foreign_stig_reconciliation.clone();
                                                                  preview_response.set(Some(resp));
                                                                 previewing.set(false);
                                                                 
                                                                 if is_cf_native {
                                                                     // Native workflow: skip rule refinement
                                                                     step.set("native-review".to_string());
                                                                 } else {
                                                                     // Foreign XCCDF workflow: do rule processing
                                                                      let parsed_rules = rules_from_preview(&preview_response.read().as_ref().unwrap());
                                                                      rules.set(parsed_rules.clone());
                                                                      let mut refined = refined_rules_from_rules(&parsed_rules);
                                                                      if let Some(reconciliation) = foreign_reconciliation.as_ref() {
                                                                          for rule in refined.iter_mut() {
                                                                              if let Some(requirement) = reconciliation.requirements.iter().find(|requirement| requirement.rule_id == rule.source.rule_id) {
                                                                                  rule.candidate_options = requirement.candidates.clone();
                                                                              }
                                                                          }
                                                                      }
                                                                      refined_rules.set(refined);
                                                                      mapping_semantics.set(
                                                                      foreign_reconciliation
                                                                          .as_ref()
                                                                          .into_iter()
                                                                          .flat_map(|reconciliation| reconciliation.requirements.iter())
                                                                          .filter_map(|requirement| {
                                                                              requirement.candidates.first().map(|candidate| {
                                                                                  (
                                                                                      requirement.rule_id.clone(),
                                                                                       crate::api::models::ImportedMappingSemantics {
                                                                                           relationship: None,
                                                                                           coverage: None,
                                                                                           rationale: Some(candidate.match_reasons.join(" ")),
                                                                                           reviewed_related_candidate: None,
                                                                                      },
                                                                                  )
                                                                              })
                                                                          })
                                                                          .collect(),
                                                                  );
                                                                     selected_envs.set(all_env_names.clone());
                                                                     step.set("review".to_string());
                                                                 }
                                                            }
                                                            Err(err) => {
                                                                previewing.set(false);
                                                                parse_error.set(Some(format!("Preview failed: {err}")));
                                                            }
                                                        }
                                                    }
                                                    Err(err) => {
                                                        previewing.set(false);
                                                        parse_error.set(Some(format!("Could not read file: {err}")));
                                                    }
                                                }
                                            });
                                        }
                                    },
                                }
                                div { style: "font-size:30px;margin-bottom:8px;", "📄" }
                                div { style: "font-size:14px;font-weight:600;",
                                    "Click to browse for an XCCDF .xml or .zip"
                                }
                                div { style: "font-size:12px;color:var(--cf-text-muted);margin-top:4px;",
                                    "DISA STIG / SCAP benchmark"
                                }
                            }

                            if let Some(err) = parse_error.read().as_ref() {
                                div { class: "sd-callout sd-callout-danger", style: "margin-top:12px;",
                                    Icon { name: IconName::Warn, size: 13 }
                                    div { style: "font-size:12px;", "{err}" }
                                }
                            }
                        }
                    }
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| props.on_close.call(()),
                            "Cancel"
                        }
                    }
                }

                // ══════════════════════════════════════════════════════
                // STEP: native-review (CF-native only)
                // ══════════════════════════════════════════════════════
                if *step.read() == "native-review" {
                    if let Some(prev) = preview_response.read().as_ref() {
                        if let Some(recon) = prev.cf_native_reconciliation.as_ref() {
                            div { class: "modal-head",
                                h2 { style: "display:flex;align-items:center;gap:8px;",
                                    Icon { name: IconName::Shield, size: 14 }
                                    "Import Crystal Forge bundle"
                                }
                                p {
                                    span { class: "mono", "{file_name.read()}" }
                                    " · SHA: "
                                    span { class: "mono", "{prev.sha256[..16].to_string()}…" }
                                }
                            }
                            div { class: "modal-body",
                                div { class: "card", style: "padding:12px;border-left:3px solid var(--cf-info-color);",
                                    "Source: {prev.xccdf_version.as_deref().unwrap_or(\"XCCDF\")} · Fidelity: {human_fidelity(prev.fidelity.as_deref())}"
                                }
                                div { style: "margin-top:12px;font-weight:600;", "Bundle: " span { "{recon.bundle.name} v{recon.bundle.version}" } }
                                div { style: "margin-top:8px;font-size:12px;", 
                                    "Exact: {recon.policies.iter().filter(|p| p.reconciliation_state == \"exact_match\").count()} · "
                                    "New version: {recon.policies.iter().filter(|p| p.reconciliation_state == \"new_version\").count()} · "
                                    "New policy: {recon.policies.iter().filter(|p| p.reconciliation_state == \"new_lineage\").count()}"
                                }
                                if recon.has_blocking_conflicts {
                                    div { class: "sd-callout sd-callout-danger", style: "margin-top:12px;",
                                        Icon { name: IconName::Warn, size: 13 }
                                        "Blocking conflicts must be resolved before import"
                                    }
                                }
                            }
                            div { class: "modal-foot",
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    onclick: move |_| step.set("upload".to_string()),
                                    "Cancel"
                                }
                                button {
                                    class: "btn btn-primary focus-ring",
                                    disabled: recon.has_blocking_conflicts,
                                    onclick: move |_| {
                                        let mut step = step;
                                        let mut committing = committing;
                                        let mut file_bytes = file_bytes;
                                        let mut preview_response = preview_response;
                                        let mut import_error = import_error;
                                        let mut import_result = import_result;
                                        step.set("committing".to_string());
                                        committing.set(true);
                                        spawn(async move {
                                            if let Some(preview) = preview_response.read().clone() {
                                     let plan = XccdfImportPlan {
                                                    expected_sha256: preview.sha256,
                                                    selected_profile_id: None,
                                                     selected_rule_ids: vec![],
                                                     rule_actions: vec![],
                                                     mapping_semantics: std::collections::HashMap::new(),
                                                    bundle: ImportedBundlePlan {
                                                        name: String::new(),
                                                        framework: "unknown".into(),
                                                        version: "1.0.0".into(),
                                                        layer: None,
                                                        owner: None,
                                                        description: None,
                                                    },
                                                };
                                                match import_xccdf(&file_bytes.read(), "import.xccdf", &plan).await {
                                                    Ok(result) => {
                                                        committing.set(false);
                                                        import_result.set(Some(result));
                                                        step.set("done".to_string());
                                                        props.on_success.call(());
                                                    }
                                                    Err(err) => {
                                                        committing.set(false);
                                                        import_error.set(Some(format!("Import failed: {err:?}")));
                                                        step.set("native-review".to_string());
                                                    }
                                                }
                                            }
                                        });
                                    },
                                    "Import as draft"
                                }
                            }
                        }
                    }
                }

                // ══════════════════════════════════════════════════════
                // STEP: review
                // ══════════════════════════════════════════════════════
                if *step.read() == "review" {
                    div { class: "modal-head",
                        h2 { style: "display:flex;align-items:center;gap:8px;white-space:nowrap;",
                            Icon { name: IconName::Shield, size: 14 }
                            "Review imported controls"
                        }
                        p {
                            span { class: "mono", "{file_name.read()}" }
                            " · {bench_title.read()} · "
                            strong { "{bench_ver.read()}" }
                        }
                    }
                    div { class: "modal-body",
                        if let Some(ref preview) = *preview_response.read() {
                            {
                                let namespace = preview.xccdf_version.as_deref().unwrap_or("XCCDF 1.2");
                                rsx! { details { class: "xccdf-source-details", summary { "Import details" }, div { class: "card", style: "padding:10px;margin-top:8px;display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:6px 14px;font-size:11px;", div { "Document class" strong { "{human_document_class(preview.document_class.as_deref())}" } }, div { "Fidelity" strong { "{human_fidelity(preview.fidelity.as_deref())}" } }, div { "Namespace" strong { "{namespace}" } }, div { "Source entry" strong { "{file_name.read()}" } }, div { "Profile count" strong { "{preview.profile_count}" } }, div { "Rule count" strong { "{preview.rule_count}" } }, div { "SHA-256" span { class: "mono", style: "word-break:break-all;", "{preview.sha256}" } }, if let Some(benchmark) = preview.benchmark.as_ref() { div { "Benchmark ID" span { class: "mono", "{benchmark.id}" } } } } } }
                            }
                        }

                        // ── Bundle name ────────────────────────────────
                        div { class: "field",
                            label { "Bundle name" }
                            input {
                                class: "input focus-ring",
                                value: "{bundle_name.read()}",
                                oninput: move |e| bundle_name.set(e.value()),
                            }
                        }

                        // ── Environment badges ─────────────────────────
                        div { class: "field",
                            label { "Applies to environments" }
                            div { style: "display:flex;flex-wrap:wrap;gap:6px;align-items:center;",
                                for env in props.environments.iter() {
                                    {
                                        let e_name  = env.name.clone();
                                        let e_color = env.color_hex.clone();
                                        let on = selected_envs.read().contains(&e_name);
                                        rsx! {
                                            button {
                                                 class: "focus-ring environment-pill",
                                                onclick: move |_| {
                                                    let mut envs = selected_envs.write();
                                                    if envs.contains(&e_name) {
                                                        envs.retain(|e| *e != e_name);
                                                    } else {
                                                        envs.push(e_name.clone());
                                                    }
                                                },
                                                style: format!(
                                                    "all:unset;cursor:pointer;padding:6px 12px;border-radius:99px;\
                                                     border:1px solid {};background:{};\
                                                     display:flex;align-items:center;gap:7px;\
                                                     font-size:12px;font-weight:600;color:{};",
                                                    if on { &e_color } else { "var(--cf-divider)" },
                                                    if on { format!("color-mix(in oklab, {} 14%, var(--cf-card-bg))", &e_color) } else { "var(--cf-card-bg)".to_string() },
                                                    if on { "var(--cf-text-primary)" } else { "var(--cf-text-muted)" },
                                                ),
                                                span { style: "width:8px;height:8px;border-radius:99px;background:{e_color};" }
                                                "{env.name}"
                                                if on { Icon { name: IconName::Check, size: 11 } }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Rule checklist ─────────────────────────────
                        div { class: "field",
                            div { style: "display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;",
                                label { style: "margin:0;",
                                    "Controls "
                                    span { style: "color:var(--cf-text-muted);font-weight:400;",
                                        "· {sel_count} of {total_count} selected"
                                    }
                                }
                                // CAT severity bulk-toggle buttons
                                div { style: "display:flex;gap:6px;",
                                    for (sev, n, sel) in counts.iter() {
                                        {
                                            let sev_s = sev.to_string();
                                            let color = sev_color(sev);
                                            let cat   = sev_cat(sev);
                                            let all_sel = *sel >= *n && *n > 0;
                                            rsx! {
                                                button {
                                                    class: "focus-ring",
                                                    title: "Toggle all {cat}",
                                                    onclick: move |_| {
                                                        let target = !all_sel;
                                                        let mut r = rules.write();
                                                        for rule in r.iter_mut() {
                                                            if rule.severity == sev_s {
                                                                rule.selected = target;
                                                            }
                                                        }
                                                    },
                                                    style: format!(
                                                        "all:unset;cursor:pointer;font-size:11px;font-weight:600;\
                                                         padding:3px 8px;border-radius:99px;\
                                                         border:1px solid {color}55;color:{color};\
                                                         background:color-mix(in oklab, {color} 10%, transparent);"
                                                    ),
                                                    "{cat} · {sel}/{n}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "xccdf-controls-list", "data-testid": "xccdf-review-control-list", style: "display:flex;flex-direction:column;gap:5px;margin-top:8px;padding-right:2px;",
                                for (i, rule) in rules.read().iter().enumerate() {
                                    {
                                        let is_sel = rule.selected;
                                        let color  = sev_color(&rule.severity);
                                        let cat    = sev_cat(&rule.severity);
                                        let title  = rule.title.clone();
                                        let vulnerability = if rule.vulnerability_id.is_empty() { rule.rule_id.clone() } else { rule.vulnerability_id.clone() };
                                        let srg    = rule.srg.clone();
                                        rsx! {
                                            button {
                                                class: "focus-ring",
                                                onclick: move |_| {
                                                    let mut r = rules.write();
                                                    if let Some(rule) = r.get_mut(i) {
                                                        rule.selected = !rule.selected;
                                                    }
                                                },
                                                style: format!(
                                                    "all:unset;cursor:pointer;display:flex;gap:10px;\
                                                     align-items:flex-start;padding:8px 10px;border-radius:8px;\
                                                     border:1px solid {};\
                                                     background:{};",
                                                    if is_sel { "color-mix(in oklab, var(--cf-brand-purple) 40%, transparent)" } else { "var(--cf-divider)" },
                                                    if is_sel { "color-mix(in oklab, var(--cf-brand-purple) 6%, var(--cf-card-bg))" } else { "var(--cf-card-bg)" },
                                                ),
                                                // Checkbox
                                                span { style: format!(
                                                    "width:15px;height:15px;border-radius:4px;flex-shrink:0;margin-top:1px;\
                                                     border:1.5px solid {};background:{};\
                                                     display:flex;align-items:center;justify-content:center;",
                                                    if is_sel { "var(--cf-brand-purple)" } else { "var(--cf-text-muted)" },
                                                    if is_sel { "var(--cf-brand-purple)" } else { "transparent" },
                                                ),
                                                    if is_sel { Icon { name: IconName::Check, size: 10 } }
                                                }
                                                // CAT badge
                                                span { style: format!(
                                                    "flex-shrink:0;font-size:10px;font-weight:700;\
                                                     padding:2px 6px;border-radius:4px;margin-top:1px;\
                                                     color:{color};\
                                                     background:color-mix(in oklab, {color} 14%, transparent);"),
                                                    "{cat}"
                                                }
                                                    // Title + portable source identity
                                                span { style: "min-width:0;",
                                                    span { style: "font-size:12.5px;font-weight:600;display:block;line-height:1.4;", "{title}" }
                                                    span { class: "mono", style: "font-size:10.5px;color:var(--cf-text-muted);",
                                                         "{vulnerability}"
                                                        if !srg.is_empty() { " · {srg}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Info callout ───────────────────────────────
                        div { class: "sd-callout sd-callout-info", "data-testid": "xccdf-review-summary",
                            Icon { name: IconName::Check, size: 13 }
                            div { style: "font-size:12px;",
                                "Creates "
                                strong { "{sel_count}" }
                                if sel_count == 1 { " draft policy" } else { " draft policies" }
                                " and one draft bundle. Policies begin untrusted, disabled, and unassigned. Refine policies to add native assertions, evidence requirements, manual handling, or exact-version mappings."
                            }
                        }

                        if let Some(err) = import_error.read().as_ref() {
                            div { class: "sd-callout sd-callout-danger", style: "margin-top:10px;",
                                Icon { name: IconName::Warn, size: 13 }
                                div { style: "font-size:12px;", "{err}" }
                            }
                        }
                    }
                    div { class: "modal-foot", style: "justify-content:space-between;",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| {
                                step.set("upload".to_string());
                                rules.set(Vec::new());
                                refined_rules.set(Vec::new());
                                parse_error.set(None);
                                preview_response.set(None);
                            },
                            Icon { name: IconName::ArrowLeft, size: 13 }
                            " Back"
                        }
                        div { style: "display:flex;gap:8px;",
                            button {
                                class: "btn btn-primary focus-ring",
                                "data-testid": "xccdf-review-reconcile-button",
                                disabled: !can_advance || *committing.read(),
                                style: if !can_advance || *committing.read() { "opacity:0.5;cursor:not-allowed;" } else { "" },
                                onclick: move |_| {
                                    if can_advance && !*committing.read() {
                                        step.set("reconcile".to_string());
                                    }
                                },
                                "Continue "
                                Icon { name: IconName::ChevronRight, size: 13 }
                            }
                            button {
                                class: "btn btn-ghost focus-ring",
                                disabled: !can_advance || *committing.read(),
                                style: if !can_advance || *committing.read() { "opacity:0.5;cursor:not-allowed;" } else { "" },
                                onclick: move |_| {
                                    if !can_advance || *committing.read() { return; }
                                    let selected_rule_ids: Vec<String> = rules
                                        .read()
                                        .iter()
                                        .filter(|r| r.selected)
                                        .map(|r| r.rule_id.clone())
                                        .collect();
                                     let rule_actions: Vec<XccdfRuleImportAction> = rules
                                        .read()
                                        .iter()
                                        .filter(|r| r.selected)
                                         .map(import_action_from_rule)
                                        .collect();
                                    let sha256 = preview_response
                                        .read()
                                        .as_ref()
                                        .map(|p| p.sha256.clone())
                                        .unwrap_or_default();
                                    let plan = XccdfImportPlan {
                                        expected_sha256: sha256,
                                        selected_profile_id: None,
                                        selected_rule_ids,
                                         rule_actions,
                                     mapping_semantics: refined_rules
                                         .read()
                                         .iter()
                                         .filter_map(|rule| {
                                             matches!(rule.draft.action, RefinedRuleAction::Existing(Some(_))).then(|| {
                                                 (
                                                     rule.source.rule_id.clone(),
                                                      crate::api::models::ImportedMappingSemantics {
                                                          relationship: rule.mapping_relationship.clone(),
                                                          coverage: rule.mapping_coverage.clone(),
                                                          rationale: rule.mapping_rationale.clone().filter(|value| !value.trim().is_empty()),
                                                          reviewed_related_candidate: None,
                                                     },
                                                 )
                                             })
                                         })
                                         .collect(),
                                        bundle: ImportedBundlePlan {
                                            name: bundle_name.read().trim().to_string(),
                                            framework: "xccdf".to_string(),
                                            version: bench_ver.read().clone(),
                                            layer: None,
                                            owner: None,
                                            description: None,
                                        },
                                    };
                                    let bytes = file_bytes.read().clone();
                                    let fname = file_name.read().clone();
                                    let mut committing = committing;
                                    let mut import_error = import_error;
                                    let mut import_result = import_result;
                                    let mut done_total = done_total;
                                    let mut step = step;
                                    let on_success = props.on_success;
                                    committing.set(true);
                                    import_error.set(None);
                                    spawn(async move {
                                        match import_xccdf(&bytes, &fname, &plan).await {
                                            Ok(result) => {
                                                let total = result.created_policy_count + result.reused_policy_versions;
                                                done_total.set(total as usize);
                                                import_result.set(Some(result));
                                                committing.set(false);
                                                step.set("done".to_string());
                                                on_success.call(());
                                            }
                                            Err(err) => {
                                                committing.set(false);
                                                import_error.set(Some(format!("Import failed: {err}")));
                                            }
                                        }
                                    });
                                },
                                    if *committing.read() { "Importing…" } else {
                                    "Skip & create all"
                                }
                            }
                        }
                    }
                }

                // ══════════════════════════════════════════════════════
                // STEP: reconcile (DISA STIG requirement reconciliation)
                // ══════════════════════════════════════════════════════
                if *step.read() == "reconcile" {
                     div { class: "modal-head", "data-testid": "xccdf-reconciliation-stage",
                         h2 { style: "display:flex;align-items:center;gap:8px;",
                             Icon { name: IconName::Link, size: 14 }
                             "Reconciling {sel_count} requirements"
                         }
                         p { "Deterministic implementation candidates are resolved from server-side requirement and policy evidence before review." }
                     }
                     div { class: "modal-body", style: "overflow-y:auto;",
                          if let Some(framework) = framework_reconciliation.as_ref() {
                              div { class: "card", style: "display:flex;justify-content:space-between;align-items:center;gap:12px;padding:10px 12px;margin-bottom:12px;",
                                  div {
                                      div { style: "font-size:11px;color:var(--cf-text-muted);", "Framework release" }
                                      div { class: "mono", style: "font-size:12px;margin-top:3px;", "{framework.canonical_source_key} · {framework.canonical_release_key}" }
                                  }
                                  span { class: "chip chip-success", "{framework_state_label.unwrap_or(\"Release requires review\")}" }
                              }
                          }
                          div { style: "display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:16px;",
                              for (count, label, color) in [
                                 (authoritative_count, "authoritative mappings", "#34d399"),
                                 (inherited_count, "inherited mappings", "#34d399"),
                                 (exact_count, "exact technical matches", "#60a5fa"),
                                 (attention_rule_ids.len(), "need review — no automatic resolution", if attention_rule_ids.is_empty() { "var(--cf-text-muted)" } else { "#fbbf24" }),
                             ] {
                                 div { class: "card", style: "padding:14px 12px;text-align:center;",
                                    div { style: "font-size:24px;font-weight:700;color:{color};", "{count}" }
                                    div { style: "font-size:11px;color:var(--cf-text-muted);line-height:1.4;margin-top:2px;", "{label}" }
                                 }
                             }
                         }
                         if ready_count > 0 {
                             div { class: "sd-callout sd-callout-info", style: "font-size:11px;margin-bottom:12px;", "{ready_count} requirements have inferred enforcement and can be reviewed without a reusable policy candidate." }
                         }
                         if reconciliation_rows.is_empty() {
                            div { class: "sd-callout sd-callout-warn",
                                Icon { name: IconName::Warn, size: 13 }
                                div { style: "font-size:12px;", "This XCCDF source does not expose a stable DISA STIG identity. Review all selected controls before importing." }
                         }
                         if !shared_groups.is_empty() {
                             div { class: "field", "data-testid": "xccdf-shared-implementation-groups",
                                 label { "Shared implementation groups · {shared_groups.len()}" }
                                 div { style: "display:flex;flex-direction:column;gap:7px;",
                                     for group in shared_groups.iter() {
                                         div { key: "shared-{group.group_id}", style: "padding:10px 12px;border:1px solid var(--cf-divider);border-radius:9px;background:var(--cf-subtle-bg);",
                                             div { style: "display:flex;justify-content:space-between;gap:8px;align-items:center;",
                                                 div { style: "font-size:12px;font-weight:650;", "{group.requirement_keys.len()} requirements share one technical implementation" }
                                                 span { class: "chip chip-neutral", style: "font-size:9px;", "{shared_reconciliation_action_label(&group.recommended_action)}" }
                                             }
                                              div { class: "mono", style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:4px;", "{shared_reconciliation_requirement_keys(&group.requirement_keys)}" }
                                             if let Some(candidate) = group.existing_candidate.as_ref() {
                                                 div { style: "font-size:10.5px;color:var(--cf-text-secondary);margin-top:5px;", "Candidate: {candidate.policy_name} · {candidate.confidence}% confidence" }
                                             } else {
                                                 div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:5px;", "No common existing policy candidate; review or create one shared policy." }
                                             }
                                         }
                                     }
                                 }
                             }
                         }
                        }
                        if !attention_rule_ids.is_empty() {
                            div { class: "field",
                                label { "Requiring attention · {attention_rule_ids.len()}" }
                                div { style: "display:flex;flex-direction:column;gap:5px;max-height:220px;overflow-y:auto;",
                                    for row in reconciliation_rows.iter().filter(|row| !row.auto_resolvable || row.state == "identity_conflict") {
                                         div { key: "attention-{row.rule_id}", style: "display:flex;gap:10px;align-items:flex-start;padding:8px 10px;border-radius:8px;border:1px solid var(--cf-divider);",
                                            span { style: "color:#fbbf24;margin-top:2px;flex-shrink:0;display:inline-flex;", Icon { name: IconName::Warn, size: 13 } }
                                            div { style: "min-width:0;",
                                                div { style: "font-size:12.5px;font-weight:600;line-height:1.4;", "{row.title.clone().unwrap_or_else(|| row.external_id.clone())}" }
                                                div { class: "mono", style: "font-size:10.5px;color:var(--cf-text-muted);", "{row.external_id}" }
                                                  div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:4px;display:flex;align-items:center;gap:6px;flex-wrap:wrap;",
                                                      if row.state == "identity_conflict" { "Requirement identity conflict needs a human decision." } else if let Some(candidate) = row.candidates.first() {
                                                          span { class: reconciliation_match_class(&candidate.match_type), style: "font-size:9px;padding:2px 6px;", "{reconciliation_match_label(&candidate.match_type)}" }
                                                          span { "{candidate.confidence}% confidence" }
                                                      } else { "No trusted mapped implementation or inferred enforcement is available." }
                                                 }
                                                  if let Some(candidate) = row.candidates.first() {
                                                      div { style: "display:flex;gap:5px;flex-wrap:wrap;margin-top:5px;",
                                                          span { class: "chip chip-neutral", style: "font-size:9px;", "{candidate.policy_name}" }
                                                          for reason in candidate.match_reasons.iter() {
                                                              span { class: "chip mono", style: "font-size:9px;", "{reason}" }
                                                          }
                                                     }
                                                 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if reused_count + ready_count > 0 {
                            details { style: "margin-top:4px;",
                                 summary { style: "cursor:pointer;font-size:11.5px;font-weight:600;color:var(--cf-text-muted);", "Show {reused_count + ready_count} auto-resolved requirements" }
                                div { style: "display:flex;flex-direction:column;gap:5px;margin-top:8px;max-height:220px;overflow-y:auto;",
                                      for row in reconciliation_rows.iter().filter(|row| row.auto_resolvable && row.state != "identity_conflict") {
                                          div { key: "resolved-{row.rule_id}", "data-testid": "xccdf-reconciliation-resolved-row", style: "display:flex;gap:10px;align-items:flex-start;padding:7px 10px;border-radius:8px;background:var(--cf-subtle-bg);",
                                            span { style: if !row.candidates.is_empty() { "color:#34d399;margin-top:2px;display:inline-flex;" } else { "color:#60a5fa;margin-top:2px;display:inline-flex;" }, Icon { name: if !row.candidates.is_empty() { IconName::Check } else { IconName::Shield }, size: 12 } }
                                            div { style: "min-width:0;",
                                                div { style: "font-size:12px;font-weight:600;line-height:1.4;", "{row.title.clone().unwrap_or_else(|| row.external_id.clone())}" }
                                                 div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:1px;",
                                      if let Some(candidate) = row.candidates.first() {
                                          span { class: reconciliation_match_class(&candidate.match_type), style: "font-size:9px;padding:2px 6px;margin-right:4px;", "{reconciliation_match_label(&candidate.match_type)}" }
                                          "{candidate.policy_name} · {candidate.confidence}%"
                                      } else { "Enforcement inferred from the STIG fix text" }
                                                 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "modal-foot", style: "justify-content:space-between;",
                        button { class: "btn btn-ghost focus-ring", onclick: move |_| step.set("review".to_string()), Icon { name: IconName::ArrowLeft, size: 13 } " Back" }
                        div { style: "display:flex;gap:8px;align-items:center;",
                            button {
                                class: "btn btn-ghost focus-ring",
                                title: "Walk through every control, not just the ones needing attention",
                                onclick: move |_| {
                                    refine_rule_ids.set(refine_all_rule_ids.clone());
                                    cursor.set(0);
                                    step.set("refine".to_string());
                                },
                                "Refine all instead"
                            }
                            if attention_rule_ids.is_empty() && !reconciliation_rows.is_empty() {
                                button { class: "btn btn-primary focus-ring", onclick: move |_| step.set("final-review".to_string()), Icon { name: IconName::Check, size: 13 } " Create bundle + {sel_count} policies" }
                            } else {
                                button {
                                    class: "btn btn-primary focus-ring",
                                    onclick: move |_| {
                                        let scope = if attention_rule_ids.is_empty() { fallback_review_rule_ids.clone() } else { attention_rule_ids.clone() };
                                        refine_rule_ids.set(scope);
                                        cursor.set(0);
                                        step.set("refine".to_string());
                                    },
                                     "Review {review_count} requirements "
                                    Icon { name: IconName::ChevronRight, size: 13 }
                                }
                            }
                        }
                    }
                }

                // ══════════════════════════════════════════════════════
                // STEP: refine (structured per-control walkthrough)
                // ══════════════════════════════════════════════════════
                if *step.read() == "refine" {
                    {
                        let refined_rules_signal = refined_rules;
                        let cursor_signal = cursor;
                        rsx! {
                        RefinePolicyStep {
                            rules: refined_rules_signal,
                            cursor: cursor_signal,
                            review_rule_ids: Some(refine_rule_ids.read().clone()),
                            on_back: move |_| step.set("reconcile".to_string()),
                            on_review: move |_| step.set("final-review".to_string()),
                        }
                    }
                }

                if *step.read() == "final-review" {
                    ImportReview {
                        rules: refined_rules,
                        on_back: move |_| step.set("refine".to_string()),
                        on_confirm: move |_| {
                                if *committing.read() { return; }
                                let selected_rule_ids = refined_rules.read().iter().filter(|rule| rule.selected).map(|rule| rule.source.rule_id.clone()).collect::<Vec<_>>();
                                let rule_actions = refined_rules.read().iter().filter(|rule| rule.selected).map(action_to_import).collect::<Vec<_>>();
                                 let plan = XccdfImportPlan {
                                    expected_sha256: preview_response.read().as_ref().map(|preview| preview.sha256.clone()).unwrap_or_default(),
                                    selected_profile_id: None,
                                    selected_rule_ids,
                                    rule_actions,
                                     mapping_semantics: refined_rules
                                         .read()
                                         .iter()
                                         .filter_map(|rule| {
                                             matches!(rule.draft.action, RefinedRuleAction::Existing(Some(_))).then(|| {
                                                 (
                                                     rule.source.rule_id.clone(),
                                                     crate::api::models::ImportedMappingSemantics {
                                                         relationship: rule.mapping_relationship.clone(),
                                                         coverage: rule.mapping_coverage.clone(),
                                                         rationale: rule.mapping_rationale.clone().filter(|value| !value.trim().is_empty()),
                                                         reviewed_related_candidate: mapping_semantics_for(rule).and_then(|semantics| semantics.reviewed_related_candidate),
                                                     },
                                                 )
                                             })
                                         })
                                         .collect(),
                                    bundle: ImportedBundlePlan { name: bundle_name.read().trim().to_string(), framework: "xccdf".into(), version: bench_ver.read().clone(), layer: None, owner: None, description: None },
                                };
                                let bytes = file_bytes.read().clone();
                                let filename = file_name.read().clone();
                                let mut committing = committing;
                                let mut import_error = import_error;
                                let mut import_result = import_result;
                                let mut done_total = done_total;
                                let mut step = step;
                                committing.set(true);
                                import_error.set(None);
                                spawn(async move {
                                    match import_xccdf(&bytes, &filename, &plan).await {
                                        Ok(result) => {
                                            done_total.set((result.created_policy_count + result.reused_policy_versions) as usize);
                                            import_result.set(Some(result));
                                            committing.set(false);
                                            step.set("done".into());
                                             props.on_success.call(());
                                        }
                                        Err(error) => { committing.set(false); import_error.set(Some(format!("Import failed: {error}"))); }
                                    }
                                });
                            },
                        }
                    }
                }

                // ══════════════════════════════════════════════════════
                // STEP: done
                // ══════════════════════════════════════════════════════
                if *step.read() == "done" {
                    div { class: "modal-head",
                        h2 { style: "display:flex;align-items:center;gap:8px;",
                            span { style: "color:#34d399;display:inline-flex;",
                                Icon { name: IconName::Check, size: 16 }
                            }
                            "Import complete"
                        }
                        p {
                            span { class: "mono", style: "font-weight:600;", "{bundle_name.read()}" }
                            " was created as a draft bundle."
                        }
                    }
                    div { class: "modal-body",
                        // Stats grid — created / reused / total
                        {
                            let created = import_result.read().as_ref().map(|r| r.created_policy_count).unwrap_or(0);
                            let reused = import_result.read().as_ref().map(|r| r.reused_policy_versions).unwrap_or(0);
                            let total = created + reused;
                            rsx! {
                                div { style: "display:grid;grid-template-columns:repeat(3,1fr);gap:10px;",
                                    for (n, label) in [(total, "total controls"), (created, "new policies"), (reused, "reused")] {
                                        div { class: "card", style: "padding:14px 12px;text-align:center;",
                                            div { style: "font-size:24px;font-weight:700;", "{n}" }
                                            div { style: "font-size:11px;color:var(--cf-text-muted);", "{label}" }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "sd-callout sd-callout-info", style: "margin-top:12px;",
                            Icon { name: IconName::Shield, size: 13 }
                            div { style: "font-size:12px;",
                                "Imported policies are "
                                strong { "draft, disabled, and untrusted" }
                                ". Review and trust them in the Policies view before enabling enforcement."
                            }
                        }
                    }
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-primary focus-ring",
                            onclick: move |_| props.on_close.call(()),
                            "Close"
                        }
                    }
                }
            }
        }
    }
}

// ─── Export evidence modal ────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct ExportModalProps {
    /// All compliance bundles (for multi-select).
    bundles: Vec<ComplianceBundleSummary>,
    /// The currently selected bundle in the catalog (initial default).
    selected_bundle: Option<ComplianceBundleSummary>,
    /// Systems/totals response for the primary (selected) bundle.
    systems_resp: Option<ComplianceBundleSystemsResponse>,
    /// All environments (for colored badges).
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
}

#[component]
fn ExportModal(props: ExportModalProps) -> Element {
    let all_bundles = props.bundles.clone();
    let all_envs = props.environments.clone();

    let initial_bundle_ids: Vec<uuid::Uuid> = props
        .selected_bundle
        .as_ref()
        .map(|b| vec![b.id])
        .unwrap_or_default();
    let initial_env_names: Vec<String> = props
        .selected_bundle
        .as_ref()
        .map(|b| b.required_envs.iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default();

    let mut selected_bundle_ids = use_signal(|| initial_bundle_ids);
    let mut selected_env_names = use_signal(|| initial_env_names);
    let mut bundle_query = use_signal(String::new);
    let mut format = use_signal(|| "oscal".to_string());
    let mut scope = use_signal(|| "all".to_string());
    let mut include_waivers = use_signal(|| true);
    let mut include_source = use_signal(|| true);
    let mut downloading = use_signal(|| false);
    let mut download_error = use_signal(|| None::<String>);

    // ── Env-filtered + scope-aware counts (reactive to signal reads) ──────────
    // These drive the summary callout, host-scope segment labels, and can_export.
    // They must match what the export will actually produce, so we derive them
    // from the per-system list filtered by the currently selected environments.
    let all_systems_list = props
        .systems_resp
        .as_ref()
        .map(|r| r.systems.clone())
        .unwrap_or_default();

    let env_filtered_systems: Vec<_> = {
        let envs = selected_env_names.read();
        all_systems_list
            .iter()
            .filter(|s| {
                if envs.is_empty() {
                    return true;
                }
                let env = s.environment.as_deref().unwrap_or("");
                envs.iter().any(|e| e == env)
            })
            .cloned()
            .collect()
    };

    let total_hosts = env_filtered_systems.len() as i64;
    let total_controls: i64 = env_filtered_systems.iter().map(|s| s.total).sum();
    // fail_count = number of systems that have at least one failing control
    let fail_count: i64 = env_filtered_systems.iter().filter(|s| s.fail > 0).count() as i64;

    let formats: &[(&str, &str, &str, &str)] = &[
        (
            "oscal",
            "OSCAL 1.1.2 JSON",
            "oscal.json",
            "NIST OSCAL System Security Plan + Assessment Results for ATO packages.",
        ),
        (
            "json",
            "Crystal Forge JSON",
            "cf-evidence.json",
            "Native CF schema — best for re-ingest or custom dashboards.",
        ),
        (
            "csv",
            "CSV summary",
            "summary.csv",
            "Flat per-(host, control) table. Spreadsheet-friendly.",
        ),
        (
            "pdf",
            "Print report (HTML)",
            "html",
            "Styled HTML report — open in browser and Ctrl-P to save as PDF.",
        ),
        (
            "sarif",
            "SARIF 2.1.0",
            "sarif",
            "Static analysis exchange format — works with most SAST/posture tools.",
        ),
    ];

    let fmt_name = formats
        .iter()
        .find(|f| f.0 == *format.read())
        .map(|f| f.1)
        .unwrap_or("Export");
    let ext = formats
        .iter()
        .find(|f| f.0 == *format.read())
        .map(|f| f.2)
        .unwrap_or("json");

    // ── Filtered bundles (search) ──────────────────────────────────────────
    let filtered_bundles: Vec<ComplianceBundleSummary> = all_bundles
        .iter()
        .filter(|b| {
            let q = bundle_query.read();
            q.is_empty()
                || b.name.to_lowercase().contains(&q.to_lowercase())
                || b.framework.to_lowercase().contains(&q.to_lowercase())
        })
        .cloned()
        .collect();

    // ── Available environments from selected bundles ───────────────────────
    let available_env_names: Vec<String> = {
        let ids = selected_bundle_ids.read().clone();
        let mut set = std::collections::BTreeSet::new();
        for b in &all_bundles {
            if ids.contains(&b.id) {
                for e in &b.required_envs {
                    set.insert(e.name.clone());
                }
            }
        }
        set.into_iter().collect()
    };

    // ── Filename (cf-<bundlePart>-<envPart>-<date>.<ext>) ──────────────────
    let today = js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_default();
    let today_slice: String = today.chars().take(10).collect();

    let bundle_part = {
        let ids = selected_bundle_ids.read();
        if ids.len() == 1 {
            ids[0].to_string()
        } else {
            format!("{}bundles", ids.len())
        }
    };
    let env_part = {
        let envs = selected_env_names.read();
        if envs.is_empty() {
            "no-envs".to_string()
        } else if envs.len() == 1 {
            envs[0].clone()
        } else if envs.len() >= available_env_names.len() && !available_env_names.is_empty() {
            "all-envs".to_string()
        } else {
            format!("{}envs", envs.len())
        }
    };
    let filename = format!("cf-{bundle_part}-{env_part}-{today_slice}.{ext}");

    let can_export = selected_bundle_ids.read().len() > 0
        && selected_env_names.read().len() > 0
        && total_hosts > 0;

    // Rc-shared references for use in multiple move closures.
    let selected_bundle_opt = props.selected_bundle.clone();
    let all_bundles_rc = std::rc::Rc::new(all_bundles.clone());

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal",
                style: "width:min(680px,96vw);max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-head",
                    h2 { style: "display:flex;align-items:center;gap:8px;",
                        Icon { name: IconName::Download, size: 14 }
                        "Export evidence"
                    }
                    p { "Each environment typically has its own ATO package — select the bundles and environments to scope this export." }
                }

                div { class: "modal-body", style: "overflow-y:auto;",

                    // ── Bundle multi-select ─────────────────────────────────
                    div { class: "field",
                        div {
                            style: "display:flex;align-items:center;justify-content:space-between;gap:8px;",
                            label { style: "margin:0;",
                                "Compliance bundles "
                                span { style: "color:var(--cf-text-muted);font-weight:400;", "· {selected_bundle_ids.read().len()} of {all_bundles.len()}" }
                            }
                            div { style: "display:flex;gap:4px;",
                                button {
                                    class: "focus-ring",
                                    style: "all:unset;cursor:pointer;font-size:11px;color:var(--cf-brand-purple);padding:2px 6px;",
                                    onclick: {
                                        let bundles = all_bundles_rc.clone();
                                        move |_| {
                                            let all_ids: Vec<uuid::Uuid> = bundles.iter().map(|b| b.id).collect();
                                            selected_bundle_ids.set(all_ids);
                                            // Recompute available envs
                                            let avail: Vec<String> = bundles.iter()
                                                .flat_map(|b| b.required_envs.iter().map(|e| e.name.clone()))
                                                .collect::<std::collections::BTreeSet<_>>()
                                                .into_iter()
                                                .collect();
                                            let mut envs = selected_env_names.write();
                                            envs.clear();
                                            envs.extend(avail);
                                        }
                                    },
                                    "Select all"
                                }
                                button {
                                    class: "focus-ring",
                                    style: "all:unset;cursor:pointer;font-size:11px;color:var(--cf-text-muted);padding:2px 6px;",
                                    onclick: move |_| {
                                        if let Some(b) = selected_bundle_opt.as_ref() {
                                            let id = b.id;
                                            selected_bundle_ids.set(vec![id]);
                                            let envs: Vec<String> = b.required_envs.iter().map(|e| e.name.clone()).collect();
                                            selected_env_names.set(envs);
                                        }
                                    },
                                    "Reset"
                                }
                            }
                        }
                        if all_bundles.len() > 4 {
                            input {
                                class: "input focus-ring",
                                placeholder: "Search bundles…",
                                value: "{bundle_query.read()}",
                                style: "margin-bottom:8px;",
                                oninput: move |e| bundle_query.set(e.value()),
                            }
                        }
                        div { style: "display:flex;flex-direction:column;gap:6px;max-height:208px;overflow-y:auto;padding-right:2px;",
                            if filtered_bundles.is_empty() {
                                div { style: "font-size:12px;color:var(--cf-text-muted);padding:8px 2px;",
                                    "No bundles match your search."
                                }
                            }
                            for bundle in filtered_bundles.iter() {
                                {
                                    let b = bundle.clone();
                                    let b_id = b.id;
                                    let is_selected = selected_bundle_ids.read().contains(&b_id);
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            onclick: {
                                                let bundles = all_bundles_rc.clone();
                                                move |_| {
                                                    let mut ids = selected_bundle_ids.write();
                                                    if ids.contains(&b_id) {
                                                        ids.retain(|i| *i != b_id);
                                                    } else {
                                                        ids.push(b_id);
                                                    }
                                                    // Keep envs valid after selection change
                                                    let ids_snapshot = ids.clone();
                                                    std::mem::drop(ids);
                                                    let avail: Vec<String> = bundles.iter()
                                                        .filter(|bx| ids_snapshot.contains(&bx.id))
                                                        .flat_map(|bx| bx.required_envs.iter().map(|e| e.name.clone()))
                                                        .collect::<std::collections::BTreeSet<_>>()
                                                        .into_iter()
                                                        .collect();
                                                    let mut envs = selected_env_names.write();
                                                    envs.retain(|e| avail.contains(e));
                                                }
                                            },
                                            style: {
                                                let on = is_selected;
                                                format!(
                                                    "all:unset;cursor:pointer;padding:9px 11px;border-radius:8px;\
                                                    border:1px solid {};\
                                                    background:{};\
                                                    display:flex;align-items:center;gap:10px;",
                                                    if on { "var(--cf-brand-purple)" } else { "var(--cf-divider)" },
                                                    if on { "color-mix(in oklab, var(--cf-brand-purple) 8%, var(--cf-card-bg))" } else { "var(--cf-card-bg)" },
                                                )
                                            },
                                            span {
                                                style: format!(
                                                    "width:16px;height:16px;border-radius:4px;flex-shrink:0;\
                                                    border:1.5px solid {};\
                                                    background:{};\
                                                    display:flex;align-items:center;justify-content:center;",
                                                    if is_selected { "var(--cf-brand-purple)" } else { "var(--cf-text-muted)" },
                                                    if is_selected { "var(--cf-brand-purple)" } else { "transparent" },
                                                ),
                                                if is_selected {
                                                    Icon { name: IconName::Check, size: 11 }
                                                }
                                            }
                                            div { style: "min-width:0;flex:1;",
                                                div { style: "font-size:12px;font-weight:600;", "{b.name}" }
                                                div { style: "font-size:11px;color:var(--cf-text-muted);",
                                                    "{b.framework} · {b.version} · {b.policy_ids.len()} policies"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Environment selection ────────────────────────────────
                    div { class: "field",
                        label {
                            "Environments "
                            if selected_env_names.read().len() < available_env_names.len() {
                                span { style: "color:var(--cf-brand-purple);font-weight:600;", "· scoped" }
                            }
                        }
                        div { style: "display:flex;flex-wrap:wrap;gap:6px;",
                            for env_name in available_env_names.iter() {
                                {
                                    let e_name = env_name.clone();
                                    let on = selected_env_names.read().contains(&e_name);
                                    let env_color = all_envs
                                        .iter()
                                        .find(|e| e.name == e_name)
                                        .map(|e| e.color_hex.as_str())
                                        .unwrap_or("#888")
                                        .to_string();
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            onclick: move |_| {
                                                let mut envs = selected_env_names.write();
                                                if envs.contains(&e_name) {
                                                    envs.retain(|e| *e != e_name);
                                                } else {
                                                    envs.push(e_name.clone());
                                                }
                                            },
                                            style: format!(
                                                "all:unset;cursor:pointer;padding:6px 12px;border-radius:99px;\
                                                border:1px solid {};\
                                                background:{};\
                                                display:flex;align-items:center;gap:7px;font-size:12px;font-weight:600;\
                                                color:{};",
                                                if on { &env_color } else { "var(--cf-divider)" },
                                                if on { format!("color-mix(in oklab, {} 14%, var(--cf-card-bg))", &env_color) } else { "var(--cf-card-bg)".to_string() },
                                                if on { "var(--cf-text-primary)" } else { "var(--cf-text-muted)" },
                                            ),
                                            span { style: "width:8px;height:8px;border-radius:99px;background:{env_color};" }
                                            "{e_name}"
                                            if on {
                                                Icon { name: IconName::Check, size: 11 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "help", style: "margin-top:6px;",
                            "Export one environment at a time for a focused ATO, or combine several. Only hosts in the selected environments are included."
                        }
                    }

                    // ── Output format ────────────────────────────────────────
                    div { class: "field",
                        label { "Output format" }
                        div { style: "display:grid;grid-template-columns:1fr 1fr;gap:8px;",
                            for (k, name, _ext, desc) in formats.iter() {
                                {
                                    let k_str = k.to_string();
                                    let is_sel = *format.read() == k_str;
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            style: if is_sel {
                                                "all:unset;cursor:pointer;padding:10px 12px;border-radius:8px;border:1px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 8%,var(--cf-card-bg));display:flex;flex-direction:column;gap:4px;"
                                            } else {
                                                "all:unset;cursor:pointer;padding:10px 12px;border-radius:8px;border:1px solid var(--cf-divider);background:var(--cf-card-bg);display:flex;flex-direction:column;gap:4px;"
                                            },
                                            onclick: move |_| format.set(k_str.clone()),
                                            div {
                                                style: "display:flex;align-items:center;justify-content:space-between;gap:6px;",
                                                span { style: "font-size:12px;font-weight:600;", "{name}" }
                                                if is_sel {
                                                    span { style: "color:var(--cf-brand-purple);display:inline-flex;", Icon { name: IconName::Check, size: 12 } }
                                                }
                                            }
                                            div { style: "font-size:11px;color:var(--cf-text-muted);line-height:1.4;", "{desc}" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Host scope ────────────────────────────────────────────
                    div { class: "field",
                        label { "Host scope" }
                        div { class: "seg", style: "width:fit-content;",
                            button {
                                class: if *scope.read() == "all" { "active" } else { "" },
                                onclick: move |_| scope.set("all".to_string()),
                                "All {total_hosts} host evals"
                            }
                            button {
                                class: if *scope.read() == "fail" { "active" } else { "" },
                                onclick: move |_| scope.set("fail".to_string()),
                                "Failing only ({fail_count})"
                            }
                            button {
                                class: if *scope.read() == "clean" { "active" } else { "" },
                                onclick: move |_| scope.set("clean".to_string()),
                                "Compliant only"
                            }
                        }
                    }

                    // ── Include toggles ───────────────────────────────────────
                    div { style: "display:flex;flex-direction:column;gap:8px;",
                        label {
                            style: "display:flex;gap:8px;align-items:center;font-size:13px;cursor:pointer;",
                            input {
                                r#type: "checkbox",
                                checked: *include_waivers.read(),
                                style: "accent-color:var(--cf-brand-purple);",
                                onchange: move |e| include_waivers.set(e.checked()),
                            }
                            span { "Include waiver justifications + expiry dates" }
                        }
                        label {
                            style: "display:flex;gap:8px;align-items:center;font-size:13px;cursor:pointer;",
                            input {
                                r#type: "checkbox",
                                checked: *include_source.read(),
                                style: "accent-color:var(--cf-brand-purple);",
                                onchange: move |e| include_source.set(e.checked()),
                            }
                            span { "Include rendered NixOS module source for each control" }
                        }
                    }

                    // ── Summary callout ───────────────────────────────────────
                    div { class: "sd-callout sd-callout-info", style: "margin-top:10px;",
                        Icon { name: IconName::Check, size: 13 }
                        div { style: "font-size:12px;",
                            div {
                                strong { "{selected_bundle_ids.read().len()}" } " bundle" span { if selected_bundle_ids.read().len() == 1 { " " } else { "s " } }
                                "· "
                                strong { "{selected_env_names.read().len()}" } " environment" span { if selected_env_names.read().len() == 1 { " " } else { "s " } }
                                "· "
                                strong { "{total_hosts}" } " host" span { if total_hosts == 1 { " " } else { "s " } }
                                "· "
                                strong { "{total_controls}" } " control evaluation" span { if total_controls == 1 { " " } else { "s " } }
                            }
                            div { style: "margin-top:4px;",
                                "Filename: "
                                span { class: "mono", style: "font-weight:600;", "{filename}" }
                            }
                        }
                    }

                    // ── Error display ──
                    if let Some(err) = download_error.read().as_ref() {
                        div { class: "sd-callout sd-callout-danger", style: "margin-top:10px;",
                            Icon { name: IconName::X, size: 13 }
                            div { style: "font-size:12px;", "Export failed: {err}" }
                        }
                    }
                }

                // ── Footer ──────────────────────────────────────────────────
                div { class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: !can_export || *downloading.read(),
                        style: if !can_export || *downloading.read() {
                            "opacity:0.5;cursor:not-allowed;"
                        } else { "" },
                        onclick: {
                            let bundle = props.selected_bundle.clone();
                            let systems_resp = props.systems_resp.clone();
                            move |_| {
                                let Some(bundle) = bundle.clone() else { return; };
                                let systems_resp = systems_resp.clone();
                                // Read signals inside the closure so values are fresh at click time
                                let fmt = format.read().clone();
                                let scp = scope.read().clone();
                                let iw = *include_waivers.read();
                                let is = *include_source.read();
                                let today = js_sys::Date::new_0()
                                    .to_iso_string()
                                    .as_string()
                                    .unwrap_or_default();
                                let today_slice: String = today.chars().take(10).collect();
                                let bundle_part = {
                                    let ids = selected_bundle_ids.read();
                                    if ids.len() == 1 {
                                        ids[0].to_string()
                                    } else {
                                        format!("{}bundles", ids.len())
                                    }
                                };
                                let selected_envs: Vec<String> =
                                    selected_env_names.read().clone();
                                let env_part = if selected_envs.is_empty() {
                                    "no-envs".to_string()
                                } else if selected_envs.len() == 1 {
                                    selected_envs[0].clone()
                                } else {
                                    format!("{}envs", selected_envs.len())
                                };
                                let ext = formats.iter()
                                    .find(|f| f.0 == fmt)
                                    .map(|f| f.2)
                                    .unwrap_or("json");
                                let fname = format!("cf-{bundle_part}-{env_part}-{today_slice}.{ext}");

                                downloading.set(true);
                                download_error.set(None);
                                spawn(async move {
                                    // Fetch per-system evidence for all scoped systems
                                    let all_systems = systems_resp.as_ref()
                                        .map(|r| r.systems.clone())
                                        .unwrap_or_default();
                                    let totals = systems_resp.as_ref()
                                        .map(|r| r.totals.clone())
                                        .unwrap_or_default();

                                    // Apply both environment and host-scope filters.
                                    // Environment: only systems whose env name is in selected_envs
                                    //   (if selected_envs is empty, skip env filter — no systems
                                    //    pass anyway, but that case is blocked by can_export).
                                    // Scope: "all" | "fail" | "clean"
                                    let systems: Vec<_> = all_systems.iter()
                                        .filter(|s| {
                                            if !selected_envs.is_empty() {
                                                let env = s.environment.as_deref().unwrap_or("");
                                                if !selected_envs.iter().any(|e| e == env) {
                                                    return false;
                                                }
                                            }
                                            match scp.as_str() {
                                                "fail"  => s.fail > 0,
                                                "clean" => s.fail == 0 && s.warn == 0,
                                                _       => true,
                                            }
                                        })
                                        .cloned()
                                        .collect();

                                    let scoped_sys_ids: Vec<uuid::Uuid> =
                                        systems.iter().map(|s| s.system_id).collect();

                                    let mut all_evidence = Vec::new();
                                    let mut evidence_failures = Vec::new();
                                    for sys_id in &scoped_sys_ids {
                                        match fetch_compliance_system_evidence(&bundle.id, sys_id, None).await {
                                            Ok(ev) => all_evidence.push(ev),
                                            Err(err) => {
                                                let hostname = systems
                                                    .iter()
                                                    .find(|s| s.system_id == *sys_id)
                                                    .map(|s| s.hostname.as_str())
                                                    .unwrap_or("unknown");
                                                evidence_failures
                                                    .push(format!("{}: {err}", hostname));
                                            }
                                        }
                                    }

                                    if !evidence_failures.is_empty() {
                                        download_error.set(Some(format!(
                                            "Could not fetch evidence for {}; export aborted. Failed hosts: {}",
                                            evidence_failures.len(),
                                            evidence_failures.join("; "),
                                        )));
                                        downloading.set(false);
                                        return;
                                    }

                                    let payload = ExportPayload {
                                        bundle: &bundle,
                                        totals: &totals,
                                        // Already filtered by env + scope above;
                                        // pass "all" so generators don't double-filter.
                                        systems: &systems,
                                        evidence: &all_evidence,
                                        include_waivers: iw,
                                        include_source: is,
                                        scope: "all",
                                    };

                                    // Use application/octet-stream for all text-based
                                    // formats so the browser always saves to disk
                                    // rather than rendering the content inline.
                                    // Browsers treat application/json and text/csv
                                    // as displayable and may open them in a new tab
                                    // instead of triggering a save dialog.
                                    let result = match fmt.as_str() {
                                        "json" => {
                                            let content = build_cf_json(&payload);
                                            trigger_download(&fname, "application/octet-stream", &content)
                                        }
                                        "csv" => {
                                            let content = build_csv(&payload);
                                            trigger_download(&fname, "application/octet-stream", &content)
                                        }
                                        "sarif" => {
                                            let content = build_sarif(&payload);
                                            trigger_download(&fname, "application/octet-stream", &content)
                                        }
                                        "oscal" => {
                                            let content = build_oscal(&payload);
                                            trigger_download(&fname, "application/octet-stream", &content)
                                        }
                                        "pdf" => download_print_html(&fname, &payload),
                                        _ => Err(format!("Unknown format: {fmt}")),
                                    };

                                    match result {
                                        Ok(()) => {}
                                        Err(e) => download_error.set(Some(e)),
                                    }
                                    downloading.set(false);
                                });
                            }
                        },
                        if *downloading.read() {
                            span { class: "cf-spinner-ring", style: "display:inline-flex;margin-right:4px;", Icon { name: IconName::Sync, size: 13 } }
                            " Preparing…"
                        } else {
                            Icon { name: IconName::Download, size: 13 }
                            " Download {fmt_name}"
                        }
                    }
                }
            }
        }
    }
}

// ─── New bundle modal ─────────────────────────────────────────────────────────

const STANDARD_BUNDLE_FRAMEWORKS: [&str; 4] =
    ["DISA STIG", "NIST 800-53", "CMMC 2.0", "CIS Benchmark"];
const STANDARD_BUNDLE_FRAMEWORK_ALIASES: [&str; 5] = [
    "DISA STIG",
    "NIST 800-53",
    "CMMC",
    "CMMC 2.0",
    "CIS Benchmark",
];
const DEFINE_NEW_FRAMEWORK: &str = "__define_new_framework__";

fn is_standard_bundle_framework(framework: &str) -> bool {
    STANDARD_BUNDLE_FRAMEWORK_ALIASES
        .iter()
        .any(|standard| framework.trim().eq_ignore_ascii_case(standard))
}

fn custom_bundle_frameworks(
    bundles: &[ComplianceBundleSummary],
    policies: &[DeploymentPolicySummary],
) -> Vec<String> {
    custom_framework_values(
        bundles
            .iter()
            .map(|bundle| bundle.framework.as_str())
            .chain(
                policies
                    .iter()
                    .filter_map(|policy| policy.framework.as_deref()),
            ),
    )
}

fn custom_framework_values<'a>(frameworks: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut frameworks = frameworks
        .into_iter()
        .map(str::trim)
        .filter(|framework| !framework.is_empty() && !is_standard_bundle_framework(framework))
        .map(str::to_string)
        .collect::<Vec<_>>();
    frameworks.sort_by_key(|framework| framework.to_ascii_lowercase());
    frameworks.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    frameworks
}

fn accept_new_bundle_framework(
    mut framework: Signal<String>,
    mut new_framework: Signal<String>,
    mut accepted_frameworks: Signal<Vec<String>>,
    mut defining_new: Signal<bool>,
) {
    let value = new_framework.read().trim().to_string();
    if value.is_empty() {
        return;
    }
    if !is_standard_bundle_framework(&value)
        && !accepted_frameworks
            .read()
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        accepted_frameworks.write().push(value.clone());
    }
    framework.set(value);
    new_framework.set(String::new());
    defining_new.set(false);
}

#[derive(Props, Clone, PartialEq)]
struct BundleFrameworkFieldProps {
    framework: Signal<String>,
    custom_frameworks: Vec<String>,
}

#[component]
fn BundleFrameworkField(props: BundleFrameworkFieldProps) -> Element {
    let mut framework = props.framework;
    let mut normalized_frameworks = use_signal(Vec::<ComplianceFrameworkSummary>::new);
    let mut normalized_frameworks_loading = use_signal(|| false);
    let mut defining_new = use_signal(|| false);
    let mut new_framework = use_signal(String::new);
    let mut accepted_frameworks = use_signal(Vec::<String>::new);
    let current_framework = framework.read().clone();
    let legacy_cmmc_value = current_framework
        .trim()
        .eq_ignore_ascii_case("CMMC")
        .then_some(current_framework.clone());
    let mut custom_frameworks = props.custom_frameworks.clone();
    if normalized_frameworks.read().is_empty() && !*normalized_frameworks_loading.read() {
        normalized_frameworks_loading.set(true);
        spawn(async move {
            if let Ok(value) = fetch_compliance_frameworks().await {
                normalized_frameworks.set(value);
            }
            normalized_frameworks_loading.set(false);
        });
    }
    custom_frameworks.extend(
        normalized_frameworks
            .read()
            .iter()
            .map(|item| item.name.clone()),
    );
    custom_frameworks.extend(accepted_frameworks.read().iter().cloned());
    custom_frameworks.sort_by_key(|framework| framework.to_ascii_lowercase());
    custom_frameworks.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    rsx! {
        div { class: "field", style: "margin-top:0;",
            label { "Framework" }
            if *defining_new.read() {
                div { style: "display:flex;gap:6px;",
                    input {
                        class: "input focus-ring",
                        autofocus: true,
                        placeholder: "e.g. Acme Internal Baseline",
                        value: "{new_framework}",
                        oninput: move |event| new_framework.set(event.value()),
                        onkeydown: move |event| match event.key() {
                            Key::Enter => accept_new_bundle_framework(
                                framework,
                                new_framework,
                                accepted_frameworks,
                                defining_new,
                            ),
                            Key::Escape => {
                                new_framework.set(String::new());
                                defining_new.set(false);
                            }
                            _ => {}
                        },
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        disabled: new_framework.read().trim().is_empty(),
                        onclick: move |_| accept_new_bundle_framework(
                            framework,
                            new_framework,
                            accepted_frameworks,
                            defining_new,
                        ),
                        "Add"
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        onclick: move |_| {
                            new_framework.set(String::new());
                            defining_new.set(false);
                        },
                        "Cancel"
                    }
                }
            } else {
                select {
                    class: "input focus-ring",
                    value: "{current_framework}",
                    onchange: move |event| {
                        let value = event.value();
                        if value == DEFINE_NEW_FRAMEWORK {
                            new_framework.set(String::new());
                            defining_new.set(true);
                        } else {
                            framework.set(value);
                        }
                    },
                    optgroup { label: "Standard",
                        for standard in STANDARD_BUNDLE_FRAMEWORKS {
                            option { value: "{standard}", "{standard}" }
                        }
                        if let Some(value) = legacy_cmmc_value {
                            option { value: "{value}", "CMMC 2.0" }
                        }
                    }
                    if !custom_frameworks.is_empty() {
                        optgroup { label: "Custom",
                            for custom in custom_frameworks {
                                option { value: "{custom}", "{custom}" }
                            }
                        }
                    }
                    option { value: DEFINE_NEW_FRAMEWORK, "+ Define new framework…" }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct BundlePolicyPickerProps {
    framework: Signal<String>,
    policies: Vec<DeploymentPolicySummary>,
    selected_policy_ids: Signal<Vec<uuid::Uuid>>,
}

/// Framework-aware bundle policy selector.  The split is driven by the
/// authoritative mapping projection, not legacy policy framework metadata.
#[component]
fn BundlePolicyPicker(props: BundlePolicyPickerProps) -> Element {
    let mut query = use_signal(String::new);
    let mut mapped_policy_version_ids = use_signal(Vec::<uuid::Uuid>::new);
    let mut loaded_framework_name = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let framework_name = props.framework.read().trim().to_string();

    if *loaded_framework_name.read() != Some(framework_name.clone()) && !*loading.read() {
        loaded_framework_name.set(Some(framework_name.clone()));
        mapped_policy_version_ids.set(vec![]);
        loading.set(true);
        let framework_name_for_request = framework_name.clone();
        spawn(async move {
            let mapped = match fetch_compliance_frameworks().await {
                Ok(frameworks) => match frameworks
                    .iter()
                    .find(|framework| {
                        framework.name.eq_ignore_ascii_case(&framework_name_for_request)
                            || (framework_name_for_request.eq_ignore_ascii_case("DISA STIG")
                                && framework
                                    .canonical_source_key
                                    .to_ascii_lowercase()
                                    .starts_with("disa-"))
                    })
                {
                    Some(framework) => fetch_framework_mapped_policy_versions(&framework.id)
                        .await
                        .map(|response| response.policy_version_ids)
                        .unwrap_or_default(),
                    None => vec![],
                },
                Err(_) => vec![],
            };
            mapped_policy_version_ids.set(mapped);
            loading.set(false);
        });
    }

    let filter = query.read().to_ascii_lowercase();
    let matches_query = |policy: &&DeploymentPolicySummary| {
        filter.is_empty()
            || policy.name.to_ascii_lowercase().contains(&filter)
            || policy
                .description
                .as_deref()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains(&filter)
    };
    let mapped_versions = mapped_policy_version_ids.read().clone();
    let mapped: Vec<_> = props
        .policies
        .iter()
        .filter(|policy| {
            policy
                .version_id
                .is_some_and(|version_id| mapped_versions.contains(&version_id))
                && matches_query(policy)
        })
        .cloned()
        .collect();
    let custom: Vec<_> = props
        .policies
        .iter()
        .filter(|policy| {
            !policy
                .version_id
                .is_some_and(|version_id| mapped_versions.contains(&version_id))
                && matches_query(policy)
        })
        .cloned()
        .collect();
    let selected_count = props.selected_policy_ids.read().len();

    rsx! {
        div { style: "padding:14px;border:1px solid var(--cf-divider);border-radius:10px;background:color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg));",
            div { style: "display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:10px;",
                div { style: "font-size:13px;font-weight:600;display:flex;align-items:center;gap:6px;",
                    Icon { name: IconName::File, size: 13 }
                    "Controls in this bundle"
                    span { class: "chip chip-info", style: "font-size:10px;", "{selected_count} selected" }
                }
                div { class: "filter-search", style: "max-width:200px;margin:0;",
                    Icon { name: IconName::Search, size: 14 }
                    input { class: "input focus-ring", placeholder: "Filter policies…", value: "{query}", oninput: move |event| query.set(event.value()) }
                }
            }
            if *loading.read() {
                div { style: "font-size:11px;color:var(--cf-text-muted);padding:4px 0 8px;", "Loading normalized mappings…" }
            }
            BundlePolicyPickerSection {
                title: format!("Mapped to {framework_name}"),
                subtitle: "Policies with an authoritative requirement mapping to this framework.",
                policies: mapped,
                selected_policy_ids: props.selected_policy_ids,
            }
            BundlePolicyPickerSection {
                title: format!("Custom addition / No mapping to {framework_name}"),
                subtitle: "Explicit additions. These policies are included without claiming framework ownership.",
                policies: custom,
                selected_policy_ids: props.selected_policy_ids,
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct BundlePolicyPickerSectionProps {
    title: String,
    subtitle: &'static str,
    policies: Vec<DeploymentPolicySummary>,
    selected_policy_ids: Signal<Vec<uuid::Uuid>>,
}

#[component]
fn BundlePolicyPickerSection(mut props: BundlePolicyPickerSectionProps) -> Element {
    rsx! {
        div { style: "display:flex;flex-direction:column;gap:4px;margin-top:12px;",
            div { style: "font-size:11px;font-weight:700;color:var(--cf-text-secondary);", "{props.title}" }
            div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-bottom:3px;", "{props.subtitle}" }
            if props.policies.is_empty() {
                div { style: "font-size:11px;color:var(--cf-text-muted);padding:7px 0;", "No policies in this section." }
            }
            for policy in props.policies {
                {
                    let policy_id = policy.id;
                    let selected = props.selected_policy_ids.read().contains(&policy_id);
                    let description = policy.description.clone().unwrap_or_default();
                    rsx! {
                        button {
                            key: "{policy_id}", class: "focus-ring",
                            style: if selected { "all:unset;cursor:pointer;display:flex;gap:10px;align-items:flex-start;padding:9px 11px;border-radius:8px;border:1px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 9%,var(--cf-card-bg));" } else { "all:unset;cursor:pointer;display:flex;gap:10px;align-items:flex-start;padding:9px 11px;border-radius:8px;border:1px solid var(--cf-divider);background:var(--cf-card-bg);" },
                            onclick: move |_| toggle_uuid(&mut props.selected_policy_ids, policy_id),
                            div { style: if selected { "width:16px;height:16px;border-radius:5px;flex-shrink:0;margin-top:1px;border:1.5px solid var(--cf-brand-purple);background:var(--cf-brand-purple);display:grid;place-items:center;" } else { "width:16px;height:16px;border-radius:5px;flex-shrink:0;margin-top:1px;border:1.5px solid var(--cf-card-border);background:transparent;display:grid;place-items:center;" }, if selected { span { style: "color:#fff;display:inline-flex;", Icon { name: IconName::Check, size: 11 } } } }
                            div { style: "min-width:0;flex:1;",
                                div { style: "display:flex;align-items:center;gap:8px;", span { class: "mono", style: "font-size:12px;font-weight:600;", "{policy.name}" } span { class: "chip chip-unknown", style: "font-size:9px;", "{policy.policy_type}" } }
                                div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:2px;", "{description}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct BundleRequirementPickerProps {
    framework: Signal<String>,
    selected_requirement_ids: Signal<Vec<uuid::Uuid>>,
    baseline_version_id: Option<uuid::Uuid>,
}

/// Selects the exact normalized requirement versions that form a bundle's
/// baseline. This is intentionally independent from policy selection so a
/// bundle may contain requirements with no implementation yet.
#[component]
fn BundleRequirementPicker(mut props: BundleRequirementPickerProps) -> Element {
    let mut frameworks = use_signal(Vec::<ComplianceFrameworkSummary>::new);
    let mut versions = use_signal(Vec::<ComplianceFrameworkVersionSummary>::new);
    let mut results = use_signal(Vec::<RequirementVersionSummary>::new);
    let mut query = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut loaded_framework = use_signal(|| None::<String>);
    let mut loaded_baseline = use_signal(|| false);
    let mut selected_version_id = use_signal(|| None::<uuid::Uuid>);
    let framework_name = props.framework.read().trim().to_string();

    if !*loaded_baseline.read() {
        loaded_baseline.set(true);
        if let Some(version_id) = props.baseline_version_id {
            let mut selected_requirement_ids = props.selected_requirement_ids;
            spawn(async move {
                if let Ok(members) = fetch_bundle_version_requirement_membership(&version_id).await {
                    selected_requirement_ids.set(
                        members.into_iter().filter(|member| member.selected).map(|member| member.requirement_version_id).collect(),
                    );
                }
            });
        }
    }

    if *loaded_framework.read() != Some(framework_name.clone()) {
        loaded_framework.set(Some(framework_name.clone()));
        versions.set(Vec::new());
        results.set(Vec::new());
        error.set(None);
        let framework_name_for_request = framework_name.clone();
        spawn(async move {
            let loaded = if frameworks.read().is_empty() {
                match fetch_compliance_frameworks().await {
                    Ok(value) => {
                        frameworks.set(value.clone());
                        value
                    }
                    Err(err) => {
                        error.set(Some(err.to_string()));
                        return;
                    }
                }
            } else {
                frameworks.read().clone()
            };
            if let Some(framework) = loaded.iter().find(|candidate| {
                candidate.name.eq_ignore_ascii_case(&framework_name_for_request)
                    || (framework_name_for_request.eq_ignore_ascii_case("DISA STIG")
                        && candidate.canonical_source_key.to_ascii_lowercase().starts_with("disa-"))
            }) {
                match fetch_compliance_framework_versions(&framework.id).await {
                    Ok(value) => {
                        let first_version_id = value.first().map(|version| version.id);
                        selected_version_id.set(first_version_id);
                        versions.set(value);
                        if let Some(version_id) = first_version_id {
                            match search_requirements(&version_id, None, None, 50, 0).await {
                                Ok(value) => results.set(value),
                                Err(err) => error.set(Some(err.to_string())),
                            }
                        }
                    }
                    Err(err) => error.set(Some(err.to_string())),
                }
            }
        });
    }

    let version_id = *selected_version_id.read();
    let selected_ids = props.selected_requirement_ids.read().clone();
    let selected_count = selected_ids.len();
    let visible_results = results.read().clone();

    rsx! {
        div { style: "margin-top:14px;padding:14px;border:1px solid var(--cf-divider);border-radius:10px;background:color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg));",
            div { style: "display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:8px;",
                div {
                    div { style: "font-size:13px;font-weight:600;", "Requirement baseline" }
                    div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:2px;", "Select exact requirements independently of the policies in this bundle." }
                }
                span { class: "chip chip-info", style: "font-size:10px;", "{selected_count} selected" }
            }
            if let Some(message) = error.read().as_ref() {
                div { class: "sd-callout sd-callout-danger", style: "font-size:11px;margin-bottom:8px;", "{message}" }
            }
            if versions.read().is_empty() {
                div { style: "font-size:11px;color:var(--cf-text-muted);padding:6px 0;", "Loading normalized framework releases…" }
            } else {
                div { class: "field", style: "margin-top:0;",
                    label { "Framework release" }
                    select {
                        class: "input focus-ring mono",
                        value: version_id.map(|id| id.to_string()).unwrap_or_default(),
                        onchange: move |event| {
                            let selected = event.value().parse::<uuid::Uuid>().ok();
                            selected_version_id.set(selected);
                            // Requirement-version IDs are release-specific. Do not
                            // silently carry a baseline from the previous release.
                            props.selected_requirement_ids.set(Vec::new());
                            results.set(Vec::new());
                            if let Some(id) = selected {
                                let q = query.read().clone();
                                loading.set(true);
                                spawn(async move {
                                    match search_requirements(&id, (!q.trim().is_empty()).then_some(q.as_str()), None, 50, 0).await {
                                        Ok(value) => results.set(value),
                                        Err(err) => error.set(Some(err.to_string())),
                                    }
                                    loading.set(false);
                                });
                            }
                        },
                        for version in versions.read().iter() {
                            option { value: "{version.id}", "{version.version}" }
                        }
                    }
                }
                div { class: "filter-search", style: "max-width:none;margin:8px 0 6px;",
                    Icon { name: IconName::Search, size: 14 }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search requirement ID or title…",
                        value: "{query}",
                        oninput: move |event| {
                            let value = event.value();
                            query.set(value.clone());
                            if let Some(id) = version_id {
                                loading.set(true);
                                spawn(async move {
                                    match search_requirements(&id, (!value.trim().is_empty()).then_some(value.as_str()), None, 50, 0).await {
                                        Ok(found) => results.set(found),
                                        Err(err) => error.set(Some(err.to_string())),
                                    }
                                    loading.set(false);
                                });
                            }
                        }
                    }
                }
                if *loading.read() { div { style: "font-size:10.5px;color:var(--cf-text-muted);", "Searching…" } }
                div { style: "display:flex;flex-direction:column;gap:4px;max-height:180px;overflow-y:auto;",
                    for requirement in visible_results {
                        {
                            let requirement_id = requirement.id;
                            let selected = selected_ids.contains(&requirement_id);
                            let external_id = requirement.external_id.clone();
                            let title = requirement.title.clone().unwrap_or_default();
                            rsx! {
                                button {
                                    key: "{requirement_id}", class: "focus-ring",
                                    style: if selected { "all:unset;cursor:pointer;padding:7px 9px;border-radius:7px;border:1px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 9%,var(--cf-card-bg));" } else { "all:unset;cursor:pointer;padding:7px 9px;border-radius:7px;border:1px solid var(--cf-divider);background:var(--cf-card-bg);" },
                                    onclick: move |_| {
                                        let mut ids = props.selected_requirement_ids.write();
                                        if let Some(position) = ids.iter().position(|id| *id == requirement_id) { ids.remove(position); } else { ids.push(requirement_id); }
                                    },
                                    div { style: "display:flex;align-items:center;gap:7px;",
                                        span { class: "mono", style: "font-size:11px;font-weight:600;", "{external_id}" }
                                        if selected { span { class: "chip chip-info", style: "font-size:9px;", "Selected" } }
                                    }
                                    if !title.is_empty() { div { style: "font-size:10.5px;color:var(--cf-text-secondary);margin-top:2px;", "{title}" } }
                                }
                            }
                        }
                    }
                }
                if selected_count == 0 { div { style: "font-size:10.5px;color:var(--cf-text-muted);margin-top:7px;", "No requirement baseline selected. This is valid for a custom policy-only bundle." } }
                if selected_count > 0 { div { style: "font-size:10px;color:var(--cf-text-muted);margin-top:7px;", "Changing the framework release clears the selected requirement versions." } }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct NewBundleModalProps {
    policies: Vec<DeploymentPolicySummary>,
    bundles: Vec<ComplianceBundleSummary>,
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
    on_created: EventHandler<ComplianceBundleSummary>,
}

#[component]
fn NewBundleModal(props: NewBundleModalProps) -> Element {
    let mut name = use_signal(String::new);
    let mut version = use_signal(String::new);
    let mut framework = use_signal(|| "DISA STIG".to_string());
    let mut description = use_signal(String::new);
    let mut selected_env_ids = use_signal(Vec::<uuid::Uuid>::new);
    let mut selected_policy_ids = use_signal(Vec::<uuid::Uuid>::new);
    let selected_requirement_ids = use_signal(Vec::<uuid::Uuid>::new);
    let mut query = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let framework_options = custom_bundle_frameworks(&props.bundles, &props.policies);

    let can_save = !name.read().trim().is_empty()
        && (!selected_policy_ids.read().is_empty() || !selected_requirement_ids.read().is_empty());

    let filtered_policies: Vec<_> = props
        .policies
        .iter()
        .filter(|p| {
            let q = query.read().to_lowercase();
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
        })
        .collect();

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal",
                style: "width:min(760px,97vw);max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-head",
                    h2 {
                        Icon { name: IconName::Shield, size: 14 }
                        "New compliance bundle"
                    }
                    p { "A bundle represents a standard assembled from granular policies that each assert one thing." }
                }

                div { class: "modal-body", style: "overflow-y:auto;",
                    if let Some(msg) = error.read().as_ref() {
                        div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;",
                            Icon { name: IconName::X, size: 13 }
                            div { style: "font-size:12px;", "{msg}" }
                        }
                    }

                    // Name + version row
                    div { style: "display:grid;grid-template-columns:2fr 1fr;gap:14px;",
                        div { class: "field", style: "margin-top:0;",
                            label { "Bundle name" }
                            input {
                                class: "input focus-ring",
                                value: "{name}",
                                placeholder: "e.g. DISA RHEL9 STIG (v1r5)",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "field", style: "margin-top:0;",
                            label { "Version / revision" }
                            input {
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: "{version}",
                                placeholder: "v1r5",
                                oninput: move |e| version.set(e.value()),
                            }
                        }
                    }

                    // Framework + description row
                    div { style: "display:grid;grid-template-columns:1fr 2fr;gap:14px;margin-top:14px;",
                        BundleFrameworkField { framework, custom_frameworks: framework_options }
                        div { class: "field", style: "margin-top:0;",
                            label { "Description" }
                            input {
                                class: "input focus-ring",
                                value: "{description}",
                                placeholder: "What this bundle verifies",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                    }

                    // Environments
                    div { class: "field",
                        label { "Applies to environments" }
                        div { style: "display:flex;flex-wrap:wrap;gap:6px;",
                            for env in props.environments.iter() {
                                {
                                    let env_id = env.id;
                                    let env_name = env.name.clone();
                                    let color = env.color_hex.clone();
                                    let on = selected_env_ids.read().contains(&env_id);
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            style: if on {
                                                format!("padding:4px 10px;border-radius:99px;font-size:11px;cursor:pointer;border:1px solid {color};background:color-mix(in oklab,{color} 14%,var(--cf-card-bg));color:{color};display:inline-flex;align-items:center;gap:6px;font-family:inherit;")
                                            } else {
                                                "padding:4px 10px;border-radius:99px;font-size:11px;cursor:pointer;border:1px solid var(--cf-card-border);background:transparent;color:var(--cf-text-secondary);display:inline-flex;align-items:center;gap:6px;font-family:inherit;".to_string()
                                            },
                                            onclick: move |_| toggle_uuid(&mut selected_env_ids, env_id),
                                            span { style: "width:6px;height:6px;border-radius:50%;background:{color};" }
                                            "{env_name}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    BundlePolicyPicker { framework, policies: props.policies.clone(), selected_policy_ids }
                    BundleRequirementPicker { framework, selected_requirement_ids, baseline_version_id: None }

                    if !can_save && !name.read().trim().is_empty() {
                        div { class: "help", style: "color:#fbbf24;margin-top:8px;",
                            Icon { name: IconName::Warn, size: 10 }
                            " Select at least one policy or requirement. A requirement-only baseline is valid."
                        }
                    }
                }

                div { class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: !can_save || *saving.read(),
                        onclick: move |_| {
                            if !can_save { return; }
                            let req = CreateComplianceBundleRequest {
                                name: name.read().trim().to_string(),
                                framework: framework.read().trim().to_string(),
                                version: Some(version.read().trim().to_string()),
                                description: {
                                    let d = description.read().trim().to_string();
                                    if d.is_empty() { None } else { Some(d) }
                                },
                                layer: Some("fleet".to_string()),
                                required_envs: selected_env_ids.read().clone(),
                                policy_ids: selected_policy_ids.read().clone(),
                                requirement_version_ids: selected_requirement_ids.read().clone(),
                            };
                            saving.set(true);
                            spawn(async move {
                                match create_compliance_bundle(&req).await {
                                    Ok(bundle) => props.on_created.call(bundle),
                                    Err(err) => {
                                        error.set(Some(err.to_string()));
                                        saving.set(false);
                                    }
                                }
                            });
                        },
                        Icon { name: IconName::Check, size: 13 }
                        if *saving.read() { " Saving…" } else { " Create bundle" }
                    }
                }
            }
        }
    }
}

// ─── Edit bundle modal ────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct EditBundleModalProps {
    bundle: ComplianceBundleSummary,
    policies: Vec<DeploymentPolicySummary>,
    bundles: Vec<ComplianceBundleSummary>,
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<ComplianceBundleSummary>,
    on_deleted: EventHandler<uuid::Uuid>,
}

#[component]
fn EditBundleModal(props: EditBundleModalProps) -> Element {
    let bundle_id = props.bundle.id;
    let mut name = use_signal(|| props.bundle.name.clone());
    let mut version = use_signal(|| props.bundle.version.clone());
    let mut framework = use_signal(|| props.bundle.framework.clone());
    let mut description = use_signal(|| props.bundle.description.clone().unwrap_or_default());
    let initial_env_ids: Vec<uuid::Uuid> =
        props.bundle.required_envs.iter().map(|e| e.id).collect();
    let initial_policy_ids: Vec<uuid::Uuid> = props.bundle.policy_ids.clone();
    let mut selected_env_ids = use_signal(|| initial_env_ids);
    let mut selected_policy_ids = use_signal(|| initial_policy_ids);
    let selected_requirement_ids = use_signal(Vec::<uuid::Uuid>::new);
    let mut query = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let mut delete_busy = use_signal(|| false);
    let mut eligibility: Signal<Option<crate::api::models::DeletionEligibility>> =
        use_signal(|| None);
    let mut eligibility_loading = use_signal(|| false);
    let mut eligibility_error = use_signal(|| None::<String>);
    let framework_options = custom_bundle_frameworks(&props.bundles, &props.policies);

    // Local pre-check from data already in the bundle summary.
    let has_immutable_history = props.bundle.versions.iter().any(|version| {
        matches!(
            version.publication_state.as_str(),
            "accepted" | "deprecated"
        )
    });
    let assigned_count = props.bundle.active_assignment_count;

    let can_save = !name.read().trim().is_empty()
        && (!selected_policy_ids.read().is_empty() || !selected_requirement_ids.read().is_empty());

    let filtered_policies: Vec<_> = props
        .policies
        .iter()
        .filter(|p| {
            let q = query.read().to_lowercase();
            q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.description
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
        })
        .collect();

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal",
                style: "width:min(760px,97vw);max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                if *confirm_delete.read() {
                    DeleteBundleConfirm {
                        bundle_name: props.bundle.name.clone(),
                        policy_count: props.bundle.policy_ids.len(),
                        busy: *delete_busy.read(),
                        error: error.read().clone(),
                        on_cancel: move |_| {
                            if !*delete_busy.read() {
                                error.set(None);
                                confirm_delete.set(false);
                            }
                        },
                        on_confirm: move |_| {
                            let bid = bundle_id;
                            error.set(None);
                            delete_busy.set(true);
                            spawn(async move {
                                match delete_compliance_bundle(&bid).await {
                                    Ok(()) => {
                                        delete_busy.set(false);
                                        props.on_deleted.call(bid)
                                    }
                                    Err(err) => {
                                        web_sys::console::error_1(&format!("Failed to delete bundle: {err}").into());
                                        delete_busy.set(false);
                                        error.set(Some(match err {
                                            crate::api::client::ApiClientError::Status { code, body } => {
                                                format!("Delete failed (HTTP {code}): {body}")
                                            }
                                            other => format!("Failed to delete bundle: {other}"),
                                        }));
                                    }
                                }
                            });
                        },
                    }
                } else {
                // Fragment wrapper required — Dioxus if/else each branch must be one element
                div { style: "display:contents;",

                div { class: "modal-head",
                    h2 {
                        span { style: "margin-right:6px;vertical-align:text-bottom;display:inline-flex;", Icon { name: IconName::Shield, size: 14 } }
                        " Edit compliance bundle"
                    }
                    p { "A bundle represents a standard (a STIG, NIST baseline, or your own) — assembled from granular policies that each assert one thing." }
                }

                div { class: "modal-body", style: "overflow-y:auto;",
                    if let Some(msg) = error.read().as_ref() {
                        div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;",
                            Icon { name: IconName::X, size: 13 }
                            div { style: "font-size:12px;", "{msg}" }
                        }
                    }

                    div { style: "display:grid;grid-template-columns:2fr 1fr;gap:14px;",
                        div { class: "field", style: "margin-top:0;",
                            label { "Bundle name" }
                            input {
                                class: "input focus-ring",
                                value: "{name}",
                                placeholder: "e.g. Anduril NixOS STIG (v1r2)",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "field", style: "margin-top:0;",
                            label { "Version / revision" }
                            input {
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: "{version}",
                                placeholder: "v1r5",
                                oninput: move |e| version.set(e.value()),
                            }
                        }
                    }

                    div { style: "display:grid;grid-template-columns:1fr 2fr;gap:14px;margin-top:14px;",
                        BundleFrameworkField { framework, custom_frameworks: framework_options }
                        div { class: "field", style: "margin-top:0;",
                            label { "Description" }
                            input {
                                class: "input focus-ring",
                                value: "{description}",
                                placeholder: "What this bundle verifies",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                    }

                    div { class: "field",
                        label { "Applies to environments" }
                        div { style: "display:flex;flex-wrap:wrap;gap:6px;",
                            for env in props.environments.iter() {
                                {
                                    let env_id = env.id;
                                    let env_name = env.name.clone();
                                    let color = env.color_hex.clone();
                                    let on = selected_env_ids.read().contains(&env_id);
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            style: if on {
                                                format!("padding:4px 10px;border-radius:99px;font-size:11px;cursor:pointer;border:1px solid {color};background:color-mix(in oklab,{color} 14%,var(--cf-card-bg));color:{color};display:inline-flex;align-items:center;gap:6px;font-family:inherit;")
                                            } else {
                                                "padding:4px 10px;border-radius:99px;font-size:11px;cursor:pointer;border:1px solid var(--cf-card-border);background:transparent;color:var(--cf-text-secondary);display:inline-flex;align-items:center;gap:6px;font-family:inherit;".to_string()
                                            },
                                            onclick: move |_| toggle_uuid(&mut selected_env_ids, env_id),
                                            span { style: "width:6px;height:6px;border-radius:50%;background:{color};" }
                                            "{env_name}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    BundlePolicyPicker { framework, policies: props.policies.clone(), selected_policy_ids }
                    BundleRequirementPicker { framework, selected_requirement_ids, baseline_version_id: props.bundle.current_draft_version_id }

                    if selected_policy_ids.read().is_empty() {
                        div { class: "help", style: "color:#fbbf24;margin-top:8px;",
                            Icon { name: IconName::Warn, size: 10 }
                            " Select at least one policy or requirement. A requirement-only baseline is valid."
                        }
                    }

                    // Danger zone. Use the server preflight when available;
                    // fall back to local summary heuristics for the initial state.
                    div { style: "margin-top:10px;padding-top:14px;border-top:1px solid var(--cf-divider);",
                        div { style: "font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);margin-bottom:8px;",
                            "Danger zone"
                        }

                        if let Some(preflight_error) = eligibility_error.read().clone() {
                            div { class: "sd-callout sd-callout-danger", style: "font-size:12px;",
                                Icon { name: IconName::Warn, size: 13 }
                                div {
                                    p { style: "margin:0 0 6px;font-weight:600;", "Could not determine whether this item can be permanently deleted." }
                                    p { style: "margin:0 0 8px;", "{preflight_error}" }
                                    button { class: "btn btn-ghost xs focus-ring", disabled: *eligibility_loading.read(), onclick: move |_| {
                                        eligibility_error.set(None);
                                        eligibility_loading.set(true);
                                        let bid = bundle_id;
                                        spawn(async move {
                                            match crate::api::client::fetch_bundle_deletion_eligibility(&bid).await {
                                                Ok(result) => eligibility.set(Some(result)),
                                                Err(error) => eligibility_error.set(Some(error.to_string())),
                                            }
                                            eligibility_loading.set(false);
                                        });
                                    }, "Retry" }
                                }
                            }
                        } else if let Some(elig) = eligibility.read().clone() {
                            // ── Authoritative eligibility from server preflight ──
                            if elig.eligible {
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    style: "color:#f87171;border-color:rgba(248,113,113,0.3);",
                                    onclick: move |_| confirm_delete.set(true),
                                    Icon { name: IconName::Trash, size: 12 }
                                    " Delete bundle"
                                }
                            } else if elig.permanently_blocked() {
                                // Show the specific permanent blockers.
                                div { class: "sd-callout sd-callout-danger", style: "font-size:12px;",
                                    Icon { name: IconName::Warn, size: 13 }
                                    div {
                                        p { style: "margin:0 0 6px;font-weight:600;", "Permanent deletion unavailable" }
                                        for blocker in elig.blockers.iter() {
                                            p { style: "margin:0 0 4px;", "{blocker.message}" }
                                        }
                                        p { style: "margin:4px 0 0;color:var(--cf-text-muted);font-size:11px;",
                                            "To stop using this bundle, deactivate active assignments and deprecate published versions. Historical records are retained for auditability."
                                        }
                                    }
                                }
                            } else {
                                // Removable blockers — tell user exactly what to fix.
                                div { class: "sd-callout sd-callout-danger", style: "font-size:12px;",
                                    Icon { name: IconName::Warn, size: 13 }
                                    div {
                                        p { style: "margin:0 0 6px;font-weight:600;", "Permanent deletion blocked" }
                                        for blocker in elig.blockers.iter() {
                                            p { style: "margin:0 0 4px;",
                                                "{blocker.message}"
                                                if let Some(count) = blocker.count { " ({count})" }
                                            }
                                        }
                                        p { style: "margin:4px 0 0;color:var(--cf-text-muted);font-size:11px;",
                                            "Remove these references, then try again."
                                        }
                                    }
                                }
                            }
                        } else if has_immutable_history {
                            // Fast local gate — matches server would return permanently_blocked.
                            div { style: "font-size:12px;color:var(--cf-text-muted);",
                                "This bundle has published compliance history (accepted or deprecated versions). "
                                "Permanent deletion is unavailable. Deactivate assignments and deprecate the bundle to stop using it; historical records remain for auditability."
                            }
                        } else if assigned_count > 0 {
                            // An active assignment already implies immutable assignment
                            // history (the server reports `immutable_assignment_history`),
                            // so permanent deletion is unavailable; no preflight needed.
                            div { style: "font-size:12px;color:var(--cf-text-muted);",
                                "This bundle has active assignments and therefore has immutable assignment history. "
                                "Permanent deletion is unavailable. Deactivate assignments to stop using the bundle; "
                                "historical assignment records are retained for auditability."
                            }
                        } else {
                            // No local blockers detected — fetch authoritative check.
                            button {
                                class: "btn btn-ghost focus-ring",
                                style: "color:#f87171;border-color:rgba(248,113,113,0.3);",
                                disabled: *eligibility_loading.read(),
                                onclick: move |_| {
                                    eligibility_loading.set(true);
                                    eligibility_error.set(None);
                                    let bid = bundle_id;
                                    spawn(async move {
                                        match crate::api::client::fetch_bundle_deletion_eligibility(&bid).await {
                                            Ok(result) => { eligibility.set(Some(result)); }
                                            Err(error) => eligibility_error.set(Some(error.to_string())),
                                        }
                                        eligibility_loading.set(false);
                                    });
                                },
                                Icon { name: IconName::Trash, size: 12 }
                                if *eligibility_loading.read() { " Checking…" } else { " Check deletion eligibility" }
                            }
                        }
                    }
                }

                div { class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: !can_save || *saving.read(),
                        onclick: move |_| {
                            if !can_save { return; }
                            let req = UpdateComplianceBundleRequest {
                                name: name.read().trim().to_string(),
                                framework: framework.read().trim().to_string(),
                                version: Some(version.read().trim().to_string()),
                                description: {
                                    let d = description.read().trim().to_string();
                                    if d.is_empty() { None } else { Some(d) }
                                },
                                required_envs: selected_env_ids.read().clone(),
                                policy_ids: selected_policy_ids.read().clone(),
                                requirement_version_ids: selected_requirement_ids.read().clone(),
                            };
                            saving.set(true);
                            spawn(async move {
                                match update_compliance_bundle(&bundle_id, &req).await {
                                    Ok(updated) => props.on_saved.call(updated),
                                    Err(err) => {
                                        error.set(Some(err.to_string()));
                                        saving.set(false);
                                    }
                                }
                            });
                        },
                        Icon { name: IconName::Check, size: 13 }
                        if *saving.read() { " Saving…" } else { " Save changes" }
                    }
                }

                } // end display:contents wrapper
                } // end else (not confirm_delete)
            }
        }
    }
}

// ─── Delete bundle confirmation ───────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct DeleteBundleConfirmProps {
    bundle_name: String,
    policy_count: usize,
    busy: bool,
    error: Option<String>,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
}

#[component]
fn DeleteBundleConfirm(props: DeleteBundleConfirmProps) -> Element {
    let mut typed = use_signal(String::new);
    let bundle_name = props.bundle_name.clone();
    let matches = *typed.read() == bundle_name;
    let policy_count = props.policy_count;
    let policy_word = if policy_count == 1 {
        "policy"
    } else {
        "policies"
    };

    rsx! {
        div { class: "modal-head", style: "background:rgba(248,113,113,0.06);",
            h2 { style: "color:#fecaca;display:flex;align-items:center;gap:8px;",
                span { style: "color:#f87171;display:inline-flex;", Icon { name: IconName::Warn, size: 16 } }
                "Delete bundle"
            }
            p {
                "This permanently removes the "
                span { class: "mono", style: "font-weight:600;", "{bundle_name}" }
                " compliance bundle."
            }
        }
        div { class: "modal-body",
            div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;",
                span { style: "color:#f87171;display:inline-flex;", Icon { name: IconName::Warn, size: 14 } }
                div { style: "font-size:12px;color:#fecaca;",
                    ul { style: "margin:0;padding-left:16px;line-height:1.6;",
                        li { "The bundle and its mapping of {policy_count} {policy_word} is removed" }
                        li { "Underlying policies are "  em { "not" } " deleted — they remain in the Policies view" }
                        li { "Systems referencing this bundle for compliance will no longer be gated by it" }
                        li { "Collected evidence history is retained for audit" }
                    }
                }
            }
            if let Some(error) = props.error.as_ref() {
                div { class: "sd-callout sd-callout-danger", style: "margin-bottom:12px;", "{error}" }
            }
            div { class: "field",
                label {
                    "Type "
                    span { class: "mono", style: "color:#fecaca;font-weight:700;", "{bundle_name}" }
                    " to confirm"
                }
                input {
                    class: "input focus-ring mono",
                    placeholder: "{bundle_name}",
                    value: "{typed}",
                    autofocus: true,
                    oninput: move |e| typed.set(e.value()),
                }
            }
        }
        div { class: "modal-foot",
            button {
                class: "btn btn-ghost focus-ring",
                disabled: props.busy,
                onclick: move |_| props.on_cancel.call(()),
                "Cancel"
            }
            button {
                class: "btn focus-ring",
                style: if matches {
                    "background:#dc2626;color:white;"
                } else {
                    "background:var(--cf-subtle-bg);color:var(--cf-text-muted);"
                },
                disabled: !matches || props.busy,
                onclick: move |_| { if matches { props.on_confirm.call(()); } },
                Icon { name: IconName::Trash, size: 13 }
                if props.busy { " Deleting…" } else { " Delete bundle" }
            }
        }
    }
}

fn toggle_uuid(signal: &mut Signal<Vec<uuid::Uuid>>, id: uuid::Uuid) {
    let mut next = signal.read().clone();
    if let Some(pos) = next.iter().position(|x| *x == id) {
        next.remove(pos);
    } else {
        next.push(id);
    }
    signal.set(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_all_ordered_preview_check_body_parts() {
        let check = serde_json::json!({
            "body_parts": [
                { "type": "inline", "content": "first inline" },
                { "type": "inline", "content": "second inline" },
                { "type": "reference", "href": "https://example.test/check", "name": "Vendor check" }
            ]
        });

        assert_eq!(
            source_check_body_parts(&check),
            vec![
                SourceCheckBodyPart::Inline("first inline".into()),
                SourceCheckBodyPart::Inline("second inline".into()),
                SourceCheckBodyPart::Reference {
                    href: "https://example.test/check".into(),
                    name: Some("Vendor check".into()),
                },
            ]
        );
    }

    #[test]
    fn custom_framework_catalog_excludes_standard_aliases_and_normalizes_values() {
        assert_eq!(
            custom_framework_values([
                "DISA STIG",
                "nist 800-53",
                "CMMC",
                "cmmc 2.0",
                "CIS Benchmark",
                "  Internal  ",
                "internal",
                "Custom Baseline",
                "",
            ]),
            vec!["Custom Baseline", "Internal"]
        );
    }

    #[test]
    fn legacy_cmmc_is_a_standard_alias_without_normalizing_its_value() {
        assert!(is_standard_bundle_framework("CMMC"));
        assert!(!is_standard_bundle_framework("CMMC 3.0"));
    }

    #[test]
    fn foreign_nix_looking_fix_defaults_to_unbound_without_assertions() {
        let preview = XccdfPreviewResponse {
            sha256: "a".repeat(64),
            filename: Some("foreign.xml".into()),
            document_class: Some("foreign_xccdf".into()),
            fidelity: Some("preserved_opaque".into()),
            fidelity_losses: vec![],
            xccdf_version: Some("1.2".into()),
            benchmark: None,
            profiles: vec![],
            rules: vec![crate::api::models::XccdfRuleInfo {
                id: "foreign-firewall".into(),
                title: Some("Firewall".into()),
                description: Some("Foreign prose".into()),
                severity: Some("medium".into()),
                is_native: false,
                version: None,
                group_id: None,
                platforms: vec![],
                identifiers: vec![],
                checks: vec![serde_json::json!({
                    "content": "networking.firewall.enable = true;"
                })],
                fix: Some(serde_json::json!({
                    "content": "networking.firewall.enable = true;"
                })),
                inferred_assertions: vec![],
                references: vec![],
                has_opaque_xml: true,
            }],
            rule_count: 1,
            profile_count: 0,
            errors: vec![],
            warnings: vec![],
            cf_native_reconciliation: None,
            foreign_stig_reconciliation: None,
        };

        let rules = rules_from_preview(&preview);
        assert_eq!(rules[0].action, "unbound");
        assert!(rules[0].assertions.is_empty());
        assert!(matches!(
            import_action_from_rule(&rules[0]),
            XccdfRuleImportAction::CreateUnbound { .. }
        ));
    }
}

// ── Requirement coverage card ─────────────────────────────────────────────────

/// Renders the requirement coverage card for a selected bundle version.
///
/// Matches the design from commit 861fd877: three summary chips (full/partial/
/// unmapped), expandable to show individual requirement rows grouped by parent.
#[component]
fn RequirementCoverageCard(
    report: BundleCoverageReport,
    expanded: Signal<bool>,
) -> Element {
    let total = report.total_requirements;
    let full = report.full;
    let partial = report.partial;
    let unmapped = report.unmapped;

    rsx! {
         div { class: "card", "data-testid": "requirement-coverage-card", style: "display:flex;flex-direction:column;gap:10px;",
            // Header row with expand toggle.
            div { style: "display:flex;align-items:center;justify-content:space-between;",
                div { style: "font-size:13px;font-weight:600;", "Requirement coverage" }
                button {
                    class: "btn btn-ghost xs focus-ring",
                    style: "font-size:11px;",
                    onclick: move |_| {
                        let current = *expanded.read();
                        expanded.set(!current);
                    },
                    if *expanded.read() { "Collapse" } else { "Expand" }
                }
            }
            // Summary chips.
            div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                div { style: "display:flex;align-items:center;gap:4px;",
                                     span { class: "chip chip-success", "{full}" }
                                     span { style: "font-size:11px;color:var(--cf-text-muted);", "Fully covered" }
                }
                div { style: "display:flex;align-items:center;gap:4px;",
                    span { class: "chip chip-warn", "{partial}" }
                                     span { style: "font-size:11px;color:var(--cf-text-muted);", "Partially covered" }
                }
                div { style: "display:flex;align-items:center;gap:4px;",
                    span { class: "chip chip-neutral", "{unmapped}" }
                                     span { style: "font-size:11px;color:var(--cf-text-muted);", "Unmapped" }
                }
                div { style: "display:flex;align-items:center;gap:4px;margin-left:auto;",
                    span { style: "font-size:11px;color:var(--cf-text-muted);", "{total} total" }
                }
            }
            // Expanded requirement rows.
            if *expanded.read() && !report.rows.is_empty() {
                div { style: "display:flex;flex-direction:column;gap:2px;max-height:360px;overflow-y:auto;",
                    for row in report.rows.iter() {
                        {
                            let row_id = row.requirement_version_id;
                            let coverage_color = match row.coverage {
                                RequirementCoverage::Full => "var(--cf-success)",
                                RequirementCoverage::Partial => "var(--cf-warn)",
                                RequirementCoverage::Unmapped => "var(--cf-text-muted)",
                            };
                            let coverage_label = match row.coverage {
                                RequirementCoverage::Full => "Full",
                                RequirementCoverage::Partial => "Partial",
                                RequirementCoverage::Unmapped => "Unmapped",
                            };
                            rsx! {
                                 div { key: "{row_id}", "data-testid": "requirement-coverage-row",
                                    style: "display:grid;grid-template-columns:auto 1fr auto;gap:8px;align-items:center;padding:5px 8px;border-radius:6px;font-size:11px;background:var(--cf-subtle-bg);",
                                    span { class: "mono", style: "color:var(--cf-text-secondary);", "{row.external_id}" }
                                    span { style: "color:var(--cf-text-primary);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                                        {row.title.as_deref().unwrap_or("-")}
                                    }
                                     div { style: "display:flex;flex-direction:column;align-items:flex-end;gap:3px;",
                                         span { style: "color:{coverage_color};font-weight:600;white-space:nowrap;", "{coverage_label}" }
                                         for mapping in row.mappings.iter() {
                                             span { style: "font-size:10px;color:var(--cf-text-muted);white-space:nowrap;", "{mapping.policy_name} · {mapping.relationship} · {mapping.coverage}" }
                                         }
                                     }
                                 }
                            }
                        }
                    }
                }
            } else if *expanded.read() && report.rows.is_empty() {
                div { style: "font-size:11px;color:var(--cf-text-muted);", "No requirements in this bundle version's baseline." }
            }
        }
    }
}
