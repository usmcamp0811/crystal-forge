//! Temporary OIDC session storage for state/nonce/PKCE verifier.
//!
//! This is a temporary solution until proper session management is implemented (TASK-65.3).
//! Stores OIDC flow state in memory with TTL-based expiration.

use openidconnect::{CsrfToken, Nonce, PkceCodeVerifier};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// OIDC session data for OAuth2 flow validation.
///
/// Note: Does not implement Clone because PkceCodeVerifier cannot be cloned
/// (it contains secret material that should only be used once).
#[derive(Debug)]
pub struct OidcSession {
    pub csrf_token: CsrfToken,
    pub nonce: Nonce,
    pub pkce_verifier: PkceCodeVerifier,
    pub created_at: SystemTime,
}

impl OidcSession {
    pub fn new(csrf_token: CsrfToken, nonce: Nonce, pkce_verifier: PkceCodeVerifier) -> Self {
        Self {
            csrf_token,
            nonce,
            pkce_verifier,
            created_at: SystemTime::now(),
        }
    }

    /// Check if session has expired (default: 10 minutes).
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at
            .elapsed()
            .map(|elapsed| elapsed > ttl)
            .unwrap_or(true)
    }
}

/// In-memory store for OIDC session data.
///
/// **SECURITY NOTE**: This is a temporary solution. In production, use:
/// - Redis/Memcached for distributed deployments
/// - Encrypted cookies for single-server deployments
/// - Database-backed sessions for persistence
///
/// See TASK-65.3 for proper session management implementation.
#[derive(Debug, Clone)]
pub struct OidcSessionStore {
    sessions: Arc<RwLock<HashMap<String, OidcSession>>>,
    ttl: Duration,
}

impl OidcSessionStore {
    /// Create a new session store with the given TTL.
    ///
    /// Default TTL is 10 minutes (OAuth2 authorization code is short-lived).
    pub fn new(ttl: Option<Duration>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            ttl: ttl.unwrap_or(Duration::from_secs(600)), // 10 minutes
        }
    }

    /// Store OIDC session data keyed by state token.
    pub async fn store(&self, state: String, session: OidcSession) {
        let mut sessions = self.sessions.write().await;
        tracing::debug!("Storing OIDC session for state: {}", state);
        sessions.insert(state, session);
    }

    /// Retrieve and remove OIDC session data by state token.
    ///
    /// Returns `None` if:
    /// - State not found
    /// - Session expired
    ///
    /// **Note**: This is a consume operation - session is removed after retrieval.
    pub async fn retrieve(&self, state: &str) -> Option<OidcSession> {
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.remove(state) {
            if session.is_expired(self.ttl) {
                tracing::warn!("OIDC session expired for state: {}", state);
                return None;
            }
            tracing::debug!("Retrieved OIDC session for state: {}", state);
            return Some(session);
        }
        
        tracing::warn!("OIDC session not found for state: {}", state);
        None
    }

    /// Clean up expired sessions (should be called periodically).
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        let initial_count = sessions.len();
        
        sessions.retain(|_, session| !session.is_expired(self.ttl));
        
        let removed = initial_count - sessions.len();
        if removed > 0 {
            tracing::info!("Cleaned up {} expired OIDC sessions", removed);
        }
    }

    /// Get current session count (for monitoring).
    pub async fn count(&self) -> usize {
        self.sessions.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn store_and_retrieve_session() {
        let store = OidcSessionStore::new(None);
        let session = OidcSession::new(
            CsrfToken::new("csrf-token".to_string()),
            Nonce::new("nonce".to_string()),
            PkceCodeVerifier::new("verifier".to_string()),
        );

        store.store("state-123".to_string(), session).await;
        
        let retrieved = store.retrieve("state-123").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().csrf_token.secret(), "csrf-token");
    }

    #[tokio::test]
    async fn retrieve_removes_session() {
        let store = OidcSessionStore::new(None);
        let session = OidcSession::new(
            CsrfToken::new("csrf-token".to_string()),
            Nonce::new("nonce".to_string()),
            PkceCodeVerifier::new("verifier".to_string()),
        );

        store.store("state-123".to_string(), session).await;
        
        // First retrieval succeeds
        assert!(store.retrieve("state-123").await.is_some());
        
        // Second retrieval fails (session was removed)
        assert!(store.retrieve("state-123").await.is_none());
    }

    #[tokio::test]
    async fn expired_sessions_not_retrieved() {
        let store = OidcSessionStore::new(Some(Duration::from_millis(100)));
        let session = OidcSession::new(
            CsrfToken::new("csrf-token".to_string()),
            Nonce::new("nonce".to_string()),
            PkceCodeVerifier::new("verifier".to_string()),
        );

        store.store("state-123".to_string(), session).await;
        
        // Wait for expiration
        sleep(Duration::from_millis(150)).await;
        
        // Session should not be retrieved (expired)
        assert!(store.retrieve("state-123").await.is_none());
    }

    #[tokio::test]
    async fn cleanup_removes_expired() {
        let store = OidcSessionStore::new(Some(Duration::from_millis(100)));
        
        for i in 0..5 {
            let session = OidcSession::new(
                CsrfToken::new(format!("csrf-{}", i)),
                Nonce::new(format!("nonce-{}", i)),
                PkceCodeVerifier::new(format!("verifier-{}", i)),
            );
            store.store(format!("state-{}", i), session).await;
        }

        assert_eq!(store.count().await, 5);
        
        // Wait for expiration
        sleep(Duration::from_millis(150)).await;
        
        // Cleanup
        store.cleanup_expired().await;
        
        assert_eq!(store.count().await, 0);
    }
}
