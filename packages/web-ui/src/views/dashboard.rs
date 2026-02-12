//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use dioxus::prelude::*;

use crate::components::stat_card::StatCard;

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_dashboard()
    rsx! {
        div {
            class: "p-8",
            h1 {
                class: "text-2xl font-bold mb-6",
                "Dashboard"
            }

            // Summary cards row
            div {
                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-8",
                StatCard { label: "Total Systems".to_string(), value: "--".to_string() }
                StatCard { label: "Healthy".to_string(), value: "--".to_string(), color_class: "text-emerald-400".to_string() }
                StatCard { label: "Critical".to_string(), value: "--".to_string(), color_class: "text-red-400".to_string() }
                StatCard { label: "Active Builds".to_string(), value: "--".to_string(), color_class: "text-blue-400".to_string() }
            }

            // Placeholder sections
            div {
                class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                // Fleet health breakdown
                div {
                    class: "bg-gray-900 border border-gray-800 rounded-xl p-6",
                    h2 {
                        class: "text-lg font-semibold mb-4",
                        "Fleet Health"
                    }
                    p {
                        class: "text-gray-500 text-sm",
                        "Connect to API to view fleet health breakdown."
                    }
                }

                // Recent deployments
                div {
                    class: "bg-gray-900 border border-gray-800 rounded-xl p-6",
                    h2 {
                        class: "text-lg font-semibold mb-4",
                        "Recent Deployments"
                    }
                    p {
                        class: "text-gray-500 text-sm",
                        "Connect to API to view recent deployments."
                    }
                }
            }
        }
    }
}
