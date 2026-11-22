//! Preserves credential limits independently of refreshed owner entitlements.
//!
//! These nonsecret values are domain authorization inputs, not proof of bearer
//! authentication. No token storage, revocation validator, or MCP admission is
//! implemented here. Restricted POA&M mutations validate the token row in
//! their own transaction (`services::poam::restricted_progress` and
//! `services::poam::restricted_verify`); a grant alone never authorizes one.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Identifies the credential channel without an implicit browser fallback.
#[derive(Debug, Clone)]
pub enum CredentialIdentity {
    /// Represents an explicitly authenticated human browser session.
    BrowserSession,
    /// Carries immutable limits supplied by a trusted credential validator.
    Restricted(RestrictedGrant),
}

/// Stores nonsecret, immutable credential limits.
///
/// Fields are private so domain refresh cannot overwrite the original grant.
/// Constructing this value does not authenticate a token or check revocation.
#[derive(Debug, Clone)]
pub struct RestrictedGrant {
    token_id: Uuid,
    audience: Option<String>,
    scopes: BTreeSet<String>,
    environment_ids: BTreeSet<Uuid>,
    expires_at: DateTime<Utc>,
}

impl RestrictedGrant {
    /// Creates a nonsecret grant for trusted domain callers.
    ///
    /// Callers must independently authenticate the credential and owner. Missing
    /// required scopes, a missing or unsupported audience, and empty environment
    /// grants cannot authorize a resource read.
    /// This constructor MUST NOT be used as a bearer authentication validator.
    pub fn new(
        token_id: Uuid,
        audience: Option<String>,
        scopes: BTreeSet<String>,
        environment_ids: BTreeSet<Uuid>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            token_id,
            audience,
            scopes,
            environment_ids,
            expires_at,
        }
    }

    /// Returns the public token identifier, never a secret or hash.
    pub fn token_id(&self) -> Uuid {
        self.token_id
    }

    /// Reports whether the grant contains every scope in `required`.
    ///
    /// This checks scope names only, for example to filter an MCP tool list.
    /// It does not check audience, expiry, environments, or owner state, so it
    /// MUST NOT authorize a read by itself. Use
    /// [`CredentialIdentity::reading_environments`] for that decision.
    pub fn has_scopes(&self, required: &[&str]) -> bool {
        required.iter().all(|scope| self.scopes.contains(*scope))
    }
}

impl CredentialIdentity {
    /// Reports whether the caller explicitly authenticated a browser session.
    pub fn is_browser_session(&self) -> bool {
        matches!(self, Self::BrowserSession)
    }

    /// Intersects current owner environments with an unexpired MCP read grant.
    ///
    /// `required_scopes` is the exact set needed by the returned representation;
    /// every entry is required. Admin bypass applies only to owner memberships,
    /// never to credential environments. Browser behavior is unchanged.
    /// Returns `None` for a missing/wrong audience, expiry (including equality),
    /// or missing scope. The caller must separately validate the active owner,
    /// current roles, resource visibility, and credential admission/revocation.
    pub fn reading_environments(
        &self,
        owner_is_admin: bool,
        owner_environments: Vec<Uuid>,
        required_scopes: &[&str],
        now: DateTime<Utc>,
    ) -> Option<Vec<Uuid>> {
        match self {
            Self::BrowserSession => Some(owner_environments),
            Self::Restricted(grant) => {
                if grant.audience.as_deref() != Some("mcp")
                    || now >= grant.expires_at
                    || required_scopes
                        .iter()
                        .any(|scope| !grant.scopes.contains(*scope))
                {
                    return None;
                }
                Some(
                    grant
                        .environment_ids
                        .iter()
                        .copied()
                        .filter(|id| owner_is_admin || owner_environments.contains(id))
                        .collect(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_read_grant_intersects_current_owner_and_exact_requirements() {
        let staging = Uuid::new_v4();
        let production = Uuid::new_v4();
        let now = Utc::now();
        let grant = CredentialIdentity::Restricted(RestrictedGrant::new(
            Uuid::new_v4(),
            Some("mcp".into()),
            ["cves:read".into(), "poams:read".into()].into(),
            [staging].into(),
            now + chrono::Duration::hours(1),
        ));
        let scopes = &["cves:read", "poams:read"];
        assert_eq!(
            grant.reading_environments(false, vec![staging, production], scopes, now),
            Some(vec![staging])
        );
        assert_eq!(
            grant.reading_environments(true, vec![], scopes, now),
            Some(vec![staging])
        );
        assert_eq!(
            grant.reading_environments(false, vec![production], scopes, now),
            Some(vec![])
        );
        assert_eq!(
            grant.reading_environments(false, vec![], scopes, now),
            Some(vec![])
        );
        assert_eq!(
            grant.reading_environments(true, vec![], &["provenance:read"], now),
            None
        );
    }

    #[test]
    fn expiry_audience_and_each_required_scope_fail_closed() {
        let now = Utc::now();
        let environment = Uuid::new_v4();
        for (audience, scopes, expiry) in [
            (
                None,
                vec!["cves:read", "poams:read"],
                now + chrono::Duration::hours(1),
            ),
            (
                Some("cli"),
                vec!["cves:read", "poams:read"],
                now + chrono::Duration::hours(1),
            ),
            (
                Some("mcp"),
                vec!["cves:read"],
                now + chrono::Duration::hours(1),
            ),
            (
                Some("mcp"),
                vec!["poams:read"],
                now + chrono::Duration::hours(1),
            ),
            (Some("mcp"), vec!["cves:read", "poams:read"], now),
            (
                Some("mcp"),
                vec!["cves:read", "poams:read"],
                now - chrono::Duration::seconds(1),
            ),
        ] {
            let credential = CredentialIdentity::Restricted(RestrictedGrant::new(
                Uuid::new_v4(),
                audience.map(str::to_owned),
                scopes.into_iter().map(str::to_owned).collect(),
                [environment].into(),
                expiry,
            ));
            assert_eq!(
                credential.reading_environments(true, vec![], &["cves:read", "poams:read"], now),
                None
            );
        }
        let empty = CredentialIdentity::Restricted(RestrictedGrant::new(
            Uuid::new_v4(),
            Some("mcp".into()),
            ["cves:read".into(), "poams:read".into()].into(),
            BTreeSet::new(),
            now + chrono::Duration::hours(1),
        ));
        assert_eq!(
            empty.reading_environments(true, vec![environment], &["cves:read", "poams:read"], now),
            Some(vec![])
        );
    }
}
