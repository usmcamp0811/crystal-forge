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
    // These are no longer needed with native select controls, but are kept in
    // the component signature to avoid touching call sites.
    let _ = open_dropdown;
    let _ = dropdown_id;

    let selected_index = selected.read().first().and_then(|current| {
        options
            .iter()
            .position(|(value, _)| value == current)
            .map(|idx| idx.to_string())
    });

    rsx! {
        div {
            class: "relative",
            label {
                class: "sr-only",
                r#for: "filter-{label}",
                "{label}"
            }
            select {
                id: "filter-{label}",
                class: "input filter-select focus-ring w-full {theme::interactive::INPUT} {theme::text::SECONDARY}",
                value: selected_index.unwrap_or_else(|| "__all__".to_string()),
                onchange: move |evt| {
                    let value = evt.value();
                    if value == "__all__" {
                        selected.set(Vec::new());
                    } else if let Ok(idx) = value.parse::<usize>() {
                        if let Some((option, _)) = options.get(idx) {
                            selected.set(vec![option.clone()]);
                        }
                    }
                },
                option {
                    value: "__all__",
                    "{all_label}"
                }
                for (idx, (_, option_label)) in options.iter().enumerate() {
                    option {
                        key: "{option_label}",
                        value: "{idx}",
                        "{option_label}"
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
