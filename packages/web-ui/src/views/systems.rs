//! Systems list view — table of all registered NixOS systems.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

/// The systems list page.
#[component]
pub fn SystemsView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_systems()
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
