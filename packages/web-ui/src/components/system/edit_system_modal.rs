//! Modal for editing system configuration.
//!
//! Tabbed modal with General, Deployment, Security, and Danger tabs.

use crate::api::models::{CommitInfo, FieldUpdate, SystemDetail, UpdateSystemRequest};
use crate::components::icon::{Icon, IconName};
use crate::components::modals::{GeneratedKeyPair, RemoveSystemDialog, generate_key_pair};
use crate::components::system::key_rotation;
use crate::systems::adapter::{
    SystemPublicKeyRotationOutcome, update_system_public_key_and_reconcile,
};
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Deployment,
    Security,
    Danger,
}

/// How the operator supplies the replacement agent public key.
#[derive(Clone, Copy, PartialEq)]
enum KeyMode {
    /// Generate a fresh Ed25519 keypair in the browser.
    Generate,
    /// Paste a public key generated on the host.
    Paste,
}

/// Copy the generated private key to the clipboard.
///
/// Best effort: the private key stays on screen either way, and no failure path
/// here may be reported to the operator as a successful copy.
async fn copy_to_clipboard(value: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::JsFuture;

        let window = web_sys::window().ok_or_else(|| {
            "Clipboard access is unavailable in this browser context.".to_string()
        })?;
        JsFuture::from(window.navigator().clipboard().write_text(value))
            .await
            .map_err(|_| {
                "Copy failed. Select and copy the private key manually before rotating.".to_string()
            })?;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = value;
        Err("Clipboard access is available only in a browser.".to_string())
    }
}

#[derive(Clone, PartialEq)]
struct RotationError {
    message: String,
    outcome_unknown: bool,
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
    on_key_rotated: EventHandler<SystemDetail>,
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

    // ── Agent identity / key rotation state (TASK-435) ──────────────────────
    // All of this is local to the open modal. The generated private key never
    // leaves this component and is dropped when the modal unmounts, so it is
    // unrecoverable once the operator closes the modal.
    let system_id = system.id;
    let mut current_fingerprint = use_signal(|| system.public_key_fingerprint.clone());
    let mut rotating_key = use_signal(|| false);
    let mut key_mode = use_signal(|| KeyMode::Generate);
    let mut generated_keys = use_signal(|| None::<GeneratedKeyPair>);
    let mut pasted_public_key = use_signal(String::new);
    let mut private_key_copied = use_signal(|| false);
    let mut copy_in_flight = use_signal(|| false);
    let mut copy_error = use_signal(|| None::<String>);
    let mut rotate_in_flight = use_signal(|| false);
    let mut rotate_error = use_signal(|| None::<RotationError>);
    let mut rotate_warning = use_signal(|| None::<String>);
    let mut rotated = use_signal(|| false);

    // Leaving the rotate flow discards local key material only; the stored key,
    // its fingerprint, and the audit log are untouched because no request was sent.
    let mut cancel_rotation = move || {
        rotating_key.set(false);
        key_mode.set(KeyMode::Generate);
        generated_keys.set(None);
        pasted_public_key.set(String::new());
        private_key_copied.set(false);
        copy_in_flight.set(false);
        copy_error.set(None);
        rotate_error.set(None);
        rotate_warning.set(None);
    };

    // The public key that would be submitted, if there is a valid one.
    let candidate_public_key: Option<String> = match *key_mode.read() {
        KeyMode::Generate => generated_keys
            .read()
            .as_ref()
            .map(|keys| keys.public_key.clone()),
        KeyMode::Paste => key_rotation::validate_public_key_input(&pasted_public_key.read()).ok(),
    };
    let candidate_fingerprint = candidate_public_key
        .as_deref()
        .and_then(key_rotation::public_key_fingerprint);

    let confirm_rotation = {
        let candidate = candidate_public_key.clone();
        move |_| {
            // Duplicate-submit guard: one click sequence must produce one request.
            if *rotate_in_flight.read() || *copy_in_flight.read() {
                return;
            }
            let Some(new_public_key) = candidate.clone() else {
                return;
            };

            rotate_in_flight.set(true);
            rotate_error.set(None);
            rotate_warning.set(None);

            spawn(async move {
                // Same endpoint the Systems-list "Update Key" action uses.
                // Only the public half is ever sent.
                let Some(expected_fingerprint) =
                    key_rotation::public_key_fingerprint(&new_public_key)
                else {
                    rotate_error.set(Some(RotationError {
                        message: "Public key is not a valid Ed25519 public key".to_string(),
                        outcome_unknown: false,
                    }));
                    rotate_in_flight.set(false);
                    return;
                };
                match update_system_public_key_and_reconcile(
                    system_id,
                    new_public_key,
                    expected_fingerprint,
                )
                .await
                {
                    Ok(outcome) => {
                        let (detail, warning) = match outcome {
                            SystemPublicKeyRotationOutcome::Confirmed(detail) => (detail, None),
                            SystemPublicKeyRotationOutcome::ConfirmedAfterAmbiguousResponse {
                                detail,
                                warning,
                            } => (detail, Some(warning)),
                        };
                        current_fingerprint.set(detail.public_key_fingerprint.clone());
                        rotated.set(true);
                        rotating_key.set(false);
                        // The private key is shown exactly once and is dropped here.
                        generated_keys.set(None);
                        pasted_public_key.set(String::new());
                        private_key_copied.set(false);
                        copy_error.set(None);
                        rotate_error.set(None);
                        rotate_warning.set(warning);
                        on_key_rotated.call(detail);
                    }
                    Err(error) => {
                        // Never present a failure as a rotation. The generated
                        // keypair stays on screen so the operator — who may
                        // already be installing that exact private key — can
                        // retry without regenerating.
                        rotate_error.set(Some(RotationError {
                            message: error.to_string(),
                            outcome_unknown: error.outcome_unknown(),
                        }));
                    }
                }
                rotate_in_flight.set(false);
            });
        }
    };

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

    let handle_save = move |_| {
        if rotate_in_flight() || copy_in_flight() {
            return;
        }
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
            "data-testid": "edit-system-modal-backdrop",
            onclick: move |_| {
                if !rotate_in_flight() && !copy_in_flight() {
                    on_close.call(());
                }
            },

            div {
                class: "modal",
                "data-testid": "edit-system-modal",
                "aria-busy": rotate_in_flight() || copy_in_flight(),
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
                            disabled: rotate_in_flight() || copy_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() && !copy_in_flight() {
                                    active_tab.set(Tab::General);
                                }
                            },
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Gear, size: 12 }
                            }
                            "General"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Deployment { "active" } else { "" },
                            disabled: rotate_in_flight() || copy_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() && !copy_in_flight() {
                                    active_tab.set(Tab::Deployment);
                                }
                            },
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Git, size: 12 }
                            }
                            "Deployment"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Security { "active" } else { "" },
                            disabled: rotate_in_flight() || copy_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() && !copy_in_flight() {
                                    active_tab.set(Tab::Security);
                                }
                            },
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Key, size: 12 }
                            }
                            "Security"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Danger { "active" } else { "" },
                            disabled: rotate_in_flight() || copy_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() && !copy_in_flight() {
                                    active_tab.set(Tab::Danger);
                                }
                            },
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
                                "data-testid": "agent-identity-section",
                                style: "margin-top: 8px; padding: 14px; border: 1px solid var(--cf-divider); border-radius: 10px; background: color-mix(in oklab, var(--cf-page-bg) 50%, var(--cf-card-bg));",
                                div {
                                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 13px; font-weight: 600;",
                                    Icon { name: IconName::Key, size: 13 }
                                    " Agent identity"
                                }

                                if !rotating_key() {
                                    div {
                                        class: "field",
                                        label { "Current public key fingerprint" }
                                        div {
                                            class: "mono",
                                            "data-testid": "agent-key-fingerprint",
                                            style: "font-size: 12px; word-break: break-all; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 6px;",
                                            {
                                                current_fingerprint
                                                    .read()
                                                    .clone()
                                                    .unwrap_or_else(|| "Unavailable".to_string())
                                            }
                                        }
                                        if current_fingerprint.read().is_none() {
                                            p {
                                                class: "help",
                                                "Crystal Forge could not read a valid Ed25519 public key for this system. Rotating installs a fresh key."
                                            }
                                        }
                                    }

                                    if rotated() {
                                        if let Some(warning) = rotate_warning.read().clone() {
                                            div {
                                                class: "sd-callout sd-callout-warn",
                                                "data-testid": "rotate-confirmed-warning-callout",
                                                style: "margin-top: 8px;",
                                                Icon { name: IconName::Warn, size: 13 }
                                                div { style: "font-size: 12px;",
                                                    "{warning}"
                                                }
                                            }
                                        } else {
                                            div {
                                                class: "sd-callout sd-callout-healthy",
                                                "data-testid": "rotate-success-callout",
                                                style: "margin-top: 8px;",
                                                Icon { name: IconName::Check, size: 13 }
                                                div { style: "font-size: 12px;",
                                                    "Key rotated. The old key is revoked immediately — the agent will authenticate with the new key on its next heartbeat."
                                                }
                                            }
                                        }
                                    } else {
                                        button {
                                            class: "btn btn-ghost focus-ring",
                                            "data-testid": "rotate-key-button",
                                            style: "margin-top: 4px;",
                                                onclick: move |_| {
                                                    rotate_error.set(None);
                                                    rotate_warning.set(None);
                                                    copy_error.set(None);
                                                    rotating_key.set(true);
                                                },
                                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                Icon { name: IconName::Sync, size: 12 }
                                            }
                                            "Rotate key"
                                        }
                                    }
                                } else {
                                    div {
                                        class: "seg",
                                        style: "width: fit-content; margin-bottom: 12px;",
                                        button {
                                            class: if *key_mode.read() == KeyMode::Generate { "active" } else { "" },
                                            "data-testid": "key-mode-generate",
                                            disabled: rotate_in_flight() || copy_in_flight(),
                                            onclick: move |_| key_mode.set(KeyMode::Generate),
                                            "Generate new keypair"
                                        }
                                        button {
                                            class: if *key_mode.read() == KeyMode::Paste { "active" } else { "" },
                                            "data-testid": "key-mode-paste",
                                            disabled: rotate_in_flight() || copy_in_flight(),
                                            onclick: move |_| {
                                                key_mode.set(KeyMode::Paste);
                                                // Switching away discards any generated
                                                // material so the private key is never
                                                // left dangling out of view.
                                                 generated_keys.set(None);
                                                 private_key_copied.set(false);
                                                 copy_error.set(None);
                                            },
                                            "Paste existing public key"
                                        }
                                    }

                                    if *key_mode.read() == KeyMode::Generate {
                                        div {
                                            class: "field",
                                            if let Some(keys) = generated_keys.read().clone() {
                                                label {
                                                    "Public key "
                                                    span { style: "color: var(--cf-text-muted); font-weight: 400;", "· registered with Crystal Forge" }
                                                }
                                                div {
                                                    class: "mono",
                                                    "data-testid": "generated-public-key",
                                                    style: "font-size: 11px; word-break: break-all; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 6px; margin-bottom: 10px;",
                                                    "{keys.public_key}"
                                                }
                                                label {
                                                    "Private key "
                                                    span { style: "color: #f87171; font-weight: 600;", "· shown once, copy it now" }
                                                }
                                                div {
                                                    style: "position: relative;",
                                                    pre {
                                                        class: "mono",
                                                        "data-testid": "generated-private-key",
                                                        style: "margin: 0; font-size: 10.5px; line-height: 1.5; white-space: pre-wrap; word-break: break-all; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 6px; border: 1px solid rgba(248,113,113,0.3);",
                                                        "{keys.private_key}"
                                                    }
                                                    button {
                                                        class: "btn btn-ghost focus-ring xs",
                                                        "data-testid": "copy-private-key-button",
                                                        style: "position: absolute; top: 6px; right: 6px;",
                                                        disabled: copy_in_flight() || rotate_in_flight(),
                                                        onclick: {
                                                            let private_key = keys.private_key.clone();
                                                            move |_| {
                                                                let private_key = private_key.clone();
                                                                private_key_copied.set(false);
                                                                copy_error.set(None);
                                                                copy_in_flight.set(true);
                                                                spawn(async move {
                                                                    match copy_to_clipboard(&private_key).await {
                                                                        Ok(()) => private_key_copied.set(true),
                                                                        Err(message) => copy_error.set(Some(message)),
                                                                    }
                                                                    copy_in_flight.set(false);
                                                                });
                                                            }
                                                        },
                                                        span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                            Icon {
                                                                name: if private_key_copied() { IconName::Check } else { IconName::File },
                                                                size: 11,
                                                            }
                                                        }
                                                        if copy_in_flight() {
                                                            "Copying…"
                                                        } else if private_key_copied() {
                                                            "Copied"
                                                        } else {
                                                            "Copy"
                                                        }
                                                    }
                                                }
                                                if let Some(message) = copy_error.read().clone() {
                                                    p {
                                                        "data-testid": "copy-private-key-error",
                                                        style: "margin-top: 8px; color: #fca5a5; font-size: 11.5px;",
                                                        "{message}"
                                                    }
                                                }
                                                p {
                                                    class: "help",
                                                    style: "margin-top: 8px;",
                                                    "Write the private key to "
                                                    span { class: "mono", "/var/lib/crystal-forge/host.key" }
                                                    " on the host before confirming. Crystal Forge never receives or stores it, and it cannot be shown again."
                                                }
                                            } else {
                                                p {
                                                    class: "help",
                                                    style: "margin-top: 0;",
                                                    "Generates a new Ed25519 keypair in your browser. The private key is shown once for you to install on the host — Crystal Forge does not keep a copy."
                                                }
                                                button {
                                                    class: "btn btn-ghost focus-ring",
                                                    "data-testid": "generate-keypair-button",
                                                    style: "margin-top: 8px;",
                                                    disabled: rotate_in_flight(),
                                                    onclick: move |_| {
                                                        private_key_copied.set(false);
                                                        copy_error.set(None);
                                                        rotate_error.set(None);
                                                        match generate_key_pair() {
                                                            Ok(keys) => generated_keys.set(Some(keys)),
                                                            Err(message) => rotate_error.set(Some(RotationError {
                                                                message,
                                                                outcome_unknown: false,
                                                            })),
                                                        }
                                                    },
                                                    span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                        Icon { name: IconName::Key, size: 12 }
                                                    }
                                                    "Generate keypair"
                                                }
                                            }
                                        }
                                    } else {
                                        div {
                                            class: "field",
                                            label {
                                                "New agent public key "
                                                span { style: "color: #f87171;", "*" }
                                            }
                                            textarea {
                                                class: "input focus-ring mono",
                                                "data-testid": "paste-public-key-input",
                                                rows: "3",
                                                style: "font-size: 11px; resize: vertical;",
                                                placeholder: "Base64 Ed25519 public key, e.g. 1Vw4kQ0PPk1zzO9Lp0kD2P7lqQ0N8f0O0m2VGmYb1yM=",
                                                value: "{pasted_public_key}",
                                                disabled: rotate_in_flight(),
                                                oninput: move |e| pasted_public_key.set(e.value().clone()),
                                            }
                                            p {
                                                class: "help",
                                                "Generate a keypair on the host and paste the base64 public half here. The old key is revoked the moment you confirm — the agent must present the new key on its next heartbeat or it will be rejected."
                                            }
                                            if !pasted_public_key.read().trim().is_empty() {
                                                if let Some(fingerprint) = candidate_fingerprint.clone() {
                                                    div {
                                                        "data-testid": "new-key-fingerprint",
                                                        style: "margin-top: 10px; padding: 9px 12px; border-radius: 8px; border: 1px solid rgba(52,211,153,0.3); background: rgba(52,211,153,0.06);",
                                                        div {
                                                            style: "font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--cf-text-muted); font-weight: 600;",
                                                            "New fingerprint"
                                                        }
                                                        div {
                                                            class: "mono",
                                                            style: "font-size: 11.5px; color: var(--cf-text-primary); word-break: break-all;",
                                                            "{fingerprint}"
                                                        }
                                                    }
                                                } else {
                                                    div {
                                                        "data-testid": "paste-key-invalid",
                                                        style: "margin-top: 10px; padding: 9px 12px; border-radius: 8px; border: 1px solid rgba(248,113,113,0.35); background: rgba(248,113,113,0.06);",
                                                        span {
                                                            style: "font-size: 11.5px; color: #fca5a5;",
                                                            {
                                                                key_rotation::validate_public_key_input(
                                                                        &pasted_public_key.read(),
                                                                    )
                                                                    .err()
                                                                    .unwrap_or_default()
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    div {
                                        class: "sd-callout sd-callout-warn",
                                        "data-testid": "rotate-warning-callout",
                                        style: "margin-top: 10px;",
                                        Icon { name: IconName::Warn, size: 13 }
                                        div { style: "font-size: 12px;",
                                            "Rotating revokes the current key immediately. Install the new private key on the host first — until you do, the agent's next heartbeat will be rejected."
                                        }
                                    }

                                    if let Some(error) = rotate_error.read().clone() {
                                        div {
                                            class: "sd-callout sd-callout-danger",
                                            "data-testid": "rotate-error-callout",
                                            "data-outcome": if error.outcome_unknown { "unknown" } else { "failed" },
                                            style: "margin-top: 8px;",
                                            div { style: "font-size: 12px;",
                                                if error.outcome_unknown {
                                                    strong { "Key rotation outcome unknown. " }
                                                } else {
                                                    strong { "Key rotation failed. " }
                                                }
                                                "{error.message}"
                                            }
                                        }
                                    }

                                    div {
                                        style: "display: flex; gap: 8px; margin-top: 10px;",
                                        button {
                                            class: "btn btn-ghost focus-ring",
                                            "data-testid": "rotate-cancel-button",
                                            disabled: rotate_in_flight() || copy_in_flight(),
                                            onclick: move |_| cancel_rotation(),
                                            "Cancel"
                                        }
                                        button {
                                            class: "btn focus-ring",
                                            "data-testid": "rotate-confirm-button",
                                            disabled: rotate_in_flight() || copy_in_flight() || candidate_public_key.is_none(),
                                            style: if candidate_public_key.is_some() && !rotate_in_flight() && !copy_in_flight() {
                                                "background: #dc2626; color: white;"
                                            } else {
                                                "background: var(--cf-subtle-bg); color: var(--cf-text-muted);"
                                            },
                                            onclick: confirm_rotation,
                                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                                Icon { name: IconName::Key, size: 12 }
                                            }
                                            if rotate_in_flight() {
                                                "Rotating…"
                                            } else {
                                                "Revoke old key & rotate"
                                            }
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
                        "data-testid": "edit-system-footer-cancel",
                        onclick: move |_| {
                            if !rotate_in_flight() && !copy_in_flight() {
                                on_close.call(());
                            }
                        },
                        disabled: is_saving() || rotate_in_flight() || copy_in_flight(),
                        "Cancel"
                    }

                    button {
                        class: "btn btn-primary focus-ring disabled:opacity-50 disabled:cursor-not-allowed",
                        "data-testid": "edit-system-save",
                        onclick: handle_save,
                        disabled: is_saving()
                            || rotate_in_flight()
                            || copy_in_flight()
                            || hostname.read().trim().is_empty(),

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
