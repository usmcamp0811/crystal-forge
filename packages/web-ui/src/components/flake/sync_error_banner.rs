//! FlakeSyncErrorBanner — "Sync failed" tray banner for errored flakes.
//!
//! Mirrors FlakesView.jsx lines 168-184:
//! warn icon + "Sync failed" + relative last_sync_at + "Retry sync" button
//! + pre block with the real nix error + last-good-commit and remote meta.

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

/// Human-readable relative time (matches the design's relTime helper).
fn relative_time_short(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let secs = now.signed_duration_since(*dt).num_seconds().max(0);
    if secs < 60 {
        return "just now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d ago");
    }
    let weeks = days / 7;
    format!("{weeks}w ago")
}

/// Sync-failed banner component for the flake tray.
#[component]
pub fn FlakeSyncErrorBanner(
    repo_url: String,
    last_sync_error: String,
    #[props(default)] last_sync_at: Option<DateTime<Utc>>,
    /// Latest known commit SHA (last good commit).
    #[props(default)]
    latest_commit: Option<String>,
    /// Callback to trigger a retry sync.
    on_retry: EventHandler<()>,
) -> Element {
    let when_text = last_sync_at
        .as_ref()
        .map(relative_time_short)
        .unwrap_or_default();

    let error_pre = format!(
        "$ nix flake metadata {}\nerror: {}",
        repo_url, last_sync_error
    );

    rsx! {
        div {
            class: "fl-sync-error",

            // Head row: icon + title + timestamp + retry button
            div {
                class: "fl-sync-error-head",
                svg {
                    width: "14",
                    height: "14",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    view_box: "0 0 24 24",
                    path { d: "M12 3l10 18H2L12 3z" }
                    path { d: "M12 10v5M12 18h.01" }
                }
                span { "Sync failed" }
                span { class: "fl-sync-error-when", "{when_text}" }
                span { style: "flex:1;" }
                button {
                    class: "btn btn-ghost focus-ring xs",
                    onclick: move |_| on_retry.call(()),
                    svg {
                        width: "11",
                        height: "11",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        view_box: "0 0 24 24",
                        path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                    }
                    " Retry sync"
                }
            }

            // Error pre block
            pre {
                class: "fl-sync-error-msg mono",
                "{error_pre}"
            }

            // Meta row: last good commit + remote
            div {
                class: "fl-sync-error-meta",
                span {
                    span { style: "color: var(--cf-text-muted);", "last good commit " }
                    if let Some(ref sha) = latest_commit {
                        span { class: "mono", "{sha}" }
                    } else {
                        span { style: "color: var(--cf-text-muted);", "—" }
                    }
                }
                span {
                    span { style: "color: var(--cf-text-muted);", "remote " }
                    span { class: "mono", style: "word-break: break-all;", "{repo_url}" }
                }
            }
        }
    }
}
