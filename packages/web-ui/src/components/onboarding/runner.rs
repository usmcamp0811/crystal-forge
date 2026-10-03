//! Walkthrough runner: navigation, open-only preparation and target tracking.
//!
//! The runner moves the application to the surface a stop explains and then
//! keeps measuring the stop's target so the spotlight follows layout changes.
//!
//! # Safety contract
//!
//! The runner NEVER mutates a security record. It enforces this structurally:
//!
//! - It navigates only through typed routes.
//! - It clicks only elements that carry a `data-coach-open` attribute. Views
//!   add that attribute to controls that only open a surface or select local
//!   draft state. No save, submit, apply, renew, convert, verify, close or
//!   archive control carries it. Tests in [`super::tours`] reject opener names
//!   that look like mutations.
//! - It never types into a field and never dispatches a form submission.
//!
//! # Same-route navigation
//!
//! Some pages read their query string once and again on `popstate`. When a
//! stop changes only the query of the current page, the runner replaces the
//! history entry and dispatches `popstate` so the page re-reads its state.

use dioxus::prelude::*;
use dioxus_router::Navigator;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::JsCast;

use super::layout::{Rect, Viewport};
use super::records::{self, Resolution};
use super::state::CoachController;
use super::tours::{self, CoachRole, Prep, Stop, StopAccess};
use crate::routes::Route;

/// How a stop's target lookup is going.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum TourStatus {
    /// No stop is active.
    #[default]
    Idle,
    /// The runner is navigating or waiting for the target.
    Running,
    /// The target is on screen and the spotlight follows it.
    Found,
    /// The target did not appear. The person can ask the coach to show it.
    Missing,
    /// No suitable record exists. The text explains what is required.
    NoExample(String),
    /// The role cannot read the destination, so the coach did not navigate.
    NoView,
    /// A read failed. The text is the API error.
    Failed(String),
}

/// Live measurements for the active stop.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TourRuntime {
    /// Target rectangle in viewport coordinates.
    pub rect: Option<Rect>,
    /// Lookup status.
    pub status: TourStatus,
}

/// Measurements of the page around the coach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surroundings {
    /// Whether a large drawer or modal is open.
    pub overlay_open: bool,
    /// Viewport and shell measurements.
    pub viewport: Viewport,
}

impl Default for Surroundings {
    fn default() -> Self {
        Self {
            overlay_open: false,
            viewport: Viewport {
                width: 1280.0,
                height: 800.0,
                sidebar: 240.0,
                top: 72.0,
            },
        }
    }
}

/// Elements that mean a large drawer or modal covers the working surface.
const OVERLAY_SELECTOR: &str =
    ".modal-backdrop, .fl-tray, .side-panel, .poam-tray, [role=\"dialog\"][aria-modal=\"true\"]";

/// Elements the runner never treats as a target, including the coach itself.
const COACH_SELECTOR: &str = ".coach, .coach-pill, .coach-spot";

/// Time the runner waits for a page to settle after navigation.
const SETTLE_MS: u32 = 280;
/// Time the runner waits after clicking an opener.
const OPEN_MS: u32 = 240;
/// Longest wait for an opener before the stop reports no example.
const OPENER_TIMEOUT_MS: u32 = 1800;
/// Interval between opener lookups.
const OPENER_POLL_MS: u32 = 90;
/// Interval between target measurements.
const TARGET_POLL_MS: u32 = 400;
/// Time after which an absent target counts as missing.
const MISSING_AFTER_MS: u32 = 2600;

fn document() -> Option<web_sys::Document> {
    web_sys::window().and_then(|window| window.document())
}

fn is_visible(element: &web_sys::Element) -> bool {
    // `getBoundingClientRect` can report dimensions for descendants that are
    // inside a closed disclosure. The coach must not skip an opener because
    // that hidden target has a non-zero box.
    if element
        .closest("details:not([open])")
        .ok()
        .flatten()
        .is_some()
    {
        return false;
    }
    let rect = element.get_bounding_client_rect();
    rect.width() > 0.0 && rect.height() > 0.0
}

fn is_inside_coach(element: &web_sys::Element) -> bool {
    element.closest(COACH_SELECTOR).ok().flatten().is_some()
}

fn is_disabled(element: &web_sys::Element) -> bool {
    element.has_attribute("disabled")
        || element.get_attribute("aria-disabled").as_deref() == Some("true")
}

fn all_matching(selector: &str) -> Vec<web_sys::Element> {
    let Some(document) = document() else {
        return Vec::new();
    };
    let Ok(list) = document.query_selector_all(selector) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|index| list.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .collect()
}

/// Returns the first visible element carrying one of `ids` in
/// `data-coach-target`. Identifiers are tried in order.
pub fn find_target(ids: &[&str]) -> Option<web_sys::Element> {
    ids.iter().find_map(|id| {
        all_matching(&format!("[data-coach-target=\"{id}\"]"))
            .into_iter()
            .find(|element| is_visible(element) && !is_inside_coach(element))
    })
}

fn target_present(id: &str) -> bool {
    find_target(&[id]).is_some()
}

fn rect_of(element: &web_sys::Element) -> Rect {
    let rect = element.get_bounding_client_rect();
    Rect {
        top: rect.top(),
        left: rect.left(),
        width: rect.width(),
        height: rect.height(),
    }
}

/// Returns the current rectangle of the first visible stop target.
///
/// The tour runner also stores a polled rectangle for spotlight rendering.
/// Layout can change between polls, so the dock reads the live DOM rectangle
/// before placing a card near an open drawer or modal.
pub fn find_target_rect(ids: &[&str]) -> Option<Rect> {
    find_target(ids).as_ref().map(rect_of)
}

/// Clicks the first enabled, visible `data-coach-open` control among `openers`.
///
/// This is the only click the coach performs. Returns whether it clicked.
fn click_open(openers: &[&str], key: Option<&str>) -> bool {
    for opener in openers {
        let selector = match key {
            Some(key) => format!("[data-coach-open=\"{opener}\"][data-coach-key=\"{key}\"]"),
            None => format!("[data-coach-open=\"{opener}\"]"),
        };
        let candidate = all_matching(&selector)
            .into_iter()
            .find(|element| is_visible(element) && !is_disabled(element));
        if let Some(element) = candidate
            && let Ok(html) = element.dyn_into::<web_sys::HtmlElement>()
        {
            html.click();
            return true;
        }
    }
    false
}

async fn wait_and_click(openers: &[&str], key: Option<&str>) -> bool {
    let mut waited = 0;
    while waited <= OPENER_TIMEOUT_MS {
        if click_open(openers, key) {
            return true;
        }
        TimeoutFuture::new(OPENER_POLL_MS).await;
        waited += OPENER_POLL_MS;
    }
    false
}

/// Returns whether a large drawer or modal is open.
pub fn overlay_open() -> bool {
    all_matching(OVERLAY_SELECTOR)
        .iter()
        .any(|element| is_visible(element) && !is_inside_coach(element))
}

/// Measures the viewport and shell.
pub fn measure_viewport() -> Viewport {
    let fallback = Surroundings::default().viewport;
    let Some(window) = web_sys::window() else {
        return fallback;
    };
    let width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(fallback.width);
    let height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(fallback.height);
    let sidebar = all_matching(".sidebar")
        .first()
        .map(|element| element.get_bounding_client_rect().right().max(0.0))
        .unwrap_or(fallback.sidebar);
    let top = all_matching("header.topbar")
        .first()
        .map(|element| element.get_bounding_client_rect().bottom() + 8.0)
        .unwrap_or(fallback.top);
    Viewport {
        width,
        height,
        sidebar,
        top,
    }
}

/// Scrolls `element` into the middle of its scroll container when it is not
/// comfortably visible.
fn reveal(element: &web_sys::Element) {
    let viewport = measure_viewport();
    let rect = element.get_bounding_client_rect();
    if rect.top() < viewport.top + 60.0 || rect.bottom() > viewport.height - 40.0 {
        let options = web_sys::ScrollIntoViewOptions::new();
        options.set_block(web_sys::ScrollLogicalPosition::Center);
        element.scroll_into_view_with_scroll_into_view_options(&options);
    }
}

fn location() -> (String, String) {
    web_sys::window()
        .map(|window| {
            let location = window.location();
            (
                location.pathname().unwrap_or_default(),
                location.search().unwrap_or_default(),
            )
        })
        .unwrap_or_default()
}

fn dispatch_popstate() {
    if let (Some(window), Ok(event)) = (web_sys::window(), web_sys::Event::new("popstate")) {
        let _ = window.dispatch_event(&event);
    }
}

/// Moves to `route` through the typed router.
///
/// Changing only the query of the current page replaces the history entry and
/// dispatches `popstate`, because several pages re-read their query only then.
async fn navigate(navigator: Navigator, route: Route) {
    let target = route.to_string();
    let (path, search) = location();
    let (target_path, target_query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), format!("?{query}")),
        None => (target.clone(), String::new()),
    };
    if path == target_path && search == target_query {
        return;
    }
    if path == target_path {
        navigator.replace(route);
        TimeoutFuture::new(0).await;
        dispatch_popstate();
    } else {
        navigator.push(route);
    }
}

async fn run_prep(prep: &Prep) -> bool {
    if prep.satisfied_by.is_some_and(target_present) {
        return true;
    }
    if wait_and_click(prep.openers, None).await {
        TimeoutFuture::new(OPEN_MS).await;
        return true;
    }
    false
}

fn explain_missing(stop: &Stop) -> TourStatus {
    match stop.no_example {
        Some(text) => TourStatus::NoExample(text.to_string()),
        None => TourStatus::Missing,
    }
}

fn publish(mut runtime: Signal<TourRuntime>, next: TourRuntime) {
    if *runtime.peek() != next {
        runtime.set(next);
    }
}

/// Runs one stop to completion of its preparation, then tracks its target
/// until the caller drops the future.
async fn run_stop(
    ctrl: CoachController,
    navigator: Navigator,
    stop: &'static Stop,
    role: CoachRole,
) {
    let runtime = ctrl.runtime;
    let running = TourRuntime {
        rect: None,
        status: TourStatus::Running,
    };
    publish(runtime, running.clone());

    if stop.access(role) == StopAccess::NoView {
        publish(
            runtime,
            TourRuntime {
                rect: None,
                status: TourStatus::NoView,
            },
        );
        return;
    }

    match records::resolve(stop.nav, role).await {
        Err(error) => {
            publish(
                runtime,
                TourRuntime {
                    rect: None,
                    status: TourStatus::Failed(error),
                },
            );
            return;
        }
        Ok(Resolution::NoExample) => {
            publish(
                runtime,
                TourRuntime {
                    rect: None,
                    status: explain_missing(stop),
                },
            );
            return;
        }
        Ok(Resolution::Go(destination)) => {
            navigate(navigator, destination.route).await;
            TimeoutFuture::new(SETTLE_MS).await;
            if let Some(open) = destination.keyed_open
                && stop.access(role) != StopAccess::NoView
            {
                if !wait_and_click(&[open.opener], Some(&open.key)).await {
                    publish(
                        runtime,
                        TourRuntime {
                            rect: None,
                            status: explain_missing(stop),
                        },
                    );
                    return;
                }
                TimeoutFuture::new(OPEN_MS).await;
            }
        }
    }

    for prep in stop.prep_for(role) {
        if !run_prep(prep).await {
            publish(
                runtime,
                TourRuntime {
                    rect: None,
                    status: explain_missing(stop),
                },
            );
            return;
        }
    }

    let targets = stop.target_for(role);
    let mut waited = 0_u32;
    let mut revealed = false;
    loop {
        match find_target(targets) {
            Some(element) => {
                if !revealed {
                    reveal(&element);
                    revealed = true;
                }
                publish(
                    runtime,
                    TourRuntime {
                        rect: Some(rect_of(&element)),
                        status: TourStatus::Found,
                    },
                );
            }
            None => {
                let status = if waited > MISSING_AFTER_MS {
                    TourStatus::Missing
                } else {
                    TourStatus::Running
                };
                publish(runtime, TourRuntime { rect: None, status });
            }
        }
        TimeoutFuture::new(TARGET_POLL_MS).await;
        waited += TARGET_POLL_MS;
    }
}

/// Runs the active walkthrough stop. Mount once, inside the router.
///
/// Dioxus restarts the future when the active stop, the rerun counter or the
/// role changes, which cancels the previous stop's polling.
pub fn use_tour_runner(ctrl: CoachController) {
    let navigator = use_navigator();
    let _runner = use_resource(move || async move {
        let active = (ctrl.active)();
        let _rerun = (ctrl.nonce)();
        let role = ctrl.role();
        let mut runtime = ctrl.runtime;
        let Some(active) = active else {
            publish(runtime, TourRuntime::default());
            return;
        };
        let stop = tours::module(active.module).and_then(|module| module.stops.get(active.index));
        match stop {
            Some(stop) => run_stop(ctrl, navigator, stop, role).await,
            None => runtime.set(TourRuntime::default()),
        }
    });
}

/// Keeps the overlay and viewport measurements current. Mount once.
pub fn use_surroundings(ctrl: CoachController) {
    use_future(move || async move {
        let mut surroundings = ctrl.surroundings;
        loop {
            let next = Surroundings {
                overlay_open: overlay_open(),
                viewport: measure_viewport(),
            };
            if *surroundings.peek() != next {
                surroundings.set(next);
            }
            TimeoutFuture::new(350).await;
        }
    });
}
