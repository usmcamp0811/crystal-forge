//! Renders the evaluation Log tab shared by the Queue and History drawers.
//!
//! The pane owns log retrieval, polling, Auto-scroll, and the distinction
//! between loading, an empty log, and a failed request. Callers own the drawer
//! chrome and the text shown for an empty log.

use dioxus::prelude::*;

use crate::api::client::ApiClientError;
use crate::api::models::EvalLogEntry;
use crate::components::{Icon, IconName};

/// DOM id of the scrollable log viewport.
///
/// At most one Log tab exists at a time, so a fixed id locates the viewport
/// for Auto-scroll without keeping an element handle.
const LOG_VIEWPORT_ID: &str = "ed-log-viewport";

/// Describes what the log viewport shows for one retrieval result.
///
/// The three non-line states are mutually exclusive. A failed request MUST NOT
/// render as an empty log, because an empty log is a successful answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LogView {
    /// No response has arrived yet for this tab.
    Loading,
    /// The request failed. The summary never contains a response body.
    Failed(String),
    /// The request succeeded. The vector can be empty.
    Lines(Vec<String>),
}

/// Maps one retrieval result to the state the viewport renders.
///
/// `None` means the request is still in flight. Entries keep server order.
pub(super) fn log_view(snapshot: Option<&Result<Vec<EvalLogEntry>, ApiClientError>>) -> LogView {
    match snapshot {
        None => LogView::Loading,
        Some(Err(error)) => LogView::Failed(failure_summary(error)),
        Some(Ok(entries)) => LogView::Lines(entries.iter().map(format_line).collect()),
    }
}

/// Summarizes a failed log request without echoing the response body.
///
/// The body can carry server-internal detail, so only the status code or the
/// failure class reaches the page.
fn failure_summary(error: &ApiClientError) -> String {
    match error {
        ApiClientError::Status { code, .. } if matches!(code, 401 | 403) => {
            format!("You are not authorized to read this evaluation's log (HTTP {code}).")
        }
        ApiClientError::Status { code, .. } => {
            format!("The server could not return the log (HTTP {code}).")
        }
        ApiClientError::Network(_) => "The log request could not reach the server.".to_string(),
        ApiClientError::Deserialize(_) => {
            "The server returned a log that could not be read.".to_string()
        }
    }
}

fn format_line(entry: &EvalLogEntry) -> String {
    let level = entry
        .level
        .as_ref()
        .map(|level| level.to_uppercase())
        .unwrap_or_else(|| "INFO".to_string());
    format!(
        "{} [{}] {}",
        entry.timestamp.format("%Y-%m-%d %H:%M:%S%.3fZ"),
        level,
        entry.message
    )
}

/// Scrolls the log viewport to its end after the next paint.
///
/// An effect can run before the renderer has patched new lines into the DOM.
/// Reading `scrollHeight` at that moment returns the old height and leaves the
/// new lines below the fold. One animation frame later, the patch is applied.
/// Does nothing outside the browser or when the viewport is not mounted.
fn scroll_viewport_to_end() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;

        let Some(window) = web_sys::window() else {
            return;
        };
        let scroll = wasm_bindgen::closure::Closure::once_into_js(|| {
            if let Some(viewport) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(LOG_VIEWPORT_ID))
            {
                viewport.set_scroll_top(viewport.scroll_height());
            }
        });
        if window
            .request_animation_frame(scroll.unchecked_ref())
            .is_err()
        {
            // Invoke the one-shot closure so its captured state is released
            // even when a closing window cannot schedule another frame.
            let callback: &js_sys::Function = scroll.unchecked_ref();
            let _ = callback.call0(&wasm_bindgen::JsValue::NULL);
        }
    }
}

/// Shows the evaluation log for one commit and follows its tail on request.
///
/// While `live` is true the log is fetched again every two seconds. A failed
/// fetch replaces the lines with a failure notice and a **Retry** action, and
/// the next successful poll restores the lines.
///
/// When Auto-scroll is checked, the viewport moves to its end after each
/// change in the line count and when the setting is turned on. When it is
/// unchecked, new lines do not move the viewport.
///
/// `empty_message` replaces the default text for a successful empty log.
#[component]
pub(super) fn EvalLogPane(commit_id: i32, live: bool, empty_message: Option<String>) -> Element {
    let mut autoscroll = use_signal(|| true);
    let mut poll_tick = use_signal(|| 0_u64);

    use_future(move || async move {
        #[cfg(target_arch = "wasm32")]
        loop {
            // Preserve the existing two-second live-log refresh cadence for
            // both queue and history drawers.
            gloo_timers::future::TimeoutFuture::new(2000).await;
            if live {
                poll_tick.set(poll_tick() + 1);
            } else {
                break;
            }
        }
    });

    // Fetch again whenever `poll_tick` changes.
    let mut logs_resource = use_resource(move || async move {
        let _ = poll_tick();
        crate::api::client::fetch_eval_logs(commit_id).await
    });

    let view = log_view(logs_resource.read().as_ref());
    let line_count = match &view {
        LogView::Lines(lines) => lines.len(),
        LogView::Loading | LogView::Failed(_) => 0,
    };

    // Runs after the new lines are in the DOM, so the viewport height already
    // includes them. `line_count` is a plain value, not a signal. Passing it
    // through `use_reactive` is what re-runs the effect when the count changes.
    // The effect also reads `autoscroll`, so turning the setting on scrolls
    // immediately, and turning it off schedules no scroll.
    use_effect(use_reactive(&line_count, move |_| {
        if autoscroll() {
            scroll_viewport_to_end();
        }
    }));

    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;",
            div {
                style: "padding: 8px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; align-items: center; flex-shrink: 0;",
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "{line_count} lines" }
                div { style: "flex: 1;" }
                label {
                    style: "display: flex; gap: 6px; align-items: center; font-size: 11px;",
                    input {
                        r#type: "checkbox",
                        class: "ed-checkbox",
                        checked: autoscroll(),
                        oninput: move |_| autoscroll.set(!autoscroll()),
                    }
                    "Auto-scroll"
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Download",
                    Icon { name: IconName::Download, size: 13 }
                }
            }
            pre { id: LOG_VIEWPORT_ID, "data-testid": "eval-log-viewport", class: "fl-diff",
                match &view {
                    LogView::Loading => rsx! {
                        div { style: "color: var(--cf-text-muted); padding: 12px;", "Loading logs..." }
                    },
                    LogView::Failed(summary) => rsx! {
                        div {
                            role: "alert",
                            "data-testid": "eval-log-error",
                            style: "color: #f87171; padding: 12px; display: flex; gap: 10px; align-items: center; flex-wrap: wrap;",
                            span { "Evaluation log unavailable. {summary}" }
                            button {
                                class: "btn btn-ghost xs focus-ring",
                                onclick: move |_| logs_resource.restart(),
                                "Retry"
                            }
                        }
                    },
                    LogView::Lines(lines) if lines.is_empty() => rsx! {
                        div { style: "color: var(--cf-text-muted); padding: 12px;",
                            if let Some(message) = empty_message.as_deref() {
                                "{message}"
                            } else if live {
                                "Waiting for log output..."
                            } else {
                                "No logs available"
                            }
                        }
                    },
                    LogView::Lines(lines) => rsx! {
                        for (idx, line) in lines.iter().enumerate() {
                            {
                                let color = super::log_line_color(line);
                                let line_num = format!("{:>3}", idx + 1);
                                rsx! {
                                    div { style: "color: {color};",
                                        span { style: "color: var(--cf-text-muted); user-select: none; display: inline-block; width: 36px;", "{line_num}" }
                                        " {line}"
                                    }
                                }
                            }
                        }
                    },
                }
                if live && !matches!(view, LogView::Failed(_)) {
                    {
                        let next_line_num = format!("{:>3}", line_count + 1);
                        rsx! {
                            div { style: "color: #60a5fa;",
                                span { style: "color: var(--cf-text-muted); display: inline-block; width: 36px;", "{next_line_num}" }
                                " "
                                span { class: "ed-pulse", style: "margin-left: 0;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LogView, failure_summary, log_view};
    use crate::api::client::ApiClientError;
    use crate::api::models::EvalLogEntry;
    use chrono::{TimeZone, Utc};

    fn entry(sequence: i32, level: Option<&str>, message: &str) -> EvalLogEntry {
        EvalLogEntry {
            timestamp: Utc.with_ymd_and_hms(2026, 10, 17, 1, 2, 3).unwrap(),
            sequence,
            level: level.map(str::to_string),
            message: message.to_string(),
        }
    }

    #[test]
    fn pending_request_is_loading_not_empty() {
        assert_eq!(log_view(None), LogView::Loading);
    }

    #[test]
    fn successful_empty_log_is_distinct_from_a_failed_request() {
        let empty: Result<Vec<EvalLogEntry>, ApiClientError> = Ok(Vec::new());
        assert_eq!(log_view(Some(&empty)), LogView::Lines(Vec::new()));
        for error in [
            ApiClientError::Status {
                code: 503,
                body: "x".into(),
            },
            ApiClientError::Status {
                code: 403,
                body: "x".into(),
            },
            ApiClientError::Network("offline".into()),
            ApiClientError::Deserialize("bad".into()),
        ] {
            let failed: Result<Vec<EvalLogEntry>, ApiClientError> = Err(error);
            assert!(
                matches!(log_view(Some(&failed)), LogView::Failed(_)),
                "a failed request must not render as an empty log"
            );
        }
    }

    #[test]
    fn lines_keep_server_order_and_default_the_level() {
        let ok: Result<Vec<EvalLogEntry>, ApiClientError> = Ok(vec![
            entry(1, None, "first"),
            entry(2, Some("warn"), "second"),
        ]);
        assert_eq!(
            log_view(Some(&ok)),
            LogView::Lines(vec![
                "2026-10-17 01:02:03.000Z [INFO] first".to_string(),
                "2026-10-17 01:02:03.000Z [WARN] second".to_string(),
            ])
        );
    }

    #[test]
    fn failure_summary_never_echoes_the_response_body() {
        let secret = "internal-token-abc";
        for error in [
            ApiClientError::Status {
                code: 503,
                body: secret.into(),
            },
            ApiClientError::Status {
                code: 403,
                body: secret.into(),
            },
            ApiClientError::Network(secret.into()),
            ApiClientError::Deserialize(secret.into()),
        ] {
            assert!(!failure_summary(&error).contains(secret));
        }
        assert!(
            failure_summary(&ApiClientError::Status {
                code: 503,
                body: String::new()
            })
            .contains("503")
        );
    }
}
