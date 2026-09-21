//! Shared draft and dialog support for exact-CVE triage.
//!
//! Fleet and System Detail triage use the same validation, typed assignee, and
//! scheduled POA&M hydration rules. The server remains authoritative for the
//! environment and exact host scope.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::views::poam_api::{self, PoamApiError};

/// Selects the intended disposition for one environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnvironmentTriageChoice {
    /// Leaves exact findings outstanding.
    Open,
    /// Records accepted risk without remediation or verification semantics.
    Accepted,
    /// Schedules remediation through a POA&M.
    Scheduled,
}

impl EnvironmentTriageChoice {
    /// Returns the stable form value used by disposition controls.
    pub(crate) const fn value(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Scheduled => "scheduled",
        }
    }
}

/// Stores editable disposition fields for one server-provided environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EnvironmentTriageDraft {
    pub(crate) environment_id: Uuid,
    pub(crate) environment_name: String,
    pub(crate) choice: EnvironmentTriageChoice,
    pub(crate) justification: String,
    pub(crate) review_date: String,
}

/// Preserves the typed identity and display label hydrated from an existing POA&M.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HydratedAssigneeOption {
    /// Contains the immutable typed form value hydrated from the POA&M.
    pub(crate) value: String,
    /// Contains the display label that belongs to `value`.
    pub(crate) label: String,
}

/// Stores the shared fleet and System Detail exact-CVE triage form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CveTriageDraft {
    pub(crate) environments: Vec<EnvironmentTriageDraft>,
    pub(crate) title: String,
    pub(crate) target_date: String,
    pub(crate) plan: String,
    pub(crate) assignee: String,
    /// Preserves the original typed assignee independently of the selection.
    pub(crate) hydrated_assignee: Option<HydratedAssigneeOption>,
    pub(crate) risk: poam_api::PoamRisk,
    pub(crate) preservation_error: Option<String>,
    pub(crate) default_milestones: bool,
    /// Indicates that scheduled actions must reuse compatible POA&M metadata.
    pub(crate) existing_poam_reuse: bool,
}

impl CveTriageDraft {
    /// Hydrates a fleet draft without changing existing scheduled metadata.
    pub(crate) fn from_fleet_detail(detail: &poam_api::FleetCveDetail) -> Self {
        Self::from_environments(
            &detail.cve.cve_id,
            &detail.canonical_package_name,
            &detail.cve.severity,
            &detail.environments,
        )
    }

    /// Hydrates a single-environment System Detail draft.
    pub(crate) fn from_system_detail(
        detail: &poam_api::SystemCveTriageDetail,
        severity: &str,
        scope: poam_api::SystemCveTriageScopeChoice,
        fixed_version: Option<&str>,
        fix_available: bool,
    ) -> Self {
        let disposition = match scope {
            poam_api::SystemCveTriageScopeChoice::Host => &detail.host_disposition,
            poam_api::SystemCveTriageScopeChoice::Environment => &detail.environment_disposition,
        };
        let environment = poam_api::CveAffectedEnvironment {
            environment_id: detail.scope.environment_id,
            environment_name: match scope {
                poam_api::SystemCveTriageScopeChoice::Host => {
                    detail.scope.selected_system_hostname.clone()
                }
                poam_api::SystemCveTriageScopeChoice::Environment => {
                    detail.scope.environment_name.clone()
                }
            },
            affected_system_count: match scope {
                poam_api::SystemCveTriageScopeChoice::Host => 1,
                poam_api::SystemCveTriageScopeChoice::Environment => {
                    detail.scope.exact_affected_system_count
                }
            },
            exact_affected_system_count: match scope {
                poam_api::SystemCveTriageScopeChoice::Host => 1,
                poam_api::SystemCveTriageScopeChoice::Environment => {
                    detail.scope.exact_affected_system_count
                }
            },
            legacy_affected_system_count: 0,
            current_affected_system_count: Some(detail.scope.exact_affected_system_count),
            scheduled_deployment_target_count: Some(0),
            historical_inventory_system_count: Some(0),
            systems: detail.systems.clone(),
            disposition: disposition.clone(),
        };
        let mut draft = Self::from_environments(
            &detail.canonical_cve_id,
            &detail.canonical_package_name,
            severity,
            &[environment],
        );
        let host_needs_generated_poam = scope == poam_api::SystemCveTriageScopeChoice::Host
            && !matches!(
                disposition,
                Some(poam_api::CveEnvironmentDisposition::Scheduled { .. })
            );
        if host_needs_generated_poam {
            let fix_target = patch_target(fixed_version, fix_available);
            draft.title = format!(
                "{} - patch {} on {}",
                detail.canonical_cve_id,
                detail.canonical_package_name,
                detail.scope.selected_system_hostname
            );
            draft.plan = format!(
                "Upgrade {} to {} on {} and verify with an exact follow-up scan.",
                detail.canonical_package_name, fix_target, detail.scope.selected_system_hostname
            );
        }
        draft
    }

    fn from_environments(
        cve_id: &str,
        package: &str,
        severity: &str,
        environments: &[poam_api::CveAffectedEnvironment],
    ) -> Self {
        let environment_drafts = environments
            .iter()
            .filter(|environment| environment.exact_affected_system_count > 0)
            .map(|environment| {
                let (choice, justification, review_date) = match &environment.disposition {
                    Some(poam_api::CveEnvironmentDisposition::Accepted {
                        justification,
                        review_date,
                        ..
                    }) => (
                        EnvironmentTriageChoice::Accepted,
                        justification.clone(),
                        review_date.map(|date| date.to_string()).unwrap_or_default(),
                    ),
                    Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }) => (
                        EnvironmentTriageChoice::Scheduled,
                        String::new(),
                        String::new(),
                    ),
                    None => (EnvironmentTriageChoice::Open, String::new(), String::new()),
                };
                EnvironmentTriageDraft {
                    environment_id: environment.environment_id,
                    environment_name: environment.environment_name.clone(),
                    choice,
                    justification,
                    review_date,
                }
            })
            .collect();
        let mut draft = Self {
            environments: environment_drafts,
            title: format!("{cve_id} - patch {package}"),
            target_date: String::new(),
            plan: String::new(),
            assignee: String::new(),
            hydrated_assignee: None,
            risk: risk_for_severity(severity),
            preservation_error: None,
            default_milestones: true,
            existing_poam_reuse: false,
        };
        let scheduled = environments
            .iter()
            .filter_map(|environment| match &environment.disposition {
                Some(poam_api::CveEnvironmentDisposition::Scheduled { poam_id, poam, .. }) => {
                    Some((*poam_id, poam.as_ref()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if scheduled.is_empty() {
            return draft;
        }
        if scheduled.iter().any(|(_, poam)| poam.is_none()) {
            draft.preservation_error = Some(
                "Scheduled POA&M metadata is unavailable from this server version. Change all scheduled environments to OPEN or ACCEPTED, or retry after the server upgrade."
                    .to_string(),
            );
            return draft;
        }
        let first_id = scheduled[0].0;
        if scheduled.iter().any(|(poam_id, _)| *poam_id != first_id) {
            draft.preservation_error = Some(
                "Scheduled environments reference different POA&Ms. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                    .to_string(),
            );
            return draft;
        }
        let Some(metadata) = scheduled[0].1 else {
            return draft;
        };
        if metadata.id != first_id
            || scheduled
                .iter()
                .any(|(_, candidate)| candidate.is_some_and(|candidate| candidate != metadata))
        {
            draft.preservation_error = Some(
                "Scheduled environments have conflicting POA&M metadata. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                    .to_string(),
            );
            return draft;
        }
        if metadata.title.trim().is_empty() || metadata.plan.trim().is_empty() {
            draft.preservation_error = Some(
                "The scheduled POA&M metadata cannot satisfy compatible reuse. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                    .to_string(),
            );
            return draft;
        }
        draft.title = metadata.title.clone();
        draft.plan = metadata.plan.clone();
        draft.target_date = metadata.target_date.to_string();
        draft.risk = metadata.risk;
        match scheduled_assignee_selection(&metadata.assignee) {
            Ok((value, label)) => {
                draft.assignee = value.clone();
                draft.hydrated_assignee = Some(HydratedAssigneeOption { value, label });
                draft.existing_poam_reuse = true;
            }
            Err(message) => draft.preservation_error = Some(message),
        }
        draft
    }

    /// Replaces the disposition choice for one server-provided environment.
    pub(crate) fn set_choice(&mut self, environment_id: Uuid, choice: EnvironmentTriageChoice) {
        if let Some(environment) = self
            .environments
            .iter_mut()
            .find(|environment| environment.environment_id == environment_id)
        {
            environment.choice = choice;
        }
    }

    fn action_fields(
        environment: &EnvironmentTriageDraft,
    ) -> Result<(String, Option<chrono::NaiveDate>), String> {
        let justification = environment.justification.trim();
        if !(10..=2000).contains(&justification.len()) {
            return Err(format!(
                "Enter an acceptance justification of 10 to 2000 bytes for {}.",
                environment.environment_name
            ));
        }
        let review_date = if environment.review_date.trim().is_empty() {
            None
        } else {
            Some(
                chrono::NaiveDate::parse_from_str(environment.review_date.trim(), "%Y-%m-%d")
                    .map_err(|_| {
                        format!(
                            "Enter a valid review date for {}.",
                            environment.environment_name
                        )
                    })?,
            )
        };
        Ok((justification.to_string(), review_date))
    }

    fn poam_request(
        &self,
        scheduled: bool,
    ) -> Result<Option<poam_api::FleetCvePoamRequest>, String> {
        if !scheduled {
            return Ok(None);
        }
        if let Some(message) = &self.preservation_error {
            return Err(message.clone());
        }
        if self.title.trim().is_empty() {
            return Err("Enter a POA&M title for scheduled patching.".to_string());
        }
        if self.plan.trim().is_empty() {
            return Err("Enter a remediation plan for scheduled patching.".to_string());
        }
        let target_date = chrono::NaiveDate::parse_from_str(self.target_date.trim(), "%Y-%m-%d")
            .map_err(|_| "Enter a valid POA&M target date.".to_string())?;
        Ok(Some(poam_api::FleetCvePoamRequest {
            title: self.title.trim().to_string(),
            plan: self.plan.trim().to_string(),
            assignee: parse_assignee(&self.assignee)?,
            target_date,
            risk: self.risk,
            // Existing compatible POA&Ms are reused. Reuse must not request
            // creation of milestones on the existing record.
            default_milestones: !self.existing_poam_reuse && self.default_milestones,
        }))
    }

    /// Returns whether scheduled actions must reuse hydrated POA&M metadata.
    pub(crate) const fn reuses_existing_poam(&self) -> bool {
        self.existing_poam_reuse
    }

    /// Builds the fleet request without adding host identities.
    pub(crate) fn fleet_request(
        &self,
        package: &str,
    ) -> Result<poam_api::FleetCveTriageRequest, String> {
        let mut scheduled = false;
        let actions = self
            .environments
            .iter()
            .map(|environment| match environment.choice {
                EnvironmentTriageChoice::Open => {
                    Ok(poam_api::CveEnvironmentTriageAction::LeaveOpen {
                        environment_id: environment.environment_id,
                    })
                }
                EnvironmentTriageChoice::Accepted => {
                    let (justification, review_date) = Self::action_fields(environment)?;
                    Ok(poam_api::CveEnvironmentTriageAction::AcceptRisk {
                        environment_id: environment.environment_id,
                        justification,
                        review_date,
                    })
                }
                EnvironmentTriageChoice::Scheduled => {
                    scheduled = true;
                    Ok(poam_api::CveEnvironmentTriageAction::SchedulePatch {
                        environment_id: environment.environment_id,
                    })
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(poam_api::FleetCveTriageRequest {
            canonical_package_name: package.to_string(),
            actions,
            poam: self.poam_request(scheduled)?,
        })
    }

    /// Builds the system request without adding environment or host identities.
    pub(crate) fn system_request(
        &self,
        package: &str,
        scope: poam_api::SystemCveTriageScopeChoice,
    ) -> Result<poam_api::SystemCveTriageRequest, String> {
        let environment = self
            .environments
            .first()
            .ok_or_else(|| "No exact environment is available for triage.".to_string())?;
        let (action, scheduled) = match environment.choice {
            EnvironmentTriageChoice::Open => (poam_api::SystemCveTriageAction::LeaveOpen, false),
            EnvironmentTriageChoice::Accepted => {
                let (justification, review_date) = Self::action_fields(environment)?;
                (
                    poam_api::SystemCveTriageAction::AcceptRisk {
                        justification,
                        review_date,
                    },
                    false,
                )
            }
            EnvironmentTriageChoice::Scheduled => {
                (poam_api::SystemCveTriageAction::SchedulePatch, true)
            }
        };
        Ok(poam_api::SystemCveTriageRequest {
            canonical_package_name: package.to_string(),
            scope,
            action,
            poam: self.poam_request(scheduled)?,
        })
    }

    /// Returns whether the System Detail draft can produce a valid mutation.
    pub(crate) fn can_submit_system(
        &self,
        package: &str,
        scope: poam_api::SystemCveTriageScopeChoice,
    ) -> bool {
        self.system_request(package, scope).is_ok()
    }
}

fn scheduled_assignee_selection(
    assignee: &poam_api::PoamAssigneeView,
) -> Result<(String, String), String> {
    match assignee {
        poam_api::PoamAssigneeView::User {
            user_id,
            display,
            available: true,
        } => Ok((format!("user:{user_id}"), display.clone())),
        poam_api::PoamAssigneeView::OidcGroup {
            group_name,
            display,
            available: true,
        } => Ok((format!("group:{group_name}"), display.clone())),
        _ => Err(
            "The scheduled POA&M assignee is no longer available for compatible reuse. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                .to_string(),
        ),
    }
}

fn parse_assignee(value: &str) -> Result<poam_api::PoamAssigneeRequest, String> {
    if let Some(user_id) = value.strip_prefix("user:") {
        return Uuid::parse_str(user_id)
            .map(|user_id| poam_api::PoamAssigneeRequest::User { user_id })
            .map_err(|_| "Select a valid POA&M assignee.".to_string());
    }
    if let Some(group_name) = value.strip_prefix("group:")
        && !group_name.trim().is_empty()
        && group_name == group_name.trim()
    {
        return Ok(poam_api::PoamAssigneeRequest::OidcGroup {
            group_name: group_name.to_string(),
        });
    }
    Err("Select a valid POA&M assignee.".to_string())
}

/// Returns whether the catalog already renders one typed assignee value.
pub(crate) fn catalog_contains_assignee(
    catalog: &poam_api::PoamAssigneeCatalog,
    value: &str,
) -> bool {
    if let Some(user_id) = value.strip_prefix("user:") {
        return Uuid::parse_str(user_id).is_ok_and(|user_id| {
            catalog
                .people
                .iter()
                .any(|person| person.user_id == user_id)
        });
    }
    value.strip_prefix("group:").is_some_and(|group_name| {
        catalog
            .groups
            .iter()
            .any(|group| group.group_name == group_name)
    })
}

fn risk_for_severity(severity: &str) -> poam_api::PoamRisk {
    match severity.to_ascii_lowercase().as_str() {
        "critical" | "high" => poam_api::PoamRisk::High,
        "medium" => poam_api::PoamRisk::Medium,
        _ => poam_api::PoamRisk::Low,
    }
}

fn patch_target(fixed_version: Option<&str>, fix_available: bool) -> String {
    fixed_version
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if fix_available {
                "a patched release".to_string()
            } else {
                "a patched release once available".to_string()
            }
        })
}

/// Returns truthful fixed-version copy from exact and availability evidence.
pub(crate) fn fixed_version_label(fixed_version: Option<&str>, fix_available: bool) -> String {
    fixed_version
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if fix_available {
                "available — version pending".to_string()
            } else {
                "pending".to_string()
            }
        })
}

/// Renders the System Detail adapter for the shared exact-CVE triage draft.
#[component]
pub(crate) fn SystemCveTriageDialog(
    system_id: Uuid,
    detail: poam_api::SystemCveTriageDetail,
    severity: String,
    cvss_score: Option<f32>,
    fixed_version: Option<String>,
    fix_available: bool,
    on_close: EventHandler<()>,
    on_success: EventHandler<poam_api::SystemCveTriageResponse>,
    on_conflict: EventHandler<String>,
) -> Element {
    let mut scope = use_signal(|| poam_api::SystemCveTriageScopeChoice::Host);
    let initial_detail = detail.clone();
    let initial_severity = severity.clone();
    let initial_fixed_version = fixed_version.clone();
    let mut draft = use_signal(move || {
        CveTriageDraft::from_system_detail(
            &initial_detail,
            &initial_severity,
            poam_api::SystemCveTriageScopeChoice::Host,
            initial_fixed_version.as_deref(),
            fix_available,
        )
    });
    let mut catalog = use_signal(|| None::<Result<poam_api::PoamAssigneeCatalog, String>>);
    let mut error = use_signal(|| None::<String>);
    let mut pending = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            catalog.set(Some(
                poam_api::fetch_assignee_catalog()
                    .await
                    .map_err(|error| error.to_string()),
            ));
        });
    });
    let environment_id = detail.scope.environment_id;
    let current = draft.read().environments.first().cloned();
    let scheduled = current
        .as_ref()
        .is_some_and(|environment| environment.choice == EnvironmentTriageChoice::Scheduled);
    let existing_poam_reuse = scheduled && draft.read().reuses_existing_poam();
    let hydrated_assignee = draft.read().hydrated_assignee.clone();
    let hydrated_assignee_in_catalog = hydrated_assignee.as_ref().is_some_and(|assignee| {
        catalog
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .is_some_and(|catalog| catalog_contains_assignee(catalog, &assignee.value))
    });
    let cvss = cvss_score
        .map(|score| format!("{score:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let fix = fixed_version_label(fixed_version.as_deref(), fix_available);
    let host_scoped = scope() == poam_api::SystemCveTriageScopeChoice::Host;
    let inherited_help = host_scoped
        && detail.host_disposition.is_none()
        && detail.environment_disposition.is_some();
    let inherited_state = detail
        .environment_disposition
        .as_ref()
        .map(disposition_label)
        .unwrap_or("open");
    let direct_disposition_exists = match scope() {
        poam_api::SystemCveTriageScopeChoice::Host => detail.host_disposition.is_some(),
        poam_api::SystemCveTriageScopeChoice::Environment => {
            detail.environment_disposition.is_some()
        }
    };
    let choice = current.as_ref().map(|item| item.choice);
    let can_submit = draft
        .read()
        .can_submit_system(&detail.canonical_package_name, scope())
        && (choice != Some(EnvironmentTriageChoice::Open) || direct_disposition_exists);
    let dialog_label = format!(
        "Triage {} {}",
        detail.canonical_cve_id, detail.canonical_package_name
    );
    let submit_detail = detail.clone();
    let submit = move |_: MouseEvent| {
        let request = match draft
            .read()
            .system_request(&submit_detail.canonical_package_name, scope())
        {
            Ok(request) => request,
            Err(message) => {
                error.set(Some(message));
                return;
            }
        };
        pending.set(true);
        error.set(None);
        let cve_id = submit_detail.canonical_cve_id.clone();
        spawn(async move {
            match poam_api::triage_system_cve(system_id, &cve_id, &request).await {
                Ok(response) => {
                    pending.set(false);
                    on_success.call(response);
                }
                Err(PoamApiError::Server(server))
                    if server.status == 409 || server.status == 412 =>
                {
                    pending.set(false);
                    on_conflict.call(format!(
                        "{} Refresh the CVE inventory before retrying.",
                        server.message
                    ));
                }
                Err(request_error) => {
                    pending.set(false);
                    error.set(Some(format!("Triage was not applied: {request_error}")));
                }
            }
        });
    };

    rsx! {
        DialogFocusRestore {}
        DialogInitialFocus { dialog_id: "system-cve-triage-dialog" }
        button { class: "modal-backdrop cve-triage-backdrop", aria_label: "Close {dialog_label}", tabindex: "-1", onclick: move |_| if !pending() { on_close.call(()) } }
        div { id: "system-cve-triage-dialog", class: "modal cve-triage-modal", role: "dialog", aria_modal: "true", aria_label: "{dialog_label}", "data-testid": "system-cve-triage-dialog", tabindex: "-1", onkeydown: move |event| if event.key() == Key::Escape && !pending() { event.stop_propagation(); on_close.call(()); },
            DialogFocusSentinel { dialog_id: "system-cve-triage-dialog", boundary: DialogFocusBoundary::Last }
            div { class: "modal-head",
                div { h2 { "Triage {detail.canonical_cve_id}" } p { "Decide for this host alone, or for every host in its environment." } }
                button { class: "btn-icon focus-ring", aria_label: "Close triage editor", autofocus: true, disabled: pending(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
            }
            div { class: "modal-body cve-triage-body",
                p { "The server derives both scopes from current exact evidence. Accepted and scheduled states do not prove remediation or verification. Closure requires later exact evidence." }
                div { class: "cve-triage-context", "data-testid": "cve-triage-context",
                    header { Icon { name: IconName::Shield, size: 12 } " Vulnerability" span { "Scope is server-owned" } }
                    div { class: "cve-triage-context-grid",
                        div { span { "CVE" } strong { class: "mono", "{detail.canonical_cve_id}" } }
                        div { span { "Package" } strong { class: "mono", "{detail.canonical_package_name}" } }
                        div { span { "CVSS" } strong { "{cvss} · {severity}" } }
                        div { span { "Host" } strong { "{detail.scope.selected_system_hostname}" } }
                        div { span { "Environment" } strong { "{detail.scope.environment_name}" } }
                        div { span { "Affected hosts" } strong { if host_scoped { "1" } else { "{detail.scope.exact_affected_system_count}" } } }
                        div { span { "Fix" } strong { class: "mono", "{fix}" } }
                    }
                }
                if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                div { class: "field",
                    span { "Applies to" }
                    div { class: "seg", role: "radiogroup", aria_label: "Applies to",
                        button {
                            r#type: "button",
                            role: "radio",
                            aria_checked: host_scoped,
                            class: if host_scoped { "active" } else { "" },
                            "data-testid": "cve-triage-scope-host",
                            onclick: {
                                let detail = detail.clone();
                                let severity = severity.clone();
                                let fixed_version = fixed_version.clone();
                                move |_| {
                                    let next = poam_api::SystemCveTriageScopeChoice::Host;
                                    scope.set(next);
                                    draft.set(CveTriageDraft::from_system_detail(
                                        &detail,
                                        &severity,
                                        next,
                                        fixed_version.as_deref(),
                                        fix_available,
                                    ));
                                    error.set(None);
                                }
                            },
                            "{detail.scope.selected_system_hostname} only"
                        }
                        button {
                            r#type: "button",
                            role: "radio",
                            aria_checked: !host_scoped,
                            class: if !host_scoped { "active" } else { "" },
                            "data-testid": "cve-triage-scope-environment",
                            onclick: {
                                let detail = detail.clone();
                                let severity = severity.clone();
                                let fixed_version = fixed_version.clone();
                                move |_| {
                                    let next = poam_api::SystemCveTriageScopeChoice::Environment;
                                    scope.set(next);
                                    draft.set(CveTriageDraft::from_system_detail(
                                        &detail,
                                        &severity,
                                        next,
                                        fixed_version.as_deref(),
                                        fix_available,
                                    ));
                                    error.set(None);
                                }
                            },
                            "All of {detail.scope.environment_name}"
                        }
                    }
                    small {
                        if host_scoped {
                            "A host-specific decision overrides the environment default for this machine only."
                            if inherited_help {
                                " This host currently follows the {detail.scope.environment_name} decision ({inherited_state})."
                            } else if detail.host_disposition.is_some() && detail.environment_disposition.is_some() {
                                " Choosing OPEN removes this override; the host then follows the {detail.scope.environment_name} decision ({inherited_state})."
                            } else if detail.host_disposition.is_some() {
                                " Choosing OPEN removes this override and leaves the host outstanding."
                            }
                        } else {
                            "Covers every host in {detail.scope.environment_name}, including hosts added later. POA&M evidence attaches only the {detail.scope.exact_affected_system_count} host(s) where this CVE and package were observed exactly."
                        }
                    }
                }
                fieldset { class: "cve-triage-env", "data-testid": "cve-triage-environment",
                    legend { if host_scoped { "{detail.scope.selected_system_hostname} · host override" } else { "{detail.scope.environment_name} · {detail.scope.exact_affected_system_count} exact host(s)" } }
                    div { class: "seg", role: "group", aria_label: if host_scoped { "Disposition for {detail.scope.selected_system_hostname}" } else { "Disposition for {detail.scope.environment_name}" },
                        for (choice, label) in [(EnvironmentTriageChoice::Open, "Leave outstanding"), (EnvironmentTriageChoice::Accepted, "Accept risk"), (EnvironmentTriageChoice::Scheduled, "Schedule patch")] {
                            button { r#type: "button", class: if current.as_ref().map(|item| item.choice) == Some(choice) { "active" } else { "" }, aria_pressed: if current.as_ref().map(|item| item.choice) == Some(choice) { "true" } else { "false" }, "data-action": "{choice.value()}", onclick: move |_| draft.write().set_choice(environment_id, choice), if host_scoped && choice == EnvironmentTriageChoice::Open { "Use environment default" } else { "{label}" } }
                        }
                    }
                    if current.as_ref().map(|item| item.choice) == Some(EnvironmentTriageChoice::Accepted) {
                        label { class: "field", span { "Justification · required" } textarea { value: "{current.as_ref().map(|item| item.justification.as_str()).unwrap_or_default()}", "data-testid": "cve-accept-justification", oninput: move |event| if let Some(item) = draft.write().environments.first_mut() { item.justification = event.value(); } } }
                        label { class: "field", span { "Review date · optional" } input { r#type: "date", value: "{current.as_ref().map(|item| item.review_date.as_str()).unwrap_or_default()}", "data-testid": "cve-accept-review-date", oninput: move |event| if let Some(item) = draft.write().environments.first_mut() { item.review_date = event.value(); } } }
                    }
                }
                if scheduled {
                    fieldset { class: "cve-triage-poam", legend { if host_scoped { "POA&M for {detail.scope.selected_system_hostname}" } else { "POA&M for scheduled exact hosts" } }
                        if existing_poam_reuse {
                            div { class: "sd-callout sd-callout-info", "This schedule will reuse the existing compatible POA&M. Its metadata and milestones are not changed. Verification and closure require a later exact scan that no longer reports this CVE and package." }
                        } else {
                            div { class: "sd-callout sd-callout-info", "The POA&M records planned remediation. Verification and closure require a later exact scan that no longer reports this CVE and package." }
                        }
                        if let Some(message) = &draft.read().preservation_error { div { class: "sd-callout sd-callout-warn", role: "alert", "{message}" } }
                        label { class: "field", span { "Title" } input { value: "{draft.read().title}", "data-testid": "cve-poam-title", readonly: existing_poam_reuse, oninput: move |event| draft.write().title = event.value() } }
                        div { class: "cve-triage-poam-grid",
                            label { class: "field", span { "Target completion" } input { r#type: "date", value: "{draft.read().target_date}", "data-testid": "cve-poam-target", readonly: existing_poam_reuse, oninput: move |event| draft.write().target_date = event.value() } }
                            label { class: "field", span { "Risk" } select { value: "{risk_value(draft.read().risk)}", "data-testid": "cve-poam-risk", disabled: existing_poam_reuse, onchange: move |event| draft.write().risk = parse_risk(&event.value()), option { value: "high", "CAT I - High" } option { value: "medium", "CAT II - Medium" } option { value: "low", "CAT III - Low" } } }
                        }
                        label { class: "field", span { "Remediation plan" } textarea { value: "{draft.read().plan}", "data-testid": "cve-poam-plan", readonly: existing_poam_reuse, oninput: move |event| draft.write().plan = event.value() } }
                        label { class: "field", span { "Assignee · required" }
                            select { value: "{draft.read().assignee}", "data-testid": "cve-poam-assignee", disabled: existing_poam_reuse || catalog.read().is_none(), onchange: move |event| draft.write().assignee = event.value(),
                                option { value: "", disabled: true, "Select a user or group" }
                                if let Some(assignee) = hydrated_assignee.as_ref().filter(|_| !hydrated_assignee_in_catalog) { option { value: "{assignee.value}", "{assignee.label} (current)" } }
                                if let Some(Ok(catalog)) = &*catalog.read() {
                                    optgroup { label: "People", for person in &catalog.people { option { value: "user:{person.user_id}", "{person.label}" } } }
                                    optgroup { label: "Groups", for group in &catalog.groups { option { value: "group:{group.group_name}", "{group.group_name}" } } }
                                }
                            }
                            if let Some(Err(message)) = &*catalog.read() { small { role: "alert", "Assignees unavailable: {message}" } }
                        }
                        if existing_poam_reuse {
                            small { "Existing milestones remain unchanged." }
                        } else {
                            label { class: "poam-check", input { r#type: "checkbox", checked: draft.read().default_milestones, onchange: move |event| draft.write().default_milestones = event.checked() } span { "Add the default vulnerability remediation milestones" } }
                        }
                    }
                }
            }
            div { class: "modal-foot cve-triage-foot",
                div { class: "cve-triage-outcome", if host_scoped { "{detail.scope.selected_system_hostname} only" } else { "All of {detail.scope.environment_name} · {detail.scope.exact_affected_system_count} exact observed host(s)" } }
                button { class: "btn btn-ghost focus-ring", disabled: pending(), onclick: move |_| on_close.call(()), "Cancel" }
                button { class: "btn btn-primary focus-ring", "data-testid": "cve-triage-submit", disabled: pending() || !can_submit, onclick: submit, if pending() { "Applying..." } else { "Apply triage" } }
            }
            DialogFocusSentinel { dialog_id: "system-cve-triage-dialog", boundary: DialogFocusBoundary::First }
        }
    }
}

fn disposition_label(disposition: &poam_api::CveEnvironmentDisposition) -> &'static str {
    match disposition {
        poam_api::CveEnvironmentDisposition::Accepted { .. } => "accepted",
        poam_api::CveEnvironmentDisposition::Scheduled { .. } => "scheduled",
    }
}

/// Returns the stable form value for a POA&M risk.
pub(crate) const fn risk_value(risk: poam_api::PoamRisk) -> &'static str {
    match risk {
        poam_api::PoamRisk::High => "high",
        poam_api::PoamRisk::Medium => "medium",
        poam_api::PoamRisk::Low => "low",
    }
}

/// Parses a risk form value and fails closed to high risk.
pub(crate) fn parse_risk(value: &str) -> poam_api::PoamRisk {
    match value {
        "medium" => poam_api::PoamRisk::Medium,
        "low" => poam_api::PoamRisk::Low,
        _ => poam_api::PoamRisk::High,
    }
}

#[cfg(test)]
mod tests {
    use super::{CveTriageDraft, EnvironmentTriageChoice, fixed_version_label};
    use crate::views::poam_api::{
        self, PoamRisk, SystemCveTriageAction, SystemCveTriageScopeChoice,
    };

    fn detail(
        host_disposition: serde_json::Value,
        environment_disposition: serde_json::Value,
    ) -> poam_api::SystemCveTriageDetail {
        let (effective_disposition, effective_source) = if !host_disposition.is_null() {
            (host_disposition.clone(), "host")
        } else if !environment_disposition.is_null() {
            (environment_disposition.clone(), "environment")
        } else {
            (serde_json::Value::Null, "none")
        };
        serde_json::from_value(serde_json::json!({
            "canonical_cve_id": "CVE-2026-3262",
            "canonical_package_name": "openssl",
            "scope": {
                "kind": "current_exact_affected_hosts_in_environment",
                "selected_system_id": "00000000-0000-0000-0000-000000000001",
                "selected_system_hostname": "prod-web-01",
                "environment_id": "00000000-0000-0000-0000-000000000002",
                "environment_name": "Production",
                "exact_affected_system_count": 3
            },
            "systems": [],
            "host_disposition": host_disposition,
            "environment_disposition": environment_disposition,
            "effective_disposition": effective_disposition,
            "effective_source": effective_source,
            "disposition": effective_disposition
        }))
        .unwrap()
    }

    #[test]
    fn system_request_validates_acceptance_and_omits_scope_ids() {
        let mut draft = CveTriageDraft::from_system_detail(
            &detail(serde_json::Value::Null, serde_json::Value::Null),
            "high",
            SystemCveTriageScopeChoice::Host,
            None,
            false,
        );
        draft.environments[0].choice = EnvironmentTriageChoice::Accepted;
        assert!(
            draft
                .system_request("openssl", SystemCveTriageScopeChoice::Host)
                .unwrap_err()
                .contains("10 to 2000 bytes")
        );
        draft.environments[0].justification = "Compensating controls are active.".to_string();
        let request = draft
            .system_request("openssl", SystemCveTriageScopeChoice::Host)
            .unwrap();
        assert!(matches!(
            request.action,
            SystemCveTriageAction::AcceptRisk { .. }
        ));
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["canonical_package_name"], "openssl");
        assert_eq!(json["scope"], "host");
        assert_eq!(json["action"], "accept_risk");
        assert_eq!(json["justification"], "Compensating controls are active.");
        assert!(json["review_date"].is_null());
        assert!(json["poam"].is_null());
        assert!(json.get("environment_id").is_none());
        assert!(json.get("system_id").is_none());
        assert!(json.get("hostname").is_none());
    }

    #[test]
    fn system_scope_hydration_is_independent_and_host_does_not_copy_inheritance() {
        let accepted = serde_json::json!({
            "state": "accepted",
            "justification": "Environment compensating controls are active.",
            "review_date": "2026-12-01",
            "actor": { "user_id": "00000000-0000-0000-0000-000000000004", "display": "Operator" },
            "accepted_at": "2026-09-19T12:00:00Z"
        });
        let detail = detail(serde_json::Value::Null, accepted);
        let mut host = CveTriageDraft::from_system_detail(
            &detail,
            "high",
            SystemCveTriageScopeChoice::Host,
            None,
            true,
        );
        assert_eq!(host.environments[0].choice, EnvironmentTriageChoice::Open);
        assert!(host.environments[0].justification.is_empty());
        assert!(host.title.contains("prod-web-01"));
        assert!(host.plan.contains("a patched release"));
        assert!(host.plan.contains("prod-web-01"));

        host.environments[0].choice = EnvironmentTriageChoice::Scheduled;
        host.plan = "Unsaved host-only plan".to_string();
        let environment = CveTriageDraft::from_system_detail(
            &detail,
            "high",
            SystemCveTriageScopeChoice::Environment,
            None,
            true,
        );
        assert_eq!(
            environment.environments[0].choice,
            EnvironmentTriageChoice::Accepted
        );
        assert_eq!(
            environment.environments[0].justification,
            "Environment compensating controls are active."
        );
        assert_ne!(environment.plan, "Unsaved host-only plan");
    }

    #[test]
    fn fixed_version_copy_never_synthesizes_a_version() {
        assert_eq!(fixed_version_label(Some("  "), false), "pending");
        assert_eq!(
            fixed_version_label(Some(""), true),
            "available — version pending"
        );
        assert_eq!(fixed_version_label(Some("3.4.2"), true), "3.4.2");
    }

    #[test]
    fn system_scheduled_hydration_preserves_metadata_and_blocks_incompatible_reuse() {
        let scheduled = serde_json::json!({
            "state": "scheduled",
            "poam_id": "00000000-0000-0000-0000-000000000003",
            "poam": {
                "id": "00000000-0000-0000-0000-000000000003",
                "human_id": "POAM-3262",
                "title": "Patch OpenSSL",
                "plan": "Deploy the fixed package and verify exact evidence.",
                "target_date": "2026-10-20",
                "risk": "medium",
                "assignee": { "kind": "oidc_group", "group_name": "operators", "display": "operators", "available": true }
            },
            "actor": { "user_id": "00000000-0000-0000-0000-000000000004", "display": "Operator" },
            "scheduled_at": "2026-09-19T12:00:00Z"
        });
        let draft = CveTriageDraft::from_system_detail(
            &detail(scheduled, serde_json::Value::Null),
            "critical",
            SystemCveTriageScopeChoice::Host,
            Some("3.4.2"),
            true,
        );
        assert_eq!(draft.title, "Patch OpenSSL");
        assert_eq!(
            draft.plan,
            "Deploy the fixed package and verify exact evidence."
        );
        assert_eq!(draft.target_date, "2026-10-20");
        assert_eq!(draft.risk, PoamRisk::Medium);
        assert_eq!(draft.assignee, "group:operators");
        assert!(draft.reuses_existing_poam());
        assert_eq!(
            draft.hydrated_assignee.as_ref().unwrap().value,
            "group:operators"
        );
        let request = draft
            .system_request("openssl", SystemCveTriageScopeChoice::Host)
            .unwrap();
        assert!(!request.poam.unwrap().default_milestones);

        let incompatible = serde_json::json!({
            "state": "scheduled",
            "poam_id": "00000000-0000-0000-0000-000000000003",
            "actor": { "user_id": "00000000-0000-0000-0000-000000000004", "display": "Operator" },
            "scheduled_at": "2026-09-19T12:00:00Z"
        });
        let mut draft = CveTriageDraft::from_system_detail(
            &detail(incompatible, serde_json::Value::Null),
            "high",
            SystemCveTriageScopeChoice::Host,
            None,
            false,
        );
        assert!(!draft.reuses_existing_poam());
        assert!(
            draft
                .system_request("openssl", SystemCveTriageScopeChoice::Host)
                .unwrap_err()
                .contains("server version")
        );
        draft.environments[0].choice = EnvironmentTriageChoice::Open;
        assert!(
            draft
                .system_request("openssl", SystemCveTriageScopeChoice::Host)
                .unwrap()
                .poam
                .is_none()
        );
    }
}
