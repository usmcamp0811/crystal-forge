//! Renders drawer header controls without owning evaluation API actions.

use chrono::{DateTime, Datelike, Timelike, Utc};
use dioxus::prelude::*;

use crate::components::{Icon, IconName};

/// Selects an authoritative completion time or a truthfully labelled queue time.
///
/// The evaluation DTO has no actual start time. Neither queue nor commit time
/// can establish when execution started.
pub(super) fn history_timestamp(
    completed_at: Option<DateTime<Utc>>,
    enqueued_at: DateTime<Utc>,
) -> (&'static str, DateTime<Utc>) {
    match completed_at {
        Some(timestamp) => ("Completed", timestamp),
        None => ("Queued", enqueued_at),
    }
}

/// Toggles the shared drawer width and exposes the current expansion state.
#[component]
pub(super) fn ExpandButton(mut maximized: Signal<bool>) -> Element {
    let label = if maximized() { "Restore" } else { "Expand" };
    rsx! {
        button {
            class: "btn-icon focus-ring",
            title: label,
            aria_label: label,
            aria_expanded: maximized(),
            onclick: move |_| maximized.set(!maximized()),
            Icon {
                name: if maximized() { IconName::Minimize } else { IconName::Maximize },
                size: 15,
            }
        }
    }
}

fn utc_dtg(timestamp: DateTime<Utc>) -> String {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    format!(
        "{:02}{:02}{:02}Z {} {:02}",
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        MONTHS[timestamp.month0() as usize],
        timestamp.year().rem_euclid(100)
    )
}

/// Displays UTC date-time-group text and relative age through escaped markup.
#[component]
pub(super) fn Timestamp(timestamp: DateTime<Utc>) -> Element {
    let dtg = utc_dtg(timestamp);
    let relative = super::format_relative_time(timestamp);
    let title = format!("{} · {relative}", timestamp.format("%b %-d, %Y, %H:%M UTC"));
    rsx! {
        span { class: "mono dtg", title,
            "{dtg}"
            span { class: "dtg-rel", " · {relative}" }
        }
    }
}

/// Requests confirmation for one commit without issuing an API request.
///
/// The caller owns submission, duplicate-request prevention, and API errors.
/// Dismissal invokes only `on_cancel`; Escape does not reach the drawer page.
#[component]
pub(super) fn ConfirmForceCancel(
    commit_id: i32,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            // The confirmation is a sibling of the tray. Use the existing
            // modal layer above drawers so both controls remain reachable.
            class: "modal-backdrop modal-backdrop-above-drawer",
            onclick: move |_| on_cancel.call(()),
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.stop_propagation();
                    on_cancel.call(());
                }
            },
            div {
                class: "modal",
                style: "max-width: 440px;",
                role: "alertdialog",
                aria_modal: "true",
                aria_labelledby: "ed-force-cancel-title",
                aria_describedby: "ed-force-cancel-description",
                onclick: move |event| event.stop_propagation(),
                div { class: "modal-head",
                    h2 { id: "ed-force-cancel-title", style: "display: flex; gap: 8px; align-items: center;",
                        span { style: "color: #f87171;", Icon { name: IconName::Warn, size: 18 } }
                        "Force-cancel evaluation?"
                    }
                    p { id: "ed-force-cancel-description",
                        "This terminates "
                        span { class: "mono", "{commit_id}" }
                        " immediately. In-flight builds may leave partial state in the Nix store. This action cannot be undone."
                    }
                }
                div { class: "modal-body", style: "padding-top: 0;",
                    div { class: "sd-callout sd-callout-danger",
                        Icon { name: IconName::Warn, size: 14 }
                        div { style: "font-size: 12px;",
                            "Prefer normal cancel — it lets in-flight derivations finish cleanly."
                        }
                    }
                }
                div { class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        autofocus: true,
                        onclick: move |_| on_cancel.call(()),
                        "Keep running"
                    }
                    button {
                        class: "btn focus-ring",
                        style: "background: #dc2626; color: white;",
                        onclick: move |_| on_confirm.call(()),
                        "Force-cancel"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{history_timestamp, utc_dtg};
    use chrono::{DateTime, Utc};

    #[test]
    fn dtg_uses_utc_date_across_offset_and_year_boundary() {
        let timestamp = DateTime::parse_from_rfc3339("2027-01-01T00:05:00+02:00")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(utc_dtg(timestamp), "312205Z DEC 26");
    }

    #[test]
    fn dtg_preserves_leap_day_and_zero_padding() {
        let timestamp = DateTime::parse_from_rfc3339("2024-02-29T01:02:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(utc_dtg(timestamp), "290102Z FEB 24");
    }

    #[test]
    fn history_uses_completion_and_labels_missing_completion_as_queued() {
        let queued = DateTime::parse_from_rfc3339("2026-10-15T23:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let completed = DateTime::parse_from_rfc3339("2026-10-16T00:01:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            history_timestamp(Some(completed), queued),
            ("Completed", completed)
        );
        assert_eq!(history_timestamp(None, queued), ("Queued", queued));
    }
}
