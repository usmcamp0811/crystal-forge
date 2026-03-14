//! Builders list component showing all registered builders.

use dioxus::prelude::*;

use crate::api;
use crate::components::builders::{AddBuilderModal, BuilderCard, EditBuilderModal};
use crate::components::loading::LoadingSpinner;
use crate::theme;

#[component]
pub fn BuildersList(show_onboarding_hint: bool) -> Element {
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
                div {
                    class: "relative",
                    button {
                        class: if show_onboarding_hint && !show_add_modal() {
                            "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING} animate-pulse ring-2 ring-violet-300/70 ring-offset-2 ring-offset-slate-950"
                        } else {
                            "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}"
                        },
                        onclick: move |_| show_add_modal.set(true),
                        "➕ Add Builder"
                    }
                    if show_onboarding_hint && !show_add_modal() {
                        div {
                            "data-testid": "setup-coach-builders-target-callout",
                            style: "position:absolute; right:0; top:calc(100% + 10px); background:rgba(30,41,59,0.96); border:1px solid rgba(167,139,250,0.6); border-radius:10px; padding:8px 10px; color:#ddd6fe; font-size:12px; width:220px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                            div {
                                style: "position:absolute; top:-6px; right:18px; width:10px; height:10px; background:rgba(30,41,59,0.96); border-left:1px solid rgba(167,139,250,0.6); border-top:1px solid rgba(167,139,250,0.6); transform:rotate(45deg);"
                            }
                            p { style: "margin:0; color:#e9d5ff; font-weight:600;", "Next action" }
                            p { style: "margin:2px 0 0 0;", "Click Add Builder to connect your first build worker." }
                        }
                    }
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
