//! Modal/drawer for viewing real-time evaluation logs via WebSocket.

use dioxus::prelude::*;

use crate::hooks::websocket::{use_websocket_logs, ConnectionState};
use crate::theme;

#[component]
pub fn EvalLogModal(commit_id: i32, commit_hash: String, on_close: EventHandler<()>) -> Element {
    // Connect to eval log WebSocket
    let (logs, connection_state, _reconnect) = use_websocket_logs(&commit_id.to_string());

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 z-50 bg-black/70 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal panel
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl w-full max-w-4xl max-h-[80vh] flex flex-col",
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
                    class: "flex-1 overflow-auto p-4 bg-gray-950",
                    pre {
                        class: "text-xs font-mono text-gray-200 whitespace-pre-wrap",
                        if logs.read().is_empty() {
                            "Waiting for evaluation logs..."
                        } else {
                            for line in logs.read().iter() {
                                "{line}\n"
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "p-4 border-t border-gray-700 flex justify-end",
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
