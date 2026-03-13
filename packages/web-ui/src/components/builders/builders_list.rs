//! Builders list component showing all registered builders.

use dioxus::prelude::*;

use crate::api;
use crate::components::builders::{AddBuilderModal, BuilderCard, EditBuilderModal};
use crate::components::loading::LoadingSpinner;
use crate::theme;

#[component]
pub fn BuildersList() -> Element {
    let mut show_add_modal = use_signal(|| false);
    let mut edit_builder_id = use_signal(|| None::<uuid::Uuid>);
    let mut refresh_trigger = use_signal(|| 0);

    let builders = use_resource(move || async move {
        let _ = refresh_trigger();
        api::client::fetch_builders().await
    });

    // Trigger refresh when modals close
    let mut on_builder_added = move || {
        show_add_modal.set(false);
        refresh_trigger.set(refresh_trigger() + 1);
    };

    let mut on_builder_updated = move || {
        edit_builder_id.set(None);
        refresh_trigger.set(refresh_trigger() + 1);
    };

    let mut on_edit_builder = move |id: uuid::Uuid| {
        edit_builder_id.set(Some(id));
    };

    rsx! {
        div {
            class: "space-y-6",

            // Header with Add Builder button
            div {
                class: "flex items-center justify-between",
                h2 {
                    class: "{theme::typography::SECTION_TITLE}",
                    "Registered Builders"
                }
                button {
                    class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                    onclick: move |_| show_add_modal.set(true),
                    "➕ Add Builder"
                }
            }

            // Builders grid
            {
                let builder_data = builders.read();
                match &*builder_data {
                    Some(Ok(builder_list)) => rsx! {
                        if builder_list.is_empty() {
                            div {
                                class: "text-center py-12 border border-dashed border-slate-700 rounded-lg",
                                p {
                                    class: "text-slate-400",
                                    "No builders registered yet."
                                }
                                p {
                                    class: "text-sm text-slate-500 mt-2",
                                    "Click \"Add Builder\" to register your first build worker."
                                }
                            }
                        } else {
                            // Warn if any builder has no environment assigned — builds
                            // will never be dispatched to an unassigned builder.
                            {
                                let unassigned: Vec<_> = builder_list
                                    .iter()
                                    .filter(|b| b.assigned_environment_count == 0)
                                    .collect();
                                if !unassigned.is_empty() {
                                    let names = unassigned
                                        .iter()
                                        .map(|b| b.name.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    let plural = if unassigned.len() == 1 { "" } else { "s" };
                                    let verb = if unassigned.len() == 1 { "has" } else { "have" };
                                    let msg = format!(
                                        "The following builder{plural} {verb} no environment assigned and will not process any build jobs: {names}. Open the builder settings (✏ Edit) and assign it to an environment."
                                    );
                                    rsx! {
                                        div {
                                            style: "background:rgba(161,98,7,0.2); border:1px solid rgba(217,119,6,0.5); border-radius:8px; padding:12px 16px; display:flex; gap:10px; align-items:flex-start;",
                                            span { style: "font-size:18px; line-height:1;", "⚠️" }
                                            div {
                                                p { style: "color:#fcd34d; font-weight:600; font-size:14px; margin:0 0 4px;", "Builder not assigned to any environment" }
                                                p { style: "color:#fde68a; font-size:13px; margin:0;", "{msg}" }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                            div {
                                class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4",
                                for builder in builder_list {
                                    {
                                        let builder_id = builder.id;
                                        rsx! {
                                            BuilderCard {
                                                key: "{builder.id}",
                                                builder: builder.clone(),
                                                on_edit: move |_| on_edit_builder(builder_id),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "border border-red-500/30 bg-red-500/10 rounded-lg p-4",
                            p {
                                class: "text-red-400",
                                "⚠️ Failed to load builders: {e}"
                            }
                        }
                    },
                    None => rsx! {
                        LoadingSpinner {}
                    },
                }
            }
        }

        // Modals
        if show_add_modal() {
            AddBuilderModal {
                on_close: move |_| show_add_modal.set(false),
                on_success: move |_| on_builder_added(),
            }
        }

        if let Some(id) = edit_builder_id() {
            EditBuilderModal {
                builder_id: id,
                on_close: move |_| edit_builder_id.set(None),
                on_success: move |_| on_builder_updated(),
            }
        }
    }
}
