//! Crystal Forge Coach: setup track, security workflow track and Guide button.
//!
//! The coach has two tracks in one surface.
//!
//! - **Setup** asks whether Crystal Forge is configured. It has nine steps and
//!   is for Administrators. The server derives completion from persisted
//!   resources. The browser never completes a step.
//! - **Security workflows** asks whether a person knows how to operate it. It
//!   has five walkthrough modules for every role. Progress is presentation
//!   state in this browser and never claims a security, compliance or
//!   remediation state.
//!
//! # Role and traffic
//!
//! The role comes from the authenticated session. Only Administrators mount
//! [`SetupProgressSource`], so Operators and Viewers never call the
//! Administrator-only `GET /api/v1/admin/setup-progress` endpoint.
//!
//! # Authority
//!
//! The authoritative design is `docs/design/CrystalForge/components/SetupCoach.jsx`
//! and `CoachTours.jsx`. The coach navigates and opens read-only surfaces. It
//! never submits a mutation. See [`super::runner`].

use dioxus::prelude::*;
use dioxus_router::Navigator;
use gloo_timers::future::TimeoutFuture;

use super::figures::CoachFigure;
use super::layout::{
    self, DOCK_CARD_HEIGHT, Dock, DockChoice, FULL_CARD_HEIGHT, Place, choose_dock,
    choose_narrow_dock, choose_place,
};
use super::runner::{
    TourStatus, find_target_rect, measure_viewport, use_surroundings, use_tour_runner,
};
use super::setup::{self, SetupDestination, SetupStep, route_for_step};
use super::state::{CoachController, Panel, SetupSnapshot, Track, save_ui, store_setup_context};
use super::tours::{
    self, CoachRole, ModuleState, SECURITY_MODULES, SecurityModule, Stop, StopAccess,
};
use crate::api::client::{
    fetch_setup_wizard_progress, fetch_systems, set_setup_wizard_agent_acknowledged,
    set_setup_wizard_dismissed,
};
use crate::api::models::SystemsListParams;
use crate::components::icon::{Icon, IconName};
use crate::routes::Route;

/// Interval between setup progress reads while the shell is open.
const SETUP_POLL_MS: u32 = 8000;

fn step_icon(name: &str) -> IconName {
    match name {
        "env" => IconName::Env,
        "git" => IconName::Git,
        "cpu" => IconName::Cpu,
        "cube" => IconName::Cube,
        "server" => IconName::Server,
        "deploy" => IconName::Deploy,
        "file" => IconName::File,
        "activity" => IconName::Activity,
        _ => IconName::Shield,
    }
}

/// Draws the Crystal Forge hexagon mark.
#[component]
fn CoachMark(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        svg { width: "{size}", height: "{size}", view_box: "0 0 24 24", "aria-hidden": "true",
            path { d: "M12 2.5 20.5 7v10L12 21.5 3.5 17V7L12 2.5Z", fill: "none", stroke: "currentColor", stroke_width: "1.6" }
            path { d: "M12 7.2 16.3 9.6v4.8L12 16.8 7.7 14.4V9.6L12 7.2Z", fill: "currentColor", opacity: "0.85" }
        }
    }
}

/// Reads Administrator-only setup progress and shares it with the coach.
///
/// Mounted only for Administrators. This component is the only caller of the
/// setup progress endpoint.
#[component]
fn SetupProgressSource() -> Element {
    let ctrl = use_context::<CoachController>();
    let mut applied_first_run = use_signal(|| false);

    // Poll so completion is live while the person configures other pages.
    use_future(move || async move {
        let mut refresh = ctrl.refresh;
        loop {
            TimeoutFuture::new(SETUP_POLL_MS).await;
            refresh.set(refresh() + 1);
        }
    });

    let progress = use_resource(move || async move {
        let _ = (ctrl.refresh)();
        fetch_setup_wizard_progress().await
    });

    // A signed heartbeat is only a prerequisite for Administrator acknowledgement.
    // It never completes the setup step by itself.
    let systems = use_resource(move || async move {
        let _ = (ctrl.refresh)();
        fetch_systems(&SystemsListParams {
            per_page: Some(50),
            ..SystemsListParams::default()
        })
        .await
    });

    use_effect(move || {
        if let Some(Ok(page)) = systems.read().as_ref() {
            let mut reported = ctrl.agent_reported;
            reported.set(page.items.iter().any(|system| system.last_seen.is_some()));
        }
    });

    use_effect(move || {
        let mut setup = ctrl.setup;
        match progress.read().as_ref() {
            Some(Ok(value)) => {
                // First run on this browser: respect a server-side dismissal
                // from another browser by starting closed. The Guide reopens it.
                if !applied_first_run() {
                    applied_first_run.set(true);
                    if !ctrl.had_stored_state && value.dismissed {
                        ctrl.set_panel(Panel::Dismissed);
                    }
                }
                setup.set(SetupSnapshot::Loaded(value.clone()));
            }
            Some(Err(_)) => setup.set(SetupSnapshot::Unavailable),
            None => {
                // Keep the previous snapshot while a refresh is pending.
                if matches!(*setup.peek(), SetupSnapshot::NotApplicable) {
                    setup.set(SetupSnapshot::Loading);
                }
            }
        }
    });

    rsx! {}
}

/// Mounts the coach for an authenticated session.
///
/// Renders the setup source (Administrators only), the runner, the active
/// panel, pill or walkthrough card, and the spotlight.
#[component]
pub fn CoachRoot() -> Element {
    let ctrl = use_context::<CoachController>();
    use_tour_runner(ctrl);
    use_surroundings(ctrl);

    // Persist presentation state only.
    use_effect(move || {
        let ui = ctrl.ui.read().clone();
        save_ui(&ui);
    });

    // Other roles must not keep a snapshot from a previous session.
    use_effect(move || {
        if (ctrl.role)() != Some(CoachRole::Admin) {
            let mut setup = ctrl.setup;
            if !matches!(*setup.peek(), SetupSnapshot::NotApplicable) {
                setup.set(SetupSnapshot::NotApplicable);
            }
        }
    });

    let Some(role) = (ctrl.role)() else {
        return rsx! {};
    };

    let surroundings = (ctrl.surroundings)();
    let viewport = measure_viewport();
    let narrow = layout::is_narrow(viewport.width);
    let overlay = surroundings.overlay_open;
    // A drawer/modal owns the working surface. On a narrow viewport, use the
    // collision-aware compact dock while the overlay is open so its primary
    // controls remain exposed. Use the bottom sheet only on an unobstructed
    // page.
    let sheet = narrow && !overlay;
    let compact = overlay || narrow;
    let panel = ctrl.ui.read().panel;
    let active = (ctrl.active)();
    let stop = active
        .and_then(|tour| tours::module(tour.module).map(|module| (module, tour.index)))
        .and_then(|(module, index)| module.stops.get(index).map(|stop| (module, index, stop)));

    let source = if role == CoachRole::Admin {
        rsx! { SetupProgressSource {} }
    } else {
        rsx! {}
    };

    if panel == Panel::Dismissed {
        return source;
    }

    let body = if let Some((module, index, stop)) = stop {
        if panel == Panel::Minimized {
            rsx! { CoachPill { tour: Some((module, index, stop)), docked: compact } CoachSpotlight {} }
        } else {
            // The key remounts the card per stop so Details/Less starts closed.
            rsx! {
                CoachTourCard { key: "{stop.id}", module: module, index: index, stop: stop, compact: compact, sheet: sheet }
                CoachSpotlight {}
            }
        }
    } else if panel == Panel::Minimized || overlay {
        rsx! { CoachPill { tour: None, docked: compact } CoachBubble {} }
    } else {
        rsx! { CoachPanel { sheet: sheet } }
    };

    rsx! { {source} {body} }
}

/// Points from a minimized Setup Coach to its current page action.
#[component]
fn CoachBubble() -> Element {
    let ctrl = use_context::<CoachController>();
    let route = use_route::<Route>();
    let destination = setup_destination_for_route(&route);
    let snapshot = (ctrl.setup)();
    let step = snapshot
        .progress()
        .zip(destination)
        .and_then(|(progress, destination)| setup::callout_step(progress, destination))
        .map(|(_, step)| step);
    let target = step.and_then(|step| step.target);
    let page_key = destination.map(setup_page_key).unwrap_or_default();
    let mut position = use_signal(|| None::<(f64, f64)>);

    use_future(move || async move {
        loop {
            let hidden = ctrl
                .ui
                .read()
                .callout_hidden
                .get(page_key)
                .copied()
                .unwrap_or(false);
            if ctrl.is_admin()
                && (ctrl.active)().is_none()
                && ctrl.ui.read().panel == Panel::Minimized
                && !(ctrl.surroundings)().overlay_open
                && !hidden
                && let Some(target) = target
                && let Some(rect) = find_target_rect(&[target])
            {
                let width = web_sys::window()
                    .and_then(|window| window.inner_width().ok())
                    .and_then(|value| value.as_f64())
                    .unwrap_or(1280.0);
                let x = (rect.left + rect.width / 2.0).min(width - 150.0);
                position.set(Some((rect.top + rect.height + 12.0, x)));
            } else {
                position.set(None);
            }
            TimeoutFuture::new(600).await;
        }
    });

    let Some(step) = step else {
        return rsx! {};
    };
    let Some(action) = step.action else {
        return rsx! {};
    };
    let Some((top, left)) = position() else {
        return rsx! {};
    };
    rsx! {
        div { class: "coach-bubble", "data-testid": "onboarding-coach-action-bubble", style: "top:{top}px;left:{left}px",
            span { class: "coach-bubble-arrow" }
            span { class: "coach-bubble-eyebrow", "Setup · next action" }
            span { class: "coach-bubble-text", "Click " strong { "{action}" } }
        }
    }
}

/// Draws the spotlight ring around the active target.
#[component]
fn CoachSpotlight() -> Element {
    let ctrl = use_context::<CoachController>();
    let runtime = (ctrl.runtime)();
    let Some(rect) = runtime.rect else {
        return rsx! {};
    };
    let pad = 6.0;
    let (top, left, width, height) = (
        rect.top - pad,
        rect.left - pad,
        rect.width + pad * 2.0,
        rect.height + pad * 2.0,
    );
    rsx! {
        div {
            class: "coach-spot",
            "data-testid": "coach-spot",
            style: "top:{top}px;left:{left}px;width:{width}px;height:{height}px",
            aria_hidden: "true",
        }
    }
}

/// Draws the minimized coach.
#[component]
fn CoachPill(
    tour: Option<(&'static SecurityModule, usize, &'static Stop)>,
    docked: bool,
) -> Element {
    let ctrl = use_context::<CoachController>();
    let snapshot = (ctrl.setup)();
    let setup_open = ctrl.is_admin() && snapshot.setup_open();
    let (count, total) = (snapshot.count(), snapshot.total());
    let percent = match tour {
        Some((module, index, _)) => (index + 1) * 100 / module.stops.len().max(1),
        None if setup_open => count * 100 / total.max(1),
        None => 100,
    };
    let label = match tour {
        Some((_, _, stop)) => format!("Open walkthrough: {}", stop.title),
        None if setup_open => format!("Open Setup Coach, {count} of {total} complete"),
        None => "Open the Guide".to_string(),
    };
    rsx! {
        button {
            class: if docked { "coach-pill focus-ring docked" } else { "coach-pill focus-ring" },
            "data-testid": "onboarding-coach-panel",
            "data-coach-pill": "true",
            title: "Open the Coach",
            aria_label: "{label}",
            onclick: move |_| ctrl.set_panel(Panel::Expanded),
            span { class: "coach-pill-ring", style: "--p:{percent}%", CoachMark { size: 15 } }
            span { class: "coach-pill-text",
                match tour {
                    Some((module, index, stop)) => rsx! {
                        strong { "Walkthrough {index + 1}/{module.stops.len()}" }
                        span { "{stop.title}" }
                    },
                    None if setup_open => rsx! {
                        strong { "Setup" }
                        span { "{count}/{total} reported complete" }
                    },
                    None => rsx! {
                        strong { "Guide" }
                        span { "Security walkthroughs" }
                    },
                }
            }
        }
    }
}

/// Draws the expanded two-track panel.
#[component]
fn CoachPanel(sheet: bool) -> Element {
    let ctrl = use_context::<CoachController>();
    let role = ctrl.role();
    let admin = ctrl.is_admin();
    let track = ctrl.track();
    let snapshot = (ctrl.setup)();
    let (count, total) = (snapshot.count(), snapshot.total());
    let mut action_error = use_signal(|| None::<String>);

    let subtitle = match track {
        Track::Setup if snapshot.all_done() => "All setup complete".to_string(),
        Track::Setup => format!("{count} of {total} reported complete"),
        Track::Security => "Guided security workflows".to_string(),
    };

    rsx! {
        div {
            class: if sheet { "coach coach-sheet" } else { "coach" },
            role: "complementary",
            aria_label: "Crystal Forge Coach",
            aria_describedby: "setup-coach-progress-status",
            "data-testid": "onboarding-coach-panel",
            div { class: "coach-head",
                div { class: "coach-head-title",
                    span { class: "coach-head-mark", CoachMark { size: 17 } }
                    div { style: "min-width:0",
                        strong { "Crystal Forge Coach" }
                        div { id: "setup-coach-progress-status", class: "coach-head-sub", role: "status", aria_live: "polite", aria_atomic: "true", "{subtitle}" }
                    }
                }
                div { class: "coach-head-actions",
                    button {
                        class: "coach-link focus-ring",
                        "data-testid": "onboarding-coach-collapse",
                        aria_label: "Minimize Crystal Forge Coach",
                        onclick: move |_| ctrl.set_panel(Panel::Minimized),
                        "Minimize"
                    }
                    button {
                        class: "coach-link focus-ring",
                        "data-testid": "onboarding-coach-dismiss",
                        aria_label: "Close Crystal Forge Coach",
                        title: "Reopen from Guide in the top bar",
                        onclick: move |_| {
                            // Closing the Setup track also records the dismissal on the
                            // server, as before. A failed write keeps the panel open.
                            if admin && track == Track::Setup && snapshot.setup_open() {
                                spawn(async move {
                                    match set_setup_wizard_dismissed(true).await {
                                        Ok(_) => {
                                            action_error.set(None);
                                            ctrl.set_panel(Panel::Dismissed);
                                        }
                                        Err(error) => action_error
                                            .set(Some(format!("Failed to dismiss onboarding coach: {error}"))),
                                    }
                                });
                            } else {
                                ctrl.set_panel(Panel::Dismissed);
                            }
                        },
                        "Close"
                    }
                }
            }
            div { class: "coach-tabs", role: "tablist",
                if admin {
                    button {
                        role: "tab",
                        "data-testid": "coach-tab-setup",
                        aria_selected: "{track == Track::Setup}",
                        class: if track == Track::Setup { "active" } else { "" },
                        onclick: move |_| ctrl.set_track(Track::Setup),
                        "Setup "
                        span { class: "mono", "{count}/{total}" }
                    }
                }
                button {
                    role: "tab",
                    "data-testid": "coach-tab-security",
                    aria_selected: "{track == Track::Security}",
                    class: if track == Track::Security { "active" } else { "" },
                    onclick: move |_| ctrl.set_track(Track::Security),
                    "Security workflows"
                }
                span { class: "coach-role", "data-testid": "coach-role", title: "Walkthroughs adapt to your role", "{role.label()}" }
            }
            if let Some(message) = action_error() {
                div { role: "alert", class: "coach-error", "{message}" }
            }
            match track {
                Track::Setup => rsx! { CoachSetupTrack {} },
                Track::Security => rsx! { CoachSecurityHome {} },
            }
        }
    }
}

/// Draws the nine setup steps from server-reported progress.
#[component]
fn CoachSetupTrack() -> Element {
    let ctrl = use_context::<CoachController>();
    let navigator = use_navigator();
    let snapshot = (ctrl.setup)();
    let mut acknowledgement_error = use_signal(|| None::<String>);

    let progress = match &snapshot {
        SetupSnapshot::Loaded(progress) => progress.clone(),
        SetupSnapshot::Unavailable => {
            return rsx! {
                div { class: "coach-state", role: "alert",
                    p { "Onboarding coach unavailable because progress could not be loaded." }
                    button {
                        class: "coach-link focus-ring",
                        "data-testid": "onboarding-coach-refresh",
                        aria_label: "Refresh Setup Coach progress",
                        onclick: move |_| { let mut refresh = ctrl.refresh; refresh.set(refresh() + 1); },
                        "Refresh"
                    }
                }
            };
        }
        _ => {
            return rsx! {
                div { class: "coach-state", role: "status", aria_live: "polite",
                    p { "Loading onboarding coach..." }
                }
            };
        }
    };

    let steps = setup::visible_steps(&progress);
    let all_done = setup::all_complete(&progress);
    let current = setup::current_step_id(steps, &progress);
    let total = steps.len();
    let completed = setup::completed_count(&progress);
    let percent = completed * 100 / total.max(1);

    rsx! {
        if all_done {
            div { class: "coach-done-card", "data-testid": "coach-done-card",
                div { class: "coach-done-icon", Icon { name: IconName::Check, size: 16 } }
                div { style: "min-width:0",
                    strong { "All setup complete" }
                    p { "The server reports all nine setup steps complete. The Guide stays in the top bar for everyone." }
                    button {
                        class: "btn btn-primary focus-ring xs",
                        "data-testid": "coach-explore-security",
                        onclick: move |_| ctrl.set_track(Track::Security),
                        Icon { name: IconName::Shield, size: 12 }
                        " Explore security workflows"
                    }
                }
            }
        } else {
            div { class: "coach-progress", role: "progressbar", aria_label: "Setup progress", aria_valuemin: "0", aria_valuemax: "100", aria_valuenow: "{percent}",
                for step in steps.iter().copied() {
                    {
                        let done = setup::step_status(step, &progress).complete;
                        let is_current = current == Some(step.id);
                        rsx! { span { class: if done { "coach-progress-seg done" } else if is_current { "coach-progress-seg current" } else { "coach-progress-seg" } } }
                    }
                }
            }
        }
        div { class: "coach-steps",
            for (index, step) in steps.iter().copied().enumerate() {
                {
                    let done = setup::step_status(step, &progress).complete;
                    let locked = setup::step_locked(step, &progress);
                    let is_current = current == Some(step.id);
                    let state = if done { "done" } else if locked { "locked" } else if is_current { "current" } else { "pending" };
                    let position = index + 1;
                    let status_text = if done && step.id == "agent" {
                        "Acknowledged"
                    } else if done {
                        "Reported complete"
                    } else if locked {
                        "Register a system first"
                    } else if step.id == "agent" && (ctrl.agent_reported)() {
                        "First signed report received · acknowledge to finish"
                    } else if step.id == "agent" {
                        "Waiting for the agent's first signed report"
                    } else if is_current {
                        step.short
                    } else {
                        "Not configured"
                    };
                    let label = if locked {
                        format!("Step {position}, {}: locked until a system is registered", step.label)
                    } else {
                        format!("Step {position}, {}: {state}", step.label)
                    };
                    rsx! {
                        button {
                            class: "coach-step coach-step-{state} focus-ring",
                            "data-testid": "onboarding-step-{step.id}",
                            disabled: locked,
                            aria_current: if is_current { "step" } else { "false" },
                            aria_label: "{label}",
                            onclick: move |_| open_setup_step(navigator, step),
                            span { class: "coach-step-rail",
                                span { class: "coach-step-node",
                                    if done { Icon { name: IconName::Check, size: 13 } }
                                    else if locked { Icon { name: IconName::Key, size: 11 } }
                                    else { span { class: "coach-step-num", "{position}" } }
                                }
                                if position < total { span { class: "coach-step-line" } }
                            }
                            span { class: "coach-step-body",
                                span { class: "coach-step-title", Icon { name: step_icon(step.icon), size: 13 } "{step.label}" }
                                span { class: "coach-step-status", "{status_text}" }
                if step.id == "agent" && !done && !locked && (ctrl.agent_reported)() {
                                    button {
                                        class: "btn btn-primary focus-ring xs",
                                        "data-testid": "onboarding-agent-acknowledge",
                                        style: "margin-top:6px;width:fit-content",
                                        onclick: move |event| {
                                            event.stop_propagation();
                                            spawn(async move {
                                                match set_setup_wizard_agent_acknowledged(true).await {
                                                    Ok(_) => { let mut refresh = ctrl.refresh; refresh.set(refresh() + 1); }
                                                    Err(error) => acknowledgement_error.set(Some(format!("Failed to acknowledge agent setup: {error}"))),
                                                }
                                            });
                                        },
                                        Icon { name: IconName::Check, size: 12 }
                                        " Acknowledge agent setup"
                                    }
                                }
                            }
                            span { class: "coach-step-aff", aria_hidden: "true",
                                if done { span { class: "coach-step-tick", "✓" } }
                                else if !locked { Icon { name: IconName::ChevronRight, size: 15 } }
                            }
                        }
                    }
                }
            }
        }
        if let Some(error) = acknowledgement_error() {
            div { class: "coach-error", role: "alert", "{error}" }
        }
        div { class: "coach-foot",
            span { class: "coach-foot-note",
                "Completion is reported by the server from saved resources. Opening a page never completes a step. Reopen from "
                strong { "Guide" }
                " or "
                strong { "Server Management" }
                ". "
                button {
                    class: "coach-inline-link",
                    "data-testid": "onboarding-coach-refresh",
                    aria_label: "Refresh Setup Coach progress",
                    onclick: move |_| { let mut refresh = ctrl.refresh; refresh.set(refresh() + 1); },
                    "Refresh"
                }
            }
        }
    }
}

/// Opens the page for a setup step. Navigation never completes the step.
fn open_setup_step(navigator: Navigator, step: SetupStep) {
    if step.setup_context {
        store_setup_context();
    }
    let route: Route = route_for_step(step);
    navigator.push(route);
}

fn setup_destination_for_route(route: &Route) -> Option<SetupDestination> {
    match route {
        Route::EnvironmentsView { .. } => Some(SetupDestination::Environments),
        Route::FlakesView { .. } => Some(SetupDestination::Flakes),
        Route::BuildersView {} => Some(SetupDestination::Builders),
        Route::CachesView {} => Some(SetupDestination::Caches),
        Route::SystemsView { .. } | Route::SystemDetailView { .. } => {
            Some(SetupDestination::Systems)
        }
        Route::PoliciesView {} => Some(SetupDestination::Policies),
        Route::ComplianceView { .. } => Some(SetupDestination::Compliance),
        Route::PoamsView { .. } => Some(SetupDestination::Poams),
        _ => None,
    }
}

fn setup_page_key(destination: SetupDestination) -> &'static str {
    match destination {
        SetupDestination::Environments => "environments",
        SetupDestination::Flakes => "flakes",
        SetupDestination::Builders => "builders",
        SetupDestination::Caches => "caches",
        SetupDestination::Systems => "systems",
        SetupDestination::Policies => "policies",
        SetupDestination::Compliance => "compliance",
        SetupDestination::Poams => "poams",
    }
}

fn setup_callout_test_id(destination: SetupDestination) -> &'static str {
    match destination {
        SetupDestination::Environments => "setup-coach-environments-callout",
        SetupDestination::Flakes => "setup-coach-flakes-callout",
        SetupDestination::Builders => "setup-coach-builders-callout",
        SetupDestination::Caches => "setup-coach-caches-callout",
        SetupDestination::Systems => "setup-coach-systems-callout",
        SetupDestination::Policies => "setup-coach-policies-callout",
        SetupDestination::Compliance => "setup-coach-compliance-callout",
        SetupDestination::Poams => "setup-coach-poams-callout",
    }
}

/// Shows the current server-reported Setup step on its destination page.
///
/// This is a non-modal callout. Hiding it changes only browser presentation
/// state. It does not complete a step or dismiss the Setup Coach.
#[component]
pub fn CoachCallout() -> Element {
    let Some(ctrl) = try_use_context::<CoachController>() else {
        return rsx! {};
    };
    let route = use_route::<Route>();
    if !ctrl.is_admin() || (ctrl.active)().is_some() || ctrl.ui.read().panel == Panel::Dismissed {
        return rsx! {};
    }
    let Some(destination) = setup_destination_for_route(&route) else {
        return rsx! {};
    };
    let snapshot = (ctrl.setup)();
    let Some(progress) = snapshot.progress() else {
        return rsx! {};
    };
    let Some((step_number, step)) = setup::callout_step(progress, destination) else {
        return rsx! {};
    };
    let page_key = setup_page_key(destination);
    if ctrl
        .ui
        .read()
        .callout_hidden
        .get(page_key)
        .copied()
        .unwrap_or(false)
    {
        return rsx! {};
    }
    let agent_step = step.id == "agent";
    let agent_reported = (ctrl.agent_reported)();
    let mut acknowledgement_error = use_signal(|| None::<String>);
    let viewport = measure_viewport();
    let margin = if ctrl.ui.read().panel == Panel::Expanded && !layout::is_narrow(viewport.width) {
        "min(360px, 42vw)"
    } else {
        "0"
    };
    let callout_test_id = setup_callout_test_id(destination);

    rsx! {
        div {
            class: "coach-callout",
            role: "status",
            "data-testid": "{callout_test_id}",
            style: "margin-right:{margin}",
            div { class: "coach-callout-rail" }
            div { class: "coach-callout-icon", Icon { name: step_icon(step.icon), size: 20 } }
            div { class: "coach-callout-body",
                div { class: "coach-callout-eyebrow", "Setup · Step {step_number} of {setup::visible_steps(progress).len()}" }
                div { class: "coach-callout-title", "{step.label}" }
                div { class: "coach-callout-blurb", "{step.blurb}" }
                if agent_step && agent_reported {
                    div { class: "coach-callout-hint", Icon { name: IconName::Check, size: 12 } "The first signed report arrived. Acknowledge the agent to finish this step." }
                } else if agent_step {
                    div { class: "coach-callout-hint", Icon { name: IconName::Clock, size: 12 } "Waiting for the agent's first signed report. Nothing to click yet." }
                } else {
                    div { class: "coach-callout-hint",
                        Icon { name: IconName::ArrowRight, size: 12 }
                        if let Some(action) = step.action { "Use " strong { "{action}" } ". " }
                        "Crystal Forge marks this complete when {step.complete_when}."
                    }
                }
                if let Some(error) = acknowledgement_error() {
                    div { role: "alert", class: "coach-error", "{error}" }
                }
            }
            div { class: "coach-callout-actions",
                if agent_step && agent_reported {
                    button {
                        class: "btn btn-primary focus-ring xs",
                        "data-testid": "onboarding-agent-acknowledge-callout",
                        onclick: move |_| {
                            spawn(async move {
                                match set_setup_wizard_agent_acknowledged(true).await {
                                    Ok(_) => { let mut refresh = ctrl.refresh; refresh.set(refresh() + 1); }
                                    Err(error) => acknowledgement_error.set(Some(format!("Failed to acknowledge agent setup: {error}"))),
                                }
                            });
                        },
                        Icon { name: IconName::Check, size: 12 }
                        " Acknowledge agent setup"
                    }
                }
                button {
                    class: "coach-link focus-ring",
                    "data-testid": "onboarding-coach-callout-hide",
                    onclick: move |_| ctrl.hide_callout(page_key),
                    "Hide"
                }
            }
        }
    }
}

/// Draws the five security walkthrough modules.
#[component]
fn CoachSecurityHome() -> Element {
    let ctrl = use_context::<CoachController>();
    let role = ctrl.role();
    let progress = ctrl.ui.read().progress.clone();

    rsx! {
        div { class: "coach-mods",
            for module in SECURITY_MODULES.iter() {
                {
                    let state = tours::module_state(module, &progress);
                    let read_only = module.read_only_stops(role);
                    let key = module.key;
                    let resume = tours::resume_index(module, &progress);
                    let (class, pill) = match state {
                        ModuleState::Completed => ("done", "Walkthrough completed".to_string()),
                        ModuleState::InProgress { seen, total } => ("progress", format!("In progress · {seen} of {total} viewed")),
                        ModuleState::NotStarted => ("none", "Not started".to_string()),
                    };
                    rsx! {
                        div { class: "coach-mod coach-mod-{class}", "data-testid": "coach-module-{key}",
                            div { class: "coach-mod-top",
                                span { class: "coach-mod-key mono", "{key}" }
                                strong { "{module.title}" }
                            }
                            p { "{module.purpose}" }
                            div { class: "coach-mod-meta",
                                span { "{module.stops.len()} stops" }
                                if read_only > 0 {
                                    span { class: "coach-mod-gate", title: "These stops explain the action without opening it", "{read_only} read-only for {role.label()}" }
                                }
                                span { class: "coach-mod-state s-{class}", "data-testid": "coach-module-state-{key}", "{pill}" }
                                span { style: "flex:1" }
                                match state {
                                    ModuleState::Completed => rsx! {
                                        button { class: "btn btn-ghost focus-ring xs", "data-testid": "coach-module-start-{key}", onclick: move |_| ctrl.start_tour(key, 0), "Restart" }
                                    },
                                    ModuleState::InProgress { .. } => rsx! {
                                        button { class: "btn btn-primary focus-ring xs", "data-testid": "coach-module-start-{key}", onclick: move |_| ctrl.start_tour(key, resume), "Resume" }
                                    },
                                    ModuleState::NotStarted => rsx! {
                                        button { class: "btn btn-primary focus-ring xs", "data-testid": "coach-module-start-{key}", onclick: move |_| ctrl.start_tour(key, 0), "Start" }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
        div { class: "coach-foot",
            span { class: "coach-foot-note",
                "Walkthrough progress is kept in this browser and records only what you've viewed. It never reflects the state of a finding, scan or POA&M. "
                button {
                    class: "coach-inline-link",
                    "data-testid": "coach-restart-walkthroughs",
                    onclick: move |_| ctrl.restart_walkthroughs(),
                    "Restart walkthroughs"
                }
            }
        }
    }
}

/// Returns the administrator note adapted to the role.
///
/// Below Administrator the note names the permission instead of the person.
fn admin_note_for(note: &str, role: CoachRole) -> String {
    if role == CoachRole::Admin {
        note.to_string()
    } else {
        note.replace(
            " are administrator actions",
            " require Administrator permission",
        )
        .replace(
            " is an administrator action",
            " requires Administrator permission",
        )
    }
}

/// Draws the walkthrough card for one stop.
#[component]
fn CoachTourCard(
    module: &'static SecurityModule,
    index: usize,
    stop: &'static Stop,
    compact: bool,
    sheet: bool,
) -> Element {
    let ctrl = use_context::<CoachController>();
    let role = ctrl.role();
    let mut more = use_signal(|| false);
    let runtime = (ctrl.runtime)();
    // Read the current viewport directly. The shared surroundings signal also
    // tracks drawers and provides a resize fallback, but a viewport may change
    // before its next observer tick.
    let viewport = measure_viewport();
    let count = module.stops.len();
    let access = stop.access(role);
    let viewed = ctrl
        .ui
        .read()
        .progress
        .get(module.key)
        .cloned()
        .unwrap_or_default();

    let live_target = find_target_rect(stop.target_for(role));
    // These CVE triage surfaces put Apply in the bottom action row. On a
    // narrow viewport, reserve a 112px dock below that row instead of using a
    // corner whose modal overlap could cover the action while content reflows.
    let narrow_triage_dock =
        layout::is_narrow(viewport.width) && matches!(stop.id, "B2" | "B3" | "B5");
    let dock = if compact && !sheet {
        if narrow_triage_dock {
            DockChoice {
                dock: Dock::BottomLeft,
                max_height: Some(112.0),
            }
        } else {
            live_target
                .map(|rect| {
                    // `compact` already reflects the parent overlay state. Do not
                    // re-read that signal here: it can update between parent and
                    // child render and select a corner that overlaps the modal.
                    if sheet || layout::is_narrow(viewport.width) {
                        choose_narrow_dock(rect, viewport)
                    } else {
                        choose_dock(rect, viewport, DOCK_CARD_HEIGHT)
                    }
                })
                .unwrap_or(DockChoice {
                    dock: Dock::BottomLeft,
                    max_height: None,
                })
        }
    } else {
        DockChoice {
            dock: Dock::BottomLeft,
            max_height: None,
        }
    };
    let place = if !compact && !sheet {
        live_target
            .map(|rect| choose_place(rect, viewport, FULL_CARD_HEIGHT))
            .unwrap_or(Place::TopRight)
    } else {
        Place::TopRight
    };
    let expanded = more();
    let tight = compact && dock.max_height.is_some_and(|max| max < 140.0) && !expanded;
    let show_detail = !compact || expanded;
    let mut class = String::from("coach coach-tour");
    if compact {
        class.push_str(&format!(" coach-dock {}", dock.dock.class()));
        if tight {
            class.push_str(" dock-tight");
        }
    } else {
        class.push(' ');
        class.push_str(place.class());
    }
    if sheet {
        class.push_str(" coach-sheet");
    }
    let style = match (compact, dock.max_height) {
        (true, Some(max)) => format!("max-height:{}px", max.max(96.0)),
        _ => String::new(),
    };

    let module_key = module.key;
    let stop_id = stop.id;
    let next = move |_| {
        ctrl.mark_viewed(module_key, stop_id);
        if index + 1 < count {
            ctrl.go_stop(index + 1);
        } else {
            ctrl.exit_tour();
        }
    };
    let back = move |_| {
        if index > 0 {
            ctrl.go_stop(index - 1);
        }
    };
    let exit = move |_| {
        ctrl.exit_tour();
        ctrl.set_panel(Panel::Expanded);
    };

    let eyebrow = if compact {
        "Walkthrough"
    } else {
        "Security walkthrough"
    };
    let notice_role = stop.notice_role(role).permission_label();
    let admin_note = stop.admin_note.map(|note| admin_note_for(note, role));

    rsx! {
        div {
            class: "{class}",
            style: "{style}",
            role: "dialog",
            aria_label: "Security walkthrough: {stop.title}",
            "data-testid": "coach-tour-card",
            "data-stop": "{stop.id}",
            "data-dock-max-height": dock.max_height.map(|height| height.to_string()).unwrap_or_default(),
            "data-coach-narrow": "{layout::is_narrow(viewport.width)}",
            "data-status": match runtime.status {
                TourStatus::Idle => "idle",
                TourStatus::Running => "running",
                TourStatus::Found => "found",
                TourStatus::Missing => "missing",
                TourStatus::NoExample(_) => "no-example",
                TourStatus::NoView => "no-view",
                TourStatus::Failed(_) => "failed",
            },
            div { class: "coach-tour-head",
                span { class: "coach-head-mark sm", CoachMark { size: 14 } }
                div { style: "min-width:0;flex:1",
                    div { class: "coach-callout-eyebrow", style: "margin-bottom:1px", "{eyebrow} · {index + 1} of {count}" }
                    div { class: "coach-tour-mod", "{module.title}" }
                }
                button {
                    class: "coach-link focus-ring",
                    "data-testid": "coach-tour-minimize",
                    onclick: move |_| ctrl.set_panel(Panel::Minimized),
                    "Minimize"
                }
            }
            div { class: "coach-tour-dots", aria_hidden: "true",
                for (position, other) in module.stops.iter().enumerate() {
                    span { class: if position == index { "cur" } else if viewed.iter().any(|id| id == other.id) { "seen" } else { "" } }
                }
            }
            div { class: "coach-tour-body",
                h3 { "{stop.title}" }
                p { class: if compact && !expanded { "clamp" } else { "" }, "{stop.why}" }
                if show_detail {
                    if let Some(figure) = stop.figure { CoachFigure { figure: figure } }
                    if access != StopAccess::Open {
                        div { class: "coach-gate", "data-testid": "coach-gate",
                            Icon { name: IconName::Key, size: 12 }
                            span {
                                b { "{notice_role} permission required." }
                                " {stop.gated_doing.unwrap_or_default()}"
                            }
                        }
                    } else {
                        div { class: "coach-do",
                            Icon { name: IconName::ArrowRight, size: 12 }
                            span { "{stop.doing}" }
                        }
                    }
                    if let Some(note) = admin_note {
                        div { class: "coach-admin-note",
                            Icon { name: IconName::Key, size: 11 }
                            span { "{note}" }
                        }
                    }
                    if let Some(important) = stop.important {
                        div { class: "coach-important", "data-testid": "coach-important",
                            b { "Important" }
                            span { "{important}" }
                        }
                    }
                }
                if compact {
                    button {
                        class: "coach-inline-link",
                        "data-testid": "coach-tour-more",
                        onclick: move |_| more.set(!more()),
                        if expanded { "Less" } else { "Details" }
                    }
                }
                match &runtime.status {
                    TourStatus::Missing => rsx! {
                        div { class: "coach-missing", "data-testid": "coach-missing",
                            Icon { name: IconName::Info, size: 12 }
                            span { "The highlighted control isn't on screen. It may have been closed." }
                            button { class: "coach-inline-link", "data-testid": "coach-show-me", onclick: move |_| ctrl.rerun(), "Show me" }
                        }
                    },
                    TourStatus::NoExample(text) => rsx! {
                        div { class: "coach-missing", "data-testid": "coach-no-example", role: "status",
                            Icon { name: IconName::Info, size: 12 }
                            span { b { "No example is currently available. " } "{text}" }
                        }
                    },
                    TourStatus::Failed(error) => rsx! {
                        div { class: "coach-missing", "data-testid": "coach-failed", role: "alert",
                            Icon { name: IconName::Info, size: 12 }
                            span { "Could not load an example: {error}" }
                            button { class: "coach-inline-link", "data-testid": "coach-show-me", onclick: move |_| ctrl.rerun(), "Try again" }
                        }
                    },
                    _ => rsx! {},
                }
            }
            div { class: "coach-tour-foot",
                button {
                    class: "btn btn-ghost focus-ring xs",
                    "data-testid": "coach-tour-back",
                    disabled: index == 0,
                    onclick: back,
                    Icon { name: IconName::ChevronLeft, size: 12 }
                    " Back"
                }
                button { class: "coach-link focus-ring", "data-testid": "coach-tour-exit", onclick: exit, "Exit walkthrough" }
                span { style: "flex:1" }
                button {
                    class: "btn btn-primary focus-ring xs",
                    "data-testid": "coach-tour-next",
                    onclick: next,
                    if index + 1 == count { "Finish" } else { "Next " }
                    if index + 1 < count { Icon { name: IconName::ChevronRight, size: 12 } }
                }
            }
        }
    }
}

/// Top bar launcher. Available to every authenticated role.
///
/// Reopens the coach after setup completes, after it was closed and after a
/// walkthrough was exited.
#[component]
pub fn CoachGuideButton() -> Element {
    let Some(ctrl) = try_use_context::<CoachController>() else {
        return rsx! {};
    };
    if (ctrl.role)().is_none() {
        return rsx! {};
    }
    let snapshot = (ctrl.setup)();
    let setup_open = ctrl.is_admin() && snapshot.setup_open();
    let (count, total) = (snapshot.count(), snapshot.total());
    rsx! {
        button {
            class: "btn btn-ghost focus-ring coach-guide-btn",
            "data-testid": "coach-guide-button",
            title: "Open the Crystal Forge Coach: setup and security walkthroughs",
            aria_label: "Guide",
            onclick: move |_| {
                let expanded = ctrl.ui.read().panel == Panel::Expanded;
                if expanded && (ctrl.active)().is_none() {
                    ctrl.set_panel(Panel::Minimized);
                } else {
                    ctrl.open();
                }
            },
            Icon { name: IconName::Help, size: 15 }
            span { class: "coach-guide-label", "Guide" }
            if setup_open {
                span { class: "coach-guide-dot", title: "{count} of {total} setup steps complete" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::onboarding::setup::STEPS;

    #[test]
    fn admin_note_names_the_permission_for_other_roles() {
        let plural = "New bundle are administrator actions.";
        assert_eq!(admin_note_for(plural, CoachRole::Admin), plural);
        assert_eq!(
            admin_note_for(plural, CoachRole::Viewer),
            "New bundle require Administrator permission."
        );
        let singular = "Importing policies is an administrator action.";
        assert_eq!(
            admin_note_for(singular, CoachRole::Operator),
            "Importing policies requires Administrator permission."
        );
    }

    #[test]
    fn setup_steps_use_unique_icons_the_panel_can_draw() {
        for step in STEPS {
            assert_ne!(step.icon, "", "{} needs an icon", step.id);
        }
        assert_eq!(step_icon("env"), IconName::Env);
        assert_eq!(step_icon("cube"), IconName::Cube);
        assert_eq!(step_icon("unknown"), IconName::Shield);
    }

    #[test]
    fn coach_css_keeps_the_nonmodal_bounded_contract() {
        // The coach is a floating, non-modal card. It must not use a backdrop,
        // must stay inside the viewport, and must dock compactly over drawers.
        let css = include_str!("../../../assets/app.css");
        assert!(css.contains(".coach {"));
        assert!(css.contains("max-height: calc(100vh - 92px)"));
        assert!(css.contains(".coach.coach-dock"));
        assert!(css.contains(".coach.coach-sheet"));
        assert!(css.contains("@media (prefers-reduced-motion: reduce) { .coach-spot"));
        assert!(!css.contains(".coach-backdrop"));
    }
}
