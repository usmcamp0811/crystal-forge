//! Modal for editing system configuration.
//!
//! Tabbed modal with General, Deployment, Security, and Danger tabs.
//! Includes SSH key rotation flow in the Security tab.

use crate::api::models::{CommitInfo, FieldUpdate, SystemDetail, UpdateSystemRequest};
use crate::components::icon::{Icon, IconName};
use crate::components::modals::RemoveSystemDialog;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Deployment,
    Security,
    Danger,
}

#[derive(Clone, Copy, PartialEq)]
enum KeyMode {
    Generate,
    Paste,
}

#[derive(Clone)]
struct GeneratedKeys {
    pub_key: String,
    priv_key: String,
    fingerprint: String,
}

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
    #[props(default)] remove_in_progress: bool,
    #[props(default)] remove_error_message: Option<String>,
    on_close: EventHandler<()>,
    on_save: EventHandler<UpdateSystemRequest>,
    on_delete: EventHandler<()>,
) -> Element {
    let mut hostname = use_signal(|| system.hostname.clone());
    let mut environment = use_signal(|| system.environment.clone().unwrap_or_default());
    let mut fqdn = use_signal(|| {
        system
            .fqdn
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| derived_fqdn(&system.hostname, system.environment.as_deref()))
    });
    let mut fqdn_manually_edited = use_signal(|| {
        system
            .fqdn
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
    });
    let system_configuration_name = system.system_configuration_name.clone().unwrap_or_default();
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
    let mut show_remove_modal = use_signal(|| false);
    // Pinned commit selection. Seeds from the system's latest known commit so the picker
    // highlights the active pin. Wired to the real `/systems/:id/commits` data passed in
    // via `recent_commits`.
    let current_commit_sha = system
        .flake
        .as_ref()
        .and_then(|flake| flake.latest_commit.clone());
    let mut pinned_commit = use_signal(|| {
        current_commit_sha
            .clone()
            .or_else(|| recent_commits.first().map(|commit| commit.sha.clone()))
            .unwrap_or_default()
    });

    // Seed heartbeat interval from the persisted system value.
    // 0 is a sentinel meaning "use server default" (NULL in the database).
    // When the system has no per-system override (heartbeat_interval_secs is None),
    // we show "Use server default" (0) so the user can see the current state.
    let mut heartbeat_interval_sec = use_signal(|| system.heartbeat_interval_secs.unwrap_or(0));
    let mut tags_draft = use_signal(String::new);

    // Tab state
    let mut active_tab = use_signal(|| Tab::General);

    // SSH key rotation state
    let mut rotating_key = use_signal(|| false);
    let mut key_mode = use_signal(|| KeyMode::Generate);
    let mut new_pub_key = use_signal(String::new);
    let mut generated_keys = use_signal(|| None::<GeneratedKeys>);
    let mut priv_copied = use_signal(|| false);
    let mut rotated = use_signal(|| false);

    // Mock fingerprints (in real implementation, these would come from the server)
    let current_fingerprint = "SHA256:jKLm8NoPqRsTuVwXyZ0123456789ABCDEfghijk";
    let mut new_fingerprint =
        use_signal(|| "SHA256:newFingerprintAfterRotation1234567890ABC".to_string());

    // Sync FQDN when hostname or environment changes
    {
        let hostname_clone = hostname.clone();
        let environment_clone = environment.clone();
        let mut fqdn_clone = fqdn.clone();
        use_effect(move || {
            let h = hostname_clone.read().clone();
            let e = environment_clone.read().clone();
            let env_opt = if e.is_empty() { None } else { Some(e.as_str()) };
            if !*fqdn_manually_edited.read() {
                fqdn_clone.set(derived_fqdn(&h, env_opt));
            }
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

    // SSH key validation
    let new_key_valid = {
        let key = new_pub_key.read();
        key.trim().starts_with("ssh-ed25519")
    };

    // Generate keypair (mock implementation for design)
    let gen_keypair = move |_| {
        let mock_pub = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMockGeneratedPublicKey1234567890ABCDEF crystal-forge@system".to_string();
        let mock_priv = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\nQyNTUxOQAAACBNb2NrR2VuZXJhdGVkUHJpdmF0ZUtleTEyMzQ1Njc4OTBBQkNERUYAAAAA\nAQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8wMTIzND\nU2Nzg5Ojs8PT4/QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gYWJjZGVmZ2hp\namtsbW5vcHFyc3R1dnd4eXp7fH1+fw==\n-----END OPENSSH PRIVATE KEY-----".to_string();
        let mock_fingerprint = "SHA256:MockGeneratedFingerprint7890ABCDEFGH".to_string();

        generated_keys.set(Some(GeneratedKeys {
            pub_key: mock_pub,
            priv_key: mock_priv,
            fingerprint: mock_fingerprint.clone(),
        }));
        new_fingerprint.set(mock_fingerprint);
    };

    let handle_save = move |_| {
        is_saving.set(true);

        // FieldUpdate semantics for heartbeat_interval_secs:
        // - Sentinel 0 → Clear (set DB column to NULL → agent uses server default)
        // - Any other value that differs from original → Set(value)
        // - Unchanged → Unset (omit the field; server preserves stored value)
        let current_heartbeat_interval = *heartbeat_interval_sec.read();
        let original_heartbeat_interval = system.heartbeat_interval_secs.unwrap_or(0);

        let heartbeat_interval = if current_heartbeat_interval == original_heartbeat_interval {
            FieldUpdate::Unset
        } else if current_heartbeat_interval == 0 {
            // User selected "Use server default" → clear the per-system override.
            FieldUpdate::Clear
        } else {
            FieldUpdate::Set(current_heartbeat_interval)
        };

        let request = UpdateSystemRequest {
            hostname: hostname.read().clone(),
            fqdn: if fqdn.read().trim().is_empty() {
                None
            } else {
                Some(fqdn.read().trim().to_string())
            },
            system_configuration_name: if system_configuration_name.trim().is_empty() {
                None
            } else {
                Some(system_configuration_name.clone())
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
            heartbeat_interval_secs: heartbeat_interval,
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
                        span { style: "margin-right:6px; display:inline-flex; vertical-align:text-bottom;",
                            Icon { name: IconName::Gear, size: 14 }
                        }
                        "Edit {system.hostname}"
                    }
                    p {
                        "Update system registration, flake assignment, deployment policy, and security settings."
                    }
                }

                // Tab bar
                div {
                    style: "border-bottom: 1px solid var(--cf-divider); padding: 0 20px;",
                    div {
                        class: "seg",
                        style: "width: fit-content; margin: 0;",
                        button {
                            class: if *active_tab.read() == Tab::General { "active" } else { "" },
                            onclick: move |_| active_tab.set(Tab::General),
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Gear, size: 12 }
                            }
                            "General"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Deployment { "active" } else { "" },
                            onclick: move |_| active_tab.set(Tab::Deployment),
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Git, size: 12 }
                            }
                            "Deployment"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Security { "active" } else { "" },
                            onclick: move |_| active_tab.set(Tab::Security),
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Key, size: 12 }
                            }
                            "Security"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Danger { "active" } else { "" },
                            onclick: move |_| active_tab.set(Tab::Danger),
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Warn, size: 12 }
                            }
                            "Danger"
                        }
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    // GENERAL TAB
                    if *active_tab.read() == Tab::General {
                        div {
                            // Two-column: Hostname + Environment (design layout)
                            div {
                                style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                                div {
                                    class: "field",
                                    label { "Hostname" }
                                    input {
                                        r#type: "text",
                                        class: "input focus-ring mono",
                                        value: "{hostname}",
                                        oninput: move |e| hostname.set(e.value().clone()),
                                    }
                                }
                                div {
                                    class: "field",
                                    label { "Environment" }
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
                                label { "FQDN" }
                                input {
                                    r#type: "text",
                                    class: "input focus-ring mono",
                                    value: "{fqdn}",
                                    oninput: move |e| {
                                        fqdn_manually_edited.set(true);
                                        fqdn.set(e.value().clone());
                                    },
                                }
                                p { class: "help", "Saved as this system's operator-managed FQDN. Clear it to fall back to hostname + environment." }
                            }

                            // Reachability section (design: Direct/LAN vs Agent pull-only)
                            div {
                                style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg));",
                                div {
                                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 13px; font-weight: 600;",
                                    Icon { name: IconName::Server, size: 13 }
                                    " Reachability"
                                }
                                div {
                                    class: "field",
                                    label { "How the server reaches this system" }
                                    div {
                                        class: "seg",
                                        style: "width: fit-content;",
                                        button { class: "active", "Direct / LAN" }
                                        button { "Agent pull-only" }
                                    }
                                    p { class: "help", "Server can open connections to the agent (same LAN / routable / VPN). Enables server-initiated deploys and live log tail." }
                                }
                            }

                            // Server address field
                            div {
                                class: "field",
                                label { "Server address" }
                                input {
                                    r#type: "text",
                                    class: "input focus-ring mono",
                                    value: "https://crystal-forge.internal",
                                    disabled: true,
                                }
                                p { class: "help", "The Crystal Forge server this agent checks in with. Set during agent bootstrap." }
                            }

                            // Two-column: Tags + Description combined in one section
                            div {
                                class: "field",
                                label {
                                    "Tags "
                                    span { style: "color: var(--cf-text-muted); font-weight: 400;", "· free-form labels for grouping & filtering" }
                                }
                                input {
                                    r#type: "text",
                                    class: "input focus-ring",
                                    value: "{tags_draft}",
                                    placeholder: "e.g. builder, stig-enforced",
                                    oninput: move |e| tags_draft.set(e.value().clone()),
                                }
                                p { class: "help", "Free-form labels for grouping & filtering. Click a tag in System Detail to slice the fleet by it." }
                            }

                            // Description / notes (local-only; no systems.description column yet).
                            div {
                                class: "field",
                                "data-testid": "description-field",
                                label { "Description / notes" }
                                textarea {
                                    class: "input focus-ring",
                                    rows: "2",
                                    placeholder: "Optional context for operators…",
                                    style: "resize: vertical;",
                                }
                                p { class: "help", "Optional context for operators. Not persisted to the backend yet." }
                            }
                        }
                    }

                    // DEPLOYMENT TAB
                    if *active_tab.read() == Tab::Deployment {
                        div {
                            // Flake assignment section (design: grouped in a bordered box)
                            div {
                                style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg));",
                                div {
                                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 13px; font-weight: 600;",
                                    Icon { name: IconName::Git, size: 13 }
                                    " Flake assignment"
                                }
                                div {
                                    style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                                    div {
                                        class: "field",
                                        label { "Flake" }
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
                                        label { "Branch" }
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
                                label { "Deployment mode" }
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
                                    class: "help",
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
                                    label { "Pinned commit" }
                                    div {
                                        class: "sd-commit-list",
                                        style: "max-height: 200px;",
                                        for commit in recent_commits.iter().cloned() {
                                            {
                                                let is_selected = *pinned_commit.read() == commit.sha;
                                                let is_current = current_commit_sha
                                                    .as_deref()
                                                    .map(|sha| sha == commit.sha)
                                                    .unwrap_or(false);
                                                let sha = commit.sha.clone();
                                                rsx! {
                                                    button {
                                                        class: if is_selected { "sd-commit-item focus-ring selected" } else { "sd-commit-item focus-ring" },
                                                        onclick: move |_| pinned_commit.set(sha.clone()),
                                                        span { class: "mono sd-commit-sha", "{commit.short_sha}" }
                                                        span { class: "sd-commit-msg", "{commit.message}" }
                                                        span { class: "sd-commit-meta mono", "{commit.author}" }
                                                        span { class: "sd-commit-meta", "{commit.timestamp}" }
                                                        if is_current {
                                                            span { class: "chip chip-info", style: "font-size:10px;", "current" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    p {
                                        class: "help",
                                        "System will not auto-advance off this commit. Operators can change the pin or temporarily deploy a different commit from System Detail."
                                    }
                                }
                            }

                            // Heartbeat interval
                            div {
                                class: "field",
                                "data-testid": "heartbeat-interval-field",
                                label { "Heartbeat interval" }
                                select {
                                    class: "input focus-ring",
                                    value: "{heartbeat_interval_sec}",
                                    onchange: move |e| {
                                        if let Ok(value) = e.value().parse::<i32>() {
                                            heartbeat_interval_sec.set(value);
                                        }
                                    },
                                    // Sentinel 0 = "use server default" (clears the per-system DB override).
                                    option { value: "0",   selected: *heartbeat_interval_sec.read() == 0,   "Use server default" }
                                    option { value: "30",  selected: *heartbeat_interval_sec.read() == 30,  "30 seconds" }
                                    option { value: "60",  selected: *heartbeat_interval_sec.read() == 60,  "1 minute" }
                                    option { value: "90",  selected: *heartbeat_interval_sec.read() == 90,  "90 seconds" }
                                    option { value: "120", selected: *heartbeat_interval_sec.read() == 120, "2 minutes" }
                                    option { value: "300", selected: *heartbeat_interval_sec.read() == 300, "5 minutes" }
                                    option { value: "600", selected: *heartbeat_interval_sec.read() == 600, "10 minutes" }
                                }
                                p { class: "help", "Agent heartbeat cadence. \"Use server default\" clears any per-system override (inherits the server-configured default). Takes effect on the agent's next check-in after saving." }
                            }
                        }
                    }

                    // SECURITY TAB
                    if *active_tab.read() == Tab::Security {
                        div {
                            div {
                                style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg));",
                                div {
                                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 13px; font-weight: 600;",
                                    Icon { name: IconName::Key, size: 13 }
                                    " Agent identity"
                                }

                                if !*rotating_key.read() {
                                    // Display current fingerprint and rotate button
                                    div {
                                        class: "field",
                                        label { "Current public key fingerprint" }
                                        div {
                                            class: "mono",
                                            style: "font-size: 12px; word-break: break-all; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 6px;",
                                            if *rotated.read() {
                                                "{new_fingerprint}"
                                            } else {
                                                "{current_fingerprint}"
                                            }
                                        }
                                    }

                                    if *rotated.read() {
                                        div {
                                            class: "sd-callout sd-callout-healthy",
                                            style: "margin-top: 8px;",
                                            span { style: "margin-right: 6px; display:inline-flex; vertical-align:text-bottom;",
                                                Icon { name: IconName::Check, size: 13 }
                                            }
                                            div { style: "font-size: 12px;",
                                                "Key rotated. The old key is revoked immediately — the agent will re-register with the new key on its next heartbeat."
                                            }
                                        }
                                    } else {
                                        button {
                                            class: "btn btn-ghost focus-ring",
                                            style: "margin-top: 4px;",
                                            onclick: move |_| rotating_key.set(true),
                                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                Icon { name: IconName::Sync, size: 12 }
                                            }
                                            "Rotate key"
                                        }
                                    }
                                } else {
                                    // Key rotation flow
                                    div {
                                        class: "seg",
                                        style: "width: fit-content; margin-bottom: 12px;",
                                        button {
                                            class: if *key_mode.read() == KeyMode::Generate { "active" } else { "" },
                                            onclick: move |_| key_mode.set(KeyMode::Generate),
                                            "Generate new keypair"
                                        }
                                        button {
                                            class: if *key_mode.read() == KeyMode::Paste { "active" } else { "" },
                                            onclick: move |_| {
                                                key_mode.set(KeyMode::Paste);
                                                generated_keys.set(None);
                                                new_pub_key.set(String::new());
                                            },
                                            "Paste existing public key"
                                        }
                                    }

                                    if *key_mode.read() == KeyMode::Generate {
                                        div {
                                            class: "field",
                                            if generated_keys.read().is_none() {
                                                div {
                                                    class: "help",
                                                    style: "margin-top: 0;",
                                                    "Generates a new Ed25519 keypair now. The private key is shown once for you to install on the host — Crystal Forge does not keep a copy."
                                                }
                                                button {
                                                    class: "btn btn-ghost focus-ring",
                                                    style: "margin-top: 8px;",
                                                    onclick: gen_keypair,
                                                    span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                        Icon { name: IconName::Key, size: 12 }
                                                    }
                                                    "Generate keypair"
                                                }
                                            } else {
                                                // Show generated keys
                                                if let Some(keys) = generated_keys.read().as_ref() {
                                                    div {
                                                        label {
                                                            "Public key "
                                                            span { style: "color: var(--cf-text-muted); font-weight: 400;", "· install on the host" }
                                                        }
                                                        div {
                                                            class: "mono",
                                                            style: "font-size: 11px; word-break: break-all; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 6px; margin-bottom: 10px;",
                                                            "{keys.pub_key}"
                                                        }

                                                        label {
                                                            "Private key "
                                                            span { style: "color: #f87171; font-weight: 600;", "· shown once, copy it now" }
                                                        }
                                                        div {
                                                            style: "position: relative;",
                                                            pre {
                                                                class: "mono",
                                                                style: "margin: 0; font-size: 10.5px; line-height: 1.5; white-space: pre-wrap; word-break: break-all; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 6px; border: 1px solid rgba(248,113,113,0.3);",
                                                                "{keys.priv_key}"
                                                            }
                                                            button {
                                                                class: "btn btn-ghost focus-ring xs",
                                                                style: "position: absolute; top: 6px; right: 6px;",
                                                                onclick: move |_| {
                                                                    // Mock clipboard copy
                                                                    priv_copied.set(true);
                                                                    spawn(async move {
                                                                        gloo_timers::future::TimeoutFuture::new(1600).await;
                                                                        priv_copied.set(false);
                                                                    });
                                                                },
                                                                span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                                    Icon {
                                                                        name: if *priv_copied.read() { IconName::Check } else { IconName::File },
                                                                        size: 11
                                                                    }
                                                                }
                                                                if *priv_copied.read() { "Copied" } else { "Copy" }
                                                            }
                                                        }

                                                        div {
                                                            class: "help",
                                                            style: "margin-top: 8px;",
                                                            "Write the private key to "
                                                            span { class: "mono", "/var/lib/crystal-forge/host.key" }
                                                            " on the host before confirming — once rotated, the old key stops being accepted on the agent's next heartbeat."
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        // Paste mode
                                        div {
                                            class: "field",
                                            label {
                                                "New agent public key "
                                                span { style: "color: #f87171;", "*" }
                                            }
                                            textarea {
                                                class: "input focus-ring mono",
                                                rows: "3",
                                                value: "{new_pub_key}",
                                                placeholder: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5… crystal-forge@hostname",
                                                style: "font-size: 11px; resize: vertical;",
                                                oninput: move |e| new_pub_key.set(e.value().clone()),
                                            }
                                            div {
                                                class: "help",
                                                "Generate a new keypair on the host and paste the public half here. The old key is revoked the moment you confirm — the agent must present the new key on its next heartbeat or it will be treated as unrecognized."
                                            }

                                            if !new_pub_key.read().trim().is_empty() {
                                                div {
                                                    style: format!(
                                                        "margin-top: 10px; padding: 9px 12px; border-radius: 8px; border: 1px solid {}; background: {};",
                                                        if new_key_valid { "rgba(52,211,153,0.3)" } else { "rgba(248,113,113,0.35)" },
                                                        if new_key_valid { "rgba(52,211,153,0.06)" } else { "rgba(248,113,113,0.06)" }
                                                    ),
                                                    if new_key_valid {
                                                        div {
                                                            style: "min-width: 0;",
                                                            div {
                                                                style: "font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--cf-text-muted); font-weight: 600;",
                                                                "New fingerprint"
                                                            }
                                                            div {
                                                                class: "mono",
                                                                style: "font-size: 11.5px; color: var(--cf-text-primary); word-break: break-all;",
                                                                "SHA256:ComputedFingerprintFromPastedKey123"
                                                            }
                                                        }
                                                    } else {
                                                        span {
                                                            style: "font-size: 11.5px; color: #fca5a5;",
                                                            "Doesn't look like an SSH public key — expected it to start with "
                                                            span { class: "mono", "ssh-ed25519" }
                                                            "."
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Action buttons for rotation flow
                                    div {
                                        style: "display: flex; gap: 8px; margin-top: 8px;",
                                        button {
                                            class: "btn btn-ghost focus-ring",
                                            onclick: move |_| {
                                                rotating_key.set(false);
                                                new_pub_key.set(String::new());
                                                generated_keys.set(None);
                                            },
                                            "Cancel"
                                        }
                                        button {
                                            class: "btn focus-ring",
                                            disabled: if *key_mode.read() == KeyMode::Generate {
                                                generated_keys.read().is_none()
                                            } else {
                                                !new_key_valid
                                            },
                                            style: format!(
                                                "background: {}; color: {};",
                                                if (if *key_mode.read() == KeyMode::Generate { generated_keys.read().is_some() } else { new_key_valid }) {
                                                    "#dc2626"
                                                } else {
                                                    "var(--cf-subtle-bg)"
                                                },
                                                if (if *key_mode.read() == KeyMode::Generate { generated_keys.read().is_some() } else { new_key_valid }) {
                                                    "white"
                                                } else {
                                                    "var(--cf-text-muted)"
                                                }
                                            ),
                                            onclick: move |_| {
                                                rotating_key.set(false);
                                                rotated.set(true);
                                            },
                                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                Icon { name: IconName::Key, size: 12 }
                                            }
                                            "Revoke old key & rotate"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // DANGER TAB
                    if *active_tab.read() == Tab::Danger {
                        div {
                            div {
                                style: "font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.08em; color: var(--cf-text-muted); margin-bottom: 8px;",
                                "Danger zone"
                            }
                            button {
                                class: "btn btn-ghost focus-ring",
                                style: "color: #f87171; border-color: rgba(248,113,113,0.3);",
                                onclick: move |_| show_remove_modal.set(true),
                                span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                    Icon { name: IconName::X, size: 12 }
                                }
                                "Remove system from registry"
                            }
                        }
                    }

                    if let Some(message) = &error_message {
                        div {
                            class: "sd-callout sd-callout-danger",
                            style: "margin-top: 10px;",
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
                            "Saving…"
                        } else {
                            "Save changes"
                        }
                    }
                }
            }
        }

        // Remove system confirmation modal
        if show_remove_modal() {
            {
                rsx! {
                    RemoveSystemDialog {
                        hostname: system.hostname.clone(),
                        environment: system.environment.clone(),
                        is_loading: remove_in_progress,
                        error_message: remove_error_message.clone(),
                        on_confirm: move |_| {
                            on_delete.call(());
                        },
                        on_cancel: move |_| {
                            show_remove_modal.set(false);
                        },
                    }
                }
            }
        }
    }
}
