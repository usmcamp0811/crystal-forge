use dioxus::prelude::*;
use std::cell::RefCell;

use crate::api::client::update_user_preferences;
use crate::api::models::{
    SystemsViewPreference, UiDensityPreference, UiThemePreference, UpdateUserPreferences,
    UserPreferencesDto,
};
use crate::state::theme::UiTheme;

pub const THEME_KEY: &str = "cf.ui.theme";
pub const DENSITY_KEY: &str = "cf.ui.density";
pub const SYSTEMS_VIEW_KEY: &str = "crystal_forge.systems.view";
pub const SIDEBAR_COLLAPSED_KEY: &str = "cf-sidebar-collapsed";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyPreferenceSnapshot {
    pub theme: UiThemePreference,
    pub density: UiDensityPreference,
    pub sidebar_collapsed: bool,
    pub default_systems_view: SystemsViewPreference,
}

pub fn legacy_snapshot() -> LegacyPreferenceSnapshot {
    LegacyPreferenceSnapshot {
        theme: theme_from_storage(read_storage(THEME_KEY).as_deref()),
        density: density_from_storage(read_storage(DENSITY_KEY).as_deref()),
        sidebar_collapsed: read_storage(SIDEBAR_COLLAPSED_KEY)
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or(false),
        default_systems_view: systems_view_from_storage(read_storage(SYSTEMS_VIEW_KEY).as_deref()),
    }
}

pub fn legacy_snapshot_with_current_defaults(
    current_theme: UiTheme,
    current_density: &str,
    current_sidebar_collapsed: bool,
    current_systems_view: &str,
) -> LegacyPreferenceSnapshot {
    LegacyPreferenceSnapshot {
        theme: read_storage(THEME_KEY)
            .as_deref()
            .map(Some)
            .map(theme_from_storage)
            .unwrap_or_else(|| theme_to_preference(current_theme)),
        density: read_storage(DENSITY_KEY)
            .as_deref()
            .map(Some)
            .map(density_from_storage)
            .unwrap_or_else(|| density_from_storage(Some(current_density))),
        sidebar_collapsed: read_storage(SIDEBAR_COLLAPSED_KEY)
            .as_deref()
            .map(|value| value == "true")
            .unwrap_or(current_sidebar_collapsed),
        default_systems_view: read_storage(SYSTEMS_VIEW_KEY)
            .as_deref()
            .map(Some)
            .map(systems_view_from_storage)
            .unwrap_or_else(|| systems_view_from_storage(Some(current_systems_view))),
    }
}

pub fn import_request(snapshot: &LegacyPreferenceSnapshot) -> UpdateUserPreferences {
    UpdateUserPreferences {
        theme: Some(snapshot.theme),
        density: Some(snapshot.density),
        sidebar_collapsed: Some(snapshot.sidebar_collapsed),
        default_systems_view: Some(snapshot.default_systems_view),
    }
}

pub fn theme_from_server(value: &str) -> UiTheme {
    if value == "light" {
        UiTheme::Light
    } else {
        UiTheme::Dark
    }
}

pub fn theme_from_storage(value: Option<&str>) -> UiThemePreference {
    if value == Some("light") {
        UiThemePreference::Light
    } else {
        UiThemePreference::Dark
    }
}

pub fn theme_to_preference(theme: UiTheme) -> UiThemePreference {
    match theme {
        UiTheme::Dark => UiThemePreference::Dark,
        UiTheme::Light => UiThemePreference::Light,
    }
}

pub fn density_from_storage(value: Option<&str>) -> UiDensityPreference {
    if value == Some("compact") {
        UiDensityPreference::Compact
    } else {
        UiDensityPreference::Comfortable
    }
}

pub fn systems_view_from_storage(value: Option<&str>) -> SystemsViewPreference {
    if value == Some("table") {
        SystemsViewPreference::Table
    } else {
        SystemsViewPreference::Cards
    }
}

pub fn density_to_storage(value: UiDensityPreference) -> &'static str {
    match value {
        UiDensityPreference::Comfortable => "comfortable",
        UiDensityPreference::Compact => "compact",
    }
}

pub fn systems_view_to_storage(value: SystemsViewPreference) -> &'static str {
    match value {
        SystemsViewPreference::Cards => "cards",
        SystemsViewPreference::Table => "table",
    }
}

pub fn mirror_to_storage(preferences: &UserPreferencesDto) {
    write_storage(THEME_KEY, &preferences.theme);
    write_storage(DENSITY_KEY, &preferences.density);
    write_storage(
        SIDEBAR_COLLAPSED_KEY,
        if preferences.sidebar_collapsed {
            "true"
        } else {
            "false"
        },
    );
    write_storage(SYSTEMS_VIEW_KEY, &preferences.default_systems_view);
}

#[derive(Default)]
struct PreferenceSaveState {
    in_flight: bool,
    pending: Option<UpdateUserPreferences>,
}

thread_local! {
    static PREFERENCE_SAVE_STATE: RefCell<PreferenceSaveState> = RefCell::new(PreferenceSaveState::default());
}

#[derive(Default)]
struct PreferenceSaveWorkerGuard {
    current: Option<UpdateUserPreferences>,
    finished: bool,
}

impl PreferenceSaveWorkerGuard {
    fn new() -> Self {
        Self::default()
    }

    fn set_current(&mut self, update: UpdateUserPreferences) {
        self.current = Some(update);
    }

    fn clear_current(&mut self) {
        self.current = None;
    }

    fn finish(&mut self) {
        self.finished = true;
    }
}

impl Drop for PreferenceSaveWorkerGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }

        PREFERENCE_SAVE_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(mut current) = self.current.take() {
                if let Some(pending) = state.pending.take() {
                    merge_update(&mut current, pending);
                }
                state.pending = Some(current);
            }
            state.in_flight = false;
        });
    }
}

fn merge_update(target: &mut UpdateUserPreferences, update: UpdateUserPreferences) {
    if update.theme.is_some() {
        target.theme = update.theme;
    }
    if update.density.is_some() {
        target.density = update.density;
    }
    if update.sidebar_collapsed.is_some() {
        target.sidebar_collapsed = update.sidebar_collapsed;
    }
    if update.default_systems_view.is_some() {
        target.default_systems_view = update.default_systems_view;
    }
}

fn take_next_pending_update() -> Option<UpdateUserPreferences> {
    PREFERENCE_SAVE_STATE.with(|state| state.borrow_mut().pending.take())
}

fn mark_save_worker_finished_if_idle() -> bool {
    PREFERENCE_SAVE_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.pending.is_some() {
            false
        } else {
            state.in_flight = false;
            true
        }
    })
}

pub fn save_update(update: UpdateUserPreferences, mut save_error: Signal<Option<String>>) {
    save_error.set(None);

    let should_start_worker = PREFERENCE_SAVE_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(pending) = state.pending.as_mut() {
            merge_update(pending, update);
        } else {
            state.pending = Some(update);
        }

        if state.in_flight {
            false
        } else {
            state.in_flight = true;
            true
        }
    });

    if !should_start_worker {
        return;
    }

    spawn(async move {
        let mut guard = PreferenceSaveWorkerGuard::new();

        loop {
            let Some(update) = take_next_pending_update() else {
                if mark_save_worker_finished_if_idle() {
                    guard.finish();
                    break;
                }
                continue;
            };

            guard.set_current(update.clone());
            let result = update_user_preferences(&update).await;
            guard.clear_current();

            match result {
                Ok(response) => {
                    if let Some(preferences) = response.preferences {
                        mirror_to_storage(&preferences);
                        save_error.set(None);
                    } else {
                        save_error.set(Some(
                            "Could not save preferences: server returned no preferences"
                                .to_string(),
                        ));
                    }
                }
                Err(err) => save_error.set(Some(format!("Could not save preferences: {err}"))),
            }
        }
    });
}

pub fn write_storage(key: &str, value: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (key, value);

    #[cfg(target_arch = "wasm32")]
    {
        if let Some(storage) = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
        {
            let _ = storage.set_item(key, value);
        }
    }
}

pub fn read_storage(key: &str) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
        None
    }

    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
            .and_then(|storage| storage.get_item(key).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_import_request_contains_all_preferences_without_user_id() {
        let snapshot = LegacyPreferenceSnapshot {
            theme: UiThemePreference::Light,
            density: UiDensityPreference::Compact,
            sidebar_collapsed: true,
            default_systems_view: SystemsViewPreference::Table,
        };

        let request = import_request(&snapshot);

        assert_eq!(request.theme, Some(UiThemePreference::Light));
        assert_eq!(request.density, Some(UiDensityPreference::Compact));
        assert_eq!(request.sidebar_collapsed, Some(true));
        assert_eq!(
            request.default_systems_view,
            Some(SystemsViewPreference::Table)
        );
    }

    #[test]
    fn unknown_storage_values_fall_back_to_safe_defaults() {
        assert_eq!(
            density_from_storage(Some("spacious")),
            UiDensityPreference::Comfortable
        );
        assert_eq!(
            systems_view_from_storage(Some("grid")),
            SystemsViewPreference::Cards
        );
        assert_eq!(theme_from_server("solarized"), UiTheme::Dark);
    }

    #[test]
    fn legacy_snapshot_import_can_use_current_responsive_sidebar_default() {
        let snapshot =
            legacy_snapshot_with_current_defaults(UiTheme::Light, "compact", true, "table");

        assert_eq!(snapshot.theme, UiThemePreference::Light);
        assert_eq!(snapshot.density, UiDensityPreference::Compact);
        assert!(snapshot.sidebar_collapsed);
        assert_eq!(snapshot.default_systems_view, SystemsViewPreference::Table);
    }

    #[test]
    fn merge_update_keeps_latest_value_per_preference_field() {
        let mut pending = UpdateUserPreferences {
            theme: Some(UiThemePreference::Light),
            density: Some(UiDensityPreference::Comfortable),
            sidebar_collapsed: Some(false),
            default_systems_view: Some(SystemsViewPreference::Cards),
        };

        merge_update(
            &mut pending,
            UpdateUserPreferences {
                theme: Some(UiThemePreference::Dark),
                density: None,
                sidebar_collapsed: Some(true),
                default_systems_view: None,
            },
        );

        assert_eq!(pending.theme, Some(UiThemePreference::Dark));
        assert_eq!(pending.density, Some(UiDensityPreference::Comfortable));
        assert_eq!(pending.sidebar_collapsed, Some(true));
        assert_eq!(
            pending.default_systems_view,
            Some(SystemsViewPreference::Cards)
        );
    }

    #[test]
    fn dropped_save_worker_releases_in_flight_and_requeues_current_update() {
        PREFERENCE_SAVE_STATE.with(|state| {
            *state.borrow_mut() = PreferenceSaveState {
                in_flight: true,
                pending: Some(UpdateUserPreferences {
                    density: Some(UiDensityPreference::Compact),
                    ..UpdateUserPreferences::default()
                }),
            };
        });

        {
            let mut guard = PreferenceSaveWorkerGuard::new();
            guard.set_current(UpdateUserPreferences {
                theme: Some(UiThemePreference::Light),
                density: Some(UiDensityPreference::Comfortable),
                ..UpdateUserPreferences::default()
            });
        }

        PREFERENCE_SAVE_STATE.with(|state| {
            let state = state.borrow();
            assert!(!state.in_flight);
            let pending = state.pending.as_ref().expect("update should be requeued");
            assert_eq!(pending.theme, Some(UiThemePreference::Light));
            assert_eq!(pending.density, Some(UiDensityPreference::Compact));
        });
    }
}
