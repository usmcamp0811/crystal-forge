//! Add system form for registering new systems.

use dioxus::prelude::*;

use crate::theme;

/// Draft data for a new system being registered.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NewSystemDraft {
    /// System hostname
    pub hostname: String,
    /// Base64-encoded public key
    pub public_key: String,
    /// Environment name (e.g., "production", "staging")
    pub environment: String,
    /// Name of the flake this system belongs to
    pub flake_name: String,
    /// Deployment policy ("manual", "auto_latest", "pinned")
    pub deployment_policy: String,
}

impl NewSystemDraft {
    /// Create a new empty draft with default policy.
    pub fn new() -> Self {
        Self {
            hostname: String::new(),
            public_key: String::new(),
            environment: String::new(),
            flake_name: String::new(),
            deployment_policy: "manual".to_string(),
        }
    }
}

/// Form for registering a new system in the fleet.
///
/// Collects hostname, public key, environment, flake assignment, and deployment policy.
/// IP address is discovered from agent heartbeats, not entered manually.
#[component]
pub fn AddSystemForm(
    /// Current draft state
    draft: Signal<NewSystemDraft>,
    /// Validation error message (if any)
    error: Signal<Option<String>>,
    /// Called when the form is submitted
    on_submit: EventHandler<()>,
    /// Called when the user cancels
    on_cancel: EventHandler<()>,
    /// Called when user requests key generation
    on_generate_keys: EventHandler<()>,
    /// Available environment names
    environments: Vec<String>,
    /// Available flake names
    flake_names: Vec<String>,
    /// Whether onboarding field callouts should be shown
    show_onboarding_callouts: bool,
) -> Element {
    let mut show_hostname_callout = use_signal(|| show_onboarding_callouts);
    let mut show_public_key_callout = use_signal(|| show_onboarding_callouts);
    let mut show_environment_callout = use_signal(|| show_onboarding_callouts);
    let mut show_flake_callout = use_signal(|| show_onboarding_callouts);

    rsx! {
        crate::components::layout::Card {
            title: Some("Register System".to_string()),
            children: rsx! {
                div {
                    class: "space-y-4",
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "System IP is discovered from agent heartbeats and is not required at registration."
                    }
                    div {
                        class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                        // Hostname
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Hostname" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().hostname}",
                                placeholder: "atlas-09",
                                onfocus: move |_| show_hostname_callout.set(false),
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.hostname = evt.value();
                                    draft.set(next);
                                    show_hostname_callout.set(false);
                                }
                            }
                            if show_hostname_callout() {
                                div {
                                    "data-testid": "setup-coach-system-field-hostname",
                                    class: "rounded-md border border-blue-400/70 bg-blue-900/90 px-3 py-2 shadow-[0_6px_18px_rgba(30,64,175,0.35)]",
                                    p { class: "text-[11px] font-semibold uppercase tracking-wide text-blue-100", "Setup Coach" }
                                    p { class: "mt-1 text-xs text-blue-100", "Set the host name used by this machine in your infrastructure config (for example: web-01)." }
                                }
                            }
                        }
                        // Public Key
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Public Key" }
                            div {
                                class: "flex gap-2",
                                input {
                                    class: "w-full rounded-lg px-3 py-2 text-sm font-mono {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                    value: "{draft.read().public_key}",
                                    placeholder: "base64 public key",
                                    onfocus: move |_| show_public_key_callout.set(false),
                                    oninput: move |evt| {
                                        let mut next = draft.read().clone();
                                        next.public_key = evt.value();
                                        draft.set(next);
                                        show_public_key_callout.set(false);
                                    }
                                }
                                button {
                                    class: "px-3 py-2 rounded-lg text-xs font-medium border border-gray-600 text-gray-200 hover:bg-gray-700 transition",
                                    onclick: move |_| on_generate_keys.call(()),
                                    "Generate"
                                }
                            }
                            if show_public_key_callout() {
                                div {
                                    "data-testid": "setup-coach-system-field-public-key",
                                    class: "rounded-md border border-blue-400/70 bg-blue-900/90 px-3 py-2 shadow-[0_6px_18px_rgba(30,64,175,0.35)]",
                                    p { class: "text-[11px] font-semibold uppercase tracking-wide text-blue-100", "Setup Coach" }
                                    p { class: "mt-1 text-xs text-blue-100", "Paste the Crystal Forge agent public key for this host, or generate one and install its private key on the target machine." }
                                }
                            }
                        }
                        // Environment
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Environment" }
                            select {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().environment}",
                                onfocus: move |_| show_environment_callout.set(false),
                                onchange: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.environment = evt.value();
                                    draft.set(next);
                                    show_environment_callout.set(false);
                                },
                                option { value: "", "Select environment" }
                                for env in environments {
                                    option { value: "{env}", "{env}" }
                                }
                            }
                            if show_environment_callout() {
                                div {
                                    "data-testid": "setup-coach-system-field-environment",
                                    class: "rounded-md border border-blue-400/70 bg-blue-900/90 px-3 py-2 shadow-[0_6px_18px_rgba(30,64,175,0.35)]",
                                    p { class: "text-[11px] font-semibold uppercase tracking-wide text-blue-100", "Setup Coach" }
                                    p { class: "mt-1 text-xs text-blue-100", "Choose where this system belongs (for example staging or production) so policies and deployments target it correctly." }
                                }
                            }
                        }
                        // Flake Name
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Flake Name" }
                            select {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().flake_name}",
                                onfocus: move |_| show_flake_callout.set(false),
                                onchange: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.flake_name = evt.value();
                                    draft.set(next);
                                    show_flake_callout.set(false);
                                },
                                option { value: "", "Select flake" }
                                for flake_name in flake_names {
                                    option { value: "{flake_name}", "{flake_name}" }
                                }
                            }
                            if show_flake_callout() {
                                div {
                                    "data-testid": "setup-coach-system-field-flake",
                                    class: "rounded-md border border-blue-400/70 bg-blue-900/90 px-3 py-2 shadow-[0_6px_18px_rgba(30,64,175,0.35)]",
                                    p { class: "text-[11px] font-semibold uppercase tracking-wide text-blue-100", "Setup Coach" }
                                    p { class: "mt-1 text-xs text-blue-100", "Select the flake source this system should evaluate and deploy from." }
                                }
                            }
                        }
                        // Deployment Policy
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Deployment Policy" }
                            select {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().deployment_policy}",
                                onchange: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.deployment_policy = evt.value();
                                    draft.set(next);
                                },
                                option { value: "manual", "manual" }
                                option { value: "auto_latest", "auto_latest" }
                                option { value: "pinned", "pinned" }
                            }
                        }
                    }

                    if let Some(message) = error.read().clone() {
                        p { class: "text-sm text-red-300", "{message}" }
                    }

                    div {
                        class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                        button {
                            class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                            onclick: move |_| on_cancel.call(()),
                            "Cancel"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| on_submit.call(()),
                            "Save System"
                        }
                    }
                }
            }
        }
    }
}

/// Validate a new system draft before submission.
///
/// Returns Ok(()) if valid, or Err(message) if validation fails.
pub fn validate_new_system(
    draft: &NewSystemDraft,
    existing: &[crate::api::models::SystemSummary],
    flake_names: &[String],
) -> Result<(), String> {
    let hostname = draft.hostname.trim();
    if hostname.is_empty() {
        return Err("Hostname is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| item.hostname.eq_ignore_ascii_case(hostname))
    {
        return Err("Hostname already exists in this view.".to_string());
    }

    if draft.environment.trim().is_empty() {
        return Err("Environment is required.".to_string());
    }

    if draft.flake_name.trim().is_empty() {
        return Err("Flake name is required.".to_string());
    }
    if !flake_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(draft.flake_name.trim()))
    {
        return Err("Flake must be selected from registered flakes.".to_string());
    }

    let public_key = draft.public_key.trim();
    if public_key.is_empty() {
        return Err("Public key is required.".to_string());
    }
    if !looks_like_base64_key(public_key) {
        return Err("Public key must be a valid base64 string.".to_string());
    }

    let policy = draft.deployment_policy.trim();
    if !policy.is_empty() && !matches!(policy, "manual" | "auto_latest" | "pinned") {
        return Err("Deployment policy must be manual, auto_latest, or pinned.".to_string());
    }

    Ok(())
}

/// Check if a string looks like a valid base64-encoded key.
fn looks_like_base64_key(value: &str) -> bool {
    if value.len() < 40 {
        return false;
    }

    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=')
}
