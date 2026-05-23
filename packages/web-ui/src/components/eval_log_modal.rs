//! Modal/drawer for viewing real-time evaluation logs via WebSocket.

use dioxus::prelude::*;

use crate::api::client::fetch_eval_logs;
use crate::hooks::websocket::{ConnectionState, use_websocket_eval_stream};
use crate::theme;

#[component]
pub fn EvalLogModal(
    commit_id: i32,
    commit_hash: String,
    evaluation_status: String,
    on_close: EventHandler<()>,
) -> Element {
    // Connect to eval log WebSocket with per-system status tracking
    let (ws_logs, _system_status, connection_state, _reconnect) =
        use_websocket_eval_stream(&commit_id.to_string());

    // Fetch historical logs for completed evaluations
    let historical_logs = use_resource({
        let evaluation_status = evaluation_status.clone();
        move || {
            let evaluation_status = evaluation_status.clone();
            async move {
                let status = evaluation_status.trim().to_ascii_lowercase();
                if status == "complete" || status == "failed" || status == "cancelled" {
                    fetch_eval_logs(commit_id).await.ok()
                } else {
                    None
                }
            }
        }
    });

    // Merge historical and live logs
    let logs = use_memo({
        let evaluation_status = evaluation_status.clone();
        move || {
            let mut all_logs = Vec::new();

            // Add historical logs first (if available and evaluation is complete)
            if let Some(Some(hist)) = historical_logs.read().as_ref() {
                all_logs.extend(hist.iter().map(|entry| entry.message.clone()));
            }

            // For in-progress evaluations, use websocket logs
            let status = evaluation_status.trim().to_ascii_lowercase();
            if status == "in_progress" || status == "pending" || status == "cancelling" {
                all_logs.extend(ws_logs.read().clone());
            }

            all_logs
        }
    });

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal panel
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl w-full max-w-4xl max-h-[80vh] flex flex-col",
                style: "overflow: hidden;",
                onclick: |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between p-4 border-b border-gray-700",
                    div {
                        h3 { class: "text-lg font-semibold text-white", "Evaluation Logs" }
                        p { class: "text-sm text-gray-400", "Commit: {commit_hash}" }
                    }
                    button {
                        class: "text-gray-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }

                // Connection status
                div {
                    class: "px-4 py-2 border-b border-gray-700 bg-gray-900/60",
                    div {
                        class: "flex items-center gap-2 text-xs",
                        div {
                            class: match *connection_state.read() {
                                ConnectionState::Connected => "w-2 h-2 rounded-full bg-green-500",
                                ConnectionState::Connecting => "w-2 h-2 rounded-full bg-yellow-500 animate-pulse",
                                ConnectionState::Disconnected => "w-2 h-2 rounded-full bg-gray-500",
                                ConnectionState::Error(_) => "w-2 h-2 rounded-full bg-red-500",
                            }
                        }
                        span {
                            class: "text-gray-300",
                            match connection_state.read().clone() {
                                ConnectionState::Connected => "Live streaming",
                                ConnectionState::Connecting => "Connecting...",
                                ConnectionState::Disconnected => "Disconnected",
                                ConnectionState::Error(ref e) => e.as_str(),
                            }
                        }
                    }
                }

                // Log content (scrollable)
                div {
                    class: "flex-1 min-h-0 min-w-0 overflow-auto p-4 bg-gray-950",
                    pre {
                        class: "block w-full max-w-full text-xs font-mono text-gray-200 whitespace-pre-wrap",
                        style: "white-space: pre-wrap; overflow-wrap: anywhere; word-break: break-all;",
                        if logs().is_empty() {
                            "Waiting for evaluation logs..."
                        } else {
                            for line in logs().iter() {
                                "{line}\n"
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "p-4 border-t border-gray-700 flex justify-between items-center",
                    span {
                        class: "text-sm text-gray-400",
                        "{logs().len()} lines"
                    }
                    div {
                        class: "flex gap-2",
                        button {
                            class: "px-3 py-2 rounded-lg font-medium text-sm text-gray-300 bg-gray-800 hover:bg-gray-700 border border-gray-700",
                            disabled: logs().is_empty(),
                            onclick: move |_| {
                                let log_content = logs().join("\n");
                                let filename = format!("eval-{}-{}.log", commit_id, commit_hash.chars().take(8).collect::<String>());
                                download_text_file(&log_content, &filename);
                            },
                            "⬇ Download"
                        }
                        button {
                            class: "px-4 py-2 rounded-lg font-medium text-sm text-white {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| on_close.call(()),
                            "Close"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn download_text_file(content: &str, filename: &str) {
    use wasm_bindgen::JsCast;
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            // Create blob
            let array = js_sys::Array::new();
            array.push(&wasm_bindgen::JsValue::from_str(content));

            let mut props = BlobPropertyBag::new();
            props.type_("text/plain");

            if let Ok(blob) = Blob::new_with_str_sequence_and_options(&array, &props) {
                // Create object URL
                if let Ok(url) = Url::create_object_url_with_blob(&blob) {
                    // Create temporary anchor and trigger download
                    if let Ok(anchor) = document.create_element("a") {
                        if let Ok(anchor) = anchor.dyn_into::<HtmlAnchorElement>() {
                            anchor.set_href(&url);
                            anchor.set_download(filename);
                            anchor.click();

                            // Clean up object URL
                            let _ = Url::revoke_object_url(&url);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn download_text_file(_content: &str, _filename: &str) {
    // No-op for non-wasm targets
}
