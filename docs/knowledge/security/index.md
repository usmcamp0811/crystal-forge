# Operator Guide

* [OIDC Role Mapping Configuration](oidc-role-mapping.md) - Describes mapping OIDC groups to the Admin, Operator, and Viewer roles through environment variables, highest-privilege selection, safe-deny behavior, provider examples, and troubleshooting; read the status note before relying on it.

# Security Model

* [API authentication, sessions, and role-based authorization](api-authentication-and-authorization.md) - Explains cookie-session login, dev-mode login, the Viewer/Operator/Admin roles, authorization middleware, and environment scoping with non-disclosing not-found behavior; open it when changing who may call an API endpoint.
* [Auth Session Security Strategy](session-cookies-and-csrf.md) - Describes the server-authoritative browser session: cookie names and attributes, hashed token storage, session lifetime and logout invalidation, and the double-submit CSRF protection.
* [Authentication, authorization, and system registration](authentication-and-authorization-overview.md) - Explains how a system is registered with a public key, the OIDC and dev authentication modes, and the Viewer, Operator, and Admin role matrix with environment scoping; open it when changing login, roles, or registration.
* [Builder credential boundary, key management, and audit logging](builder-credential-boundary-key-management-and-audit-logging.md) - Summarizes builder authentication, authorization, network boundary, and credential boundary rules, the builder key lifecycle (cf-keygen, registration, rotation), and the audit log events that the server and builder record.
* [Builder request authentication and data in transit](builder-request-authentication-and-data-in-transit.md) - Specifies the per-request Ed25519 signing protocol, replay protection, session scoping, the exact permissions of the builder private key, and the classification of every data flow between builder, server, Git, and cache.
* [Builder threat model](builder-threat-model.md) - Analyzes what an attacker obtains from a compromised builder, a malicious job claim, source archive tampering, and request replay, and which defenses (digest check, derivation_mismatch, timestamp window) apply.

# Testing Guide

* [TASK-65.7 Provider Compatibility and Security Validation](oidc-provider-compatibility-validation.md) - Lists the OIDC provider claim shapes covered by tests (Authentik, Keycloak, Entra, Okta, generic), the security regression coverage, and the residual risks of that validation.
