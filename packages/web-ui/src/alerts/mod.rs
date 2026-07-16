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
use gloo_storage::{LocalStorage, Storage};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// LocalStorage key prefix for per-item dismissals.  The current user's ID
/// is appended (e.g. `"cf.alert.dismissed.abc123"`) so dismissals are
/// isolated per user when multiple accounts share the same browser profile.
const DISMISSED_STORAGE_KEY_PREFIX: &str = "cf.alert.dismissed.";

/// Shared alert state.  Hold in a `GlobalSignal` initialised in `main.rs`.
#[derive(Debug, Clone, Default)]
pub struct AlertState {
    /// Views whose attention badge has been dismissed this page load.
    pub acknowledged: HashSet<String>,
    /// Views that have already had their one-shot attention flash fired.
    pub flashed: HashSet<String>,
    /// Individual attention rows/cards dismissed after the user opens/clicks them.
    pub dismissed_items: HashSet<String>,
    /// Last acknowledgement payload sent per view key.  This prevents render or
    /// signal feedback loops from repeatedly POSTing the same acknowledgement
    /// and hammering user_alert_acknowledgments.
    pub last_ack_payloads: HashMap<String, String>,
    /// Unix-second timestamp of the last optimistic zero for each view key.
    /// Used by the sidebar poll to skip overwriting a recently zeroed badge
    /// field even if `acknowledged` was populated concurrently with the GET.
    pub zeroed_at: HashMap<String, u64>,
    /// LocalStorage key currently loaded into `dismissed_items`.
    pub dismissed_storage_key: Option<String>,
}

/// Global singleton.  Initialised to default (empty) on startup.
pub static ALERT_STATE: GlobalSignal<AlertState> = Signal::global(AlertState::default);

/// Register the current user's ID so dismissal storage is isolated per user.
/// Call this whenever authentication changes. It replaces the in-memory
/// dismissal set with the target user's stored set so local-login and logout
/// transitions do not share the anonymous namespace.
pub fn set_current_user_id(user_id: &str) {
    load_dismissals_for_storage_key(storage_key_for_user(Some(user_id)));
}

pub fn clear_current_user_id() {
    load_dismissals_for_storage_key(storage_key_for_user(None));
}

/// The LocalStorage key for dismissed items scoped to the current user.
/// Falls back to the suffix-less key for unauthenticated page loads (e.g.
/// the login page) so dismissals are at least stored rather than dropped.
fn dismissed_storage_key() -> String {
    ALERT_STATE
        .read()
        .dismissed_storage_key
        .clone()
        .unwrap_or_else(|| storage_key_for_user(None))
}

fn storage_key_for_user(user_id: Option<&str>) -> String {
    match user_id {
        Some(uid) => format!("{DISMISSED_STORAGE_KEY_PREFIX}{uid}"),
        None => format!("{DISMISSED_STORAGE_KEY_PREFIX}anon"),
    }
}

fn load_dismissals_for_storage_key(storage_key: String) {
    let stored = LocalStorage::get::<Vec<String>>(&storage_key).unwrap_or_default();
    let mut state = ALERT_STATE.write();
    state.dismissed_items = stored.into_iter().collect();
    state.dismissed_storage_key = Some(storage_key);
}

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
    // Capture the observation cursor and fingerprint from the badge snapshot
    // the user was shown before we zero-out the field, so the server anchors
    // last_seen_at to the rendered data (not POST receive time) and records
    // the exact alerting-ID set for replacement-failure detection.
    let (observed_at, fingerprint) = {
        let badges = NAV_BADGES.read_unchecked();
        let fp = match view_key {
            "systems" => badges.systems_fingerprint.clone(),
            "environments" => badges.environments_fingerprint.clone(),
            _ => None,
        };
        (badges.observed_at.clone(), fp)
    };
    let Some(observed_at) = observed_at else {
        // New clients must not acknowledge without a server cursor.  Otherwise
        // the server would have to fall back to NOW(), which can consume
        // failures that arrived before the relevant view data was rendered.
        return;
    };
    acknowledge_with_cursor_and_ids(view_key, current_count, observed_at, fingerprint, None);
}

/// Acknowledge using a cursor captured from the relevant rendered dataset.
/// Prefer this over [`acknowledge`] from views that have their own async data
/// loading; this prevents a later sidebar poll cursor from acknowledging data
/// that was not present in the rendered view.
pub fn acknowledge_with_cursor(view_key: &str, current_count: i64, observed_at: Option<String>) {
    {
        let mut state = ALERT_STATE.write();
        state.acknowledged.insert(view_key.to_string());
    }
    zero_nav_badge_field(view_key);

    let observed_at = observed_at.or_else(|| NAV_BADGES.read_unchecked().observed_at.clone());
    let Some(observed_at) = observed_at else {
        return;
    };
    acknowledge_with_cursor_and_ids(view_key, current_count, observed_at, None, None);
}

/// Acknowledge using a view-owned cursor and optional alerting IDs.  The IDs
/// are used by systems/environments so the server computes `current - seen`
/// rather than re-surfacing old alerts on recovery-only set changes.
pub fn acknowledge_with_cursor_and_ids(
    view_key: &str,
    current_count: i64,
    observed_at: String,
    fingerprint: Option<String>,
    alert_ids: Option<Vec<String>>,
) {
    let payload_key = acknowledgement_payload_key(
        current_count,
        observed_at.as_str(),
        fingerprint.as_deref(),
        alert_ids.as_deref(),
    );
    {
        let mut state = ALERT_STATE.write();
        if state
            .last_ack_payloads
            .get(view_key)
            .is_some_and(|last| last == &payload_key)
        {
            return;
        }
        state.acknowledged.insert(view_key.to_string());
        state
            .last_ack_payloads
            .insert(view_key.to_string(), payload_key);
    }
    zero_nav_badge_field(view_key);
    let view_key = view_key.to_string();
    spawn(async move {
        if acknowledge_navigation_category(
            &view_key,
            observed_at.as_str(),
            current_count,
            fingerprint.as_deref(),
            alert_ids.as_deref(),
        )
        .await
        .is_ok()
        {
            if let Ok(fresh) = get_navigation_badges().await {
                *NAV_BADGES.write() = fresh;
            }
        }
    });
}

fn acknowledgement_payload_key(
    current_count: i64,
    observed_at: &str,
    fingerprint: Option<&str>,
    alert_ids: Option<&[String]>,
) -> String {
    let mut ids = alert_ids.map(|ids| ids.to_vec()).unwrap_or_default();
    ids.sort();
    format!(
        "count={current_count};cursor={observed_at};fingerprint={};ids={}",
        fingerprint.unwrap_or(""),
        ids.join(",")
    )
}

/// Returns the current unix-second timestamp, or 0 if unavailable (WASM
/// `SystemTime` may not be available in all runtimes).
fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// Optimistically zero the `NAV_BADGES` field for `view_key` so the badge
/// hides immediately on acknowledge, without waiting for the network
/// round-trip. Also records the zero timestamp so the sidebar poll can skip
/// overwriting this field during a brief grace window.
/// See [`acknowledge`].
fn zero_nav_badge_field(view_key: &str) {
    {
        let mut state = ALERT_STATE.write();
        state.zeroed_at.insert(view_key.to_string(), now_unix_secs());
    }
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

/// Returns `true` if the badge for `view_key` was zeroed within the last
/// `grace_secs` seconds.  Used by the sidebar poll to avoid overwriting a
/// recently hidden badge before the POST /acknowledge round-trip completes.
pub fn badge_recently_zeroed(view_key: &str, grace_secs: u64) -> bool {
    let state = ALERT_STATE.read();
    match state.zeroed_at.get(view_key) {
        Some(&ts) => now_unix_secs().saturating_sub(ts) < grace_secs,
        None => false,
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

/// Load persisted dismissed-item keys from LocalStorage into [`ALERT_STATE`]
/// for the current storage namespace if it has not already been loaded.
fn ensure_dismissed_loaded() {
    let key = dismissed_storage_key();
    if ALERT_STATE.read().dismissed_storage_key.as_deref() != Some(key.as_str()) {
        load_dismissals_for_storage_key(key);
    }
}

/// Dismiss a specific attention row/card after the user clicks or opens it.
///
/// The dismissal is persisted to LocalStorage so it survives page refresh.
/// The highlight will only reappear if the underlying cause resolves and
/// returns (a genuinely new event).
pub fn dismiss_attention_item(view_key: &str, item_key: &str) {
    ensure_dismissed_loaded();
    let key = attention_item_key(view_key, item_key);
    {
        let mut state = ALERT_STATE.write();
        state.dismissed_items.insert(key.clone());
    }
    // Persist to LocalStorage — best-effort, ignore storage errors silently.
    let storage_key = dismissed_storage_key();
    if let Ok(mut stored) = LocalStorage::get::<Vec<String>>(&storage_key) {
        stored.push(key);
        let _ = LocalStorage::set(&storage_key, stored);
    } else {
        let _ = LocalStorage::set(&storage_key, vec![key]);
    }
}

/// Returns true while a specific row/card should remain highlighted.
pub fn attention_item_active(view_key: &str, item_key: &str, has_attention: bool) -> bool {
    if !has_attention {
        return false;
    }
    ensure_dismissed_loaded();
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
