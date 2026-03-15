use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api::client::{fetch_setup_wizard_progress, set_setup_wizard_dismissed};
use crate::api::models::{SetupWizardProgressResponse, SetupWizardStepStatus};

#[derive(Clone, Copy)]
struct CoachStep {
    id: &'static str,
    label: &'static str,
    href: &'static str,
}

const STEPS: [CoachStep; 6] = [
    CoachStep {
        id: "environment",
        label: "Create environment",
        href: "/environments",
    },
    CoachStep {
        id: "flake",
        label: "Add flake",
        href: "/flakes",
    },
    CoachStep {
        id: "builder",
        label: "Register builder",
        href: "/builders",
    },
    CoachStep {
        id: "cache",
        label: "Configure cache",
        href: "/caches",
    },
    CoachStep {
        id: "system",
        label: "Register system",
        href: "/systems",
    },
    CoachStep {
        id: "agent",
        label: "Deploy agent",
        href: "/systems",
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

fn store_collapsed(value: bool) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("cf.coach.collapsed", if value { "true" } else { "false" });
    }
}

fn route_from_step(step: CoachStep) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        // Existing destination views use this for contextual setup guidance banners.
        let _ = storage.set_item("cf.from_setup", "1");
    }
    if let Some(window) = web_sys::window() {
        let _ = window.location().set_href(step.href);
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
        _ => SetupWizardStepStatus {
            complete: false,
            count: 0,
        },
    }
}

#[component]
pub fn OnboardingCoachPanel() -> Element {
    let mut refresh_tick = use_signal(|| 0_u64);
    let mut collapsed = use_signal(collapsed_from_storage);
    let mut action_error = use_signal(|| None::<String>);

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
                    class: "fixed z-40 top-20 right-4 w-[340px] rounded-xl border p-3",
                    style: "border:1px solid rgba(239,68,68,0.5); background:rgba(127,29,29,0.92);",
                    p { class: "text-sm text-red-100", "Onboarding coach unavailable (failed to load progress)." }
                }
            };
        }
        None => {
            return rsx! {
                aside {
                    class: "fixed z-40 top-20 right-4 w-[340px] rounded-xl border p-3",
                    style: "border:1px solid rgba(100,116,139,0.45); background:rgba(15,23,42,0.94);",
                    p { class: "text-sm text-slate-200", "Loading onboarding coach..." }
                }
            };
        }
    };

    // Respect persisted dismissal and hide automatically once fully complete.
    if progress_data.dismissed
        || (progress_data.all_required_complete && progress_data.agent_acknowledged)
    {
        return rsx! {};
    }

    let required_completed = [
        progress_data.environment.complete,
        progress_data.flake.complete,
        progress_data.builder.complete,
        progress_data.cache.complete,
        progress_data.system.complete,
        progress_data.agent_acknowledged,
    ]
    .into_iter()
    .filter(|v| *v)
    .count();

    // Minimized: compact tab anchored to the top-right just below the top bar
    if collapsed() {
        return rsx! {
            button {
                "data-testid": "onboarding-coach-panel",
                onclick: move |_| {
                    collapsed.set(false);
                    store_collapsed(false);
                },
                style: "position:fixed; top:64px; right:0; z-index:40; display:flex; align-items:center; gap:6px; padding:6px 12px 6px 10px; border-radius:0 0 0 8px; background:rgba(15,23,42,0.96); border:1px solid rgba(96,165,250,0.5); border-right:none; border-top:none; box-shadow:0 4px 12px rgba(15,23,42,0.5); cursor:pointer;",
                span { style: "font-size:13px; line-height:1;", "🧭" }
                span { style: "font-size:12px; font-weight:600; color:#bfdbfe; line-height:1; white-space:nowrap;", "Setup Guide" }
                span { style: "font-size:11px; color:#64748b; line-height:1; white-space:nowrap;", "{required_completed}/6" }
            }
        };
    }

    rsx! {
        aside {
            class: "fixed z-40 top-20 right-4 rounded-xl border shadow-2xl max-sm:top-auto max-sm:bottom-4 max-sm:left-4 max-sm:right-4",
            "data-testid": "onboarding-coach-panel",
            style: "width:min(280px, calc(100vw - 2rem)); border:1px solid rgba(124,58,237,0.45); background:rgba(15,23,42,0.96);",

            div {
                class: "flex items-center justify-between px-3 py-2 border-b",
                style: "border-color:rgba(124,58,237,0.35);",
                div {
                    p { class: "text-sm font-semibold text-violet-200", "Setup Coach" }
                    p { class: "text-xs text-slate-300", "{required_completed}/6 complete" }
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

                    for step in STEPS {
                        {
                            let status = step_status(step, &progress_data);
                            rsx! {
                                button {
                                    class: "w-full rounded-lg px-3 py-2 text-left border flex items-start justify-between gap-2",
                                    "data-testid": "onboarding-step-{step.id}",
                                    style: if status.complete {
                                        "border:1px solid rgba(16,185,129,0.6); background:rgba(6,95,70,0.35);"
                                    } else {
                                        "border:1px solid rgba(100,116,139,0.55); background:rgba(30,41,59,0.75);"
                                    },
                                    onclick: move |_| route_from_step(step),
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
                                            if step.id == "agent" {
                                                if status.complete {
                                                    "Acknowledged"
                                                } else {
                                                    "Completes after first system setup"
                                                }
                                            } else if status.complete {
                                                "Configured"
                                            } else {
                                                "Pending"
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


