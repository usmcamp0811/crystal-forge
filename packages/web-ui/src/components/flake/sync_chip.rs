//! FlakeSyncChip — status chip for a flake's sync state.
//!
//! Mirrors FlakeSyncChip from FlakesView.jsx (~line 496):
//! synced (green) / syncing (blue) / error (red) chip with dot,
//! `title` attribute = last_sync_error on hover.

use dioxus::prelude::*;

/// Display a sync-status chip for a flake.
///
/// `sync_status` values: "unknown" | "synced" | "syncing" | "error"
#[component]
pub fn FlakeSyncChip(
    sync_status: String,
    #[props(default)]
    last_sync_error: Option<String>,
) -> Element {
    let (chip_class, dot_color, label) = match sync_status.as_str() {
        "synced" => ("chip chip-healthy", "#34d399", "synced"),
        "syncing" => ("chip chip-info", "#60a5fa", "syncing"),
        "error" => ("chip chip-critical", "#f87171", "error"),
        _ => ("chip chip-unknown", "#6b7280", "unknown"),
    };

    let title = last_sync_error.as_deref().unwrap_or("").to_string();

    rsx! {
        span {
            class: "{chip_class}",
            title: "{title}",
            span {
                class: "chip-dot",
                style: "background: {dot_color};",
            }
            "{label}"
        }
    }
}
