//! Edit builder modal component.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::BuilderStatus};
use crate::components::builders::generate_ed25519_keypair;
use crate::components::{Icon, IconName};
use crate::components::loading::LoadingSpinner;
use crate::components::modals::ConfirmDialog;
use crate::theme;

#[path = "edit_builder_modal_actions.rs"]
mod edit_builder_modal_actions;
#[path = "edit_builder_modal_sections.rs"]
mod edit_builder_modal_sections;
use edit_builder_modal_actions::{
    apply_builder_public_key, build_update_request, deactivate_builder, delete_builder_permanently,
    submit_builder_update,
};

#[component]
pub fn EditBuilderModal(
    builder_id: Uuid,
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
) -> Element {
    let builder =
        use_resource(move || async move { api::client::fetch_builder(&builder_id).await });

    let environments = use_resource(|| async move { api::client::fetch_environments().await });

    let mut name = use_signal(|| String::new());
    let mut host = use_signal(|| String::new());
    let mut arch = use_signal(|| String::from("x86_64-linux"));
    let mut enabled = use_signal(|| true);
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
                host.set(builder_data.host.clone().unwrap_or_default());
                arch.set(builder_data.arch.clone());
                enabled.set(builder_data.enabled);
                status.set(builder_data.status.clone());
                max_cpu_cores.set(
                    builder_data
                        .max_cpu_cores
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                );
                max_memory_mb.set(
                    builder_data
                        .max_memory_mb
                        .map(|n| n.to_string())
                        .unwrap_or_default(),
                );
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

        let update_request = build_update_request(
            name().as_str(),
            host().as_str(),
            arch().as_str(),
            enabled(),
            status(),
            max_cpu_cores().as_str(),
            max_memory_mb().as_str(),
            max_concurrent_jobs().as_str(),
        );

        match submit_builder_update(&builder_id, &update_request, selected_environments()).await {
            Ok(_) => on_success.call(()),
            Err(message) => {
                error_message.set(Some(message));
                is_submitting.set(false);
            }
        }
    };

    let handle_generate_keypair = move |_| match generate_ed25519_keypair() {
        Ok((priv_b64, pub_b64)) => {
            rotated_private_key.set(priv_b64);
            rotated_public_key.set(pub_b64);
            show_rotated_private_key.set(false);
            error_message.set(None);
        }
        Err(e) => {
            error_message.set(Some(format!("Failed to generate keypair: {}", e)));
        }
    };

    let handle_toggle_private_key = move |_| {
        show_rotated_private_key.set(!show_rotated_private_key());
    };

    let handle_update_public_key = move |_: Event<MouseData>| async move {
        if is_submitting() {
            return;
        }

        let next_public_key = rotated_public_key().trim().to_string();
        if next_public_key.is_empty() {
            error_message.set(Some(
                "Generate a keypair first before updating the builder key.".to_string(),
            ));
            return;
        }

        is_submitting.set(true);
        error_message.set(None);

        match apply_builder_public_key(&builder_id, next_public_key).await {
            Ok(_) => on_success.call(()),
            Err(message) => {
                error_message.set(Some(message));
                is_submitting.set(false);
            }
        }
    };

    let handle_deactivate = move |_: Event<MouseData>| async move {
        if !is_submitting() {
            is_submitting.set(true);
            error_message.set(None);

            match deactivate_builder(&builder_id).await {
                Ok(_) => on_success.call(()),
                Err(message) => {
                    error_message.set(Some(message));
                    is_submitting.set(false);
                }
            }
        }
    };

    let delete_builder = move || async move {
        if !is_submitting() {
            is_submitting.set(true);
            error_message.set(None);

            match delete_builder_permanently(&builder_id).await {
                Ok(_) => on_success.call(()),
                Err(message) => {
                    error_message.set(Some(message));
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
            class: "modal-backdrop",
            onclick: move |_| {
                if !is_submitting() {
                    on_close.call(())
                }
            },

            div {
                class: "modal",
                style: "width:min(620px,96vw); max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                match &*builder.read_unchecked() {
                    Some(Ok(builder_data)) => rsx! {
                        // Header
                        div {
                            class: "modal-head",
                            style: "display:flex; align-items:center; justify-content:space-between;",
                            div {
                                h2 {
                                    Icon { name: IconName::Gear, size: 14 }
                                    " Edit {builder_data.name}"
                                }
                                p {
                                    "Update builder registration."
                                }
                            }
                            button {
                                class: "btn btn-ghost focus-ring",
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
                            class: "modal-body",
                            style: "overflow-y:auto;",

                            div {
                                class: "space-y-4",

                            // Name
                            div {
                                class: "field",
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                    "Builder Name"
                                }
                                input {
                                    class: "input focus-ring mono",
                                    r#type: "text",
                                    value: "{name}",
                                    oninput: move |e| name.set(e.value()),
                                    disabled: is_submitting(),
                                }
                            }

                            div {
                                class: "field",
                                label {
                                    class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                    "Host (SSH endpoint)"
                                }
                                input {
                                    class: "input focus-ring mono",
                                    r#type: "text",
                                    value: "{host}",
                                    oninput: move |e| host.set(e.value()),
                                    disabled: is_submitting(),
                                }
                            }

                            // Resource Limits
                            div {
                                class: "grid grid-cols-3 gap-4",
                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Architecture"
                                    }
                                    select {
                                        class: "input focus-ring",
                                        value: "{arch}",
                                        onchange: move |e| arch.set(e.value()),
                                        disabled: is_submitting(),
                                        option { value: "x86_64-linux", "x86_64-linux" }
                                        option { value: "aarch64-linux", "aarch64-linux" }
                                        option { value: "aarch64-darwin", "aarch64-darwin" }
                                        option { value: "x86_64-darwin", "x86_64-darwin" }
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Cores"
                                    }
                                    input {
                                        class: "input focus-ring",
                                        r#type: "number",
                                        min: "1",
                                        value: "{max_memory_mb}",
                                        oninput: move |e| max_cpu_cores.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                }
                                div {
                                    label {
                                        class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                        "Memory (GiB)"
                                    }
                                    input {
                                        class: "input focus-ring",
                                        r#type: "number",
                                        min: "1",
                                        step: "1",
                                        value: "{max_concurrent_jobs}",
                                        oninput: move |e| max_memory_mb.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                }
                            }

                            div {
                                class: "grid grid-cols-2 gap-4",
                                div {
                                    class: "field",
                                    label { "Max concurrent slots" }
                                    input {
                                        class: "input focus-ring",
                                        r#type: "number",
                                        min: "1",
                                        value: "{max_concurrent_jobs}",
                                        oninput: move |e| max_concurrent_jobs.set(e.value()),
                                        disabled: is_submitting(),
                                    }
                                    div { class: "help", "How many builds this worker may run in parallel." }
                                }
                                div {
                                    class: "field",
                                    label { "Status" }
                                    label {
                                        style: "display:flex; gap:8px; align-items:center; font-size:13px; padding:6px 0;",
                                        input {
                                            r#type: "checkbox",
                                            checked: enabled(),
                                            onchange: move |e| enabled.set(e.checked()),
                                        }
                                        span { "Enabled (accepts jobs)" }
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

                            div {
                                class: "field",
                                label { "SSH public key" }
                                textarea {
                                    class: "input focus-ring mono",
                                    rows: "3",
                                    value: "{rotated_public_key}",
                                    oninput: move |e| rotated_public_key.set(e.value()),
                                    style: "font-size:11px; resize:vertical; padding:10px;",
                                }
                                div { class: "help", "Crystal Forge uses this key to verify build result signatures." }
                                div { style: "margin-top:8px; display:flex; gap:8px; flex-wrap:wrap;",
                                    button {
                                        class: "btn btn-primary focus-ring",
                                        onclick: handle_generate_keypair,
                                        disabled: is_submitting(),
                                        "Generate Keypair"
                                    }
                                    button {
                                        class: "btn btn-ghost focus-ring",
                                        onclick: move |e| {
                                            spawn(handle_update_public_key(e));
                                        },
                                        disabled: is_submitting() || rotated_public_key().trim().is_empty(),
                                        "Apply Public Key Update"
                                    }
                                }
                                if !rotated_private_key().is_empty() {
                                    div { style: "margin-top:10px;",
                                        div { style: "display:flex; align-items:center; justify-content:space-between; margin-bottom:6px;",
                                            span { class: "help", style: "margin:0;", "Generated private key (save securely)" }
                                            button {
                                                class: "btn btn-ghost focus-ring",
                                                onclick: handle_toggle_private_key,
                                                style: "padding:2px 8px; font-size:11px;",
                                                if show_rotated_private_key() { "Hide" } else { "Show" }
                                            }
                                        }
                                        if show_rotated_private_key() {
                                            textarea {
                                                class: "input focus-ring mono",
                                                rows: "3",
                                                readonly: true,
                                                value: "{rotated_private_key()}",
                                                style: "font-size:11px; resize:vertical; padding:10px;"
                                            }
                                        } else {
                                            div {
                                                class: "input mono",
                                                style: "font-size:11px; padding:10px; color:var(--cf-text-muted);",
                                                "••••••••••••••••••••••••••••••••"
                                            }
                                        }
                                    }
                                }
                            }

                            div { style: "margin-top:10px; padding-top:14px; border-top:1px solid var(--cf-divider);",
                                div { style: "font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.08em; color:var(--cf-text-muted); margin-bottom:8px;", "Danger zone" }
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    onclick: handle_delete,
                                    disabled: is_submitting(),
                                    style: "color:#f87171; border-color: rgba(248,113,113,0.3);",
                                    Icon { name: IconName::X, size: 12 }
                                    " Remove builder"
                                }
                            }
                        }

                        }

                        // Footer buttons
                        div {
                            class: "modal-foot",
                            button {
                                class: "btn btn-ghost focus-ring",
                                onclick: move |_| on_close.call(()),
                                disabled: is_submitting(),
                                "Cancel"
                            }
                            button {
                                class: "btn btn-primary focus-ring",
                                onclick: handle_submit,
                                disabled: is_submitting(),
                                Icon { name: IconName::Check, size: 13 }
                                if is_submitting() {
                                    " Saving..."
                                } else {
                                    " Save changes"
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
