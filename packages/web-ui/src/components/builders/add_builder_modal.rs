//! Add builder modal component with keypair generation.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::CreateBuilderRequest};
use crate::components::builders::generate_ed25519_keypair;
use crate::theme;

#[component]
pub fn AddBuilderModal(
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
    show_onboarding_callouts: bool,
) -> Element {
    let mut name = use_signal(|| String::new());
    let mut public_key = use_signal(|| String::new());
    let mut private_key = use_signal(|| String::new());
    let mut max_cpu_cores = use_signal(|| String::new());
    let mut max_memory_mb = use_signal(|| String::new());
    let mut max_concurrent_jobs = use_signal(|| String::from("1"));
    let mut selected_environments = use_signal(|| Vec::<Uuid>::new());

    let mut show_private_key = use_signal(|| false);
    let mut is_submitting = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut show_name_callout = use_signal(|| show_onboarding_callouts);
    let mut show_public_key_callout = use_signal(|| show_onboarding_callouts);
    let mut show_environment_callout = use_signal(|| show_onboarding_callouts);

    // Fetch environments for multi-select
    let environments = use_resource(|| async move { api::client::fetch_environments().await });

    let generate_keypair = move |_| match generate_ed25519_keypair() {
        Ok((priv_hex, pub_b64)) => {
            private_key.set(priv_hex);
            public_key.set(pub_b64);
        }
        Err(e) => {
            error_message.set(Some(format!("Failed to generate keypair: {}", e)));
        }
    };

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

        let request = CreateBuilderRequest {
            name: name().trim().to_string(),
            public_key: public_key().trim().to_string(),
            max_cpu_cores: max_cpu_cores()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|&n| n > 0),
            max_memory_mb: max_memory_mb()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|&n| n > 0),
            max_concurrent_jobs: max_concurrent_jobs()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|&n| n > 0),
            environment_ids: selected_environments(),
        };

        match api::client::create_builder(&request).await {
            Ok(_) => {
                on_success.call(());
            }
            Err(e) => {
                error_message.set(Some(format!("Failed to create builder: {}", e)));
                is_submitting.set(false);
            }
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

                // Header
                div {
                    class: "flex items-center justify-between mb-6",
                    h2 {
                        class: "text-xl font-semibold text-white",
                        "Add Builder"
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
                            span { class: "text-red-400", " *" }
                        }
                        input {
                            class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white placeholder-slate-500 focus:outline-none focus:border-blue-500",
                            r#type: "text",
                            placeholder: "e.g., builder-01",
                            value: "{name}",
                            onfocus: move |_| show_name_callout.set(false),
                            oninput: move |e| {
                                show_name_callout.set(false);
                                name.set(e.value())
                            },
                            disabled: is_submitting(),
                        }
                        if show_name_callout() {
                            div {
                                "data-testid": "setup-coach-builder-field-name",
                                style: "position:relative; margin-top:10px; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                div {
                                    style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                }
                                p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                p { style: "margin:2px 0 0 0;", "Name this builder so operators can identify where builds run (for example: build-eu-west-1)." }
                            }
                        }
                    }

                    // Keypair section
                    div {
                        class: "border border-slate-700 rounded p-4 space-y-3",
                        div {
                            class: "flex items-center justify-between mb-2",
                            h3 {
                                class: "text-sm font-medium {theme::text::PRIMARY}",
                                "Authentication Keypair"
                            }
                            button {
                                class: "px-3 py-1 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                                onclick: generate_keypair,
                                disabled: is_submitting(),
                                "🔑 Generate Keypair"
                            }
                        }
                        p {
                            class: "text-xs {theme::text::SECONDARY}",
                            "Generate a new Ed25519 keypair for this builder. Save the private key securely - you'll need it to configure the builder binary."
                        }

                        // Public Key
                        div {
                            label {
                                class: "block text-sm font-medium {theme::text::PRIMARY} mb-1",
                                "Public Key"
                                span { class: "text-red-400", " *" }
                            }
                            textarea {
                                class: "w-full px-3 py-2 bg-slate-900 border border-slate-700 rounded text-white placeholder-slate-500 focus:outline-none focus:border-blue-500 font-mono text-xs",
                                rows: "2",
                                placeholder: "Base64-encoded public key",
                                value: "{public_key}",
                                onfocus: move |_| show_public_key_callout.set(false),
                                oninput: move |e| {
                                    show_public_key_callout.set(false);
                                    public_key.set(e.value())
                                },
                                disabled: is_submitting(),
                            }
                            if show_public_key_callout() {
                                div {
                                    "data-testid": "setup-coach-builder-field-public-key",
                                    style: "position:relative; margin-top:10px; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                    div {
                                        style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                    }
                                    p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                    p { style: "margin:2px 0 0 0;", "Use the builder public key from the host's Crystal Forge builder config, or generate one and install the paired private key on the builder host." }
                                }
                            }
                        }

                        // Private Key (show/hide)
                        if !private_key().is_empty() {
                            div {
                                div {
                                    class: "flex items-center justify-between mb-1",
                                    label {
                                        class: "block text-sm font-medium text-amber-400",
                                        "Private Key"
                                        span { class: "text-xs {theme::text::SECONDARY} ml-2", "(save this securely)" }
                                    }
                                    button {
                                        class: "text-xs text-blue-400 hover:text-blue-300",
                                        onclick: move |_| show_private_key.set(!show_private_key()),
                                        if show_private_key() { "Hide" } else { "Show" }
                                    }
                                }
                                if show_private_key() {
                                    textarea {
                                        class: "w-full px-3 py-2 bg-amber-900/20 border border-amber-700/50 rounded text-amber-200 font-mono text-xs",
                                        rows: "2",
                                        readonly: true,
                                        value: "{private_key}",
                                    }
                                } else {
                                    div {
                                        class: "px-3 py-2 bg-slate-900 border border-slate-700 rounded text-slate-500 font-mono text-xs",
                                        "••••••••••••••••••••••••••••••••"
                                    }
                                }
                            }
                        }
                    }

                    // Resource Limits
                    div {
                        class: "grid grid-cols-3 gap-4",
                        div {
                            class: "col-span-3 mb-3",
                            style: "position:relative; margin-top:4px; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                            div {
                                style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                            }
                            p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                            p { style: "margin:2px 0 0 0;", "Leave CPU or memory empty only when this host is dedicated and heavily provisioned for builder workloads." }
                            p { style: "margin:2px 0 0 0;", "Max Concurrent Jobs controls how many builds run at once. If CPU/memory are unlimited and concurrency is greater than 1, heavy builds (for example Firefox or Chromium) can exhaust resources and stall or fail repeatedly." }
                            p { style: "margin:2px 0 0 0;", "Safer default: keep concurrency at 1 and set explicit CPU/memory limits close to what this host can sustain." }
                        }
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
                                span { class: "text-red-400", " *" }
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
                                                                id: "env-{env.id}",
                                                                class: "rounded border-slate-600 text-blue-600 focus:ring-blue-500",
                                                                checked: selected_environments().contains(&env.id),
                                                                onchange: move |_| {
                                                                    show_environment_callout.set(false);
                                                                    toggle_environment(env_id)
                                                                },
                                                                disabled: is_submitting(),
                                                            }
                                                            label {
                                                                r#for: "env-{env.id}",
                                                                class: "text-sm {theme::text::PRIMARY} cursor-pointer",
                                                                "{env.name}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if show_environment_callout() {
                                        div {
                                            "data-testid": "setup-coach-builder-field-environments",
                                            style: "position:relative; margin-top:10px; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                            div {
                                                style: "position:absolute; top:-6px; left:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                            }
                                            p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                            p { style: "margin:2px 0 0 0;", "Select environments this builder should serve, or leave empty to allow all environments." }
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
                }

                // Footer buttons
                div {
                    class: "flex justify-end gap-3 mt-6 pt-4 border-t border-slate-700",
                    button {
                        class: "px-4 py-2 text-slate-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        disabled: is_submitting(),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-sm font-medium text-white transition-colors {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-50 disabled:cursor-not-allowed",
                        onclick: handle_submit,
                        disabled: is_submitting() || name().trim().is_empty() || public_key().trim().is_empty(),
                        if is_submitting() {
                            "Creating..."
                        } else {
                            "Create Builder"
                        }
                    }
                }
            }
        }
    }
}
