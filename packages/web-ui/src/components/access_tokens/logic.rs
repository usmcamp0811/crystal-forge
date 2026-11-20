//! Pure rules of the access-token card: labels, summaries, form validation and
//! list merging.
//!
//! Nothing here touches Dioxus signals or the browser, so every rule has a unit
//! test. The server repeats every validation. The client rules only avoid
//! round trips for input that the contract rejects.

use std::collections::BTreeMap;

use chrono::{DateTime, Days, Utc};
use uuid::Uuid;

use crate::api::client::AccessTokenApiError;
use crate::api::models::{
    AccessTokenDto, AccessTokenMcpDto, AccessTokenPolicyDto, AccessTokenPolicyScopeDto,
    AccessTokenStatus, CreateAccessTokenRequest, Role,
};
use crate::components::icon::IconName;

/// Longest token name the server accepts, in characters.
pub(super) const NAME_MAX_CHARS: usize = 100;
/// Most environments the server accepts in one grant.
pub(super) const MAX_ENVIRONMENTS: usize = 50;
/// Tokens requested per list page. The server accepts 1 to 100.
pub(super) const LIST_PAGE_SIZE: u32 = 25;
/// Bullets that stand in for the secret part of a hidden token. The design
/// shows 28; the count does not reveal the real secret length.
const MASK_BULLETS: usize = 28;
/// Fallback dot color for an environment without a usable color.
const ENV_COLOR_FALLBACK: &str = "var(--cf-text-muted)";
/// Header example for the reveal view. Names an environment variable, never
/// the secret.
pub(super) const AUTH_HEADER_EXAMPLE: &str = "Authorization: Bearer $CF_MCP_TOKEN";
/// Reads a user's own sign-out and revocation behavior, shown under the list.
const FOOTER_NOTE: &str = "Tokens keep working after you sign out of the browser. They stop on revocation, expiry, account disable, or when your role or environment access is removed.";
/// Shown when a create request may have succeeded without the browser seeing
/// the secret.
pub(super) const CREATE_UNKNOWN_OUTCOME: &str = "The server response was not received, so the token may have been created. Close this dialog, check the access token list, and revoke any token you do not recognize. Crystal Forge cannot show a lost secret again.";
/// Shown when a rotate request may have succeeded without the browser seeing
/// the new secret.
pub(super) const ROTATE_UNKNOWN_OUTCOME: &str = "The server response was not received, so the token may have been replaced. Close this dialog, check the access token list, and revoke any token you do not recognize. Crystal Forge cannot show a lost secret again.";
/// Shown when a revoke request got no response. Revocation is idempotent, so
/// the user may repeat it.
pub(super) const REVOKE_UNKNOWN_OUTCOME: &str = "The server response was not received, so the token may or may not be revoked. Check the access token list. You can repeat the revocation safely.";

/// Status filter of the token list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StatusFilter {
    /// Every status.
    All,
    /// Tokens that can authenticate.
    Active,
    /// Tokens past their expiry time.
    Expired,
    /// Revoked tokens.
    Revoked,
}

impl StatusFilter {
    /// Every filter, in display order.
    pub(super) const ALL: [StatusFilter; 4] = [
        StatusFilter::All,
        StatusFilter::Active,
        StatusFilter::Expired,
        StatusFilter::Revoked,
    ];

    /// Returns the value of the `status` query parameter. `None` omits it.
    pub(super) fn param(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Active => Some("active"),
            Self::Expired => Some("expired"),
            Self::Revoked => Some("revoked"),
        }
    }

    /// Returns the option label.
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Active => "Active",
            Self::Expired => "Expired",
            Self::Revoked => "Revoked",
        }
    }

    /// Returns the `<option>` value.
    pub(super) fn value(self) -> &'static str {
        self.param().unwrap_or("all")
    }

    /// Parses an `<option>` value. An unknown value selects [`Self::All`].
    pub(super) fn from_value(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|filter| filter.value() == value)
            .unwrap_or(Self::All)
    }
}

/// Text, color class, and glyph of a status chip.
///
/// The label and the glyph carry the meaning. Color only reinforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StatusPresentation {
    /// Visible status text.
    pub(super) label: &'static str,
    /// Chip color class from the design system.
    pub(super) chip_class: &'static str,
    /// Glyph shown before the label.
    pub(super) icon: IconName,
}

/// Returns how a status is presented.
pub(super) fn status_presentation(status: AccessTokenStatus) -> StatusPresentation {
    let (label, chip_class, icon) = match status {
        AccessTokenStatus::Active => ("Active", "chip-healthy", IconName::Check),
        AccessTokenStatus::ExpiringSoon => ("Expiring soon", "chip-warning", IconName::Clock),
        AccessTokenStatus::Expired => ("Expired", "chip-unknown", IconName::Clock),
        AccessTokenStatus::Revoked => ("Revoked", "chip-critical", IconName::X),
        AccessTokenStatus::OwnerSuspended => ("Access suspended", "chip-warning", IconName::Warn),
        AccessTokenStatus::Unknown => ("Unknown", "chip-unknown", IconName::Info),
    };
    StatusPresentation {
        label,
        chip_class,
        icon,
    }
}

/// Returns whether a token is live: not revoked and not expired.
///
/// The approved design offers Rotate and Revoke for every live token,
/// including one whose owner access is suspended, so the owner can still
/// revoke it. Every other row offers Details only. The server decides whether
/// a rotation of a suspended token is permitted.
pub(super) fn is_live(status: AccessTokenStatus) -> bool {
    matches!(
        status,
        AccessTokenStatus::Active
            | AccessTokenStatus::ExpiringSoon
            | AccessTokenStatus::OwnerSuspended
    )
}

fn plural(count: i64, unit: &str) -> String {
    if count == 1 {
        format!("{count} {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

/// Formats a timestamp as a UTC calendar date, for example `2026-12-02`.
pub(super) fn format_date(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d").to_string()
}

/// Formats a timestamp as a short UTC date, for example `Nov 16`.
///
/// The year is omitted, as in the approved design. Callers keep the full date
/// (see [`format_date`]) in a `title` attribute.
pub(super) fn format_short_date(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%b %-d").to_string()
}

/// Returns the short date on which a token created at `now` with a lifetime of
/// `days` expires, for example `Dec 16`.
///
/// Pure calendar arithmetic on a UTC timestamp, so it works in the browser
/// build. A lifetime that overflows the calendar returns `-`.
pub(super) fn expiry_date(now: DateTime<Utc>, days: u32) -> String {
    now.checked_add_days(Days::new(u64::from(days)))
        .map_or_else(|| "-".to_string(), format_short_date)
}

/// Describes a past instant relative to `now`. A future instant reads as now.
pub(super) fn relative_past(timestamp: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = now.signed_duration_since(timestamp);
    if delta.num_minutes() < 1 {
        "just now".to_string()
    } else if delta.num_hours() < 1 {
        format!("{} ago", plural(delta.num_minutes(), "minute"))
    } else if delta.num_days() < 1 {
        format!("{} ago", plural(delta.num_hours(), "hour"))
    } else {
        format!("{} ago", plural(delta.num_days(), "day"))
    }
}

/// Describes an expiry time relative to `now`, for example `in 29 days`.
pub(super) fn relative_expiry(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let remaining = expires_at.signed_duration_since(now);
    if remaining.num_seconds() <= 0 {
        format!("expired {}", relative_past(expires_at, now))
    } else if remaining.num_days() >= 1 {
        format!("in {}", plural(remaining.num_days(), "day"))
    } else if remaining.num_hours() >= 1 {
        format!("in {}", plural(remaining.num_hours(), "hour"))
    } else if remaining.num_minutes() >= 1 {
        format!("in {}", plural(remaining.num_minutes(), "minute"))
    } else {
        "in less than a minute".to_string()
    }
}

/// Describes the last use of a token as a short date. `None` means the token
/// was never used and reads `never`.
pub(super) fn last_used_short(last_used_at: Option<DateTime<Utc>>) -> String {
    last_used_at.map_or_else(|| "never".to_string(), format_short_date)
}

/// Returns the `title` text of a last-used cell: the full date and the age.
pub(super) fn last_used_title(last_used_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    last_used_at.map_or_else(
        || "This token has not been used".to_string(),
        |at| format!("{} ({})", format_date(at), relative_past(at, now)),
    )
}

/// Returns a CSS color for an environment dot.
///
/// The server sends `#RRGGBB` or nothing. The value goes into a `style`
/// attribute, so anything that is not a `#` followed by three or six hex
/// digits falls back to the muted text color.
pub(super) fn env_dot_color(color: Option<&str>) -> String {
    match color {
        Some(value)
            if value.strip_prefix('#').is_some_and(|hex| {
                matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
            }) =>
        {
            value.to_string()
        }
        _ => ENV_COLOR_FALLBACK.to_string(),
    }
}

/// Returns the highest role of a user. A user with no role reads as Viewer,
/// the least privileged.
pub(super) fn highest_role(roles: &[Role]) -> Role {
    if roles.contains(&Role::Admin) {
        Role::Admin
    } else if roles.contains(&Role::Operator) {
        Role::Operator
    } else {
        Role::Viewer
    }
}

/// Returns the display name of a role.
pub(super) fn role_label(role: Role) -> &'static str {
    match role {
        Role::Admin => "Admin",
        Role::Operator => "Operator",
        Role::Viewer => "Viewer",
    }
}

/// Returns the description under the card title.
pub(super) fn card_description(role: Role) -> String {
    let approval = if role == Role::Viewer {
        " Viewer tokens are read-only."
    } else {
        " No administrator approval needed for allowed scopes."
    };
    format!(
        "Connect a coding agent to your CVE and POA&M work.{approval} Tokens never exceed your role or assigned environments."
    )
}

/// Returns the sentence under the create dialog title.
pub(super) fn create_intro(role: Role) -> String {
    let approval = if role == Role::Operator {
        "Operator access does not require Admin approval. "
    } else {
        ""
    };
    format!("Create your own credential for an agent. {approval}The secret is shown once.")
}

/// Returns the help line of the Environments section.
pub(super) fn environments_help(role: Role) -> &'static str {
    if role == Role::Admin {
        "Select explicit environments. Admin tokens never get an implicit all-environments grant."
    } else {
        "Select at least one of your assigned environments."
    }
}

/// Returns the sentence of the Review box about the role and grant limits.
pub(super) fn review_note(role: Role) -> String {
    format!(
        "Cannot exceed your current {} role or assigned environments. Grants are fixed after creation; to change them, create a replacement.",
        role_label(role)
    )
}

/// Returns the text of the empty list state.
pub(super) fn empty_list_text(role: Role) -> String {
    let verb = if role == Role::Viewer {
        "read "
    } else {
        "read and work on "
    };
    format!("Create one to let an MCP coding agent {verb}CVEs and POA&Ms in your environments.")
}

/// Returns the help text under the token list.
///
/// The sentence about OIDC appears only for an OIDC-owned user, and only with
/// the number of hours that the server reported.
pub(super) fn footer_note(oidc_owned: bool, max_age_hours: Option<u32>) -> String {
    match (oidc_owned, max_age_hours) {
        (true, Some(hours)) => format!(
            "{FOOTER_NOTE} OIDC tokens pause if your group membership hasn't been refreshed by a sign-in in the last {hours} hours."
        ),
        _ => FOOTER_NOTE.to_string(),
    }
}

/// Returns the masked form of a token: its public prefix and id, then
/// bullets in place of the secret.
///
/// SECURITY: The input is the public token id, never the token text, so this
/// function cannot leak any part of the secret. The 32 lowercase hex digits
/// equal the id embedded in the plaintext token.
pub(super) fn masked_token(id: Uuid) -> String {
    format!("cfat_v1_{}_{}", id.simple(), "•".repeat(MASK_BULLETS))
}

fn is_read_scope(scope: &str) -> bool {
    scope.ends_with(":read")
}

/// Summarizes granted scopes as `Read · 3` or `Read + write · 3`.
pub(super) fn scope_summary(scopes: &[String]) -> String {
    if scopes.is_empty() {
        "None".to_string()
    } else if scopes.iter().all(|scope| is_read_scope(scope)) {
        format!("Read · {}", scopes.len())
    } else {
        format!("Read + write · {}", scopes.len())
    }
}

/// Formats a lifetime option, for example `30 days`.
pub(super) fn lifetime_label(days: u32) -> String {
    plural(i64::from(days), "day")
}

/// Summarizes the grant under review, for example
/// `1 environment · 3 read scopes · 30 days`.
pub(super) fn review_summary(
    environment_count: usize,
    scopes: &[String],
    lifetime_days: u32,
) -> String {
    let scope_count = scopes.len();
    let scope_text = if scopes.iter().all(|scope| is_read_scope(scope)) {
        plural(scope_count as i64, "read scope")
    } else {
        format!("{scope_count} scopes (includes write)")
    };
    format!(
        "{} · {} · {}",
        plural(environment_count as i64, "environment"),
        scope_text,
        lifetime_label(lifetime_days),
    )
}

/// Describes where the agent connects.
///
/// `None` means that the policy is unavailable. The text never claims that an
/// endpoint works while the MCP server is disabled.
pub(super) fn mcp_endpoint_text(mcp: Option<&AccessTokenMcpDto>) -> String {
    match mcp {
        Some(AccessTokenMcpDto {
            url: Some(url),
            enabled: true,
        }) => url.clone(),
        Some(AccessTokenMcpDto {
            url: Some(url),
            enabled: false,
        }) => format!("{url} - the MCP server is not enabled yet"),
        _ => "MCP endpoint not configured - the MCP server is not enabled yet".to_string(),
    }
}

/// Returns the short type name of a token audience, for the Type chip.
pub(super) fn audience_label(audience: &str) -> &str {
    if audience == "mcp" { "MCP" } else { audience }
}

/// Returns why a scope cannot be selected, or `None` for an available scope.
pub(super) fn scope_unavailable_text(scope: &AccessTokenPolicyScopeDto) -> Option<String> {
    (!scope.available).then(|| {
        scope
            .unavailable_reason
            .clone()
            .unwrap_or_else(|| "Unavailable".to_string())
    })
}

/// Returns the message that disables the whole card, if the server disabled
/// access tokens.
///
/// The policy endpoint and the list endpoint both report the state. The
/// policy shows it first, so the list error is the fallback.
pub(super) fn feature_disabled_message(
    policy: Option<&Result<AccessTokenPolicyDto, AccessTokenApiError>>,
    list_error: Option<&AccessTokenApiError>,
) -> Option<String> {
    match policy {
        Some(Err(error)) if error.is_feature_disabled() => Some(error.message.clone()),
        Some(Ok(policy)) if !policy.enabled => {
            Some("Access tokens are disabled on this server.".to_string())
        }
        _ => list_error
            .filter(|error| error.is_feature_disabled())
            .map(|error| error.message.clone()),
    }
}

/// Returns the message to show after a state-changing request failed.
///
/// An unknown outcome gets `unknown_outcome_text` instead of the transport
/// error, because the request may have succeeded.
pub(super) fn failure_message(error: &AccessTokenApiError, unknown_outcome_text: &str) -> String {
    if error.outcome_unknown() {
        unknown_outcome_text.to_string()
    } else {
        error.message.clone()
    }
}

/// Maps a failed create request onto the form.
///
/// A response with field messages marks those fields. A response without them
/// shows its message at the top of the form. The form values stay unchanged.
pub(super) fn create_failure(error: &AccessTokenApiError) -> CreateErrors {
    if error.outcome_unknown() {
        return CreateErrors {
            general: Some(CREATE_UNKNOWN_OUTCOME.to_string()),
            ..CreateErrors::default()
        };
    }
    let mut errors = server_field_errors(&error.fields);
    if errors.is_empty() {
        errors.general = Some(error.message.clone());
    }
    errors
}

/// Maps an audit action name to an activity line without its date. Unknown
/// names pass through.
pub(super) fn event_label(action: &str) -> String {
    match action {
        "access_token_created" => "Created by you from Profile".to_string(),
        "access_token_rotated" => "Rotated".to_string(),
        "access_token_revoked" => "Revoked by you".to_string(),
        other => other.to_string(),
    }
}

/// Returns the name of the signed-in owner for the Details drawer: the display
/// name when set, otherwise the part of the email before `@`.
pub(super) fn owner_name(display_name: Option<&str>, email: &str) -> String {
    display_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map_or_else(
            || email.split('@').next().unwrap_or_default().to_string(),
            str::to_string,
        )
}

/// Returns the line under the "Effective now" chips for a token without
/// effective environments: `No effective access.`, with the token status for
/// a token that is not live.
pub(super) fn no_effective_access_text(status: AccessTokenStatus) -> String {
    if is_live(status) {
        "No effective access.".to_string()
    } else {
        format!(
            "No effective access (token {}).",
            status_presentation(status).label.to_lowercase()
        )
    }
}

/// Returns the line that names granted scopes the owner's current role no
/// longer permits, or `None` when every granted scope is permitted.
///
/// The approved design has no such line. The effective-access response
/// reports it, and hiding a shrunk grant would mislead the owner.
pub(super) fn unpermitted_scopes_text(scopes: &[&str]) -> Option<String> {
    (!scopes.is_empty())
        .then(|| format!("Not permitted by your current role: {}.", scopes.join(", ")))
}

/// Returns the "Unavailable: a, b (removed from your assignments)." line, or
/// `None` when no granted environment is unavailable.
pub(super) fn unavailable_environments_text(names: &[&str]) -> Option<String> {
    (!names.is_empty()).then(|| {
        format!(
            "Unavailable: {} (removed from your assignments).",
            names.join(", ")
        )
    })
}

/// Returns why creation is unavailable, or `None` when the user can create.
pub(super) fn create_disabled_reason(policy: &AccessTokenPolicyDto) -> Option<String> {
    if !policy.enabled {
        Some("Access tokens are disabled on this server.".to_string())
    } else if let Some(blocked) = policy.create_blocked.as_ref() {
        Some(blocked.message.clone())
    } else if policy.environments.is_empty() {
        Some("You have no environments available for access tokens.".to_string())
    } else {
        None
    }
}

/// Values of the create form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CreateForm {
    /// Raw token name as typed.
    pub(super) name: String,
    /// Selected lifetime in days.
    pub(super) expires_in_days: u32,
    /// Selected environments. Starts empty: there is no all-environments grant.
    pub(super) environment_ids: Vec<Uuid>,
    /// Selected scopes.
    pub(super) scopes: Vec<String>,
    /// State of the production write confirmation checkbox. It counts only
    /// while [`production_write_selected`] is `true`.
    pub(super) confirm_production: bool,
}

/// Returns the production write confirmation text for the current selection.
///
/// The text names the selected write capabilities by their policy labels and
/// the selected production environments by name, both in policy order, so the
/// owner confirms the exact grant. It extends the approved design sentence
/// ("... write operations in production. ...") with these names. The text is
/// shown only while [`production_write_selected`] is `true`; for another
/// selection the lists can be empty.
pub(super) fn production_write_confirmation_text(
    form: &CreateForm,
    policy: &AccessTokenPolicyDto,
) -> String {
    let capabilities: Vec<&str> = policy
        .scopes
        .iter()
        .filter(|scope| !is_read_scope(&scope.scope) && form.scopes.contains(&scope.scope))
        .map(|scope| scope.label.as_str())
        .collect();
    let environments: Vec<&str> = policy
        .environments
        .iter()
        .filter(|env| env.is_production && form.environment_ids.contains(&env.id))
        .map(|env| env.name.as_str())
        .collect();
    format!(
        "I understand this token can request write operations ({}) in production \
         environment(s) {}. Server authorization and approval workflows still apply.",
        capabilities.join(", "),
        environments.join(", ")
    )
}

/// Returns whether the selection combines a write scope with a production
/// environment. The dialog then shows the production write confirmation and
/// requires it, as the server does.
pub(super) fn production_write_selected(form: &CreateForm, policy: &AccessTokenPolicyDto) -> bool {
    form.scopes.iter().any(|scope| !is_read_scope(scope))
        && policy
            .environments
            .iter()
            .any(|env| env.is_production && form.environment_ids.contains(&env.id))
}

/// Returns the form state at dialog open.
///
/// No environment is selected. A scope is selected when the policy marks it
/// available and default-selected.
pub(super) fn initial_form(policy: &AccessTokenPolicyDto) -> CreateForm {
    let expires_in_days = if policy
        .lifetime_options_days
        .contains(&policy.default_lifetime_days)
    {
        policy.default_lifetime_days
    } else {
        policy
            .lifetime_options_days
            .first()
            .copied()
            .unwrap_or(policy.default_lifetime_days)
    };
    CreateForm {
        name: String::new(),
        expires_in_days,
        environment_ids: Vec::new(),
        scopes: policy
            .scopes
            .iter()
            .filter(|scope| scope.available && scope.default_selected)
            .map(|scope| scope.scope.clone())
            .collect(),
        confirm_production: false,
    }
}

/// Adds `value` to `selected` or removes it, without creating a duplicate.
pub(super) fn set_selected<T: PartialEq>(selected: &mut Vec<T>, value: T, on: bool) {
    selected.retain(|item| *item != value);
    if on {
        selected.push(value);
    }
}

/// Field-level and form-level messages of the create form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CreateErrors {
    /// Message for the name field.
    pub(super) name: Option<String>,
    /// Message for the expiration field.
    pub(super) expires: Option<String>,
    /// Message for the environment group.
    pub(super) environments: Option<String>,
    /// Message for the scope group.
    pub(super) scopes: Option<String>,
    /// Message for the production write confirmation.
    pub(super) production: Option<String>,
    /// Server messages that name no field of this form.
    pub(super) general: Option<String>,
}

impl CreateErrors {
    /// Returns whether no message is set.
    pub(super) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Returns the lowercase names of the owner's non-revoked tokens, for the
/// duplicate-name check of the create form.
///
/// The server allows one non-revoked token per name and owner, ignoring case.
/// The list holds only the pages and filter result that the card loaded, so
/// the check is a hint. The server still decides.
pub(super) fn existing_token_names(items: &[AccessTokenDto]) -> Vec<String> {
    items
        .iter()
        .filter(|token| token.status != AccessTokenStatus::Revoked)
        .map(|token| token.name.trim().to_lowercase())
        .collect()
}

/// Checks the form against the contract limits and the loaded policy.
///
/// `existing_names` holds lowercase names from [`existing_token_names`].
pub(super) fn validate_create(
    form: &CreateForm,
    policy: &AccessTokenPolicyDto,
    existing_names: &[String],
) -> CreateErrors {
    let mut errors = CreateErrors::default();
    let name_chars = form.name.trim().chars().count();
    if name_chars == 0 {
        errors.name = Some("Give the token a name so you can recognize it later.".to_string());
    } else if name_chars > NAME_MAX_CHARS {
        errors.name = Some(format!("Use {NAME_MAX_CHARS} characters or fewer."));
    } else if existing_names.contains(&form.name.trim().to_lowercase()) {
        errors.name = Some("You already have a token with this name.".to_string());
    }
    if !policy.lifetime_options_days.contains(&form.expires_in_days)
        || form.expires_in_days == 0
        || form.expires_in_days > policy.max_lifetime_days
    {
        errors.expires = Some(format!(
            "Choose one of the offered lifetimes (at most {} days).",
            policy.max_lifetime_days
        ));
    }
    if form.environment_ids.is_empty() {
        errors.environments = Some("Select at least one environment.".to_string());
    } else if form.environment_ids.len() > MAX_ENVIRONMENTS {
        errors.environments = Some(format!("Select at most {MAX_ENVIRONMENTS} environments."));
    } else if form
        .environment_ids
        .iter()
        .any(|id| !policy.environments.iter().any(|env| env.id == *id))
    {
        errors.environments = Some("A selected environment is no longer available.".to_string());
    }
    if form.scopes.is_empty() {
        errors.scopes = Some("Select at least one capability.".to_string());
    } else if form.scopes.iter().any(|scope| {
        !policy
            .scopes
            .iter()
            .any(|offered| offered.available && offered.scope == *scope)
    }) {
        errors.scopes = Some("A selected capability is not available.".to_string());
    }
    if production_write_selected(form, policy) && !form.confirm_production {
        errors.production = Some("Confirm the production write access below.".to_string());
    }
    errors
}

/// Maps the `details.fields` of a `validation_failed` response onto the form.
///
/// A field that the form does not show goes to `general`, so no server message
/// is lost.
pub(super) fn server_field_errors(fields: &BTreeMap<String, String>) -> CreateErrors {
    let mut errors = CreateErrors::default();
    let mut general = Vec::new();
    for (field, message) in fields {
        match field.as_str() {
            "name" => errors.name = Some(message.clone()),
            "expires_in_days" => errors.expires = Some(message.clone()),
            "environment_ids" => errors.environments = Some(message.clone()),
            "scopes" => errors.scopes = Some(message.clone()),
            "confirm_production_write" => errors.production = Some(message.clone()),
            other => general.push(format!("{other}: {message}")),
        }
    }
    if !general.is_empty() {
        errors.general = Some(general.join(" "));
    }
    errors
}

/// Builds the request body. Environments and scopes follow the policy order,
/// so the body is stable for a given selection.
///
/// `confirm_production_write` is `true` only when the confirmation is shown
/// and checked. A checkbox state left over from an earlier selection is not
/// sent.
pub(super) fn build_create_request(
    form: &CreateForm,
    policy: &AccessTokenPolicyDto,
) -> CreateAccessTokenRequest {
    CreateAccessTokenRequest {
        name: form.name.trim().to_string(),
        audience: "mcp".to_string(),
        expires_in_days: form.expires_in_days,
        environment_ids: policy
            .environments
            .iter()
            .filter(|env| form.environment_ids.contains(&env.id))
            .map(|env| env.id)
            .collect(),
        scopes: policy
            .scopes
            .iter()
            .filter(|offered| form.scopes.contains(&offered.scope))
            .map(|offered| offered.scope.clone())
            .collect(),
        confirm_production_write: form.confirm_production
            && production_write_selected(form, policy),
    }
}

/// Appends the tokens of a later page to `items`.
///
/// A token that a concurrent change moved across the page boundary can appear
/// on two pages. The first copy wins, so no row repeats.
pub(super) fn merge_page(items: &mut Vec<AccessTokenDto>, page: Vec<AccessTokenDto>) {
    for token in page {
        if !items.iter().any(|existing| existing.id == token.id) {
            items.push(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{
        AccessTokenCreateBlockedDto, AccessTokenPolicyEnvironmentDto, AccessTokenPolicyScopeDto,
    };
    use chrono::Duration;

    fn env_id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn policy() -> AccessTokenPolicyDto {
        let scope =
            |scope: &str, available: bool, default_selected: bool| AccessTokenPolicyScopeDto {
                scope: scope.to_string(),
                label: scope.to_string(),
                available,
                default_selected,
                unavailable_reason: (!available).then(|| "Not supported yet".to_string()),
            };
        AccessTokenPolicyDto {
            enabled: true,
            mcp: AccessTokenMcpDto {
                enabled: false,
                url: None,
            },
            audience: "mcp".to_string(),
            default_lifetime_days: 30,
            max_lifetime_days: 90,
            lifetime_options_days: vec![7, 30, 90],
            max_active_tokens: 20,
            environments: vec![
                AccessTokenPolicyEnvironmentDto {
                    id: env_id(1),
                    name: "production".to_string(),
                    color: Some("#f87171".to_string()),
                    is_production: true,
                },
                AccessTokenPolicyEnvironmentDto {
                    id: env_id(2),
                    name: "staging".to_string(),
                    color: None,
                    is_production: false,
                },
            ],
            scopes: vec![
                scope("cves:read", true, true),
                scope("poams:read", true, true),
                scope("provenance:read", true, false),
                scope("poams:write", false, false),
            ],
            create_blocked: None,
            oidc_owned: false,
            entitlement_max_age_hours: None,
        }
    }

    fn token_dto(id: u128, name: &str) -> AccessTokenDto {
        let now = Utc::now();
        AccessTokenDto {
            id: Uuid::from_u128(id),
            name: name.to_string(),
            audience: "mcp".to_string(),
            display_hint: "cfat_v1_0000…".to_string(),
            scopes: vec!["cves:read".to_string()],
            environments: Vec::new(),
            created_at: now,
            expires_at: now + Duration::days(30),
            revoked_at: None,
            last_used_at: None,
            status: AccessTokenStatus::Active,
        }
    }

    fn valid_form() -> CreateForm {
        CreateForm {
            name: "OpenCode - Sledge".to_string(),
            expires_in_days: 30,
            environment_ids: vec![env_id(2)],
            scopes: vec!["cves:read".to_string()],
            confirm_production: false,
        }
    }

    #[test]
    fn status_presentation_has_text_and_glyph_for_every_status() {
        let cases = [
            (AccessTokenStatus::Active, "Active", IconName::Check),
            (
                AccessTokenStatus::ExpiringSoon,
                "Expiring soon",
                IconName::Clock,
            ),
            (AccessTokenStatus::Expired, "Expired", IconName::Clock),
            (AccessTokenStatus::Revoked, "Revoked", IconName::X),
            (
                AccessTokenStatus::OwnerSuspended,
                "Access suspended",
                IconName::Warn,
            ),
            (AccessTokenStatus::Unknown, "Unknown", IconName::Info),
        ];
        for (status, label, icon) in cases {
            let presentation = status_presentation(status);
            assert_eq!(presentation.label, label);
            assert_eq!(presentation.icon, icon);
            assert!(presentation.chip_class.starts_with("chip-"));
        }
    }

    #[test]
    fn only_live_statuses_offer_rotate_and_revoke() {
        assert!(is_live(AccessTokenStatus::Active));
        assert!(is_live(AccessTokenStatus::ExpiringSoon));
        // The design lists a suspended token as live: it can still be revoked.
        assert!(is_live(AccessTokenStatus::OwnerSuspended));
        for status in [
            AccessTokenStatus::Expired,
            AccessTokenStatus::Revoked,
            AccessTokenStatus::Unknown,
        ] {
            assert!(!is_live(status));
        }
    }

    #[test]
    fn status_chips_match_the_design() {
        let cases = [
            (AccessTokenStatus::Active, "chip-healthy"),
            (AccessTokenStatus::ExpiringSoon, "chip-warning"),
            (AccessTokenStatus::Expired, "chip-unknown"),
            (AccessTokenStatus::Revoked, "chip-critical"),
            (AccessTokenStatus::OwnerSuspended, "chip-warning"),
        ];
        for (status, chip) in cases {
            assert_eq!(status_presentation(status).chip_class, chip);
        }
    }

    #[test]
    fn status_filter_maps_to_query_values_and_back() {
        assert_eq!(StatusFilter::All.param(), None);
        assert_eq!(StatusFilter::Active.param(), Some("active"));
        assert_eq!(StatusFilter::Expired.param(), Some("expired"));
        assert_eq!(StatusFilter::Revoked.param(), Some("revoked"));
        for filter in StatusFilter::ALL {
            assert_eq!(StatusFilter::from_value(filter.value()), filter);
        }
        assert_eq!(StatusFilter::from_value("bogus"), StatusFilter::All);
    }

    #[test]
    fn env_dot_color_accepts_hex_and_falls_back_otherwise() {
        assert_eq!(env_dot_color(Some("#f87171")), "#f87171");
        assert_eq!(env_dot_color(Some("#FBB")), "#FBB");
        let fallback = "var(--cf-text-muted)";
        assert_eq!(env_dot_color(None), fallback);
        assert_eq!(env_dot_color(Some("")), fallback);
        assert_eq!(env_dot_color(Some("red")), fallback);
        assert_eq!(env_dot_color(Some("#12345")), fallback);
        assert_eq!(env_dot_color(Some("#gggggg")), fallback);
        assert_eq!(env_dot_color(Some("#fff; background: url(x)")), fallback);
    }

    #[test]
    fn scope_summary_separates_read_from_write() {
        let read = vec!["cves:read".to_string(), "poams:read".to_string()];
        assert_eq!(scope_summary(&read), "Read · 2");
        let mixed = vec!["cves:read".to_string(), "poams:write".to_string()];
        assert_eq!(scope_summary(&mixed), "Read + write · 2");
        assert_eq!(scope_summary(&[]), "None");
    }

    #[test]
    fn review_summary_uses_singular_and_plural_forms() {
        let three = vec![
            "cves:read".to_string(),
            "poams:read".to_string(),
            "provenance:read".to_string(),
        ];
        assert_eq!(
            review_summary(1, &three, 30),
            "1 environment · 3 read scopes · 30 days"
        );
        assert_eq!(
            review_summary(2, &three[..1], 1),
            "2 environments · 1 read scope · 1 day"
        );
        let mixed = vec!["cves:read".to_string(), "poams:write".to_string()];
        assert_eq!(
            review_summary(1, &mixed, 7),
            "1 environment · 2 scopes (includes write) · 7 days"
        );
    }

    #[test]
    fn lifetime_label_pluralizes() {
        assert_eq!(lifetime_label(1), "1 day");
        assert_eq!(lifetime_label(30), "30 days");
    }

    #[test]
    fn relative_times_pick_the_largest_whole_unit() {
        let now = Utc::now();
        assert_eq!(relative_past(now - Duration::seconds(10), now), "just now");
        assert_eq!(
            relative_past(now - Duration::minutes(1), now),
            "1 minute ago"
        );
        assert_eq!(relative_past(now - Duration::hours(5), now), "5 hours ago");
        assert_eq!(relative_past(now - Duration::days(3), now), "3 days ago");
        assert_eq!(relative_past(now + Duration::days(3), now), "just now");
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn last_used_reads_never_or_a_short_date_with_the_full_date_in_the_title() {
        let now = at("2026-11-16T12:00:00Z");
        assert_eq!(last_used_short(None), "never");
        assert_eq!(last_used_short(Some(at("2026-10-03T08:00:00Z"))), "Oct 3");
        assert_eq!(
            last_used_title(Some(now - Duration::hours(2)), now),
            "2026-11-16 (2 hours ago)"
        );
        assert_eq!(last_used_title(None, now), "This token has not been used");
    }

    #[test]
    fn relative_expiry_counts_down_and_reports_past_expiry() {
        let now = Utc::now();
        assert_eq!(relative_expiry(now + Duration::days(29), now), "in 29 days");
        assert_eq!(relative_expiry(now + Duration::days(1), now), "in 1 day");
        assert_eq!(relative_expiry(now + Duration::hours(5), now), "in 5 hours");
        assert_eq!(
            relative_expiry(now + Duration::minutes(3), now),
            "in 3 minutes"
        );
        assert_eq!(
            relative_expiry(now + Duration::seconds(20), now),
            "in less than a minute"
        );
        assert_eq!(
            relative_expiry(now - Duration::days(2), now),
            "expired 2 days ago"
        );
    }

    #[test]
    fn format_date_is_a_utc_calendar_date() {
        let at = DateTime::parse_from_rfc3339("2026-12-02T23:59:59Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        assert_eq!(format_date(at), "2026-12-02");
    }

    #[test]
    fn format_short_date_is_month_abbreviation_and_unpadded_day() {
        assert_eq!(format_short_date(at("2026-11-02T23:59:59Z")), "Nov 2");
        assert_eq!(format_short_date(at("2026-12-16T00:00:00Z")), "Dec 16");
        assert_eq!(format_short_date(at("2027-01-01T00:00:00Z")), "Jan 1");
    }

    #[test]
    fn expiry_date_adds_calendar_days_across_month_and_year_ends() {
        let created = at("2026-11-16T10:00:00Z");
        assert_eq!(expiry_date(created, 7), "Nov 23");
        assert_eq!(expiry_date(created, 30), "Dec 16");
        assert_eq!(expiry_date(created, 90), "Feb 14");
        assert_eq!(expiry_date(at("2026-12-31T23:00:00Z"), 1), "Jan 1");
        assert_eq!(expiry_date(at("2028-02-28T00:00:00Z"), 1), "Feb 29");
        assert_eq!(expiry_date(created, 0), "Nov 16");
    }

    #[test]
    fn card_description_depends_on_the_role() {
        let viewer = card_description(Role::Viewer);
        assert_eq!(
            viewer,
            "Connect a coding agent to your CVE and POA&M work. Viewer tokens are read-only. Tokens never exceed your role or assigned environments."
        );
        for role in [Role::Operator, Role::Admin] {
            assert_eq!(
                card_description(role),
                "Connect a coding agent to your CVE and POA&M work. No administrator approval needed for allowed scopes. Tokens never exceed your role or assigned environments."
            );
        }
    }

    #[test]
    fn role_dependent_dialog_copy() {
        assert_eq!(
            create_intro(Role::Operator),
            "Create your own credential for an agent. Operator access does not require Admin approval. The secret is shown once."
        );
        for role in [Role::Viewer, Role::Admin] {
            assert_eq!(
                create_intro(role),
                "Create your own credential for an agent. The secret is shown once."
            );
        }
        assert!(environments_help(Role::Admin).starts_with("Select explicit environments."));
        assert_eq!(
            environments_help(Role::Operator),
            "Select at least one of your assigned environments."
        );
        assert_eq!(
            environments_help(Role::Viewer),
            environments_help(Role::Operator)
        );
        assert!(review_note(Role::Operator).starts_with(
            "Cannot exceed your current Operator role or assigned environments. Grants are fixed after creation; to change them, create a replacement."
        ));
        assert!(review_note(Role::Admin).contains("current Admin role"));
        assert_eq!(
            empty_list_text(Role::Viewer),
            "Create one to let an MCP coding agent read CVEs and POA&Ms in your environments."
        );
        assert_eq!(
            empty_list_text(Role::Operator),
            "Create one to let an MCP coding agent read and work on CVEs and POA&Ms in your environments."
        );
    }

    #[test]
    fn highest_role_prefers_admin_then_operator_and_defaults_to_viewer() {
        assert_eq!(highest_role(&[]), Role::Viewer);
        assert_eq!(highest_role(&[Role::Viewer]), Role::Viewer);
        assert_eq!(
            highest_role(&[Role::Viewer, Role::Operator]),
            Role::Operator
        );
        assert_eq!(highest_role(&[Role::Operator, Role::Admin]), Role::Admin);
    }

    #[test]
    fn footer_note_adds_the_oidc_sentence_only_for_oidc_owned_users() {
        let base = "Tokens keep working after you sign out of the browser. They stop on revocation, expiry, account disable, or when your role or environment access is removed.";
        assert_eq!(footer_note(false, None), base);
        assert_eq!(footer_note(false, Some(24)), base);
        // Without a reported number of hours the sentence would be a guess.
        assert_eq!(footer_note(true, None), base);
        assert_eq!(
            footer_note(true, Some(24)),
            format!(
                "{base} OIDC tokens pause if your group membership hasn't been refreshed by a sign-in in the last 24 hours."
            )
        );
    }

    #[test]
    fn masked_token_is_prefix_id_and_28_bullets_without_any_secret() {
        let id = Uuid::parse_str("8d12aaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee").expect("uuid");
        let masked = masked_token(id);
        assert_eq!(
            masked,
            format!(
                "cfat_v1_8d12aaaabbbb4ccc8dddeeeeeeeeeeee_{}",
                "•".repeat(28)
            )
        );
        let (prefix, bullets) = masked.rsplit_once('_').expect("separator");
        assert_eq!(prefix, "cfat_v1_8d12aaaabbbb4ccc8dddeeeeeeeeeeee");
        assert_eq!(bullets.chars().count(), 28);
        assert!(bullets.chars().all(|c| c == '•'));
        assert_eq!(prefix.trim_start_matches("cfat_v1_").len(), 32);
    }

    #[test]
    fn owner_name_prefers_the_display_name_then_the_email_local_part() {
        assert_eq!(
            owner_name(Some("Jordan Park"), "jp@example.com"),
            "Jordan Park"
        );
        assert_eq!(owner_name(Some("  "), "jp@example.com"), "jp");
        assert_eq!(owner_name(None, "jp@example.com"), "jp");
    }

    #[test]
    fn effective_access_lines_follow_the_design() {
        assert_eq!(
            no_effective_access_text(AccessTokenStatus::OwnerSuspended),
            "No effective access."
        );
        assert_eq!(
            no_effective_access_text(AccessTokenStatus::Revoked),
            "No effective access (token revoked)."
        );
        assert_eq!(
            no_effective_access_text(AccessTokenStatus::Expired),
            "No effective access (token expired)."
        );
        assert_eq!(unpermitted_scopes_text(&[]), None);
        assert_eq!(
            unpermitted_scopes_text(&["poams:write"]).as_deref(),
            Some("Not permitted by your current role: poams:write.")
        );
        assert_eq!(unavailable_environments_text(&[]), None);
        assert_eq!(
            unavailable_environments_text(&["edge", "lab"]).as_deref(),
            Some("Unavailable: edge, lab (removed from your assignments).")
        );
    }

    #[test]
    fn mcp_endpoint_text_is_honest_about_a_disabled_server() {
        let enabled = AccessTokenMcpDto {
            enabled: true,
            url: Some("https://forge.example.com/mcp".to_string()),
        };
        assert_eq!(
            mcp_endpoint_text(Some(&enabled)),
            "https://forge.example.com/mcp"
        );
        let disabled = AccessTokenMcpDto {
            enabled: false,
            url: None,
        };
        let expected = "MCP endpoint not configured - the MCP server is not enabled yet";
        assert_eq!(mcp_endpoint_text(Some(&disabled)), expected);
        assert_eq!(mcp_endpoint_text(None), expected);
        let configured_but_off = AccessTokenMcpDto {
            enabled: false,
            url: Some("https://forge.example.com/mcp".to_string()),
        };
        assert!(
            mcp_endpoint_text(Some(&configured_but_off))
                .ends_with("the MCP server is not enabled yet")
        );
    }

    #[test]
    fn header_example_names_an_environment_variable() {
        assert_eq!(AUTH_HEADER_EXAMPLE, "Authorization: Bearer $CF_MCP_TOKEN");
        assert!(!AUTH_HEADER_EXAMPLE.contains("cfat_v1_"));
    }

    #[test]
    fn event_labels_cover_token_audit_actions() {
        assert_eq!(
            event_label("access_token_created"),
            "Created by you from Profile"
        );
        assert_eq!(event_label("access_token_rotated"), "Rotated");
        assert_eq!(event_label("access_token_revoked"), "Revoked by you");
        assert_eq!(event_label("something_else"), "something_else");
    }

    #[test]
    fn existing_names_skip_revoked_tokens_and_ignore_case() {
        let mut revoked = token_dto(2, "Old Bot");
        revoked.status = AccessTokenStatus::Revoked;
        let mut expired = token_dto(3, "  Prototype ");
        expired.status = AccessTokenStatus::Expired;
        let names = existing_token_names(&[token_dto(1, "OpenCode - Sledge"), revoked, expired]);
        assert_eq!(names, vec!["opencode - sledge", "prototype"]);

        let mut form = valid_form();
        form.name = "  OPENCODE - sledge ".to_string();
        assert_eq!(
            validate_create(&form, &policy(), &names).name.as_deref(),
            Some("You already have a token with this name.")
        );
        form.name = "Old Bot".to_string();
        assert!(validate_create(&form, &policy(), &names).name.is_none());
    }

    #[test]
    fn create_is_disabled_with_a_visible_reason() {
        assert_eq!(create_disabled_reason(&policy()), None);

        let mut disabled = policy();
        disabled.enabled = false;
        assert!(
            create_disabled_reason(&disabled)
                .expect("reason")
                .contains("disabled")
        );

        let mut blocked = policy();
        blocked.create_blocked = Some(AccessTokenCreateBlockedDto {
            code: "entitlements_stale".to_string(),
            message: "Sign in again to refresh your groups.".to_string(),
        });
        assert_eq!(
            create_disabled_reason(&blocked).as_deref(),
            Some("Sign in again to refresh your groups.")
        );

        let mut no_environments = policy();
        no_environments.environments.clear();
        assert!(
            create_disabled_reason(&no_environments)
                .expect("reason")
                .contains("no environments")
        );
    }

    #[test]
    fn initial_form_selects_no_environment_and_default_scopes() {
        let form = initial_form(&policy());
        assert!(form.name.is_empty());
        assert_eq!(form.expires_in_days, 30);
        assert!(form.environment_ids.is_empty());
        assert_eq!(form.scopes, vec!["cves:read", "poams:read"]);
    }

    #[test]
    fn initial_form_falls_back_to_an_offered_lifetime() {
        let mut odd = policy();
        odd.default_lifetime_days = 45;
        assert_eq!(initial_form(&odd).expires_in_days, 7);
    }

    #[test]
    fn set_selected_toggles_without_duplicates() {
        let mut selected = vec![1, 2];
        set_selected(&mut selected, 2, true);
        assert_eq!(selected, vec![1, 2]);
        set_selected(&mut selected, 1, false);
        assert_eq!(selected, vec![2]);
        set_selected(&mut selected, 3, true);
        assert_eq!(selected, vec![2, 3]);
    }

    #[test]
    fn valid_form_has_no_errors() {
        assert!(validate_create(&valid_form(), &policy(), &[]).is_empty());
    }

    #[test]
    fn name_must_be_one_to_one_hundred_characters_after_trimming() {
        let mut form = valid_form();
        form.name = "   ".to_string();
        assert_eq!(
            validate_create(&form, &policy(), &[]).name.as_deref(),
            Some("Give the token a name so you can recognize it later.")
        );
        form.name = "é".repeat(NAME_MAX_CHARS);
        assert!(validate_create(&form, &policy(), &[]).name.is_none());
        form.name = "é".repeat(NAME_MAX_CHARS + 1);
        assert_eq!(
            validate_create(&form, &policy(), &[]).name.as_deref(),
            Some("Use 100 characters or fewer.")
        );
    }

    #[test]
    fn empty_selections_use_the_design_messages() {
        let mut form = valid_form();
        form.environment_ids.clear();
        form.scopes.clear();
        let errors = validate_create(&form, &policy(), &[]);
        assert_eq!(
            errors.environments.as_deref(),
            Some("Select at least one environment.")
        );
        assert_eq!(
            errors.scopes.as_deref(),
            Some("Select at least one capability.")
        );
    }

    #[test]
    fn environments_must_be_explicit_known_and_bounded() {
        let mut form = valid_form();
        form.environment_ids.clear();
        assert!(
            validate_create(&form, &policy(), &[])
                .environments
                .is_some()
        );
        form.environment_ids = vec![env_id(99)];
        assert!(
            validate_create(&form, &policy(), &[])
                .environments
                .is_some()
        );
        form.environment_ids = (0..=MAX_ENVIRONMENTS as u128).map(env_id).collect();
        assert!(
            validate_create(&form, &policy(), &[])
                .environments
                .is_some()
        );
    }

    #[test]
    fn scopes_must_be_selected_and_available() {
        let mut form = valid_form();
        form.scopes.clear();
        assert!(validate_create(&form, &policy(), &[]).scopes.is_some());
        form.scopes = vec!["poams:write".to_string()];
        assert!(validate_create(&form, &policy(), &[]).scopes.is_some());
        form.scopes = vec!["unknown:read".to_string()];
        assert!(validate_create(&form, &policy(), &[]).scopes.is_some());
    }

    #[test]
    fn lifetime_must_be_offered_and_within_the_maximum() {
        let mut form = valid_form();
        form.expires_in_days = 14;
        assert!(validate_create(&form, &policy(), &[]).expires.is_some());
        form.expires_in_days = 0;
        assert!(validate_create(&form, &policy(), &[]).expires.is_some());
        let mut strict = policy();
        strict.max_lifetime_days = 30;
        form.expires_in_days = 90;
        assert!(validate_create(&form, &strict, &[]).expires.is_some());
    }

    #[test]
    fn server_field_errors_map_known_fields_and_keep_the_rest() {
        let fields = BTreeMap::from([
            ("name".to_string(), "Name is already in use.".to_string()),
            (
                "environment_ids".to_string(),
                "Unknown environment.".to_string(),
            ),
            ("scopes".to_string(), "Scope unavailable.".to_string()),
            ("expires_in_days".to_string(), "Too long.".to_string()),
            ("audience".to_string(), "Unsupported.".to_string()),
            (
                "confirm_production_write".to_string(),
                "Confirm the production write access.".to_string(),
            ),
        ]);
        let errors = server_field_errors(&fields);
        assert_eq!(
            errors.production.as_deref(),
            Some("Confirm the production write access.")
        );
        assert_eq!(errors.name.as_deref(), Some("Name is already in use."));
        assert_eq!(errors.environments.as_deref(), Some("Unknown environment."));
        assert_eq!(errors.scopes.as_deref(), Some("Scope unavailable."));
        assert_eq!(errors.expires.as_deref(), Some("Too long."));
        assert_eq!(errors.general.as_deref(), Some("audience: Unsupported."));
        assert!(server_field_errors(&BTreeMap::new()).is_empty());
    }

    #[test]
    fn create_request_trims_the_name_and_follows_policy_order() {
        let mut form = valid_form();
        form.name = "  OpenCode - Sledge  ".to_string();
        form.environment_ids = vec![env_id(2), env_id(1)];
        form.scopes = vec!["poams:read".to_string(), "cves:read".to_string()];
        let request = build_create_request(&form, &policy());
        assert_eq!(request.name, "OpenCode - Sledge");
        assert_eq!(request.audience, "mcp");
        assert_eq!(request.expires_in_days, 30);
        assert_eq!(request.environment_ids, vec![env_id(1), env_id(2)]);
        assert_eq!(request.scopes, vec!["cves:read", "poams:read"]);
        assert!(!request.confirm_production_write);
    }

    /// Returns the fixture policy with `poams:write` available, as for an
    /// Operator.
    fn write_policy() -> AccessTokenPolicyDto {
        let mut policy = policy();
        policy.scopes[3].available = true;
        policy.scopes[3].unavailable_reason = None;
        policy
    }

    #[test]
    fn production_write_needs_a_write_scope_and_a_production_environment() {
        let policy = write_policy();
        let mut form = valid_form();
        form.scopes = vec!["cves:read".to_string(), "poams:write".to_string()];
        // staging is not production.
        assert!(!production_write_selected(&form, &policy));
        form.environment_ids = vec![env_id(1)];
        assert!(production_write_selected(&form, &policy));
        form.scopes = vec!["cves:read".to_string()];
        assert!(!production_write_selected(&form, &policy));
    }

    #[test]
    fn production_write_requires_the_confirmation_and_sends_it() {
        let policy = write_policy();
        let mut form = valid_form();
        form.environment_ids = vec![env_id(1), env_id(2)];
        form.scopes = vec!["poams:write".to_string()];
        let errors = validate_create(&form, &policy, &[]);
        assert_eq!(
            errors.production.as_deref(),
            Some("Confirm the production write access below.")
        );
        assert!(errors.scopes.is_none());
        assert!(!build_create_request(&form, &policy).confirm_production_write);

        form.confirm_production = true;
        assert!(validate_create(&form, &policy, &[]).is_empty());
        assert!(build_create_request(&form, &policy).confirm_production_write);

        // A confirmation left checked from an earlier selection is not sent.
        form.environment_ids = vec![env_id(2)];
        assert!(validate_create(&form, &policy, &[]).is_empty());
        assert!(!build_create_request(&form, &policy).confirm_production_write);
    }

    /// Returns a policy with two production environments and both write
    /// scopes available under their server labels.
    fn labeled_write_policy() -> AccessTokenPolicyDto {
        let mut policy = write_policy();
        policy.environments[0].name = "LAN".to_string();
        policy.environments.push(AccessTokenPolicyEnvironmentDto {
            id: env_id(3),
            name: "prod-east".to_string(),
            color: None,
            is_production: true,
        });
        policy.scopes[3].label = "Update POA&M progress".to_string();
        policy.scopes.insert(
            3,
            AccessTokenPolicyScopeDto {
                scope: "scans:request".to_string(),
                label: "Request CVE scans".to_string(),
                available: true,
                default_selected: false,
                unavailable_reason: None,
            },
        );
        policy
    }

    #[test]
    fn production_confirmation_names_one_environment_and_one_capability() {
        let policy = labeled_write_policy();
        let mut form = valid_form();
        // The staging selection and the read scope are not named.
        form.environment_ids = vec![env_id(2), env_id(1)];
        form.scopes = vec!["cves:read".to_string(), "poams:write".to_string()];
        assert!(production_write_selected(&form, &policy));
        assert_eq!(
            production_write_confirmation_text(&form, &policy),
            "I understand this token can request write operations (Update POA&M progress) in \
             production environment(s) LAN. Server authorization and approval workflows still apply."
        );
    }

    #[test]
    fn production_confirmation_names_every_environment_and_capability_in_policy_order() {
        let policy = labeled_write_policy();
        let mut form = valid_form();
        form.environment_ids = vec![env_id(3), env_id(1)];
        form.scopes = vec!["poams:write".to_string(), "scans:request".to_string()];
        assert_eq!(
            production_write_confirmation_text(&form, &policy),
            "I understand this token can request write operations (Request CVE scans, Update \
             POA&M progress) in production environment(s) LAN, prod-east. Server authorization \
             and approval workflows still apply."
        );
    }

    fn api_error(
        status: Option<u16>,
        code: Option<&str>,
        message: &str,
        fields: &[(&str, &str)],
    ) -> AccessTokenApiError {
        AccessTokenApiError {
            status,
            code: code.map(str::to_string),
            message: message.to_string(),
            fields: fields
                .iter()
                .map(|(name, text)| (name.to_string(), text.to_string()))
                .collect(),
            unreadable: false,
        }
    }

    #[test]
    fn audience_and_unavailable_scope_text() {
        assert_eq!(audience_label("mcp"), "MCP");
        assert_eq!(audience_label("cli"), "cli");
        let policy = policy();
        assert_eq!(scope_unavailable_text(&policy.scopes[0]), None);
        assert_eq!(
            scope_unavailable_text(&policy.scopes[3]).as_deref(),
            Some("Not supported yet")
        );
        let mut without_reason = policy.scopes[3].clone();
        without_reason.unavailable_reason = None;
        assert_eq!(
            scope_unavailable_text(&without_reason).as_deref(),
            Some("Unavailable")
        );
    }

    #[test]
    fn feature_disabled_message_reads_policy_then_list_error() {
        let disabled = api_error(
            Some(403),
            Some("access_tokens_disabled"),
            "Access tokens are turned off.",
            &[],
        );
        let other = api_error(Some(500), None, "boom", &[]);
        assert_eq!(
            feature_disabled_message(Some(&Err(disabled.clone())), None).as_deref(),
            Some("Access tokens are turned off.")
        );
        let mut off = policy();
        off.enabled = false;
        assert!(feature_disabled_message(Some(&Ok(off)), None).is_some());
        assert_eq!(
            feature_disabled_message(Some(&Ok(policy())), Some(&disabled)).as_deref(),
            Some("Access tokens are turned off.")
        );
        assert_eq!(
            feature_disabled_message(None, Some(&disabled)).as_deref(),
            Some("Access tokens are turned off.")
        );
        assert_eq!(
            feature_disabled_message(Some(&Err(other.clone())), Some(&other)),
            None
        );
        assert_eq!(feature_disabled_message(Some(&Ok(policy())), None), None);
    }

    #[test]
    fn failure_message_hides_transport_detail_when_the_outcome_is_unknown() {
        let rejected = api_error(Some(409), Some("conflict"), "Grant not permitted.", &[]);
        assert_eq!(
            failure_message(&rejected, "unknown"),
            "Grant not permitted."
        );
        let lost = api_error(None, None, "Network error: offline", &[]);
        assert_eq!(failure_message(&lost, "unknown"), "unknown");
    }

    #[test]
    fn create_failure_marks_fields_and_preserves_unmapped_messages() {
        let validation = api_error(
            Some(400),
            Some("validation_failed"),
            "Request is invalid.",
            &[("name", "Name is already in use.")],
        );
        let errors = create_failure(&validation);
        assert_eq!(errors.name.as_deref(), Some("Name is already in use."));
        assert_eq!(errors.general, None);

        let limited = api_error(Some(429), Some("rate_limited"), "Too many tokens.", &[]);
        assert_eq!(
            create_failure(&limited).general.as_deref(),
            Some("Too many tokens.")
        );

        let lost = api_error(None, None, "Network error: offline", &[]);
        let errors = create_failure(&lost);
        assert_eq!(errors.general.as_deref(), Some(CREATE_UNKNOWN_OUTCOME));
        assert!(CREATE_UNKNOWN_OUTCOME.contains("revoke any token you do not recognize"));
        assert!(errors.name.is_none());
    }

    #[test]
    fn merge_page_appends_new_tokens_and_skips_repeats() {
        let mut items = vec![token_dto(1, "a"), token_dto(2, "b")];
        merge_page(&mut items, vec![token_dto(2, "b-again"), token_dto(3, "c")]);
        let names: Vec<_> = items.iter().map(|token| token.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        merge_page(&mut items, Vec::new());
        assert_eq!(items.len(), 3);
    }
}
