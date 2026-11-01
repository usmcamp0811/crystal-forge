//! Management-first registration and shared configuration enrollment fields.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::{client, models::*};
use crate::components::icon::{Icon, IconName};
use crate::components::system::{EvidenceFreshness, EvidencePipeline, MgmtChip, key_rotation};

/// Holds local registration input. Private enrollment material never enters this draft.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NewSystemDraft {
    /// Comma-separated operator labels, normalized when submitted.
    pub tags: String,
    /// Optional operator context retained in every mode.
    pub description: String,
    /// Optional managed address; omitted for configuration-only resources.
    pub fqdn: String,
    /// Existing managed direct/pull metadata; does not create a transport.
    pub reachability: String,
    /// Descriptive resource identity; only managed resources require a hostname.
    pub hostname: String,
    /// Real base64 Ed25519 public key, used only for managed enrollment.
    pub public_key: String,
    /// Authorized environment name.
    pub environment: String,
    /// Registered flake name.
    pub flake_name: String,
    /// Registered branch context, not an independent branch override.
    pub flake_branch: String,
    /// Exact declared NixOS output name.
    pub system_configuration_name: String,
    /// Managed deployment policy or [`ENVIRONMENT_DEFAULT_POLICY`], which the
    /// server copies from the environment when saved. Nonmanaged requests use
    /// manual without authority.
    pub deployment_policy: String,
    /// Explicit deployment authority.
    pub management_mode: ManagementMode,
    /// Optional reporting parent for an external resource.
    pub parent_host_id: Option<Uuid>,
}

impl NewSystemDraft {
    /// Creates an empty managed enrollment that inherits the environment default
    /// policy (copied at save time), matching the approved registration design.
    pub fn new() -> Self {
        Self {
            deployment_policy: ENVIRONMENT_DEFAULT_POLICY.into(),
            reachability: "direct".into(),
            ..Self::default()
        }
    }

    /// Produces the wire request, omitting credentials and parent outside their modes.
    pub fn request(&self) -> CreateSystemRequest {
        CreateSystemRequest {
            tags: parse_tags(&self.tags),
            description: optional_text(&self.description),
            fqdn: (self.management_mode == ManagementMode::Managed)
                .then(|| optional_text(&self.fqdn))
                .flatten(),
            reachability: if self.management_mode == ManagementMode::Managed {
                self.reachability.clone()
            } else {
                "direct".into()
            },
            management_mode: self.management_mode,
            parent_host_id: (self.management_mode == ManagementMode::External)
                .then_some(self.parent_host_id)
                .flatten(),
            hostname: self.hostname.trim().into(),
            system_configuration_name: Some(self.system_configuration_name.trim().into()),
            public_key: (self.management_mode == ManagementMode::Managed)
                .then(|| self.public_key.trim().into()),
            environment: Some(self.environment.trim().into()),
            flake_name: Some(self.flake_name.clone()),
            deployment_policy: if self.management_mode == ManagementMode::Managed {
                self.deployment_policy.clone()
            } else {
                "manual".into()
            },
        }
    }
}

/// Returns trimmed, distinct comma-separated labels in input order.
pub fn parse_tags(value: &str) -> Vec<String> {
    let mut tags = Vec::new();
    for tag in value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
    {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.to_string());
        }
    }
    tags
}

fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Edits real operator labels and suggests labels from the authorized fleet.
/// Suggestions load once per mounted form and never use fixture labels.
#[component]
pub fn EnrollmentTagsField(value: String, on_change: EventHandler<String>) -> Element {
    let fleet = use_resource(|| async {
        crate::systems::adapter::load_systems_with_fallback(&SystemsListParams::default()).await
    });
    let used = parse_tags(&value);
    let suggestions = fleet
        .read()
        .as_ref()
        .map(|result| {
            result
                .systems
                .iter()
                .flat_map(|system| system.tags.iter().cloned())
                .filter(|tag| !used.contains(tag))
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    rsx! {
        label { class: "field", "Tags · optional",
            input { class: "input focus-ring", value: "{value}", placeholder: "e.g. builder, stig-enforced",
                oninput: move |e| on_change.call(e.value()) }
        }
        div { class: "help", style: "display:flex;gap:6px;flex-wrap:wrap;",
            for tag in used.iter().cloned() {
                button { r#type: "button", class: "chip chip-unknown focus-ring", aria_label: "Remove tag {tag}",
                    onclick: { let used = used.clone(); let tag = tag.clone(); move |_| on_change.call(used.iter().filter(|v| **v != tag).cloned().collect::<Vec<_>>().join(", ")) },
                    "#{tag} ×"
                }
            }
        }
        if !suggestions.is_empty() {
            div { class: "help", style: "display:flex;gap:6px;flex-wrap:wrap;", "In use:",
                for tag in suggestions.into_iter().take(8) {
                    button { r#type: "button", class: "chip chip-unknown focus-ring",
                        onclick: { let used = used.clone(); let tag = tag.clone(); move |_| { let mut next = used.clone(); next.push(tag.clone()); on_change.call(next.join(", ")); } },
                        "#{tag}"
                    }
                }
            }
        }
        if let Some(result) = fleet.read().as_ref() {
            if let Some(notice) = &result.notice { p { class: "help", "Tag suggestions unavailable: {notice}. Labels can still be entered." } }
        }
    }
}

/// Edits managed address/reachability metadata without promising new connectivity.
#[component]
pub fn EnrollmentNetworkFields(
    fqdn: String,
    reachability: String,
    on_address: EventHandler<String>,
    on_reachability: EventHandler<String>,
) -> Element {
    rsx! {
        label { class: "field", "FQDN or address · optional",
            input { class: "input focus-ring mono", value: "{fqdn}", placeholder: "host.example.com or 10.0.4.12", oninput: move |e| on_address.call(e.value()) }
        }
        label { class: "field",
            input { r#type: "checkbox", checked: reachability == "direct", onchange: move |e| on_reachability.call(if e.checked() { "direct".into() } else { "pull".into() }) }
            "Reachable by the server directly"
        }
        p { class: "help",
            if reachability == "pull" { "Recorded for reference only. The agent connects outbound and receives authorized deployments on check-in." }
            else { "Records direct reachability. Agent reporting and deployment authorization still use the existing signed check-in transport." }
        }
    }
}

/// Parses Flake Explorer enrollment context without inventing source or identity.
/// Invalid modes or incomplete links do not open a registration dialog.
pub fn registration_prefill(query: &str) -> Option<NewSystemDraft> {
    #[derive(serde::Deserialize)]
    struct Prefill {
        add: String,
        hostname: String,
        configuration: Option<String>,
        flake_name: String,
        #[serde(default)]
        branch: String,
        #[serde(default)]
        management_mode: ManagementMode,
    }
    let value: Prefill = serde_urlencoded::from_str(query.trim_start_matches('?')).ok()?;
    if value.add != "1" || value.hostname.is_empty() || value.flake_name.is_empty() {
        return None;
    }
    Some(NewSystemDraft {
        system_configuration_name: value
            .configuration
            .unwrap_or_else(|| value.hostname.clone()),
        hostname: value.hostname,
        flake_name: value.flake_name,
        flake_branch: value.branch,
        management_mode: value.management_mode,
        ..NewSystemDraft::new()
    })
}

/// Validates required registration fields without inferring declared outputs.
///
/// # Errors
/// Returns a display-safe explanation for missing identity, scope, configuration,
/// credentials, duplicate identity, or unsupported deployment policy.
pub fn validate_new_system(
    draft: &NewSystemDraft,
    existing: &[SystemSummary],
    flake_names: &[String],
) -> Result<(), String> {
    if draft.hostname.trim().is_empty() {
        return Err("A resource name is required.".into());
    }
    if existing
        .iter()
        .any(|s| s.hostname.eq_ignore_ascii_case(draft.hostname.trim()))
    {
        return Err("This resource name is already registered. Choose a unique name; configurations may be shared.".into());
    }
    if draft.environment.trim().is_empty() {
        return Err("Select an environment.".into());
    }
    if !flake_names.contains(&draft.flake_name) {
        return Err("Select a registered flake.".into());
    }
    if draft.system_configuration_name.trim().is_empty() {
        return Err("Select an explicit NixOS configuration declared by the flake.".into());
    }
    if draft.management_mode == ManagementMode::Managed {
        key_rotation::validate_public_key_input(&draft.public_key)?;
        if !matches!(
            draft.deployment_policy.as_str(),
            "manual" | "auto_latest" | "pinned" | ENVIRONMENT_DEFAULT_POLICY
        ) {
            return Err("Select inherit, manual, auto_latest, or pinned deployment policy.".into());
        }
        if draft.deployment_policy == ENVIRONMENT_DEFAULT_POLICY
            && draft.environment.trim().is_empty()
        {
            return Err("Select an environment to inherit its default deployment policy.".into());
        }
    }
    Ok(())
}

/// Renders the three approved authority choices using real domain values.
#[component]
pub fn ManagementModePicker(
    value: ManagementMode,
    #[props(default)] current: Option<ManagementMode>,
    on_change: EventHandler<ManagementMode>,
) -> Element {
    rsx! {
        div { class: "mg-picker", role: "radiogroup", aria_label: "Management mode",
            for mode in [ManagementMode::Managed, ManagementMode::External, ManagementMode::Reference] {
                button {
                    r#type: "button", role: "radio", aria_checked: value == mode,
                    class: if value == mode { format!("mg-pick focus-ring mg-{} active", mode.as_str()) } else { format!("mg-pick focus-ring mg-{}", mode.as_str()) },
                    onclick: move |_| on_change.call(mode),
                    span { class: "mg-pick-head",
                        Icon { name: match mode { ManagementMode::Managed => IconName::Server, ManagementMode::External => IconName::Link, ManagementMode::Reference => IconName::File }, size: 14 }
                        b { "{mode.long_label()}" }
                        if current == Some(mode) { span { class: "chip chip-unknown", "current" } }
                    }
                    span { class: "mg-pick-desc", "{mode.description()}" }
                    span { class: "mg-pick-caps",
                        match mode {
                            ManagementMode::Managed => "Deploys · Agent · Scans",
                            ManagementMode::External => "Scans · No deploys · No agent",
                            ManagementMode::Reference => "Scans · No machine",
                        }
                    }
                }
            }
        }
    }
}

/// Reads every declared output from one immutable registry-head snapshot.
/// Continuation tokens prevent mixed revisions; unavailable discovery is an error.
async fn declared_configurations(
    name: String,
    branch: String,
) -> Result<(Vec<String>, String), String> {
    let flakes = client::fetch_flakes().await.map_err(|e| e.to_string())?;
    let flake = flakes
        .into_iter()
        .find(|f| f.name == name)
        .ok_or("Select a registered flake to load its declared outputs.")?;
    if !branch.is_empty() && branch != flake.branch {
        return Err(format!(
            "This flake tracks {}. Registration cannot override its branch; choose the registered branch.",
            flake.branch
        ));
    }
    let revision = flake
        .latest_commit_hash
        .ok_or("No synchronized revision is available. Sync and evaluate the flake first.")?;
    let mut offset = 0;
    let mut token = None;
    let mut configs = Vec::new();
    let mut declared_count = None;
    loop {
        let page = client::fetch_flake_revision_outputs(
            flake.id,
            &revision,
            FlakeSystemFilter::All,
            100,
            offset,
            token.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if page.declared_system_names.is_none() && page.lifecycle != SnapshotLifecycle::Available {
            return Err(page.error.unwrap_or_else(|| {
                format!(
                    "Declared outputs are {:?}. Sync and evaluate the flake, then retry.",
                    page.lifecycle
                )
            }));
        }
        if *declared_count.get_or_insert(page.declared_system_count) != page.declared_system_count {
            return Err("The declared-output inventory changed. Reload the flake.".into());
        }
        let names = page
            .declared_system_names
            .or_else(|| page.outputs.map(|o| o.declared_systems))
            .ok_or("Declared outputs are unavailable.")?;
        let count = names.len();
        configs.extend(names);
        if configs.len() as i64 >= page.declared_system_count {
            break;
        }
        if count == 0 {
            return Err("The declared-output response is incomplete. Reload the flake.".into());
        }
        offset += page.pagination.limit;
        // Match the server's continuation offset bound; do not repeat its last
        // clamped page indefinitely for an oversized declaration inventory.
        if offset > 100_000 {
            return Err("The declared-output inventory exceeds the browser page limit.".into());
        }
        token = page.snapshot_token;
    }
    configs.sort();
    configs.dedup();
    if Some(configs.len() as i64) != declared_count {
        return Err("The declared-output response is incomplete. Reload the flake.".into());
    }
    Ok((configs, flake.branch))
}

/// Renders exact-output autocomplete and reports whether selection is proven.
/// Branch follows the registered flake because create/edit APIs do not persist
/// per-system branch overrides. Failed discovery never validates free text.
#[component]
pub fn EnrollmentConfigurationFields(
    flake_name: String,
    branch: String,
    configuration: String,
    flake_names: Vec<String>,
    on_change: EventHandler<(String, String, String)>,
    on_valid: EventHandler<bool>,
) -> Element {
    let source = use_reactive((&flake_name, &branch), |(name, branch)| async move {
        let result = declared_configurations(name.clone(), branch.clone()).await;
        (name, branch, result)
    });
    let mut outputs = use_resource(source);
    // A retained response from the previous source cannot validate a new source.
    let loaded = outputs
        .read()
        .as_ref()
        .filter(|(name, requested_branch, _)| name == &flake_name && requested_branch == &branch)
        .map(|(_, _, result)| result.clone());
    let valid = loaded.as_ref().is_some_and(|r| {
        r.as_ref()
            .is_ok_and(|(names, _)| names.contains(&configuration))
    });
    use_effect(use_reactive(
        (&valid, &configuration, &flake_name, &branch),
        move |(valid, _, _, _)| on_valid.call(valid),
    ));
    let config_for_flake = configuration.clone();
    let flake_for_config = flake_name.clone();
    let branch_for_config = branch.clone();
    rsx! {
        div { class: "management-form-grid",
            label { class: "field", "Flake",
                select { class: "input focus-ring", value: "{flake_name}",
                    onchange: move |e| on_change.call((e.value(), String::new(), config_for_flake.clone())),
                    option { value: "", "Select flake" }
                    for name in flake_names { option { value: "{name}", "{name}" } }
                }
            }
            label { class: "field", "Branch",
                input { class: "input mono", readonly: true,
                    value: loaded.as_ref().and_then(|r| r.as_ref().ok()).map(|(_, b)| b.clone()).unwrap_or(branch.clone()),
                }
                span { class: "help", "Follows the registered flake branch." }
            }
        }
        label { class: "field", "NixOS configuration *",
            input { class: "input focus-ring mono", list: "enrollment-configurations", value: "{configuration}",
                oninput: move |e| { on_valid.call(false); on_change.call((flake_for_config.clone(), branch_for_config.clone(), e.value())); },
                placeholder: "Exact nixosConfigurations output name",
                aria_invalid: !configuration.is_empty() && !valid,
            }
        }
        match loaded.as_ref() {
            None => rsx! { p { class: "help", "Loading declared configurations…" } },
            Some(Err(message)) => rsx! {
                p { class: "sd-callout sd-callout-warn", role: "alert", "{message}" }
                button { class: "btn btn-ghost focus-ring", onclick: move |_| outputs.restart(), "Retry discovery" }
            },
            Some(Ok((names, _))) => rsx! {
                datalist { id: "enrollment-configurations", for name in names { option { value: "{name}" } } }
                if valid { p { class: "help", "Declared output: nixosConfigurations.{configuration}. Evidence describes this configuration, not a running machine." } }
                else { p { class: "help", role: "status", "Choose an exact declared configuration. Names are case-sensitive." } }
            },
        }
    }
}

async fn eligible_parents(
    environment: String,
    exclude: Option<Uuid>,
) -> Result<Vec<SystemDetail>, String> {
    if environment.is_empty() {
        return Ok(Vec::new());
    }
    let mut page = 1;
    let mut hosts = Vec::new();
    loop {
        let response = client::fetch_systems(&SystemsListParams {
            management_mode: Some(ManagementMode::Managed),
            environment: Some(environment.clone()),
            page: Some(page),
            per_page: Some(100),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
        let last = page >= response.total_pages();
        for item in response.items {
            if Some(item.id) == exclude {
                continue;
            }
            // List summaries omit is_active. Verify eligibility from authoritative detail.
            let detail = client::fetch_system(&item.id)
                .await
                .map_err(|e| e.to_string())?;
            if detail.is_active
                && detail.management_mode == ManagementMode::Managed
                && detail.health_status != crate::api::models::HealthStatus::Offline
                && detail.environment.as_deref() == Some(environment.as_str())
            {
                hosts.push(detail);
            }
        }
        if last {
            break;
        }
        page += 1;
    }
    Ok(hosts)
}

/// Offers active, nonoffline managed parents in the selected environment.
/// The server revalidates the association when the request commits.
#[component]
pub fn EnrollmentParentField(
    environment: String,
    value: Option<Uuid>,
    #[props(default)] exclude: Option<Uuid>,
    on_change: EventHandler<Option<Uuid>>,
) -> Element {
    let mut parents = use_resource(use_reactive(
        (&environment, &exclude),
        |(env, exclude)| async move { eligible_parents(env, exclude).await },
    ));
    rsx! {
        label { class: "field", "Parent host · optional",
            select { class: "input focus-ring mono", value: value.map(|v| v.to_string()).unwrap_or_default(),
                onchange: move |e| on_change.call(Uuid::parse_str(&e.value()).ok()),
                option { value: "", "None" }
                if let Some(Ok(hosts)) = parents.read().as_ref() {
                    if let Some(id) = value.filter(|id| !hosts.iter().any(|h| h.id == *id)) {
                        option { value: "{id}", disabled: true, "Current association unavailable — clear or select an eligible host" }
                    }
                    for host in hosts.iter().filter(|host| host.environment.as_deref() == Some(environment.as_str())) { option { value: "{host.id}", "{host.hostname}" } }
                }
            }
        }
        match parents.read().as_ref() {
            None => rsx! { p { class: "help", "Loading eligible parent hosts…" } },
            Some(Err(message)) => rsx! { p { role: "alert", "Could not load parents: {message}" } button { class: "btn btn-ghost", onclick: move |_| parents.restart(), "Retry parents" } },
            Some(Ok(hosts)) => rsx! { if hosts.is_empty() { p { class: "help", "No active managed hosts are available in this environment." } } },
        }
        p { class: "help", "A parent can report guest-unit observations. Crystal Forge never starts, stops or deploys the guest. No observation means runtime unknown." }
    }
}

/// Presents management-first registration sections with mode-specific obligations.
#[component]
pub fn AddSystemForm(
    mut draft: Signal<NewSystemDraft>,
    error: Signal<Option<String>>,
    on_submit: EventHandler<()>,
    on_cancel: EventHandler<()>,
    on_generate_keys: EventHandler<()>,
    environments: Vec<String>,
    flake_names: Vec<String>,
    show_onboarding_callouts: bool,
    #[props(default)] key_modal_open: bool,
    #[props(default)] submitting: bool,
    #[props(default = "Register system".into())] title: String,
    #[props(default = "Register system".into())] submit_label: String,
) -> Element {
    let mut section = use_signal(|| "management");
    let mut config_valid = use_signal(|| false);
    let form = draft();
    let mode = form.management_mode;
    let validation = validate_new_system(&form, &[], &flake_names);
    let key_fingerprint = key_rotation::public_key_fingerprint(&form.public_key);
    let _ = (show_onboarding_callouts, key_modal_open);
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| { if !submitting { on_cancel.call(()); } },
            div { class: "pe-shell management-form-shell", role: "dialog", aria_modal: "true", aria_label: "Register system",
                onclick: |e| e.stop_propagation(),
                onkeydown: move |e| { if e.key() == Key::Escape && !submitting { on_cancel.call(()); } },
                header { class: "pe-head",
                    div { style: "min-width:0;",
                        span { class: "pe-head-title",
                            if !form.hostname.is_empty() { "{form.hostname}" }
                            else if mode == ManagementMode::Reference { "Add reference configuration" }
                            else { "{title}" }
                        }
                        MgmtChip { mode }
                        if !form.environment.is_empty() { span { class: "chip chip-info", "{form.environment}" } }
                        if !form.flake_name.is_empty() { span { class: "chip chip-unknown mono", "{form.flake_name}", if !form.flake_branch.is_empty() { " · {form.flake_branch}" } } }
                        p { class: "pe-head-sub", "{mode.description()}" }
                    }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: submitting, onclick: move |_| on_cancel.call(()), "×" }
                }
                nav { class: "pe-rail", aria_label: "Registration sections",
                    for (id, label) in [("management", "Management"), ("identity", match mode { ManagementMode::Managed => "Host & environment", ManagementMode::External => "System & environment", ManagementMode::Reference => "Reference & environment" }), ("agent", "Agent identity"), ("configuration", "Configuration"), ("policy", if mode == ManagementMode::Managed { "Policy & tags" } else { "Tags" })] {
                        if id != "agent" || mode == ManagementMode::Managed {
                            button { class: if section() == id { "pe-rail-item active focus-ring" } else { "pe-rail-item focus-ring" }, onclick: move |_| section.set(id),
                                Icon { name: match id { "management" => IconName::Server, "identity" => IconName::Server, "agent" => IconName::Key, "configuration" => IconName::Git, _ => IconName::Shield }, size: 13 }
                                span { class: "pe-rail-label", "{label}" }
                                if id == "identity" && form.hostname.trim().is_empty() { span { class: "pe-rail-badge warn", "!" } }
                                if id == "agent" { span { class: "pe-rail-badge", if key_fingerprint.is_some() { "ok" } else { "!" } } }
                            }
                        }
                    }
                }
                div { class: "pe-body",
                    fieldset { disabled: submitting,
                        if section() == "management" {
                            div { class: "pe-sec-head", h3 { "Management mode" } p { "Who controls deployment, and what Crystal Forge does with this configuration." } }
                            ManagementModePicker { value: mode, on_change: move |mode| { draft.write().management_mode = mode; } }
                            div { class: "mg-notice", div { class: "mg-notice-body", b { "{mode.label()}" } p { "{mode.description()}" }
                                if mode != ManagementMode::Managed { p { "No agent credentials are needed. Configuration evidence never proves runtime state." } }
                            } }
                        }
                        if section() == "identity" {
                            div { class: "pe-sec-head", h3 { "Identity & environment" } p { "Choose a unique resource identity and its authorized environment." } }
                            div { class: "management-form-grid",
                                label { class: "field", match mode { ManagementMode::Managed => "Hostname *", ManagementMode::External => "System name *", ManagementMode::Reference => "Reference name *" },
                                    input { class: "input focus-ring", value: "{form.hostname}", oninput: move |e| draft.write().hostname = e.value() }
                                }
                                label { class: "field", "Environment *",
                                    select { class: "input focus-ring", value: "{form.environment}", onchange: move |e| { let mut d = draft.write(); d.environment = e.value(); d.parent_host_id = None; },
                                        option { value: "", "Select environment" }
                                        for env in environments { option { value: "{env}", "{env}" } }
                                    }
                                }
                            }
                            if mode == ManagementMode::Reference { p { class: "help", "Use a descriptive name. This resource is not a machine and does not require a runtime hostname." } }
                            if mode == ManagementMode::External { EnrollmentParentField { environment: form.environment.clone(), value: form.parent_host_id, on_change: move |id| draft.write().parent_host_id = id } }
                            if mode == ManagementMode::Managed { EnrollmentNetworkFields { fqdn: form.fqdn.clone(), reachability: form.reachability.clone(), on_address: move |value| draft.write().fqdn = value, on_reachability: move |value| draft.write().reachability = value } }
                        }
                        if section() == "agent" && mode == ManagementMode::Managed {
                            div { class: "pe-sec-head", h3 { "Agent identity" } p { "Register the real base64 Ed25519 public key. The private key stays on the host." } }
                            label { class: "field", "Agent public key *", textarea { class: "input focus-ring mono", rows: 4, value: "{form.public_key}", placeholder: "Base64 Ed25519 public key", oninput: move |e| draft.write().public_key = e.value() } }
                            button { class: "btn btn-ghost focus-ring", onclick: move |_| on_generate_keys.call(()), "Generate keypair" }
                            if let Some(fingerprint) = key_fingerprint { p { class: "help mono", "Fingerprint: {fingerprint}" } }
                            else if !form.public_key.is_empty() { p { role: "alert", "Enter a valid base64-encoded Ed25519 public key (32 bytes). SSH-formatted keys are not accepted." } }
                        }
                        // Keep discovery mounted across rail navigation; its result gates submit.
                        div { hidden: section() != "configuration",
                            div { class: "pe-sec-head", h3 { "Flake assignment" } p { "Select the exact declared output to evaluate, build and scan." } }
                            EnrollmentConfigurationFields {
                                flake_name: form.flake_name.clone(), branch: form.flake_branch.clone(), configuration: form.system_configuration_name.clone(), flake_names: flake_names.clone(),
                                on_change: move |(flake, branch, config)| { config_valid.set(false); let mut d = draft.write(); d.flake_name = flake; d.flake_branch = branch; d.system_configuration_name = config; },
                                on_valid: move |valid| config_valid.set(valid),
                            }
                        }
                        if section() == "policy" {
                            div { class: "pe-sec-head", h3 { "Policy & tags" } }
                            if mode == ManagementMode::Managed {
                                div { class: "field",
                                    label { "Deployment policy" }
                                    div { class: "seg", style: "width: fit-content; flex-wrap: wrap;", role: "radiogroup", aria_label: "Deployment policy",
                                        for (value, label) in [(ENVIRONMENT_DEFAULT_POLICY, "inherit"), ("manual", "manual"), ("auto_latest", "auto_latest"), ("pinned", "pinned")] {
                                            button { r#type: "button", role: "radio", aria_checked: form.deployment_policy == value,
                                                class: if form.deployment_policy == value { "active focus-ring" } else { "focus-ring" },
                                                onclick: move |_| draft.write().deployment_policy = value.to_string(),
                                                "{label}"
                                            }
                                        }
                                    }
                                    p { class: "help",
                                        match form.deployment_policy.as_str() {
                                            ENVIRONMENT_DEFAULT_POLICY => rsx! {
                                                "Use the default policy of the "
                                                span { class: "mono", if form.environment.is_empty() { "selected" } else { "{form.environment}" } }
                                                " environment. The current default is copied when you register; later changes to it do not update this system."
                                            },
                                            "manual" => rsx! { "Operator must explicitly approve every deploy." },
                                            "auto_latest" => rsx! { "Auto-deploy the newest passing commit on the assigned flake/branch." },
                                            _ => rsx! { "Stay on a chosen commit until manually changed (set the pin from System Detail after registering)." },
                                        }
                                    }
                                }
                            } else { div { class: "mg-notice", "No deployment policy applies. Crystal Forge retains configuration evidence without deployment authority." } }
                            EnrollmentTagsField { value: form.tags.clone(), on_change: move |value| draft.write().tags = value }
                            label { class: "field", "Description / notes", textarea { class: "input focus-ring", rows: 2, value: "{form.description}", placeholder: "Optional context for operators…", oninput: move |e| draft.write().description = e.value() } }
                        }
                    }
                    if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                }
                footer { class: "pe-foot",
                    span { class: "pe-foot-state", "{form.hostname} · {form.environment} · ",
                        if let Err(message) = validation.as_ref() { "{message}" } else if !config_valid() { "Declared configuration required" } else { "Ready to register" }
                    }
                    div { button { class: "btn btn-ghost focus-ring", disabled: submitting, onclick: move |_| on_cancel.call(()), "Cancel" }
                        button { class: "btn btn-primary focus-ring", disabled: submitting || validation.is_err() || !config_valid(), onclick: move |_| on_submit.call(()),
                            if submitting { "Registering…" } else if mode == ManagementMode::Reference { "Add reference configuration" } else { "{submit_label}" }
                        }
                    }
                }
            }
        }
    }
}

/// Shows confirmed registration and reloadable server evidence, never simulated progress.
#[component]
pub fn SystemRegistrationComplete(system: SystemDetail, on_close: EventHandler<()>) -> Element {
    let id = system.id;
    let mut evidence =
        use_resource(move || async move { client::fetch_configuration_assurance(&id).await });
    rsx! {
        div { class: "modal-backdrop", div { class: "modal management-complete", role: "dialog", aria_modal: "true", aria_label: "Registration complete",
            div { class: "modal-head", h2 { "{system.hostname} registered" } MgmtChip { mode: system.management_mode } }
            div { class: "modal-body",
                if system.management_mode == ManagementMode::Managed {
                    h3 { "Deploy the agent" }
                    p { "Install the agent module and its private key on the target host. Crystal Forge stores only the public key." }
                    pre { class: "mono", "services.crystal-forge.client = {{\n  enable = true;\n  # Configure this server's host and port.\n  private_key = \"/var/lib/crystal-forge/host.key\";\n}};" }
                    p { "Apply the host configuration with sudo nixos-rebuild switch. Wait for the first signed heartbeat; registration alone does not establish that the agent is online. For Setup, an administrator then acknowledges the agent report." }
                } else {
                    p { "Nothing will be deployed. These statuses come from retained configuration evidence." }
                    match evidence.read().as_ref() {
                        None => rsx! { p { "Loading evidence…" } },
                        Some(Err(e)) => rsx! { p { role: "alert", "Evidence unavailable: {e}. Registration succeeded; retry evidence loading." } },
                        Some(Ok(data)) => rsx! { EvidencePipeline { evidence: data.clone(), freshness: EvidenceFreshness::Unknown } },
                    }
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| evidence.restart(), "Reload evidence" }
                    if system.management_mode == ManagementMode::External { p { class: "help", "Runtime remains unknown until an associated parent reports an observation. A scan does not verify the running configuration." } }
                    else { p { class: "help", "This reference is not a running machine. Evidence is available from its Overview, CVEs, Hardening and Compliance tabs." } }
                }
            }
            div { class: "modal-foot", a { class: "btn btn-ghost focus-ring", href: "/systems/{id}", "Open resource" } button { class: "btn btn-primary focus-ring", onclick: move |_| on_close.call(()), "Done" } }
        } }
    }
}

#[cfg(test)]
mod management_registration_tests {
    use super::*;

    #[test]
    fn metadata_is_real_in_every_mode_and_network_fields_are_managed_only() {
        for mode in [
            ManagementMode::Managed,
            ManagementMode::External,
            ManagementMode::Reference,
        ] {
            let mut draft = reference();
            draft.management_mode = mode;
            draft.tags = " web, web, , Web, edge ".into();
            draft.description = " Operator notes \n".into();
            draft.fqdn = " 192.0.2.7 ".into();
            draft.reachability = "pull".into();
            let request = draft.request();
            assert_eq!(request.tags, ["web", "Web", "edge"]);
            assert_eq!(request.description.as_deref(), Some("Operator notes"));
            assert_eq!(
                request.fqdn.as_deref(),
                (mode == ManagementMode::Managed).then_some("192.0.2.7")
            );
            assert_eq!(
                request.reachability,
                if mode == ManagementMode::Managed {
                    "pull"
                } else {
                    "direct"
                }
            );
        }
        let mut draft = reference();
        draft.description = " \n".into();
        assert!(draft.request().description.is_none());
    }

    fn reference() -> NewSystemDraft {
        NewSystemDraft {
            management_mode: ManagementMode::Reference,
            hostname: "Golden workstation baseline".into(),
            environment: "lab".into(),
            flake_name: "platform".into(),
            system_configuration_name: "golden".into(),
            ..NewSystemDraft::new()
        }
    }

    #[test]
    fn reference_uses_descriptive_identity_and_no_agent_credentials() {
        let draft = reference();
        assert!(validate_new_system(&draft, &[], &["platform".into()]).is_ok());
        assert!(draft.request().public_key.is_none());
        assert_eq!(
            draft.request().system_configuration_name.as_deref(),
            Some("golden")
        );
    }

    #[test]
    fn flake_preselection_decodes_and_preserves_every_mode() {
        for (wire, mode) in [
            ("managed", ManagementMode::Managed),
            ("external", ManagementMode::External),
            ("reference", ManagementMode::Reference),
        ] {
            let draft = registration_prefill(&format!("?add=1&hostname=Golden%20workstation&configuration=base%2Fgolden&flake_name=platform%2Fcore&branch=release%201&management_mode={wire}")).unwrap();
            assert_eq!(draft.hostname, "Golden workstation");
            assert_eq!(draft.system_configuration_name, "base/golden");
            assert_eq!(draft.flake_name, "platform/core");
            assert_eq!(draft.flake_branch, "release 1");
            assert_eq!(draft.management_mode, mode);
            assert!(draft.public_key.is_empty());
        }
        assert!(
            registration_prefill(
                "add=1&hostname=golden&flake_name=platform&management_mode=invalid"
            )
            .is_none()
        );
        let legacy = registration_prefill("add=1&hostname=host&flake_name=platform").unwrap();
        assert_eq!(legacy.management_mode, ManagementMode::Managed);
        assert_eq!(legacy.system_configuration_name, "host");
    }

    #[test]
    fn keyless_modes_require_explicit_configuration_and_registered_flake() {
        for mode in [ManagementMode::External, ManagementMode::Reference] {
            let mut draft = reference();
            draft.management_mode = mode;
            draft.system_configuration_name.clear();
            assert!(validate_new_system(&draft, &[], &["platform".into()]).is_err());
            draft.system_configuration_name = "golden".into();
            assert!(validate_new_system(&draft, &[], &["Platform".into()]).is_err());
        }
    }

    #[test]
    fn managed_requires_real_ed25519_and_nonmanaged_omits_stale_key_parent() {
        let mut draft = reference();
        draft.management_mode = ManagementMode::Managed;
        draft.public_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into();
        assert!(validate_new_system(&draft, &[], &["platform".into()]).is_err());
        draft.management_mode = ManagementMode::Reference;
        draft.parent_host_id = Some(Uuid::new_v4());
        assert!(draft.request().public_key.is_none());
        assert!(draft.request().parent_host_id.is_none());
    }
}
