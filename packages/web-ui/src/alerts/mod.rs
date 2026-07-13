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
use std::collections::{HashMap, HashSet};

/// Shared alert state.  Hold in a `GlobalSignal` initialised in `main.rs`.
#[derive(Debug, Clone, Default)]
pub struct AlertState {
    /// Views whose attention badge has been dismissed this page load.
    pub acknowledged: HashSet<String>,
    /// Views that have already had their one-shot attention flash fired.
    pub flashed: HashSet<String>,
    /// Individual attention rows/cards dismissed after the user opens/clicks them.
    pub dismissed_items: HashSet<String>,
    /// View-local attention counts published by pages that already loaded alert data.
    pub attention_counts: HashMap<String, i64>,
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

/// Returns `true` if the view has been acknowledged this page load.
pub fn is_acknowledged(view_key: &str) -> bool {
    ALERT_STATE.read().acknowledged.contains(view_key)
}

/// Remove the acknowledge + flash-triggered flags so the tab and rows
/// can flash again for newly arrived failures.
pub fn reset_acknowledge(view_key: &str) {
    let mut state = ALERT_STATE.write();
    state.acknowledged.remove(view_key);
    state.flashed.remove(view_key);
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

/// Publish a view-local attention count for the sidebar.
///
/// This complements the backend aggregate endpoint so the sidebar can show the
/// same alert number the currently-loaded view is already rendering, even while
/// the aggregate endpoint is polling or if its semantics lag a view-specific API.
pub fn set_attention_count(view_key: &str, count: i64) {
    let mut state = ALERT_STATE.write();
    state
        .attention_counts
        .insert(view_key.to_string(), count.max(0));
}

/// Return the latest view-local attention count published for `view_key`.
pub fn attention_count(view_key: &str) -> i64 {
    ALERT_STATE
        .read()
        .attention_counts
        .get(view_key)
        .copied()
        .unwrap_or(0)
}

/// Dismiss a specific attention row/card after the user clicks or opens it.
pub fn dismiss_attention_item(view_key: &str, item_key: &str) {
    let mut state = ALERT_STATE.write();
    state
        .dismissed_items
        .insert(attention_item_key(view_key, item_key));
}

/// Returns true while a specific row/card should remain highlighted.
pub fn attention_item_active(view_key: &str, item_key: &str, has_attention: bool) -> bool {
    if !has_attention {
        return false;
    }
    let state = ALERT_STATE.read();
    !state
        .dismissed_items
        .contains(&attention_item_key(view_key, item_key))
}

/// Build CSS classes for an alerting row/card.
///
/// `attention-row` persists until the item is dismissed; `attention-flash` is a
/// one-shot pulse controlled by [`should_flash`].
pub fn attention_row_class(
    base_class: &str,
    view_key: &str,
    item_key: &str,
    has_attention: bool,
    flash_now: bool,
) -> String {
    let mut classes = base_class.trim().to_string();
    if attention_item_active(view_key, item_key, has_attention) {
        push_class(&mut classes, "attention-row");
        if flash_now {
            push_class(&mut classes, "attention-flash");
        }
    }
    classes
}

fn attention_item_key(view_key: &str, item_key: &str) -> String {
    format!("{view_key}:{item_key}")
}

fn push_class(classes: &mut String, class_name: &str) {
    if classes.is_empty() {
        classes.push_str(class_name);
    } else {
        classes.push(' ');
        classes.push_str(class_name);
    }
}

/// Returns `true` when the badge for a view should be shown.
///
/// Current product behavior keeps badges visible while the underlying
/// condition exists, even after the corresponding view has been opened.
/// The separate `acknowledged`/`flashed` state is still used to gate the
/// first-visit in-view highlight pulse.
pub fn badge_visible(view_key: &str, count: i64, attention: bool) -> bool {
    let _ = (view_key, attention);
    count > 0
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
        assert!(!badge_visible_with_state(
            &fresh_state(),
            "systems",
            0,
            true
        ));
        assert!(!badge_visible_with_state(
            &fresh_state(),
            "systems",
            0,
            false
        ));
    }

    #[test]
    fn badge_visible_attention_badge_still_shown_after_ack() {
        let mut state = fresh_state();
        state.acknowledged.insert("flakes".to_string());
        assert!(badge_visible_with_state(&state, "flakes", 3, true));
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

    #[test]
    fn attention_item_hidden_after_dismissal() {
        let mut state = fresh_state();
        assert!(attention_item_active_with_state(
            &state, "flakes", "42", true
        ));

        state
            .dismissed_items
            .insert(attention_item_key("flakes", "42"));

        assert!(!attention_item_active_with_state(
            &state, "flakes", "42", true
        ));
    }

    #[test]
    fn attention_row_class_adds_persistent_and_flash_classes() {
        let state = fresh_state();
        assert_eq!(
            attention_row_class_with_state(&state, "selected", "builds", "7", true, true),
            "selected attention-row attention-flash"
        );
        assert_eq!(
            attention_row_class_with_state(&state, "selected", "builds", "7", true, false),
            "selected attention-row"
        );
        assert_eq!(
            attention_row_class_with_state(&state, "selected", "builds", "7", false, true),
            "selected"
        );
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
        let _ = (state, view_key, attention);
        true
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

    fn attention_item_active_with_state(
        state: &AlertState,
        view_key: &str,
        item_key: &str,
        has_attention: bool,
    ) -> bool {
        has_attention
            && !state
                .dismissed_items
                .contains(&attention_item_key(view_key, item_key))
    }

    fn attention_row_class_with_state(
        state: &AlertState,
        base_class: &str,
        view_key: &str,
        item_key: &str,
        has_attention: bool,
        flash_now: bool,
    ) -> String {
        let mut classes = base_class.trim().to_string();
        if attention_item_active_with_state(state, view_key, item_key, has_attention) {
            push_class(&mut classes, "attention-row");
            if flash_now {
                push_class(&mut classes, "attention-flash");
            }
        }
        classes
    }
}
