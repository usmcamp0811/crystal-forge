//! WebSocket hooks for real-time build logs and system metrics.
//!
//! # Connection ownership
//!
//! Each hook owns exactly one connection through a [`SocketSlot`]. The slot
//! holds an [`OwnedSocket`]: the browser socket and the four callbacks that the
//! socket calls. Replacing the connection, closing it, or unmounting the hook
//! drops the previous [`OwnedSocket`], which detaches its handlers, closes the
//! socket and releases its callbacks together with the signals they captured.
//! No callback is forgotten.
//!
//! Every connection carries a generation number. A callback ignores its events
//! unless its generation is still the slot's current one, so a connection that
//! was replaced can never change the state of its replacement.
//!
//! # Call sites
//!
//! At the time of writing only `use_websocket_eval_stream` has a caller,
//! `EvalLogModal`, and that component is not rendered by any view. The other
//! hooks have no caller. They share this ownership model so that a later caller
//! inherits it.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
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

/// Holds the single live connection of one hook and the generation that
/// identifies it.
///
/// The slot is generic so that the ownership rules are tested without a
/// browser. Dropping the slot drops the connection.
pub(crate) struct SocketSlot<T> {
    current: RefCell<Option<T>>,
    generation: Cell<u64>,
}

impl<T> SocketSlot<T> {
    /// Creates an empty slot.
    pub(crate) fn new() -> Self {
        Self {
            current: RefCell::new(None),
            generation: Cell::new(0),
        }
    }

    /// Replaces the connection.
    ///
    /// The previous connection is made stale and dropped first, so it is closed
    /// before the replacement exists. `connect` receives the new generation and
    /// returns `None` when the connection cannot be created; the slot is then
    /// empty.
    pub(crate) fn replace(&self, connect: impl FnOnce(u64) -> Option<T>) -> u64 {
        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        // Drop outside the borrow: dropping may run arbitrary teardown.
        let previous = self.current.borrow_mut().take();
        drop(previous);
        let next = connect(generation);
        *self.current.borrow_mut() = next;
        generation
    }

    /// Returns whether `generation` identifies the current connection.
    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation.get() == generation
    }

    /// Drops the current connection and makes every earlier generation stale.
    /// Closing an empty slot does nothing.
    pub(crate) fn close(&self) {
        self.generation.set(self.generation.get() + 1);
        let previous = self.current.borrow_mut().take();
        drop(previous);
    }

    /// Drops the connection when `generation` is still current. A connection
    /// that was already replaced is left alone.
    pub(crate) fn release_if_current(&self, generation: u64) {
        if self.is_current(generation) {
            let previous = self.current.borrow_mut().take();
            drop(previous);
        }
    }
}

/// A browser WebSocket together with the callbacks it owns.
pub(crate) struct OwnedSocket {
    ws: WebSocket,
    _onopen: Closure<dyn FnMut(JsValue)>,
    _onmessage: Closure<dyn FnMut(MessageEvent)>,
    _onerror: Closure<dyn FnMut(ErrorEvent)>,
    _onclose: Closure<dyn FnMut(CloseEvent)>,
}

impl Drop for OwnedSocket {
    fn drop(&mut self) {
        // Detach first, so the browser cannot call a callback that is about to
        // be freed. The callbacks are freed after this body runs.
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

type Slot = Rc<SocketSlot<OwnedSocket>>;

/// Owns one connection slot for the calling component and closes it on unmount.
fn use_socket_slot() -> Slot {
    let slot = use_hook(|| Rc::new(SocketSlot::new()));
    use_drop({
        let slot = Rc::clone(&slot);
        move || slot.close()
    });
    slot
}

/// Opens a WebSocket for `url` and installs it in `slot`.
///
/// `label` names the stream in console messages. `on_text` receives each text
/// frame. Callbacks act only while their connection is the slot's current one.
/// When the server closes the connection, the slot releases it after the close
/// callback returns: a callback must not free itself while it runs.
fn open_socket(
    slot: &Slot,
    url: &str,
    label: &'static str,
    mut connection_state: Signal<ConnectionState>,
    mut on_text: impl FnMut(String) + 'static,
) {
    connection_state.set(ConnectionState::Connecting);
    let weak: Weak<SocketSlot<OwnedSocket>> = Rc::downgrade(slot);
    let is_current = move |weak: &Weak<SocketSlot<OwnedSocket>>, generation: u64| {
        weak.upgrade()
            .is_some_and(|slot| slot.is_current(generation))
    };

    slot.replace(|generation| {
        let ws = match WebSocket::new(url) {
            Ok(ws) => ws,
            Err(error) => {
                let message = format!("Failed to create WebSocket: {error:?}");
                web_sys::console::error_1(&message.clone().into());
                connection_state.set(ConnectionState::Error(message));
                return None;
            }
        };

        let onopen = {
            let weak = weak.clone();
            Closure::wrap(Box::new(move |_event: JsValue| {
                if is_current(&weak, generation) {
                    web_sys::console::log_1(&format!("{label} connected").into());
                    connection_state.set(ConnectionState::Connected);
                }
            }) as Box<dyn FnMut(JsValue)>)
        };
        let onmessage = {
            let weak = weak.clone();
            Closure::wrap(Box::new(move |event: MessageEvent| {
                if !is_current(&weak, generation) {
                    return;
                }
                if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
                    on_text(text.into());
                }
            }) as Box<dyn FnMut(MessageEvent)>)
        };
        let onerror = {
            let weak = weak.clone();
            Closure::wrap(Box::new(move |event: ErrorEvent| {
                if is_current(&weak, generation) {
                    let message = format!("WebSocket error: {}", event.message());
                    web_sys::console::error_1(&message.clone().into());
                    connection_state.set(ConnectionState::Error(message));
                }
            }) as Box<dyn FnMut(ErrorEvent)>)
        };
        let onclose = {
            let weak = weak.clone();
            Closure::wrap(Box::new(move |event: CloseEvent| {
                if !is_current(&weak, generation) {
                    return;
                }
                let message = format!(
                    "WebSocket closed: code={}, reason={}",
                    event.code(),
                    event.reason()
                );
                web_sys::console::log_1(&message.into());
                connection_state.set(ConnectionState::Disconnected);
                // Release the callbacks of this finished connection after this
                // callback has returned.
                let weak = weak.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(slot) = weak.upgrade() {
                        slot.release_if_current(generation);
                    }
                });
            }) as Box<dyn FnMut(CloseEvent)>)
        };

        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        Some(OwnedSocket {
            ws,
            _onopen: onopen,
            _onmessage: onmessage,
            _onerror: onerror,
            _onclose: onclose,
        })
    });
}

/// Builds the text handler of a build-job stream.
fn build_stream_handler(
    mut logs: Signal<Vec<String>>,
    mut metrics: Option<Signal<Option<SystemMetrics>>>,
) -> impl FnMut(String) {
    move |message| match serde_json::from_str::<BuildStreamMessage>(&message) {
        Ok(BuildStreamMessage::Metrics {
            cpu_percent,
            ram_used_mb,
            ram_total_mb,
            timestamp,
        }) => {
            if let Some(metrics) = metrics.as_mut() {
                metrics.set(Some(SystemMetrics {
                    cpu_percent,
                    ram_used_mb,
                    ram_total_mb,
                    timestamp,
                }));
            }
        }
        Ok(BuildStreamMessage::Log { message }) => {
            push_bounded_log(&mut logs.write(), message);
        }
        Ok(BuildStreamMessage::Error { message }) => {
            push_bounded_log(&mut logs.write(), format!("[stream-error] {}", message));
        }
        Err(_) => {
            // Temporary backward compatibility with older plain-text streams.
            push_bounded_log(&mut logs.write(), message);
        }
    }
}

/// Builds the text handler of an evaluation stream.
fn eval_stream_handler(
    mut logs: Signal<Vec<String>>,
    mut system_status: Signal<std::collections::HashMap<String, SystemEvalStatus>>,
) -> impl FnMut(String) {
    move |message| {
        match serde_json::from_str::<EvalLogMessage>(&message) {
            Ok(EvalLogMessage::Log { message: log_msg }) => {
                push_bounded_log(&mut logs.write(), log_msg);
            }
            Ok(EvalLogMessage::SystemStatus {
                system,
                status,
                error,
            }) => {
                // Update system status map
                system_status.write().insert(system.clone(), status.clone());

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
                push_bounded_log(&mut logs.write(), log_line);
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
                push_bounded_log(&mut logs.write(), log_line);
            }
            Err(_) => {
                // Fallback: treat as plain text log (for backward compatibility)
                push_bounded_log(&mut logs.write(), message);
            }
        }
    }
}

/// Internal function to establish a build-job WebSocket connection.
fn connect_websocket(
    slot: &Slot,
    job_id: &str,
    logs: Signal<Vec<String>>,
    connection_state: Signal<ConnectionState>,
    metrics: Option<Signal<Option<SystemMetrics>>>,
) {
    let (protocol, host) = websocket_base();
    let url = format!("{protocol}://{host}/api/v1/build-jobs/{job_id}/logs/stream");
    open_socket(
        slot,
        &url,
        "WebSocket",
        connection_state,
        build_stream_handler(logs, metrics),
    );
}

/// Internal function to establish an evaluation-log WebSocket connection.
fn connect_eval_websocket(
    slot: &Slot,
    commit_id: &str,
    logs: Signal<Vec<String>>,
    system_status: Signal<std::collections::HashMap<String, SystemEvalStatus>>,
    connection_state: Signal<ConnectionState>,
) {
    let (protocol, host) = websocket_base();
    let url = format!("{protocol}://{host}/api/v1/commits/{commit_id}/eval/stream");
    open_socket(
        slot,
        &url,
        "Eval WebSocket",
        connection_state,
        eval_stream_handler(logs, system_status),
    );
}

/// Hook for streaming build logs via WebSocket.
///
/// The hook owns its connection and closes it on unmount.
/// Returns (logs, connection_state, reconnect_fn).
pub fn use_websocket_logs(
    job_id: &str,
) -> (Signal<Vec<String>>, Signal<ConnectionState>, Rc<dyn Fn()>) {
    let logs = use_signal(Vec::<String>::new);
    let connection_state = use_signal(|| ConnectionState::Disconnected);
    let slot = use_socket_slot();
    let job_id = job_id.to_string();

    // Reconnect function
    let reconnect = use_hook({
        let (slot, job_id) = (Rc::clone(&slot), job_id.clone());
        move || {
            Rc::new(move || connect_websocket(&slot, &job_id, logs, connection_state, None))
                as Rc<dyn Fn()>
        }
    });

    // Auto-connect on mount
    use_effect(move || connect_websocket(&slot, &job_id, logs, connection_state, None));

    (logs, connection_state, reconnect)
}

/// Hook for streaming eval logs with per-system status tracking.
///
/// The hook owns its connection, replaces it when the commit changes, and
/// closes it on unmount.
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
    let slot = use_socket_slot();

    // Reconnect function
    let reconnect = {
        let slot = Rc::clone(&slot);
        Rc::new(move || {
            let commit_id = commit_id_signal();
            if commit_id.is_empty() || commit_id == "0" {
                return;
            }
            connect_eval_websocket(&slot, &commit_id, logs, system_status, connection_state);
        })
    };

    // Auto-connect whenever selected commit changes.
    use_effect(move || {
        let commit_id = commit_id_signal();
        if commit_id.is_empty() || commit_id == "0" {
            slot.close();
            connection_state.set(ConnectionState::Disconnected);
            return;
        }

        logs.set(Vec::new());
        system_status.set(std::collections::HashMap::new());
        connect_eval_websocket(&slot, &commit_id, logs, system_status, connection_state);
    });

    (logs, system_status, connection_state, reconnect)
}

/// Hook for streaming system metrics via WebSocket.
///
/// The hook owns its connection and closes it on unmount.
/// Returns (metrics, connection_state, reconnect_fn).
pub fn use_websocket_metrics(
    job_id: &str,
) -> (
    Signal<Option<SystemMetrics>>,
    Signal<ConnectionState>,
    Rc<dyn Fn()>,
) {
    let metrics = use_signal(|| None);
    let connection_state = use_signal(|| ConnectionState::Disconnected);
    let slot = use_socket_slot();
    let job_id = job_id.to_string();

    // Reconnect function
    let reconnect = use_hook({
        let (slot, job_id) = (Rc::clone(&slot), job_id.clone());
        move || {
            Rc::new(move || {
                connect_websocket(
                    &slot,
                    &job_id,
                    Signal::default(),
                    connection_state,
                    Some(metrics),
                )
            }) as Rc<dyn Fn()>
        }
    });

    // Auto-connect on mount
    use_effect(move || {
        connect_websocket(
            &slot,
            &job_id,
            Signal::default(),
            connection_state,
            Some(metrics),
        )
    });

    (metrics, connection_state, reconnect)
}

/// Combined hook for both logs and metrics.
///
/// The hook owns its connection and closes it on unmount.
/// Returns (logs, metrics, connection_state, reconnect_fn).
pub fn use_websocket_build_stream(
    job_id: &str,
) -> (
    Signal<Vec<String>>,
    Signal<Option<SystemMetrics>>,
    Signal<ConnectionState>,
    Rc<dyn Fn()>,
) {
    let logs = use_signal(Vec::<String>::new);
    let metrics = use_signal(|| None);
    let connection_state = use_signal(|| ConnectionState::Disconnected);
    let slot = use_socket_slot();
    let job_id = job_id.to_string();

    // Reconnect function
    let reconnect = use_hook({
        let (slot, job_id) = (Rc::clone(&slot), job_id.clone());
        move || {
            Rc::new(move || {
                connect_websocket(&slot, &job_id, logs, connection_state, Some(metrics))
            }) as Rc<dyn Fn()>
        }
    });

    // Auto-connect on mount
    use_effect(move || connect_websocket(&slot, &job_id, logs, connection_state, Some(metrics)));

    (logs, metrics, connection_state, reconnect)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records when it is created and dropped, in order.
    struct Probe {
        name: &'static str,
        events: Rc<RefCell<Vec<String>>>,
    }

    impl Probe {
        fn open(name: &'static str, events: &Rc<RefCell<Vec<String>>>) -> Option<Probe> {
            events.borrow_mut().push(format!("open {name}"));
            Some(Probe {
                name,
                events: Rc::clone(events),
            })
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            self.events
                .borrow_mut()
                .push(format!("close {}", self.name));
        }
    }

    fn events() -> Rc<RefCell<Vec<String>>> {
        Rc::new(RefCell::new(Vec::new()))
    }

    #[test]
    fn replacing_closes_the_old_connection_before_opening_the_new_one() {
        let log = events();
        let slot = SocketSlot::new();
        slot.replace(|_| Probe::open("a", &log));
        slot.replace(|_| Probe::open("b", &log));
        assert_eq!(*log.borrow(), ["open a", "close a", "open b"]);
    }

    #[test]
    fn a_replaced_connection_is_stale() {
        let log = events();
        let slot = SocketSlot::new();
        let first = slot.replace(|_| Probe::open("a", &log));
        assert!(slot.is_current(first));
        let second = slot.replace(|_| Probe::open("b", &log));
        assert!(
            !slot.is_current(first),
            "late callbacks of `a` must be ignored"
        );
        assert!(slot.is_current(second));
    }

    #[test]
    fn closing_releases_the_connection_and_makes_every_generation_stale() {
        let log = events();
        let slot = SocketSlot::new();
        let generation = slot.replace(|_| Probe::open("a", &log));
        slot.close();
        assert_eq!(*log.borrow(), ["open a", "close a"]);
        assert!(!slot.is_current(generation));
        // Closing again, or after unmount, is harmless and releases nothing twice.
        slot.close();
        assert_eq!(log.borrow().len(), 2);
    }

    #[test]
    fn dropping_the_slot_on_unmount_releases_the_connection() {
        let log = events();
        {
            let slot = SocketSlot::new();
            slot.replace(|_| Probe::open("a", &log));
        }
        assert_eq!(*log.borrow(), ["open a", "close a"]);
    }

    #[test]
    fn a_failed_connect_leaves_the_slot_empty_and_still_closes_the_old_one() {
        let log = events();
        let slot: SocketSlot<Probe> = SocketSlot::new();
        slot.replace(|_| Probe::open("a", &log));
        slot.replace(|_| None);
        assert_eq!(*log.borrow(), ["open a", "close a"]);
        // The next reconnect works.
        slot.replace(|_| Probe::open("b", &log));
        assert_eq!(log.borrow().last().map(String::as_str), Some("open b"));
    }

    #[test]
    fn a_server_close_releases_only_the_connection_it_belongs_to() {
        let log = events();
        let slot = SocketSlot::new();
        let first = slot.replace(|_| Probe::open("a", &log));
        let second = slot.replace(|_| Probe::open("b", &log));
        // The old connection's deferred release arrives late and does nothing.
        slot.release_if_current(first);
        assert_eq!(*log.borrow(), ["open a", "close a", "open b"]);
        // The current connection's own close releases its callbacks.
        slot.release_if_current(second);
        assert_eq!(log.borrow().last().map(String::as_str), Some("close b"));
        // A second release finds nothing.
        slot.release_if_current(second);
        assert_eq!(log.borrow().len(), 4);
    }

    #[test]
    fn the_stream_log_buffer_stays_bounded() {
        let mut lines = Vec::new();
        for index in 0..(MAX_STREAM_LOG_LINES + 25) {
            push_bounded_log(&mut lines, format!("line {index}"));
        }
        assert_eq!(lines.len(), MAX_STREAM_LOG_LINES);
        assert_eq!(lines.first().map(String::as_str), Some("line 25"));
        assert_eq!(
            lines.last().map(String::as_str),
            Some(format!("line {}", MAX_STREAM_LOG_LINES + 24).as_str())
        );
    }
}
