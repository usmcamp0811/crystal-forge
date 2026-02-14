//! Systems list view — table of all registered NixOS systems.

use dioxus::prelude::*;

use crate::api::models::{CveSummary, DeploymentStatus, HealthStatus, PipelineStage, SystemSummary};
use crate::components::layout::Card;
use crate::components::system::SystemCard;
use crate::theme;

/// The systems list page.
#[component]
pub fn SystemsView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_systems()
    let mock_systems = vec![
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "atlas-01".to_string(),
            environment: Some("production".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary {
                critical: 0,
                high: 1,
                medium: 4,
                low: 12,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: None,
            deployment_policy: "Immediate".to_string(),
        },
        SystemSummary {
            id: uuid::Uuid::new_v4(),
            hostname: "luna-02".to_string(),
            environment: Some("staging".to_string()),
            health_status: HealthStatus::Warning,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 1,
                high: 3,
                medium: 6,
                low: 9,
            },
            nixos_version: Some("24.05".to_string()),
            last_seen: None,
            deployment_policy: "Boot Only".to_string(),
        },
    ];

    rsx! {
        div {
            class: "space-y-6",
            div {
                class: "flex flex-col gap-4 md:flex-row md:items-center md:justify-between",
                h1 {
                    class: "{theme::typography::PAGE_TITLE}",
                    "Systems"
                }
                div {
                    class: "flex items-center gap-3",
                    input {
                        class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        r#type: "text",
                        placeholder: "Search systems...",
                    }
                }
            }

            div {
                class: "grid grid-cols-1 xl:grid-cols-2 gap-6",
                for system in mock_systems {
                    SystemCard { system }
                }
            }

            Card {
                title: Some("Fleet Systems".to_string()),
                children: rsx! {
                    div {
                        class: "overflow-x-auto",
                        table {
                            class: "w-full",
                            thead {
                                class: "{theme::surface::SUBTLE_BG}",
                                tr {
                                    th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Hostname" }
                                    th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Health" }
                                    th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Deployment" }
                                    th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Environment" }
                                    th { class: "{theme::spacing::TABLE_CELL} {theme::typography::TABLE_HEADER}", "Last Seen" }
                                }
                            }
                            tbody {
                                class: "divide-y {theme::surface::DIVIDER}",
                                tr {
                                    td {
                                        class: "{theme::spacing::TABLE_CELL} text-center {theme::text::MUTED}",
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
    }
}
