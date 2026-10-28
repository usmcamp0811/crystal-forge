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

> **Status:** implemented. The five `provider_matrix_*` tests exist in `packages/default/crates/cf-server/src/auth/integration_matrix.rs` and the HS256 rejection test is in `auth/security_regression.rs`. The callback still assigns a default Viewer when a user has no roles (`handlers/api/auth_oidc.rs`), as the residual risks state.

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
- Role claim parsing failures (unexpected object shape) degrade to empty roles rather than unsafe escalation.
- OIDC unverified email denial maps to HTTP 403.
- Agent key-auth path remains stable:
  - accepts valid signed payloads;
  - rejects tampered payloads with `401 Unauthorized`.

## Residual Risks

- Provider matrix is claim-shape coverage, not a full live-provider end-to-end matrix for all providers in CI.
- OIDC callback/database/session flow still relies on broader integration tests for complete lifecycle validation.
- Role assignment policy is still evolving (default viewer assignment remains in callback path until role mapping task is fully completed).

## Related concepts

- [OIDC role mapping](oidc-role-mapping.md): the role mapping that consumes the extracted claims.
- [Session cookies and CSRF](session-cookies-and-csrf.md): session security that follows a validated login.
