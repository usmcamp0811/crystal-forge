//! Owner access-token lifecycle: policy, create, list, detail, revoke, rotate
//! and server-side validation of a presented token.
//!
//! Authority model: only the owner of a token manages it, and only through a
//! browser session. The handler authenticates the session and passes the owner
//! id here. This module never trusts roles or memberships read by the handler;
//! it reads them again inside the issuing transaction.
//!
//! Issuing (create and rotate) runs in one transaction. Lock order is the
//! owner advisory lock, the owner row `FOR SHARE`, token rows `FOR UPDATE`,
//! then environment rows `FOR SHARE` (see
//! [`crate::queries::access_tokens::lock_owner_tx`]). Check order inside the
//! transaction is: grant validation (400), hourly rate (429), live-token count
//! (409), name uniqueness (409). The audit event commits with the change.
//!
//! A token grant is immutable. Validation reports the grant as a
//! [`RestrictedGrant`], which the M0 authorization boundary intersects with the
//! owner's current roles and memberships. This module never widens it.
//!
//! Kill switch: `access_tokens_enable = false` blocks create, rotate and the
//! admission of a presented token ([`validate_presented_token`]). Policy, list,
//! detail and revoke keep working, so an owner can still see and revoke every
//! token. Nothing in this module reads or writes the switch except through
//! [`AccessTokenSettings`].

use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::models::{
    AccessTokenCreateBlockedDto, AccessTokenDetailDto, AccessTokenDto, AccessTokenEffectiveDto,
    AccessTokenEffectiveEnvironmentDto, AccessTokenEffectiveScopeDto, AccessTokenEnvironmentDto,
    AccessTokenEventDto, AccessTokenListDto, AccessTokenMcpDto, AccessTokenPolicyDto,
    AccessTokenPolicyEnvironmentDto, AccessTokenPolicyScopeDto, CreateAccessTokenRequest,
};
use crate::auth::access_token::{
    TokenSecret, generate_access_token, hash_access_token, hashes_equal, parse_access_token,
};
use crate::auth::authorization::RestrictedGrant;
use crate::config::ServerConfig;
use crate::queries::access_tokens as store;
use crate::queries::access_tokens::OwnerAccess;
use crate::queries::oidc_entitlements::require_fresh_entitlements;

/// Maximum non-revoked, unexpired tokens per owner.
pub const MAX_ACTIVE_TOKENS: i64 = 20;
/// Maximum environments in one token grant.
pub const MAX_ENVIRONMENTS_PER_TOKEN: usize = 50;
/// Maximum create plus rotate operations per owner in a rolling hour.
pub const MAX_ISSUES_PER_HOUR: i64 = 10;
/// Maximum characters in a token name.
pub const MAX_NAME_CHARS: usize = 100;
/// Default list page size.
pub const DEFAULT_PAGE_SIZE: i64 = 25;
/// Largest list page size; larger requests are clamped.
pub const MAX_PAGE_SIZE: i64 = 100;
/// Maximum characters in a list name filter.
const MAX_NAME_QUERY_CHARS: usize = 100;
/// Number of audit events returned with a token detail.
const RECENT_EVENT_LIMIT: i64 = 10;
/// Stand-in hash compared when the token id is unknown, so a lookup miss runs
/// the same comparison as a hit.
const ZERO_HASH: [u8; 32] = [0; 32];
/// Lifetime choices offered in addition to the default and maximum.
const LIFETIME_CHOICES_DAYS: [u32; 3] = [7, 30, 90];

/// Carries the access-token policy for one request.
#[derive(Debug, Clone)]
pub struct AccessTokenSettings {
    /// Mirrors `access_tokens_enable`.
    pub enabled: bool,
    /// Mirrors `access_tokens_default_lifetime_days`.
    pub default_lifetime_days: u32,
    /// Mirrors `access_tokens_max_lifetime_days`.
    pub max_lifetime_days: u32,
    /// Maximum OIDC entitlement age in seconds. Zero denies every OIDC owner.
    pub entitlement_max_age_seconds: u64,
    /// Mirrors `mcp_enable`. Reported by [`policy`] only; admission does not
    /// read it.
    pub mcp_enabled: bool,
    /// Mirrors `mcp_public_url`. [`policy`] reports it only when
    /// `mcp_enabled` is `true`.
    pub mcp_public_url: Option<String>,
}

impl AccessTokenSettings {
    /// Builds settings from the server configuration and the OIDC freshness
    /// bound.
    pub fn from_server_config(config: &ServerConfig, entitlement_max_age_seconds: u64) -> Self {
        Self {
            enabled: config.access_tokens_enable,
            default_lifetime_days: config.access_tokens_default_lifetime_days,
            max_lifetime_days: config.access_tokens_max_lifetime_days,
            entitlement_max_age_seconds,
            mcp_enabled: config.mcp_enable,
            mcp_public_url: config.mcp_public_url.clone(),
        }
    }
}

/// Reports why an access-token operation did not succeed.
#[derive(Debug)]
pub enum AccessTokenError {
    /// `access_tokens_enable` is off. Only create and rotate return it.
    Disabled,
    /// The request is invalid. The map goes from field name to message.
    Validation(BTreeMap<String, String>),
    /// The token does not exist or belongs to another user.
    NotFound,
    /// A lifecycle rule or limit blocks the request.
    Conflict {
        /// Safe message for display.
        message: String,
        /// Optional structured context such as `fields` or `code`.
        details: Option<serde_json::Value>,
    },
    /// The owner reached the hourly create and rotate limit.
    RateLimited,
    /// The owner is missing, disabled or has no role.
    OwnerUnavailable,
    /// A database or internal failure. The text is for server logs only.
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for AccessTokenError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

impl From<sqlx::Error> for AccessTokenError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(err.into())
    }
}

fn conflict(message: &str, details: Option<serde_json::Value>) -> AccessTokenError {
    AccessTokenError::Conflict {
        message: message.to_owned(),
        details,
    }
}

/// Name of the partial unique index on non-revoked `(user_id, lower(name))`.
const NAME_INDEX: &str = "api_access_tokens_owner_active_name_idx";
/// SQLSTATE of a unique violation.
const UNIQUE_VIOLATION: &str = "23505";

/// Maps an error from the token insert to the service error.
///
/// The owner lock and the name check make a duplicate name unreachable in
/// normal operation. A unique violation on [`NAME_INDEX`] can still occur when
/// a writer bypasses the lock, so it reports the same conflict as the name
/// check instead of an internal failure. Every other error stays internal.
fn insert_error(err: anyhow::Error) -> AccessTokenError {
    let name_taken = err
        .downcast_ref::<sqlx::Error>()
        .and_then(|err| err.as_database_error())
        .is_some_and(|db| {
            db.code().as_deref() == Some(UNIQUE_VIOLATION) && db.constraint() == Some(NAME_INDEX)
        });
    if name_taken {
        name_conflict()
    } else {
        AccessTokenError::Internal(err)
    }
}

/// Describes one token created or rotated by this module.
///
/// SECURITY: `secret` holds the only plaintext copy. The caller returns it in
/// the single create or rotate response and drops it.
#[derive(Debug)]
pub struct IssuedAccessToken {
    /// Metadata of the new token.
    pub metadata: AccessTokenDto,
    /// Plaintext token.
    pub secret: TokenSecret,
    /// Id of the revoked predecessor for a rotation.
    pub replaced_token_id: Option<Uuid>,
}

struct ScopeSpec {
    scope: &'static str,
    label: &'static str,
    default_selected: bool,
    requires_operator: bool,
    /// The owner must hold Admin to receive the scope and, at every call, to
    /// use it. See [`scope_requires_admin`].
    requires_admin: bool,
    released: bool,
}

/// Scope catalog, specification section 7. `released` is false for scopes that
/// no consumer supports yet; they stay unselectable until a later milestone.
///
/// The write scopes `scans:request`, `poams:write` and `poams:verify` are
/// released for Operator and Admin owners (TASK-534.5). `poams:close` stays
/// unreleased. Every scope that does not end in `:read` is a write scope; see
/// [`is_write_scope`].
///
/// The compliance scopes come from TASK-534.6. `policies:read` and
/// `bundles:read` are released for every role. The authoring scopes
/// `policies:draft`, `policies:import` and `bundles:draft` are Admin-only and
/// unreleased until TASK-534.7 and TASK-534.8; `compliance:preview` is open to
/// every role and unreleased until TASK-534.8.
const SCOPES: [ScopeSpec; 13] = [
    ScopeSpec {
        scope: "cves:read",
        label: "Read CVEs and systems",
        default_selected: true,
        requires_operator: false,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "poams:read",
        label: "Read POA&Ms",
        default_selected: true,
        requires_operator: false,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "provenance:read",
        label: "Read authorized provenance",
        default_selected: true,
        requires_operator: false,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "scans:request",
        label: "Request CVE scans",
        default_selected: false,
        requires_operator: true,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "poams:write",
        label: "Update POA&M progress",
        default_selected: false,
        requires_operator: true,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "poams:verify",
        label: "Verify POA&M evidence",
        default_selected: false,
        requires_operator: true,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "poams:close",
        label: "Close POA&Ms",
        default_selected: false,
        requires_operator: true,
        requires_admin: false,
        released: false,
    },
    ScopeSpec {
        scope: "policies:read",
        label: "Read security policies",
        default_selected: false,
        requires_operator: false,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "policies:draft",
        label: "Draft security policies",
        default_selected: false,
        requires_operator: false,
        requires_admin: true,
        released: false,
    },
    ScopeSpec {
        scope: "policies:import",
        label: "Import security policies",
        default_selected: false,
        requires_operator: false,
        requires_admin: true,
        released: false,
    },
    ScopeSpec {
        scope: "bundles:read",
        label: "Read compliance bundles",
        default_selected: false,
        requires_operator: false,
        requires_admin: false,
        released: true,
    },
    ScopeSpec {
        scope: "bundles:draft",
        label: "Draft compliance bundles",
        default_selected: false,
        requires_operator: false,
        requires_admin: true,
        released: false,
    },
    ScopeSpec {
        scope: "compliance:preview",
        label: "Preview compliance impact",
        default_selected: false,
        requires_operator: false,
        requires_admin: false,
        released: false,
    },
];

fn scope_spec(scope: &str) -> Option<&'static ScopeSpec> {
    SCOPES.iter().find(|spec| spec.scope == scope)
}

/// Reports whether a scope is usable only while the token owner holds Admin.
///
/// Issuing and rotation reject such a scope for a non-Admin owner. The MCP
/// dispatcher also calls this function on every tool call, so a token keeps
/// the stored scope after its owner loses Admin, but the scope grants nothing
/// until the owner is Admin again. Unknown scopes return `false`; they are
/// never granted.
pub fn scope_requires_admin(scope: &str) -> bool {
    scope_spec(scope).is_some_and(|spec| spec.requires_admin)
}

/// Reports whether a scope lets a token request a write. The web UI uses the
/// same `:read` suffix rule for its "includes write" review line.
fn is_write_scope(scope: &str) -> bool {
    !scope.ends_with(":read")
}

/// Field message when a write scope and a production environment are
/// combined without `confirm_production_write: true`.
const PRODUCTION_WRITE_MESSAGE: &str = "Confirm the production write access.";

/// Returns why the owner cannot select a scope now, or `None` when it can.
///
/// The role reasons come before "Not supported yet", so a non-Admin owner sees
/// "Requires Admin" for an unreleased Admin-only scope: releasing the scope
/// later does not make it available to that owner.
fn unavailable_reason(spec: &ScopeSpec, can_operate: bool, is_admin: bool) -> Option<&'static str> {
    if spec.requires_admin && !is_admin {
        Some("Requires Admin")
    } else if spec.requires_operator && !can_operate {
        Some("Requires Operator")
    } else if !spec.released {
        Some("Not supported yet")
    } else {
        None
    }
}

fn lifetime_options(default_days: u32, max_days: u32) -> Vec<u32> {
    let mut options: Vec<u32> = LIFETIME_CHOICES_DAYS
        .into_iter()
        .chain([default_days, max_days])
        .filter(|days| (1..=max_days).contains(days))
        .collect();
    options.sort_unstable();
    options.dedup();
    options
}

/// Decides whether an OIDC-owned account may hold or receive tokens now.
///
/// An owner is OIDC-owned when any `external_identities` row exists for the
/// user. Such an owner must have exactly one identity, with provider key
/// `oidc`, and passes [`require_fresh_entitlements`]. A local user without
/// identity rows is exempt. Returns `None` when eligible.
async fn oidc_block(
    pool: &PgPool,
    owner: Uuid,
    entitlement_max_age_seconds: u64,
) -> Result<Option<AccessTokenCreateBlockedDto>, AccessTokenError> {
    let identities = store::owner_identities(pool, owner).await?;
    oidc_block_for(pool, owner, &identities, entitlement_max_age_seconds).await
}

/// Applies the [`oidc_block`] rule to identities the caller already loaded.
async fn oidc_block_for(
    pool: &PgPool,
    owner: Uuid,
    identities: &[store::OwnerIdentity],
    entitlement_max_age_seconds: u64,
) -> Result<Option<AccessTokenCreateBlockedDto>, AccessTokenError> {
    let blocked = |code: &str, message: &str| {
        Some(AccessTokenCreateBlockedDto {
            code: code.to_owned(),
            message: message.to_owned(),
        })
    };
    if identities.is_empty() {
        return Ok(None);
    }
    let unresolved = "Your identity ownership is not resolved. Ask an administrator to \
                      classify your access.";
    let [identity] = identities else {
        return Ok(blocked("ownership_unresolved", unresolved));
    };
    if identity.provider_key != "oidc" || identity.entitlement_sync_state == "legacy_unresolved" {
        return Ok(blocked("ownership_unresolved", unresolved));
    }
    // SECURITY: Any failure, including a database error, denies. The check is
    // an eligibility gate, not part of the issuing transaction, so it uses its
    // own connection and cannot deadlock the issuing transaction's pool slot.
    match require_fresh_entitlements(pool, owner, identity.id, entitlement_max_age_seconds).await {
        Ok(()) => Ok(None),
        Err(_) => Ok(blocked(
            "entitlements_stale",
            "Your identity provider entitlements are out of date. Sign in again to refresh them.",
        )),
    }
}

async fn ensure_owner_eligible(
    pool: &PgPool,
    owner: Uuid,
    settings: &AccessTokenSettings,
) -> Result<(), AccessTokenError> {
    match oidc_block(pool, owner, settings.entitlement_max_age_seconds).await? {
        None => Ok(()),
        Some(blocked) => Err(conflict(
            &blocked.message,
            Some(json!({ "code": blocked.code })),
        )),
    }
}

fn ensure_enabled(settings: &AccessTokenSettings) -> Result<(), AccessTokenError> {
    if settings.enabled {
        Ok(())
    } else {
        Err(AccessTokenError::Disabled)
    }
}

/// Returns the owner's effective access-token policy.
///
/// The policy is readable when tokens are disabled; `enabled` is then `false`
/// and `create_blocked` stays `null`. `create_blocked` is set only for an
/// OIDC owner who fails the freshness or ownership rule. `oidc_owned` is
/// `true` when the owner has any `external_identities` row, even when tokens
/// are disabled; `entitlement_max_age_hours` is then the configured maximum
/// entitlement age in hours, rounded up, and is `None` otherwise.
///
/// # Errors
/// Returns [`AccessTokenError::OwnerUnavailable`] for a missing, disabled or
/// roleless owner, and [`AccessTokenError::Internal`] for database failures.
pub async fn policy(
    pool: &PgPool,
    owner: Uuid,
    settings: &AccessTokenSettings,
) -> Result<AccessTokenPolicyDto, AccessTokenError> {
    let access = store::owner_access(pool, owner)
        .await?
        .ok_or(AccessTokenError::OwnerUnavailable)?;
    if !access.is_active || !access.has_any_role() {
        return Err(AccessTokenError::OwnerUnavailable);
    }
    let can_operate = access.is_operator_or_admin();
    let environments = store::selectable_environments(pool, owner, access.is_admin())
        .await?
        .into_iter()
        .map(
            |(id, name, color, is_production)| AccessTokenPolicyEnvironmentDto {
                id,
                name,
                color,
                is_production,
            },
        )
        .collect();
    let scopes = SCOPES
        .iter()
        .map(|spec| {
            let reason = unavailable_reason(spec, can_operate, access.is_admin());
            AccessTokenPolicyScopeDto {
                scope: spec.scope.to_owned(),
                label: spec.label.to_owned(),
                available: reason.is_none(),
                default_selected: spec.default_selected && reason.is_none(),
                unavailable_reason: reason.map(str::to_owned),
            }
        })
        .collect();
    // The same identity rows decide `oidc_owned` and `create_blocked`, so one
    // lookup serves both, including when tokens are disabled.
    let identities = store::owner_identities(pool, owner).await?;
    let oidc_owned = !identities.is_empty();
    let create_blocked = if settings.enabled {
        oidc_block_for(
            pool,
            owner,
            &identities,
            settings.entitlement_max_age_seconds,
        )
        .await?
    } else {
        None
    };
    // Rounded up so the UI never states a window shorter than the real one.
    let entitlement_max_age_hours = oidc_owned.then(|| {
        u32::try_from(settings.entitlement_max_age_seconds.div_ceil(3600)).unwrap_or(u32::MAX)
    });
    Ok(AccessTokenPolicyDto {
        enabled: settings.enabled,
        // The URL is reported only while the endpoint is served, so the token
        // reveal never shows an endpoint that answers 404.
        mcp: AccessTokenMcpDto {
            enabled: settings.mcp_enabled,
            url: settings
                .mcp_public_url
                .clone()
                .filter(|_| settings.mcp_enabled),
        },
        audience: "mcp".to_owned(),
        default_lifetime_days: settings.default_lifetime_days,
        max_lifetime_days: settings.max_lifetime_days,
        lifetime_options_days: lifetime_options(
            settings.default_lifetime_days,
            settings.max_lifetime_days,
        ),
        max_active_tokens: MAX_ACTIVE_TOKENS as u32,
        environments,
        scopes,
        create_blocked,
        oidc_owned,
        entitlement_max_age_hours,
    })
}

struct IssueRequest {
    name: String,
    lifetime_days: u32,
    environment_ids: Vec<Uuid>,
    scopes: Vec<String>,
    /// `true` only when the request sent `confirm_production_write: true`.
    confirm_production_write: bool,
}

/// Validates request shape without touching the database.
///
/// Messages are fixed text and never echo request values.
fn validate_shape(
    request: &CreateAccessTokenRequest,
    settings: &AccessTokenSettings,
) -> Result<IssueRequest, BTreeMap<String, String>> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut fail = |field: &str, message: &str| {
        fields.insert(field.to_owned(), message.to_owned());
    };

    let name = match request.name.as_deref().map(str::trim) {
        None => {
            fail("name", "name is required");
            String::new()
        }
        Some(name) if name.is_empty() || name.chars().count() > MAX_NAME_CHARS => {
            fail("name", "name must be 1 to 100 characters");
            String::new()
        }
        Some(name) if name.chars().any(char::is_control) => {
            fail("name", "name must not contain control characters");
            String::new()
        }
        Some(name) => name.to_owned(),
    };

    match request.audience.as_deref() {
        Some("mcp") => {}
        None => fail("audience", "audience is required"),
        Some(_) => fail("audience", "audience must be \"mcp\""),
    }

    let lifetime_days = match request.expires_in_days {
        None => {
            fail("expires_in_days", "expires_in_days is required");
            0
        }
        Some(days) if days < 1 || days > i64::from(settings.max_lifetime_days) => {
            fail(
                "expires_in_days",
                &format!(
                    "expires_in_days must be between 1 and {}",
                    settings.max_lifetime_days
                ),
            );
            0
        }
        Some(days) => u32::try_from(days).unwrap_or(0),
    };

    let mut environment_ids = Vec::new();
    match request.environment_ids.as_deref() {
        None | Some([]) => fail("environment_ids", "select at least one environment"),
        Some(ids) if ids.len() > MAX_ENVIRONMENTS_PER_TOKEN => {
            fail("environment_ids", "select at most 50 environments")
        }
        Some(ids) => {
            let mut seen = HashSet::new();
            let mut problems = BTreeSet::new();
            for raw in ids {
                match Uuid::parse_str(raw) {
                    Ok(id) if seen.insert(id) => environment_ids.push(id),
                    Ok(_) => {
                        problems.insert("environment ids must not repeat");
                    }
                    Err(_) => {
                        problems.insert("environment ids must be UUIDs");
                    }
                }
            }
            if !problems.is_empty() {
                fail(
                    "environment_ids",
                    &problems.into_iter().collect::<Vec<_>>().join("; "),
                );
            }
        }
    }

    let mut scopes = Vec::new();
    match request.scopes.as_deref() {
        None | Some([]) => fail("scopes", "select at least one scope"),
        Some(requested) => {
            let mut seen = HashSet::new();
            let mut problems = BTreeSet::new();
            for scope in requested {
                if !seen.insert(scope.as_str()) {
                    problems.insert("scopes must not repeat");
                } else if scope_spec(scope).is_none() {
                    problems.insert("scopes must be known scope names");
                } else {
                    scopes.push(scope.clone());
                }
            }
            if !problems.is_empty() {
                fail(
                    "scopes",
                    &problems.into_iter().collect::<Vec<_>>().join("; "),
                );
            }
        }
    }

    if fields.is_empty() {
        Ok(IssueRequest {
            name,
            lifetime_days,
            environment_ids,
            scopes,
            confirm_production_write: request.confirm_production_write == Some(true),
        })
    } else {
        Err(fields)
    }
}

/// Checks a grant against the owner's current state and returns field
/// messages. An empty map means the grant is permitted.
///
/// `environments` holds `(id, name, is_active)` rows locked in the same
/// transaction. Unknown, inactive and unassigned environments share one
/// message so the response does not reveal which case applies.
fn check_grant(
    access: &OwnerAccess,
    scopes: &[String],
    environments: &[(Uuid, String, bool)],
    environment_ids: &[Uuid],
) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let can_operate = access.is_operator_or_admin();
    let scope_problems: Vec<String> = scopes
        .iter()
        .filter_map(|scope| match scope_spec(scope) {
            None => Some("unknown scope".to_owned()),
            Some(spec) => unavailable_reason(spec, can_operate, access.is_admin())
                .map(|reason| format!("{} is unavailable: {reason}", spec.scope)),
        })
        .collect();
    if !scope_problems.is_empty() {
        fields.insert("scopes".to_owned(), scope_problems.join("; "));
    }
    let unavailable: Vec<String> = environment_ids
        .iter()
        .filter(|id| {
            let active = environments
                .iter()
                .any(|(known, _, active)| known == *id && *active);
            let assigned = access.is_admin() || access.environment_ids.contains(id);
            !(active && assigned)
        })
        .map(|id| format!("{id} is not an active environment available to you"))
        .collect();
    if !unavailable.is_empty() {
        fields.insert("environment_ids".to_owned(), unavailable.join("; "));
    }
    fields
}

fn assemble(
    rows: Vec<store::TokenRow>,
    scopes: Vec<(Uuid, String)>,
    environments: Vec<store::TokenEnvironmentRow>,
) -> Vec<AccessTokenDto> {
    let mut scopes = store::group_scopes(scopes);
    rows.into_iter()
        .map(|row| {
            let mut granted = scopes.remove(&row.id).unwrap_or_default();
            granted.sort();
            AccessTokenDto {
                id: row.id,
                name: row.name,
                audience: row.audience,
                display_hint: row.display_hint,
                scopes: granted,
                environments: environments
                    .iter()
                    .filter(|env| env.token_id == row.id)
                    .map(|env| AccessTokenEnvironmentDto {
                        id: env.id,
                        name: env.name.clone(),
                        is_active: env.is_active,
                        color: env.color.clone(),
                    })
                    .collect(),
                created_at: row.created_at,
                expires_at: row.expires_at,
                revoked_at: row.revoked_at,
                last_used_at: row.last_used_at,
                status: row.status,
            }
        })
        .collect()
}

async fn load_dtos(
    pool: &PgPool,
    rows: Vec<store::TokenRow>,
) -> Result<Vec<AccessTokenDto>, AccessTokenError> {
    let ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
    let scopes = store::token_scopes(pool, &ids).await?;
    let environments = store::token_environments(pool, &ids).await?;
    Ok(assemble(rows, scopes, environments))
}

async fn load_dto_tx(
    tx: &mut Transaction<'_, Postgres>,
    owner: Uuid,
    token_id: Uuid,
) -> Result<AccessTokenDto, AccessTokenError> {
    let row = store::get_token(&mut **tx, owner, token_id)
        .await?
        .ok_or(AccessTokenError::NotFound)?;
    let scopes = store::token_scopes(&mut **tx, &[token_id]).await?;
    let environments = store::token_environments(&mut **tx, &[token_id]).await?;
    assemble(vec![row], scopes, environments)
        .into_iter()
        .next()
        .ok_or(AccessTokenError::NotFound)
}

/// Builds audit metadata from token metadata. It never contains the secret or
/// the hash because [`AccessTokenDto`] holds neither.
fn audit_metadata(token: &AccessTokenDto, extra: &[(&str, String)]) -> serde_json::Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("name".to_owned(), json!(token.name));
    metadata.insert("audience".to_owned(), json!(token.audience));
    metadata.insert("scopes".to_owned(), json!(token.scopes));
    metadata.insert(
        "environment_ids".to_owned(),
        json!(
            token
                .environments
                .iter()
                .map(|env| env.id.to_string())
                .collect::<Vec<_>>()
        ),
    );
    metadata.insert(
        "expires_at".to_owned(),
        json!(token.expires_at.to_rfc3339()),
    );
    for (key, value) in extra {
        metadata.insert((*key).to_owned(), json!(value));
    }
    serde_json::Value::Object(metadata)
}

/// Starts the issuing transaction: locks the owner and rechecks the account.
async fn begin_owner_tx(
    pool: &PgPool,
    owner: Uuid,
) -> Result<(Transaction<'static, Postgres>, OwnerAccess), AccessTokenError> {
    let mut tx = pool.begin().await?;
    match store::lock_owner_tx(&mut tx, owner).await? {
        Some(true) => {}
        _ => return Err(AccessTokenError::OwnerUnavailable),
    }
    // INVARIANT: Roles and memberships are read after the owner lock, so a
    // concurrent grant change either committed before this read or waits for
    // this transaction. Pre-fetched handler values are never used.
    let access = store::owner_access(&mut *tx, owner)
        .await?
        .ok_or(AccessTokenError::OwnerUnavailable)?;
    if !access.is_active || !access.has_any_role() {
        return Err(AccessTokenError::OwnerUnavailable);
    }
    Ok((tx, access))
}

async fn check_rate(
    tx: &mut Transaction<'_, Postgres>,
    owner: Uuid,
) -> Result<(), AccessTokenError> {
    if store::count_recent_issues_tx(tx, owner).await? >= MAX_ISSUES_PER_HOUR {
        return Err(AccessTokenError::RateLimited);
    }
    Ok(())
}

/// Creates a token for the owner.
///
/// Returns the metadata and the one-time plaintext. The request never narrows:
/// any invalid, unavailable, duplicate or unassigned item rejects the whole
/// request and creates nothing.
///
/// # Errors
/// - [`AccessTokenError::Disabled`] when tokens are disabled.
/// - [`AccessTokenError::Validation`] with field messages for invalid input,
///   including field `confirm_production_write` when the scopes include a
///   write scope, an environment has `is_production`, and the request did not
///   set `confirm_production_write: true`.
/// - [`AccessTokenError::Conflict`] for an OIDC eligibility block, the live
///   token limit or a duplicate non-revoked name (case-insensitive). A unique
///   violation on the name index at insert time reports the same name conflict.
/// - [`AccessTokenError::RateLimited`] after 10 issues in an hour.
/// - [`AccessTokenError::OwnerUnavailable`] for a disabled or roleless owner.
pub async fn create(
    pool: &PgPool,
    owner: Uuid,
    request: &CreateAccessTokenRequest,
    settings: &AccessTokenSettings,
) -> Result<IssuedAccessToken, AccessTokenError> {
    ensure_enabled(settings)?;
    let issue = validate_shape(request, settings).map_err(AccessTokenError::Validation)?;
    ensure_owner_eligible(pool, owner, settings).await?;

    // A return before commit drops the transaction, which rolls it back.
    let (mut tx, access) = begin_owner_tx(pool, owner).await?;
    let environments = store::lock_environments_tx(&mut tx, &issue.environment_ids).await?;
    let mut fields = check_grant(
        &access,
        &issue.scopes,
        &environments,
        &issue.environment_ids,
    );
    // SECURITY: A write scope in a production environment needs an explicit
    // owner confirmation. The production flags are read after the
    // environment share lock, so they cannot change before commit. Rotation
    // never repeats this check: it keeps the grant that the owner confirmed.
    let production_write = issue.scopes.iter().any(|scope| is_write_scope(scope))
        && store::any_production_environment_tx(&mut tx, &issue.environment_ids).await?;
    if production_write && !issue.confirm_production_write {
        fields.insert(
            "confirm_production_write".to_owned(),
            PRODUCTION_WRITE_MESSAGE.to_owned(),
        );
    }
    if !fields.is_empty() {
        return Err(AccessTokenError::Validation(fields));
    }
    check_rate(&mut tx, owner).await?;
    if store::count_live_tokens_tx(&mut tx, owner).await? >= MAX_ACTIVE_TOKENS {
        return Err(conflict(
            "The limit of 20 active access tokens is reached; revoke one first",
            None,
        ));
    }
    if store::name_in_use_tx(&mut tx, owner, &issue.name).await? {
        return Err(name_conflict());
    }

    let id = Uuid::new_v4();
    let generated = generate_access_token(id);
    store::insert_token_tx(
        &mut tx,
        id,
        owner,
        &issue.name,
        &generated.hash,
        &generated.display_hint,
        f64::from(issue.lifetime_days) * 86_400.0,
    )
    .await
    .map_err(insert_error)?;
    store::insert_grants_tx(&mut tx, id, &issue.scopes, &issue.environment_ids).await?;
    let metadata = load_dto_tx(&mut tx, owner, id).await?;
    // `production_write_confirmed` is `true` only when the grant combines a
    // write scope with a production environment, which requires the
    // confirmation. A confirmation sent for any other grant records `false`.
    let mut event = audit_metadata(&metadata, &[]);
    event["production_write_confirmed"] = json!(production_write);
    store::audit_access_token_tx(&mut tx, owner, "access_token_created", id, event).await?;
    tx.commit().await?;
    Ok(IssuedAccessToken {
        metadata,
        secret: generated.token,
        replaced_token_id: None,
    })
}

fn name_conflict() -> AccessTokenError {
    conflict(
        "An active access token with this name already exists",
        Some(json!({ "fields": { "name": "name is already used by an active token" } })),
    )
}

/// Rotates an active token into a replacement with the same grant.
///
/// The transaction locks the old token row, rechecks that it is neither
/// revoked nor expired, revalidates the same grant against the owner's current
/// role and memberships, revokes the old token, inserts the replacement and
/// writes one `access_token_rotated` audit event. The replacement keeps the
/// name, audience, scopes and environments and expires after the original
/// lifetime, clamped to the current maximum. Write scopes are revalidated
/// against the owner's current role, but the production write confirmation is
/// not requested again: the replacement keeps the confirmed grant.
///
/// # Errors
/// - [`AccessTokenError::NotFound`] for a missing token or another user's token.
/// - [`AccessTokenError::Conflict`] when the token is revoked or expired, it has
///   no remaining environments (for example after the environments were
///   deleted), the grant is no longer permitted, or OIDC eligibility fails.
///   Nothing changes.
/// - [`AccessTokenError::Disabled`], [`AccessTokenError::RateLimited`] and
///   [`AccessTokenError::OwnerUnavailable`] as for [`create`].
pub async fn rotate(
    pool: &PgPool,
    owner: Uuid,
    token_id: Uuid,
    settings: &AccessTokenSettings,
) -> Result<IssuedAccessToken, AccessTokenError> {
    ensure_enabled(settings)?;
    ensure_owner_eligible(pool, owner, settings).await?;

    let (mut tx, access) = begin_owner_tx(pool, owner).await?;
    // CONCURRENCY: The owner advisory lock already serializes rotations of one
    // owner. The row lock also orders rotation against a concurrent revoke,
    // which takes only this row lock.
    let old = store::lock_token_tx(&mut tx, owner, token_id)
        .await?
        .ok_or(AccessTokenError::NotFound)?;
    if old.revoked_at.is_some() || old.expired {
        return Err(conflict("Only an active access token can be rotated", None));
    }
    let scopes = store::token_scope_names(&mut *tx, token_id).await?;
    let environment_ids = store::token_environment_ids(&mut *tx, token_id).await?;
    // INVARIANT: A token grants at least one environment. Deleting environment
    // rows cascades to the junction table and can leave none. Rotating such a
    // token would issue a credential with an empty grant, so it conflicts.
    if environment_ids.is_empty() {
        return Err(conflict(
            "The token has no remaining environments; create a new token",
            None,
        ));
    }
    let environments = store::lock_environments_tx(&mut tx, &environment_ids).await?;
    let fields = check_grant(&access, &scopes, &environments, &environment_ids);
    if !fields.is_empty() {
        return Err(conflict(
            "The token grant is no longer permitted for its owner; create a new token instead",
            Some(json!({ "fields": fields })),
        ));
    }
    check_rate(&mut tx, owner).await?;

    let lifetime = (old.expires_at - old.created_at)
        .min(Duration::days(i64::from(settings.max_lifetime_days)));
    let lifetime_seconds = lifetime.num_milliseconds() as f64 / 1000.0;

    // The unique name index covers non-revoked rows only, so revoke first.
    store::revoke_token_tx(&mut tx, token_id, owner).await?;
    let id = Uuid::new_v4();
    let generated = generate_access_token(id);
    store::insert_token_tx(
        &mut tx,
        id,
        owner,
        &old.name,
        &generated.hash,
        &generated.display_hint,
        lifetime_seconds,
    )
    .await
    .map_err(insert_error)?;
    store::insert_grants_tx(&mut tx, id, &scopes, &environment_ids).await?;
    let metadata = load_dto_tx(&mut tx, owner, id).await?;
    store::audit_access_token_tx(
        &mut tx,
        owner,
        "access_token_rotated",
        id,
        audit_metadata(
            &metadata,
            &[
                ("replaced_token_id", token_id.simple().to_string()),
                ("replacement_token_id", id.simple().to_string()),
            ],
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(IssuedAccessToken {
        metadata,
        secret: generated.token,
        replaced_token_id: Some(token_id),
    })
}

/// Revokes one of the owner's tokens. Repeating the call succeeds.
///
/// Only the first call that changes the token writes an
/// `access_token_revoked` audit event. A token that a rotation already revoked
/// is a no-op here. Revoking an expired token is allowed. Revocation works
/// when tokens are disabled so an owner can always disable a credential.
///
/// CONCURRENCY: The transaction locks the owner row `FOR SHARE` before the
/// token row, the same order as [`create`] and [`rotate`]. It does not take the
/// owner advisory lock, because revoke needs no serialization with issuing
/// beyond the token row lock. The user row first order avoids a deadlock with
/// a concurrent user deletion, which locks the user row and then cascades to
/// the token rows.
///
/// # Errors
/// [`AccessTokenError::NotFound`] for a missing token or another user's token.
pub async fn revoke(
    pool: &PgPool,
    owner: Uuid,
    token_id: Uuid,
) -> Result<AccessTokenDto, AccessTokenError> {
    let mut tx = pool.begin().await?;
    // A missing owner row means the user is gone, so the token is gone too.
    store::lock_owner_row_tx(&mut tx, owner)
        .await?
        .ok_or(AccessTokenError::NotFound)?;
    let locked = store::lock_token_tx(&mut tx, owner, token_id)
        .await?
        .ok_or(AccessTokenError::NotFound)?;
    let transitioned =
        locked.revoked_at.is_none() && store::revoke_token_tx(&mut tx, token_id, owner).await?;
    let metadata = load_dto_tx(&mut tx, owner, token_id).await?;
    if transitioned {
        store::audit_access_token_tx(
            &mut tx,
            owner,
            "access_token_revoked",
            token_id,
            audit_metadata(&metadata, &[]),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(metadata)
}

/// Selects a page of the owner's tokens.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    /// `all`, `active` (not revoked and not expired, so it includes tokens
    /// whose derived status is `expiring_soon`), `expired` (not revoked) or
    /// `revoked`. Absent or empty means `all`.
    pub status: Option<String>,
    /// Case-insensitive name substring, at most 100 characters.
    pub q: Option<String>,
    /// Page size; default 25, clamped to 1..=100.
    pub limit: Option<i64>,
    /// Zero-based offset; negative values clamp to 0.
    pub offset: Option<i64>,
}

/// Lists the owner's tokens, newest first.
///
/// This read-only call works when tokens are disabled, so an owner can still
/// see which tokens to revoke.
///
/// # Errors
/// [`AccessTokenError::Validation`] for an unknown `status`, or for a `q` that
/// is longer than 100 characters or contains a control character (including
/// NUL, which the database rejects).
pub async fn list(
    pool: &PgPool,
    owner: Uuid,
    query: &ListQuery,
) -> Result<AccessTokenListDto, AccessTokenError> {
    let lifecycle = match query.status.as_deref().map(str::trim) {
        None | Some("") | Some("all") => "all",
        Some("active") => "active",
        Some("expired") => "expired",
        Some("revoked") => "revoked",
        Some(_) => {
            return Err(AccessTokenError::Validation(BTreeMap::from([(
                "status".to_owned(),
                "status must be active, expired or revoked".to_owned(),
            )])));
        }
    };
    let name_query = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    if name_query.is_some_and(|q| q.chars().count() > MAX_NAME_QUERY_CHARS) {
        return Err(AccessTokenError::Validation(BTreeMap::from([(
            "q".to_owned(),
            "q must be at most 100 characters".to_owned(),
        )])));
    }
    // SECURITY: A NUL byte makes PostgreSQL fail the query, which would be a
    // 500 that any session can trigger. Names reject control characters too,
    // so no stored name can match such a query.
    if name_query.is_some_and(|q| q.chars().any(char::is_control)) {
        return Err(AccessTokenError::Validation(BTreeMap::from([(
            "q".to_owned(),
            "q must not contain control characters".to_owned(),
        )])));
    }
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = query.offset.unwrap_or(0).max(0);

    // PERFORMANCE: One extra row tells whether another page exists without a
    // second count query.
    let mut rows =
        store::list_tokens(pool, owner, lifecycle, name_query, limit + 1, offset).await?;
    let has_more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    Ok(AccessTokenListDto {
        items: load_dtos(pool, rows).await?,
        limit,
        offset,
        has_more,
        next_offset: has_more.then_some(offset + limit),
    })
}

/// Returns one token with granted versus current effective access and recent
/// audit events.
///
/// `effective` compares the immutable grant with the owner's current state.
/// An environment is available when it is active and the owner is an Admin or
/// a member. A scope is permitted when the owner is active and the current
/// role allows it. A disabled owner has nothing available.
///
/// This read-only call works when tokens are disabled.
///
/// # Errors
/// [`AccessTokenError::NotFound`] for a missing token or another user's token.
pub async fn detail(
    pool: &PgPool,
    owner: Uuid,
    token_id: Uuid,
) -> Result<AccessTokenDetailDto, AccessTokenError> {
    let row = store::get_token(pool, owner, token_id)
        .await?
        .ok_or(AccessTokenError::NotFound)?;
    let metadata = load_dtos(pool, vec![row])
        .await?
        .into_iter()
        .next()
        .ok_or(AccessTokenError::NotFound)?;
    let access = store::owner_access(pool, owner)
        .await?
        .ok_or(AccessTokenError::NotFound)?;
    let can_operate = access.is_operator_or_admin();
    let owner_usable = access.is_active && access.has_any_role();
    let effective = AccessTokenEffectiveDto {
        environments: metadata
            .environments
            .iter()
            .map(|env| AccessTokenEffectiveEnvironmentDto {
                id: env.id,
                name: env.name.clone(),
                available: owner_usable
                    && env.is_active
                    && (access.is_admin() || access.environment_ids.contains(&env.id)),
            })
            .collect(),
        scopes: metadata
            .scopes
            .iter()
            .map(|scope| AccessTokenEffectiveScopeDto {
                scope: scope.clone(),
                permitted: owner_usable
                    && scope_spec(scope).is_some_and(|spec| {
                        (!spec.requires_operator || can_operate)
                            && (!spec.requires_admin || access.is_admin())
                    }),
            })
            .collect(),
    };
    let recent_events = store::recent_events(pool, token_id, RECENT_EVENT_LIMIT)
        .await?
        .into_iter()
        .map(|(action, created_at)| AccessTokenEventDto { action, created_at })
        .collect();
    Ok(AccessTokenDetailDto {
        metadata,
        effective,
        recent_events,
    })
}

/// Reports that a presented token was not admitted.
///
/// This is the only error [`validate_presented_token`] returns. It carries no
/// reason, so a caller cannot tell a malformed, unknown, wrong, expired,
/// revoked, suspended or ineligible token apart. Details go to server logs
/// only for database failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenRejected;

impl std::fmt::Display for TokenRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("access token rejected")
    }
}

impl std::error::Error for TokenRejected {}

/// Describes an admitted token and its owner's state at admission time.
#[derive(Debug, Clone)]
pub struct ValidatedToken {
    /// Owner user id.
    pub owner_user_id: Uuid,
    /// Immutable grant. Pass it as
    /// [`crate::auth::authorization::CredentialIdentity::Restricted`]; the M0
    /// boundary intersects it with the owner's current state.
    pub grant: RestrictedGrant,
    /// Whether the owner currently holds Admin. Admin never widens the grant.
    pub owner_is_admin: bool,
    /// The owner's current environment memberships.
    pub owner_environment_ids: Vec<Uuid>,
}

/// Admits a presented `cfat_v1_` token or rejects it without a reason.
///
/// Checks, in order: the kill switch (`settings.enabled`); strict bounded
/// parse; lookup by public id; constant-time comparison of the full SHA-256
/// (also run against a zero hash for an unknown id so lookup misses cost the
/// same); not revoked; `now` before `expires_at`; owner active with at least
/// one role; OIDC owners pass the freshness and ownership rule with
/// `settings.entitlement_max_age_seconds`; the token still has at least one
/// environment. Local users without identity rows are exempt from the OIDC
/// rule.
///
/// SECURITY: When `settings.enabled` is `false`, every token is rejected
/// before any database access, so `access_tokens_enable = false` stops
/// admission and not only issuing. A token whose environment set is empty is
/// rejected (fail closed), because an empty grant must never read as
/// unrestricted.
///
/// SECURITY: This is a point-in-time admission check made without locks. A
/// mutation MUST repeat the owner, token and grant checks inside its own
/// transaction (specification section 4.4). The returned grant is the token's
/// immutable limit; owner state in the result is informational. Callers pass
/// `Utc::now()`; the parameter exists so tests can pin time.
///
/// # Errors
/// Returns [`TokenRejected`] for every failure, including database errors.
pub async fn validate_presented_token(
    pool: &PgPool,
    presented: &str,
    now: DateTime<Utc>,
    settings: &AccessTokenSettings,
) -> Result<ValidatedToken, TokenRejected> {
    match admit(pool, presented, now, settings).await {
        Ok(Some(validated)) => Ok(validated),
        Ok(None) => Err(TokenRejected),
        Err(AccessTokenError::Internal(err)) => {
            // SECURITY: Log the error chain as text only. The Debug form can
            // carry a backtrace and driver detail that does not belong in logs.
            let detail = format!("{err:#}");
            tracing::error!(error = %detail, "access token validation failed closed");
            Err(TokenRejected)
        }
        Err(other) => {
            let kind = match &other {
                AccessTokenError::Disabled => "disabled",
                AccessTokenError::Validation(_) => "validation",
                AccessTokenError::NotFound => "not_found",
                AccessTokenError::Conflict { .. } => "conflict",
                AccessTokenError::RateLimited => "rate_limited",
                AccessTokenError::OwnerUnavailable => "owner_unavailable",
                AccessTokenError::Internal(_) => "internal",
            };
            tracing::error!(kind, "access token validation failed closed");
            Err(TokenRejected)
        }
    }
}

async fn admit(
    pool: &PgPool,
    presented: &str,
    now: DateTime<Utc>,
    settings: &AccessTokenSettings,
) -> Result<Option<ValidatedToken>, AccessTokenError> {
    if !settings.enabled {
        return Ok(None);
    }
    let entitlement_max_age_seconds = settings.entitlement_max_age_seconds;
    let Some(id) = parse_access_token(presented) else {
        return Ok(None);
    };
    let row = store::find_token_for_validation(pool, id).await?;
    let presented_hash = hash_access_token(presented);
    let stored: &[u8] = match &row {
        Some(row) => &row.secret_hash,
        None => &ZERO_HASH,
    };
    let hash_matches = hashes_equal(&presented_hash, stored);
    let Some(row) = row.filter(|_| hash_matches) else {
        return Ok(None);
    };
    if row.revoked_at.is_some() || now >= row.expires_at || !row.owner_active {
        return Ok(None);
    }
    if oidc_block(pool, row.user_id, entitlement_max_age_seconds)
        .await?
        .is_some()
    {
        return Ok(None);
    }
    let Some(access) = store::owner_access(pool, row.user_id).await? else {
        return Ok(None);
    };
    if !access.is_active || !access.has_any_role() {
        return Ok(None);
    }
    let scopes: BTreeSet<String> = store::token_scope_names(pool, id)
        .await?
        .into_iter()
        .collect();
    let environments: BTreeSet<Uuid> = store::token_environment_ids(pool, id)
        .await?
        .into_iter()
        .collect();
    if environments.is_empty() {
        return Ok(None);
    }
    Ok(Some(ValidatedToken {
        owner_user_id: row.user_id,
        grant: RestrictedGrant::new(id, Some(row.audience), scopes, environments, row.expires_at),
        owner_is_admin: access.is_admin(),
        owner_environment_ids: access.environment_ids,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> AccessTokenSettings {
        AccessTokenSettings {
            enabled: true,
            default_lifetime_days: 30,
            max_lifetime_days: 90,
            entitlement_max_age_seconds: 86_400,
            mcp_enabled: false,
            mcp_public_url: None,
        }
    }

    fn request() -> CreateAccessTokenRequest {
        CreateAccessTokenRequest {
            name: Some("OpenCode".into()),
            audience: Some("mcp".into()),
            expires_in_days: Some(30),
            environment_ids: Some(vec![Uuid::new_v4().to_string()]),
            scopes: Some(vec!["cves:read".into(), "poams:read".into()]),
            confirm_production_write: None,
        }
    }

    fn field(request: &CreateAccessTokenRequest, name: &str) -> Option<String> {
        validate_shape(request, &settings())
            .err()
            .and_then(|fields| fields.get(name).cloned())
    }

    #[test]
    fn valid_shape_is_accepted_and_name_is_trimmed() {
        let mut req = request();
        req.name = Some("  OpenCode  ".into());
        let issue = validate_shape(&req, &settings()).unwrap();
        assert_eq!(issue.name, "OpenCode");
        assert_eq!(issue.lifetime_days, 30);
        assert_eq!(issue.environment_ids.len(), 1);
    }

    #[test]
    fn shape_rejects_each_invalid_field_with_a_field_message() {
        let mut req = request();
        for name in [
            None,
            Some(String::new()),
            Some("   ".into()),
            Some("x".repeat(101)),
        ] {
            req.name = name;
            assert!(field(&req, "name").is_some());
        }
        req.name = Some("x".repeat(100));
        assert!(validate_shape(&req, &settings()).is_ok());
        req.name = Some("bad\u{7}name".into());
        assert!(field(&req, "name").is_some());
        req.name = Some("ok".into());

        for audience in [None, Some("cli".into()), Some("MCP".into())] {
            req.audience = audience;
            assert!(field(&req, "audience").is_some());
        }
        req.audience = Some("mcp".into());

        for days in [None, Some(0), Some(-1), Some(91), Some(i64::MAX)] {
            req.expires_in_days = days;
            assert!(field(&req, "expires_in_days").is_some(), "{days:?}");
        }
        req.expires_in_days = Some(90);
        assert!(validate_shape(&req, &settings()).is_ok());

        let id = Uuid::new_v4().to_string();
        for ids in [
            None,
            Some(vec![]),
            Some(vec!["not-a-uuid".to_owned()]),
            Some(vec![id.clone(), id.clone()]),
            Some(vec![id.clone(), id.to_uppercase()]),
            Some((0..51).map(|_| Uuid::new_v4().to_string()).collect()),
        ] {
            req.environment_ids = ids;
            assert!(field(&req, "environment_ids").is_some());
        }
        req.environment_ids = Some((0..50).map(|_| Uuid::new_v4().to_string()).collect());
        assert!(validate_shape(&req, &settings()).is_ok());
        req.environment_ids = Some(vec![id]);

        for scopes in [
            None,
            Some(vec![]),
            Some(vec!["admin".to_owned()]),
            Some(vec!["cves:read".to_owned(), "cves:read".to_owned()]),
        ] {
            req.scopes = scopes;
            assert!(field(&req, "scopes").is_some());
        }
    }

    #[test]
    fn shape_messages_do_not_echo_request_values() {
        let mut req = request();
        req.scopes = Some(vec!["leaky-scope-value".into()]);
        req.environment_ids = Some(vec!["leaky-environment-value".into()]);
        let fields = validate_shape(&req, &settings()).err().unwrap();
        let text = format!("{fields:?}");
        assert!(!text.contains("leaky"));
    }

    #[test]
    fn lifetime_options_are_sorted_bounded_and_include_default_and_max() {
        assert_eq!(lifetime_options(30, 90), vec![7, 30, 90]);
        assert_eq!(lifetime_options(14, 45), vec![7, 14, 30, 45]);
        assert_eq!(lifetime_options(1, 1), vec![1]);
        assert_eq!(lifetime_options(5, 5), vec![5]);
    }

    #[test]
    fn scope_availability_follows_role_and_release() {
        for read in ["cves:read", "policies:read", "bundles:read"] {
            let spec = scope_spec(read).unwrap();
            assert_eq!(unavailable_reason(spec, false, false), None, "{read}");
            assert!(!scope_requires_admin(read));
        }
        for write in ["scans:request", "poams:write", "poams:verify"] {
            let spec = scope_spec(write).unwrap();
            assert_eq!(
                unavailable_reason(spec, false, false),
                Some("Requires Operator")
            );
            assert_eq!(unavailable_reason(spec, true, false), None, "{write}");
            assert!(is_write_scope(write));
        }
        let close = scope_spec("poams:close").unwrap();
        assert_eq!(
            unavailable_reason(close, false, false),
            Some("Requires Operator")
        );
        assert_eq!(
            unavailable_reason(close, true, false),
            Some("Not supported yet")
        );
        // Admin-only scopes report the role first, then the release state.
        for authoring in ["policies:draft", "policies:import", "bundles:draft"] {
            let spec = scope_spec(authoring).unwrap();
            assert!(scope_requires_admin(authoring));
            assert_eq!(
                unavailable_reason(spec, false, false),
                Some("Requires Admin")
            );
            assert_eq!(
                unavailable_reason(spec, true, false),
                Some("Requires Admin")
            );
            assert_eq!(
                unavailable_reason(spec, true, true),
                Some("Not supported yet")
            );
        }
        let preview = scope_spec("compliance:preview").unwrap();
        assert_eq!(
            unavailable_reason(preview, false, false),
            Some("Not supported yet")
        );
        assert!(!scope_requires_admin("compliance:preview"));
        assert!(!scope_requires_admin("admin"));
        assert!(!is_write_scope("cves:read"));
        assert!(scope_spec("admin").is_none());
        assert_eq!(SCOPES.len(), 13);
    }

    #[test]
    fn production_confirmation_is_true_only_for_an_explicit_true() {
        for (sent, expected) in [(None, false), (Some(false), false), (Some(true), true)] {
            let mut req = request();
            req.confirm_production_write = sent;
            let issue = validate_shape(&req, &settings()).unwrap();
            assert_eq!(issue.confirm_production_write, expected, "{sent:?}");
        }
    }

    #[test]
    fn grant_check_hides_why_an_environment_is_unavailable() {
        let assigned = Uuid::new_v4();
        let other = Uuid::new_v4();
        let inactive = Uuid::new_v4();
        let operator = OwnerAccess {
            is_active: true,
            roles: vec![crate::models::auth_identity::AuthRole::Operator],
            environment_ids: vec![assigned, inactive],
        };
        let rows = vec![
            (assigned, "a".to_owned(), true),
            (other, "b".to_owned(), true),
            (inactive, "c".to_owned(), false),
        ];
        let scopes = vec!["cves:read".to_owned()];
        assert!(check_grant(&operator, &scopes, &rows, &[assigned]).is_empty());
        let fields = check_grant(
            &operator,
            &scopes,
            &rows,
            &[other, inactive, Uuid::new_v4()],
        );
        let message = fields.get("environment_ids").unwrap();
        assert_eq!(message.matches("not an active environment").count(), 3);

        let admin = OwnerAccess {
            is_active: true,
            roles: vec![crate::models::auth_identity::AuthRole::Admin],
            environment_ids: vec![],
        };
        assert!(check_grant(&admin, &scopes, &rows, &[other]).is_empty());
        assert!(check_grant(&admin, &scopes, &rows, &[inactive]).contains_key("environment_ids"));
        assert!(check_grant(&admin, &["scans:request".to_owned()], &rows, &[other]).is_empty());
        assert!(
            check_grant(&admin, &["poams:close".to_owned()], &rows, &[other])
                .contains_key("scopes")
        );
        assert!(check_grant(&admin, &["policies:read".to_owned()], &rows, &[other]).is_empty());
        let draft = vec!["policies:draft".to_owned()];
        assert!(
            check_grant(&operator, &draft, &rows, &[assigned])["scopes"]
                .contains("policies:draft is unavailable: Requires Admin")
        );
    }
}
