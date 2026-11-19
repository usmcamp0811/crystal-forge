//! Pure rules of the access-token card: labels, summaries, form validation and
//! list merging.
//!
//! Nothing here touches Dioxus signals or the browser, so every rule has a unit
//! test. The server repeats every validation. The client rules only avoid
//! round trips for input that the contract rejects.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::api::client::AccessTokenApiError;
use crate::api::models::{
    AccessTokenDto, AccessTokenEnvironmentDto, AccessTokenMcpDto, AccessTokenPolicyDto,
    AccessTokenPolicyScopeDto, AccessTokenStatus, CreateAccessTokenRequest,
};
use crate::components::icon::IconName;

/// Longest token name the server accepts, in characters.
pub(super) const NAME_MAX_CHARS: usize = 100;
/// Most environments the server accepts in one grant.
pub(super) const MAX_ENVIRONMENTS: usize = 50;
/// Tokens requested per list page. The server accepts 1 to 100.
pub(super) const LIST_PAGE_SIZE: u32 = 25;
/// Placeholder shown while the secret is hidden. Independent of the secret.
pub(super) const MASKED_TOKEN: &str = "cfat_v1_••••••••••••••••••••••••••••••••";
/// Header example for the reveal view. Names an environment variable, never
/// the secret.
pub(super) const AUTH_HEADER_EXAMPLE: &str = "Authorization: Bearer $CF_ACCESS_TOKEN";
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

/// Returns whether Rotate and Revoke apply to a token in this status.
///
/// Only a token that can authenticate offers them; every other row offers
/// Details only.
pub(super) fn can_manage(status: AccessTokenStatus) -> bool {
    matches!(
        status,
        AccessTokenStatus::Active | AccessTokenStatus::ExpiringSoon
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

/// Describes the last use of a token. `None` means the token was never used.
pub(super) fn last_used_text(last_used_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    last_used_at.map_or_else(|| "Never".to_string(), |at| relative_past(at, now))
}

/// Returns an environment name, marked when the environment is inactive.
pub(super) fn environment_label(environment: &AccessTokenEnvironmentDto) -> String {
    if environment.is_active {
        environment.name.clone()
    } else {
        format!("{} (inactive)", environment.name)
    }
}

/// Summarizes granted environments: up to two names, then a count.
pub(super) fn environment_summary(environments: &[AccessTokenEnvironmentDto]) -> String {
    match environments {
        [] => "None".to_string(),
        [one] => environment_label(one),
        [first, second] => format!(
            "{}, {}",
            environment_label(first),
            environment_label(second)
        ),
        [first, second, rest @ ..] => format!(
            "{}, {} +{} more",
            environment_label(first),
            environment_label(second),
            rest.len()
        ),
    }
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

/// Returns the display name of a token audience.
pub(super) fn audience_label(audience: &str) -> &str {
    if audience == "mcp" {
        "MCP Server"
    } else {
        audience
    }
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

/// Maps an audit action name to a short label. Unknown names pass through.
pub(super) fn event_label(action: &str) -> String {
    match action {
        "access_token_created" => "Created".to_string(),
        "access_token_rotated" => "Rotated".to_string(),
        "access_token_revoked" => "Revoked".to_string(),
        other => other.to_string(),
    }
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
    /// Server messages that name no field of this form.
    pub(super) general: Option<String>,
}

impl CreateErrors {
    /// Returns whether no message is set.
    pub(super) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Checks the form against the contract limits and the loaded policy.
pub(super) fn validate_create(form: &CreateForm, policy: &AccessTokenPolicyDto) -> CreateErrors {
    let mut errors = CreateErrors::default();
    let name_chars = form.name.trim().chars().count();
    if name_chars == 0 {
        errors.name = Some("Give the token a name so you can recognize it later.".to_string());
    } else if name_chars > NAME_MAX_CHARS {
        errors.name = Some(format!("Use {NAME_MAX_CHARS} characters or fewer."));
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
                },
                AccessTokenPolicyEnvironmentDto {
                    id: env_id(2),
                    name: "staging".to_string(),
                },
            ],
            scopes: vec![
                scope("cves:read", true, true),
                scope("poams:read", true, true),
                scope("provenance:read", true, false),
                scope("poams:write", false, false),
            ],
            create_blocked: None,
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

    fn environment(name: &str, is_active: bool) -> AccessTokenEnvironmentDto {
        AccessTokenEnvironmentDto {
            id: Uuid::new_v4(),
            name: name.to_string(),
            is_active,
        }
    }

    fn valid_form() -> CreateForm {
        CreateForm {
            name: "OpenCode - Sledge".to_string(),
            expires_in_days: 30,
            environment_ids: vec![env_id(2)],
            scopes: vec!["cves:read".to_string()],
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
    fn only_authenticating_statuses_offer_rotate_and_revoke() {
        assert!(can_manage(AccessTokenStatus::Active));
        assert!(can_manage(AccessTokenStatus::ExpiringSoon));
        for status in [
            AccessTokenStatus::Expired,
            AccessTokenStatus::Revoked,
            AccessTokenStatus::OwnerSuspended,
            AccessTokenStatus::Unknown,
        ] {
            assert!(!can_manage(status));
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
    fn environment_summary_names_two_then_counts() {
        assert_eq!(environment_summary(&[]), "None");
        assert_eq!(
            environment_summary(&[environment("staging", true)]),
            "staging"
        );
        assert_eq!(
            environment_summary(&[environment("a", true), environment("b", true)]),
            "a, b"
        );
        assert_eq!(
            environment_summary(&[
                environment("a", true),
                environment("b", true),
                environment("c", true),
                environment("d", true),
            ]),
            "a, b +2 more"
        );
        assert_eq!(
            environment_summary(&[environment("edge", false)]),
            "edge (inactive)"
        );
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
        assert_eq!(last_used_text(None, now), "Never");
        assert_eq!(
            last_used_text(Some(now - Duration::hours(2)), now),
            "2 hours ago"
        );
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
        assert!(AUTH_HEADER_EXAMPLE.contains("$CF_ACCESS_TOKEN"));
        assert!(!AUTH_HEADER_EXAMPLE.contains("cfat_v1_"));
        assert!(MASKED_TOKEN.starts_with("cfat_v1_"));
    }

    #[test]
    fn event_labels_cover_token_audit_actions() {
        assert_eq!(event_label("access_token_created"), "Created");
        assert_eq!(event_label("access_token_rotated"), "Rotated");
        assert_eq!(event_label("access_token_revoked"), "Revoked");
        assert_eq!(event_label("something_else"), "something_else");
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
        assert!(validate_create(&valid_form(), &policy()).is_empty());
    }

    #[test]
    fn name_must_be_one_to_one_hundred_characters_after_trimming() {
        let mut form = valid_form();
        form.name = "   ".to_string();
        assert!(validate_create(&form, &policy()).name.is_some());
        form.name = "é".repeat(NAME_MAX_CHARS);
        assert!(validate_create(&form, &policy()).name.is_none());
        form.name = "é".repeat(NAME_MAX_CHARS + 1);
        assert!(validate_create(&form, &policy()).name.is_some());
    }

    #[test]
    fn environments_must_be_explicit_known_and_bounded() {
        let mut form = valid_form();
        form.environment_ids.clear();
        assert!(validate_create(&form, &policy()).environments.is_some());
        form.environment_ids = vec![env_id(99)];
        assert!(validate_create(&form, &policy()).environments.is_some());
        form.environment_ids = (0..=MAX_ENVIRONMENTS as u128).map(env_id).collect();
        assert!(validate_create(&form, &policy()).environments.is_some());
    }

    #[test]
    fn scopes_must_be_selected_and_available() {
        let mut form = valid_form();
        form.scopes.clear();
        assert!(validate_create(&form, &policy()).scopes.is_some());
        form.scopes = vec!["poams:write".to_string()];
        assert!(validate_create(&form, &policy()).scopes.is_some());
        form.scopes = vec!["unknown:read".to_string()];
        assert!(validate_create(&form, &policy()).scopes.is_some());
    }

    #[test]
    fn lifetime_must_be_offered_and_within_the_maximum() {
        let mut form = valid_form();
        form.expires_in_days = 14;
        assert!(validate_create(&form, &policy()).expires.is_some());
        form.expires_in_days = 0;
        assert!(validate_create(&form, &policy()).expires.is_some());
        let mut strict = policy();
        strict.max_lifetime_days = 30;
        form.expires_in_days = 90;
        assert!(validate_create(&form, &strict).expires.is_some());
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
        ]);
        let errors = server_field_errors(&fields);
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
        assert_eq!(audience_label("mcp"), "MCP Server");
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
