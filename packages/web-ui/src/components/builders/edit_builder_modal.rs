//! Edit builder modal component.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::{BuilderStatus, UpdateBuilderRequest, UpdateBuilderEnvironmentsRequest}};
use crate::components::loading::LoadingSpinner;
use crate::theme;

#[component]
pub fn EditBuilderModal(builder_id: Uuid, on_close: EventHandler<()>, on_success: EventHandler<()>) -> Element {
    let builder = use_resource(move || async move {
        api::client::fetch_builder(&builder_id).await
    });

    let environments = use_resource(|| async move {
        api::client::fetch_environments().await
    });

    let mut name = use_signal(|| String::new());
    let mut status = use_signal(|| BuilderStatus::Active);
    let mut max_cpu_cores = use_signal(|| String::new());
    let mut max_memory_mb = use_signal(|| String::new());
    let mut max_concurrent_jobs = use_signal(|| String::new());
    let mut selected_environments = use_signal(|| Vec::<Uuid>::new());
    
    let mut is_initialized = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    // Initialize form when builder data loads
    use_effect(move || {
        if let Some(Ok(builder_data)) = &*builder.read() {
            if !is_initialized() {
                name.set(builder_data.name.clone());
                status.set(builder_data.status.clone());
                max_cpu_cores.set(builder_data.max_cpu_cores.map(|n| n.to_string()).unwrap_or_default());
                max_memory_mb.set(builder_data.max_memory_mb.map(|n| n.to_string()).unwrap_or_default());
                max_concurrent_jobs.set(builder_data.max_concurrent_jobs.to_string());
                selected_environments.set(builder_data.assigned_environment_ids.clone());
                is_initialized.set(true);
            }
        }
    });

    let toggle_environment = move |env_id: Uuid| {
        let mut envs = selected_environments();
        if envs.contains(&env_id) {
            envs.retain(|id| *id != env_id);
        } else {
            envs.push(env_id);
        }
        selected_environments.set(envs);
    };

    let handle_submit = move |_| async move {
        is_submitting.set(true);
        error_message.set(None);

        // Update builder config
        let update_request = UpdateBuilderRequest {
            name: if name().trim().is_empty() {
                None
            } else {
                Some(name().trim().to_string())
            },
            status: Some(status()),
            max_cpu_cores: max_cpu_cores().trim().parse::<i32>().ok(),
            max_memory_mb: max_memory_mb().trim().parse::<i32>().ok(),
            max_concurrent_jobs: max_concurrent_jobs().trim().parse::<i32>().ok(),
        };

        match api::client::update_builder(&builder_id, &update_request).await {
            Ok(_) => {
                // Update environment assignments
                let env_request = UpdateBuilderEnvironmentsRequest {
                    environment_ids: selected_environments(),
                };
                
                match api::client::update_builder_environments(&builder_id, &env_request).await {
                    Ok(_) => {
                        on_success.call(());
                    }
                    Err(e) => {
                        error_message.set(Some(format!("Failed to update environments: {}", e)));
                        is_submitting.set(false);
                    }
                }
            }
            Err(e) => {
                error_message.set(Some(format!("Failed to update builder: {}", e)));
                is_submitting.set(false);
            }
        }
    };

    let handle_deactivate = move |_| async move {
        if !is_submitting() {
            is_submitting.set(true);
            error_message.set(None);

            match api::client::deactivate_builder(&builder_id).await {
                Ok(_) => {
                    on_success.call(());
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to deactivate builder: {}", e)));
                    is_submitting.set(false);
                }
            }
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| {
                if !is_submitting() {
                    on_close.call(())
                }
            },
            
            div {
                class: "bg-slate-800 border border-slate-700 rounded-lg p-6 max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                
                match &*builder.read_unchecked() {
                    Some(Ok(builder_data)) => rsx! {
                        // Header
                        div {
                            class: "flex items-center justify-between mb-6",
                            div {
                                h2 {
                                    class: "text-xl font-semibold text-white",
                                    "Edit Builder"
                                }
                                p {
                                    class: "text-sm {theme::text::SECONDARY} mt-1",
                                    "ID: {builder_data.id}"
                                }
                            }
                            button {
                                class: "text-slate-400 hover:text-white transition-colors",
                                onclick: move |_| on_close.call(()),
                                disabled: is_submitting(),
                                "✕"
                            }
                        }

                        // Error message
                        if let Some(err) = error_message() {
                            div {
                                class: "mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded text-red-400 text-sm",
                                "{err}"
                            }
                        }

                        // Form
                        div {
                            class: "space-y-4",

                            // Name
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                    "Builder Name"
                                }
                                input {
                                    class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white placeholder-slate-500 focus:outline-none focus:border-blue-500",
                                    r#type: "text",
                                    value: "{name}",
                                    oninput: move |e| name.set(e.value()),
                                    disabled: is_submitting(),
                                }
                            }

                            // Status
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                    "Status"
                                }
                                select {
                                    class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white focus:outline-none focus:border-blue-500",
                                    value: "{status().label()}",
                                    onchange: move |e| {
                                        let new_status = match e.value().as_str() {
                                            "Active" => BuilderStatus::Active,
                                            "Inactive" => BuilderStatus::Inactive,
                                            _ => BuilderStatus::Offline,
                                        };
                                        status.set(new_status);
                                    },
                                    disabled: is_submitting(),
                                    option { value: "Active", "Active" }
                                    option { value: "Inactive", "Inactive" }
                                    option { value: "Offline", "Offline" }
                                }
                            }

                            // Resource Limits
                            div {
                                class: "grid grid-cols-3 gap-4",
                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Max CPU Cores"
                                    }
                                    input {
                                        class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white placeholder-slate-500 focus:outline-none focus:border-blue-500",
                                        r#type: "number",
                                        min: "1",
                                        placeholder: "Unlimited",
                                        value: "{max_cpu_cores}",
                                        oninput: move |e| max_cpu_cores.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Max Memory (MB)"
                                    }
                                    input {
                                        class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white placeholder-slate-500 focus:outline-none focus:border-blue-500",
                                        r#type: "number",
                                        min: "1024",
                                        step: "1024",
                                        placeholder: "Unlimited",
                                        value: "{max_memory_mb}",
                                        oninput: move |e| max_memory_mb.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Max Concurrent Jobs"
                                    }
                                    input {
                                        class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white placeholder-slate-500 focus:outline-none focus:border-blue-500",
                                        r#type: "number",
                                        min: "1",
                                        value: "{max_concurrent_jobs}",
                                        oninput: move |e| max_concurrent_jobs.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                }
                            }

                            // Environment assignments
                            div {
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                    "Environment Assignments"
                                }
                                p {
                                    class: "text-xs {theme::text::SECONDARY} mb-2",
                                    "Leave empty for wildcard (builder handles all environments)"
                                }
                                
                                match &*environments.read_unchecked() {
                                    Some(Ok(env_list)) => rsx! {
                                        div {
                                            class: "border border-slate-700 rounded p-3 space-y-2 max-h-48 overflow-y-auto",
                                            if env_list.is_empty() {
                                                p {
                                                    class: "text-sm {theme::text::SECONDARY}",
                                                    "No environments available"
                                                }
                                            } else {
                                                for env in env_list {
                                                    div {
                                                        key: "{env.id}",
                                                        class: "flex items-center gap-2",
                                                        input {
                                                            r#type: "checkbox",
                                                            id: "env-edit-{env.id}",
                                                            class: "rounded border-slate-600 text-blue-600 focus:ring-blue-500",
                                                            checked: selected_environments().contains(&env.id),
                                                            onchange: move |_| toggle_environment(env.id),
                                                            disabled: is_submitting(),
                                                        }
                                                        label {
                                                            r#for: "env-edit-{env.id}",
                                                            class: "text-sm {theme::text::PRIMARY} cursor-pointer",
                                                            "{env.name}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                    Some(Err(e)) => rsx! {
                                        p {
                                            class: "text-sm text-red-400",
                                            "Failed to load environments: {e}"
                                        }
                                    },
                                    None => rsx! {
                                        p {
                                            class: "text-sm {theme::text::SECONDARY}",
                                            "Loading environments..."
                                        }
                                    },
                                }
                            }
                        }

                        // Footer buttons
                        div {
                            class: "flex justify-between mt-6 pt-4 border-t border-slate-700",
                            button {
                                class: "px-4 py-2 bg-red-600/20 hover:bg-red-600/30 text-red-400 border border-red-600/50 rounded transition-colors disabled:opacity-50",
                                onclick: handle_deactivate,
                                disabled: is_submitting(),
                                "Deactivate Builder"
                            }
                            div {
                                class: "flex gap-3",
                                button {
                                    class: "px-4 py-2 text-slate-400 hover:text-white transition-colors",
                                    onclick: move |_| on_close.call(()),
                                    disabled: is_submitting(),
                                    "Cancel"
                                }
                                button {
                                    class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors disabled:opacity-50",
                                    onclick: handle_submit,
                                    disabled: is_submitting(),
                                    if is_submitting() {
                                        "Saving..."
                                    } else {
                                        "Save Changes"
                                    }
                                }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "text-center py-8",
                            p {
                                class: "text-red-400 mb-4",
                                "Failed to load builder: {e}"
                            }
                            button {
                                class: "px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded transition-colors",
                                onclick: move |_| on_close.call(()),
                                "Close"
                            }
                        }
                    },
                    None => rsx! {
                        LoadingSpinner {
                            message: "Loading builder..."
                        }
                    },
                }
            }
        }
    }
}
