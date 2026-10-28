//! Setup track definitions and server-derived completion rules.
//!
//! The server derives setup completion from persisted resources through
//! `GET /api/v1/admin/setup-progress`. That endpoint is Administrator-only.
//! The browser never completes a setup step, and navigating to a page never
//! completes one either.

use crate::api::models::{SetupWizardProgressResponse, SetupWizardStepStatus};
use crate::routes::Route;

/// One setup step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetupStep {
    /// Stable identifier used in test identifiers and server mapping.
    pub id: &'static str,
    /// Step title.
    pub label: &'static str,
    /// One-line description shown on the current step.
    pub short: &'static str,
    /// Explanation shown in the page callout.
    pub blurb: &'static str,
    /// Condition the server uses to report the step complete.
    pub complete_when: &'static str,
    /// Name of the page action, when the step has one.
    pub action: Option<&'static str>,
    /// Icon name from the design.
    pub icon: &'static str,
    /// Destination page.
    pub destination: SetupDestination,
    /// Whether to set the `cf.from_setup` flag so the page shows its setup
    /// callout.
    pub setup_context: bool,
    /// `data-coach-target` of the page action, when the page has one.
    pub target: Option<&'static str>,
}

/// Page a setup step opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SetupDestination {
    /// Environments.
    Environments,
    /// Flakes.
    Flakes,
    /// Builders.
    Builders,
    /// Caches.
    Caches,
    /// Systems.
    Systems,
    /// Policies.
    Policies,
    /// Compliance.
    Compliance,
    /// POA&M register.
    Poams,
}

/// The nine setup steps, in order.
pub static STEPS: [SetupStep; 9] = [
    SetupStep {
        id: "environment",
        label: "Create environment",
        short: "Define an operational boundary",
        blurb: "Environments are the operational and security boundary for deployment, authorization, compliance assignments and caches. Access grants, bundle versions and cache scope all attach here.",
        complete_when: "an environment is saved",
        action: Some("Add environment"),
        icon: "env",
        destination: SetupDestination::Environments,
        setup_context: true,
        target: Some("env"),
    },
    SetupStep {
        id: "flake",
        label: "Add flake",
        short: "Register the configuration source",
        blurb: "Crystal Forge monitors and evaluates this configuration source on the server. Builds and exact configurations are scanned according to the configured scan policy.",
        complete_when: "a flake is registered",
        action: Some("Add flake"),
        icon: "git",
        destination: SetupDestination::Flakes,
        setup_context: true,
        target: Some("flake"),
    },
    SetupStep {
        id: "builder",
        label: "Register builder",
        short: "Connect a build worker",
        blurb: "Connect a build worker that builds server-evaluated derivations and performs the build-side security work Crystal Forge assigns. Authoritative evaluation stays on the server. Paste the worker's public key so the server recognizes it.",
        complete_when: "a builder is registered with its key",
        action: Some("Register builder"),
        icon: "cpu",
        destination: SetupDestination::Builders,
        setup_context: true,
        target: Some("builder"),
    },
    SetupStep {
        id: "cache",
        label: "Configure cache",
        short: "Add a binary cache",
        blurb: "Builders push exact closures here and systems pull them instead of rebuilding. Attic is recommended for production.",
        complete_when: "a cache is configured",
        action: Some("Add cache"),
        icon: "cube",
        destination: SetupDestination::Caches,
        setup_context: true,
        target: Some("cache"),
    },
    SetupStep {
        id: "system",
        label: "Register system",
        short: "Add a host to manage",
        blurb: "Register a NixOS host with its environment, flake and key. Each system is identified by its own key.",
        complete_when: "a system record is saved",
        action: Some("Add system"),
        icon: "server",
        destination: SetupDestination::Systems,
        setup_context: true,
        target: Some("system"),
    },
    SetupStep {
        id: "agent",
        label: "Deploy agent",
        short: "Connect and acknowledge the host",
        blurb: "Install the agent, wait for its first signed report, then complete the administrator acknowledgement.",
        complete_when: "an administrator acknowledges the agent after its first signed report",
        action: None,
        icon: "deploy",
        destination: SetupDestination::Systems,
        setup_context: true,
        target: None,
    },
    SetupStep {
        id: "policy",
        label: "Create policy",
        short: "Create or import a policy",
        blurb: "Platform policies govern pipeline mechanics such as deployment and approval rules. Security controls carry framework criteria such as STIG rules. Whether a failure blocks deployment depends on enforcement, not on the policy existing.",
        complete_when: "you create or import a policy lineage",
        action: Some("New custom policy"),
        icon: "file",
        destination: SetupDestination::Policies,
        setup_context: false,
        target: Some("policy"),
    },
    SetupStep {
        id: "bundle",
        label: "Build compliance bundle",
        short: "Group controls into a baseline",
        blurb: "A bundle collects security controls into a baseline such as a STIG or NIST profile. Bundle versions are reusable; assignments select or pin a version per environment or system.",
        complete_when: "a compliance bundle is saved",
        action: Some("New bundle"),
        icon: "shield",
        destination: SetupDestination::Compliance,
        setup_context: false,
        target: Some("bundle"),
    },
    SetupStep {
        id: "poam",
        label: "Track a POA&M",
        short: "Plan remediation for a finding",
        blurb: "A POA&M is a remediation plan with an owner, target date and milestones. It can come from failing compliance evidence, a scheduled CVE patch, or converting an accepted risk.",
        complete_when: "any POA&M exists, in any lifecycle state",
        action: None,
        icon: "activity",
        destination: SetupDestination::Poams,
        setup_context: false,
        target: None,
    },
];

/// Returns the typed route that opens `step`.
pub fn route_for_step(step: SetupStep) -> Route {
    match step.destination {
        SetupDestination::Environments => Route::EnvironmentsView {
            query: String::new(),
        },
        SetupDestination::Flakes => Route::FlakesView {
            query: String::new(),
        },
        SetupDestination::Builders => Route::BuildersView {},
        SetupDestination::Caches => Route::CachesView {},
        SetupDestination::Systems => Route::SystemsView {
            query: String::new(),
        },
        SetupDestination::Policies => Route::PoliciesView {},
        SetupDestination::Compliance => Route::ComplianceView {
            bundle: String::new(),
            version: String::new(),
            system: String::new(),
            policy: String::new(),
            poam: String::new(),
            view: String::new(),
        },
        SetupDestination::Poams => Route::PoamsView {
            query: String::new(),
        },
    }
}

/// Returns the server-reported status of `step`.
pub fn step_status(
    step: SetupStep,
    progress: &SetupWizardProgressResponse,
) -> SetupWizardStepStatus {
    match step.id {
        "environment" => progress.environment.clone(),
        "flake" => progress.flake.clone(),
        "builder" => progress.builder.clone(),
        "cache" => progress.cache.clone(),
        "system" => progress.system.clone(),
        "agent" => SetupWizardStepStatus {
            complete: progress.agent_acknowledged,
            count: if progress.agent_acknowledged { 1 } else { 0 },
        },
        "policy" => progress.policy.clone().unwrap_or_default(),
        "bundle" => progress.bundle.clone().unwrap_or_default(),
        "poam" => progress.poam.clone().unwrap_or_default(),
        _ => SetupWizardStepStatus {
            complete: false,
            count: 0,
        },
    }
}

/// Returns whether `step` waits for an earlier step.
///
/// The agent step needs a registered system. Every other step is independent.
pub fn step_locked(step: SetupStep, progress: &SetupWizardProgressResponse) -> bool {
    step.id == "agent" && !progress.system.complete
}

/// Returns the identifier of the first step that is neither complete nor
/// locked.
pub fn current_step_id<'a>(
    steps: &'a [SetupStep],
    progress: &SetupWizardProgressResponse,
) -> Option<&'a str> {
    steps
        .iter()
        .copied()
        .find(|step| !step_status(*step, progress).complete && !step_locked(*step, progress))
        .map(|step| step.id)
}

/// Returns the visible step the setup page callout teaches on `destination`.
///
/// The callout appears for a step that belongs to the page, is not complete,
/// and is not locked. The step does not need to be the current step, so an
/// administrator who opens a page out of order still sees its guidance. The
/// returned number is the step's one-based position in the visible steps.
pub fn callout_step(
    progress: &SetupWizardProgressResponse,
    destination: SetupDestination,
) -> Option<(usize, SetupStep)> {
    visible_steps(progress)
        .iter()
        .copied()
        .enumerate()
        .find(|(_, step)| {
            step.destination == destination
                && !step_status(*step, progress).complete
                && !step_locked(*step, progress)
        })
        .map(|(index, step)| (index + 1, step))
}

/// Returns the steps an older server can report.
///
/// An older server omits the policy, bundle and POA&M fields. It keeps the
/// original six-step presentation and completion rule so the panel never shows
/// steps the server cannot report.
pub fn visible_steps(progress: &SetupWizardProgressResponse) -> &'static [SetupStep] {
    let extended = progress.policy.is_some()
        && progress.bundle.is_some()
        && progress.poam.is_some()
        && progress.all_coach_steps_complete.is_some();
    if extended { &STEPS[..] } else { &STEPS[..6] }
}

/// Returns whether the server reports every visible step complete.
pub fn all_complete(progress: &SetupWizardProgressResponse) -> bool {
    progress
        .all_coach_steps_complete
        .unwrap_or(progress.all_required_complete && progress.agent_acknowledged)
}

/// Returns the number of visible steps the server reports complete.
pub fn completed_count(progress: &SetupWizardProgressResponse) -> usize {
    visible_steps(progress)
        .iter()
        .copied()
        .filter(|step| step_status(*step, progress).complete)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn incomplete_progress() -> SetupWizardProgressResponse {
        SetupWizardProgressResponse {
            dismissed: false,
            agent_acknowledged: false,
            environment: SetupWizardStepStatus::default(),
            flake: SetupWizardStepStatus::default(),
            builder: SetupWizardStepStatus::default(),
            cache: SetupWizardStepStatus::default(),
            system: SetupWizardStepStatus::default(),
            policy: Some(SetupWizardStepStatus::default()),
            bundle: Some(SetupWizardStepStatus::default()),
            poam: Some(SetupWizardStepStatus::default()),
            all_required_complete: false,
            all_coach_steps_complete: Some(false),
        }
    }

    #[test]
    fn setup_steps_have_the_required_order_and_corrected_copy() {
        assert_eq!(
            STEPS.map(|step| step.label),
            [
                "Create environment",
                "Add flake",
                "Register builder",
                "Configure cache",
                "Register system",
                "Deploy agent",
                "Create policy",
                "Build compliance bundle",
                "Track a POA&M",
            ]
        );
        assert_eq!(STEPS[0].short, "Define an operational boundary");
        assert!(STEPS.iter().all(|step| !step.blurb.is_empty()));
    }

    #[test]
    fn builder_copy_does_not_say_the_builder_evaluates_flakes() {
        let builder = STEPS[2];
        assert!(builder.blurb.contains("server-evaluated derivations"));
        assert!(
            builder
                .blurb
                .contains("Authoritative evaluation stays on the server")
        );
        assert!(!builder.blurb.contains("evaluates flakes"));
        assert!(!builder.short.contains("evaluat"));
    }

    #[test]
    fn flake_copy_does_not_claim_every_commit_is_scanned() {
        let flake = STEPS[1];
        assert!(flake.blurb.contains("configured scan policy"));
        assert!(!flake.blurb.to_lowercase().contains("every commit"));
    }

    #[test]
    fn agent_copy_keeps_the_signed_report_and_acknowledgement_order() {
        let agent = STEPS[5];
        let blurb = agent.blurb;
        let report = blurb.find("first signed report").unwrap();
        let acknowledgement = blurb.find("administrator acknowledgement").unwrap();
        assert!(report < acknowledgement);
        assert!(!blurb.to_lowercase().contains("heartbeat"));
        assert!(agent.complete_when.contains("acknowledges"));
    }

    #[test]
    fn poam_copy_names_every_origin_not_only_compliance() {
        let poam = STEPS[8].blurb;
        assert!(poam.contains("failing compliance evidence"));
        assert!(poam.contains("scheduled CVE patch"));
        assert!(poam.contains("converting an accepted risk"));
    }

    #[test]
    fn step_statuses_use_server_progress() {
        let mut progress = incomplete_progress();
        progress.policy = Some(SetupWizardStepStatus {
            complete: true,
            count: 2,
        });
        assert_eq!(
            step_status(STEPS[6], &progress),
            progress.policy.clone().unwrap()
        );
        assert_eq!(
            step_status(STEPS[7], &progress),
            progress.bundle.clone().unwrap()
        );
        assert_eq!(step_status(STEPS[8], &progress), progress.poam.unwrap());
    }

    #[test]
    fn agent_step_requires_a_registered_system() {
        let mut progress = incomplete_progress();
        assert!(step_locked(STEPS[5], &progress));
        progress.system = SetupWizardStepStatus {
            complete: true,
            count: 1,
        };
        assert!(!step_locked(STEPS[5], &progress));
    }

    #[test]
    fn new_steps_use_typed_routes_without_setup_context() {
        assert_eq!(route_for_step(STEPS[6]), Route::PoliciesView {});
        assert!(matches!(
            route_for_step(STEPS[7]),
            Route::ComplianceView { .. }
        ));
        assert_eq!(
            route_for_step(STEPS[8]),
            Route::PoamsView {
                query: String::new()
            }
        );
        assert!(STEPS[..6].iter().all(|step| step.setup_context));
        assert!(STEPS[6..].iter().all(|step| !step.setup_context));
    }

    #[test]
    fn current_step_skips_complete_and_locked_steps() {
        let mut progress = incomplete_progress();
        progress.environment.complete = true;
        progress.flake.complete = true;
        progress.builder.complete = true;
        progress.cache.complete = true;
        assert_eq!(current_step_id(&STEPS, &progress), Some("system"));
        progress.system.complete = true;
        assert_eq!(current_step_id(&STEPS, &progress), Some("agent"));
        progress.agent_acknowledged = true;
        assert_eq!(current_step_id(&STEPS, &progress), Some("policy"));
    }

    #[test]
    fn callout_teaches_an_incomplete_unlocked_page_step_out_of_order() {
        let mut progress = incomplete_progress();
        // The current step is Environment, but the Flakes page still teaches
        // its own incomplete step.
        let (number, step) = callout_step(&progress, SetupDestination::Flakes).unwrap();
        assert_eq!((number, step.id), (2, "flake"));
        progress.flake.complete = true;
        assert!(callout_step(&progress, SetupDestination::Flakes).is_none());
    }

    #[test]
    fn callout_does_not_teach_a_locked_agent_step() {
        let mut progress = incomplete_progress();
        assert!(
            callout_step(&progress, SetupDestination::Systems)
                .is_some_and(|(_, step)| step.id == "system")
        );
        progress.system.complete = true;
        let (number, step) = callout_step(&progress, SetupDestination::Systems).unwrap();
        assert_eq!((number, step.id), (6, "agent"));
        progress.agent_acknowledged = true;
        assert!(callout_step(&progress, SetupDestination::Systems).is_none());
    }

    #[test]
    fn older_servers_keep_the_six_step_presentation() {
        let mut progress = incomplete_progress();
        assert_eq!(visible_steps(&progress).len(), 9);
        progress.policy = None;
        progress.bundle = None;
        progress.poam = None;
        progress.all_coach_steps_complete = None;
        assert_eq!(visible_steps(&progress).len(), 6);
        progress.all_required_complete = true;
        progress.agent_acknowledged = true;
        assert!(all_complete(&progress));
    }

    #[test]
    fn completion_counts_only_server_reported_steps() {
        let mut progress = incomplete_progress();
        assert_eq!(completed_count(&progress), 0);
        progress.environment.complete = true;
        progress.agent_acknowledged = true;
        assert_eq!(completed_count(&progress), 2);
        assert!(!all_complete(&progress));
        progress.all_coach_steps_complete = Some(true);
        assert!(all_complete(&progress));
    }
}
