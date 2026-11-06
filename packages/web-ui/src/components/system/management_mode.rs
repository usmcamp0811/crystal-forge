//! Approved management presentation for configuration evidence and parent reports.
//!
//! These components do not fetch, mutate, or infer runtime authority. Views own
//! loading/errors, policy selection, navigation, and latest-report reloads.

use chrono::{DateTime, Duration, Utc};
use dioxus::prelude::*;

use crate::api::models::{
    ConfigurationAssurance, DeploymentStatus, GuestObservationRow, ManagementMode, ScanEvidence,
};
use crate::components::icon::{Icon, IconName};

/// Classifies evidence age under the caller's actual applicable policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceFreshness {
    /// The collection timestamp is inside the applicable freshness interval.
    Fresh,
    /// The collection timestamp is outside the applicable freshness interval.
    Stale,
    /// No applicable policy/timestamp is known, or the timestamp is in the future.
    Unknown,
}

/// Compares collection time with a real policy interval, without invented ages.
/// Pass the scan's applicable rescan interval or the observation's reporting
/// policy. A missing or nonpositive interval cannot establish freshness.
pub fn evidence_freshness(
    collected_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    max_age: Option<Duration>,
) -> EvidenceFreshness {
    match (collected_at, max_age) {
        (Some(at), Some(age)) if age > Duration::zero() && at <= now => {
            if now - at < age {
                EvidenceFreshness::Fresh
            } else {
                EvidenceFreshness::Stale
            }
        }
        _ => EvidenceFreshness::Unknown,
    }
}

/// Parses a scan-schedule interval such as `24h`, `7d` or `30 minutes`.
///
/// Returns `None` for zero, negative, malformed, or overflowing values. The
/// browser never substitutes a default interval for an unparseable policy.
pub fn policy_duration(value: &str) -> Option<Duration> {
    let value = value.trim();
    let split = value.find(|c: char| !c.is_ascii_digit())?;
    let number = value[..split].parse::<i64>().ok().filter(|n| *n > 0)?;
    let seconds = number.checked_mul(match value[split..].trim() {
        "s" | "second" | "seconds" => 1,
        "m" | "minute" | "minutes" => 60,
        "h" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86400,
        "w" | "week" | "weeks" => 604800,
        _ => return None,
    })?;
    Duration::try_seconds(seconds)
}

/// Classifies the selected external/reference scan with the server cadence.
///
/// The server schedules the exact selected branch-head artifact of an
/// external or reference registration on `deployed_interval` (a cadence label,
/// not a deployment claim). `selected_scan` is always that artifact's newest
/// lifecycle, so this matches scheduler and Scanning statistics without a
/// per-system scan read. Managed registrations, a missing schedule, or a
/// missing completion time return [`EvidenceFreshness::Unknown`].
pub fn selected_head_scan_freshness(
    evidence: &ConfigurationAssurance,
    policy: Option<&crate::api::models::ScanSchedulePolicyResponse>,
    now: DateTime<Utc>,
) -> EvidenceFreshness {
    if evidence.management_mode == ManagementMode::Managed {
        return EvidenceFreshness::Unknown;
    }
    let interval = policy.and_then(|p| policy_duration(&p.deployed_interval));
    evidence_freshness(
        evidence.selected_scan.as_ref().and_then(|s| s.completed_at),
        now,
        interval,
    )
}

/// Distinguishes configuration lifecycle problems from scan freshness.
/// These states never describe Current runtime exposure or deployment readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStatus {
    /// Native discovery proves that the selected configuration was removed.
    ConfigurationMissing,
    /// Native evaluation or its owning attempt failed.
    EvaluationFailed,
    /// Discovery or a native configuration outcome is not available.
    EvaluationUnavailable,
    /// A real evaluation attempt is queued or running.
    Evaluating,
    /// An exact output is not built yet.
    AwaitingBuild,
    /// A real build job is running.
    Building,
    /// The selected artifact or latest build attempt failed.
    BuildFailed,
    /// No selected scan has completed.
    Missing,
    /// The server reports unavailable or unsupported scan evidence.
    Unavailable,
    /// The selected scan lifecycle failed; last-good data can still be shown.
    Failed,
    /// A real scan lifecycle is queued or running.
    Scanning,
    /// The scan waits for its exact closure; this is not permanent unavailability.
    AwaitingClosure,
    /// Sealed evidence matches the selection and is fresh under actual policy.
    Current,
    /// Sealed evidence differs from the selection or exceeds policy age.
    Stale,
    /// Sealed evidence matches, but its age cannot be classified under policy.
    FreshnessUnknown,
}

impl EvidenceStatus {
    /// Returns the approved compact label, keeping missing distinct from clean.
    pub fn label(self) -> &'static str {
        match self {
            Self::ConfigurationMissing => "Config missing",
            Self::EvaluationFailed => "Eval failed",
            Self::EvaluationUnavailable => "Evaluation unavailable",
            Self::Evaluating => "Evaluating",
            Self::AwaitingBuild => "Awaiting build",
            Self::Building => "Building",
            Self::BuildFailed => "Build failed",
            Self::Missing => "No scan",
            Self::Unavailable => "Scan unavailable",
            Self::Failed => "Scan failed",
            Self::Scanning => "Scanning",
            Self::AwaitingClosure => "Awaiting closure",
            Self::Current => "Scan current",
            Self::Stale => "Scan stale",
            Self::FreshnessUnknown => "Scan freshness unknown",
        }
    }

    /// Returns the existing design-system chip class for this evidence state.
    pub fn chip_class(self) -> &'static str {
        match self {
            Self::Current => "chip-healthy",
            Self::Stale => "chip-warning",
            Self::ConfigurationMissing
            | Self::EvaluationFailed
            | Self::BuildFailed
            | Self::Unavailable
            | Self::Failed => "chip-critical",
            Self::Evaluating
            | Self::AwaitingBuild
            | Self::Building
            | Self::Scanning
            | Self::AwaitingClosure => "chip-info",
            _ => "chip-unknown",
        }
    }

    fn color(self) -> &'static str {
        match self.chip_class() {
            "chip-healthy" => "var(--cf-emerald)",
            "chip-warning" => "var(--cf-amber)",
            "chip-critical" => "var(--cf-red)",
            "chip-info" => "var(--cf-blue)",
            _ => "var(--cf-gray-dot)",
        }
    }
}

fn sealed(scan: &ScanEvidence) -> bool {
    scan.state == "completed"
        && scan.completed_at.is_some()
        && scan.evidence_schema_version == 1
        && scan.cves.is_some()
}

/// Returns sealed selected evidence, otherwise the retained last-good scan.
/// The fallback remains historical and does not become selected or Current.
pub fn last_good_scan(evidence: &ConfigurationAssurance) -> Option<&ScanEvidence> {
    // Prefer exact selected evidence when a newer historical rescan also exists.
    // Its counts must not be paired with the selected scan's status or identity.
    evidence
        .selected_scan
        .as_ref()
        .filter(|scan| sealed(scan))
        .or_else(|| {
            evidence
                .latest_good_scan
                .as_ref()
                .filter(|scan| sealed(scan))
        })
}

/// Derives issue precedence from real selected evidence, retaining historical data.
/// `freshness` must classify the selected scan, not a historical last-good scan.
/// A successful attempt alone does not prove a native configuration outcome.
pub fn evidence_status(
    evidence: &ConfigurationAssurance,
    freshness: EvidenceFreshness,
) -> EvidenceStatus {
    use EvidenceStatus::*;
    let eval = &evidence.evaluation;
    if eval.availability == "removed" {
        return ConfigurationMissing;
    }
    if matches!(eval.state.as_str(), "eval_failed" | "failed") {
        return EvaluationFailed;
    }
    if matches!(
        eval.state.as_str(),
        "pending" | "queued" | "in_progress" | "running"
    ) {
        return Evaluating;
    }
    if eval.availability != "available" || eval.state != "evaluated" {
        return EvaluationUnavailable;
    }
    let Some(artifact) = &evidence.selected_artifact else {
        return AwaitingBuild;
    };
    let build_state = artifact.build.as_ref().map(|b| b.state.as_str());
    if matches!(artifact.state.as_str(), "build-failed" | "failed")
        || matches!(build_state, Some("failed"))
    {
        return BuildFailed;
    }
    if matches!(build_state, Some("building" | "running")) || artifact.state == "build-in-progress"
    {
        return Building;
    }
    if artifact.output_path.is_none() && evidence.selected_scan.is_none() {
        return AwaitingBuild;
    }
    scan_evidence_status(evidence, freshness)
}

/// Classifies the scan stage independently of evaluation and build problems.
/// Historical last-good evidence is stale for a missing selected scan. Only a
/// sealed exact selected scan inside its real policy window can be current.
pub fn scan_evidence_status(
    evidence: &ConfigurationAssurance,
    freshness: EvidenceFreshness,
) -> EvidenceStatus {
    use EvidenceStatus::*;
    let Some(scan) = &evidence.selected_scan else {
        return if last_good_scan(evidence).is_some() {
            Stale
        } else {
            Missing
        };
    };
    match scan.state.as_str() {
        "failed" => Failed,
        "unavailable" | "unsupported" => Unavailable,
        "awaiting_build" => AwaitingBuild,
        "awaiting_closure" => AwaitingClosure,
        "pending" | "queued" | "in_progress" | "running" => Scanning,
        "completed" if sealed(scan) => {
            let matches_selection = evidence.selected_revision.as_ref().is_some_and(|revision| {
                revision.commit_id == scan.revision.commit_id && revision.sha == scan.revision.sha
            }) && evidence
                .selected_artifact
                .as_ref()
                .is_some_and(|artifact| scan.derivation_id == artifact.derivation_id)
                && scan.configuration_name == evidence.configuration_name;
            if !matches_selection || freshness == EvidenceFreshness::Stale {
                Stale
            } else if freshness == EvidenceFreshness::Fresh {
                Current
            } else {
                FreshnessUnknown
            }
        }
        "completed" => Unavailable,
        _ => Unavailable,
    }
}

/// Returns the approved icon for a management mode (`MGMT_META[mode].icon`).
pub fn mode_icon(mode: ManagementMode) -> IconName {
    match mode {
        ManagementMode::Managed => IconName::Server,
        ManagementMode::External => IconName::Link,
        ManagementMode::Reference => IconName::File,
    }
}

/// Renders the approved compact mode label and explanatory tooltip.
#[component]
pub fn MgmtChip(mode: ManagementMode) -> Element {
    let icon = mode_icon(mode);
    rsx! { span { class: "chip mg-chip mg-{mode.as_str()}", title: mode.description(),
        Icon { name: icon, size: 10 } "{mode.label()}"
    } }
}

/// Renders deployment status only for managed resources; other modes have no CF deployment.
#[component]
pub fn DeploymentCell(mode: ManagementMode, status: DeploymentStatus) -> Element {
    if mode == ManagementMode::Managed {
        rsx! { span { class: "chip {status.color_class()} {status.bg_class()}", "{status.label()}" } }
    } else {
        let label = if mode == ManagementMode::External {
            "deployed externally"
        } else {
            "not deployed"
        };
        rsx! { span { class: "chip chip-unknown", "{label}" } }
    }
}

/// Renders a configuration evidence status without runtime or clean-scan claims.
#[component]
pub fn ScanEvidenceChip(status: EvidenceStatus) -> Element {
    rsx! { span { class: "chip {status.chip_class()}",
        span { class: "chip-dot", style: "background: {status.color()}" }
        "{status.label()}"
    } }
}

/// Renders the evidence scope and exact selected/scanned identities above findings.
/// Views can use this for read-only CVE, hardening, and compliance sections.
#[component]
pub fn EvidenceBanner(evidence: ConfigurationAssurance, freshness: EvidenceFreshness) -> Element {
    let status = evidence_status(&evidence, freshness);
    let tone = match status {
        EvidenceStatus::ConfigurationMissing => "danger",
        EvidenceStatus::Current => "info",
        _ => "warn",
    };
    let selected = evidence
        .selected_revision
        .as_ref()
        .map(|r| r.sha.as_str())
        .unwrap_or("unavailable");
    let good = last_good_scan(&evidence);
    let scanned = good.map(|s| s.revision.sha.as_str()).unwrap_or("none");
    let at = good
        .and_then(|s| s.completed_at)
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| "unavailable".into());
    let scope = if evidence.management_mode == ManagementMode::Reference {
        "Evidence for a reference configuration. No machine is associated with it."
    } else {
        "This is configuration evidence, not evidence of what the system is running. Runtime state is not verified."
    };
    rsx! { div { class: "mg-notice mg-notice-{tone}",
        Icon { name: IconName::Info, size: 16 }
        div { class: "mg-notice-body",
            div { class: "mg-notice-title", "{status.label()} · {evidence.configuration_name}" }
            div { class: "mg-notice-text", "{scope}" }
            div { class: "mg-notice-text", "Selected revision: {selected} · Last good scan: {scanned} · Scanned: {at}" }
            if let Some(error) = evidence.selected_scan.as_ref().and_then(|s| s.error.as_ref())
                .or(evidence.evaluation.error.as_ref())
                .or_else(|| evidence.selected_artifact.as_ref().and_then(|a| a.error.as_ref())) {
                div { class: "mg-notice-text", "{error}" }
            }
        }
    } }
}

/// Gates configuration findings on sealed last-good evidence, never on zero counts.
/// Children must remain read-only configuration evidence; this grants no Current
/// mutation, POA&M closure, or runtime-remediation authority. Request actions belong to views.
#[component]
pub fn EvidenceGate(
    evidence: ConfigurationAssurance,
    freshness: EvidenceFreshness,
    children: Element,
) -> Element {
    let status = evidence_status(&evidence, freshness);
    let has_evidence = last_good_scan(&evidence).is_some();
    // Approved empty states: waiting for a build, scanning unavailable, and no scan
    // yet each have their own title and explanation. Other statuses keep their
    // label; the banner above carries their diagnostics.
    let revision = evidence
        .selected_revision
        .as_ref()
        .map(|r| r.sha.chars().take(7).collect::<String>())
        .unwrap_or_else(|| "the selected revision".into());
    let (title, body) = match status {
        EvidenceStatus::AwaitingBuild | EvidenceStatus::Building => (
            "Waiting for a build".to_string(),
            format!("Findings appear after the build of revision {revision} completes and is scanned."),
        ),
        EvidenceStatus::Unavailable => (
            "Scanning isn't available for this configuration".to_string(),
            evidence
                .selected_scan
                .as_ref()
                .and_then(|scan| scan.error.clone())
                .unwrap_or_else(|| "No scanner can run for this configuration.".into()),
        ),
        EvidenceStatus::Missing => (
            "No scan evidence yet".to_string(),
            format!(
                "No completed scan exists for {} at {revision}. Use Re-scan to request one.",
                evidence.configuration_name
            ),
        ),
        _ => (
            status.label().to_string(),
            "No sealed configuration scan is available. Findings appear after an exact build is scanned successfully.".to_string(),
        ),
    };
    rsx! {
        EvidenceBanner { evidence, freshness }
        if has_evidence { {children} } else {
            div { class: "mg-empty",
                Icon { name: IconName::Shield, size: 22 }
                h3 { "{title}" }
                p { "{body}" }
            }
        }
    }
}

// Build-job states and derivation states use different wire vocabularies.
// Reused artifacts can have a completed derivation without a build-job row.
fn build_stage_presentation(state: &str) -> (&str, &'static str) {
    match state {
        "success" | "complete" | "build-complete" => ("Built", "var(--cf-emerald)"),
        "failed" | "build-failed" => ("Failed", "var(--cf-red)"),
        "queued" | "build-pending" => ("Queued", "var(--cf-blue)"),
        "building" | "build-in-progress" => ("Building", "var(--cf-blue)"),
        other => (other, "var(--cf-gray-dot)"),
    }
}

/// Renders revision, evaluation, build, and scan with navigation owned by the view.
/// Callback indices are 0=selected revision, 1=evaluation, 2=build, 3=scan.
#[component]
pub fn EvidencePipeline(
    evidence: ConfigurationAssurance,
    freshness: EvidenceFreshness,
    #[props(default)] on_open_stage: Option<EventHandler<usize>>,
) -> Element {
    let status = scan_evidence_status(&evidence, freshness);
    let selected_identity = evidence
        .selected_revision
        .as_ref()
        .map(|r| r.sha.clone())
        .unwrap_or_default();
    let selected = evidence
        .selected_revision
        .as_ref()
        .map(|r| r.sha.get(..7).unwrap_or(&r.sha).to_string())
        .unwrap_or_else(|| "Unavailable".into());
    let build = evidence
        .selected_artifact
        .as_ref()
        .map(|a| {
            a.build
                .as_ref()
                .map(|b| b.state.clone())
                .unwrap_or_else(|| a.state.clone())
        })
        .unwrap_or_else(|| "Not built".into());
    let scanned = evidence
        .selected_scan
        .as_ref()
        .map(|s| {
            format!(
                "{} · {}",
                s.revision.sha.get(..7).unwrap_or(&s.revision.sha),
                s.completed_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "No completion time".into())
            )
        })
        .unwrap_or_else(|| "No selected scan".into());
    let eval_color = match evidence.evaluation.state.as_str() {
        "evaluated" if evidence.evaluation.availability == "available" => "var(--cf-emerald)",
        "eval_failed" | "failed" => "var(--cf-red)",
        "pending" | "queued" | "in_progress" | "running" => "var(--cf-blue)",
        _ => "var(--cf-gray-dot)",
    };
    let (build_label, build_color) = build_stage_presentation(&build);
    let revision_color = if evidence.evaluation.availability == "removed" {
        "var(--cf-red)"
    } else if evidence.selected_revision.is_some() {
        "var(--cf-emerald)"
    } else {
        "var(--cf-gray-dot)"
    };
    let evaluation_label = match evidence.evaluation.state.as_str() {
        "evaluated" => "Evaluated",
        "eval_failed" | "failed" => "Failed",
        "pending" | "queued" => "Queued",
        "in_progress" | "running" => "Evaluating",
        "unavailable" => "Unavailable",
        other => other,
    };
    let branch = evidence.branch.as_deref().unwrap_or("No branch");
    let revision_meta = evidence
        .selected_revision
        .as_ref()
        .map(|r| format!("{branch} · {}", r.committed_at.to_rfc3339()))
        .unwrap_or_else(|| branch.into());
    let rows = [
        ("Selected revision", selected, revision_meta, revision_color),
        (
            "Evaluation",
            evaluation_label.into(),
            evidence
                .evaluation
                .completed_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| evidence.evaluation.availability.clone()),
            eval_color,
        ),
        (
            "Build",
            build_label.into(),
            evidence
                .selected_artifact
                .as_ref()
                .and_then(|a| a.build.as_ref())
                .and_then(|b| b.completed_at.or(b.started_at))
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
            build_color,
        ),
        ("Scan", status.label().into(), scanned, status.color()),
    ];
    rsx! { div { class: "mg-pipeline",
        for (index, (label, main, sub, color)) in rows.into_iter().enumerate() {
            button { class: "mg-stage focus-ring", r#type: "button", disabled: on_open_stage.is_none(),
                title: if index == 0 { selected_identity.clone() } else { sub.clone() },
                onclick: move |_| { if let Some(handler) = on_open_stage { handler.call(index); } },
                span { class: "mg-stage-label", "{label}" }
                span { class: "mg-stage-main", span { class: "chip-dot", style: "background: {color}" } "{main}" }
                span { class: "mg-stage-sub", "{sub}" }
                if index == 0 && evidence.evaluation.availability == "removed" {
                    span { class: "mg-stage-warn", "config not found" }
                }
                if index == 1 {
                    if let Some(error) = &evidence.evaluation.error {
                        span { class: "mg-stage-warn", "{error}" }
                    }
                }
                if index == 2 {
                    if let Some(error) = evidence.selected_artifact.as_ref().and_then(|a| a.error.as_ref()) {
                        span { class: "mg-stage-warn", "{error}" }
                    }
                }
                if index == 3 && status == EvidenceStatus::Stale {
                    span { class: "mg-stage-warn", "Scan revision differs from selection or evidence is past its freshness window." }
                }
            }
        }
    } }
}

/// Renders sealed, read-only last-good CVE counts with immutable scan identity.
/// Missing or unsealed counts render an evidence status, never zero
/// vulnerabilities.
#[component]
pub fn EvidenceCves(evidence: ConfigurationAssurance, freshness: EvidenceFreshness) -> Element {
    let status = evidence_status(&evidence, freshness);
    let Some(scan) = last_good_scan(&evidence) else {
        return rsx! { span { class: "chip chip-unknown", "{status.label()} · no sealed findings" } };
    };
    let Some(counts) = scan.cves.as_ref() else {
        return rsx! {};
    };
    rsx! { div { class: "mg-evidence-cves", title: "Configuration evidence only; not Current runtime exposure",
        span { class: "chip chip-critical", "{counts.critical} critical" }
        span { class: "chip chip-warning", "{counts.high} high" }
        span { class: "chip chip-unknown", "{counts.medium} medium · {counts.low} low · {counts.total} total" }
        if status != EvidenceStatus::Current { span { class: "chip chip-unknown", "last good · historical" } }
        span { class: "mg-dim", "revision {scan.revision.sha} · scan {scan.scan_id}" }
    } }
}

/// Renders sealed CVE counts in one compact line for list rows.
///
/// Shows only non-zero severities. `clean` appears only when the sealed scan
/// reports zero findings in total. When the counts are not from a Current scan
/// of the selected revision, a `historical` chip follows them and the counts
/// are dimmed, so they never read as findings for the current revision. The
/// scan identity, revision and status are in the tooltip; [`EvidenceCves`]
/// shows them in full on detail surfaces.
#[component]
pub fn CompactEvidenceCves(
    evidence: ConfigurationAssurance,
    freshness: EvidenceFreshness,
) -> Element {
    let status = evidence_status(&evidence, freshness);
    let Some((scan, counts)) = last_good_scan(&evidence).and_then(|s| Some((s, s.cves.as_ref()?)))
    else {
        return rsx! { span { class: "chip chip-unknown", title: "{status.label()}: no sealed findings", "no scan" } };
    };
    let historical = status != EvidenceStatus::Current;
    let sha = scan.revision.sha.get(..8).unwrap_or(&scan.revision.sha);
    let title = format!(
        "Configuration evidence, not runtime exposure · {} · revision {sha} · scan {} · {} medium, {} low, {} total",
        status.label(),
        scan.scan_id,
        counts.medium,
        counts.low,
        counts.total,
    );
    rsx! {
        div { class: if historical { "mg-cves-compact historical" } else { "mg-cves-compact" }, title: "{title}",
            for (class, label) in compact_cve_chips(counts, status) {
                span { class: "chip {class}", "{label}" }
            }
        }
    }
}

/// Returns the `(chip class, label)` list for [`CompactEvidenceCves`].
///
/// Zero severities are omitted. Medium and low findings appear as one chip only
/// when there are no critical or high findings. `clean` requires a total of
/// zero. A status other than Current appends `historical`, or `last good` when
/// the newest scan failed.
pub fn compact_cve_chips(
    counts: &crate::api::models::EvidenceCveCounts,
    status: EvidenceStatus,
) -> Vec<(&'static str, String)> {
    let mut chips = Vec::new();
    if counts.critical > 0 {
        chips.push((
            "chip-critical mg-cves-count",
            format!("{} crit", counts.critical),
        ));
    }
    if counts.high > 0 {
        chips.push((
            "chip-warning mg-cves-count",
            format!("{} high", counts.high),
        ));
    }
    let lower = counts.medium + counts.low;
    if counts.critical == 0 && counts.high == 0 && lower > 0 {
        chips.push(("chip-unknown mg-cves-count", format!("{lower} med/low")));
    }
    if counts.total == 0 {
        chips.push(("chip-healthy mg-cves-count", "✓ clean".to_string()));
    }
    if status != EvidenceStatus::Current {
        let label = if status == EvidenceStatus::Failed {
            "last good"
        } else {
            "historical"
        };
        chips.push(("chip-unknown mg-cves-historical", label.to_string()));
    }
    chips
}

/// Returns the status-dot color for a nonmanaged resource's evidence status.
///
/// The color describes configuration evidence only. It never describes guest
/// health, which the server does not verify.
pub fn evidence_status_color(status: EvidenceStatus) -> &'static str {
    match status.chip_class() {
        "chip-critical" => "var(--cf-red)",
        "chip-warning" => "var(--cf-amber)",
        "chip-info" => "var(--cf-blue)",
        "chip-healthy" => "var(--cf-emerald)",
        _ => "var(--cf-gray-dot)",
    }
}

/// Renders parent-reported runtime metadata separately from configuration evidence.
/// Only use for external systems. Reload invokes a latest-only read owned by the
/// view; retain the previous observation on error. Runner paths are not guest
/// closures and cannot establish a match with the scanned configuration.
///
/// `parent_label` is a display name for the reporting parent. The caller MUST
/// pass it only for the parent that `observation.parent_host_id` identifies.
/// When it is absent, the header shows the parent UUID.
#[component]
pub fn RuntimeCard(
    observation: Option<GuestObservationRow>,
    freshness: EvidenceFreshness,
    #[props(default)] parent_label: Option<String>,
    #[props(default)] loading: bool,
    #[props(default)] error: Option<String>,
    #[props(default)] on_reload: Option<EventHandler<()>>,
) -> Element {
    let age = match freshness {
        EvidenceFreshness::Fresh => "Within reporting freshness policy",
        EvidenceFreshness::Stale => "Stale observation",
        EvidenceFreshness::Unknown => "Observation freshness unknown",
    };
    rsx! { div { class: "card sd-card mg-runtime",
        div { class: "sd-card-head",
            h2 { "Runtime observations" }
            if let Some(reload) = on_reload {
                button { class: "btn btn-ghost xs focus-ring", r#type: "button", disabled: loading,
                    onclick: move |_| reload.call(()),
                    if loading { "Loading latest…" } else { "Reload latest" }
                }
            }
        }
        if let Some(message) = error {
            div { class: "mg-notice mg-notice-warn", role: "status",
                div { class: "mg-notice-body",
                    div { class: "mg-notice-title", "Could not reload the latest stored observation" }
                    div { class: "mg-notice-text", "{message} Previously loaded evidence is retained below." }
                }
            }
        }
        if let Some(report) = observation {
            div { class: "mg-obs-head", "Reported by host {parent_label.clone().unwrap_or_else(|| report.parent_host_id.to_string())} · {report.observed_at.to_rfc3339()}" }
            p { class: "mg-dim", "{age}" }
            dl { class: "kv-grid",
                dt { "Guest name" } dd { "{report.guest_identity}" }
                dt { "Systemd unit" } dd { "{report.unit}" }
                dt { "Unit state" } dd { {report.unit_active_state.as_deref().unwrap_or("unknown")} }
                dt { "Runner store path" } dd { {report.runner_store_path.as_deref().unwrap_or("unavailable")} }
                dt { "Received at" } dd { "{report.received_at.to_rfc3339()}" }
                dt { "Compared with scan" } dd { span { class: "chip chip-unknown", "not comparable" } }
            }
        } else {
            div { class: "mg-unknown",
                div { class: "mg-unknown-title", "Runtime state unknown" }
                p { "No applicable parent-host observation is available." }
            }
        }
        div { class: "mg-unknown",
            div { class: "mg-unknown-title", "Verified guest state: not available" }
            p { "An active host unit does not prove guest health or the running configuration. Reload reads stored evidence; it does not contact the parent host." }
        }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::*;

    #[test]
    fn build_job_and_derivation_wire_states_share_the_pipeline_contract() {
        for state in ["build-complete", "complete", "success"] {
            assert_eq!(
                build_stage_presentation(state),
                ("Built", "var(--cf-emerald)")
            );
        }
        for (states, expected) in [
            (["build-pending", "queued"], ("Queued", "var(--cf-blue)")),
            (
                ["build-in-progress", "building"],
                ("Building", "var(--cf-blue)"),
            ),
            (["build-failed", "failed"], ("Failed", "var(--cf-red)")),
        ] {
            for state in states {
                assert_eq!(build_stage_presentation(state), expected);
            }
        }
        assert_eq!(
            build_stage_presentation("unavailable"),
            ("unavailable", "var(--cf-gray-dot)")
        );
    }

    #[test]
    fn selected_head_freshness_uses_deployed_cadence_and_never_invents_policy() {
        let now = DateTime::from_timestamp(1_700_100_000, 0).unwrap();
        let policy = |interval: &str| ScanSchedulePolicyResponse {
            on_build: true,
            deployed_interval: interval.into(),
            recent_interval: "1h".into(),
            archived_interval: "1h".into(),
            archived_enabled: false,
            rebuild_to_scan: false,
            updated_at: now,
        };
        let mut value = evidence();
        let mut selected = scan();
        selected.completed_at = Some(now - Duration::hours(2));
        value.selected_scan = Some(selected);
        // The other cadence classes must not apply to the selected head.
        assert_eq!(
            selected_head_scan_freshness(&value, Some(&policy("1d")), now),
            EvidenceFreshness::Fresh
        );
        assert_eq!(
            selected_head_scan_freshness(&value, Some(&policy("90m")), now),
            EvidenceFreshness::Stale
        );
        assert_eq!(
            selected_head_scan_freshness(&value, None, now),
            EvidenceFreshness::Unknown
        );
        assert_eq!(
            selected_head_scan_freshness(&value, Some(&policy("never")), now),
            EvidenceFreshness::Unknown
        );
        value.management_mode = ManagementMode::Managed;
        assert_eq!(
            selected_head_scan_freshness(&value, Some(&policy("1d")), now),
            EvidenceFreshness::Unknown,
            "managed runtime freshness is owned by Current authority"
        );
        value.management_mode = ManagementMode::External;
        value.selected_scan.as_mut().unwrap().completed_at = None;
        assert_eq!(
            selected_head_scan_freshness(&value, Some(&policy("1d")), now),
            EvidenceFreshness::Unknown
        );
    }

    fn revision(id: i32) -> RevisionEvidence {
        RevisionEvidence {
            commit_id: id,
            sha: format!("{id:040x}"),
            committed_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            message: None,
        }
    }

    fn evidence() -> ConfigurationAssurance {
        ConfigurationAssurance {
            system_id: Uuid::from_u128(508),
            management_mode: ManagementMode::Reference,
            configuration_name: "baseline".into(),
            flake_id: Some(1),
            branch: Some("main".into()),
            selected_revision: Some(revision(1)),
            evaluation: EvaluationEvidence {
                availability: "available".into(),
                state: "evaluated".into(),
                attempt_id: None,
                error: None,
                completed_at: None,
            },
            selected_artifact: Some(ArtifactEvidence {
                derivation_id: 10,
                revision: revision(1),
                configuration_name: "baseline".into(),
                state: "complete".into(),
                derivation_path: None,
                output_path: Some("actual-output".into()),
                expected_output_path: None,
                published_at: None,
                error: None,
                build: None,
                execution_derivation_id: None,
                execution_revision: None,
            }),
            selected_scan: None,
            latest_terminal_scan: None,
            latest_good_scan: None,
            history: EvidenceHistory {
                limit_per_kind: 20,
                evaluations: vec![],
                builds: vec![],
                scans: vec![],
            },
        }
    }

    fn scan() -> ScanEvidence {
        ScanEvidence {
            scan_id: Uuid::from_u128(509),
            derivation_id: 10,
            revision: revision(1),
            configuration_name: "baseline".into(),
            state: "completed".into(),
            created_at: revision(1).committed_at,
            completed_at: Some(revision(1).committed_at),
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
    fn current_requires_sealing_exact_selection_and_real_freshness() {
        let mut value = evidence();
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Missing
        );
        value.selected_scan = Some(scan());
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Current
        );
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Stale),
            EvidenceStatus::Stale
        );
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Unknown),
            EvidenceStatus::FreshnessUnknown
        );
        value.selected_scan.as_mut().unwrap().revision = revision(2);
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Stale
        );
        value.selected_scan = Some(scan());
        value.selected_scan.as_mut().unwrap().cves = None;
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Unavailable
        );
        value.selected_scan.as_mut().unwrap().cves = scan().cves;
        value
            .selected_scan
            .as_mut()
            .unwrap()
            .evidence_schema_version = 0;
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Unavailable
        );
    }

    #[test]
    fn selected_failures_keep_last_good_without_promoting_history() {
        let mut value = evidence();
        value.latest_good_scan = Some(scan());
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Stale
        );
        value.selected_scan = Some(scan());
        value.selected_scan.as_mut().unwrap().state = "failed".into();
        value.selected_scan.as_mut().unwrap().cves = None;
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Failed
        );
        assert!(last_good_scan(&value).is_some());
        value.selected_artifact.as_mut().unwrap().state = "build-failed".into();
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::BuildFailed
        );
        value.evaluation.state = "eval_failed".into();
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::EvaluationFailed
        );
        value.evaluation.availability = "removed".into();
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::ConfigurationMissing
        );
        assert!(last_good_scan(&value).is_some());
        assert_eq!(
            scan_evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Failed
        );
    }

    #[test]
    fn selected_counts_are_not_replaced_by_a_newer_historical_rescan() {
        let mut value = evidence();
        let mut historical = scan();
        historical.revision = revision(2);
        historical.cves.as_mut().unwrap().total = 5;
        value.latest_good_scan = Some(historical);
        value.selected_scan = Some(scan());
        assert_eq!(last_good_scan(&value).unwrap().revision.commit_id, 1);
        assert_eq!(
            last_good_scan(&value).unwrap().cves.as_ref().unwrap().total,
            0
        );
        assert_eq!(
            scan_evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Current
        );
        value.selected_scan.as_mut().unwrap().derivation_id = 99;
        assert_eq!(
            scan_evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::Stale
        );
    }

    #[test]
    fn unknown_evaluation_and_waiting_states_are_not_failed_or_clean() {
        let mut value = evidence();
        value.evaluation.state = "complete".into();
        assert_eq!(
            evidence_status(&value, EvidenceFreshness::Fresh),
            EvidenceStatus::EvaluationUnavailable
        );
        value.evaluation.state = "evaluated".into();
        value.selected_scan = Some(scan());
        for (state, expected) in [
            ("awaiting_build", EvidenceStatus::AwaitingBuild),
            ("awaiting_closure", EvidenceStatus::AwaitingClosure),
            ("pending", EvidenceStatus::Scanning),
            ("in_progress", EvidenceStatus::Scanning),
            ("unavailable", EvidenceStatus::Unavailable),
        ] {
            value.selected_scan.as_mut().unwrap().state = state.into();
            assert_eq!(evidence_status(&value, EvidenceFreshness::Fresh), expected);
        }
    }

    #[test]
    fn freshness_uses_collection_time_and_explicit_policy_boundary() {
        let now = revision(1).committed_at;
        let age = Duration::hours(4);
        assert_eq!(
            evidence_freshness(Some(now), now, None),
            EvidenceFreshness::Unknown
        );
        assert_eq!(
            evidence_freshness(None, now, Some(age)),
            EvidenceFreshness::Unknown
        );
        assert_eq!(
            evidence_freshness(Some(now + age), now, Some(age)),
            EvidenceFreshness::Unknown
        );
        assert_eq!(
            evidence_freshness(Some(now - age), now, Some(age)),
            EvidenceFreshness::Stale
        );
        assert_eq!(
            evidence_freshness(Some(now - Duration::hours(3)), now, Some(age)),
            EvidenceFreshness::Fresh
        );
    }

    #[test]
    fn nullable_evidence_and_latest_observation_wire_contracts() {
        let mut unsealed = scan();
        unsealed.state = "failed".into();
        unsealed.cves = None;
        let json = serde_json::to_value(unsealed).unwrap();
        assert!(json["cves"].is_null());
        assert!(
            serde_json::from_value::<ScanEvidence>(json)
                .unwrap()
                .cves
                .is_none()
        );
        assert!(
            serde_json::from_str::<Option<GuestObservationRow>>("null")
                .unwrap()
                .is_none()
        );
        let report: GuestObservationRow = serde_json::from_value(serde_json::json!({
            "id":508, "guest_id":Uuid::from_u128(1), "parent_host_id":Uuid::from_u128(2),
            "environment_id":Uuid::from_u128(3), "guest_identity":"guest", "unit":"microvm@guest.service",
            "unit_active_state":null, "runner_store_path":null,
            "observed_at":"2026-10-31T10:00:00Z", "received_at":"2026-10-31T11:00:00Z"
        })).unwrap();
        assert!(report.unit_active_state.is_none());
        assert!(report.runner_store_path.is_none());
        assert!(report.observed_at < report.received_at);
    }

    #[test]
    fn compact_cve_chips_hide_zeros_and_mark_older_evidence() {
        let counts = |critical, high, medium, low| EvidenceCveCounts {
            total: critical + high + medium + low,
            critical,
            high,
            medium,
            low,
        };
        let labels = |c: &EvidenceCveCounts, s| {
            compact_cve_chips(c, s)
                .into_iter()
                .map(|(_, l)| l)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels(&counts(0, 2, 5, 1), EvidenceStatus::Current),
            vec!["2 high"]
        );
        assert_eq!(
            labels(&counts(0, 0, 3, 1), EvidenceStatus::Current),
            vec!["4 med/low"]
        );
        assert_eq!(
            labels(&counts(0, 0, 0, 0), EvidenceStatus::Current),
            vec!["✓ clean"]
        );
        // Counts from an older scan never read as findings for the current revision.
        assert_eq!(
            labels(&counts(1, 6, 0, 0), EvidenceStatus::Evaluating),
            vec!["1 crit", "6 high", "historical"]
        );
        assert_eq!(
            labels(&counts(0, 0, 0, 0), EvidenceStatus::Failed),
            vec!["✓ clean", "last good"]
        );
    }
}
