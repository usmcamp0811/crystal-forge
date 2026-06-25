//! Add builder modal component - matches BuildersView.jsx reference design.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{self, models::CreateBuilderRequest};
use crate::components::{Icon, IconName};
use crate::theme;

#[component]
pub fn AddBuilderModal(
    on_close: EventHandler<()>,
    on_success: EventHandler<()>,
    show_onboarding_callouts: bool,
) -> Element {
    let mut name = use_signal(|| String::new());
    let mut host = use_signal(|| String::new());
    let mut arch = use_signal(|| String::from("x86_64-linux"));
    let mut public_key = use_signal(|| String::new());
    let mut max_cpu_cores = use_signal(|| String::new());
    let mut max_memory_gib = use_signal(|| String::new());
    let mut max_concurrent_jobs = use_signal(|| String::from("1"));
    let mut selected_environments = use_signal(|| Vec::<Uuid>::new());
    let mut enabled = use_signal(|| true);

    let mut is_submitting = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    let environments = use_resource(|| async move { api::client::fetch_environments().await });

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
            host: if host().trim().is_empty() {
                None
            } else {
                Some(host().trim().to_string())
            },
            arch: arch(),
            public_key: public_key().trim().to_string(),
            max_cpu_cores: max_cpu_cores()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|&n| n > 0),
            max_memory_mb: max_memory_gib()
                .trim()
                .parse::<f64>()
                .ok()
                .map(|gib| ((gib * 1024.0).round() as i32).max(1024)),
            max_concurrent_jobs: max_concurrent_jobs()
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|&n| n > 0),
            enabled: enabled(),
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

                // Header
                div {
                    class: "modal-head",
                    h2 {
                        span {
                            style: "margin-right:6px; vertical-align:text-bottom;",
                            Icon { name: IconName::Plus, size: 14 }
                        }
                        "Register builder"
                    }
                    p { "Recognize a build worker by its public key." }
                }

                // Error message
                if let Some(err) = error_message() {
                    div {
                        class: "mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded text-red-400 text-sm",
                        "{err}"
                    }
                }

                // Form body
                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    // Info callout
                    if !show_onboarding_callouts {
                        div {
                            class: "sd-callout sd-callout-info",
                            style: "font-size:11.5px; display:block; margin-bottom:16px;",
                            div {
                                style: "display:flex; align-items:center; gap:6px; margin-bottom:6px; font-weight:600; color:var(--cf-text-secondary);",
                                Icon { name: IconName::Cpu, size: 12 }
                                "How registration works"
                            }
                            ol {
                                style: "margin:0; padding-left:18px; line-height:1.7; color:var(--cf-text-secondary);",
                                li {
                                    "Deploy the host with "
                                    span { class: "mono", "services.crystal-forge.build.api_mode = true" }
                                    ". On first start it generates its own keypair and runs — it just won't be recognized yet."
                                }
                                li {
                                    "Grab the builder's "
                                    strong { "public" }
                                    " key from that host:"
                                    br {}
                                    span { class: "mono", style: "font-size:10.5px;", "cat /var/lib/crystal-forge/builder-api.key.pub" }
                                    " (also printed in "
                                    span { class: "mono", style: "font-size:10.5px;", "journalctl -u crystal-forge-builder" }
                                    ")."
                                }
                                li { "Paste it below to register. The builder is recognized on its next check-in — no redeploy needed." }
                            }
                        }
                    }

                    // Row 1: Name + Environments (2-col grid)
                    div {
                        style: "display:grid; grid-template-columns:1fr 1fr; gap:14px; margin-bottom:16px;",

                        div {
                            class: "field",
                            label { "Name" }
                            input {
                                class: "input focus-ring mono",
                                value: "{name}",
                                oninput: move |e| name.set(e.value()),
                                placeholder: "e.g. hydra-03",
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

                    // Row 2: Host (full-width)
                    div {
                        class: "field",
                        style: "margin-bottom:16px;",
                        label { "Host (SSH endpoint)" }
                        input {
                            class: "input focus-ring mono",
                            r#type: "text",
                            value: "{host}",
                            oninput: move |e| host.set(e.value()),
                            placeholder: "hydra-03.production.cf.internal",
                            style: "font-size:12px;",
                            disabled: is_submitting(),
                        }
                    }

                    // Row 3: Architecture + Cores + Memory (3-col grid)
                    div {
                        style: "display:grid; grid-template-columns:1fr 1fr 1fr; gap:14px; margin-bottom:16px;",

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
                                value: "{max_memory_gib}",
                                oninput: move |e| max_memory_gib.set(e.value()),
                                disabled: is_submitting(),
                            }
                        }
                    }

                    // Row 4: Max Slots + Status (2-col grid)
                    div {
                        style: "display:grid; grid-template-columns:1fr 1fr; gap:14px; margin-bottom:16px;",

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
                            div {
                                class: "help",
                                "How many builds this worker may run in parallel."
                            }
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
                                    style: "accent-color:var(--cf-brand-purple);",
                                }
                                span { "Enabled (accepts jobs)" }
                            }
                        }
                    }

                    // Row 5: Public Key (full-width)
                    div {
                        class: "field",
                        label {
                            "Builder public key "
                            span { style: "color:#f87171;", "*" }
                        }
                        textarea {
                            class: "input focus-ring mono",
                            rows: "3",
                            value: "{public_key}",
                            oninput: move |e| public_key.set(e.value()),
                            placeholder: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5… crystal-forge@hostname",
                            style: "font-size:11px; resize:vertical; padding:10px; margin-top:2px;",
                            disabled: is_submitting(),
                        }
                        div {
                            class: "help",
                            "The public half of the keypair the builder generated on first start. Crystal Forge uses it to authenticate the builder and verify build signatures — the private key never leaves the builder host."
                        }

                        // Key validation feedback
                        if !public_key().trim().is_empty() {
                            {
                                let key_looks_valid = public_key().as_str().trim().starts_with("ssh-");
                                rsx! {
                                    div {
                                        style: if key_looks_valid {
                                            "margin-top:10px; padding:9px 12px; border-radius:8px; border:1px solid rgba(52,211,153,0.3); background:rgba(52,211,153,0.06); display:flex; align-items:center; gap:8px;"
                                        } else {
                                            "margin-top:10px; padding:9px 12px; border-radius:8px; border:1px solid rgba(248,113,113,0.35); background:rgba(248,113,113,0.06); display:flex; align-items:center; gap:8px;"
                                        },
                                        span {
                                            style: if key_looks_valid {
                                                "color:#34d399; flex-shrink:0;"
                                            } else {
                                                "color:#f87171; flex-shrink:0;"
                                            },
                                            Icon {
                                                name: if key_looks_valid { IconName::Key } else { IconName::Warn },
                                                size: 13
                                            }
                                        }
                                        if key_looks_valid {
                                            div {
                                                style: "min-width:0;",
                                                div {
                                                    style: "font-size:10px; text-transform:uppercase; letter-spacing:0.06em; color:var(--cf-text-muted); font-weight:600;",
                                                    "Key format looks valid"
                                                }
                                            }
                                        } else {
                                            span {
                                                style: "font-size:11.5px; color:#fca5a5;",
                                                "Doesn't look like an SSH public key — expected it to start with "
                                                span { class: "mono", "ssh-ed25519" }
                                                "."
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Footer
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
                        disabled: is_submitting() || !public_key().as_str().trim().starts_with("ssh-"),
                        Icon { name: IconName::Check, size: 13 }
                        if is_submitting() {
                            " Creating..."
                        } else {
                            " Register builder"
                        }
                    }
                }
            }
        }
    }
}
