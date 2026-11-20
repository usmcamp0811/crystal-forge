# Design Specification

* [Agent Access Tokens, MCP, and Remediation Context](agent-access-tokens-mcp-remediation-context.md) - CF-AGENT-ACCESS-001 v1.1 contract for Operator-owned tokens, environment grants, MCP tools, OIDC freshness and evidence; delivery TASK-534 and design handoff TASK-534.1; M0 groundwork is under review, PAT issuance (TASK-534.3) and the read-only MCP server (TASK-534.4) are implemented and unverified.

# Operator Guide

* [OIDC Role Mapping Configuration](oidc-role-mapping.md) - Configures database group mappings, managed role/environment replacement, Viewer fallback, independent local overrides and legacy-login limitations; see the ownership guide for checkpoint-specific farm evidence and remaining review/release gates.

# Security Model

* [Credential grant boundaries in POA&M services](credential-grant-groundwork.md) - Partial M0 implementation in TASK-534.2: immutable restricted grants survive read refresh without Admin wildcard expansion; source-aware OIDC groundwork exists, while PAT admission, token-row validation and acceptance evidence remain release gates.
* [Owner access token service](access-token-service.md) - Partial M1 owner slice in TASK-534.3: self-service `cfat_v1_` issue, list, inspect, rotate and revoke with hash-only storage, per-owner serialization, limits and audit; consumed by the MCP server (TASK-534.4); no admin tab or Connect agent shortcut yet; farm results are recorded in the task
* [MCP server](mcp-server.md) - Partial M2 implementation in TASK-534.4: opt-in read-only `POST /mcp` endpoint with static access-token bearer authentication, stateless Streamable HTTP, protocol versions, limits, Origin rule, tools and unsupported features; Admin status endpoint; unverified until task results are recorded
* [OIDC entitlement ownership and freshness](oidc-entitlement-ownership.md) - Local/IdP union, legacy classification, ordered reconciliation, failure fencing, future-PAT eligibility and upgrade/recovery; checkpoint database and Access tests passed, while PAT admission wiring and review-revision CI remain release gates.

* [API authentication, sessions, and role-based authorization](api-authentication-and-authorization.md) - Explains cookie-session login, dev-mode login, the Viewer/Operator/Admin roles, authorization middleware, and environment scoping with non-disclosing not-found behavior; open it when changing who may call an API endpoint.
* [Auth Session Security Strategy](session-cookies-and-csrf.md) - Describes the server-authoritative browser session: cookie names and attributes, hashed token storage, session lifetime and logout invalidation, and the double-submit CSRF protection.
* [Authentication, authorization, and system registration](authentication-and-authorization-overview.md) - Open for managed enrollment versus keyless external/reference registration, login modes, and environment-scoped roles.
* [Builder credential boundary, key management, and audit logging](builder-credential-boundary-key-management-and-audit-logging.md) - Summarizes builder authentication, authorization, network boundary, and credential boundary rules, the builder key lifecycle (cf-keygen, registration, rotation), and the audit log events that the server and builder record.
* [Builder request authentication and data in transit](builder-request-authentication-and-data-in-transit.md) - Specifies the per-request Ed25519 signing protocol, replay protection, session scoping, the exact permissions of the builder private key, and the classification of every data flow between builder, server, Git, and cache.
* [Builder threat model](builder-threat-model.md) - Analyzes what an attacker obtains from a compromised builder, a malicious job claim, source archive tampering, and request replay, and which defenses (digest check, derivation_mismatch, timestamp window) apply.

# Testing Guide

* [TASK-65.7 Provider Compatibility and Security Validation](oidc-provider-compatibility-validation.md) - Lists the OIDC provider claim shapes covered by tests (Authentik, Keycloak, Entra, Okta, generic), the security regression coverage, and the residual risks of that validation.
