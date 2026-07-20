//! WebSocket hooks for real-time build logs and system metrics.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

fn websocket_base() -> (String, String) {
    let window = web_sys::window().expect("no global window");
    let page_protocol = window
        .location()
        .protocol()
        .unwrap_or_else(|_| "http:".to_string());
    let default_ws_protocol = if page_protocol == "https:" {
        "wss".to_string()
    } else {
        "ws".to_string()
    };

    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(origin)) = storage.get_item("cf_backend_origin") {
            let trimmed = origin.trim();
            if !trimmed.is_empty() {
                let ws_origin = if let Some(rest) = trimmed.strip_prefix("https://") {
                    format!("wss://{rest}")
                } else if let Some(rest) = trimmed.strip_prefix("http://") {
                    format!("ws://{rest}")
                } else {
                    format!("{}://{}", default_ws_protocol, trimmed)
                };

                if let Some((proto, host)) = ws_origin.split_once("://") {
                    return (proto.to_string(), host.to_string());
                }
            }
        }
    }

    let host = window
        .location()
        .host()
        .unwrap_or_else(|_| "localhost:8080".to_string());
    (default_ws_protocol, host)
}

/// System metrics sent by the builder during a build.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SystemMetrics {
    pub cpu_percent: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub timestamp: String,
}

/// Structured eval log message types
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvalLogMessage {
    Log {
        message: String,
    },
    SystemStatus {
        system: String,
        status: SystemEvalStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    EvalStatus {
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Structured build stream message types.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildStreamMessage {
    Log {
        message: String,
    },
    Metrics {
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
        timestamp: String,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SystemEvalStatus {
    Pending,
    Evaluating,
    Success,
    Failed,
    PolicyFailed,
    QueuedForBuild,
}

const MAX_STREAM_LOG_LINES: usize = 2000;

fn push_bounded_log(logs: &mut Vec<String>, line: String) {
    logs.push(line);
    if logs.len() > MAX_STREAM_LOG_LINES {
        let overflow = logs.len() - MAX_STREAM_LOG_LINES;
        logs.drain(0..overflow);
    }
}

/// WebSocket connection state.
#[derive(Clone, Debug, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Hook for streaming build logs via WebSocket.
///
/// Returns (logs, connection_state, reconnect_fn).
pub fn use_websocket_logs(
    job_id: &str,
) -> (Signal<Vec<String>>, Signal<ConnectionState>, Rc<dyn Fn()>) {
    let mut logs = use_signal(Vec::<String>::new);
    let mut connection_state = use_signal(|| ConnectionState::Disconnected);

    let job_id = job_id.to_string();

    // Reconnect function
    let reconnect = use_hook(|| {
        let job_id = job_id.clone();
        let logs = logs.clone();
        let connection_state = connection_state.clone();

        Rc::new(move || {
            connect_websocket(&job_id, logs, connection_state, None);
        })
    });

    // Auto-connect on mount
    use_effect(move || {
        let job_id = job_id.clone();
        connect_websocket(&job_id, logs, connection_state, None);
    });

    (logs, connection_state, reconnect)
}

/// Hook for streaming eval logs with per-system status tracking.
///
/// Returns (logs, system_status_map, connection_state, reconnect_fn).
pub fn use_websocket_eval_stream(
    commit_id: &str,
) -> (
    Signal<Vec<String>>,
    Signal<std::collections::HashMap<String, SystemEvalStatus>>,
    Signal<ConnectionState>,
    Rc<dyn Fn()>,
) {
    let mut commit_id_signal = use_signal(|| commit_id.to_string());
    if commit_id_signal.read().as_str() != commit_id {
        commit_id_signal.set(commit_id.to_string());
    }

    let mut logs = use_signal(Vec::<String>::new);
    let mut system_status = use_signal(std::collections::HashMap::<String, SystemEvalStatus>::new);
    let mut connection_state = use_signal(|| ConnectionState::Disconnected);
    let mut active_socket = use_signal(|| None::<WebSocket>);

    // Reconnect function
    let reconnect = {
        let commit_id_signal = commit_id_signal.clone();
        let logs = logs.clone();
        let system_status = system_status.clone();
        let connection_state = connection_state.clone();
        let active_socket = active_socket.clone();

        Rc::new(move || {
            let commit_id = commit_id_signal();
            if commit_id.is_empty() || commit_id == "0" {
                return;
            }
            connect_eval_websocket(
                &commit_id,
                logs,
                system_status,
                connection_state,
                active_socket,
            );
        })
    };

    // Auto-connect whenever selected commit changes.
    use_effect(move || {
        let commit_id = commit_id_signal();
        if commit_id.is_empty() || commit_id == "0" {
            connection_state.set(ConnectionState::Disconnected);
            return;
        }

        logs.set(Vec::new());
        system_status.set(std::collections::HashMap::new());
        connect_eval_websocket(
            &commit_id,
            logs,
            system_status,
            connection_state,
            active_socket,
        );
    });

    (logs, system_status, connection_state, reconnect)
}

/// Hook for streaming system metrics via WebSocket.
///
/// Returns (metrics, connection_state, reconnect_fn).
pub fn use_websocket_metrics(
    job_id: &str,
) -> (
    Signal<Option<SystemMetrics>>,
    Signal<ConnectionState>,
    Rc<dyn Fn()>,
) {
    let mut metrics = use_signal(|| None);
    let mut connection_state = use_signal(|| ConnectionState::Disconnected);

    let job_id = job_id.to_string();

    // Reconnect function
    let reconnect = use_hook(|| {
        let job_id = job_id.clone();
        let metrics = metrics.clone();
        let connection_state = connection_state.clone();

        Rc::new(move || {
            connect_websocket(&job_id, Signal::default(), connection_state, Some(metrics));
        })
    });

    // Auto-connect on mount
    use_effect(move || {
        let job_id = job_id.clone();
        connect_websocket(&job_id, Signal::default(), connection_state, Some(metrics));
    });

    (metrics, connection_state, reconnect)
}

/// Combined hook for both logs and metrics.
///
/// Returns (logs, metrics, connection_state, reconnect_fn).
pub fn use_websocket_build_stream(
    job_id: &str,
) -> (
    Signal<Vec<String>>,
    Signal<Option<SystemMetrics>>,
    Signal<ConnectionState>,
    Rc<dyn Fn()>,
) {
    let mut logs = use_signal(Vec::<String>::new);
    let mut metrics = use_signal(|| None);
    let mut connection_state = use_signal(|| ConnectionState::Disconnected);

    let job_id = job_id.to_string();

    // Reconnect function
    let reconnect = use_hook(|| {
        let job_id = job_id.clone();
        let logs = logs.clone();
        let metrics = metrics.clone();
        let connection_state = connection_state.clone();

        Rc::new(move || {
            connect_websocket(&job_id, logs, connection_state, Some(metrics));
        })
    });

    // Auto-connect on mount
    use_effect(move || {
        let job_id = job_id.clone();
        connect_websocket(&job_id, logs, connection_state, Some(metrics));
    });

    (logs, metrics, connection_state, reconnect)
}

/// Internal function to establish WebSocket connection.
fn connect_websocket(
    job_id: &str,
    mut logs: Signal<Vec<String>>,
    mut connection_state: Signal<ConnectionState>,
    metrics: Option<Signal<Option<SystemMetrics>>>,
) {
    connection_state.set(ConnectionState::Connecting);

    // Build WebSocket URL
    let (protocol, host) = websocket_base();
    let ws_url = format!("{protocol}://{host}/api/v1/build-jobs/{job_id}/logs/stream");

    // Create WebSocket
    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            let error_msg = format!("Failed to create WebSocket: {:?}", e);
            web_sys::console::error_1(&error_msg.clone().into());
            connection_state.set(ConnectionState::Error(error_msg));
            return;
        }
    };

    // Clone signals for closures
    let ws_clone = ws.clone();
    let mut connection_state_open = connection_state.clone();

    // onopen handler
    let onopen = Closure::wrap(Box::new(move |_event: JsValue| {
        web_sys::console::log_1(&"WebSocket connected".into());
        connection_state_open.set(ConnectionState::Connected);
    }) as Box<dyn FnMut(JsValue)>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // onmessage handler
    let mut logs_msg = logs.clone();
    let mut metrics_msg = metrics.clone();
    let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
            let message: String = text.into();

            match serde_json::from_str::<BuildStreamMessage>(&message) {
                Ok(BuildStreamMessage::Metrics {
                    cpu_percent,
                    ram_used_mb,
                    ram_total_mb,
                    timestamp,
                }) => {
                    if let Some(mut metrics_signal) = metrics_msg {
                        metrics_signal.set(Some(SystemMetrics {
                            cpu_percent,
                            ram_used_mb,
                            ram_total_mb,
                            timestamp,
                        }));
                    }
                }
                Ok(BuildStreamMessage::Log { message }) => {
                    push_bounded_log(&mut logs_msg.write(), message);
                }
                Ok(BuildStreamMessage::Error { message }) => {
                    push_bounded_log(&mut logs_msg.write(), format!("[stream-error] {}", message));
                }
                Err(_) => {
                    // Temporary backward compatibility with older plain-text streams.
                    push_bounded_log(&mut logs_msg.write(), message);
                }
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // onerror handler
    let mut connection_state_error = connection_state.clone();
    let onerror = Closure::wrap(Box::new(move |event: ErrorEvent| {
        let error_msg = format!("WebSocket error: {}", event.message());
        web_sys::console::error_1(&error_msg.clone().into());
        connection_state_error.set(ConnectionState::Error(error_msg));
    }) as Box<dyn FnMut(ErrorEvent)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    // onclose handler
    let mut connection_state_close = connection_state.clone();
    let onclose = Closure::wrap(Box::new(move |event: CloseEvent| {
        let msg = format!(
            "WebSocket closed: code={}, reason={}",
            event.code(),
            event.reason()
        );
        web_sys::console::log_1(&msg.into());
        connection_state_close.set(ConnectionState::Disconnected);
    }) as Box<dyn FnMut(CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();
}

/// Internal function to establish WebSocket connection for eval logs.
fn connect_eval_websocket(
    commit_id: &str,
    mut logs: Signal<Vec<String>>,
    mut system_status: Signal<std::collections::HashMap<String, SystemEvalStatus>>,
    mut connection_state: Signal<ConnectionState>,
    mut active_socket: Signal<Option<WebSocket>>,
) {
    if let Some(existing) = active_socket.write().take() {
        existing.set_onopen(None);
        existing.set_onmessage(None);
        existing.set_onerror(None);
        existing.set_onclose(None);
        let _ = existing.close();
    }

    connection_state.set(ConnectionState::Connecting);

    // Build WebSocket URL
    let (protocol, host) = websocket_base();
    let ws_url = format!("{protocol}://{host}/api/v1/commits/{commit_id}/eval/stream");

    // Create WebSocket
    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            let error_msg = format!("Failed to create WebSocket: {:?}", e);
            web_sys::console::error_1(&error_msg.clone().into());
            connection_state.set(ConnectionState::Error(error_msg));
            return;
        }
    };

    // Clone signals for closures
    let mut connection_state_open = connection_state.clone();

    // onopen handler
    let onopen = Closure::wrap(Box::new(move |_event: JsValue| {
        web_sys::console::log_1(&"Eval WebSocket connected".into());
        connection_state_open.set(ConnectionState::Connected);
    }) as Box<dyn FnMut(JsValue)>);
    ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
    onopen.forget();

    // onmessage handler - parses structured messages
    let mut logs_msg = logs.clone();
    let mut system_status_msg = system_status.clone();
    let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
            let message: String = text.into();

            // Try to parse as structured EvalLogMessage
            match serde_json::from_str::<EvalLogMessage>(&message) {
                Ok(EvalLogMessage::Log { message: log_msg }) => {
                    push_bounded_log(&mut logs_msg.write(), log_msg);
                }
                Ok(EvalLogMessage::SystemStatus {
                    system,
                    status,
                    error,
                }) => {
                    // Update system status map
                    system_status_msg
                        .write()
                        .insert(system.clone(), status.clone());

                    // Also add to logs for display
                    let log_line = if let Some(err) = error {
                        format!("❌ {}: {}", system, err)
                    } else {
                        match status {
                            SystemEvalStatus::Success => {
                                format!("✅ {}: evaluated successfully", system)
                            }
                            SystemEvalStatus::Failed => format!("❌ {}: evaluation failed", system),
                            SystemEvalStatus::Evaluating => format!("⏳ {}: evaluating...", system),
                            SystemEvalStatus::Pending => format!("⏸ {}: pending", system),
                            SystemEvalStatus::PolicyFailed => {
                                format!("⚠️ {}: policy failed", system)
                            }
                            SystemEvalStatus::QueuedForBuild => {
                                format!("🚀 {}: queued for build", system)
                            }
                        }
                    };
                    push_bounded_log(&mut logs_msg.write(), log_line);
                }
                Ok(EvalLogMessage::EvalStatus {
                    status,
                    message: msg,
                }) => {
                    let log_line = if let Some(m) = msg {
                        format!("📊 Eval {}: {}", status, m)
                    } else {
                        format!("📊 Eval {}", status)
                    };
                    push_bounded_log(&mut logs_msg.write(), log_line);
                }
                Err(_) => {
                    // Fallback: treat as plain text log (for backward compatibility)
                    push_bounded_log(&mut logs_msg.write(), message);
                }
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();

    // onerror handler
    let mut connection_state_error = connection_state.clone();
    let onerror = Closure::wrap(Box::new(move |event: ErrorEvent| {
        let error_msg = format!("WebSocket error: {}", event.message());
        web_sys::console::error_1(&error_msg.clone().into());
        connection_state_error.set(ConnectionState::Error(error_msg));
    }) as Box<dyn FnMut(ErrorEvent)>);
    ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();

    // onclose handler
    let mut connection_state_close = connection_state.clone();
    let onclose = Closure::wrap(Box::new(move |event: CloseEvent| {
        let msg = format!(
            "WebSocket closed: code={}, reason={}",
            event.code(),
            event.reason()
        );
        web_sys::console::log_1(&msg.into());
        connection_state_close.set(ConnectionState::Disconnected);
    }) as Box<dyn FnMut(CloseEvent)>);
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();

    active_socket.set(Some(ws));
}
