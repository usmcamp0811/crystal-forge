---
type: Testing Guide
title: "TASK-65.7 Provider Compatibility and Security Validation"
description: "Lists the OIDC provider claim shapes covered by tests (Authentik, Keycloak, Entra, Okta, generic), the security regression coverage, and the residual risks of that validation."
tags:
  - crystal-forge
  - auth
  - oidc
  - testing
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/auth-provider-compatibility-validation.md at commit 3b23d36f"
    title: "TASK-65.7 Provider Compatibility and Security Validation"
---
# TASK-65.7 Provider Compatibility and Security Validation

The historical TASK-65.7 provider matrix and HS256 rejection tests exist in
`auth/integration_matrix.rs` and `auth/security_regression.rs`. Their claim-shape
coverage does not establish TASK-529.2 authoritative reconciliation or a passing
current-source farm result. The callback now uses the strict authoritative group
extractor and source-aware [OIDC entitlement reconciliation](oidc-entitlement-ownership.md).

## Validation Matrix

`rolesClaim` means "claim path interpreted as authorization roles" in Crystal Forge.
Some providers expose these as `groups` while others expose them as `roles`; both map
to the same internal role extraction path.

| Provider | Claim shape exercised | Test coverage |
| --- | --- | --- |
| Authentik | `groups` array | `auth::integration_matrix::provider_matrix_authentik_groups_claim` |
| Keycloak | `realm_access.roles` nested claim | `auth::integration_matrix::provider_matrix_keycloak_realm_access_roles_claim` |
| Microsoft Entra | `roles` array | `auth::integration_matrix::provider_matrix_entra_roles_claim` |
| Okta | `groups` array | `auth::integration_matrix::provider_matrix_okta_groups_claim` |
| Generic OIDC | comma-separated `roles` string | `auth::integration_matrix::provider_matrix_generic_oidc_comma_separated_roles` |

## Security Regression Coverage

- Token validation rejects non-RSA algorithms (`HS256`) in JWT validation path.
- The historical role-extraction path degrades unexpected shapes to empty roles.
  The managed callback now separately requires authoritative evidence: malformed
  or missing groups deny login, while valid empty groups produce managed Viewer
  without managed memberships. Unresolved legacy ownership preserves login
  without freshness, subject to identity, activity and generation checks.
- OIDC unverified email denial maps to HTTP 403.
- Agent key-auth path remains stable:
  - accepts valid signed payloads;
  - rejects tampered payloads with `401 Unauthorized`.

## Residual Risks

- Provider matrix is claim-shape coverage, not a full live-provider end-to-end matrix for all providers in CI.
- OIDC callback/database/session flow still relies on broader integration tests for complete lifecycle validation.
- Current-source farm verification of strict claim evidence, source ownership,
  failure fencing and callback/database lifecycle remains pending. Historical
  provider-shape coverage does not prove these contracts or future PAT admission.

## Related concepts

- [OIDC role mapping](oidc-role-mapping.md): the role mapping that consumes the extracted claims.
- [Session cookies and CSRF](session-cookies-and-csrf.md): session security that follows a validated login.
