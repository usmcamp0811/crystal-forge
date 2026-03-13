use dioxus::prelude::*;

use crate::api::client::{
    fetch_setup_wizard_progress, set_setup_wizard_agent_acknowledged, set_setup_wizard_dismissed,
};
use crate::api::models::SetupWizardProgressResponse;
use crate::state::app_state::{AppState, AuthFetchState};
use crate::state::auth;
use crate::theme;

const TOTAL_STEPS: usize = 6;
const AGENT_SNIPPET: &str = r#"services.crystal-forge.client = {
  enable = true;
  serverUrl = 'https://your-crystal-forge.example';
  signingKeyFile = '/var/lib/crystal-forge/signing-key';
};"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum WizardStep {
    Environment,
    Flake,
    Builder,
    Cache,
    System,
    Agent,
}

impl WizardStep {
    fn all() -> [WizardStep; TOTAL_STEPS] {
        [
            WizardStep::Environment,
            WizardStep::Flake,
            WizardStep::Builder,
            WizardStep::Cache,
            WizardStep::System,
            WizardStep::Agent,
        ]
    }

    fn idx(self) -> usize {
        match self {
            WizardStep::Environment => 0,
            WizardStep::Flake => 1,
            WizardStep::Builder => 2,
            WizardStep::Cache => 3,
            WizardStep::System => 4,
            WizardStep::Agent => 5,
        }
    }

    fn title(self) -> &'static str {
        match self {
            WizardStep::Environment => "Create Your First Environment",
            WizardStep::Flake => "Add a Flake",
            WizardStep::Builder => "Register a Builder",
            WizardStep::Cache => "Configure a Cache Destination",
            WizardStep::System => "Register a System",
            WizardStep::Agent => "Deploy the Agent",
        }
    }
}

#[component]
pub fn SetupView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let nav = navigator();

    let mut current_step = use_signal(|| WizardStep::Environment);
    let mut refresh = use_signal(|| 0_u64);

    let progress = use_resource(move || async move {
        let _ = refresh();
        fetch_setup_wizard_progress().await
    });

    let state = app_state.read();
    let auth_context = state.auth.clone();
    let auth_fetch_state = state.auth_fetch_state.clone();
    drop(state);

    if auth_fetch_state == AuthFetchState::Loading {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                p { class: "{theme::text::SECONDARY}", "Loading setup wizard..." }
            }
        };
    }

    if !auth::is_authenticated(&auth_context) {
        nav.push("/login");
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }

    if !auth::is_admin(&auth_context) {
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center p-6 {theme::surface::PAGE_BG}",
                div {
                    class: "max-w-xl rounded-xl border border-amber-500/40 bg-amber-900/20 p-6 space-y-3",
                    h1 { class: "text-xl font-semibold text-amber-100", "Access Denied" }
                    p { class: "text-sm text-amber-200/90", "Setup wizard is only available to administrators." }
                    button {
                        class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| {
                            nav.push("/");
                        },
                        "Go to Dashboard"
                    }
                }
            }
        };
    }

    let maybe_progress = progress.read().clone();
    let progress_data = match maybe_progress {
        Some(Ok(data)) => data,
        Some(Err(_)) => {
            return rsx! {
                div { class: "min-h-screen flex items-center justify-center p-6 {theme::surface::PAGE_BG}",
                    div { class: "max-w-xl rounded-xl border border-red-500/40 bg-red-900/20 p-6 space-y-3",
                        h1 { class: "text-xl font-semibold text-red-200", "Failed to load setup progress" }
                        p { class: "text-sm text-red-100/90", "Please refresh or try again." }
                        button {
                            class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| refresh.set(refresh() + 1),
                            "Retry"
                        }
                    }
                }
            };
        }
        None => {
            return rsx! {
                div { class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                    p { class: "{theme::text::SECONDARY}", "Loading setup progress..." }
                }
            };
        }
    };

    let completed_required = progress_data.all_required_complete;
    let agent_done = progress_data.agent_acknowledged;
    let all_done = completed_required && agent_done;

    if progress_data.dismissed {
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center p-6 {theme::surface::PAGE_BG}",
                div {
                    class: "max-w-xl rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-6 space-y-3",
                    h1 { class: "text-xl font-semibold {theme::text::PRIMARY}", "Setup Wizard Dismissed" }
                    p { class: "text-sm {theme::text::SECONDARY}", "You can re-run setup from Server Management at any time." }
                    div { class: "flex gap-2",
                        button {
                            class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| {
                                nav.push("/");
                            },
                            "Go to Dashboard"
                        }
                        button {
                            class: "rounded-lg px-3 py-2 text-sm font-medium border {theme::surface::CARD_BORDER} {theme::text::PRIMARY}",
                            onclick: move |_| {
                                spawn(async move {
                                    let _ = set_setup_wizard_dismissed(false).await;
                                    refresh.set(refresh() + 1);
                                });
                            },
                            "Re-enable Wizard"
                        }
                    }
                }
            }
        };
    }

    let step = current_step();

    rsx! {
        div {
            class: "min-h-screen {theme::surface::PAGE_BG} p-4 md:p-8",
            div {
                class: "mx-auto w-full max-w-6xl space-y-6",

                header {
                    class: "flex flex-wrap items-center justify-between gap-3",
                    div {
                        h1 { class: "text-2xl font-bold {theme::text::PRIMARY}", "First-Time Admin Setup" }
                        p { class: "text-sm {theme::text::SECONDARY}", "Follow this checklist to configure a working deployment pipeline." }
                    }
                    button {
                        class: "rounded-lg px-3 py-2 text-sm font-medium border {theme::surface::CARD_BORDER} {theme::text::PRIMARY}",
                        onclick: move |_| {
                            spawn(async move {
                                let _ = set_setup_wizard_dismissed(true).await;
                                nav.push("/");
                            });
                        },
                        "Skip Setup"
                    }
                }

                div {
                    class: "rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 md:p-6 space-y-4",
                    SetupProgressBar {
                        current_step: step,
                        progress: progress_data.clone(),
                    }

                    if all_done {
                        CompletionPanel {
                            progress: progress_data.clone(),
                        }
                    } else {
                        StepPanel {
                            step,
                            progress: progress_data.clone(),
                            on_ack_agent: move || {
                                spawn(async move {
                                    let _ = set_setup_wizard_agent_acknowledged(true).await;
                                    refresh.set(refresh() + 1);
                                });
                            },
                            on_refresh: move || refresh.set(refresh() + 1),
                        }
                    }

                    div {
                        class: "flex flex-wrap items-center justify-between gap-2 pt-2",
                        button {
                            class: "rounded-lg px-3 py-2 text-sm font-medium border {theme::surface::CARD_BORDER} {theme::text::PRIMARY}",
                            disabled: step.idx() == 0,
                            onclick: move |_| {
                                if step.idx() > 0 {
                                    current_step.set(WizardStep::all()[step.idx() - 1]);
                                }
                            },
                            "Back"
                        }

                        div { class: "flex gap-2",
                            button {
                                class: "rounded-lg px-3 py-2 text-sm font-medium border {theme::surface::CARD_BORDER} {theme::text::PRIMARY}",
                                onclick: move |_| refresh.set(refresh() + 1),
                                "Refresh Status"
                            }

                            if all_done {
                                button {
                                    class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                                    onclick: move |_| {
                                        spawn(async move {
                                            let _ = set_setup_wizard_dismissed(true).await;
                                            nav.push("/");
                                        });
                                    },
                                    "Get Started"
                                }
                            } else {
                                button {
                                    class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                                    disabled: step.idx() >= TOTAL_STEPS - 1,
                                    onclick: move |_| {
                                        if step.idx() < TOTAL_STEPS - 1 {
                                            current_step.set(WizardStep::all()[step.idx() + 1]);
                                        }
                                    },
                                    "Next"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SetupProgressBar(current_step: WizardStep, progress: SetupWizardProgressResponse) -> Element {
    rsx! {
        div { class: "grid gap-2 md:grid-cols-6",
            for step in WizardStep::all() {
                {
                    let (label, complete) = match step {
                        WizardStep::Environment => ("Environment", progress.environment.complete),
                        WizardStep::Flake => ("Flake", progress.flake.complete),
                        WizardStep::Builder => ("Builder", progress.builder.complete),
                        WizardStep::Cache => ("Cache", progress.cache.complete),
                        WizardStep::System => ("System", progress.system.complete),
                        WizardStep::Agent => ("Agent", progress.agent_acknowledged),
                    };
                    let card_class = if current_step == step {
                        "rounded-lg border px-3 py-2 text-xs md:text-sm border-violet-400 bg-violet-500/10"
                    } else {
                        "rounded-lg border px-3 py-2 text-xs md:text-sm border-slate-700 bg-slate-900/20"
                    };
                    rsx! {
                        div {
                            class: "{card_class}",
                            div { class: "flex items-center justify-between gap-2",
                                span { class: "font-medium {theme::text::PRIMARY}", "{label}" }
                                if complete {
                                    span { class: "text-emerald-400", "✓" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, PartialEq, Clone)]
struct StepPanelProps {
    step: WizardStep,
    progress: SetupWizardProgressResponse,
    on_ack_agent: EventHandler<()>,
    on_refresh: EventHandler<()>,
}

#[component]
fn StepPanel(props: StepPanelProps) -> Element {
    let (description, href, complete, count) = match props.step {
        WizardStep::Environment => (
            "Environments group systems, builders, and caches. Create at least one environment to continue.",
            "/environments",
            props.progress.environment.complete,
            props.progress.environment.count,
        ),
        WizardStep::Flake => (
            "Flakes are your source of NixOS configurations. Add a flake and verify polling is active.",
            "/flakes",
            props.progress.flake.complete,
            props.progress.flake.count,
        ),
        WizardStep::Builder => (
            "Builders execute build jobs. Add a builder and assign it to an environment.",
            "/builders",
            props.progress.builder.complete,
            props.progress.builder.count,
        ),
        WizardStep::Cache => (
            "Caches store build outputs for agents. Add at least one cache destination and assign it to an environment.",
            "/caches",
            props.progress.cache.complete,
            props.progress.cache.count,
        ),
        WizardStep::System => (
            "Systems are managed hosts. Register at least one system with both environment and flake linked.",
            "/systems",
            props.progress.system.complete,
            props.progress.system.count,
        ),
        WizardStep::Agent => (
            "Enable and configure the Crystal Forge agent module on target hosts, then confirm you understand this step.",
            "/systems",
            props.progress.agent_acknowledged,
            if props.progress.agent_acknowledged { 1 } else { 0 },
        ),
    };

    rsx! {
        div { class: "space-y-4",
            h2 { class: "text-xl font-semibold {theme::text::PRIMARY}", "{props.step.title()}" }
            p { class: "text-sm {theme::text::SECONDARY}", "{description}" }

            div { class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-3",
                p { class: "text-sm {theme::text::PRIMARY}",
                    if complete { "Status: Complete" } else { "Status: Incomplete" }
                }
                p { class: "text-xs {theme::text::MUTED}", "Detected count: {count}" }
            }

            if props.step == WizardStep::Agent {
                pre {
                    class: "text-xs rounded-lg border {theme::surface::CARD_BORDER} bg-slate-950 p-3 overflow-auto",
                    "{AGENT_SNIPPET}"
                }

                button {
                    class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                    onclick: move |_| props.on_ack_agent.call(()),
                    "Mark as understood"
                }
            } else {
                a {
                    class: "inline-flex rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                    href: "{href}",
                    "Open related page"
                }
            }

            button {
                class: "block rounded-lg px-3 py-2 text-sm font-medium border {theme::surface::CARD_BORDER} {theme::text::PRIMARY}",
                onclick: move |_| props.on_refresh.call(()),
                "Refresh this step"
            }
        }
    }
}

#[derive(Props, PartialEq, Clone)]
struct CompletionPanelProps {
    progress: SetupWizardProgressResponse,
}

#[component]
fn CompletionPanel(props: CompletionPanelProps) -> Element {
    rsx! {
        div { class: "space-y-2 rounded-lg border border-emerald-500/40 bg-emerald-900/20 p-4",
            h2 { class: "text-lg font-semibold text-emerald-100", "Setup Complete" }
            p { class: "text-sm text-emerald-200/90", "All required setup steps are complete and agent deployment has been acknowledged." }
            ul { class: "grid gap-2 text-sm text-emerald-100 md:grid-cols-2",
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Environments: {}", props.progress.environment.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Flakes: {}", props.progress.flake.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Builders with environment: {}", props.progress.builder.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Caches with environment: {}", props.progress.cache.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Systems linked: {}", props.progress.system.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", "✓ Agent step acknowledged" }
            }
            p { class: "text-sm text-emerald-200/90", "Select 'Get Started' to continue to the dashboard." }
        }
    }
}
