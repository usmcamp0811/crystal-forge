//! Browser-local coach presentation state and the shared coach controller.
//!
//! # State ownership
//!
//! | State | Owner | Persisted |
//! | --- | --- | --- |
//! | Setup completion | Server, through the Administrator-only endpoint | Server |
//! | Panel visibility, track, viewed walkthrough stops | This browser | `localStorage` |
//! | Active walkthrough stop | This tab | No |
//! | Role | The authenticated session | No |
//!
//! Browser storage never records, and never changes, setup or security-domain
//! state. Clearing it only resets what the person has seen.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::runner::{Surroundings, TourRuntime};
use super::setup::{self, STEPS};
use super::tours::{self, CoachRole, Progress};
use crate::api::models::SetupWizardProgressResponse;

/// `localStorage` key for coach presentation state.
///
/// The shape matches the design's `cf.coach.ui.v2` record so the two stay
/// comparable. Role is deliberately not stored.
pub const UI_STORAGE_KEY: &str = "cf.coach.ui.v2";

/// Visibility of the coach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Panel {
    /// The full panel or walkthrough card is shown.
    #[default]
    Expanded,
    /// Only the pill is shown.
    Minimized,
    /// Nothing is shown. The Guide button reopens the coach.
    Dismissed,
}

/// The track shown in the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Track {
    /// Server-derived first-run setup. Administrators only.
    #[default]
    Setup,
    /// Security workflow walkthroughs. Every role.
    Security,
}

/// Persisted coach presentation state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoachUi {
    /// Panel visibility.
    pub panel: Panel,
    /// Selected track. Non-administrators always see the Security track.
    pub track: Track,
    /// Stops viewed in this browser.
    pub progress: Progress,
    /// Setup callouts hidden by page. Hiding a callout never changes progress.
    pub callout_hidden: BTreeMap<String, bool>,
}

impl CoachUi {
    /// Parses stored state. Returns [`None`] for absent or malformed data.
    pub fn parse(raw: &str) -> Option<Self> {
        serde_json::from_str(raw).ok()
    }

    /// Serializes state for storage.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Returns the track a person with `role` sees.
    pub fn effective_track(&self, role: CoachRole) -> Track {
        if role == CoachRole::Admin {
            self.track
        } else {
            Track::Security
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|window| window.local_storage().ok().flatten())
}

/// Reads stored state. Returns [`None`] on first run or when storage fails.
pub fn load_ui() -> Option<CoachUi> {
    storage()
        .and_then(|store| store.get_item(UI_STORAGE_KEY).ok().flatten())
        .and_then(|raw| CoachUi::parse(&raw))
}

/// Writes state. A storage failure only loses the presentation record.
pub fn save_ui(ui: &CoachUi) {
    if let Some(store) = storage() {
        let _ = store.set_item(UI_STORAGE_KEY, &ui.to_json());
    }
}

/// Sets the flag existing setup pages read to show their setup callout.
pub fn store_setup_context() {
    if let Some(store) = storage() {
        let _ = store.set_item("cf.from_setup", "1");
    }
}

/// One active walkthrough position. Not persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveTour {
    /// Module key, `A` to `E`.
    pub module: &'static str,
    /// Zero-based stop index.
    pub index: usize,
}

/// Server-reported setup state as the browser sees it.
///
/// Only Administrators load this. Other roles stay in [`SetupSnapshot::NotApplicable`]
/// and never request the Administrator-only endpoint.
#[derive(Clone, Debug, PartialEq)]
pub enum SetupSnapshot {
    /// The role cannot read setup progress.
    NotApplicable,
    /// The first load is pending.
    Loading,
    /// The server could not report progress.
    Unavailable,
    /// The server reported progress.
    Loaded(SetupWizardProgressResponse),
}

impl SetupSnapshot {
    /// Returns the loaded progress.
    pub fn progress(&self) -> Option<&SetupWizardProgressResponse> {
        match self {
            Self::Loaded(progress) => Some(progress),
            _ => None,
        }
    }

    /// Returns the number of steps the server reports complete.
    pub fn count(&self) -> usize {
        self.progress().map(setup::completed_count).unwrap_or(0)
    }

    /// Returns the number of visible setup steps.
    pub fn total(&self) -> usize {
        self.progress()
            .map(|progress| setup::visible_steps(progress).len())
            .unwrap_or(STEPS.len())
    }

    /// Returns whether the server reports every setup step complete.
    pub fn all_done(&self) -> bool {
        self.progress().is_some_and(setup::all_complete)
    }

    /// Returns whether setup still needs an Administrator.
    pub fn setup_open(&self) -> bool {
        self.progress().is_some() && !self.all_done()
    }
}

/// Shared handle to coach state. Provided once by the application shell.
#[derive(Clone, Copy)]
pub struct CoachController {
    /// Persisted presentation state.
    pub ui: Signal<CoachUi>,
    /// Active walkthrough position.
    pub active: Signal<Option<ActiveTour>>,
    /// Increments to force the runner to run the current stop again.
    pub nonce: Signal<u64>,
    /// Role of the authenticated session.
    pub role: Memo<Option<CoachRole>>,
    /// Server-reported setup state.
    pub setup: Signal<SetupSnapshot>,
    /// Whether an authorized system has a first signed report. This is a
    /// read-only page observation; it does not complete the agent step.
    pub agent_reported: Signal<bool>,
    /// Increments to request a fresh setup progress read.
    pub refresh: Signal<u64>,
    /// Live spotlight measurements for the active stop.
    pub runtime: Signal<TourRuntime>,
    /// Measurements of the page around the coach.
    pub surroundings: Signal<Surroundings>,
    /// Whether the person already had stored state before this session.
    pub had_stored_state: bool,
}

impl CoachController {
    /// Creates the controller. Call once from the application shell.
    pub fn new(role: Memo<Option<CoachRole>>) -> Self {
        let stored = load_ui();
        Self {
            had_stored_state: stored.is_some(),
            ui: Signal::new(stored.unwrap_or_default()),
            active: Signal::new(None),
            nonce: Signal::new(0),
            role,
            setup: Signal::new(SetupSnapshot::NotApplicable),
            agent_reported: Signal::new(false),
            refresh: Signal::new(0),
            runtime: Signal::new(TourRuntime::default()),
            surroundings: Signal::new(Surroundings::default()),
        }
    }

    /// Returns the role, treating an unresolved role as the least privileged.
    pub fn role(self) -> CoachRole {
        (self.role)().unwrap_or(CoachRole::Viewer)
    }

    /// Returns whether the role can read setup progress.
    pub fn is_admin(self) -> bool {
        self.role() == CoachRole::Admin
    }

    /// Returns the track currently shown.
    pub fn track(self) -> Track {
        self.ui.read().effective_track(self.role())
    }

    /// Sets panel visibility.
    pub fn set_panel(self, panel: Panel) {
        let mut ui = self.ui;
        ui.write().panel = panel;
    }

    /// Selects a track. Non-administrators cannot leave the Security track.
    pub fn set_track(self, track: Track) {
        let mut ui = self.ui;
        ui.write().track = track;
    }

    /// Opens the coach. Non-administrators open on the Security track.
    pub fn open(self) {
        let admin = self.is_admin();
        let mut ui = self.ui;
        let mut ui = ui.write();
        ui.panel = Panel::Expanded;
        if !admin {
            ui.track = Track::Security;
        }
    }

    /// Reopens the Setup track for an Administrator.
    pub fn relaunch(self) {
        let mut active = self.active;
        active.set(None);
        let mut ui = self.ui;
        let mut ui = ui.write();
        ui.panel = Panel::Expanded;
        ui.track = Track::Setup;
        ui.callout_hidden.clear();
    }

    /// Starts a walkthrough at `index`.
    pub fn start_tour(self, module: &'static str, index: usize) {
        let mut active = self.active;
        active.set(Some(ActiveTour { module, index }));
        let mut ui = self.ui;
        {
            let mut ui = ui.write();
            if ui.panel == Panel::Dismissed {
                ui.panel = Panel::Expanded;
            }
            ui.track = Track::Security;
        }
        self.bump();
    }

    /// Moves the active walkthrough to `index`.
    pub fn go_stop(self, index: usize) {
        let mut active = self.active;
        let next = active().map(|tour| ActiveTour { index, ..tour });
        active.set(next);
        self.bump();
    }

    /// Runs the current stop again.
    pub fn rerun(self) {
        self.bump();
    }

    /// Ends the active walkthrough.
    pub fn exit_tour(self) {
        let mut active = self.active;
        active.set(None);
    }

    /// Records that a stop was viewed.
    pub fn mark_viewed(self, module: &str, stop: &str) {
        let mut ui = self.ui;
        tours::mark_viewed(&mut ui.write().progress, module, stop);
    }

    /// Clears which walkthrough stops this browser has viewed.
    ///
    /// Does not touch setup progress or any security record.
    pub fn restart_walkthroughs(self) {
        let mut active = self.active;
        active.set(None);
        let mut ui = self.ui;
        ui.write().progress = BTreeMap::new();
    }

    /// Hides the setup callout for `page` in this browser.
    pub fn hide_callout(self, page: &str) {
        let mut ui = self.ui;
        ui.write().callout_hidden.insert(page.to_string(), true);
    }

    fn bump(self) {
        let mut nonce = self.nonce;
        nonce.set(nonce() + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::SetupWizardStepStatus;

    #[test]
    fn stored_state_round_trips_and_ignores_unknown_fields() {
        let mut ui = CoachUi {
            panel: Panel::Minimized,
            track: Track::Security,
            ..CoachUi::default()
        };
        tours::mark_viewed(&mut ui.progress, "A", "A1");
        assert_eq!(CoachUi::parse(&ui.to_json()), Some(ui));

        // The design stores more fields. They must not break loading.
        let parsed = CoachUi::parse(
            r#"{"panel":"dismissed","track":"setup","role":"admin","nonce":4,
                "calloutHidden":{"x":true},"progress":{"B":["B1"]},"forceSheet":false}"#,
        )
        .unwrap();
        assert_eq!(parsed.panel, Panel::Dismissed);
        assert_eq!(parsed.track, Track::Setup);
        assert_eq!(parsed.progress["B"], ["B1"]);
    }

    #[test]
    fn malformed_state_is_ignored_instead_of_trusted() {
        assert_eq!(CoachUi::parse(""), None);
        assert_eq!(CoachUi::parse("{not json"), None);
        assert_eq!(CoachUi::parse(r#"{"panel":"sideways"}"#), None);
        // Partial records fall back to defaults.
        assert_eq!(CoachUi::parse("{}"), Some(CoachUi::default()));
    }

    #[test]
    fn stored_state_cannot_carry_security_or_setup_state() {
        let json = CoachUi::default().to_json();
        for forbidden in ["observed", "agent", "complete", "acknowledged", "role"] {
            assert!(!json.contains(forbidden), "{forbidden} must not be stored");
        }
    }

    #[test]
    fn only_administrators_can_select_the_setup_track() {
        let ui = CoachUi {
            track: Track::Setup,
            ..CoachUi::default()
        };
        assert_eq!(ui.effective_track(CoachRole::Admin), Track::Setup);
        assert_eq!(ui.effective_track(CoachRole::Operator), Track::Security);
        assert_eq!(ui.effective_track(CoachRole::Viewer), Track::Security);
    }

    #[test]
    fn setup_snapshot_reports_server_counts_and_completion() {
        assert!(!SetupSnapshot::NotApplicable.setup_open());
        assert!(!SetupSnapshot::Loading.setup_open());
        assert!(!SetupSnapshot::Unavailable.setup_open());
        assert_eq!(SetupSnapshot::NotApplicable.total(), 9);

        let mut progress = SetupWizardProgressResponse {
            dismissed: false,
            agent_acknowledged: false,
            environment: SetupWizardStepStatus {
                complete: true,
                count: 1,
            },
            flake: SetupWizardStepStatus::default(),
            builder: SetupWizardStepStatus::default(),
            cache: SetupWizardStepStatus::default(),
            system: SetupWizardStepStatus::default(),
            policy: Some(SetupWizardStepStatus::default()),
            bundle: Some(SetupWizardStepStatus::default()),
            poam: Some(SetupWizardStepStatus::default()),
            all_required_complete: false,
            all_coach_steps_complete: Some(false),
        };
        let open = SetupSnapshot::Loaded(progress.clone());
        assert_eq!((open.count(), open.total()), (1, 9));
        assert!(open.setup_open() && !open.all_done());

        progress.all_coach_steps_complete = Some(true);
        let done = SetupSnapshot::Loaded(progress);
        assert!(done.all_done() && !done.setup_open());
    }
}
