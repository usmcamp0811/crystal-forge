//! Systems list view — table of all registered NixOS systems.

use dioxus::prelude::*;

/// The systems list page.
#[component]
pub fn SystemsView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_systems()
    rsx! {
        div {
            class: "p-8",
            div {
                class: "flex items-center justify-between mb-6",
                h1 {
                    class: "text-2xl font-bold",
                    "Systems"
                }
                // Search / filter controls placeholder
                div {
                    class: "flex items-center gap-3",
                    input {
                        class: "bg-gray-900 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-300 placeholder-gray-600 focus:outline-none focus:border-blue-500",
                        r#type: "text",
                        placeholder: "Search systems...",
                    }
                }
            }

            // Table placeholder
            div {
                class: "bg-gray-900 border border-gray-800 rounded-xl overflow-hidden",
                table {
                    class: "w-full",
                    thead {
                        class: "bg-gray-800/50",
                        tr {
                            th { class: "px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider", "Hostname" }
                            th { class: "px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider", "Health" }
                            th { class: "px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider", "Deployment" }
                            th { class: "px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider", "Environment" }
                            th { class: "px-6 py-3 text-left text-xs font-medium text-gray-400 uppercase tracking-wider", "Last Seen" }
                        }
                    }
                    tbody {
                        class: "divide-y divide-gray-800",
                        tr {
                            td {
                                class: "px-6 py-8 text-center text-gray-500 text-sm",
                                colspan: "5",
                                "Connect to API to view systems."
                            }
                        }
                    }
                }
            }
        }
    }
}
