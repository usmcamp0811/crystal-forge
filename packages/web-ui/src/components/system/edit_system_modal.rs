//! Modal for editing system configuration.
//!
//! Section rail with mode-specific configuration, real key rotation and removal.

use crate::api::models::{
    CommitInfo, ENVIRONMENT_DEFAULT_POLICY, FieldUpdate, ManagementMode, SystemDetail,
    UpdateSystemRequest,
};
use crate::components::forms::add_system_form::{
    EnrollmentConfigurationFields, EnrollmentNetworkFields, EnrollmentParentField,
    EnrollmentTagsField, ManagementModePicker, parse_tags,
};
use crate::components::icon::{Icon, IconName};
use crate::components::modals::{GeneratedKeyPair, RemoveSystemDialog, generate_key_pair};
use crate::components::system::MgmtChip;
use crate::components::system::key_rotation;
use crate::systems::adapter::{
    SystemPublicKeyRotationOutcome, update_system_public_key_and_reconcile,
};
use dioxus::prelude::*;

// Omitted mode preserves authority; parent updates preserve/clear/set explicitly.
fn parent_update(
    original: Option<uuid::Uuid>,
    selected: Option<uuid::Uuid>,
) -> FieldUpdate<uuid::Uuid> {
    if selected == original {
        FieldUpdate::Unset
    } else {
        selected.map(FieldUpdate::Set).unwrap_or(FieldUpdate::Clear)
    }
}

// Unchanged input must not become an explicit clear in a PATCH payload.
fn text_update(original: &str, value: &str) -> FieldUpdate<String> {
    if original == value.trim() {
        FieldUpdate::Unset
    } else if value.trim().is_empty() {
        FieldUpdate::Clear
    } else {
        FieldUpdate::Set(value.trim().into())
    }
}

/// Returns the pin change to send. Only an edit that stays managed with the
/// pinned policy can set a pin, and only a changed selection is sent. Other
/// policies omit it: the server clears a pin when the policy leaves `pinned`.
fn pin_update(
    stays_managed: bool,
    policy: &str,
    original: Option<&str>,
    selected: Option<&str>,
) -> FieldUpdate<String> {
    match selected {
        Some(sha) if stays_managed && policy == "pinned" && Some(sha) != original => {
            FieldUpdate::Set(sha.to_string())
        }
        _ => FieldUpdate::Unset,
    }
}

fn conversion_policy(original: ManagementMode, selected: ManagementMode, policy: &str) -> String {
    if selected != ManagementMode::Managed || original != ManagementMode::Managed {
        "manual".into()
    } else {
        policy.into()
    }
}

// COMPATIBILITY: The API resolves absent or blank configuration names against
// the incoming hostname. Compare effective names so renames cannot silently
// change a legacy hostname-based assignment without declaration proof.
fn effective_configuration<'a>(configuration: &'a str, hostname: &'a str) -> &'a str {
    let configuration = configuration.trim();
    if configuration.is_empty() {
        hostname.trim()
    } else {
        configuration
    }
}

// Retaining a persisted assignment does not assert that its output still exists.
// Only changed associations require current declaration proof. Nonmanaged
// requests still need a real registered flake and an explicit effective name.
fn assignment_allowed(
    original: (&str, &str),
    selected: (&str, &str),
    mode: ManagementMode,
    registered_flake: bool,
    declared: bool,
) -> bool {
    let configured = registered_flake && !selected.0.is_empty() && !selected.1.is_empty();
    if mode != ManagementMode::Managed && !configured {
        return false;
    }
    original == selected || (configured && declared)
}

#[cfg(test)]
mod management_registration_tests {
    use super::*;

    #[test]
    fn existing_assignment_survives_unavailable_or_removed_declarations() {
        for mode in [
            ManagementMode::Managed,
            ManagementMode::External,
            ManagementMode::Reference,
        ] {
            assert!(assignment_allowed(
                ("fleet", "removed"),
                ("fleet", "removed"),
                mode,
                true,
                false,
            ));
            assert!(!assignment_allowed(
                ("fleet", "removed"),
                ("fleet", "new"),
                mode,
                true,
                false,
            ));
            assert!(!assignment_allowed(
                ("fleet", "removed"),
                ("other", "removed"),
                mode,
                true,
                false,
            ));
            assert!(assignment_allowed(
                ("fleet", "removed"),
                ("other", "new"),
                mode,
                true,
                true,
            ));
            assert!(!assignment_allowed(
                ("fleet", "removed"),
                ("other", "new"),
                mode,
                false,
                true,
            ));
        }
    }

    #[test]
    fn legacy_no_flake_is_retained_only_for_managed_metadata() {
        assert!(assignment_allowed(
            ("", "host"),
            ("", "host"),
            ManagementMode::Managed,
            false,
            false,
        ));
        for mode in [ManagementMode::External, ManagementMode::Reference] {
            assert!(!assignment_allowed(
                ("", "host"),
                ("", "host"),
                mode,
                false,
                true
            ));
            assert!(!assignment_allowed(
                ("fleet", ""),
                ("fleet", ""),
                mode,
                true,
                true
            ));
        }
    }

    #[test]
    fn hostname_fallback_preserves_identity_and_requires_proof_for_a_new_output() {
        for original in ["", " \n", " old-host "] {
            let original = effective_configuration(original, "old-host");
            assert_eq!(original, "old-host");
            // A rename keeps the output when the editor retains its explicit name.
            assert!(assignment_allowed(
                ("fleet", original),
                ("fleet", effective_configuration("old-host", "new-host")),
                ManagementMode::Reference,
                true,
                false,
            ));
            // Clearing the name while renaming selects a different output.
            assert!(!assignment_allowed(
                ("fleet", original),
                ("fleet", effective_configuration("  ", " new-host ")),
                ManagementMode::Managed,
                true,
                false,
            ));
            assert!(assignment_allowed(
                ("fleet", original),
                ("fleet", effective_configuration("", "new-host")),
                ManagementMode::Managed,
                true,
                true,
            ));
        }
    }

    #[test]
    fn metadata_patch_distinguishes_unchanged_clear_and_set() {
        assert_eq!(text_update("notes", " notes "), FieldUpdate::Unset);
        assert_eq!(text_update("notes", " \n"), FieldUpdate::Clear);
        assert_eq!(
            text_update("", " new notes "),
            FieldUpdate::Set("new notes".into())
        );
        let request: UpdateSystemRequest = serde_json::from_value(
            serde_json::json!({"hostname":"host", "deployment_policy":"manual"}),
        )
        .unwrap();
        let wire = serde_json::to_value(request).unwrap();
        for field in ["tags", "description", "fqdn", "reachability"] {
            assert!(wire.get(field).is_none());
        }
        let request: UpdateSystemRequest = serde_json::from_value(serde_json::json!({"hostname":"host", "deployment_policy":"manual", "tags":[], "description":null, "fqdn":null, "reachability":"pull"})).unwrap();
        assert_eq!(request.tags, FieldUpdate::Set(Vec::new()));
        assert_eq!(request.description, FieldUpdate::Clear);
        assert_eq!(request.fqdn, FieldUpdate::Clear);
        assert_eq!(request.reachability, FieldUpdate::Set("pull".into()));
    }

    #[test]
    fn pin_selection_is_sent_only_for_changed_managed_pinned_edits() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        assert_eq!(
            pin_update(true, "pinned", Some(&a), Some(&b)),
            FieldUpdate::Set(b.clone())
        );
        assert_eq!(
            pin_update(true, "pinned", None, Some(&a)),
            FieldUpdate::Set(a.clone())
        );
        assert_eq!(
            pin_update(true, "pinned", Some(&a), Some(&a)),
            FieldUpdate::Unset
        );
        assert_eq!(
            pin_update(true, "manual", Some(&a), Some(&b)),
            FieldUpdate::Unset
        );
        assert_eq!(
            pin_update(true, ENVIRONMENT_DEFAULT_POLICY, None, Some(&a)),
            FieldUpdate::Unset
        );
        assert_eq!(
            pin_update(false, "pinned", None, Some(&a)),
            FieldUpdate::Unset
        );
        // Unset never serializes, so omission keeps the stored pin.
        let request: UpdateSystemRequest = serde_json::from_value(
            serde_json::json!({"hostname":"host", "deployment_policy":"pinned"}),
        )
        .unwrap();
        assert!(
            serde_json::to_value(&request)
                .unwrap()
                .get("pinned_commit")
                .is_none()
        );
    }

    #[test]
    fn conversion_into_managed_cannot_enable_automatic_deployment() {
        for original in [ManagementMode::External, ManagementMode::Reference] {
            for policy in ["manual", "auto_latest", "pinned"] {
                assert_eq!(
                    conversion_policy(original, ManagementMode::Managed, policy),
                    "manual"
                );
            }
        }
        assert_eq!(
            conversion_policy(
                ManagementMode::Managed,
                ManagementMode::Managed,
                "auto_latest"
            ),
            "auto_latest"
        );
        assert_eq!(
            conversion_policy(
                ManagementMode::Managed,
                ManagementMode::External,
                "auto_latest"
            ),
            "manual"
        );
    }

    #[test]
    fn parent_selection_preserves_clears_and_replaces_association() {
        let original = uuid::Uuid::from_u128(1);
        let replacement = uuid::Uuid::from_u128(2);
        assert_eq!(
            parent_update(Some(original), Some(original)),
            FieldUpdate::Unset
        );
        assert_eq!(parent_update(None, None), FieldUpdate::Unset);
        assert_eq!(parent_update(Some(original), None), FieldUpdate::Clear);
        assert_eq!(
            serde_json::to_value(parent_update(Some(original), None)).unwrap(),
            serde_json::Value::Null
        );
        assert_eq!(
            parent_update(Some(original), Some(replacement)),
            FieldUpdate::Set(replacement)
        );
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Deployment,
    Security,
    Management,
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

/// Edits registration locally until Save, preserving independent key rotation.
/// Mode conversion requires explicit confirmation and new credentials when managed.
/// Retains unchanged effective assignments when declaration discovery is unavailable;
/// changed assignments require proof for the exact selected flake and output.
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
    let mut fqdn = use_signal(|| system.fqdn.clone().unwrap_or_default());
    let original_fqdn = system.fqdn.clone().unwrap_or_default();
    let original_tags = system.tags.clone();
    let original_description = system.description.clone().unwrap_or_default();
    let original_reachability = system.network.reachability.clone();
    let mut reachability = use_signal(|| original_reachability.clone());
    let mut description = use_signal(|| original_description.clone());
    let mut system_configuration_name = use_signal(|| {
        effective_configuration(
            system
                .system_configuration_name
                .as_deref()
                .unwrap_or_default(),
            &system.hostname,
        )
        .to_string()
    });
    let original_mode = system.management_mode;
    let original_parent = system.parent_host_id;
    let mut management_mode = use_signal(|| original_mode);
    let mut parent_host_id = use_signal(|| original_parent);
    let mut enrollment_key = use_signal(String::new);
    let mut transition_confirmed = use_signal(|| false);
    // A bare boolean could validate a different assignment after a rapid edit.
    let mut declared_assignment = use_signal(|| None::<(String, String)>);
    let mut validation_error = use_signal(|| None::<String>);
    let mut deployment_policy = use_signal(|| system.deployment_policy.clone());
    let mut flake_name = use_signal(|| {
        system
            .flake
            .as_ref()
            .map(|flake| flake.name.clone())
            .unwrap_or_default()
    });
    let mut flake_branch = use_signal(String::new);
    let mut is_saving = use_signal(|| false);
    let mut show_remove_modal = use_signal(|| false);
    // Deployment targets have a separate API and remain owned by System Detail.
    // The pinned picker reuses the System Detail commit list. Selecting a pin
    // is metadata only; deployment stays a separate, authorized action.
    let original_pin = system.pinned_commit.clone();
    let mut pinned_commit = use_signal(|| original_pin.clone());

    // Seed heartbeat interval from the persisted system value.
    // 0 is a sentinel meaning "use server default" (NULL in the database).
    // When the system has no per-system override (heartbeat_interval_secs is None),
    // we show "Use server default" (0) so the user can see the current state.
    let mut heartbeat_interval_sec = use_signal(|| system.heartbeat_interval_secs.unwrap_or(0));
    let mut tags_draft = use_signal(|| system.tags.join(", "));

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
            if *rotate_in_flight.read() {
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

    {
        use_effect(use_reactive((&error_message,), move |(message,)| {
            if message.is_some() {
                is_saving.set(false);
            }
        }));
    }

    let original_assignment = (
        system
            .flake
            .as_ref()
            .map(|flake| flake.name.trim())
            .unwrap_or_default()
            .to_string(),
        effective_configuration(
            system
                .system_configuration_name
                .as_deref()
                .unwrap_or_default(),
            &system.hostname,
        )
        .to_string(),
    );
    let selected_assignment = (
        flake_name().trim().to_string(),
        effective_configuration(&system_configuration_name(), &hostname()).to_string(),
    );
    // The persisted detail proves its existing flake association even when the
    // registry dropdown cannot load. It proves no newly selected association.
    let registered_flake = !selected_assignment.0.is_empty()
        && (selected_assignment.0 == original_assignment.0
            || flake_names
                .iter()
                .any(|name| name == &selected_assignment.0));
    let declared = declared_assignment().as_ref() == Some(&selected_assignment);
    let can_keep_assignment = assignment_allowed(
        (&original_assignment.0, &original_assignment.1),
        (&selected_assignment.0, &selected_assignment.1),
        management_mode(),
        registered_flake,
        declared,
    );
    let discovery_assignment = selected_assignment.clone();
    let original_for_save = original_assignment.clone();
    let registered_flakes_for_save = flake_names.clone();

    let save_original_pin = original_pin.clone();
    let handle_save = move |_| {
        if rotate_in_flight() || is_saving() {
            return;
        }
        validation_error.set(None);
        // Re-read the current draft at the event boundary. Render-time validity
        // must never authorize a changed assignment before the next render.
        let selected = (
            flake_name().trim().to_string(),
            effective_configuration(&system_configuration_name(), &hostname()).to_string(),
        );
        let registered = !selected.0.is_empty()
            && (selected.0 == original_for_save.0
                || registered_flakes_for_save
                    .iter()
                    .any(|name| name == &selected.0));
        if !assignment_allowed(
            (&original_for_save.0, &original_for_save.1),
            (&selected.0, &selected.1),
            management_mode(),
            registered,
            declared_assignment().as_ref() == Some(&selected),
        ) {
            validation_error.set(Some(
                "Keep the existing assignment or select a registered flake and an exact declared configuration. External and reference resources require a flake and configuration.".into(),
            ));
            return;
        }
        if management_mode() != original_mode && !transition_confirmed() {
            validation_error.set(Some(
                "Confirm the management-mode change in Management before saving.".into(),
            ));
            return;
        }
        if original_mode != ManagementMode::Managed && management_mode() == ManagementMode::Managed
        {
            if let Err(message) = key_rotation::validate_public_key_input(&enrollment_key()) {
                validation_error.set(Some(message));
                return;
            }
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
            tags: if parse_tags(&tags_draft()) == original_tags {
                FieldUpdate::Unset
            } else {
                FieldUpdate::Set(parse_tags(&tags_draft()))
            },
            description: text_update(&original_description, &description()),
            reachability: if management_mode() == ManagementMode::Managed {
                text_update(&original_reachability, &reachability())
            } else {
                FieldUpdate::Unset
            },
            management_mode: (management_mode() != original_mode).then_some(management_mode()),
            public_key: (management_mode() == ManagementMode::Managed
                && original_mode != ManagementMode::Managed)
                .then(|| enrollment_key().trim().to_string()),
            parent_host_id: parent_update(
                original_parent,
                if management_mode() == ManagementMode::External {
                    parent_host_id()
                } else {
                    None
                },
            ),
            hostname: hostname.read().clone(),
            fqdn: if management_mode() == ManagementMode::Managed {
                text_update(&original_fqdn, &fqdn())
            } else {
                FieldUpdate::Unset
            },
            system_configuration_name: if selected.1.is_empty() {
                None
            } else {
                Some(selected.1)
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
            deployment_policy: conversion_policy(
                original_mode,
                management_mode(),
                &deployment_policy(),
            ),
            pinned_commit: pin_update(
                original_mode == ManagementMode::Managed
                    && management_mode() == ManagementMode::Managed,
                &deployment_policy(),
                save_original_pin.as_deref(),
                pinned_commit().as_deref(),
            ),
            heartbeat_interval_secs: if management_mode() == ManagementMode::Managed {
                heartbeat_interval
            } else {
                FieldUpdate::Unset
            },
        };

        on_save.call(request);
    };

    rsx! {
        div {
            class: "modal-backdrop",
            "data-testid": "edit-system-modal-backdrop",
            onclick: move |_| {
                if !rotate_in_flight() && !is_saving() {
                    on_close.call(());
                }
            },

            div {
                class: "pe-shell management-form-shell",
                "data-testid": "edit-system-modal",
                "aria-busy": rotate_in_flight(),
                onclick: move |e| e.stop_propagation(),

                div {
                    class: "pe-head",
                    div {
                    h2 {
                        span { style: "margin-right:6px; display:inline-flex; vertical-align:text-bottom;",
                            Icon { name: IconName::Gear, size: 14 }
                        }
                        "Edit {system.hostname}"
                        MgmtChip { mode: management_mode() }
                    }
                    p {
                        class: "pe-head-sub",
                        if management_mode() == ManagementMode::Managed { "System registration, flake assignment, deployment policy and agent identity." }
                        else { "Name, environment, configuration and management mode." }
                    }
                    }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: is_saving() || rotate_in_flight(), onclick: move |_| on_close.call(()), "×" }
                }

                // Conditional sections retain the existing key rotation and danger flows.
                div {
                    class: "pe-rail",
                    div {
                        class: "management-form-nav",
                        button {
                            class: if *active_tab.read() == Tab::General { "pe-rail-item active" } else { "pe-rail-item" },
                            disabled: rotate_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() {
                                    active_tab.set(Tab::General);
                                }
                            },
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Gear, size: 12 }
                            }
                            "General"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Deployment { "pe-rail-item active" } else { "pe-rail-item" },
                            disabled: rotate_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() {
                                    active_tab.set(Tab::Deployment);
                                }
                            },
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Git, size: 12 }
                            }
                            if management_mode() == ManagementMode::Managed { "Deployment" } else { "Configuration" }
                        }
                        if management_mode() == ManagementMode::Managed && original_mode == ManagementMode::Managed {
                        button {
                            class: if *active_tab.read() == Tab::Security { "pe-rail-item active" } else { "pe-rail-item" },
                            disabled: rotate_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() {
                                    active_tab.set(Tab::Security);
                                }
                            },
                            span { style: "margin-right: 4px; display:inline-flex; vertical-align:text-bottom;",
                                Icon { name: IconName::Key, size: 12 }
                            }
                            "Security"
                        }
                        }
                        button {
                            class: if active_tab() == Tab::Management { "pe-rail-item active" } else { "pe-rail-item" },
                            disabled: rotate_in_flight() || is_saving(),
                            onclick: move |_| active_tab.set(Tab::Management),
                            "Management"
                        }
                        button {
                            class: if *active_tab.read() == Tab::Danger { "pe-rail-item active" } else { "pe-rail-item" },
                            disabled: rotate_in_flight(),
                            onclick: move |_| {
                                if !rotate_in_flight() {
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
                    class: "pe-body",
                    style: "overflow-y:auto;",

                    // GENERAL TAB
                    fieldset { disabled: is_saving(),
                    if *active_tab.read() == Tab::General {
                        div {
                            // Two-column: Hostname + Environment (design layout)
                            div {
                                style: "display: grid; grid-template-columns: 1fr 1fr; gap: 14px;",
                                div {
                                    class: "field",
                                    label { if management_mode() == ManagementMode::Managed { "Hostname" } else { "Name" } }
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
                                            onchange: move |e| { environment.set(e.value()); parent_host_id.set(None); },
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

                            if management_mode() == ManagementMode::Managed {
                            EnrollmentNetworkFields { fqdn: fqdn(), reachability: reachability(), on_address: move |value| fqdn.set(value), on_reachability: move |value| reachability.set(value) }

                            }

                            EnrollmentTagsField { value: tags_draft(), on_change: move |value| tags_draft.set(value) }

                            // Notes remain local until the authorized Save succeeds.
                            div {
                                class: "field",
                                "data-testid": "description-field",
                                label { "Description / notes" }
                                textarea {
                                    class: "input focus-ring",
                                    rows: "2",
                                    aria_label: "Description / notes",
                                    value: "{description}",
                                    oninput: move |e| description.set(e.value()),
                                    placeholder: "Optional context for operators…",
                                    style: "resize: vertical;",
                                }
                                p { class: "help", "Optional context for operators." }
                            }
                        }
                    }

                    if active_tab() == Tab::Management {
                        div { class: "pe-sec-head", h3 { "Management mode" } p { "Who controls deployment. Configuration evidence and history are retained." } }
                        ManagementModePicker { value: management_mode(), current: original_mode,
                            on_change: move |mode| { management_mode.set(mode); transition_confirmed.set(false); validation_error.set(None); }
                        }
                        if management_mode() != original_mode {
                            div { class: "mg-notice mg-notice-warn",
                                div { class: "mg-notice-body",
                                    b { "Confirm management change: {original_mode.label()} → {management_mode().label()}" }
                                    if original_mode == ManagementMode::Managed {
                                        p { "Saving atomically revokes the agent key and deployment authority, invalidates pending deployments, and disables deploys and rollbacks. Evidence and history remain available." }
                                    } else if management_mode() == ManagementMode::Managed {
                                        p { "Supply a new real agent public key. Conversion saves MANUAL policy and does not deploy. Enable an automatic policy in a later edit after enrollment." }
                                    } else if management_mode() == ManagementMode::Reference {
                                        p { "The parent association is cleared. This resource becomes configuration-only; retained runtime observations do not establish current runtime state." }
                                    } else { p { "This becomes a system deployed outside Crystal Forge. Associate a reporting parent from Configuration if required." } }
                                }
                            }
                            if management_mode() == ManagementMode::Managed {
                                label { class: "field", "New agent public key *",
                                    textarea { class: "input focus-ring mono", aria_label: "New agent public key *", rows: 3, value: "{enrollment_key}", placeholder: "Base64 Ed25519 public key", oninput: move |e| enrollment_key.set(e.value()) }
                                }
                            }
                            label { class: "management-transition-confirm",
                                input { r#type: "checkbox", checked: transition_confirmed(), onchange: move |e| transition_confirmed.set(e.checked()) }
                                "I authorize this management-mode change when Save changes succeeds."
                            }
                        }
                    }

                    // Keep discovery mounted while navigating the section rail.
                    div { hidden: active_tab() != Tab::Deployment,
                        div {
                            EnrollmentConfigurationFields {
                                flake_name: selected_assignment.0.clone(), branch: flake_branch(), configuration: selected_assignment.1.clone(), flake_names: flake_names.clone(),
                                on_change: move |(flake, branch, config)| { declared_assignment.set(None); flake_name.set(flake); flake_branch.set(branch); system_configuration_name.set(config); },
                                on_valid: move |valid: bool| declared_assignment.set(valid.then(|| discovery_assignment.clone())),
                            }
                            if can_keep_assignment && !declared {
                                p { class: "help", role: "status", "Keeping the existing assignment; declaration discovery is unavailable or has not confirmed this output. This does not prove that the configuration is still declared." }
                            }
                            if management_mode() == ManagementMode::External {
                                EnrollmentParentField { environment: environment(), value: parent_host_id(), exclude: system_id, on_change: move |id| parent_host_id.set(id) }
                            }
                            if management_mode() == ManagementMode::Managed {
                            // Conversion policy is forced to manual in the request.
                            if original_mode != ManagementMode::Managed {
                                p { class: "mg-notice", "Initial managed policy: Manual. Automatic deployment can be enabled after conversion in a later edit." }
                            } else {
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
                                    button {
                                        class: if *deployment_policy.read() == ENVIRONMENT_DEFAULT_POLICY { "active" } else { "" },
                                        onclick: move |_| deployment_policy.set(ENVIRONMENT_DEFAULT_POLICY.to_string()),
                                        "Environment default"
                                    }
                                }
                                p {
                                    class: "help",
                                    match deployment_policy.read().as_str() {
                                        "manual" => "Operator must explicitly approve every deploy.".to_string(),
                                        "auto_latest" => "Automatically deploy the latest commit.".to_string(),
                                        "pinned" => "System will not auto-advance off the pinned commit. Saving a pin does not deploy it.".to_string(),
                                        ENVIRONMENT_DEFAULT_POLICY => format!(
                                            "Copies the {} environment's current default policy when you save. Later changes to that default do not update this system.",
                                            if environment().trim().is_empty() { "selected".to_string() } else { environment() }
                                        ),
                                        _ => String::new(),
                                    }
                                }
                            }
                            if deployment_policy() == "pinned" {
                                div { class: "field", "data-testid": "pinned-commit-field",
                                    label { "Pinned commit" }
                                    if recent_commits.is_empty() {
                                        p { class: "help", "No commits of the registered flake are available to pin." }
                                    } else {
                                        div { class: "sd-commit-list", style: "max-height: 200px;", role: "listbox", aria_label: "Pinned commit",
                                            for commit in recent_commits.clone() {
                                                {
                                                    let selected = pinned_commit().as_deref() == Some(commit.sha.as_str());
                                                    let sha = commit.sha.clone();
                                                    rsx! {
                                                        button {
                                                            r#type: "button",
                                                            role: "option",
                                                            aria_selected: selected,
                                                            class: if selected { "sd-commit-item focus-ring selected" } else { "sd-commit-item focus-ring" },
                                                            onclick: move |_| pinned_commit.set(Some(sha.clone())),
                                                            span { class: "mono sd-commit-sha", "{commit.short_sha}" }
                                                            span { class: "sd-commit-msg", "{commit.message}" }
                                                            span { class: "sd-commit-meta mono", "{commit.author}" }
                                                            span { class: "sd-commit-meta", title: "{commit.timestamp}",
                                                                {chrono::DateTime::parse_from_rfc3339(&commit.timestamp)
                                                                    .map(|at| crate::components::system::system_card_v2::ago(at.with_timezone(&chrono::Utc)))
                                                                    .unwrap_or_else(|_| commit.timestamp.clone())}
                                                            }
                                                            if original_pin.as_deref() == Some(commit.sha.as_str()) {
                                                                span { class: "chip chip-info", style: "font-size:10px", "pinned" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    p { class: "help", "System will not auto-advance off this commit. Operators can change the pin or temporarily deploy a different commit from System Detail. Saving the pin does not deploy it." }
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
                    }

                    // SECURITY TAB
                    if *active_tab.read() == Tab::Security && management_mode() == ManagementMode::Managed && original_mode == ManagementMode::Managed {
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
                                            disabled: rotate_in_flight(),
                                            onclick: move |_| key_mode.set(KeyMode::Generate),
                                            "Generate new keypair"
                                        }
                                        button {
                                            class: if *key_mode.read() == KeyMode::Paste { "active" } else { "" },
                                            "data-testid": "key-mode-paste",
                                            disabled: rotate_in_flight(),
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
                                            disabled: rotate_in_flight(),
                                            onclick: move |_| cancel_rotation(),
                                            "Cancel"
                                        }
                                        button {
                                            class: "btn focus-ring",
                                            "data-testid": "rotate-confirm-button",
                                            disabled: rotate_in_flight() || candidate_public_key.is_none(),
                                            style: if candidate_public_key.is_some() && !rotate_in_flight() {
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
                            p { class: "help",
                                if management_mode() == ManagementMode::Managed { "Unregistering stops deployments. The agent keeps running its current generation." }
                                else { "Removal stops configuration evaluation and scanning. It does not change an external system or the source configuration." }
                            }
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
                    if let Some(message) = validation_error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                    }
                }

                div {
                    class: "pe-foot",
                    span { class: "pe-foot-state",
                        "{flake_name} · {management_mode().label()}",
                        if !can_keep_assignment { " · Registered flake and declared configuration required for a new assignment" }
                        else if !declared { " · Keeping existing assignment; declaration unconfirmed" }
                        if management_mode() != original_mode { " · {original_mode.label()} → {management_mode().label()}" }
                    }
                    div {

                    button {
                        class: "btn btn-ghost focus-ring",
                        "data-testid": "edit-system-footer-cancel",
                        onclick: move |_| {
                            if !rotate_in_flight() {
                                on_close.call(());
                            }
                        },
                        disabled: is_saving() || rotate_in_flight(),
                        "Cancel"
                    }

                    button {
                        class: "btn btn-primary focus-ring disabled:opacity-50 disabled:cursor-not-allowed",
                        "data-testid": "edit-system-save",
                        onclick: handle_save,
                        disabled: is_saving()
                            || rotate_in_flight()
                            || !can_keep_assignment
                            || (management_mode() != original_mode && !transition_confirmed())
                            || (management_mode() == ManagementMode::Managed && original_mode != ManagementMode::Managed && key_rotation::validate_public_key_input(&enrollment_key()).is_err())
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
