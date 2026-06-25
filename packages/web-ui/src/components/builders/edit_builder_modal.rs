//! Edit builder modal component.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::BuilderStatus};
use crate::components::builders::generate_ed25519_keypair;
use crate::components::loading::LoadingSpinner;
use crate::components::modals::ConfirmDialog;
use crate::components::{Icon, IconName};
use crate::theme;

#[path = "edit_builder_modal_actions.rs"]
mod edit_builder_modal_actions;
use edit_builder_modal_actions::{
    apply_builder_public_key, build_update_request, delete_builder_permanently,
    submit_builder_update,
};

fn memory_mb_to_gib_string(value: Option<i32>) -> String {
    value
        .map(|mb| {
            let gib = (mb as f64) / 1024.0;
            if (gib.fract()).abs() < f64::EPSILON {
                format!("{}", gib as i32)
            } else {
                format!("{gib:.1}")
            }
        })
        .unwrap_or_default()
}

fn memory_gib_to_mb_string(value: &str) -> String {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .map(|gib| ((gib * 1024.0).round() as i32).max(1).to_string())
        .unwrap_or_default()
}

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
    let mut key_rotation_applied = use_signal(|| false);
    let mut current_fingerprint = use_signal(|| String::new());

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
                max_memory_mb.set(memory_mb_to_gib_string(builder_data.max_memory_mb));
                max_concurrent_jobs.set(builder_data.max_concurrent_jobs.to_string());
                selected_environments.set(builder_data.assigned_environment_ids.clone());
                rotated_public_key.set(builder_data.public_key.clone());
                current_fingerprint.set(builder_data.public_key_fingerprint.clone());
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
            memory_gib_to_mb_string(&max_memory_mb()).as_str(),
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
            key_rotation_applied.set(false);
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
            Ok(updated_builder) => {
                current_fingerprint.set(updated_builder.public_key_fingerprint);
                key_rotation_applied.set(true);
                is_submitting.set(false);
            }
            Err(message) => {
                error_message.set(Some(message));
                is_submitting.set(false);
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
                style: "width:min(760px,96vw); max-height:92vh;",
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

                        // Key rotation success message
                        if key_rotation_applied() {
                            div {
                                class: "mb-4 p-3 bg-green-500/10 border border-green-500/30 rounded text-green-400 text-sm",
                                "✓ Public key updated successfully. ",
                                strong { "Save the private key below before closing this modal." }
                            }
                        }

                        // Form
                        div {
                            class: "modal-body",
                            style: "overflow-y:auto;",

                            div {
                                style: "display:flex; flex-direction:column; gap:16px;",

                                // Row 1: Name + Environments served
                                div {
                                    style: "display:grid; grid-template-columns:1fr 1fr; gap:14px;",
                                    div {
                                        class: "field",
                                        label { "Name" }
                                        input {
                                            class: "input focus-ring",
                                            r#type: "text",
                                            value: "{name}",
                                            oninput: move |e| name.set(e.value()),
                                            disabled: is_submitting(),
                                        }
                                    }
                                    div {
                                        class: "field",
                                        label { "Environments served" }
                                        {
                                            let env_data = environments.read();
                                            match &*env_data {
                                                Some(Ok(env_list)) => rsx! {
                                                    if env_list.is_empty() {
                                                        p {
                                                            class: "text-sm {theme::text::SECONDARY}",
                                                            "No environments available"
                                                        }
                                                    } else {
                                                        div {
                                                            style: "display:flex; flex-wrap:wrap; gap:6px;",
                                                            for env in env_list {
                                                                {
                                                                    let env_id = env.id;
                                                                    let env_color = if env.color_hex.trim().is_empty() {
                                                                        "#6b7280".to_string()
                                                                    } else {
                                                                        env.color_hex.clone()
                                                                    };
                                                                    let is_selected = selected_environments().contains(&env_id);
                                                                    let border = if is_selected {
                                                                        format!("1px solid {}", env_color)
                                                                    } else {
                                                                        "1px solid var(--cf-card-border)".to_string()
                                                                    };
                                                                    let background = if is_selected {
                                                                        format!(
                                                                            "color-mix(in oklab, {} 14%, var(--cf-card-bg))",
                                                                            env_color
                                                                        )
                                                                    } else {
                                                                        "transparent".to_string()
                                                                    };
                                                                    let color = if is_selected {
                                                                        env_color.clone()
                                                                    } else {
                                                                        "var(--cf-text-secondary)".to_string()
                                                                    };
                                                                    rsx! {
                                                                        button {
                                                                            key: "{env.id}",
                                                                            class: "focus-ring",
                                                                            r#type: "button",
                                                                            onclick: move |_| toggle_environment(env_id),
                                                                            disabled: is_submitting(),
                                                                            style: "padding:4px 10px; border-radius:99px; font-size:11px; border:{border}; background:{background}; color:{color}; cursor:pointer; display:inline-flex; align-items:center; gap:6px; font-family:inherit;",
                                                                            span {
                                                                                style: "width:6px; height:6px; border-radius:50%; background:{env_color};"
                                                                            }
                                                                            "{env.name}"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div {
                                                        class: "help",
                                                        "Builds for systems in any of these environments will be routed to this worker."
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
                                }

                                // Host (full width)
                                div {
                                    class: "field",
                                    label { "Host (SSH endpoint)" }
                                    input {
                                        class: "input focus-ring",
                                        r#type: "text",
                                        value: "{host}",
                                        oninput: move |e| host.set(e.value()),
                                        disabled: is_submitting(),
                                        placeholder: "ssh://builder.example.internal",
                                    }
                                }

                                // Row 2: Architecture + Cores + Memory
                                div {
                                    style: "display:grid; grid-template-columns:1fr 1fr 1fr; gap:14px;",
                                    div {
                                        class: "field",
                                        label { "Architecture" }
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
                                        class: "field",
                                        label { "Cores" }
                                        input {
                                            class: "input focus-ring",
                                            r#type: "number",
                                            min: "1",
                                            value: "{max_cpu_cores}",
                                            oninput: move |e| max_cpu_cores.set(e.value()),
                                            disabled: is_submitting(),
                                        }
                                    }
                                    div {
                                        class: "field",
                                        label { "Memory (GiB)" }
                                        input {
                                            class: "input focus-ring",
                                            r#type: "number",
                                            min: "1",
                                            step: "0.5",
                                            value: "{max_memory_mb}",
                                            oninput: move |e| max_memory_mb.set(e.value()),
                                            disabled: is_submitting(),
                                        }
                                    }
                                }

                                // Row 3: Max concurrent slots + Status
                                div {
                                    style: "display:grid; grid-template-columns:1fr 1fr; gap:14px;",
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
                                            style: "height:38px; display:flex; gap:8px; align-items:center; font-size:13px; padding:0 12px; border:1px solid var(--cf-card-border); border-radius:10px; background:rgba(255,255,255,0.02);",
                                            input {
                                                r#type: "checkbox",
                                                checked: enabled(),
                                                onchange: move |e| enabled.set(e.checked()),
                                            }
                                            span { "Enabled (accepts jobs)" }
                                        }
                                    }
                                }

                                // Builder public key
                                div {
                                    class: "field",
                                    label { "Builder public key" }
                                    textarea {
                                        class: "input focus-ring mono",
                                        rows: "3",
                                        value: "{rotated_public_key}",
                                        oninput: move |e| rotated_public_key.set(e.value()),
                                        style: "font-size:11px; resize:vertical; padding:10px;",
                                    }
                                    div { class: "help", "The public half of the keypair the builder generated on first start. Crystal Forge uses it to authenticate the builder and verify build signatures \u{2014} the private key never leaves the builder host." }
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
                                        div { style: "margin-top:10px; padding:12px; border:1px solid rgba(251, 191, 36, 0.3); border-radius:10px; background:rgba(251, 191, 36, 0.08);",
                                            div { style: "display:flex; align-items:center; justify-content:space-between; margin-bottom:6px;",
                                                span { style: "font-size:12px; font-weight:600; color:#fbbf24;", "⚠️ Generated private key — save securely before closing" }
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
                                            div { style: "margin-top:8px; font-size:11px; color:var(--cf-text-secondary);",
                                                "Copy this key now. It will be lost when you close this modal."
                                            }
                                        }
                                    }
                                    // Fingerprint (below key textarea)
                                    if !current_fingerprint().is_empty() {
                                        div {
                                            style: "margin-top:8px; padding:10px 12px; border:1px solid var(--cf-card-border); border-radius:10px; background:rgba(255,255,255,0.025); display:flex; align-items:center; gap:8px;",
                                            Icon { name: IconName::Key, size: 12 }
                                            span {
                                                style: "font-size:10px; font-weight:600; text-transform:uppercase; letter-spacing:0.08em; color:var(--cf-text-muted);",
                                                "Fingerprint"
                                            }
                                            code {
                                                class: "mono",
                                                style: "font-size:11px; color:#10b981; overflow-wrap:anywhere;",
                                                "{current_fingerprint()}"
                                            }
                                        }
                                    }
                                }

                                // Danger zone
                                div { style: "padding-top:14px; border-top:1px solid var(--cf-divider);",
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
