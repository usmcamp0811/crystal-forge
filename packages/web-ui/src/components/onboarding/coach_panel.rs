//! Administrator onboarding coach component.
//!
//! The server derives completion from persisted resources. Browser storage
//! controls presentation only and never marks a setup step complete.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api::client::{fetch_setup_wizard_progress, set_setup_wizard_dismissed};
use crate::api::models::{SetupWizardProgressResponse, SetupWizardStepStatus};
use crate::routes::Route;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CoachStep {
    id: &'static str,
    label: &'static str,
    short: &'static str,
    description: &'static str,
    pending: &'static str,
    destination: CoachDestination,
    setup_context: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CoachDestination {
    Environments,
    Flakes,
    Builders,
    Caches,
    Systems,
    Policies,
    Compliance,
}

const STEPS: [CoachStep; 9] = [
    CoachStep {
        id: "environment",
        label: "Create environment",
        short: "Define a deployment boundary",
        description: "Group systems by deployment domain such as production, staging, or development.",
        pending: "Pending",
        destination: CoachDestination::Environments,
        setup_context: true,
    },
    CoachStep {
        id: "flake",
        label: "Add flake",
        short: "Point at your configuration repository",
        description: "Register the Git repository that contains the NixOS configurations Crystal Forge evaluates.",
        pending: "Pending",
        destination: CoachDestination::Flakes,
        setup_context: true,
    },
    CoachStep {
        id: "builder",
        label: "Register builder",
        short: "Connect a build worker",
        description: "Add a worker that evaluates flakes, builds derivations, and scans for vulnerabilities.",
        pending: "Pending",
        destination: CoachDestination::Builders,
        setup_context: true,
    },
    CoachStep {
        id: "cache",
        label: "Configure cache",
        short: "Add a binary cache",
        description: "Give systems a trusted source for prebuilt packages instead of rebuilding each package.",
        pending: "Pending",
        destination: CoachDestination::Caches,
        setup_context: true,
    },
    CoachStep {
        id: "system",
        label: "Register system",
        short: "Add a host to manage",
        description: "Connect a NixOS host to an environment and flake with its server-issued identity.",
        pending: "Pending",
        destination: CoachDestination::Systems,
        setup_context: true,
    },
    CoachStep {
        id: "agent",
        label: "Deploy agent",
        short: "Connect and acknowledge the host",
        description: "Install the agent, wait for its first signed report, then complete the server-backed acknowledgement.",
        pending: "Review agent deployment and acknowledge it after the first system reports in",
        destination: CoachDestination::Systems,
        setup_context: true,
    },
    CoachStep {
        id: "policy",
        label: "Create policy",
        short: "Define a compliance rule",
        description: "Create or import a policy that Crystal Forge can evaluate against managed systems.",
        pending: "Create or import a deployment policy",
        destination: CoachDestination::Policies,
        setup_context: false,
    },
    CoachStep {
        id: "bundle",
        label: "Build compliance bundle",
        short: "Group policies into an audit bundle",
        description: "Publish a compliance bundle to review framework evidence and results by system.",
        pending: "Build a bundle in Compliance",
        destination: CoachDestination::Compliance,
        setup_context: false,
    },
    CoachStep {
        id: "poam",
        label: "Track a POA&M",
        short: "Plan remediation for a failing finding",
        description: "Open failing evidence and create a server-backed remediation plan with an owner, target, and milestones.",
        pending: "A failing control's evidence is required; create the POA&M from that finding in Compliance",
        destination: CoachDestination::Compliance,
        setup_context: false,
    },
];

fn collapsed_from_storage() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|storage| storage.get_item("cf.coach.collapsed").ok())
        .flatten()
        .map(|value| value == "true")
        .unwrap_or(false)
}

fn force_show_from_storage() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|storage| storage.get_item("cf.coach.force_show").ok())
        .flatten()
        .map(|value| value == "true")
        .unwrap_or(false)
}

fn store_force_show(value: bool) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("cf.coach.force_show", if value { "true" } else { "false" });
    }
}

fn store_collapsed(value: bool) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("cf.coach.collapsed", if value { "true" } else { "false" });
    }
}

fn store_setup_context() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        // Existing destination views use this for contextual setup guidance banners.
        let _ = storage.set_item("cf.from_setup", "1");
    }
}

fn route_for_step(step: CoachStep) -> Route {
    match step.destination {
        CoachDestination::Environments => Route::EnvironmentsView {},
        CoachDestination::Flakes => Route::FlakesView {},
        CoachDestination::Builders => Route::BuildersView {},
        CoachDestination::Caches => Route::CachesView {},
        CoachDestination::Systems => Route::SystemsView {},
        CoachDestination::Policies => Route::PoliciesView {},
        CoachDestination::Compliance => Route::ComplianceView {
            bundle: String::new(),
            version: String::new(),
            system: String::new(),
            policy: String::new(),
            poam: String::new(),
            view: String::new(),
        },
    }
}

fn step_status(step: CoachStep, progress: &SetupWizardProgressResponse) -> SetupWizardStepStatus {
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

fn step_locked(step: CoachStep, progress: &SetupWizardProgressResponse) -> bool {
    step.id == "agent" && !progress.system.complete
}

fn current_step_id<'a>(
    steps: &'a [CoachStep],
    progress: &SetupWizardProgressResponse,
) -> Option<&'a str> {
    steps
        .iter()
        .copied()
        .find(|step| !step_status(*step, progress).complete && !step_locked(*step, progress))
        .map(|step| step.id)
}

/// Renders the dismissible nine-step administrator setup coach.
///
/// The component polls server-derived progress and owns only collapsed and
/// force-visible presentation state. Navigation cannot complete a setup step.
#[component]
pub fn OnboardingCoachPanel() -> Element {
    let mut refresh_tick = use_signal(|| 0_u64);
    let mut collapsed = use_signal(collapsed_from_storage);
    let mut force_show = use_signal(force_show_from_storage);
    let mut action_error = use_signal(|| None::<String>);
    let navigator = use_navigator();

    // Poll progress periodically so completion state is live while users
    // configure entities in other tabs/routes.
    use_future(move || async move {
        loop {
            TimeoutFuture::new(8000).await;
            refresh_tick.set(refresh_tick() + 1);
        }
    });

    let progress = use_resource(move || async move {
        let _ = refresh_tick();
        fetch_setup_wizard_progress().await
    });

    let progress_data = match progress.read().as_ref() {
        Some(Ok(value)) => value.clone(),
        Some(Err(_)) => {
            return rsx! {
                aside {
                    class: "cf-coach-drawer cf-coach-state",
                    role: "alert",
                    aria_label: "Setup Coach",
                    p { "Onboarding coach unavailable because progress could not be loaded." }
                }
            };
        }
        None => {
            return rsx! {
                aside {
                    class: "cf-coach-drawer cf-coach-state",
                    role: "status",
                    aria_live: "polite",
                    aria_label: "Setup Coach",
                    p { "Loading onboarding coach..." }
                }
            };
        }
    };

    // An older server omits all Phase-7 fields. Keep its original six-step
    // presentation and completion rule instead of showing impossible steps.
    let has_extended_progress = progress_data.policy.is_some()
        && progress_data.bundle.is_some()
        && progress_data.poam.is_some()
        && progress_data.all_coach_steps_complete.is_some();
    let visible_steps = if has_extended_progress {
        &STEPS[..]
    } else {
        &STEPS[..6]
    };

    // Respect persisted dismissal and hide automatically once fully complete.
    let is_force_show = force_show() || force_show_from_storage();
    let all_steps_complete = progress_data
        .all_coach_steps_complete
        .unwrap_or(progress_data.all_required_complete && progress_data.agent_acknowledged);

    if !is_force_show && (progress_data.dismissed || all_steps_complete) {
        return rsx! {};
    }

    let required_completed = visible_steps
        .iter()
        .copied()
        .filter(|step| step_status(*step, &progress_data).complete)
        .count();
    let total_steps = visible_steps.len();
    let current_step_id = current_step_id(visible_steps, &progress_data);
    let progress_percent = required_completed * 100 / total_steps.max(1);

    // Minimized: a grid row reserves space so the tab cannot obscure page content.
    if !is_force_show && collapsed() {
        return rsx! {
            button {
                class: "cf-coach-tab",
                "data-testid": "onboarding-coach-panel",
                aria_label: "Open Setup Coach, {required_completed} of {total_steps} complete",
                onclick: move |_| {
                    collapsed.set(false);
                    store_collapsed(false);
                },
                style: "--coach-top: var(--coach-top, 64px);",
                span { class: "cf-coach-tab-mark", aria_hidden: "true", "CF" }
                span { class: "cf-coach-tab-title", "Setup Guide" }
                span { class: "cf-coach-tab-count", "{required_completed}/{total_steps}" }
            }
        };
    }

    rsx! {
        aside {
            class: "cf-coach-drawer",
            role: "complementary",
            aria_label: "Setup Coach",
            aria_describedby: "setup-coach-progress-status",
            "data-testid": "onboarding-coach-panel",

            header { class: "cf-coach-head",
                div { class: "cf-coach-heading",
                    span { class: "cf-coach-mark", aria_hidden: "true", "CF" }
                    div {
                        h2 { "Setup Coach" }
                        p { id: "setup-coach-progress-status", role: "status", aria_live: "polite", aria_atomic: "true", "{required_completed} of {total_steps} complete" }
                    }
                }
                div { class: "cf-coach-actions",
                    button {
                        class: "coach-link focus-ring",
                        "data-testid": "onboarding-coach-collapse",
                        aria_label: "Minimize Setup Coach",
                        onclick: move |_| {
                            collapsed.set(true);
                            store_collapsed(true);
                        },
                        "Minimize"
                    }
                    button {
                        class: "coach-link focus-ring",
                        "data-testid": "onboarding-coach-dismiss",
                        aria_label: "Close and dismiss Setup Coach",
                        onclick: move |_| {
                            let mut action_error = action_error;
                            let mut refresh_tick = refresh_tick;
                            spawn(async move {
                                match set_setup_wizard_dismissed(true).await {
                                    Ok(_) => {
                                        action_error.set(None);
                                        force_show.set(false);
                                        store_force_show(false);
                                        refresh_tick.set(refresh_tick() + 1);
                                    }
                                    Err(err) => action_error
                                        .set(Some(format!("Failed to dismiss onboarding coach: {err}"))),
                                }
                            });
                        },
                        "Dismiss"
                    }
                }
            }

            div { class: "cf-coach-progress", role: "progressbar", aria_label: "Setup progress", aria_valuemin: "0", aria_valuemax: "100", aria_valuenow: "{progress_percent}",
                for step in visible_steps.iter().copied() {
                    { let status = step_status(step, &progress_data); let current = current_step_id == Some(step.id); rsx! { span { class: if status.complete { "done" } else if current { "current" } else { "" } } } }
                }
            }

            div { class: "cf-coach-steps",
                    if let Some(message) = action_error() {
                        div {
                            role: "alert",
                            class: "cf-coach-error",
                            "{message}"
                        }
                    }

                    for step in visible_steps.iter().copied() {
                        {
                            let status = step_status(step, &progress_data);
                            let locked = step_locked(step, &progress_data);
                            let current = current_step_id == Some(step.id);
                            let state_class = if status.complete { "complete" } else if locked { "locked" } else if current { "current" } else { "pending" };
                            let position = visible_steps.iter().position(|candidate| candidate.id == step.id).unwrap_or(0) + 1;
                            rsx! {
                                button {
                                    class: "cf-coach-step cf-coach-step-{state_class} focus-ring",
                                    "data-testid": "onboarding-step-{step.id}",
                                    disabled: locked,
                                    aria_current: if current { "step" } else { "false" },
                                    aria_label: if locked { format!("Step {position}, {}: locked until a system is registered", step.label) } else { format!("Step {position}, {}: {state_class}", step.label) },
                                    onclick: move |_| {
                                        if step.setup_context {
                                            store_setup_context();
                                        }
                                        navigator.push(route_for_step(step));
                                    },
                                    span { class: "cf-coach-step-rail",
                                        span { class: "cf-coach-step-node", if status.complete { "✓" } else if locked { "-" } else { "{position}" } }
                                        if position < total_steps { span { class: "cf-coach-step-line" } }
                                    }
                                    span { class: "cf-coach-step-body",
                                        strong { "{step.label}" }
                                        span { class: "cf-coach-step-short", "{step.short}" }
                                        if current { span { class: "cf-coach-step-description", "{step.description}" } }
                                        span { class: "cf-coach-step-status",
                                            if locked { "Locked: register a system first" }
                                            else if step.id == "agent" && status.complete { "Acknowledged" }
                                            else if status.complete { "Configured" }
                                            else if current { "Current step" }
                                            else { "{step.pending}" }
                                        }
                                    }
                                    span { class: "cf-coach-step-aff", aria_hidden: "true", if status.complete { "✓" } else if locked { "" } else { ">" } }
                                }
                            }
                        }
                    }

                    footer { class: "cf-coach-foot",
                        span { "Progress is derived from server state." }
                        button {
                            class: "coach-link focus-ring",
                            "data-testid": "onboarding-coach-refresh",
                            aria_label: "Refresh Setup Coach progress",
                            onclick: move |_| refresh_tick.set(refresh_tick() + 1),
                            "Refresh"
                        }
                    }
                }
        }
    }
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
    fn coach_steps_have_required_order_and_copy() {
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
        assert!(STEPS[8].pending.contains("failing control's evidence"));
        assert_eq!(STEPS[0].short, "Define a deployment boundary");
        assert!(STEPS.iter().all(|step| !step.description.is_empty()));
        assert!(
            STEPS[8]
                .pending
                .contains("create the POA&M from that finding")
        );
    }

    #[test]
    fn coach_new_step_statuses_use_server_progress() {
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
        assert!(STEPS[5].pending.contains("acknowledge"));
    }

    #[test]
    fn coach_new_steps_use_typed_routes_without_setup_context() {
        assert_eq!(route_for_step(STEPS[6]), Route::PoliciesView {});
        assert!(matches!(
            route_for_step(STEPS[7]),
            Route::ComplianceView { .. }
        ));
        assert!(matches!(
            route_for_step(STEPS[8]),
            Route::ComplianceView { .. }
        ));
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
    fn task433_responsive_coach_is_nonmodal_and_viewport_bounded() {
        let css = include_str!("../../../assets/app.css");
        assert!(css.contains(".cf-coach-drawer"));
        assert!(css.contains("max-height: min(56dvh"));
        assert!(css.contains("bottom: max(8px, env(safe-area-inset-bottom))"));
        assert!(css.contains(".cf-coach-steps { flex: 1; max-height: none; }"));
        assert!(css.contains(".app:has(> .cf-coach-tab)"));
        assert!(css.contains("grid-template-rows: minmax(0, 1fr) auto"));
        assert!(css.contains("grid-column: 1"));
    }
}
