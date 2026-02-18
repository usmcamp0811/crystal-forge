//! Multi-select dropdown filter components.

use dioxus::prelude::*;

use crate::api::models::{DeploymentStatus, HealthStatus};
use crate::theme;

/// Generic multi-select dropdown component.
///
/// Displays a button with the current selection, and a dropdown menu
/// with checkboxes for each option.
#[component]
pub fn MultiSelectDropdown<T: Clone + PartialEq + 'static>(
    label: String,
    options: Vec<(T, String)>,
    selected: Signal<Vec<T>>,
    open_dropdown: Signal<Option<String>>,
    dropdown_id: String,
    all_label: String,
) -> Element {
    let is_open = *open_dropdown.read() == Some(dropdown_id.clone());
    let display_label = if selected.read().is_empty() {
        all_label.clone()
    } else if selected.read().len() == 1 {
        options
            .iter()
            .find(|(opt, _)| selected.read().contains(opt))
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| all_label.clone())
    } else {
        format!("{} selected", selected.read().len())
    };

    rsx! {
        div {
            class: "relative",
            button {
                class: "w-full flex items-center justify-between rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                onclick: move |_| {
                    if is_open {
                        open_dropdown.set(None);
                    } else {
                        open_dropdown.set(Some(dropdown_id.clone()));
                    }
                },
                span { "{display_label}" }
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M19 9l-7 7-7-7" }
                }
            }

            if is_open {
                div {
                    class: "absolute left-0 right-0 mt-1 rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl z-[3000]",
                    button {
                        class: "w-full text-left px-3 py-2 text-sm hover:bg-gray-700",
                        onclick: move |_| {
                            selected.set(Vec::new());
                            open_dropdown.set(None);
                        },
                        "{all_label}"
                    }
                    for (option, option_label) in options.iter() {
                        {
                            let is_selected = selected.read().contains(option);
                            let option_clone = option.clone();
                            rsx! {
                                button {
                                    key: "{option_label}",
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-gray-700",
                                    onclick: move |_| {
                                        let mut next = selected.read().clone();
                                        if next.contains(&option_clone) {
                                            next.retain(|value| value != &option_clone);
                                        } else {
                                            next.push(option_clone.clone());
                                        }
                                        selected.set(next);
                                    },
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                            }
                                        }
                                    }
                                    span { "{option_label}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Environment filter dropdown for filtering by environment name.
#[component]
pub fn EnvironmentFilterDropdown(
    environments: Vec<String>,
    selected: Signal<Vec<String>>,
    open_dropdown: Signal<Option<String>>,
) -> Element {
    let options: Vec<(String, String)> = environments
        .into_iter()
        .map(|env| (env.clone(), env))
        .collect();

    rsx! {
        MultiSelectDropdown {
            label: "Environment".to_string(),
            options: options,
            selected: selected,
            open_dropdown: open_dropdown,
            dropdown_id: "environment".to_string(),
            all_label: "All environments".to_string(),
        }
    }
}

/// Health status filter dropdown.
#[component]
pub fn HealthFilterDropdown(
    selected: Signal<Vec<HealthStatus>>,
    open_dropdown: Signal<Option<String>>,
) -> Element {
    let options: Vec<(HealthStatus, String)> = vec![
        HealthStatus::Healthy,
        HealthStatus::Warning,
        HealthStatus::Critical,
        HealthStatus::Offline,
    ]
    .into_iter()
    .map(|status| (status, status.label().to_string()))
    .collect();

    rsx! {
        MultiSelectDropdown {
            label: "Health".to_string(),
            options: options,
            selected: selected,
            open_dropdown: open_dropdown,
            dropdown_id: "health".to_string(),
            all_label: "All health".to_string(),
        }
    }
}

/// Deployment status filter dropdown.
#[component]
pub fn DeploymentFilterDropdown(
    selected: Signal<Vec<DeploymentStatus>>,
    open_dropdown: Signal<Option<String>>,
) -> Element {
    let options: Vec<(DeploymentStatus, String)> = vec![
        DeploymentStatus::UpToDate,
        DeploymentStatus::Behind,
        DeploymentStatus::Ahead,
        DeploymentStatus::NeverDeployed,
        DeploymentStatus::Unknown,
    ]
    .into_iter()
    .map(|status| (status, status.label().to_string()))
    .collect();

    rsx! {
        MultiSelectDropdown {
            label: "Deployment".to_string(),
            options: options,
            selected: selected,
            open_dropdown: open_dropdown,
            dropdown_id: "deployment".to_string(),
            all_label: "All deployment".to_string(),
        }
    }
}
