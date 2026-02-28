//! Add builder modal component with keypair generation.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::CreateBuilderRequest};
use crate::theme;

#[component]
pub fn AddBuilderModal(on_close: EventHandler<()>, on_success: EventHandler<()>) -> Element {
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

    // Fetch environments for multi-select
    let environments = use_resource(|| async move {
        api::client::fetch_environments().await
    });

    let generate_keypair = move |_| {
        // Generate Ed25519 keypair using web-sys crypto
        // For now, we'll use a placeholder - real implementation needs ed25519-dalek in WASM
        
        // TODO: Implement real Ed25519 keypair generation
        // This requires ed25519-dalek compiled for wasm32
        
        let placeholder_private = "0000000000000000000000000000000000000000000000000000000000000000";
        let placeholder_public = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        
        private_key.set(placeholder_private.to_string());
        public_key.set(placeholder_public.to_string());
        
        // In production:
        // 1. Generate random 32 bytes for private key
        // 2. Derive public key from private key
        // 3. Encode private key as hex
        // 4. Encode public key as base64
    };

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
            class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
            onclick: move |_| {
                if !is_submitting() {
                    on_close.call(())
                }
            },
            
            div {
                class: "bg-slate-800 border border-slate-700 rounded-lg p-6 max-w-2xl w-full mx-4 max-h-[90vh] overflow-y-auto",
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
                            oninput: move |e| name.set(e.value()),
                            disabled: is_submitting(),
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
                                class: "px-3 py-1 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors",
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
                                oninput: move |e| public_key.set(e.value()),
                                disabled: is_submitting(),
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
                                                    id: "env-{env.id}",
                                                    class: "rounded border-slate-600 text-blue-600 focus:ring-blue-500",
                                                    checked: selected_environments().contains(&env.id),
                                                    onchange: move |_| toggle_environment(env.id),
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
                    class: "flex justify-end gap-3 mt-6 pt-4 border-t border-slate-700",
                    button {
                        class: "px-4 py-2 text-slate-400 hover:text-white transition-colors",
                        onclick: move |_| on_close.call(()),
                        disabled: is_submitting(),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
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
