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
    let mut action_error = use_signal(|| None::<String>);

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
                    if let Some(message) = action_error() {
                        p {
                            class: "text-sm text-red-300 rounded-lg border border-red-500/40 bg-red-900/20 px-3 py-2",
                            "{message}"
                        }
                    }
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
                                let mut action_error = action_error;
                                let mut refresh = refresh;
                                spawn(async move {
                                    match set_setup_wizard_dismissed(false).await {
                                        Ok(_) => {
                                            action_error.set(None);
                                            refresh.set(refresh() + 1);
                                        }
                                        Err(err) => action_error
                                            .set(Some(format!("Failed to re-enable setup wizard: {err}"))),
                                    }
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
                            let mut action_error = action_error;
                            let nav = nav.clone();
                            spawn(async move {
                                match set_setup_wizard_dismissed(true).await {
                                    Ok(_) => {
                                        action_error.set(None);
                                        nav.push("/");
                                    }
                                    Err(err) => action_error
                                        .set(Some(format!("Failed to skip setup: {err}"))),
                                }
                            });
                        },
                        "Skip Setup"
                    }
                }

                if let Some(message) = action_error() {
                    div {
                        class: "rounded-lg border border-red-500/40 bg-red-900/20 px-3 py-2 text-sm text-red-300",
                        "{message}"
                    }
                }

                div {
                    class: "rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 md:p-6 space-y-4",
                    SetupProgressBar {
                        current_step: step,
                        progress: progress_data.clone(),
                        on_step_click: move |s| current_step.set(s),
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
                                let mut action_error = action_error;
                                let mut refresh = refresh;
                                spawn(async move {
                                    match set_setup_wizard_agent_acknowledged(true).await {
                                        Ok(_) => {
                                            action_error.set(None);
                                            refresh.set(refresh() + 1);
                                        }
                                        Err(err) => action_error.set(Some(format!(
                                            "Failed to save agent acknowledgment: {err}"
                                        ))),
                                    }
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
                                        let mut action_error = action_error;
                                        let nav = nav.clone();
                                        spawn(async move {
                                            match set_setup_wizard_dismissed(true).await {
                                                Ok(_) => {
                                                    action_error.set(None);
                                                    nav.push("/");
                                                }
                                                Err(err) => action_error.set(Some(format!(
                                                    "Failed to finalize setup state: {err}"
                                                ))),
                                            }
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
fn SetupProgressBar(
    current_step: WizardStep,
    progress: SetupWizardProgressResponse,
    on_step_click: EventHandler<WizardStep>,
) -> Element {
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
                    let is_active = current_step == step;

                    // Use explicit inline styles so colours are guaranteed
                    // visible on the dark #111827 card background regardless
                    // of Tailwind JIT purging or opacity stacking.
                    let card_style = if is_active {
                        "border: 2px solid #7c3aed; background: rgba(109,40,217,0.35); box-shadow: 0 0 0 1px rgba(139,92,246,0.4);"
                    } else if complete {
                        "border: 1px solid rgba(16,185,129,0.6); background: rgba(6,95,70,0.35);"
                    } else {
                        "border: 1px solid rgba(100,116,139,0.5); background: rgba(30,41,59,0.6);"
                    };
                    let label_style = if is_active {
                        "color:#e9d5ff; font-weight:600;"
                    } else if complete {
                        "color:#6ee7b7; font-weight:500;"
                    } else {
                        "color:#94a3b8; font-weight:500;"
                    };

                    rsx! {
                        div {
                            class: "rounded-lg px-3 py-2 cursor-pointer transition-all",
                            style: "{card_style}",
                            role: "button",
                            onclick: move |_| on_step_click.call(step),
                            div { class: "flex items-center justify-between gap-1",
                                span {
                                    class: "truncate text-xs md:text-sm",
                                    style: "{label_style}",
                                    "{label}"
                                }
                                if complete {
                                    span { style: "color:#34d399; font-size:13px; flex-shrink:0;", "✓" }
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
            "Builders execute build jobs. Add at least one builder (no explicit environment assignment means wildcard/all environments).",
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
            if props.progress.agent_acknowledged {
                1
            } else {
                0
            },
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
                // IDE-style code block with syntax highlighting
                div {
                    style: "background:#1e1e1e; border:1px solid #3e3e42; border-radius:8px; overflow:hidden; font-family:'JetBrains Mono','Fira Code','Cascadia Code',Consolas,monospace; font-size:13px; line-height:1.6;",
                    // Title bar
                    div {
                        style: "background:#2d2d30; padding:6px 12px; display:flex; align-items:center; gap:8px; border-bottom:1px solid #3e3e42;",
                        span { style: "width:12px;height:12px;border-radius:50%;background:#ff5f57;display:inline-block;", "" }
                        span { style: "width:12px;height:12px;border-radius:50%;background:#febc2e;display:inline-block;", "" }
                        span { style: "width:12px;height:12px;border-radius:50%;background:#28c840;display:inline-block;", "" }
                        span { style: "margin-left:8px; color:#858585; font-size:11px;", "configuration.nix" }
                    }
                    // Code area
                    div {
                        style: "padding:16px; overflow-x:auto;",
                        // Line 1: services.crystal-forge.client = {
                        div { style: "display:flex; gap:16px;",
                            span { style: "color:#4e4e4e; user-select:none; min-width:16px; text-align:right;", "1" }
                            span {
                                span { style: "color:#9cdcfe;", "services" }
                                span { style: "color:#cccccc;", "." }
                                span { style: "color:#9cdcfe;", "crystal-forge" }
                                span { style: "color:#cccccc;", "." }
                                span { style: "color:#9cdcfe;", "client" }
                                span { style: "color:#cccccc;", " = {{" }
                            }
                        }
                        // Line 2: enable = true;
                        div { style: "display:flex; gap:16px;",
                            span { style: "color:#4e4e4e; user-select:none; min-width:16px; text-align:right;", "2" }
                            span {
                                span { style: "color:#cccccc;", "  " }
                                span { style: "color:#9cdcfe;", "enable" }
                                span { style: "color:#cccccc;", " = " }
                                span { style: "color:#569cd6;", "true" }
                                span { style: "color:#cccccc;", ";" }
                            }
                        }
                        // Line 3: serverUrl = '...';
                        div { style: "display:flex; gap:16px;",
                            span { style: "color:#4e4e4e; user-select:none; min-width:16px; text-align:right;", "3" }
                            span {
                                span { style: "color:#cccccc;", "  " }
                                span { style: "color:#9cdcfe;", "serverUrl" }
                                span { style: "color:#cccccc;", " = " }
                                span { style: "color:#ce9178;", "'https://your-crystal-forge.example'" }
                                span { style: "color:#cccccc;", ";" }
                            }
                        }
                        // Line 4: signingKeyFile = '...';
                        div { style: "display:flex; gap:16px;",
                            span { style: "color:#4e4e4e; user-select:none; min-width:16px; text-align:right;", "4" }
                            span {
                                span { style: "color:#cccccc;", "  " }
                                span { style: "color:#9cdcfe;", "signingKeyFile" }
                                span { style: "color:#cccccc;", " = " }
                                span { style: "color:#ce9178;", "'/var/lib/crystal-forge/signing-key'" }
                                span { style: "color:#cccccc;", ";" }
                            }
                        }
                        // Line 5: };
                        div { style: "display:flex; gap:16px;",
                            span { style: "color:#4e4e4e; user-select:none; min-width:16px; text-align:right;", "5" }
                            span { style: "color:#cccccc;", "}};" }
                        }
                    }
                }

                button {
                    class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                    onclick: move |_| props.on_ack_agent.call(()),
                    "Mark as understood"
                }
            } else {
                button {
                    class: "inline-flex rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                    onclick: move |_| {
                        // Store a flag in localStorage so the destination view
                        // knows the user came from the wizard (Dioxus router
                        // strips query params so ?from=setup doesn't survive).
                        if let Some(storage) = web_sys::window()
                            .and_then(|w| w.local_storage().ok())
                            .flatten()
                        {
                            let _ = storage.set_item("cf.from_setup", "1");
                        }
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_href(href);
                        }
                    },
                    "Open related page →"
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
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Builders: {}", props.progress.builder.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Caches with environment: {}", props.progress.cache.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", {format!("✓ Systems linked: {}", props.progress.system.count)} }
                li { class: "rounded border border-emerald-400/30 bg-emerald-950/30 px-3 py-2", "✓ Agent step acknowledged" }
            }
            p { class: "text-sm text-emerald-200/90", "Select 'Get Started' to continue to the dashboard." }
        }
    }
}
