//! Presents configuration assurance separately from managed runtime authority.
//!
//! Exact server evidence owns revision, artifact, and scan identities. Stored
//! parent reports are independent reads and never establish guest health.

use super::*;
use crate::api::{client, models::*};
use crate::components::system::management_mode::*;
use chrono::{DateTime, Duration};

/// Renders all eight detail tabs for an external or reference registration.
/// Edit callbacks retain the owning view's existing transactional modal wiring.
#[component]
pub(super) fn ManagementDetail(
    system: SystemDetail,
    active_tab: Signal<Tab>,
    navigation_state: Signal<SystemDetailNavigation>,
    artifact_selection: Signal<SystemCveInventorySelection>,
    now: DateTime<Utc>,
    commits: Vec<SystemCommitHistory>,
    allow_config_observations: bool,
    can_mutate: bool,
    on_edit: EventHandler<()>,
    on_open_flake_commit: EventHandler<FlakeCommitPeekTarget>,
) -> Element {
    let nav = navigator();
    let system_id = system.id;
    // One read of the parent's name serves the Deploy tab, the control card and
    // the runtime card.
    let parent_hostname = use_parent_hostname(system.parent_host_id);
    // Approved Deploy-tab copy: it names the deploying party when it is known.
    let deploy_copy = match (system.management_mode, system.parent_host_id) {
        (ManagementMode::External, Some(_)) => format!(
            "{} is deployed by its parent host{}. Crystal Forge evaluates, builds and scans the selected revision and keeps the evidence. It cannot deploy or roll back this system.",
            system.hostname,
            parent_hostname.as_deref().map(|name| format!(" {name}")).unwrap_or_default()
        ),
        (ManagementMode::External, None) => format!(
            "{} is deployed by its own administrators. Crystal Forge evaluates, builds and scans the selected revision and keeps the evidence. It cannot deploy or roll back this system.",
            system.hostname
        ),
        _ => "This configuration is built and scanned on each revision to produce security evidence. There is no machine to deploy to.".to_string(),
    };
    let mut refresh = use_signal(|| 0_u64);
    let assurance = use_resource(move || {
        let _ = refresh();
        async move { client::fetch_configuration_assurance(&system_id).await }
    });
    let mut schedule = use_resource(client::fetch_scanning_schedule);
    let scans = use_resource(move || {
        let _ = refresh();
        async move { client::fetch_scanning_system_scans(&system_id, Some(500)).await }
    });
    let eligibility = use_resource(move || {
        let _ = refresh();
        async move { client::fetch_system_cve_scan_eligibility(&system_id).await }
    });
    let mut requesting = use_signal(|| false);
    let mut request_notice = use_signal(|| None::<String>);
    let mut requested_scan = use_signal(|| None::<Uuid>);
    let request_status = use_resource(move || {
        let _ = refresh();
        let scan = requested_scan();
        async move {
            match scan {
                Some(id) => Some(client::fetch_cve_scan_status(&id).await),
                None => None,
            }
        }
    });
    // Poll only a real server-issued request. No client timer can complete it.
    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        loop {
            crate::hooks::visibility::visible_tick(5_000).await;
            if requested_scan.peek().is_some()
                && !request_status.peek().as_ref().is_some_and(|result| {
                    matches!(result, Some(Ok(status)) if matches!(status.status.as_str(), "completed" | "failed" | "cancelled"))
                })
            {
                refresh.set(refresh().wrapping_add(1));
            }
        }
    });
    let evidence = assurance
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .filter(|e| e.system_id == system_id && e.management_mode == system.management_mode);
    // The design shows the critical count on the CVEs tab when CVE data exists. It
    // comes from the last good sealed scan only: failed, waiting or missing
    // evidence shows no number instead of a zero.
    let cve_tab_badge = evidence
        .as_ref()
        .and_then(crate::components::system::management_mode::last_good_scan)
        .and_then(|scan| scan.cves.as_ref().map(|cves| cves.critical))
        .filter(|critical| *critical > 0);
    let freshness = evidence.as_ref().map_or(EvidenceFreshness::Unknown, |e| {
        selected_freshness(
            e,
            schedule.read().as_ref().and_then(|r| r.as_ref().ok()),
            scans
                .read()
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .map(Vec::as_slice),
            now,
        )
    });
    let scan_eligibility = eligibility.read();
    let eligible = scan_eligibility.as_ref().and_then(|r| r.as_ref().ok());
    let allowed_derivation = eligible
        .filter(|r| r.eligible)
        .and_then(|r| r.derivation_id)
        .filter(|id| {
            evidence
                .as_ref()
                .and_then(|e| e.selected_artifact.as_ref())
                .is_some_and(|a| a.derivation_id == *id)
        });
    let blocked = match scan_eligibility.as_ref() {
        None => "Loading scan eligibility…".into(),
        Some(Err(error)) => format!("Scan eligibility unavailable: {error}"),
        Some(Ok(value)) => value.reason.clone().unwrap_or_else(|| {
            if allowed_derivation.is_some() {
                "Re-scan the selected configuration".into()
            } else {
                "Waiting for an eligible exact selected build".into()
            }
        }),
    };
    let request_scan = move |_| {
        let Some(derivation_id) = allowed_derivation else {
            return;
        };
        if requesting() || !can_mutate {
            return;
        }
        requesting.set(true);
        request_notice.set(Some("Submitting scan request…".into()));
        spawn(async move {
            match client::trigger_system_cve_scan_for_artifact(&system_id, derivation_id).await {
                Ok(response) if scan_response_matches_request(derivation_id, &response) => {
                    requested_scan.set(Some(response.scan_id));
                    request_notice.set(Some(format!(
                        "{} · scan {}",
                        response.message, response.scan_id
                    )));
                }
                Ok(_) => request_notice.set(Some(
                    "Scan response identity did not match the requested artifact.".into(),
                )),
                Err(error) => request_notice.set(Some(format!("Scan request failed: {error}"))),
            }
            requesting.set(false);
            refresh.set(refresh().wrapping_add(1));
        });
    };
    rsx! {
        div { class: "sd-root", "data-testid": "system-detail", "data-screen-label": "SystemDetail",
            div { class: "sd-head",
                div { class: "sd-crumb",
                    Link { class: "sd-back focus-ring", to: Route::SystemsView { query: String::new() }, aria_label: "Back to systems", Icon { name: IconName::ArrowLeft, size: 14 } }
                    span { class: "sd-crumb-text", "Systems / {system.hostname}" }
                }
                div { class: "sd-head-main",
                    div { class: "sd-title-block",
                        div { h1 { class: "sd-hostname", "{system.hostname}" }
                            div { class: "sd-fqdn mono", "{system.system_configuration_name.as_deref().unwrap_or(&system.hostname)}" }
                        }
                        span { class: "chip chip-unknown", {system.environment.as_deref().unwrap_or("Unknown environment")} }
                        MgmtChip { mode: system.management_mode }
                        DeploymentCell { mode: system.management_mode, status: system.deployment_status }
                        if let Some(e) = &evidence { ScanEvidenceChip { status: evidence_status(e, freshness) } }
                        else { span { class: "chip chip-unknown", "Evidence unavailable" } }
                    }
                    div { class: "sd-head-actions",
                        if can_mutate {
                            button { class: "btn btn-ghost focus-ring", disabled: requesting() || allowed_derivation.is_none(), title: "{blocked}", onclick: request_scan,
                                Icon { name: IconName::Shield, size: 14 }
                                if requesting() { "Requesting…" } else { "Re-scan" }
                            }
                            button { class: "btn btn-ghost focus-ring", onclick: move |_| on_edit.call(()), Icon { name: IconName::Gear, size: 14 } "Edit" }
                        }
                        button { class: "btn btn-ghost focus-ring", onclick: move |_| { refresh.set(refresh().wrapping_add(1)); schedule.restart(); }, "Reload evidence" }
                    }
                }
                if allowed_derivation.is_none() { p { class: "mg-dim", "{blocked}" } }
                if let Some(message) = request_notice() { p { role: "status", class: "mg-copy", "{message}" } }
                if let Some(Some(result)) = request_status.read().as_ref() {
                    match result {
                        Ok(status) => rsx! { p { role: "status", class: "mg-dim", "Scan {status.scan_id}: {status.status}" } },
                        Err(error) => rsx! { p { role: "alert", "Scan status unavailable: {error}" } },
                    }
                }
                if let Some(e) = &evidence {
                    EvidenceMetrics { evidence: e.clone(), freshness, mode: system.management_mode }
                }
                div { class: "sd-tabs", role: "tablist", aria_label: "System detail sections",
                    onkeydown: move |event| {
                        let current = DETAIL_TAB_ORDER.iter().position(|t| t == &active_tab()).unwrap_or_default();
                        let next = match event.key() {
                            Key::ArrowRight => (current + 1) % DETAIL_TAB_ORDER.len(),
                            Key::ArrowLeft => (current + DETAIL_TAB_ORDER.len() - 1) % DETAIL_TAB_ORDER.len(),
                            Key::Home => 0, Key::End => DETAIL_TAB_ORDER.len() - 1, _ => return,
                        };
                        event.prevent_default();
                        navigate_system_detail_tab(active_tab, navigation_state, DETAIL_TAB_ORDER[next], true);
                        focus_dialog_by_id(&format!("system-tab-{next}"));
                    },
                    for (index, tab) in DETAIL_TAB_ORDER.into_iter().enumerate() {
                        button { id: "system-tab-{index}", class: if active_tab() == tab { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            role: "tab", aria_selected: active_tab() == tab, aria_controls: "system-tab-panel",
                            tabindex: if active_tab() == tab { "0" } else { "-1" },
                            onclick: move |_| navigate_system_detail_tab(active_tab, navigation_state, tab, true),
                            Icon { name: match tab {
                                Tab::Overview => IconName::Dashboard, Tab::Deploy => IconName::Deploy,
                                Tab::History => IconName::History, Tab::Logs => IconName::Terminal,
                                Tab::Config => IconName::File, Tab::Cves | Tab::Compliance => IconName::Shield,
                                Tab::Hardening => IconName::Key,
                            }, size: 13 }
                            if tab == Tab::History { "Evidence history" } else { "{tab.label()}" }
                            if tab == Tab::Cves {
                                if let Some(critical) = cve_tab_badge {
                                    span { class: "sd-tab-badge", title: "{critical} critical in the last good scan", "{critical}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "sd-body", id: "system-tab-panel", role: "tabpanel",
                if active_tab() == Tab::Deploy {
                    div { class: "mg-empty mg-na", Icon { name: IconName::Deploy, size: 22 }
                        h3 { if system.management_mode == ManagementMode::External { "Deployment is controlled outside Crystal Forge" } else { "Reference configurations are not deployed" } }
                        p { "{deploy_copy}" }
                        div { class: "mg-notice-actions",
                            button { class: "btn btn-ghost focus-ring", onclick: move |_| navigate_system_detail_tab(active_tab, navigation_state, Tab::History, true), "Evidence history" }
                            if can_mutate { button { class: "btn btn-ghost focus-ring", onclick: move |_| on_edit.call(()), Icon { name: IconName::Gear, size: 12 } " Change management mode" } }
                        }
                    }
                } else if active_tab() == Tab::Logs {
                    div { class: "mg-empty mg-na", Icon { name: IconName::Terminal, size: 22 }
                        h3 { "No runtime logs" }
                        p { "No guest log stream is available. Evaluation, build and scan diagnostics describe configuration work only." }
                        if let Some(e) = &evidence {
                            if let Some(job) = e.selected_artifact.as_ref().and_then(|a| a.build.as_ref()) {
                                Link { class: "btn btn-ghost focus-ring", to: Route::BuildsView { query: format!("job={}", job.job_id) }, "Open build logs · {job.job_id}" }
                            } else { p { class: "mg-dim", "No selected build attempt is recorded. Retained build attempts are available in Evidence history." } }
                        }
                        Link { class: "btn btn-ghost focus-ring", to: Route::ScanningView {}, "Open Scanning" }
                    }
                } else if active_tab() == Tab::History {
                    if let Some(e) = evidence.clone() { EvidenceTimeline { evidence: e } }
                    else { p { class: "mg-dim", role: "status", "Configuration evidence history is loading or unavailable. Retained deployment history is read independently below." } }
                    RetainedDeploymentHistory { system_id,
                        on_view_logs: move |_| navigate_system_detail_tab(active_tab, navigation_state, Tab::Logs, true),
                    }
                } else if let Some(e) = evidence.clone() {
                    match active_tab() {
                        Tab::Overview => rsx! {
                            div { class: "mg-overview",
                                if e.evaluation.availability == "removed" {
                                    div { class: "mg-notice mg-notice-danger", div { class: "mg-notice-body",
                                        h3 { "nixosConfigurations.{e.configuration_name} is missing at the selected revision" }
                                        p { "The configuration was removed or renamed. Evidence from earlier revisions is retained." }
                                        Link { class: "btn btn-ghost xs focus-ring", to: Route::FlakesView { query: e.flake_id.map(|id| format!("flake={id}")).unwrap_or_default() }, "Open flake" }
                                    } }
                                }
                                section { class: "card sd-card",
                                    div { class: "sd-card-head", h2 { "Configuration evidence" } span { class: "sd-card-meta", "{e.configuration_name}" } }
                                    EvidencePipeline { evidence: e.clone(), freshness,
                                        on_open_stage: { let e = e.clone(); let system = system.clone(); move |stage| {
                                            match stage {
                                                0 => if let (Some(flake), Some(revision)) = (&system.flake, &e.selected_revision) {
                                                    on_open_flake_commit.call(FlakeCommitPeekTarget { flake: flake.clone(), sha: revision.sha.clone(), meta: CommitFocusMeta { msg: revision.message.clone(), author: None, at: Some(revision.committed_at.to_rfc3339()) } });
                                                },
                                                1 => if let Some(revision) = &e.selected_revision {
                                                    if let Some(attempt_id) = e.evaluation.attempt_id { nav.push(Route::EvaluationsAttemptView { commit_id: revision.commit_id, attempt_id }); }
                                                    else { nav.push(Route::EvaluationsCommitView { commit_id: revision.commit_id }); }
                                                },
                                                2 => if let Some(job) = e.selected_artifact.as_ref().and_then(|a| a.build.as_ref()) { nav.push(Route::BuildsView { query: format!("job={}", job.job_id) }); },
                                                3 => navigate_system_detail_tab(active_tab, navigation_state, Tab::Cves, true),
                                                _ => {},
                                            }
                                        } }
                                    }
                                    if let Some(revision) = &e.selected_revision { p { class: "mg-copy", {revision.message.as_deref().unwrap_or("Commit message unavailable")} } }
                                    EvidenceIdentities { evidence: e.clone() }
                                }
                                div { class: "mg-cols",
                                    ControlCard { system: system.clone(), evidence: e.clone(), parent_hostname: parent_hostname.clone() }
                                    if system.management_mode == ManagementMode::External {
                                        StoredRuntime { system_id, parent_host_id: system.parent_host_id, parent_label: parent_hostname.clone() }
                                    } else {
                                        section { class: "card sd-card", div { class: "sd-card-head", h2 { "Recent evidence" }
                                            button { class: "link-action focus-ring", onclick: move |_| navigate_system_detail_tab(active_tab, navigation_state, Tab::History, true), "All history" }
                                        } EvidenceTimeline { evidence: e.clone(), compact: true } }
                                    }
                                }
                            }
                        },
                        Tab::Config => rsx! {
                            ConfigTab {
                                system: system.clone(), commits: commits.clone(), generations: vec![],
                                current_commit: e.selected_revision.as_ref().map(|r| r.sha.clone()),
                                revision: configuration_revision(&navigation_state.read().config_revision, &e),
                                allow_config_observations, on_open_flake_commit,
                                on_revision_change: move |revision| {
                                    let mut next = navigation_state.read().clone(); next.config_revision = revision;
                                    next.tab = SystemDetailTab::Config;
                                    update_query(&next.to_query(&current_query()), true); navigation_state.set(next);
                                },
                            }
                        },
                        Tab::Cves => rsx! {
                            EvidenceGate { evidence: e.clone(), freshness,
                                ArtifactFindings { system: system.clone(), evidence: e.clone(), selection: artifact_selection }
                            }
                        },
                        Tab::Hardening => rsx! {
                            ConfigurationHardening { system_id, evidence: e.clone(), selection: artifact_selection }
                        },
                        Tab::Compliance => rsx! {
                            ConfigurationCompliance { system_id, evidence: e.clone() }
                            // Existing POA&M history remains inspectable without granting
                            // configuration evidence runtime-remediation authority.
                            ComplianceTab { system: system.clone(), viewer: true, initial_poam: String::new() }
                        },
                        _ => rsx! {},
                    }
                } else {
                    div { class: "mg-empty", role: "status",
                        if let Some(Err(error)) = assurance.read().as_ref() {
                            h3 { "Configuration evidence unavailable" } p { "{error}" }
                        } else if assurance.read().is_some() { h3 { "Configuration evidence identity mismatch" } }
                        else { h3 { "Loading configuration evidence…" } }
                        p { "Unavailable evidence does not mean zero findings. Retained evidence will appear when the read succeeds." }
                    }
                }
            }
        }
    }
}

// Read persisted events directly: commit-derived fallback rows would fabricate
// deployments for resources that have never had CF deployment authority. No
// generation is designated current and no rollback capability is supplied.
#[component]
fn RetainedDeploymentHistory(system_id: Uuid, on_view_logs: EventHandler<String>) -> Element {
    let mut history =
        use_resource(move || async move { client::fetch_system_history(&system_id).await });
    rsx! {
        section { class: "card sd-card", "data-testid": "retained-deployment-history",
            h2 { "Retained managed history" }
            p { class: "mg-copy", "Recorded deployments and runtime events from earlier managed operation remain read-only. These records do not establish current runtime state or deployment authority. A reference configuration is not a running machine." }
            match history.read().as_ref() {
                None => rsx! { p { role: "status", "Loading retained history…" } },
                Some(Err(error)) => rsx! { p { role: "alert", "Retained history unavailable: {error}" }
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| history.restart(), "Retry retained history" }
                },
                Some(Ok(entries)) if entries.is_empty() => rsx! { p { class: "mg-dim", "No retained managed events are recorded." } },
                Some(Ok(entries)) => rsx! {
                    HistoryTab { entries: entries.clone(), commits: vec![], generations: vec![], current_generation: None,
                        deployment_policy: "not applicable · retained records only".to_string(),
                        allow_mutations: false, on_rollback: move |_| {}, on_view_logs,
                    }
                    details { class: "mg-copy", summary { "Exact recorded event metadata" }
                        for (index, entry) in entries.iter().enumerate() {
                            dl { class: "kv-grid", key: "{history_event_id(entry, index)}", style: "overflow-wrap:anywhere",
                                dt { "Event" } dd { "{history_event_id(entry, index)} · {entry.event_type} · {entry.event_kind}" }
                                dt { "Recorded at" } dd { "{entry.timestamp.to_rfc3339()}" }
                                if let Some(at) = entry.occurred_at { dt { "Occurred at" } dd { "{at.to_rfc3339()}" } }
                                if let Some(at) = entry.observed_at { dt { "Observed at" } dd { "{at.to_rfc3339()}" } }
                                dt { "Outcome" } dd { "{entry.outcome}" }
                                if let Some(id) = entry.deployment_id { dt { "Deployment" } dd { "{id}" } }
                            }
                        }
                    }
                },
            }
        }
    }
}

fn configuration_revision(
    intent: &ConfigRevision,
    evidence: &ConfigurationAssurance,
) -> ConfigRevision {
    match intent {
        ConfigRevision::Commit(sha) => ConfigRevision::Commit(sha.clone()),
        _ => evidence
            .selected_revision
            .as_ref()
            .map(|r| ConfigRevision::Commit(r.sha.clone()))
            .unwrap_or_else(|| ConfigRevision::Commit(String::new())),
    }
}

/// Returns whether an accepted scan response answers the request for carrier
/// `requested`. With exact reuse (0322) the request names the selected
/// configuration carrier, while `derivation_id` names the physical execution
/// source, which may differ. Older servers omit the carrier field; their
/// `derivation_id` is then the requested artifact itself.
fn scan_response_matches_request(requested: i32, response: &CveScanTriggerResponse) -> bool {
    match response.configuration_derivation_id {
        Some(carrier) => carrier == requested && response.derivation_id.is_some(),
        None => response.derivation_id == Some(requested),
    }
}

// The scanning API supplies the lifecycle class; the browser must not invent
// a recent/archive age boundary from commit dates or registration mode.
fn selected_freshness(
    evidence: &ConfigurationAssurance,
    policy: Option<&ScanSchedulePolicyResponse>,
    rows: Option<&[ScanningQueueItemResponse]>,
    now: DateTime<Utc>,
) -> EvidenceFreshness {
    let Some(scan) = evidence.selected_scan.as_ref() else {
        return EvidenceFreshness::Unknown;
    };
    let class = rows
        .and_then(|rows| {
            rows.iter()
                .find(|r| r.derivation_id == scan.derivation_id && r.scan_id == Some(scan.scan_id))
        })
        .map(|r| r.freshness.as_str());
    let interval = policy
        .and_then(|p| match class {
            Some("deployed") => Some(p.deployed_interval.as_str()),
            Some("recent") => Some(p.recent_interval.as_str()),
            Some("archived") if p.archived_enabled => Some(p.archived_interval.as_str()),
            _ => None,
        })
        .and_then(policy_duration);
    evidence_freshness(scan.completed_at, now, interval)
}

#[component]
fn EvidenceMetrics(
    evidence: ConfigurationAssurance,
    freshness: EvidenceFreshness,
    mode: ManagementMode,
) -> Element {
    let counts = last_good_scan(&evidence).and_then(|s| s.cves.as_ref());
    let selected_identity = evidence
        .selected_revision
        .as_ref()
        .map(|r| r.sha.clone())
        .unwrap_or_default();
    let metrics = [
        (
            "Selected revision",
            evidence
                .selected_revision
                .as_ref()
                .map(|r| r.sha.get(..7).unwrap_or(&r.sha).to_string())
                .unwrap_or_else(|| "Unavailable".into()),
            evidence.branch.clone().unwrap_or_default(),
        ),
        (
            "Build",
            evidence
                .selected_artifact
                .as_ref()
                .map(|a| {
                    a.build
                        .as_ref()
                        .map(|b| b.state.clone())
                        .unwrap_or_else(|| a.state.clone())
                })
                .unwrap_or_else(|| "Not built".into()),
            "Exact selected configuration".into(),
        ),
        (
            "Scan",
            scan_evidence_status(&evidence, freshness).label().into(),
            "Configuration evidence only".into(),
        ),
        (
            "CVEs",
            counts
                .map(|c| c.total.to_string())
                .unwrap_or_else(|| "—".into()),
            counts
                .map(|c| format!("{} critical · {} high · last good scan", c.critical, c.high))
                .unwrap_or_else(|| "Scan evidence unavailable".into()),
        ),
        (
            "Runtime",
            if mode == ManagementMode::Reference {
                "Not applicable"
            } else {
                "Not verified"
            }
            .into(),
            if mode == ManagementMode::Reference {
                "Not a running machine"
            } else {
                "Parent reports are separate observations"
            }
            .into(),
        ),
    ];
    rsx! { div { class: "sd-metric-strip",
        for (label, value, sub) in metrics { div { class: "sd-metric",
            div { class: "sd-metric-label", "{label}" }
            div { class: "sd-metric-val-num mono", style: "font-size:18px;overflow-wrap:anywhere", title: if label == "Selected revision" { selected_identity.clone() } else { value.clone() }, "{value}" }
            div { class: "sd-metric-sub", "{sub}" }
        } }
    } }
}

#[component]
fn EvidenceIdentities(evidence: ConfigurationAssurance) -> Element {
    rsx! { dl { class: "kv-grid", style: "overflow-wrap:anywhere",
        dt { "Evaluation" } dd { "{evidence.evaluation.state} · {evidence.evaluation.availability}"
            if let Some(id) = evidence.evaluation.attempt_id { " · attempt {id}" }
        }
        if let Some(a) = evidence.selected_artifact { dt { "Artifact" } dd { "derivation {a.derivation_id} · {a.state}"
            if a.reuses_execution() {
                " · reuses the identical build of derivation {a.evidence_derivation_id()}"
                if let Some(source) = &a.execution_revision { " at {source.sha.get(..7).unwrap_or(&source.sha)}" }
            }
            if let Some(build) = a.build { " · job {build.job_id} · {build.state}" }
        } }
        if let Some(scan) = evidence.selected_scan {
            dt { "Selected scan" } dd { "{scan.scan_id} · derivation {scan.derivation_id} · {scan.state} · {scan.revision.sha}"
                for diagnostic in scan.diagnostics { p { "{diagnostic.occurred_at} · {diagnostic.level} · {diagnostic.message}" } }
                if scan.diagnostics_truncated { p { "Additional diagnostics omitted by the server." } }
            }
        }
        dt { "Freshness policy" } dd { "Scan age uses the fetched scan schedule and server lifecycle class. If either is unavailable or disabled, freshness is unknown." }
    } }
}

/// Returns the parent host's hostname for display.
///
/// The value is `None` while loading, when no parent is associated, or when
/// the viewer cannot read the parent registration. Callers then show the
/// parent UUID. The read reveals no data beyond what the parent's own detail
/// endpoint already authorizes.
fn use_parent_hostname(parent: Option<Uuid>) -> Option<String> {
    let resource = use_resource(use_reactive!(|parent| async move {
        let id = parent?;
        client::fetch_system(&id)
            .await
            .ok()
            .map(|system| system.hostname)
    }));
    resource.read().clone().flatten()
}

#[component]
fn ControlCard(
    system: SystemDetail,
    evidence: ConfigurationAssurance,
    parent_hostname: Option<String>,
) -> Element {
    let external = system.management_mode == ManagementMode::External;
    let control_copy = match (external, system.parent_host_id) {
        (true, Some(_)) => format!(
            "Deployed by parent host {}. Crystal Forge does not deploy, roll back or restart this system.",
            parent_hostname.as_deref().unwrap_or("(name unavailable)")
        ),
        (true, None) => "Deployed by its own administrators. Crystal Forge does not deploy or roll back this system.".to_string(),
        (false, _) => "A configuration tracked for security assurance. It is not a running machine. Closure evidence does not establish that a generated ISO or VM image was scanned.".to_string(),
    };
    rsx! { section { class: "card sd-card",
        div { class: "sd-card-head", h2 { if external { "Deployment control" } else { "Purpose" } } MgmtChip { mode: system.management_mode } }
        p { class: "mg-copy", "{control_copy}" }
        dl { class: "kv-grid", style: "overflow-wrap:anywhere",
            dt { "Flake" } dd { {system.flake.as_ref().map(|f| f.name.as_str()).unwrap_or("Unassigned")} }
            dt { "Configuration" } dd { "nixosConfigurations.{evidence.configuration_name}" }
            dt { "Branch" } dd { {evidence.branch.as_deref().unwrap_or("Unassigned")} }
            dt { "Environment" } dd { {system.environment.as_deref().unwrap_or("Unassigned")} }
            if let Some(id) = system.parent_host_id { dt { "Parent host" } dd { Link { to: Route::SystemDetailView { id: id.to_string(), tab: String::new(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() }, {parent_hostname.clone().unwrap_or_else(|| id.to_string())} } } }
            if let Some(path) = evidence.selected_artifact.as_ref().and_then(|a| a.output_path.as_ref()) { dt { "Built output" } dd { "{path}" } }
        }
    } }
}

/// Returns the observation only when it belongs to this guest and the guest's
/// current parent. A guest without a parent shows no observation.
fn observation_for_parent(
    report: Option<&GuestObservationRow>,
    system_id: Uuid,
    parent_host_id: Option<Uuid>,
) -> Option<GuestObservationRow> {
    report
        .filter(|r| r.guest_id == system_id && Some(r.parent_host_id) == parent_host_id)
        .cloned()
}

/// Computes the retained observation and the error text after a reload.
///
/// A matching reload replaces the observation. A reload for another guest or
/// parent clears it. A failed reload keeps the previous observation only while
/// it still belongs to the current parent, because the card must never pair an
/// old parent's report with the new parent's name.
fn reconcile_observation(
    previous: Option<&GuestObservationRow>,
    reloaded: Result<&Option<GuestObservationRow>, String>,
    system_id: Uuid,
    parent_host_id: Option<Uuid>,
) -> (Option<GuestObservationRow>, Option<String>) {
    match reloaded {
        Ok(report)
            if report.is_none()
                || observation_for_parent(report.as_ref(), system_id, parent_host_id).is_some() =>
        {
            (report.clone(), None)
        }
        Ok(_) => (
            None,
            Some("Observation identity or parent association did not match.".into()),
        ),
        Err(message) => (
            observation_for_parent(previous, system_id, parent_host_id),
            Some(message),
        ),
    }
}

#[component]
fn StoredRuntime(
    system_id: Uuid,
    parent_host_id: Option<Uuid>,
    parent_label: Option<String>,
) -> Element {
    let mut observation = use_signal(|| None::<GuestObservationRow>);
    let mut error = use_signal(|| None::<String>);
    let mut reload = use_signal(|| 0_u64);
    // A parent change makes the retained observation stale. Refetch once when
    // the association changes; the first render is fetched by the resource.
    let mut loaded_parent = use_signal(|| parent_host_id);
    use_effect(use_reactive((&parent_host_id,), move |(parent,)| {
        if *loaded_parent.peek() != parent {
            loaded_parent.set(parent);
            reload.with_mut(|count| *count = count.wrapping_add(1));
        }
    }));
    let result = use_resource(move || {
        let _ = reload();
        async move { client::fetch_latest_guest_observation(&system_id).await }
    });
    use_effect(move || {
        if let Some(result) = result.read().as_ref() {
            let (next, message) = reconcile_observation(
                observation.peek().as_ref(),
                result.as_ref().map_err(|e| e.to_string()),
                system_id,
                parent_host_id,
            );
            observation.set(next);
            error.set(message);
        }
    });
    // INVARIANT: the card names the current parent, so it renders only an
    // observation that this parent reported. The filter runs at render time
    // because a parent change can precede the reload.
    let shown = observation_for_parent(observation().as_ref(), system_id, parent_host_id);
    rsx! {
        div {
            RuntimeCard { observation: shown, freshness: EvidenceFreshness::Unknown,
                parent_label,
                loading: result.state() == UseResourceState::Pending, error: error(),
                on_reload: move |_| reload.set(reload().wrapping_add(1)),
            }
            p { class: "mg-dim", "Observation reporting freshness policy unavailable. Unknown or old reports do not mean the guest is offline." }
        }
    }
}

#[derive(Clone, PartialEq)]
struct TimelineRow {
    kind: &'static str,
    id: String,
    revision: String,
    derivation_id: Option<i32>,
    state: String,
    at: DateTime<Utc>,
    note: Option<String>,
    route: Option<Route>,
}

fn timeline_rows(evidence: &ConfigurationAssurance) -> Vec<TimelineRow> {
    let mut rows = Vec::new();
    for event in &evidence.history.evaluations {
        rows.push(TimelineRow {
            kind: "Eval",
            id: event
                .evaluation
                .attempt_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| format!("commit:{}", event.revision.commit_id)),
            revision: event.revision.sha.clone(),
            derivation_id: None,
            state: event.evaluation.state.clone(),
            at: event.occurred_at,
            note: event.evaluation.error.clone(),
            route: event
                .evaluation
                .attempt_id
                .map(|attempt_id| Route::EvaluationsAttemptView {
                    commit_id: event.revision.commit_id,
                    attempt_id,
                }),
        });
    }
    for artifact in &evidence.history.builds {
        if let Some(build) = &artifact.build {
            rows.push(TimelineRow {
                kind: "Build",
                id: build.job_id.to_string(),
                revision: artifact.revision.sha.clone(),
                derivation_id: Some(artifact.derivation_id),
                state: build.state.clone(),
                at: build
                    .completed_at
                    .or(build.started_at)
                    .unwrap_or(build.created_at),
                note: artifact.error.clone().or(build.failure_code.clone()),
                route: Some(Route::BuildsView {
                    query: format!("job={}", build.job_id),
                }),
            });
        }
    }
    for scan in &evidence.history.scans {
        rows.push(TimelineRow {
            kind: "Scan",
            id: scan.scan_id.to_string(),
            revision: scan.revision.sha.clone(),
            derivation_id: Some(scan.derivation_id),
            state: scan.state.clone(),
            at: scan.completed_at.unwrap_or(scan.created_at),
            note: scan.error.clone().or_else(|| {
                scan.cves.as_ref().map(|c| {
                    format!(
                        "{} critical · {} high · {} total",
                        c.critical, c.high, c.total
                    )
                })
            }),
            route: None,
        });
    }
    rows.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| b.id.cmp(&a.id))
    });
    rows
}

#[component]
fn EvidenceTimeline(evidence: ConfigurationAssurance, #[props(default)] compact: bool) -> Element {
    let mut kind = use_signal(|| "All");
    let rows = timeline_rows(&evidence)
        .into_iter()
        .filter(|r| kind() == "All" || kind() == r.kind)
        .take(if compact { 4 } else { usize::MAX })
        .collect::<Vec<_>>();
    rsx! { section { class: if compact { "" } else { "card sd-card" },
        if !compact {
            div { class: "sd-card-head", h2 { "Evidence history" }
                div { class: "seg", for value in ["All", "Eval", "Build", "Scan"] {
                    button { class: if kind() == value { "active focus-ring" } else { "focus-ring" }, aria_pressed: kind() == value, onclick: move |_| kind.set(value), "{value}" }
                } }
            }
            p { class: "mg-copy", "Evaluations, builds and scans of {evidence.configuration_name}. Retained prefix: at most {evidence.history.limit_per_kind} records per kind. This is configuration evidence, not a deployment timeline." }
        }
        if rows.is_empty() {
            p { class: "mg-dim",
                if kind() == "All" { "No evidence records in the returned history." }
                else { "No {kind().to_lowercase()} evidence records in the returned history." }
            }
        }
        div { class: "timeline",
            for row in rows { div { class: "tl-item", key: "{row.kind}-{row.id}",
                span { class: "tl-dot" }
                div { class: "tl-body", style: "overflow-wrap:anywhere",
                    div { class: "tl-title", "{row.kind} · {row.state} · {row.revision}" }
                    div { class: "tl-meta mono", "{row.id} · {row.at.to_rfc3339()}" }
                    if let Some(id) = row.derivation_id { div { class: "tl-meta mono", "derivation {id}" } }
                    if let Some(note) = row.note { p { class: "mg-dim", "{note}" } }
                    if let Some(route) = row.route { Link { class: "link-action focus-ring", to: route, "Open exact attempt" } }
                }
            } }
        }
    } }
}

// SECURITY: A derivation read can resolve a later completed scan. It may only
// populate this view when its source equals the sealed scan shown in the banner.
fn artifact_page_matches(
    system_id: Uuid,
    scan: &ScanEvidence,
    page: &SystemCveInventoryPageResponse,
) -> bool {
    let selection = SystemCveInventorySelection::ExactDerivation {
        derivation_id: scan.derivation_id,
    };
    cve_page_matches_request(system_id, selection, page)
        && page.read_only
        && page
            .source
            .as_ref()
            .is_some_and(|s| s.scan_id == scan.scan_id)
}

#[component]
fn ArtifactFindings(
    system: SystemDetail,
    evidence: ConfigurationAssurance,
    mut selection: Signal<SystemCveInventorySelection>,
) -> Element {
    let system_id = system.id;
    // Current is only an unset navigation intent on this surface. It never
    // becomes a Current API read. Unsupported generation intent stays unavailable.
    let selected = match selection() {
        SystemCveInventorySelection::ExactDerivation { derivation_id } => Some(derivation_id),
        SystemCveInventorySelection::Current => None,
        SystemCveInventorySelection::RetainedGeneration { .. } => Some(-1),
    };
    let choices = evidence
        .history
        .scans
        .iter()
        .chain(evidence.selected_scan.iter())
        .chain(evidence.latest_good_scan.iter())
        .filter(|s| {
            s.state == "completed"
                && s.completed_at.is_some()
                && s.evidence_schema_version == 1
                && s.cves.is_some()
        })
        .fold(Vec::<ScanEvidence>::new(), |mut rows, scan| {
            if !rows.iter().any(|s| s.derivation_id == scan.derivation_id) {
                rows.push(scan.clone());
            }
            rows
        });
    let scan = selected
        .and_then(|id| choices.iter().find(|s| s.derivation_id == id))
        .cloned()
        .or_else(|| {
            if selected.is_none() {
                last_good_scan(&evidence).cloned()
            } else {
                None
            }
        });
    let target = scan.as_ref().map(|s| (s.derivation_id, s.scan_id));
    let mut retry = use_signal(|| 0_u64);
    let inventory = use_resource(use_reactive(&target, move |target| {
        let _ = retry();
        async move {
            match target {
                Some((derivation_id, _)) => Some(
                    client::fetch_system_cve_inventory_for_target(
                        &system_id,
                        None,
                        SystemCveInventorySelection::ExactDerivation { derivation_id },
                    )
                    .await,
                ),
                None => None,
            }
        }
    }));
    let mut pagination = use_signal(CveInventoryPaginationState::default);
    use_effect(use_reactive(&target, move |_| {
        pagination.write().reset(None)
    }));
    let scan_for_effect = scan.clone();
    use_effect(move || {
        if let (Some(scan), Some(Some(Ok(page)))) = (&scan_for_effect, inventory.read().as_ref()) {
            if artifact_page_matches(system_id, scan, page) {
                pagination.write().reset(Some(page.clone()));
            }
        }
    });
    // URL-owned POA&M intent is handled by the parent detail host. This host
    // serves row clicks only, so one deep link cannot open competing drawers.
    let mut poam = use_signal(|| None::<Uuid>);
    let page = pagination.read().inventory.clone();
    let read_error = match inventory.read().as_ref() {
        Some(Some(Err(error))) => Some(error.to_string()),
        Some(Some(Ok(page))) if scan.as_ref().is_none_or(|scan| !artifact_page_matches(system_id, scan, page)) => Some("The returned inventory does not match the selected sealed scan. Reload evidence before inspecting a newer scan.".into()),
        _ => None,
    };
    rsx! {
        div { class: "rev-bar rev-bar-hist",
            span { class: "rev-bar-label", "Configuration artifact" }
            span { class: "mg-dim", "Commits only · read-only" }
            select { class: "cfgx-select focus-ring", aria_label: "Configuration artifact", value: scan.as_ref().map(|s| s.derivation_id.to_string()).unwrap_or_default(),
                onchange: move |event| {
                    if let Ok(id) = event.value().parse::<i32>() {
                        selection.set(SystemCveInventorySelection::ExactDerivation { derivation_id: id }); pagination.write().reset(None);
                        let query = query_with_parameter(&current_system_detail_query(), "cve_target", Some(&format!("derivation:{id}")));
                        let query = query_with_parameter(&query, "cve_mode", Some("commit"));
                        sync_system_detail_query(&query, true);
                    }
                },
                if scan.is_none() { option { value: "", "Selected artifact unavailable" } }
                for choice in choices { option { value: "{choice.derivation_id}", "{choice.revision.sha} · derivation {choice.derivation_id} · scan {choice.scan_id}" } }
            }
        }
        if let Some(scan) = &scan {
            p { class: "mg-copy", "Inspecting configuration {scan.configuration_name} at {scan.revision.sha} · sealed CVE scan {scan.scan_id}. This selection does not establish Current runtime authority or permit POA&M closure." }
                CvesTab {
                    system_id, hostname: system.hostname.clone(), inventory_target_key: format!("{}:{}", scan.derivation_id, scan.scan_id),
                    vulnerabilities: page.as_ref().map(|p| p.vulnerabilities.clone()).unwrap_or_default(), inventory_metadata: page.as_ref().map(|p| p.metadata.clone()).unwrap_or_default(),
                    inventory_authority: page.as_ref().map(|p| p.authority), inventory_source: page.as_ref().and_then(|p| p.source.clone()),
                    inventory_attempt: page.as_ref().and_then(|p| p.attempt.clone()), exact_authority_failure: page.as_ref().and_then(|p| p.exact_authority_failure),
                    read_only: true, allow_mutations: false, loading: inventory.read().is_none(), error: read_error,
                    has_more: page.as_ref().is_some_and(|p| p.has_more), continuation_loading: pagination.read().continuation_loading, continuation_error: pagination.read().continuation_error.clone(),
                    on_retry_read: move |_| { pagination.write().reset(None); retry.set(retry().wrapping_add(1)); },
                    on_load_more: { let scan = scan.clone(); move |_| {
                        let Some(request) = pagination.write().begin_continuation() else { return; };
                        let scan = scan.clone();
                        spawn(async move {
                            match client::fetch_system_cve_inventory_for_target(&system_id, Some(&request.cursor), SystemCveInventorySelection::ExactDerivation { derivation_id: scan.derivation_id }).await {
                                Ok(page) if artifact_page_matches(system_id, &scan, &page) => {
                                    if !pagination.write().complete_continuation(&request, page) { pagination.write().fail_continuation(&request, "Inventory source changed; reload evidence.".into()); }
                                }
                                Ok(_) => pagination.write().fail_continuation(&request, "Inventory source changed; reload evidence.".into()),
                                Err(error) => pagination.write().fail_continuation(&request, error.to_string()),
                            }
                        });
                    } },
                    on_saved: move |_| {}, on_open_poam: move |id| poam.set(Some(id)),
                }
        } else { div { class: "mg-empty", h3 { "Selected sealed artifact unavailable" } p { "No Current or latest-derivation fallback is used. Choose an available retained artifact." } } }
        PoamDetailHost { poam_id: poam(), viewer: true, on_close: move |_| poam.set(None), on_open_finding: move |_| {}, on_open_cve_finding: move |_| {} }
    }
}

// Hardening assessments are independent of CVE scans. Keep exact derivation
// identity even when no CVE scan has completed, and retain historical choices.
// Assessments belong to the physical execution source, so a carrier that
// reuses an exact artifact (0322) is one choice with its source.
fn hardening_choices(evidence: &ConfigurationAssurance) -> Vec<ArtifactEvidence> {
    evidence
        .selected_artifact
        .iter()
        .chain(evidence.history.builds.iter())
        .fold(Vec::new(), |mut choices, artifact| {
            if !choices.iter().any(|a: &ArtifactEvidence| {
                a.evidence_derivation_id() == artifact.evidence_derivation_id()
            }) {
                choices.push(artifact.clone());
            }
            choices
        })
}

#[component]
fn ConfigurationHardening(
    system_id: Uuid,
    evidence: ConfigurationAssurance,
    mut selection: Signal<SystemCveInventorySelection>,
) -> Element {
    let choices = hardening_choices(&evidence);
    let selected = match selection() {
        SystemCveInventorySelection::Current => choices.first(),
        SystemCveInventorySelection::ExactDerivation { derivation_id } => choices
            .iter()
            .find(|a| a.evidence_derivation_id() == derivation_id),
        SystemCveInventorySelection::RetainedGeneration { .. } => None,
    }
    .cloned();
    let target = selected
        .as_ref()
        .map(ArtifactEvidence::evidence_derivation_id);
    let mut retry = use_signal(|| 0_u64);
    let inventory = use_resource(use_reactive(&target, move |target| {
        let _ = retry();
        async move {
            match target {
                Some(derivation_id) => Some(
                    client::fetch_system_hardening_inventory_for_target(
                        &system_id,
                        SystemCveInventorySelection::ExactDerivation { derivation_id },
                    )
                    .await,
                ),
                None => None,
            }
        }
    }));
    rsx! {
        div { class: "mg-notice", div { class: "mg-notice-body",
            div { class: "mg-notice-title", "Configuration hardening evidence" }
            p { class: "mg-notice-text", "Assessments describe the exact configuration, not a running machine. Hardening evidence does not require a completed CVE scan." }
        } }
        div { class: "rev-bar rev-bar-hist",
            span { class: "rev-bar-label", "Configuration artifact" }
            span { class: "mg-dim", "Commits only · read-only" }
            select { class: "cfgx-select focus-ring", aria_label: "Configuration artifact", value: target.map(|id| id.to_string()).unwrap_or_default(),
                onchange: move |event| {
                    if let Ok(derivation_id) = event.value().parse() {
                        selection.set(SystemCveInventorySelection::ExactDerivation { derivation_id });
                        let query = query_with_parameter(&current_system_detail_query(), "cve_target", Some(&format!("derivation:{derivation_id}")));
                        let query = query_with_parameter(&query, "cve_mode", Some("commit"));
                        sync_system_detail_query(&query, true);
                    }
                },
                if target.is_none() { option { value: "", "Selected artifact unavailable" } }
                for artifact in choices { option { value: "{artifact.evidence_derivation_id()}", "{artifact.revision.sha} · derivation {artifact.evidence_derivation_id()}" } }
            }
        }
        if let Some(artifact) = selected {
            p { class: "mg-copy", "Inspecting configuration {artifact.configuration_name} at {artifact.revision.sha} · derivation {artifact.derivation_id}. This selection does not establish Current runtime authority or permit POA&M closure." }
            if artifact.reuses_execution() {
                p { class: "mg-dim", "Reuses the identical build of derivation {artifact.evidence_derivation_id()}"
                    if let Some(source) = &artifact.execution_revision { " at {source.sha.get(..7).unwrap_or(&source.sha)}" }
                    ". Assessments of that build apply to this exact configuration." }
            }
            match inventory.read().as_ref() {
                Some(Some(Ok(page))) if page.read_only && page.derivation_id == target && page.selection == (SystemCveInventorySelection::ExactDerivation { derivation_id: artifact.evidence_derivation_id() }) => rsx! {
                    if page.source.is_none() { div { class: "mg-empty", h3 { "Hardening evidence unavailable" } p { "No completed hardening assessment is available for this exact artifact." } } }
                    HardeningTab { system_id, inventory_target_key: format!("artifact:{}", artifact.evidence_derivation_id()), results: page.services.clone(), justifications: vec![], source: page.source.clone(), attempt: page.attempt.clone(), read_only: true, loading: false, error: None, allow_mutations: false, can_check_now: false, checking: false, check_now_disabled_reason: "Configuration artifact inspection is read-only", on_check_now: move |_| {}, on_saved: move |_| {} }
                },
                result => rsx! { div { class: "mg-empty", h3 { "Hardening evidence unavailable" }
                    match result {
                        None => rsx! { p { "Loading exact artifact hardening evidence…" } },
                        Some(Some(Err(error))) => rsx! { p { "{error}" } },
                        _ => rsx! { p { "The returned evidence does not match the selected read-only artifact." } },
                    }
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| retry.set(retry().wrapping_add(1)), "Retry evidence read" }
                } },
            }
        } else { div { class: "mg-empty", h3 { "Hardening evidence unavailable" } p { "No exact derivation is available. Evaluation and build diagnostics remain in the evidence history." } } }
    }
}

#[component]
fn ConfigurationCompliance(system_id: Uuid, evidence: ConfigurationAssurance) -> Element {
    use crate::api::models::eval_policy_matrix::{EvalMatrixCell, EvalMatrixProvenance};
    let mut selected_attempt = use_signal(|| None::<Uuid>);
    let mut retry = use_signal(|| 0_u64);
    let attempts = evidence
        .history
        .evaluations
        .iter()
        .filter_map(|event| {
            event
                .evaluation
                .attempt_id
                .map(|id| (id, event.revision.clone(), event.evaluation.state.clone()))
        })
        .collect::<Vec<_>>();
    let target = selected_attempt()
        .and_then(|id| {
            attempts
                .iter()
                .find(|(attempt, _, _)| *attempt == id)
                .cloned()
        })
        .or_else(|| {
            if selected_attempt().is_none() {
                evidence
                    .evaluation
                    .attempt_id
                    .zip(evidence.selected_revision.clone())
                    .map(|(id, revision)| (id, revision, evidence.evaluation.state.clone()))
            } else {
                None
            }
        });
    let identity = target
        .as_ref()
        .map(|(id, revision, _)| (*id, revision.commit_id, revision.sha.clone()));
    let result = use_resource(use_reactive(&identity, move |identity| {
        let _ = retry();
        async move {
            match identity {
                Some((attempt_id, commit_id, _)) => {
                    Some(client::fetch_eval_attempt_policy_matrix(commit_id, attempt_id).await)
                }
                None => None,
            }
        }
    }));
    let loaded = result.read();
    let matrix = loaded
        .as_ref()
        .and_then(|r| r.as_ref())
        .and_then(|r| r.as_ref().ok())
        .filter(|response| {
            identity.as_ref().is_some_and(|(attempt, commit, sha)| {
                response.commit_id == *commit
                    && response.matrix.as_ref().is_some_and(|matrix| {
                        matrix.evaluation_attempt_id == Some(*attempt)
                            && matrix.commit_sha.as_ref() == Some(sha)
                            && matrix.provenance == EvalMatrixProvenance::Captured
                            && !matrix.mock_evidence
                    })
            })
        })
        .and_then(|response| response.matrix.as_ref());
    let matching_rows = matrix
        .map(|m| {
            m.rows
                .iter()
                .filter(|row| {
                    row.system_id == Some(system_id)
                        && row.configuration_name == evidence.configuration_name
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let row = (matching_rows.len() == 1).then(|| matching_rows[0]);
    rsx! {
        section { class: "card sd-card",
            div { class: "sd-card-head", h2 { "Configuration compliance evidence" }
                select { class: "cfgx-select focus-ring", aria_label: "Compliance evaluation attempt",
                    value: target.as_ref().map(|(id, _, _)| id.to_string()).unwrap_or_default(),
                    onchange: move |event| selected_attempt.set(event.value().parse().ok()),
                    if let Some((id, revision, state)) = &target {
                        if !attempts.iter().any(|(attempt, _, _)| attempt == id) { option { value: "{id}", "{revision.sha} · {state} · {id}" } }
                    } else { option { value: "", "Selected evaluation unavailable" } }
                    for (id, revision, state) in attempts { option { value: "{id}", "{revision.sha} · {state} · {id}" } }
                }
            }
            p { class: "mg-copy", "Captured bundle versions and control outcomes describe this exact configuration evaluation attempt, not what a system is running. A CVE scan is not the authority for these outcomes. They do not attest runtime compliance or authorize remediation verification or POA&M closure." }
            if let Some((attempt, revision, _)) = &target { p { class: "mg-dim mono", "revision {revision.sha} · attempt {attempt}" } }
            if let (Some(matrix), Some(row), Some((attempt_id, revision, _))) = (matrix, row, &target) {
                if matrix.bundles.is_empty() { p { class: "mg-dim", "No captured bundle contexts are available. Missing capture is not proof that no bundles apply." } }
                for bundle in &matrix.bundles {
                    {
                        let controls = matrix.columns.iter().enumerate().filter(|(_, column)| row.policies.iter().any(|binding| binding.policy_version_id == column.policy_version_id && binding.memberships.iter().any(|membership| membership.bundle_version_id == bundle.bundle_version_id))).collect::<Vec<_>>();
                        let applicable = !controls.is_empty() || row.assigned_bundles.iter().any(|membership| membership.bundle_version_id == bundle.bundle_version_id);
                        let pass = controls.iter().filter(|(index, _)| row.cells.get(*index) == Some(&EvalMatrixCell::Pass)).count();
                        let fail = controls.iter().filter(|(index, _)| row.cells.get(*index) == Some(&EvalMatrixCell::Fail)).count();
                        let unknown = controls.len() - pass - fail;
                        rsx! { if applicable {
                            section { class: "card sd-card",
                                div { class: "sd-card-head", h3 { "{bundle.name}" }
                                    span { class: "chip chip-info", "{bundle.framework}" }
                                    span { class: "chip chip-unknown", "{bundle.version} · read-only" }
                                }
                                p { class: "mg-copy", "{pass} pass · {fail} fail · {unknown} other or unknown · {controls.len()} captured controls" }
                                if controls.is_empty() { p { class: "mg-dim", "No eligible captured controls in this bundle context. This does not establish compliance." } }
                                for (index, column) in controls {
                                    div { class: "mg-copy",
                                        span { "{column.name} · {column.version} · " }
                                        span { {format!("{:?}", row.cells.get(index).unwrap_or(&EvalMatrixCell::Unknown))} }
                                        Link { class: "link-action focus-ring", to: Route::ComplianceEvaluationView { commit_id: revision.commit_id, attempt_id: *attempt_id, bundle: bundle.bundle_id.to_string(), version: bundle.bundle_version_id.to_string(), system: system_id.to_string(), policy: column.policy_version_id.to_string() }, " View exact control evidence" }
                                    }
                                }
                            }
                        } }
                    }
                }
            } else {
                div { class: "mg-empty",
                    if loaded.is_none() && target.is_some() { h3 { "Loading exact evaluation compliance…" } }
                    else { h3 { "Configuration compliance evidence unavailable" } }
                    if let Some(Some(Err(error))) = loaded.as_ref() { p { "{error}" } }
                    else { p { "No unambiguous native captured system/configuration context is available for this attempt. No current bundle or runtime scores are substituted." } }
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| retry.set(retry().wrapping_add(1)), "Retry evidence read" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(guest: u128, parent: u128) -> GuestObservationRow {
        GuestObservationRow {
            id: 1,
            guest_id: Uuid::from_u128(guest),
            parent_host_id: Uuid::from_u128(parent),
            environment_id: Uuid::from_u128(9),
            guest_identity: "vm".into(),
            unit: "microvm@vm.service".into(),
            unit_active_state: Some("active".into()),
            runner_store_path: None,
            observed_at: Utc::now(),
            received_at: Utc::now(),
        }
    }

    #[test]
    fn observation_is_shown_only_for_the_current_parent() {
        let old = report(1, 10);
        let guest = Uuid::from_u128(1);
        // Same parent: shown.
        assert!(observation_for_parent(Some(&old), guest, Some(Uuid::from_u128(10))).is_some());
        // Parent reassigned: the old parent's report is hidden before any reload.
        assert!(observation_for_parent(Some(&old), guest, Some(Uuid::from_u128(11))).is_none());
        // Parent removed: nothing is shown.
        assert!(observation_for_parent(Some(&old), guest, None).is_none());
        // Another guest's report is never shown.
        assert!(
            observation_for_parent(Some(&report(2, 10)), guest, Some(Uuid::from_u128(10)))
                .is_none()
        );
    }

    #[test]
    fn reload_outcomes_never_keep_another_parents_observation() {
        let guest = Uuid::from_u128(1);
        let (old_parent, new_parent) = (Some(Uuid::from_u128(10)), Some(Uuid::from_u128(11)));
        let old = report(1, 10);
        let fresh = Some(report(1, 11));

        // A matching reload replaces the observation and clears the error.
        assert_eq!(
            reconcile_observation(Some(&old), Ok(&fresh), guest, new_parent),
            (fresh.clone(), None)
        );
        // No observation for the new parent yet: cleared, no error.
        assert_eq!(
            reconcile_observation(Some(&old), Ok(&None), guest, new_parent),
            (None, None)
        );
        // A reload that returns the old parent's report is rejected and cleared.
        let stale = Some(old.clone());
        let (kept, error) = reconcile_observation(None, Ok(&stale), guest, new_parent);
        assert!(kept.is_none() && error.is_some());
        // Failed reload after reassignment: the old parent's report is dropped.
        let (kept, error) =
            reconcile_observation(Some(&old), Err("offline".into()), guest, new_parent);
        assert!(kept.is_none());
        assert_eq!(error.as_deref(), Some("offline"));
        // Failed reload for an unchanged parent keeps the last report.
        let (kept, error) =
            reconcile_observation(Some(&old), Err("offline".into()), guest, old_parent);
        assert_eq!(kept, Some(old));
        assert_eq!(error.as_deref(), Some("offline"));
        // Failed reload after the parent was removed: nothing is kept.
        let (kept, _) = reconcile_observation(Some(&report(1, 10)), Err("x".into()), guest, None);
        assert!(kept.is_none());
    }

    #[test]
    fn reused_carrier_scan_response_matches_requested_carrier_not_source() {
        let response = |carrier: Option<i32>, source: Option<i32>| CveScanTriggerResponse {
            configuration_derivation_id: carrier,
            derivation_id: source,
            scan_id: Uuid::from_u128(508),
            status: "accepted".into(),
            message: "Queued".into(),
        };
        // Carrier B (20) reuses physical source A (10): accepted, not a mismatch.
        assert!(scan_response_matches_request(
            20,
            &response(Some(20), Some(10))
        ));
        // A response for a different carrier is rejected even if the source matches.
        assert!(!scan_response_matches_request(
            20,
            &response(Some(21), Some(20))
        ));
        assert!(!scan_response_matches_request(
            20,
            &response(Some(20), None)
        ));
        // Older servers report only the artifact they scanned.
        assert!(scan_response_matches_request(10, &response(None, Some(10))));
        assert!(!scan_response_matches_request(
            10,
            &response(None, Some(11))
        ));
    }

    #[test]
    fn retained_managed_events_keep_identity_and_outcomes_without_current_runtime() {
        let entries = [
            "cf_deployment_started",
            "cf_deployment_failed",
            "cf_deployment_succeeded",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            serde_json::from_value::<SystemHistoryEntry>(serde_json::json!({
                "id": Uuid::from_u128(index as u128 + 1),
                "deployment_id": Uuid::from_u128(508),
                "timestamp": "2026-10-31T10:00:00Z",
                "occurred_at": "2026-10-31T09:59:00Z",
                "store_path": null, "system_configuration_name": "baseline",
                "change_reason": kind, "event_type": kind, "event_kind": "cf_deployment",
                "commit_hash": null, "flake_name": null, "flake_repo_url": null,
                "actor": "operator", "outcome": (["started", "failed", "success"][index]),
                "generation": if index == 2 { Some(7) } else { None },
            }))
            .unwrap()
        })
        .collect::<Vec<_>>();
        let events = build_history_events(&entries, &[], None);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, HistoryEventKind::DeployStarted);
        assert_eq!(events[1].kind, HistoryEventKind::DeployFailed);
        assert_eq!(events[2].kind, HistoryEventKind::Deploy);
        assert_eq!(events[0].generation, None);
        assert_eq!(events[1].generation, None);
        assert_eq!(events[2].generation, Some(7));
        for (event, entry) in events.iter().zip(&entries) {
            assert_eq!(event.id, entry.id.unwrap().to_string());
            assert_eq!(event.deployment_id, entry.deployment_id);
            assert_eq!(event.timestamp, entry.timestamp);
        }
        assert!(build_history_events(&[], &[], None).is_empty());
    }

    #[test]
    fn schedule_disabled_unknown_and_overflow_do_not_invent_freshness() {
        assert_eq!(policy_duration("24h"), Some(Duration::hours(24)));
        assert_eq!(policy_duration("7 days"), Some(Duration::days(7)));
        for value in ["never", "", "0h", "-1h", "24months", "9223372036854775807w"] {
            assert_eq!(policy_duration(value), None, "{value}");
        }
    }

    fn evidence() -> ConfigurationAssurance {
        serde_json::from_value(serde_json::json!({
            "system_id": Uuid::from_u128(508), "management_mode": "reference", "configuration_name":"baseline", "flake_id":1, "branch":"main",
            "selected_revision":null, "evaluation":{"availability":"removed","state":"failed","attempt_id":null,"error":null,"completed_at":null},
            "selected_artifact":null,"selected_scan":null,"latest_terminal_scan":null,"latest_good_scan":null,
            "history":{"limit_per_kind":20,"evaluations":[],"builds":[],"scans":[]}
        })).unwrap()
    }

    fn scan() -> ScanEvidence {
        let at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        ScanEvidence {
            scan_id: Uuid::from_u128(509),
            derivation_id: 10,
            revision: RevisionEvidence {
                commit_id: 1,
                sha: "a".repeat(40),
                committed_at: at,
                message: None,
            },
            configuration_name: "baseline".into(),
            state: "completed".into(),
            created_at: at,
            completed_at: Some(at),
            evidence_schema_version: 1,
            cves: Some(EvidenceCveCounts {
                total: 0,
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            }),
            error: None,
            diagnostics: vec![],
            diagnostics_truncated: false,
        }
    }

    #[test]
    fn hardening_artifacts_do_not_require_cve_scans_and_keep_history() {
        let mut value = evidence();
        let artifact = ArtifactEvidence {
            derivation_id: 10,
            revision: scan().revision,
            configuration_name: "baseline".into(),
            state: "build-complete".into(),
            derivation_path: Some("/nix/store/baseline.drv".into()),
            output_path: Some("/nix/store/baseline".into()),
            expected_output_path: Some("/nix/store/baseline".into()),
            published_at: None,
            error: None,
            build: None,
            execution_derivation_id: None,
            execution_revision: None,
        };
        value.selected_artifact = Some(artifact.clone());
        value.history.builds.push(artifact.clone());
        let mut historical = artifact;
        historical.derivation_id = 11;
        value.history.builds.push(historical);
        assert!(value.selected_scan.is_none());
        assert!(value.latest_good_scan.is_none());
        assert_eq!(
            hardening_choices(&value)
                .iter()
                .map(|a| a.derivation_id)
                .collect::<Vec<_>>(),
            vec![10, 11]
        );
        value.selected_artifact = None;
        assert_eq!(
            hardening_choices(&value).len(),
            2,
            "removed configurations retain exact historical choices"
        );
        // Carrier B (12) reuses source A (10): one choice, fetched as A.
        let mut carrier = value.history.builds[0].clone();
        carrier.derivation_id = 12;
        carrier.execution_derivation_id = Some(10);
        assert!(carrier.reuses_execution());
        assert_eq!(carrier.evidence_derivation_id(), 10);
        value.selected_artifact = Some(carrier);
        let choices = hardening_choices(&value);
        assert_eq!(
            choices
                .iter()
                .map(ArtifactEvidence::evidence_derivation_id)
                .collect::<Vec<_>>(),
            vec![10, 11]
        );
        assert_eq!(
            choices[0].derivation_id, 12,
            "the selected carrier is the listed configuration"
        );
    }

    #[test]
    fn explicit_config_revision_survives_missing_selected_head() {
        let mut value = evidence();
        let intent = ConfigRevision::Commit("a".repeat(40));
        assert_eq!(configuration_revision(&intent, &value), intent);
        assert_eq!(
            configuration_revision(&ConfigRevision::Generation(7), &value),
            ConfigRevision::Commit(String::new())
        );
        assert!(timeline_rows(&value).is_empty());
        value.selected_revision = Some(RevisionEvidence {
            commit_id: 1,
            sha: "b".repeat(40),
            committed_at: Utc::now(),
            message: None,
        });
        assert_eq!(
            configuration_revision(&ConfigRevision::Current, &value),
            ConfigRevision::Commit("b".repeat(40))
        );
    }

    #[test]
    fn artifact_inventory_requires_exact_sealed_source_and_read_only_capability() {
        let scan = scan();
        let system_id = Uuid::from_u128(508);
        let mut page = SystemCveInventoryPageResponse {
            authority: SystemCveInventoryAuthority::Exact,
            exact_authority_failure: None,
            current_state: None,
            running_target: None,
            system_id: Some(system_id),
            source: Some(SystemCveInventorySource {
                scan_id: scan.scan_id,
                scanner_name: "vulnix".into(),
                scanner_version: None,
                completed_at: scan.completed_at.unwrap(),
            }),
            attempt: None,
            selection: SystemCveInventorySelection::ExactDerivation {
                derivation_id: scan.derivation_id,
            },
            evidence_representation: Some(SystemCveEvidenceRepresentation::Schema1Observations),
            read_only: true,
            vulnerabilities: vec![],
            metadata: Default::default(),
            inventory_revision: "sealed".into(),
            has_more: false,
            next_cursor: None,
        };
        assert!(artifact_page_matches(system_id, &scan, &page));
        page.source.as_mut().unwrap().scan_id = Uuid::from_u128(999);
        assert!(!artifact_page_matches(system_id, &scan, &page));
        page.source.as_mut().unwrap().scan_id = scan.scan_id;
        page.read_only = false;
        assert!(!artifact_page_matches(system_id, &scan, &page));
        page.read_only = true;
        page.selection = SystemCveInventorySelection::Current;
        assert!(!artifact_page_matches(system_id, &scan, &page));
    }

    #[test]
    fn freshness_requires_server_class_exact_scan_and_loaded_enabled_policy() {
        let scan = scan();
        let now = scan.completed_at.unwrap() + Duration::hours(25);
        let mut value = evidence();
        value.selected_scan = Some(scan.clone());
        let mut policy = ScanSchedulePolicyResponse {
            on_build: true,
            deployed_interval: "1h".into(),
            recent_interval: "48h".into(),
            archived_interval: "7d".into(),
            archived_enabled: false,
            rebuild_to_scan: false,
            updated_at: now,
        };
        let mut rows = vec![ScanningQueueItemResponse {
            management_modes: vec![ManagementMode::Reference],
            derivation_id: scan.derivation_id,
            rescan_eligible: true,
            scan_id: Some(scan.scan_id),
            hostname: "baseline".into(),
            flake_name: None,
            commit_hash: Some(scan.revision.sha.clone()),
            status: "completed".into(),
            completed_at: scan.completed_at,
            scheduled_at: None,
            critical_count: 0,
            high_count: 0,
            medium_count: 0,
            freshness: "recent".into(),
            is_current: false,
            is_latest_per_flake: true,
            source_trigger: None,
        }];
        assert_eq!(
            selected_freshness(&value, Some(&policy), Some(&rows), now),
            EvidenceFreshness::Fresh
        );
        policy.recent_interval = "24h".into();
        assert_eq!(
            selected_freshness(&value, Some(&policy), Some(&rows), now),
            EvidenceFreshness::Stale
        );
        assert_eq!(
            selected_freshness(&value, None, Some(&rows), now),
            EvidenceFreshness::Unknown
        );
        rows[0].freshness = "archived".into();
        assert_eq!(
            selected_freshness(&value, Some(&policy), Some(&rows), now),
            EvidenceFreshness::Unknown
        );
        rows[0].freshness = "recent".into();
        rows[0].scan_id = Some(Uuid::from_u128(999));
        assert_eq!(
            selected_freshness(&value, Some(&policy), Some(&rows), now),
            EvidenceFreshness::Unknown
        );
    }

    #[test]
    fn removed_configuration_keeps_failed_and_sealed_history_with_exact_ids() {
        let mut value = evidence();
        let good = scan();
        let mut failed = good.clone();
        failed.scan_id = Uuid::from_u128(510);
        failed.completed_at = Some(good.completed_at.unwrap() + Duration::hours(1));
        failed.state = "failed".into();
        failed.cves = None;
        failed.error = Some("Scan execution failed".into());
        value.history.scans = vec![good.clone(), failed];
        let rows = timeline_rows(&value);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, Uuid::from_u128(510).to_string());
        assert_eq!(rows[0].state, "failed");
        assert_eq!(rows[1].id, good.scan_id.to_string());
        assert_eq!(rows[1].revision, good.revision.sha);
        assert_eq!(
            rows[1].note.as_deref(),
            Some("0 critical · 0 high · 0 total")
        );
    }
}
