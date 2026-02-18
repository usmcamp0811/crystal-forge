//! Diff viewer component.
//!
//! Displays unified diff output with syntax highlighting for
//! added, removed, and context lines.

use dioxus::prelude::*;

/// Renders a unified diff with color-coded lines.
#[component]
pub fn DiffViewer(diff: String) -> Element {
    rsx! {
        div {
            class: "text-xs font-mono text-gray-300 bg-gray-950 p-3 rounded-lg overflow-x-auto whitespace-pre",
            for line in diff.lines() {
                {
                    let class = if line.starts_with("+++") || line.starts_with("---") {
                        "text-gray-400"
                    } else if line.starts_with("@@") {
                        "text-purple-300"
                    } else if line.starts_with("+") {
                        "text-emerald-300"
                    } else if line.starts_with("-") {
                        "text-red-300"
                    } else if line.starts_with("diff --git") || line.starts_with("index ") {
                        "text-blue-300"
                    } else {
                        "text-gray-300"
                    };
                    rsx! {
                        div { class: "{class}", "{line}" }
                    }
                }
            }
        }
    }
}
