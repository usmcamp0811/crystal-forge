//! OIDC provider integration.
//!
//! This module provides standards-compliant OIDC authentication compatible with:
//! - Authentik
//! - Keycloak
//! - Microsoft Entra (Azure AD)
//! - Okta
//! - Generic OIDC providers

pub mod claims;
pub mod discovery;
pub mod jwks;
pub mod jwt;
pub mod session_store;

pub use claims::*;
pub use discovery::*;
pub use jwks::*;
pub use jwt::*;
pub use session_store::*;
