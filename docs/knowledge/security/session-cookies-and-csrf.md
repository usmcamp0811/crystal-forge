---
type: Security Model
title: "Auth Session Security Strategy"
description: "Describes the server-authoritative browser session: cookie names and attributes, hashed token storage, session lifetime and logout invalidation, and the double-submit CSRF protection."
tags:
  - crystal-forge
  - auth
  - session
  - csrf
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/auth-session-security.md at commit 3b23d36f"
    title: "Auth Session Security Strategy"
---
# Auth Session Security Strategy

> **Status:** implemented. Checked against `packages/default/crates/cf-server/src/auth/session.rs` (cookie names, attributes, `x-csrf-token`) and `handlers/api/auth_session.rs` (8 hour default, `CRYSTAL_FORGE_SESSION_TTL_SECONDS`). The code also clamps the override to a range of 60 seconds to 30 days and falls back to the default for invalid values. The text below does not state that clamp.

Crystal Forge uses server-authoritative sessions for browser authentication.

## Session Cookie

- Cookie name: `__Host-cf-session`
- Attributes: `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`
- Value: random opaque token
- Server stores only `SHA-256` hash of token in `user_sessions`

## Session Lifecycle

- Created after successful OIDC callback or local username/password login
- TTL defaults to 8h and can be overridden with `CRYSTAL_FORGE_SESSION_TTL_SECONDS`
- Expiry is enforced by `expires_at` in `user_sessions`
- Logout invalidates the server-side session by setting `invalidated_at`

## CSRF Strategy

State-changing auth actions use double-submit CSRF protection:

- Cookie: `__Host-cf-csrf` (`Secure`, `SameSite=Strict`, `Path=/`)
- Header: `x-csrf-token`
- Request is rejected unless header value exactly matches cookie value

Logout is expected to be performed via XHR/fetch so the client can copy the CSRF cookie value into the `x-csrf-token` header.

`/api/auth/logout` requires CSRF validation and clears both auth cookies on success.

## Related concepts

- [OIDC role mapping](oidc-role-mapping.md): how an OIDC login becomes a local role before a session is created.
- [OIDC provider compatibility validation](oidc-provider-compatibility-validation.md): claim-shape and security regression coverage for OIDC login.
