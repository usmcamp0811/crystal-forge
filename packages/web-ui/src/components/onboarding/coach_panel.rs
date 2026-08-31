use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api::client::{fetch_setup_wizard_progress, set_setup_wizard_dismissed};
use crate::api::models::{SetupWizardProgressResponse, SetupWizardStepStatus};
use crate::routes::Route;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CoachStep {
    id: &'static str,
    label: &'static str,
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
        pending: "Pending",
        destination: CoachDestination::Environments,
        setup_context: true,
    },
    CoachStep {
        id: "flake",
        label: "Add flake",
        pending: "Pending",
        destination: CoachDestination::Flakes,
        setup_context: true,
    },
    CoachStep {
        id: "builder",
        label: "Register builder",
        pending: "Pending",
        destination: CoachDestination::Builders,
        setup_context: true,
    },
    CoachStep {
        id: "cache",
        label: "Configure cache",
        pending: "Pending",
        destination: CoachDestination::Caches,
        setup_context: true,
    },
    CoachStep {
        id: "system",
        label: "Register system",
        pending: "Pending",
        destination: CoachDestination::Systems,
        setup_context: true,
    },
    CoachStep {
        id: "agent",
        label: "Deploy agent",
        pending: "Review agent deployment and acknowledge it after the first system reports in",
        destination: CoachDestination::Systems,
        setup_context: true,
    },
    CoachStep {
        id: "policy",
        label: "Create policy",
        pending: "Create or import a deployment policy",
        destination: CoachDestination::Policies,
        setup_context: false,
    },
    CoachStep {
        id: "bundle",
        label: "Build compliance bundle",
        pending: "Build a bundle in Compliance",
        destination: CoachDestination::Compliance,
        setup_context: false,
    },
    CoachStep {
        id: "poam",
        label: "Track a POA&M",
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

/// Renders the dismissible nine-step administrator setup coach.
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
                    class: "cf-coach-drawer p-3",
                    style: "--coach-top: var(--coach-top, 64px); border:1px solid rgba(239,68,68,0.5); background:rgba(127,29,29,0.92);",
                    p { class: "text-sm text-red-100", "Onboarding coach unavailable (failed to load progress)." }
                }
            };
        }
        None => {
            return rsx! {
                aside {
                    class: "cf-coach-drawer p-3",
                    style: "--coach-top: var(--coach-top, 64px); border:1px solid rgba(100,116,139,0.45); background:rgba(15,23,42,0.94);",
                    p { class: "text-sm text-slate-200", "Loading onboarding coach..." }
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

    // Minimized: slim tab anchored top-right of the content column, just below the topbar
    if !is_force_show && collapsed() {
        return rsx! {
            button {
                class: "cf-coach-tab",
                "data-testid": "onboarding-coach-panel",
                onclick: move |_| {
                    collapsed.set(false);
                    store_collapsed(false);
                },
                style: "--coach-top: var(--coach-top, 64px);",
                span { style: "font-size:13px; line-height:1;", "🧭" }
                span { style: "font-size:12px; font-weight:700; color:#ffffff; line-height:1; white-space:nowrap;", "Setup Guide" }
                span { style: "font-size:11px; font-weight:600; color:rgba(255,255,255,0.75); line-height:1; white-space:nowrap;", "{required_completed}/{total_steps}" }
            }
        };
    }

    rsx! {
        aside {
            class: "cf-coach-drawer border shadow-2xl",
            "data-testid": "onboarding-coach-panel",
            style: "--coach-top: var(--coach-top, 64px); border:1px solid rgba(124,58,237,0.45); background:rgba(15,23,42,0.96);",

            div {
                class: "flex items-center justify-between px-3 py-2 border-b",
                style: "border-color:rgba(124,58,237,0.35);",
                div {
                    p { class: "text-sm font-semibold text-violet-200", "Setup Coach" }
                    p { class: "text-xs text-slate-300", "{required_completed}/{total_steps} complete" }
                }
                div { class: "flex items-center gap-1",
                    button {
                        class: "rounded px-2 py-1 text-xs font-medium text-slate-200 hover:bg-slate-800",
                        "data-testid": "onboarding-coach-collapse",
                        onclick: move |_| {
                            collapsed.set(true);
                            store_collapsed(true);
                        },
                        "Minimize"
                    }
                    button {
                        class: "rounded px-2 py-1 text-xs font-medium text-red-200 hover:bg-red-900/40",
                        "data-testid": "onboarding-coach-dismiss",
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

            div { class: "p-3 space-y-2",
                    if let Some(message) = action_error() {
                        div {
                            class: "rounded-md border border-red-500/50 bg-red-900/30 px-2 py-1 text-xs text-red-200",
                            "{message}"
                        }
                    }

                    for step in visible_steps.iter().copied() {
                        {
                            let status = step_status(step, &progress_data);
                            let locked = step_locked(step, &progress_data);
                            rsx! {
                                button {
                                    class: "w-full rounded-lg px-3 py-2 text-left border flex items-start justify-between gap-2",
                                    "data-testid": "onboarding-step-{step.id}",
                                    disabled: locked,
                                    aria_label: if locked { format!("{}: register a system first", step.label) } else { step.label.to_string() },
                                    style: if status.complete {
                                        "border:1px solid rgba(16,185,129,0.6); background:rgba(6,95,70,0.35);"
                                    } else {
                                        "border:1px solid rgba(100,116,139,0.55); background:rgba(30,41,59,0.75);"
                                    },
                                    onclick: move |_| {
                                        if step.setup_context {
                                            store_setup_context();
                                        }
                                        navigator.push(route_for_step(step));
                                    },
                                    div { class: "flex-1 min-w-0", style: "text-align:left;",
                                        p {
                                            class: if status.complete {
                                                "text-sm font-medium text-emerald-200"
                                            } else {
                                                "text-sm font-medium text-slate-200"
                                            },
                                            "{step.label}"
                                        }
                                        p {
                                            class: if status.complete {
                                                "text-[11px] text-emerald-300"
                                            } else {
                                                "text-[11px] text-slate-400"
                                            },
                                            if locked {
                                                "Register a system before reviewing agent deployment"
                                            } else if step.id == "agent" && status.complete {
                                                "Acknowledged"
                                            } else if status.complete {
                                                "Configured"
                                            } else {
                                                "{step.pending}"
                                            }
                                        }
                                    }
                                    if status.complete {
                                        span { class: "text-emerald-300 text-sm shrink-0", "✓" }
                                    } else {
                                        span { class: "text-violet-300 text-sm shrink-0", "→" }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "pt-1 flex items-center justify-between",
                        button {
                            class: "rounded px-2 py-1 text-xs font-medium text-violet-200 hover:bg-violet-900/30",
                            "data-testid": "onboarding-coach-refresh",
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
}
