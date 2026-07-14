//! Sidebar alert state — badge acknowledgment and attention flash.
//!
//! Mirrors the Shell.jsx `useAttentionFlash` / `acknowledgeView` /
//! `useAcknowledgedViews` pattern (lines 167-200), extended with server-side
//! persistence (TASK-385 follow-up) so badges reflect "new since your last
//! visit" rather than a raw total that reappears identically on every page
//! refresh.
//!
//! # How it works
//! - Each navigation view has a string key (e.g. `"flakes"`, `"systems"`).
//! - When a view with an attention badge is visited, call [`acknowledge`]
//!   with the view's current raw attention count. This immediately hides the
//!   badge locally AND persists the acknowledgment server-side (per
//!   authenticated user), so the badge stays hidden across page refresh,
//!   browser restart, and re-login — not just for the current page load.
//! - [`NAV_BADGES`] holds the latest server-computed "new since last
//!   acknowledgment" counts per category, polled every 30s by the sidebar and
//!   refreshed immediately after every [`acknowledge`] call. Views read this
//!   directly for their own badge/flash display so the number always matches
//!   what the sidebar shows and reflects the persisted, delta-based
//!   semantics — see `queries::navigation` on the server for the per-category
//!   "new since" computation.
//! - On first visit to a view that has attention items, call
//!   [`should_flash`] to get a `true` once — triggering the CSS
//!   `.attention-flash` pulse on alerting rows. Subsequent calls return `false`.
//! - A `GlobalSignal<AlertState>` is the backing store so any component that
//!   reads it re-renders when it changes.

use crate::api::client::{acknowledge_navigation_category, get_navigation_badges};
use crate::api::models::NavigationBadges;
use dioxus::prelude::*;
use std::collections::HashSet;

/// Shared alert state.  Hold in a `GlobalSignal` initialised in `main.rs`.
#[derive(Debug, Clone, Default)]
pub struct AlertState {
    /// Views whose attention badge has been dismissed this page load.
    pub acknowledged: HashSet<String>,
    /// Views that have already had their one-shot attention flash fired.
    pub flashed: HashSet<String>,
    /// Individual attention rows/cards dismissed after the user opens/clicks them.
    pub dismissed_items: HashSet<String>,
}

/// Global singleton.  Initialised to default (empty) on startup.
pub static ALERT_STATE: GlobalSignal<AlertState> = Signal::global(AlertState::default);

/// Latest polled navigation badge counts — server-computed "new since last
/// acknowledgment" per category. Populated by the sidebar's 30s poll and
/// refreshed immediately after [`acknowledge`]. Views may read this directly
/// for their own in-view badge/flash display instead of computing a raw
/// local count, so the number shown stays consistent with the sidebar and
/// correctly reflects the persisted, delta-based semantics.
pub static NAV_BADGES: GlobalSignal<NavigationBadges> = Signal::global(NavigationBadges::default);

/// Acknowledge a view — optimistically hides its attention badge immediately
/// AND persists the acknowledgment server-side (per authenticated user) so it
/// stays hidden across page refresh, browser restart, and re-login until a
/// new failure appears for that category.
///
/// `current_count` is the view's raw attention count at acknowledgment time.
/// It is used server-side as the count-diff baseline for categories with no
/// discrete per-item timestamp (systems, environments); it is ignored for
/// timestamp-based categories (flakes, builds, evals, cves), which use the
/// acknowledgment's `NOW()` as their cutoff instead — pass the best count you
/// have available regardless.
///
/// NOTE: [`NAV_BADGES`] (not `ALERT_STATE.acknowledged`) is the source of
/// truth callers should read for badge visibility. This function zeroes the
/// relevant `NAV_BADGES` field immediately for a snappy UI, then the async
/// refetch below reconciles it with the server. A category must never be
/// masked indefinitely for the rest of the page load once acknowledged — if
/// a genuinely new failure arrives afterwards, the next poll (or this
/// function's own refetch) must be able to show it again.
///
/// Call this when entering the view (on mount). For Builds/Evals, call only
/// when the failures tab is opened.
pub fn acknowledge(view_key: &str, current_count: i64) {
    {
        let mut state = ALERT_STATE.write();
        state.acknowledged.insert(view_key.to_string());
    }
    zero_nav_badge_field(view_key);
    let view_key = view_key.to_string();
    spawn(async move {
        if acknowledge_navigation_category(&view_key, current_count)
            .await
            .is_ok()
        {
            // Refresh immediately so the sidebar/tab badges reflect the new
            // baseline without waiting for the next scheduled 30s poll. This
            // also corrects the optimistic zero above if something genuinely
            // new arrived in the meantime (server remains the source of
            // truth).
            if let Ok(fresh) = get_navigation_badges().await {
                *NAV_BADGES.write() = fresh;
            }
        }
    });
}

/// Optimistically zero the `NAV_BADGES` field for `view_key` so the badge
/// hides immediately on acknowledge, without waiting for the network
/// round-trip. See [`acknowledge`].
fn zero_nav_badge_field(view_key: &str) {
    let mut badges = NAV_BADGES.write();
    match view_key {
        "systems" => badges.systems_attention = 0,
        "flakes" => badges.flakes_errored = 0,
        "environments" => badges.environments_attention = 0,
        "builds" => badges.builds_failed_new = 0,
        "evals" => badges.evals_failed_new = 0,
        "cves" => badges.cves_critical_new = 0,
        _ => {}
    }
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
