# Auth Session Security Strategy

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
