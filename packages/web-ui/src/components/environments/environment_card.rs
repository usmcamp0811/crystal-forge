//! Environment card component.

use dioxus::prelude::*;
use uuid::Uuid;

use super::{required_policy_names, with_alpha, EnvironmentItem, PolicyOption};
use crate::theme;

/// Props for the environment card.
#[derive(Props, Clone, PartialEq)]
pub struct EnvironmentCardProps {
    pub environment: EnvironmentItem,
    pub policy_library: Vec<PolicyOption>,
    pub on_edit_meta: EventHandler<EnvironmentItem>,
    pub on_edit_requirements: EventHandler<(Uuid, Vec<uuid::Uuid>)>,
    pub on_remove: EventHandler<EnvironmentItem>,
}

/// Environment card with color header, system count, and policy display.
#[component]
pub fn EnvironmentCard(props: EnvironmentCardProps) -> Element {
    let env = &props.environment;
    let required_names = required_policy_names(&env.required_policy_ids, &props.policy_library);
    let required_count = required_names.len();
    let visible_chips: Vec<String> = required_names.iter().take(3).cloned().collect();
    let overflow = required_count.saturating_sub(visible_chips.len());

    let env_for_remove = env.clone();
    let env_for_edit_meta = env.clone();
    let env_for_edit_req = env.clone();

    rsx! {
        div {
            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm",
            div {
                class: "flex items-center justify-between px-6 py-4 border-b border-gray-800",
                style: "background: linear-gradient(135deg, {with_alpha(&env.color_hex, 0.42)} 0%, rgba(17, 24, 39, 0.92) 100%);",
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
                }
            }

            div {
                class: "px-6 py-3 bg-gray-800/50",
                div {
                    class: "flex flex-wrap items-center gap-2 text-xs",
                    span {
                        class: "inline-flex px-2 py-1 rounded border text-gray-100",
                        style: "background-color: #2B303B; border-color: #495264;",
                        "{env.system_count} systems"
                    }
                    span {
                        class: "inline-flex px-2 py-1 rounded border text-gray-100",
                        style: "background-color: #23363A; border-color: #3D6870;",
                        "{required_count} required"
                    }
                }
            }

            div {
                class: "px-6 py-3 bg-gray-900 space-y-2",
                p { class: "text-[10px] font-semibold uppercase tracking-wider text-gray-500", "Required Policies" }
                p {
                    class: "text-[10px] text-amber-300/80",
                    "Policies are persisted server-side and inherited as environment baseline requirements."
                }
                div {
                    class: "flex flex-wrap gap-2",
                    for policy_name in visible_chips {
                        span {
                            class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100",
                            style: "background-color: #253449; border-color: #3E5B82;",
                            "{policy_name}"
                        }
                    }
                    if overflow > 0 {
                        span { class: "inline-flex px-2 py-1 text-xs rounded border border-gray-700 text-gray-400", "+{overflow}" }
                    }
                }
            }

            div {
                class: "px-6 py-3 bg-gray-800/50 flex items-center justify-between",
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: "text-xs px-2 py-1 rounded transition-colors",
                        style: "color: #D6C3E8;",
                        onclick: move |_| props.on_edit_meta.call(env_for_edit_meta.clone()),
                        "Edit Environment"
                    }
                    button {
                        class: "text-xs px-2 py-1 rounded transition-colors",
                        style: "color: #D6C3E8;",
                        onclick: {
                            let id = env.id;
                            let ids = env.required_policy_ids.clone();
                            move |_| props.on_edit_requirements.call((id, ids.clone()))
                        },
                        "Edit Requirements"
                    }
                }

                if env.system_count > 0 {
                    span { class: "text-xs text-gray-500", "In Use" }
                } else {
                    button {
                        class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                        onclick: move |_| props.on_remove.call(env_for_remove.clone()),
                        "Remove"
                    }
                }
            }
        }
    }
}
