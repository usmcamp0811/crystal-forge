//! Sidebar alert state — badge acknowledgment and attention flash.
//!
//! Mirrors the Shell.jsx `useAttentionFlash` / `acknowledgeView` /
//! `useAcknowledgedViews` pattern (lines 167-200).
//!
//! # How it works
//! - Each navigation view has a string key (e.g. `"flakes"`, `"systems"`).
//! - When a view with an attention badge is visited, call [`acknowledge`].
//!   That badge hides for the rest of this page load.
//! - On first visit to a view that has attention items, call
//!   [`should_flash`] to get a `true` once — triggering the CSS
//!   `.attention-flash` pulse on alerting rows. Subsequent calls return `false`.
//! - A `GlobalSignal<AlertState>` is the backing store so any component that
//!   reads it re-renders when it changes.

use dioxus::prelude::*;
use std::collections::HashSet;

/// Shared alert state.  Hold in a `GlobalSignal` initialised in `main.rs`.
#[derive(Debug, Clone, Default)]
pub struct AlertState {
    /// Views whose attention badge has been dismissed this page load.
    pub acknowledged: HashSet<String>,
    /// Views that have already had their one-shot attention flash fired.
    pub flashed: HashSet<String>,
}

/// Global singleton.  Initialised to default (empty) on startup.
pub static ALERT_STATE: GlobalSignal<AlertState> = Signal::global(AlertState::default);

/// Acknowledge a view — hides its attention badge for this page load.
///
/// Call this when entering the view (on mount).
/// For Builds/Evals, call only when the failures tab is opened.
pub fn acknowledge(view_key: &str) {
    let mut state = ALERT_STATE.write();
    state.acknowledged.insert(view_key.to_string());
}

/// Returns `true` exactly once per page load for a view that has attention
/// items.  Subsequent calls always return `false`.
///
/// The caller is responsible for applying the `.attention-flash` CSS class
/// to alerting rows when this returns `true`.
pub fn should_flash(view_key: &str, has_attention: bool) -> bool {
    if !has_attention {
        return false;
    }
    let mut state = ALERT_STATE.write();
    if state.flashed.contains(view_key) {
        return false;
    }
    state.flashed.insert(view_key.to_string());
    true
}

/// Returns `true` when the badge for a view should be shown.
///
/// A badge is visible when:
/// - `count > 0`, AND
/// - the badge is NOT an attention (red) badge that has been acknowledged.
pub fn badge_visible(view_key: &str, count: i64, attention: bool) -> bool {
    if count <= 0 {
        return false;
    }
    if attention {
        let state = ALERT_STATE.read();
        !state.acknowledged.contains(view_key)
    } else {
        // Informational (gray) badges are always visible when count > 0
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_state() -> AlertState {
        AlertState::default()
    }

    #[test]
    fn badge_visible_hidden_when_count_zero() {
        // count=0 → hidden regardless of attention
        assert!(!badge_visible_with_state(&fresh_state(), "systems", 0, true));
        assert!(!badge_visible_with_state(&fresh_state(), "systems", 0, false));
    }

    #[test]
    fn badge_visible_attention_badge_hidden_after_ack() {
        let mut state = fresh_state();
        state.acknowledged.insert("flakes".to_string());
        assert!(!badge_visible_with_state(&state, "flakes", 3, true));
    }

    #[test]
    fn badge_visible_attention_badge_shown_before_ack() {
        let state = fresh_state();
        assert!(badge_visible_with_state(&state, "flakes", 3, true));
    }

    #[test]
    fn badge_visible_informational_badge_always_shown() {
        let mut state = fresh_state();
        state.acknowledged.insert("flakes".to_string());
        assert!(badge_visible_with_state(&state, "flakes", 5, false));
    }

    #[test]
    fn should_flash_fires_once_then_never() {
        let mut state = fresh_state();
        assert!(should_flash_with_state(&mut state, "flakes", true));
        assert!(!should_flash_with_state(&mut state, "flakes", true));
        assert!(!should_flash_with_state(&mut state, "flakes", true));
    }

    #[test]
    fn should_flash_false_when_no_attention() {
        let mut state = fresh_state();
        assert!(!should_flash_with_state(&mut state, "systems", false));
    }

    // Pure helpers for testing (take state explicitly, no GlobalSignal needed)
    fn badge_visible_with_state(
        state: &AlertState,
        view_key: &str,
        count: i64,
        attention: bool,
    ) -> bool {
        if count <= 0 {
            return false;
        }
        if attention {
            !state.acknowledged.contains(view_key)
        } else {
            true
        }
    }

    fn should_flash_with_state(
        state: &mut AlertState,
        view_key: &str,
        has_attention: bool,
    ) -> bool {
        if !has_attention {
            return false;
        }
        if state.flashed.contains(view_key) {
            return false;
        }
        state.flashed.insert(view_key.to_string());
        true
    }
}
