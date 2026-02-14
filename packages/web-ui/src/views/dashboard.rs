//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::components::stat_card::StatCard;
use crate::theme;

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_dashboard()
    rsx! {
        div {
            class: "space-y-8",
            div {
                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4",
                StatCard { label: "Total Systems".to_string(), value: "--".to_string() }
                StatCard { label: "Healthy".to_string(), value: "--".to_string(), color_class: "text-emerald-400".to_string() }
                StatCard { label: "Critical".to_string(), value: "--".to_string(), color_class: "text-red-400".to_string() }
                StatCard { label: "Active Builds".to_string(), value: "--".to_string(), color_class: "text-blue-400".to_string() }
            }

            div {
                class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                Card {
                    title: Some("Fleet Health".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "Connect to API to view fleet health breakdown." }
                    }
                }

                Card {
                    title: Some("Recent Deployments".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "Connect to API to view recent deployments." }
                    }
                }
            }
        }
    }
}
