//! Environments list view with add/remove controls.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

#[derive(Clone, Debug, PartialEq)]
struct EnvironmentItem {
    name: String,
    description: Option<String>,
    system_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct NewEnvironmentDraft {
    name: String,
    description: String,
}

#[component]
pub fn EnvironmentsListView() -> Element {
    let mut environments = use_signal(seed_environments);
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut draft = use_signal(|| NewEnvironmentDraft {
        name: String::new(),
        description: String::new(),
    });
    let mut pending_remove = use_signal(|| None::<EnvironmentItem>);

    let items = environments.read().clone();

    rsx! {
        div {
            class: "space-y-6",
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Environments" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Group systems by deployment domain and policy scope." }
                }
                button {
                    class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                    onclick: move |_| {
                        let next = !*show_add_form.read();
                        show_add_form.set(next);
                        add_error.set(None);
                    },
                    if *show_add_form.read() { "Close" } else { "Add Environment" }
                }
            }

            if *show_add_form.read() {
                Card {
                    title: Some("Create Environment".to_string()),
                    children: rsx! {
                        div {
                            class: "space-y-4",
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                label {
                                    class: "space-y-2",
                                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Name" }
                                    input {
                                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                        value: "{draft.read().name}",
                                        placeholder: "lan",
                                        oninput: move |evt| {
                                            let mut next = draft.read().clone();
                                            next.name = evt.value();
                                            draft.set(next);
                                        }
                                    }
                                }
                                label {
                                    class: "space-y-2",
                                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Description (optional)" }
                                    input {
                                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                        value: "{draft.read().description}",
                                        placeholder: "Local area systems",
                                        oninput: move |evt| {
                                            let mut next = draft.read().clone();
                                            next.description = evt.value();
                                            draft.set(next);
                                        }
                                    }
                                }
                            }

                            if let Some(message) = add_error.read().clone() {
                                p { class: "text-sm text-red-300", "{message}" }
                            }

                            div {
                                class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                                button {
                                    class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                                    onclick: move |_| {
                                        draft.set(NewEnvironmentDraft {
                                            name: String::new(),
                                            description: String::new(),
                                        });
                                        add_error.set(None);
                                        show_add_form.set(false);
                                    },
                                    "Cancel"
                                }
                                button {
                                    class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                                    onclick: move |_| {
                                        let next = draft.read().clone();
                                        if let Err(err) = validate_environment(&next, &environments.read()) {
                                            add_error.set(Some(err));
                                            return;
                                        }

                                        let mut values = environments.read().clone();
                                        values.push(EnvironmentItem {
                                            name: next.name.trim().to_string(),
                                            description: normalize_optional(&next.description),
                                            system_count: 0,
                                        });
                                        values.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                                        environments.set(values);
                                        draft.set(NewEnvironmentDraft {
                                            name: String::new(),
                                            description: String::new(),
                                        });
                                        add_error.set(None);
                                        show_add_form.set(false);
                                    },
                                    "Save Environment"
                                }
                            }
                        }
                    }
                }
            }

            Card {
                title: Some("Environment Registry".to_string()),
                children: rsx! {
                    div {
                        class: "space-y-3",
                        for env in items {
                            div {
                                class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/60 p-4 flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                                div {
                                    p { class: "text-sm font-semibold text-white", "{env.name}" }
                                    p {
                                        class: "text-xs {theme::text::SECONDARY}",
                                        if let Some(description) = env.description.clone() {
                                            "{description}"
                                        } else {
                                            "No description"
                                        }
                                    }
                                    p { class: "text-xs text-gray-500 mt-1", "{env.system_count} systems" }
                                }
                                if env.system_count > 0 {
                                    span {
                                        class: "inline-flex px-2 py-1 text-xs rounded border border-gray-700 text-gray-500",
                                        "In Use"
                                    }
                                } else {
                                    button {
                                        class: "px-3 py-2 rounded-lg text-sm font-medium border border-red-500/40 text-red-300 hover:bg-red-500/15 transition",
                                        onclick: move |_| pending_remove.set(Some(env.clone())),
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(env) = pending_remove.read().clone() {
                div {
                    class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
                    style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
                    onclick: move |_| pending_remove.set(None),
                    div {
                        class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                        style: "width: 100%; max-width: 30rem;",
                        onclick: |evt| evt.stop_propagation(),
                        h3 { class: "text-lg font-semibold text-white mb-2", "Remove environment {env.name}?" }
                        p {
                            class: "text-sm {theme::text::SECONDARY} mb-6",
                            "This deletes the environment from the registry view."
                        }
                        div {
                            class: "flex gap-3",
                            button {
                                class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                                onclick: move |_| pending_remove.set(None),
                                "Cancel"
                            }
                            button {
                                class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-red-500 hover:bg-red-400 text-white",
                                onclick: move |_| {
                                    let mut values = environments.read().clone();
                                    values.retain(|item| item.name != env.name);
                                    environments.set(values);
                                    pending_remove.set(None);
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn validate_environment(
    draft: &NewEnvironmentDraft,
    existing: &[EnvironmentItem],
) -> Result<(), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Environment name is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(name))
    {
        return Err("Environment already exists.".to_string());
    }
    Ok(())
}

fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn seed_environments() -> Vec<EnvironmentItem> {
    vec![
        EnvironmentItem {
            name: "production".to_string(),
            description: Some("Live fleet systems".to_string()),
            system_count: 12,
        },
        EnvironmentItem {
            name: "staging".to_string(),
            description: Some("Pre-production validation".to_string()),
            system_count: 2,
        },
        EnvironmentItem {
            name: "development".to_string(),
            description: Some("Workstations and local testing".to_string()),
            system_count: 8,
        },
        EnvironmentItem {
            name: "remote".to_string(),
            description: Some("Remote unmanaged network".to_string()),
            system_count: 0,
        },
    ]
}
