# Design Specification

* [Agent Access Tokens, MCP, and Remediation Context](agent-access-tokens-mcp-remediation-context.md) - Proposed CF-AGENT-ACCESS-001 v1.1 contract for Operator-owned tokens, immutable environment grants, MCP tools, OIDC freshness, evidence, provenance, and additive UI requirements; delivery TASK-529 and design handoff TASK-529.1; not implemented runtime behavior.

# Operator Guide

* [OIDC Role Mapping Configuration](oidc-role-mapping.md) - Configures database group mappings, managed role/environment replacement, Viewer fallback, independent local overrides and legacy-login limitations; current-source farm verification is pending.

# Security Model

* [Credential grant boundaries in POA&M services](credential-grant-groundwork.md) - Partial M0 implementation in TASK-529.2: immutable restricted grants survive read refresh without Admin wildcard expansion; source-aware OIDC groundwork exists, while PAT admission, token-row validation and acceptance evidence remain release gates.
* [OIDC entitlement ownership and freshness](oidc-entitlement-ownership.md) - Partial source implementation: local/IdP union, legacy classification, ordered callback reconciliation, failure fencing, future-PAT eligibility and one-time upgrade/recovery procedure; helper not wired to PAT admission and farm verification pending.

* [API authentication, sessions, and role-based authorization](api-authentication-and-authorization.md) - Explains cookie-session login, dev-mode login, the Viewer/Operator/Admin roles, authorization middleware, and environment scoping with non-disclosing not-found behavior; open it when changing who may call an API endpoint.
* [Auth Session Security Strategy](session-cookies-and-csrf.md) - Describes the server-authoritative browser session: cookie names and attributes, hashed token storage, session lifetime and logout invalidation, and the double-submit CSRF protection.
* [Authentication, authorization, and system registration](authentication-and-authorization-overview.md) - Open for managed enrollment versus keyless external/reference registration, login modes, and environment-scoped roles.
* [Builder credential boundary, key management, and audit logging](builder-credential-boundary-key-management-and-audit-logging.md) - Summarizes builder authentication, authorization, network boundary, and credential boundary rules, the builder key lifecycle (cf-keygen, registration, rotation), and the audit log events that the server and builder record.
* [Builder request authentication and data in transit](builder-request-authentication-and-data-in-transit.md) - Specifies the per-request Ed25519 signing protocol, replay protection, session scoping, the exact permissions of the builder private key, and the classification of every data flow between builder, server, Git, and cache.
* [Builder threat model](builder-threat-model.md) - Analyzes what an attacker obtains from a compromised builder, a malicious job claim, source archive tampering, and request replay, and which defenses (digest check, derivation_mismatch, timestamp window) apply.

# Testing Guide

* [TASK-65.7 Provider Compatibility and Security Validation](oidc-provider-compatibility-validation.md) - Lists the OIDC provider claim shapes covered by tests (Authentik, Keycloak, Entra, Okta, generic), the security regression coverage, and the residual risks of that validation.
