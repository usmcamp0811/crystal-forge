use dioxus::prelude::*;

use crate::api::client::{
    create_compliance_bundle, delete_compliance_bundle, fetch_compliance_bundle_systems,
    fetch_compliance_bundles, fetch_compliance_system_evidence, fetch_environments, fetch_policies,
    update_compliance_bundle,
};
use crate::api::models::{
    ComplianceBundleSummary, ComplianceBundleSystemsResponse, ComplianceEvidenceResponse,
    CreateComplianceBundleRequest, DeploymentPolicySummary, EnvironmentSummary,
    UpdateComplianceBundleRequest,
};
use crate::components::compliance::{
    BundleCatalog, BundleHeader, EvidenceDrawer, ScoreStrip, SystemsMatrix,
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
    let mut policies = use_signal(Vec::<DeploymentPolicySummary>::new);
    let mut environments = use_signal(Vec::<EnvironmentSummary>::new);
    let mut sys_filter = use_signal(|| "all".to_string());

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
    let mut start_systems_fetch = move |bundle_id: uuid::Uuid| {
        let gen_id = *systems_gen.read() + 1;
        systems_gen.set(gen_id);
        systems.set(None);
        systems_error.set(None);
        systems_loading.set(true);
        spawn(async move {
            match fetch_compliance_bundle_systems(&bundle_id).await {
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
                    bundles.set(items);
                    selected_bundle_id.set(first_id);
                    // loaded = true before the systems fetch so the bundle list
                    // renders immediately; systems has its own loading indicator.
                    loaded.set(true);
                    if let Some(bundle_id) = first_id {
                        start_systems_fetch(bundle_id);
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
        start_systems_fetch(bundle_id);
    };

    let on_evidence = move |system_id: uuid::Uuid| {
        if let Some(bundle_id) = *selected_bundle_id.read() {
            evidence.set(None);
            evidence_error.set(None);
            let gen_id = *evidence_gen.read() + 1;
            evidence_gen.set(gen_id);
            spawn(async move {
                match fetch_compliance_system_evidence(&bundle_id, &system_id).await {
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
                                // Import CF bundle: requires the full import UI.
                                items.push(IOMenuItem::disabled(
                                    "Import Crystal Forge bundle (.xml)",
                                    "CF bundle import coming in a later phase",
                                ));
                                items.push(IOMenuItem::Separator);
                            }
                            // Export XCCDF: enabled when a bundle version is selected.
                            let bundle_selected = selected_bundle_id.read().is_some();
                            items.push(if bundle_selected {
                                IOMenuItem::action_with_icon(
                                    "Export this bundle (XCCDF .xml)",
                                    IconName::Download,
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
                                    0 => show_import_stig.set(true),
                                    2 => {
                                        // Export XCCDF: trigger a download of the selected bundle.
                                        if let Some(bid) = *selected_bundle_id.read() {
                                            let url = format!(
                                                "{}/api/v1/compliance/bundle-versions/{}/xccdf",
                                                crate::api::client::base_url(),
                                                bid
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
                                        if let Some(bid) = *selected_bundle_id.read() {
                                            let url = format!(
                                                "{}/api/v1/compliance/bundle-versions/{}/xccdf",
                                                crate::api::client::base_url(),
                                                bid
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
                    }
                    // Right: bundle content
                    if let Some(bundle) = selected_bundle {
                        div { style: "display:flex;flex-direction:column;gap:14px;min-width:0;",
                            BundleHeader {
                                bundle: bundle.clone(),
                                on_edit: move |_| show_edit_bundle.set(true),
                                is_admin,
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
                                    start_systems_fetch(bid);
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
                environments: environments.read().clone(),
                on_close: move |_| show_new_bundle.set(false),
                on_created: move |bundle: ComplianceBundleSummary| {
                    let id = bundle.id;
                    let mut next = bundles.read().clone();
                    next.push(bundle);
                    bundles.set(next);
                    selected_bundle_id.set(Some(id));
                    evidence.set(None);
                    evidence_error.set(None);
                    let eg = *evidence_gen.read() + 1;
                    evidence_gen.set(eg);
                    show_new_bundle.set(false);
                    start_systems_fetch(id);
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
                    environments: environments.read().clone(),
                    on_close: move |_| show_edit_bundle.set(false),
                    on_saved: move |updated: ComplianceBundleSummary| {
                        let id = updated.id;
                        let mut next = bundles.read().clone();
                        if let Some(pos) = next.iter().position(|b| b.id == id) {
                            next[pos] = updated;
                        }
                        bundles.set(next);
                        show_edit_bundle.set(false);
                        start_systems_fetch(id);
                    },
                    on_deleted: move |deleted_id: uuid::Uuid| {
                        let mut next = bundles.read().clone();
                        next.retain(|b| b.id != deleted_id);
                        let next_id = next.first().map(|b| b.id);
                        bundles.set(next);
                        selected_bundle_id.set(next_id);
                        evidence.set(None);
                        evidence_error.set(None);
                        let eg = *evidence_gen.read() + 1;
                        evidence_gen.set(eg);
                        show_edit_bundle.set(false);
                        if let Some(nid) = next_id {
                            start_systems_fetch(nid);
                        }
                    },
                }
            }
        }

        // ── Import STIG modal (admin-only) ────────────────────────────────
        if is_admin && *show_import_stig.read() {
            ImportStigModal {
                environments: environments.read().clone(),
                on_close: move |_| show_import_stig.set(false),
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

// ─── Import STIG modal ────────────────────────────────────────────────────────
//
// Implements the 4-step design reference (upload → review → refine → done).
// The backend wiring (XCCDF parse + policy/bundle creation) is tracked in
// TASK-365; for now the modal is fully interactive using sample data when
// "Try with a sample RHEL 9 STIG" is clicked.  File upload advances to review
// using the sample data (real XCCDF parsing requires TASK-365).

#[derive(Clone, PartialEq)]
struct StigRule {
    rule_id: String,
    stig_id: String,
    severity: String, // "high" | "medium" | "low"
    title: String,
    fixtext: String,
    check: String,
    srg: String,
    selected: bool,
}

fn sample_stig_rules() -> Vec<StigRule> {
    vec![
        StigRule {
            rule_id: "RHEL-09-255040".into(),
            stig_id: "RHEL-09-255040".into(),
            severity: "high".into(),
            title: "RHEL 9 must disable SSH root login.".into(),
            fixtext: "Set PermitRootLogin no in /etc/ssh/sshd_config.d/.".into(),
            check: "Verify sshd -T | grep permitrootlogin returns no.".into(),
            srg: "SRG-OS-000109".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-255095".into(),
            stig_id: "RHEL-09-255095".into(),
            severity: "medium".into(),
            title: "RHEL 9 SSH must use FIPS-validated MACs.".into(),
            fixtext: "Configure approved MACs (hmac-sha2-512, hmac-sha2-256).".into(),
            check: "Verify MACs in sshd config.".into(),
            srg: "SRG-OS-000250".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-211010".into(),
            stig_id: "RHEL-09-211010".into(),
            severity: "high".into(),
            title: "RHEL 9 must enable FIPS mode.".into(),
            fixtext: "Boot kernel with fips=1 and install dracut-fips.".into(),
            check: "cat /proc/sys/crypto/fips_enabled returns 1.".into(),
            srg: "SRG-OS-000478".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-654010".into(),
            stig_id: "RHEL-09-654010".into(),
            severity: "medium".into(),
            title: "RHEL 9 must enable auditd.".into(),
            fixtext: "systemctl enable --now auditd.".into(),
            check: "systemctl is-active auditd returns active.".into(),
            srg: "SRG-OS-000062".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-654155".into(),
            stig_id: "RHEL-09-654155".into(),
            severity: "medium".into(),
            title: "RHEL 9 must audit execution of privileged functions.".into(),
            fixtext: "Add execve audit rules for b32/b64.".into(),
            check: "auditctl -l shows execve rules.".into(),
            srg: "SRG-OS-000326".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-271010".into(),
            stig_id: "RHEL-09-271010".into(),
            severity: "medium".into(),
            title: "RHEL 9 must display the Standard Mandatory DoD banner.".into(),
            fixtext: "Set /etc/issue to the DoD consent banner.".into(),
            check: "Verify /etc/issue contents.".into(),
            srg: "SRG-OS-000023".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-251010".into(),
            stig_id: "RHEL-09-251010".into(),
            severity: "high".into(),
            title: "RHEL 9 must enable the firewalld default-deny policy.".into(),
            fixtext: "Set firewalld default zone to drop.".into(),
            check: "firewall-cmd --get-default-zone returns drop.".into(),
            srg: "SRG-OS-000480".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-411015".into(),
            stig_id: "RHEL-09-411015".into(),
            severity: "medium".into(),
            title: "RHEL 9 must lock accounts after 3 failed logon attempts.".into(),
            fixtext: "Configure pam_faillock deny=3.".into(),
            check: "Verify faillock config.".into(),
            srg: "SRG-OS-000021".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-412035".into(),
            stig_id: "RHEL-09-412035".into(),
            severity: "low".into(),
            title: "RHEL 9 must set an idle session timeout.".into(),
            fixtext: "Set TMOUT=600 in /etc/profile.d/.".into(),
            check: "Verify TMOUT export.".into(),
            srg: "SRG-OS-000163".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-672010".into(),
            stig_id: "RHEL-09-672010".into(),
            severity: "low".into(),
            title: "RHEL 9 must synchronize time with an authoritative source.".into(),
            fixtext: "Configure chrony with authorized servers.".into(),
            check: "chronyc sources shows server.".into(),
            srg: "SRG-OS-000355".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-231010".into(),
            stig_id: "RHEL-09-231010".into(),
            severity: "medium".into(),
            title: "RHEL 9 must encrypt all non-boot partitions (LUKS).".into(),
            fixtext: "Provision LUKS on data partitions.".into(),
            check: "lsblk shows crypt devices.".into(),
            srg: "SRG-OS-000405".into(),
            selected: true,
        },
        StigRule {
            rule_id: "RHEL-09-215015".into(),
            stig_id: "RHEL-09-215015".into(),
            severity: "low".into(),
            title: "RHEL 9 must remove unauthorized package repositories.".into(),
            fixtext: "Remove unapproved .repo files.".into(),
            check: "dnf repolist matches baseline.".into(),
            srg: "SRG-OS-000366".into(),
            selected: true,
        },
    ]
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

#[derive(Props, Clone, PartialEq)]
struct ImportStigModalProps {
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
}

#[component]
fn ImportStigModal(props: ImportStigModalProps) -> Element {
    // step: "upload" | "review" | "refine" | "done"
    let mut step = use_signal(|| "upload".to_string());
    let mut rules = use_signal(|| Vec::<StigRule>::new());
    let mut bundle_name = use_signal(String::new);
    let mut file_name = use_signal(String::new);
    let mut bench_title = use_signal(String::new);
    let mut bench_ver = use_signal(String::new);
    let mut selected_envs = use_signal(|| Vec::<String>::new());
    let mut cursor = use_signal(|| 0usize);
    let mut parse_error = use_signal(|| Option::<String>::None);
    // done-step summary
    let mut done_total = use_signal(|| 0usize);

    let all_env_names: Vec<String> = props.environments.iter().map(|e| e.name.clone()).collect();

    let load_sample = {
        let all_env_names = all_env_names.clone();
        move |_| {
            let sample = sample_stig_rules();
            let title = "Red Hat Enterprise Linux 9 STIG".to_string();
            let ver = "V1R5".to_string();
            bundle_name.set(title.clone());
            bench_title.set(title);
            bench_ver.set(ver);
            file_name.set("RHEL_9_STIG_V1R5.xml (sample)".to_string());
            rules.set(sample);
            parse_error.set(None);
            // Pre-select all available environments so the flow works immediately,
            // matching the design reference which defaults to ["production"].
            selected_envs.set(all_env_names.clone());
            step.set("review".to_string());
        }
    };

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

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal",
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
                            "Import STIG"
                        }
                        p {
                            "Upload a DISA XCCDF benchmark ("
                            span { class: "mono", ".xml" }
                            "). Crystal Forge parses each rule into a policy and assembles them into a compliance bundle."
                        }
                    }
                    div { class: "modal-body",
                        // ── Not-yet-implemented notice ─────────────────
                        div { class: "sd-callout sd-callout-warn", style: "margin-bottom:16px;",
                            Icon { name: IconName::Warn, size: 14 }
                            div { style: "font-size:12px;",
                                strong { "File upload is not yet wired." }
                                " XCCDF parsing and backend bundle creation are tracked in "
                                span { class: "mono", style: "font-weight:600;", "TASK-365" }
                                ". Use the sample below to preview the full import flow."
                            }
                        }

                        // ── Drop zone (preview — file input disabled) ──
                        div {
                            class: "focus-ring",
                            style: "border:2px dashed var(--cf-divider);background:var(--cf-card-bg);\
                                    border-radius:12px;padding:38px 20px;text-align:center;\
                                    opacity:0.55;cursor:not-allowed;",
                            div { style: "font-size:30px;margin-bottom:8px;", "📄" }
                            div { style: "font-size:14px;font-weight:600;",
                                "Drop an XCCDF .xml here, or click to browse"
                            }
                            div { style: "font-size:12px;color:var(--cf-text-muted);margin-top:4px;",
                                "DISA STIG / SCAP benchmark · file parsing coming in TASK-365"
                            }
                        }

                        if let Some(err) = parse_error.read().as_ref() {
                            div { class: "sd-callout sd-callout-danger", style: "margin-top:12px;",
                                Icon { name: IconName::Warn, size: 13 }
                                div { style: "font-size:12px;", "{err}" }
                            }
                        }

                        div { style: "display:flex;align-items:center;gap:10px;margin:16px 0 4px;",
                            div { style: "flex:1;height:1px;background:var(--cf-divider);" }
                            span { style: "font-size:11px;color:var(--cf-text-muted);", "or" }
                            div { style: "flex:1;height:1px;background:var(--cf-divider);" }
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            style: "width:100%;",
                            onclick: load_sample,
                            Icon { name: IconName::Shield, size: 13 }
                            " Try with a sample RHEL 9 STIG"
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
                // STEP: review
                // ══════════════════════════════════════════════════════
                if *step.read() == "review" {
                    div { class: "modal-head",
                        h2 { style: "display:flex;align-items:center;gap:8px;",
                            Icon { name: IconName::Shield, size: 14 }
                            "Review imported controls"
                        }
                        p {
                            span { class: "mono", "{file_name.read()}" }
                            " · {bench_title.read()} · "
                            strong { "{bench_ver.read()}" }
                        }
                    }
                    div { class: "modal-body", style: "overflow-y:auto;",

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
                            div { style: "display:flex;flex-wrap:wrap;gap:6px;",
                                for env in props.environments.iter() {
                                    {
                                        let e_name  = env.name.clone();
                                        let e_color = env.color_hex.clone();
                                        let on = selected_envs.read().contains(&e_name);
                                        rsx! {
                                            button {
                                                class: "focus-ring",
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

                            div { style: "display:flex;flex-direction:column;gap:5px;max-height:280px;overflow-y:auto;margin-top:8px;padding-right:2px;",
                                for (i, rule) in rules.read().iter().enumerate() {
                                    {
                                        let is_sel = rule.selected;
                                        let color  = sev_color(&rule.severity);
                                        let cat    = sev_cat(&rule.severity);
                                        let title  = rule.title.clone();
                                        let stig   = rule.stig_id.clone();
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
                                                // Title + STIG ID
                                                span { style: "min-width:0;",
                                                    span { style: "font-size:12.5px;font-weight:600;display:block;line-height:1.4;", "{title}" }
                                                    span { class: "mono", style: "font-size:10.5px;color:var(--cf-text-muted);",
                                                        "{stig}"
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
                        div { class: "sd-callout sd-callout-info",
                            Icon { name: IconName::Check, size: 13 }
                            div { style: "font-size:12px;",
                                "Creates "
                                strong { "{sel_count}" }
                                if sel_count == 1 { " security policy" } else { " security policies" }
                                " and one bundle. Each control maps to a policy with its check + fix as evidence requirements. Existing policies with the same ID are reused, not duplicated."
                            }
                        }
                    }
                    div { class: "modal-foot", style: "justify-content:space-between;",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| {
                                step.set("upload".to_string());
                                rules.set(Vec::new());
                                parse_error.set(None);
                            },
                            Icon { name: IconName::ArrowLeft, size: 13 }
                            " Back"
                        }
                        div { style: "display:flex;gap:8px;",
                            button {
                                class: "btn btn-ghost focus-ring",
                                disabled: !can_advance,
                                style: if !can_advance { "opacity:0.5;cursor:not-allowed;" } else { "" },
                                title: "Create all selected policies as-is, skipping per-control review",
                                onclick: move |_| {
                                    if can_advance {
                                        done_total.set(sel_count);
                                        step.set("done".to_string());
                                    }
                                },
                                "Skip & create all"
                            }
                            button {
                                class: "btn btn-primary focus-ring",
                                disabled: !can_advance,
                                style: if !can_advance { "opacity:0.5;cursor:not-allowed;" } else { "" },
                                onclick: move |_| {
                                    if can_advance {
                                        cursor.set(0);
                                        step.set("refine".to_string());
                                    }
                                },
                                "Refine {sel_count} "
                                if sel_count == 1 { "policy" } else { "policies" }
                                Icon { name: IconName::ChevronRight, size: 13 }
                            }
                        }
                    }
                }

                // ══════════════════════════════════════════════════════
                // STEP: refine (per-control walkthrough)
                // ══════════════════════════════════════════════════════
                if *step.read() == "refine" {
                    {
                        let sel: Vec<StigRule> = rules.read().iter().filter(|r| r.selected).cloned().collect();
                        let total = sel.len();
                        let cur   = (*cursor.read()).min(total.saturating_sub(1));
                        let is_last = cur + 1 >= total;

                        if let Some(rule) = sel.get(cur) {
                            let rule = rule.clone();
                            let rule_id  = rule.rule_id.clone();
                            let rule_id2 = rule.rule_id.clone();
                            let rule_id3 = rule.rule_id.clone();
                            let rule_id4 = rule.rule_id.clone();
                            let sev_col  = sev_color(&rule.severity).to_string();
                            let cat_str  = format!("{} · {}", sev_cat(&rule.severity), sev_label(&rule.severity));
                            let pct      = ((cur + 1) as f64 / total as f64 * 100.0) as u32;

                            rsx! {
                                div { class: "modal-head",
                                    div { style: "display:flex;align-items:center;justify-content:space-between;gap:10px;",
                                        h2 { style: "display:flex;align-items:center;gap:8px;",
                                            Icon { name: IconName::Shield, size: 14 }
                                            "Refine policy {cur + 1} of {total}"
                                        }
                                        span { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);",
                                            "{rule.stig_id}"
                                        }
                                    }
                                    // Progress bar
                                    div { style: "height:4px;border-radius:99px;background:var(--cf-divider);margin-top:8px;overflow:hidden;",
                                        div { style: "height:100%;width:{pct}%;background:var(--cf-brand-purple);transition:width .2s;" }
                                    }
                                }
                                div { class: "modal-body", style: "overflow-y:auto;",
                                    // Chips row
                                    div { style: "display:flex;gap:8px;align-items:center;margin-bottom:12px;",
                                        span { class: "chip chip-info", "Security & hardening" }
                                        if !rule.srg.is_empty() {
                                            span { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "{rule.srg}" }
                                        }
                                        div { style: "flex:1;" }
                                        span { style: "font-size:11px;color:var(--cf-text-muted);",
                                            "policy id: "
                                            span { class: "mono", "stig-{rule.stig_id.to_lowercase().replace([' ', '_'], \"-\")}" }
                                        }
                                    }

                                    // Policy name
                                    div { class: "field",
                                        label { "Policy name" }
                                        input {
                                            class: "input focus-ring mono",
                                            value: "{rule.stig_id}",
                                            oninput: move |e| {
                                                let mut r = rules.write();
                                                if let Some(rule) = r.iter_mut().find(|r| r.rule_id == rule_id) {
                                                    rule.stig_id = e.value();
                                                }
                                            },
                                        }
                                    }

                                    // Severity seg
                                    div { class: "field",
                                        label { "Severity" }
                                        div { class: "seg", style: "width:fit-content;",
                                            for sev in ["high", "medium", "low"] {
                                                {
                                                    let sev_s = sev.to_string();
                                                    let is_active = rule.severity == sev;
                                                    let c = sev_color(sev);
                                                    let rid = rule_id2.clone();
                                                    rsx! {
                                                        button {
                                                            class: if is_active { "active" } else { "" },
                                                            style: if is_active { format!("color:{c};") } else { "".to_string() },
                                                            onclick: move |_| {
                                                                let mut r = rules.write();
                                                                if let Some(rule) = r.iter_mut().find(|r| r.rule_id == rid) {
                                                                    rule.severity = sev_s.clone();
                                                                }
                                                            },
                                                            "{sev_cat(sev)} · {sev_label(sev)}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Description
                                    div { class: "field",
                                        label { "Description / control statement" }
                                        textarea {
                                            class: "input focus-ring",
                                            rows: 2,
                                            style: "resize:vertical;",
                                            value: "{rule.title}",
                                            oninput: move |e| {
                                                let mut r = rules.write();
                                                if let Some(rule) = r.iter_mut().find(|r| r.rule_id == rule_id3) {
                                                    rule.title = e.value();
                                                }
                                            },
                                        }
                                    }

                                    // Check
                                    div { class: "field",
                                        label {
                                            "Compliance check "
                                            span { style: "color:var(--cf-text-muted);font-weight:400;",
                                                "· becomes the policy assertion"
                                            }
                                        }
                                        textarea {
                                            class: "input focus-ring mono",
                                            rows: 3,
                                            style: "resize:vertical;font-size:12px;",
                                            placeholder: "How Crystal Forge verifies this control…",
                                            value: "{rule.check}",
                                            oninput: move |e| {
                                                let mut r = rules.write();
                                                if let Some(rule) = r.iter_mut().find(|r| r.rule_id == rule_id4) {
                                                    rule.check = e.value();
                                                }
                                            },
                                        }
                                    }

                                    // Fixtext
                                    div { class: "field",
                                        {
                                            let rid = rule.rule_id.clone();
                                            rsx! {
                                                label {
                                                    "Remediation / rationale "
                                                    span { style: "color:var(--cf-text-muted);font-weight:400;",
                                                        "· stored as policy rationale"
                                                    }
                                                }
                                                textarea {
                                                    class: "input focus-ring",
                                                    rows: 2,
                                                    style: "resize:vertical;font-size:12px;",
                                                    value: "{rule.fixtext}",
                                                    oninput: move |e| {
                                                        let mut r = rules.write();
                                                        if let Some(rule) = r.iter_mut().find(|r| r.rule_id == rid) {
                                                            rule.fixtext = e.value();
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }

                                    // Info callout
                                    div { class: "sd-callout sd-callout-info",
                                        Icon { name: IconName::Check, size: 13 }
                                        div { style: "font-size:12px;",
                                            "Evidence required: "
                                            strong { "command output" }
                                            " matching the check, plus an "
                                            strong { "agent attestation" }
                                            " that the fix is applied. Edit evidence later from the Policies view."
                                        }
                                    }
                                }
                                div { class: "modal-foot", style: "justify-content:space-between;",
                                    div { style: "display:flex;gap:8px;",
                                        button {
                                            class: "btn btn-ghost focus-ring",
                                            onclick: move |_| {
                                                if cur == 0 {
                                                    step.set("review".to_string());
                                                } else {
                                                    cursor.set(cur - 1);
                                                }
                                            },
                                            Icon { name: IconName::ArrowLeft, size: 13 }
                                            if cur == 0 { " Back to list" } else { " Previous" }
                                        }
                                        button {
                                            class: "btn btn-ghost focus-ring",
                                            style: "color:#f87171;",
                                            title: "Exclude this control from the bundle",
                                            onclick: move |_| {
                                                {
                                                    let mut r = rules.write();
                                                    if let Some(rule) = r.iter_mut().find(|r| r.rule_id == rule.rule_id) {
                                                        rule.selected = false;
                                                    }
                                                }
                                                let remaining = rules.read().iter().filter(|r| r.selected).count();
                                                if remaining == 0 {
                                                    step.set("review".to_string());
                                                } else if is_last && cur > 0 {
                                                    cursor.set(cur - 1);
                                                }
                                            },
                                            "Exclude"
                                        }
                                    }
                                    div { style: "display:flex;gap:8px;align-items:center;",
                                        span { style: "font-size:11px;color:var(--cf-text-muted);",
                                            "{cur + 1} / {total}"
                                        }
                                        if is_last {
                                            button {
                                                class: "btn btn-primary focus-ring",
                                                onclick: move |_| {
                                                    done_total.set(sel_count);
                                                    step.set("done".to_string());
                                                },
                                                Icon { name: IconName::Check, size: 13 }
                                                " Create bundle + {sel_count} "
                                                if sel_count == 1 { "policy" } else { "policies" }
                                            }
                                        } else {
                                            button {
                                                class: "btn btn-primary focus-ring",
                                                onclick: move |_| cursor.set(cur + 1),
                                                "Next "
                                                Icon { name: IconName::ChevronRight, size: 13 }
                                            }
                                        }
                                    }
                                }
                            }
                        } else { rsx! {} }
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
                            "Bundle created"
                        }
                        p {
                            span { class: "mono", style: "font-weight:600;", "{bundle_name.read()}" }
                            " is ready."
                        }
                    }
                    div { class: "modal-body",
                        // Stats grid — controls / new policies / reused
                        div { style: "display:grid;grid-template-columns:repeat(3,1fr);gap:10px;",
                            for (n, label) in [
                                (*done_total.read(), "controls"),
                                (*done_total.read(), "new policies"),
                                (0usize, "reused"),
                            ] {
                                div { class: "card", style: "padding:14px 12px;text-align:center;",
                                    div { style: "font-size:24px;font-weight:700;", "{n}" }
                                    div { style: "font-size:11px;color:var(--cf-text-muted);", "{label}" }
                                }
                            }
                        }
                        div { class: "sd-callout sd-callout-info", style: "margin-top:12px;",
                            Icon { name: IconName::Shield, size: 13 }
                            div { style: "font-size:12px;",
                                "New policies appear in the "
                                strong { "Policies" }
                                " view under "
                                strong { "Security & hardening" }
                                ". The bundle now gates the environments you selected"
                                if !selected_envs.read().is_empty() {
                                    ": "
                                    strong { { selected_envs.read().join(", ") } }
                                }
                                "."
                            }
                        }
                        // Honest note: backend wiring is TASK-365
                        div { class: "sd-callout sd-callout-warn", style: "margin-top:10px;",
                            Icon { name: IconName::Warn, size: 13 }
                            div { style: "font-size:12px;",
                                strong { "Preview only — bundle not yet saved." }
                                " Backend persistence is tracked in "
                                span { class: "mono", style: "font-weight:600;", "TASK-365" }
                                ". The policies and bundle you reviewed will be used as the starting point when that work lands."
                            }
                        }
                    }
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-primary focus-ring",
                            onclick: move |_| props.on_close.call(()),
                            "View bundle"
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
                                        match fetch_compliance_system_evidence(&bundle.id, sys_id).await {
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

#[derive(Props, Clone, PartialEq)]
struct NewBundleModalProps {
    policies: Vec<DeploymentPolicySummary>,
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
    let mut query = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);

    let can_save = !name.read().trim().is_empty() && !selected_policy_ids.read().is_empty();

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
                        div { class: "field",
                            label { "Bundle name" }
                            input {
                                class: "input focus-ring",
                                value: "{name}",
                                placeholder: "e.g. DISA RHEL9 STIG (v1r5)",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "field",
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
                    div { style: "display:grid;grid-template-columns:1fr 2fr;gap:14px;",
                        div { class: "field",
                            label { "Framework" }
                            select {
                                class: "input focus-ring",
                                value: "{framework}",
                                onchange: move |e| framework.set(e.value()),
                                for opt in ["DISA STIG","NIST 800-53","CMMC","CIS Benchmark","Internal","Custom"] {
                                    option { value: "{opt}", "{opt}" }
                                }
                            }
                        }
                        div { class: "field",
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

                    // Policy picker
                    div {
                        style: "padding:14px;border:1px solid var(--cf-divider);border-radius:10px;background:color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg));",
                        div {
                            style: "display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:10px;",
                            div {
                                style: "font-size:13px;font-weight:600;display:flex;align-items:center;gap:6px;",
                                Icon { name: IconName::File, size: 13 }
                                "Controls in this bundle"
                                span { class: "chip chip-info", style: "font-size:10px;", "{selected_policy_ids.read().len()} selected" }
                            }
                            div { class: "filter-search", style: "max-width:200px;margin:0;",
                                Icon { name: IconName::Search, size: 14 }
                                input {
                                    class: "input focus-ring",
                                    placeholder: "Filter policies…",
                                    value: "{query}",
                                    oninput: move |e| query.set(e.value()),
                                }
                            }
                        }
                        div { style: "display:flex;flex-direction:column;gap:4px;max-height:260px;overflow-y:auto;",
                            if filtered_policies.is_empty() {
                                div {
                                    style: "font-size:12px;color:var(--cf-text-muted);padding:16px 0;text-align:center;",
                                    "No policies match. Define new policies in the Policies view."
                                }
                            }
                            for policy in filtered_policies.iter() {
                                {
                                    let pid = policy.id;
                                    let pname = policy.name.clone();
                                    let pdesc = policy.description.clone().unwrap_or_default();
                                    let ptype = policy.policy_type.clone();
                                    let on = selected_policy_ids.read().contains(&pid);
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            style: if on {
                                                "all:unset;cursor:pointer;display:flex;gap:10px;align-items:flex-start;padding:9px 11px;border-radius:8px;border:1px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 9%,var(--cf-card-bg));"
                                            } else {
                                                "all:unset;cursor:pointer;display:flex;gap:10px;align-items:flex-start;padding:9px 11px;border-radius:8px;border:1px solid var(--cf-divider);background:var(--cf-card-bg);"
                                            },
                                            onclick: move |_| toggle_uuid(&mut selected_policy_ids, pid),
                                            // Checkbox
                                            div {
                                                style: if on {
                                                    "width:16px;height:16px;border-radius:5px;flex-shrink:0;margin-top:1px;border:1.5px solid var(--cf-brand-purple);background:var(--cf-brand-purple);display:grid;place-items:center;"
                                                } else {
                                                    "width:16px;height:16px;border-radius:5px;flex-shrink:0;margin-top:1px;border:1.5px solid var(--cf-card-border);background:transparent;display:grid;place-items:center;"
                                                },
                                                if on {
                                                    span { style: "color:#fff;display:inline-flex;", Icon { name: IconName::Check, size: 11 } }
                                                }
                                            }
                                            div { style: "min-width:0;flex:1;",
                                                div { style: "display:flex;align-items:center;gap:8px;",
                                                    span { class: "mono", style: "font-size:12px;font-weight:600;", "{pname}" }
                                                    span { class: "chip chip-unknown", style: "font-size:9px;", "{ptype}" }
                                                }
                                                div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:2px;", "{pdesc}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !can_save && !name.read().trim().is_empty() {
                        div { class: "help", style: "color:#fbbf24;margin-top:8px;",
                            Icon { name: IconName::Warn, size: 10 }
                            " Select at least one policy. A bundle is a collection of policies that together represent a standard."
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
    let mut query = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);

    let can_save = !name.read().trim().is_empty() && !selected_policy_ids.read().is_empty();

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
                        on_cancel: move |_| confirm_delete.set(false),
                        on_confirm: move |_| {
                            let bid = bundle_id;
                            spawn(async move {
                                match delete_compliance_bundle(&bid).await {
                                    Ok(()) => props.on_deleted.call(bid),
                                    Err(err) => error.set(Some(err.to_string())),
                                }
                            });
                        },
                    }
                } else {
                // Fragment wrapper required — Dioxus if/else each branch must be one element
                div { style: "display:contents;",

                div { class: "modal-head",
                    h2 {
                        Icon { name: IconName::Shield, size: 14 }
                        " Edit compliance bundle"
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

                    div { style: "display:grid;grid-template-columns:2fr 1fr;gap:14px;",
                        div { class: "field",
                            label { "Bundle name" }
                            input {
                                class: "input focus-ring",
                                value: "{name}",
                                placeholder: "e.g. DISA RHEL9 STIG (v1r5)",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "field",
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

                    div { style: "display:grid;grid-template-columns:1fr 2fr;gap:14px;",
                        div { class: "field",
                            label { "Framework" }
                            select {
                                class: "input focus-ring",
                                value: "{framework}",
                                onchange: move |e| framework.set(e.value()),
                                for opt in ["DISA STIG","NIST 800-53","CMMC","CIS Benchmark","Internal","Custom"] {
                                    option { value: "{opt}", "{opt}" }
                                }
                            }
                        }
                        div { class: "field",
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

                    div {
                        style: "padding:14px;border:1px solid var(--cf-divider);border-radius:10px;background:color-mix(in oklab,var(--cf-page-bg) 50%,var(--cf-card-bg));",
                        div {
                            style: "display:flex;align-items:center;justify-content:space-between;gap:8px;margin-bottom:10px;",
                            div {
                                style: "font-size:13px;font-weight:600;display:flex;align-items:center;gap:6px;",
                                Icon { name: IconName::File, size: 13 }
                                "Controls in this bundle"
                                span { class: "chip chip-info", style: "font-size:10px;", "{selected_policy_ids.read().len()} selected" }
                            }
                            div { class: "filter-search", style: "max-width:200px;margin:0;",
                                Icon { name: IconName::Search, size: 14 }
                                input {
                                    class: "input focus-ring",
                                    placeholder: "Filter policies…",
                                    value: "{query}",
                                    oninput: move |e| query.set(e.value()),
                                }
                            }
                        }
                        div { style: "display:flex;flex-direction:column;gap:4px;max-height:260px;overflow-y:auto;",
                            if filtered_policies.is_empty() {
                                div {
                                    style: "font-size:12px;color:var(--cf-text-muted);padding:16px 0;text-align:center;",
                                    "No policies match. Define new policies in the Policies view."
                                }
                            }
                            for policy in filtered_policies.iter() {
                                {
                                    let pid = policy.id;
                                    let pname = policy.name.clone();
                                    let pdesc = policy.description.clone().unwrap_or_default();
                                    let ptype = policy.policy_type.clone();
                                    let on = selected_policy_ids.read().contains(&pid);
                                    rsx! {
                                        button {
                                            class: "focus-ring",
                                            style: if on {
                                                "all:unset;cursor:pointer;display:flex;gap:10px;align-items:flex-start;padding:9px 11px;border-radius:8px;border:1px solid var(--cf-brand-purple);background:color-mix(in oklab,var(--cf-brand-purple) 9%,var(--cf-card-bg));"
                                            } else {
                                                "all:unset;cursor:pointer;display:flex;gap:10px;align-items:flex-start;padding:9px 11px;border-radius:8px;border:1px solid var(--cf-divider);background:var(--cf-card-bg);"
                                            },
                                            onclick: move |_| toggle_uuid(&mut selected_policy_ids, pid),
                                            div {
                                                style: if on {
                                                    "width:16px;height:16px;border-radius:5px;flex-shrink:0;margin-top:1px;border:1.5px solid var(--cf-brand-purple);background:var(--cf-brand-purple);display:grid;place-items:center;"
                                                } else {
                                                    "width:16px;height:16px;border-radius:5px;flex-shrink:0;margin-top:1px;border:1.5px solid var(--cf-card-border);background:transparent;display:grid;place-items:center;"
                                                },
                                                if on {
                                                    span { style: "color:#fff;display:inline-flex;", Icon { name: IconName::Check, size: 11 } }
                                                }
                                            }
                                            div { style: "min-width:0;flex:1;",
                                                div { style: "display:flex;align-items:center;gap:8px;",
                                                    span { class: "mono", style: "font-size:12px;font-weight:600;", "{pname}" }
                                                    span { class: "chip chip-unknown", style: "font-size:9px;", "{ptype}" }
                                                }
                                                div { style: "font-size:11px;color:var(--cf-text-muted);margin-top:2px;", "{pdesc}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !can_save && !name.read().trim().is_empty() {
                        div { class: "help", style: "color:#fbbf24;margin-top:8px;",
                            Icon { name: IconName::Warn, size: 10 }
                            " Select at least one policy."
                        }
                    }

                    // Danger zone
                    div { style: "margin-top:10px;padding-top:14px;border-top:1px solid var(--cf-divider);",
                        div { style: "font-size:11px;font-weight:600;text-transform:uppercase;letter-spacing:0.08em;color:var(--cf-text-muted);margin-bottom:8px;",
                            "Danger zone"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            style: "color:#f87171;border-color:rgba(248,113,113,0.3);",
                            onclick: move |_| confirm_delete.set(true),
                            Icon { name: IconName::Trash, size: 12 }
                            " Delete bundle"
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
                disabled: !matches,
                onclick: move |_| { if matches { props.on_confirm.call(()); } },
                Icon { name: IconName::Trash, size: 13 }
                " Delete bundle"
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
