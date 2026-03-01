//! Edit builder modal component.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{
    self,
    models::{
        BuilderStatus, UpdateBuilderEnvironmentsRequest, UpdateBuilderPublicKeyRequest,
        UpdateBuilderRequest,
    },
};
use crate::components::builders::generate_ed25519_keypair;
use crate::components::loading::LoadingSpinner;
use crate::components::modals::ConfirmDialog;
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
    let mut rotated_public_key = use_signal(|| String::new());
    let mut rotated_private_key = use_signal(|| String::new());
    let mut show_rotated_private_key = use_signal(|| false);
    
    let mut is_initialized = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut show_delete_confirm = use_signal(|| false);
    let mut show_delete_final_confirm = use_signal(|| false);

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
                rotated_public_key.set(builder_data.public_key.clone());
                is_initialized.set(true);
            }
        }
    });

    let mut toggle_environment = move |env_id: Uuid| {
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

    let handle_generate_keypair = move |_| {
        match generate_ed25519_keypair() {
            Ok((priv_hex, pub_b64)) => {
                rotated_private_key.set(priv_hex);
                rotated_public_key.set(pub_b64);
                show_rotated_private_key.set(false);
                error_message.set(None);
            }
            Err(e) => {
                error_message.set(Some(format!("Failed to generate keypair: {}", e)));
            }
        }
    };

    let handle_update_public_key = move |_| async move {
        if is_submitting() {
            return;
        }

        let next_public_key = rotated_public_key().trim().to_string();
        if next_public_key.is_empty() {
            error_message.set(Some("Generate a keypair first before updating the builder key.".to_string()));
            return;
        }

        is_submitting.set(true);
        error_message.set(None);

        let request = UpdateBuilderPublicKeyRequest {
            public_key: next_public_key,
        };

        match api::client::update_builder_public_key(&builder_id, &request).await {
            Ok(_) => {
                on_success.call(());
            }
            Err(e) => {
                error_message.set(Some(format!("Failed to update builder key: {}", e)));
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

    let delete_builder = move || async move {
        if !is_submitting() {
            is_submitting.set(true);
            error_message.set(None);

            match api::client::delete_builder_permanently(&builder_id).await {
                Ok(_) => {
                    on_success.call(());
                }
                Err(e) => {
                    error_message.set(Some(format!("Failed to delete builder: {}", e)));
                    is_submitting.set(false);
                }
            }
        }
    };

    let handle_delete = move |_| {
        if !is_submitting() {
            show_delete_confirm.set(true);
        }
    };

    rsx! {
        div {
            class: "fixed inset-0 z-[60] bg-black/60 flex items-center justify-center p-4 overflow-y-auto",
            onclick: move |_| {
                if !is_submitting() {
                    on_close.call(())
                }
            },
            
            div {
                class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl p-6 w-full max-w-2xl max-h-[90vh] overflow-y-auto shadow-2xl my-auto",
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
                                
                                {
                                    let env_data = environments.read();
                                    match &*env_data {
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
                                                        {
                                                            let env_id = env.id;
                                                            rsx! {
                                                                div {
                                                                    key: "{env.id}",
                                                                    class: "flex items-center gap-2",
                                                                    input {
                                                                        r#type: "checkbox",
                                                                        id: "env-edit-{env.id}",
                                                                        class: "rounded border-slate-600 text-blue-600 focus:ring-blue-500",
                                                                        checked: selected_environments().contains(&env.id),
                                                                        onchange: move |_| toggle_environment(env_id),
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

                            // Key rotation
                            div {
                                class: "border border-slate-700 rounded p-4 space-y-3",
                                div {
                                    class: "flex items-center justify-between",
                                    h3 {
                                        class: "text-sm font-medium {theme::text::PRIMARY}",
                                        "Rotate Builder Keypair"
                                    }
                                    button {
                                        class: "px-3 py-1 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                                        onclick: handle_generate_keypair,
                                        disabled: is_submitting(),
                                        "🔑 Generate New Keypair"
                                    }
                                }
                                p {
                                    class: "text-xs {theme::text::SECONDARY}",
                                    "Paste a public key manually or generate a new keypair. Apply to save the public key to this builder."
                                }

                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Public Key (base64)"
                                    }
                                    textarea {
                                        class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white font-mono text-xs",
                                        rows: "2",
                                        value: "{rotated_public_key}",
                                        oninput: move |e| rotated_public_key.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                }

                                if !rotated_private_key().is_empty() {
                                    div {
                                        div {
                                            class: "flex items-center justify-between mb-1",
                                            label {
                                                class: "block text-sm font-medium text-amber-400",
                                                "Generated Private Key"
                                                span { class: "text-xs {theme::text::SECONDARY} ml-2", "(save this securely)" }
                                            }
                                            button {
                                                class: "text-xs text-blue-400 hover:text-blue-300",
                                                onclick: move |_| show_rotated_private_key.set(!show_rotated_private_key()),
                                                if show_rotated_private_key() { "Hide" } else { "Show" }
                                            }
                                        }
                                        if show_rotated_private_key() {
                                            textarea {
                                                class: "w-full px-3 py-2 bg-amber-900/20 border border-amber-700/50 rounded text-amber-200 font-mono text-xs",
                                                rows: "2",
                                                readonly: true,
                                                value: "{rotated_private_key}",
                                            }
                                        } else {
                                            div {
                                                class: "px-3 py-2 bg-slate-900 border border-slate-700 rounded text-slate-500 font-mono text-xs",
                                                "••••••••••••••••••••••••••••••••"
                                            }
                                        }
                                    }
                                }

                                div {
                                    class: "pt-1",
                                    button {
                                        class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-50",
                                        onclick: handle_update_public_key,
                                        disabled: is_submitting() || rotated_public_key().trim().is_empty(),
                                        if is_submitting() {
                                            "Applying..."
                                        } else {
                                            "Apply Public Key Update"
                                        }
                                    }
                                }
                            }
                        }

                        // Footer buttons
                        div {
                            class: "flex justify-between mt-6 pt-4 border-t border-slate-700",
                            div {
                                class: "flex gap-3",
                                button {
                                    class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::DANGER_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-50",
                                    onclick: handle_deactivate,
                                    disabled: is_submitting(),
                                    "Deactivate Builder"
                                }
                                button {
                                    class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::DANGER_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-50",
                                    onclick: handle_delete,
                                    disabled: is_submitting(),
                                    "Delete Permanently"
                                }
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
                                    class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-50",
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
                        LoadingSpinner {}
                    },
                }
            }
        }

        if show_delete_confirm() {
            ConfirmDialog {
                title: "Delete builder permanently?".to_string(),
                description: "This permanently removes the builder and cannot be undone.".to_string(),
                confirm_label: "Continue".to_string(),
                danger: true,
                on_cancel: move |_| show_delete_confirm.set(false),
                on_confirm: move |_| {
                    show_delete_confirm.set(false);
                    show_delete_final_confirm.set(true);
                },
            }
        }

        if show_delete_final_confirm() {
            ConfirmDialog {
                title: "Final confirmation required".to_string(),
                description: "Delete this builder now? This action is irreversible.".to_string(),
                confirm_label: "Delete Permanently".to_string(),
                danger: true,
                on_cancel: move |_| show_delete_final_confirm.set(false),
                on_confirm: move |_| {
                    show_delete_final_confirm.set(false);
                    spawn(async move {
                        delete_builder().await;
                    });
                },
            }
        }
    }
}
