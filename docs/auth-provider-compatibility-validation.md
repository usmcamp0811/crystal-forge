# TASK-65.7 Provider Compatibility and Security Validation

## Validation Matrix

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
