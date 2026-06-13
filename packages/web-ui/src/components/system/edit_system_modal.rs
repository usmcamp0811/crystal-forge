//! Modal for editing system configuration.
//!
//! Matches the design EditSystemModal layout: two-column hostname+environment,
//! flake assignment section, segmented deployment mode, pinned commit picker.

use crate::api::models::{CommitInfo, SystemDetail, UpdateSystemRequest};
use crate::theme;
use dioxus::prelude::*;

/// Branch options for the flake branch field.
const BRANCHES: &[&str] = &["main", "staging", "dev"];

/// Derived FQDN from hostname and environment.
fn derived_fqdn(hostname: &str, environment: Option<&str>) -> String {
    let env = environment.unwrap_or("unknown").to_lowercase();
    format!("{hostname}.{env}.cf.internal")
}

/// Derived branch from environment.
fn derived_branch(environment: Option<&str>) -> &'static str {
    match environment.unwrap_or("dev").to_lowercase().as_str() {
        "production" | "prod" => "main",
        "staging" | "stage" => "staging",
        _ => "dev",
    }
}

#[component]
pub fn EditSystemModal(
    system: SystemDetail,
    flake_names: Vec<String>,
    #[props(default)] environments: Vec<String>,
    #[props(default)] recent_commits: Vec<CommitInfo>,
    #[props(default)] error_message: Option<String>,
    on_close: EventHandler<()>,
    on_save: EventHandler<UpdateSystemRequest>,
) -> Element {
    let mut hostname = use_signal(|| system.hostname.clone());
    let mut environment = use_signal(|| system.environment.clone().unwrap_or_default());
    let mut fqdn = use_signal(|| derived_fqdn(&system.hostname, system.environment.as_deref()));
    let mut system_configuration_name =
        use_signal(|| system.system_configuration_name.clone().unwrap_or_default());
    let mut deployment_policy = use_signal(|| system.deployment_policy.clone());
    let mut flake_name = use_signal(|| {
        system
            .flake
            .as_ref()
            .map(|flake| flake.name.clone())
            .unwrap_or_default()
    });
    let mut flake_branch = use_signal(|| derived_branch(system.environment.as_deref()).to_string());
    let mut is_saving = use_signal(|| false);
    let mut show_danger = use_signal(|| false);

    // Sync FQDN when hostname or environment changes
    {
        let hostname_clone = hostname.clone();
        let environment_clone = environment.clone();
        let mut fqdn_clone = fqdn.clone();
        use_effect(move || {
            let h = hostname_clone.read().clone();
            let e = environment_clone.read().clone();
            let env_opt = if e.is_empty() { None } else { Some(e.as_str()) };
            fqdn_clone.set(derived_fqdn(&h, env_opt));
        });
    }

    {
        let error_message = error_message.clone();
        use_effect(move || {
            if error_message.is_some() {
                is_saving.set(false);
            }
        });
    }

    let handle_save = move |_| {
        is_saving.set(true);

        let request = UpdateSystemRequest {
            hostname: hostname.read().clone(),
            system_configuration_name: if system_configuration_name.read().trim().is_empty() {
                None
            } else {
                Some(system_configuration_name.read().clone())
            },
            environment: if environment.read().trim().is_empty() {
                None
            } else {
                Some(environment.read().clone())
            },
            flake_name: if flake_name.read().trim().is_empty() {
                None
            } else {
                Some(flake_name.read().clone())
            },
            deployment_policy: deployment_policy.read().clone(),
        };

        on_save.call(request);
    };

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),

            div {
                class: "modal",
                style: "width:min(620px,96vw); max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                div {
                    class: "modal-head",
                    h2 {
                        "Edit {system.hostname}"
                    }
                    p {
                        "Update system registration, flake assignment, and deployment policy."
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    // Two-column: Hostname + Environment (design layout)
                    div {
                        style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                        div {
                            class: "field",
                            label { class: "label", "Hostname" }
                            input {
                                r#type: "text",
                                class: "input focus-ring mono",
                                value: "{hostname}",
                                oninput: move |e| hostname.set(e.value().clone()),
                            }
                        }
                        div {
                            class: "field",
                            label { class: "label", "Environment" }
                            if !environments.is_empty() {
                                select {
                                    class: "input focus-ring",
                                    value: "{environment}",
                                    onchange: move |e| environment.set(e.value().clone()),
                                    option { value: "", "— none —" }
                                    for env_name in &environments {
                                        option {
                                            value: "{env_name}",
                                            selected: *environment.read() == *env_name,
                                            "{env_name}"
                                        }
                                    }
                                }
                            } else {
                                input {
                                    r#type: "text",
                                    class: "input focus-ring",
                                    value: "{environment}",
                                    placeholder: "e.g., production, staging",
                                    oninput: move |e| environment.set(e.value().clone()),
                                }
                            }
                        }
                    }

                    // FQDN field (design: below hostname+environment)
                    div {
                        class: "field",
                        label { class: "label", "FQDN" }
                        input {
                            r#type: "text",
                            class: "input focus-ring mono",
                            value: "{fqdn}",
                            oninput: move |e| fqdn.set(e.value().clone()),
                        }
                    }

                    // System Configuration Name
                    div {
                        class: "field",
                        label { class: "label", "System Configuration Name" }
                        input {
                            r#type: "text",
                            class: "input focus-ring mono",
                            value: "{system_configuration_name}",
                            placeholder: "Defaults to hostname if not set",
                            oninput: move |e| system_configuration_name.set(e.value().clone()),
                        }
                    }

                    // Flake assignment section (design: grouped in a bordered box)
                    div {
                        style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg));",
                        div {
                            style: "display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 13px; font-weight: 600;",
                            "Flake assignment"
                        }
                        div {
                            style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                            div {
                                class: "field",
                                label { class: "label", "Flake" }
                                select {
                                    class: "input focus-ring",
                                    value: "{flake_name}",
                                    onchange: move |e| flake_name.set(e.value().clone()),
                                    option {
                                        value: "",
                                        selected: flake_name.read().is_empty(),
                                        "— none —"
                                    }
                                    for name in &flake_names {
                                        option {
                                            value: "{name}",
                                            selected: *flake_name.read() == *name,
                                            "{name}"
                                        }
                                    }
                                }
                                if flake_name.read().is_empty() {
                                    p {
                                        class: "text-xs text-amber-400 mt-1",
                                        "⚠ No flake linked — won't be included in evaluations."
                                    }
                                }
                            }
                            div {
                                class: "field",
                                label { class: "label", "Branch" }
                                select {
                                    class: "input focus-ring",
                                    value: "{flake_branch}",
                                    onchange: move |e| flake_branch.set(e.value().clone()),
                                    for b in BRANCHES {
                                        option {
                                            value: "{b}",
                                            selected: *flake_branch.read() == *b,
                                            "{b}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Deployment mode (design: segmented buttons)
                    div {
                        class: "field",
                        label { class: "label", "Deployment mode" }
                        div {
                            class: "seg",
                            style: "width: fit-content; flex-wrap: wrap;",
                            button {
                                class: if *deployment_policy.read() == "manual" { "active" } else { "" },
                                onclick: move |_| deployment_policy.set("manual".to_string()),
                                "Manual"
                            }
                            button {
                                class: if *deployment_policy.read() == "auto_latest" { "active" } else { "" },
                                onclick: move |_| deployment_policy.set("auto_latest".to_string()),
                                "Auto Latest"
                            }
                            button {
                                class: if *deployment_policy.read() == "pinned" { "active" } else { "" },
                                onclick: move |_| deployment_policy.set("pinned".to_string()),
                                "Pinned"
                            }
                        }
                        p {
                            class: "text-xs {theme::text::SECONDARY} mt-1",
                            match deployment_policy.read().as_str() {
                                "manual" => "Operator must explicitly approve every deploy.",
                                "auto_latest" => "Automatically deploy the latest commit.",
                                "pinned" => "Deploy only specific pinned commits.",
                                _ => ""
                            }
                        }
                    }

                    // Pinned commit picker (design: only shown when mode is "pinned")
                    if *deployment_policy.read() == "pinned" && !recent_commits.is_empty() {
                        div {
                            class: "field",
                            label { class: "label", "Pinned commit" }
                            div {
                                class: "sd-commit-list",
                                style: "max-height: 200px;",
                                for commit in &recent_commits {
                                    button {
                                        class: "sd-commit-item focus-ring",
                                        onclick: move |_| {},
                                        div { class: "sd-commit-sha", "{commit.short_sha}" }
                                        div { class: "sd-commit-msg", "{commit.message}" }
                                        div { class: "sd-commit-meta mono", "{commit.author}" }
                                        div { class: "sd-commit-meta", "{commit.timestamp}" }
                                    }
                                }
                            }
                            p {
                                class: "text-xs {theme::text::SECONDARY} mt-1",
                                "System will not auto-advance off this commit."
                            }
                        }
                    }

                    // Reachability section placeholder (design: Direct/LAN vs Agent pull-only)
                    // Requires backend support for reachability_mode, server_address fields.
                    div {
                        style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg)); opacity: 0.55;",
                        "data-testid": "reachability-placeholder",
                        title: "Reachability settings require backend support (coming soon)",
                        div {
                            style: "display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 13px; font-weight: 600;",
                            "Reachability"
                        }
                        div {
                            class: "field",
                            label { class: "label", "How the server reaches this system" }
                            div {
                                class: "seg",
                                style: "width: fit-content;",
                                button { class: "active", disabled: "true", "Direct / LAN" }
                                button { disabled: "true", "Agent pull-only" }
                            }
                            p { class: "text-xs {theme::text::SECONDARY} mt-1", "Server can open connections to the agent (same LAN / routable / VPN). Enables server-initiated deploys and live log tail." }
                        }
                    }

                    // Two-column: Heartbeat interval + Tags (design placeholders)
                    div {
                        style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin-top: 8px; opacity: 0.55;",
                        "data-testid": "heartbeat-tags-placeholder",
                        title: "Heartbeat interval and tags require backend support (coming soon)",
                        div {
                            class: "field",
                            label { class: "label", "Heartbeat interval" }
                            select {
                                class: "input focus-ring",
                                disabled: "true",
                                option { "60 seconds" }
                            }
                            p { class: "text-xs {theme::text::SECONDARY} mt-1", "Agent heartbeat cadence (backend field coming soon)." }
                        }
                        div {
                            class: "field",
                            label { class: "label", "Tags" }
                            input {
                                class: "input focus-ring",
                                disabled: "true",
                                placeholder: "e.g. builder, stig-enforced (requires backend)",
                            }
                            p { class: "text-xs {theme::text::SECONDARY} mt-1", "Free-form labels for grouping & filtering." }
                        }
                    }

                    // Description / notes placeholder
                    div {
                        class: "field",
                        style: "opacity: 0.55;",
                        "data-testid": "description-placeholder",
                        title: "System description requires backend support (coming soon)",
                        label { class: "label", "Description / notes" }
                        textarea {
                            class: "input focus-ring",
                            rows: "2",
                            disabled: "true",
                            placeholder: "Optional context for operators (requires backend support)",
                            style: "resize: vertical;",
                        }
                    }

                    // Danger zone (design: remove system)
                    div {
                        style: "margin-top: 10px; padding-top: 14px; border-top: 1px solid var(--cf-divider);",
                        if show_danger() {
                            div {
                                style: "display: flex; align-items: center; gap: 8px;",
                                span {
                                    style: "font-size: 12px; color: var(--cf-text-secondary);",
                                    "Remove "
                                    span { class: "mono", style: "color: #fecaca; font-weight: 700;", "{hostname}" }
                                    " from the registry?"
                                }
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    style: "color: #f87171; border-color: rgba(248,113,113,0.3); font-size: 12px;",
                                    onclick: move |_| show_danger.set(false),
                                    "Cancel"
                                }
                            }
                        } else {
                            button {
                                class: "btn btn-ghost focus-ring",
                                style: "color: #f87171; border-color: rgba(248,113,113,0.3); font-size: 12px;",
                                onclick: move |_| show_danger.set(true),
                                "Remove system from registry"
                            }
                        }
                    }

                    if let Some(message) = &error_message {
                        div {
                            class: "rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-200",
                            "{message}"
                        }
                    }
                }

                div {
                    class: "modal-foot",

                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| on_close.call(()),
                        disabled: is_saving(),
                        "Cancel"
                    }

                    button {
                        class: "btn btn-primary focus-ring disabled:opacity-50 disabled:cursor-not-allowed",
                        onclick: handle_save,
                        disabled: is_saving() || hostname.read().trim().is_empty(),

                        if is_saving() {
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                }
            }
        }
    }
}
