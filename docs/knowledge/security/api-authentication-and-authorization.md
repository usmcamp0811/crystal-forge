---
type: Security Model
title: "API authentication, sessions, and role-based authorization"
description: "Explains cookie-session login, dev-mode login, the Viewer/Operator/Admin roles, authorization middleware, and environment scoping with non-disclosing not-found behavior; open it when changing who may call an API endpoint."
tags:
  - crystal-forge
  - security
  - authentication
  - rbac
  - sessions
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/02-backend-api.md at commit 3b23d36f"
    title: "Backend API Specification"
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/bin/server.rs at commit 3b23d36f"
    title: "server.rs implementation"
  - id: code-2
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/rbac.rs at commit 3b23d36f"
    title: "rbac.rs implementation"
  - id: code-3
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/auth/models.rs at commit 3b23d36f"
    title: "models.rs implementation"
  - id: code-4
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/auth_dev.rs at commit 3b23d36f"
    title: "auth_dev.rs implementation"
  - id: code-5
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/handlers/api/auth_local.rs at commit 3b23d36f"
    title: "auth_local.rs implementation"
  - id: code-6
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/auth/dev_mode.rs at commit 3b23d36f"
    title: "dev_mode.rs implementation"
  - id: code-7
    resource: "Crystal Forge repository file modules/nixos/crystal-forge/default.nix at commit 3b23d36f"
    title: "default.nix implementation"
---

# API authentication, sessions, and role-based authorization

## Authentication

### How Sessions Work

1. User logs in (OIDC or Dev Mode)
2. Server creates session in database
3. Server sets `session_id` cookie in browser
4. Subsequent requests include the cookie
5. Middleware validates session

### Login Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/login` | Local email/password login |
| POST | `/auth/logout` | Clear session |
| GET | `/auth/status` | Get current user info |
| POST | `/dev/login` | Dev mode role selection |

### Dev Mode

For local development without OIDC:

```bash
# After setting AUTH_MODE=dev
curl -X POST http://localhost:8080/api/v1/dev/login \
  -H "Content-Type: application/json" \
  -d '{"role": "admin"}'
```

Returns a session cookie.

> **Status (documentation stale, corrected):** The login endpoint table and the dev-mode example above are the source text. The registered authentication routes are not under `/api/v1/`. They are `GET /api/auth/whoami` (public, returns the current auth context), `GET /api/auth/setup-status` (public; `requires_setup`, `allow_registration`, `user_count`, `auth_mode`), `POST /api/auth/logout`, `POST /api/auth/dev/login` (only registered when `auth_mode` is `dev`; the body is `{"email": "<dev fixture email>"}`, not `{"role": ...}`), `POST /api/auth/local/login` (body `{"username", "password"}`; the username field accepts a username or email), `POST /api/auth/local/register` (local mode only), and `GET /api/auth/oidc/login` plus `GET /api/auth/oidc/callback` (OIDC mode only). There is no `/auth/status` route; `/api/auth/whoami` and `/api/auth/setup-status` replace it. The server reads `auth_mode` from `AUTH_MODE` (default `oidc`) and supports `dev`, `local`, and `oidc`. A `dev` server refuses to start in a release build (`cfg(not(debug_assertions))` guard in `bin/server.rs`). The NixOS module option `services.crystal-forge.server.auth_mode` accepts only `local` (default) and `oidc`, so the module cannot enable dev mode. Dev mode uses three persisted fixture users (`dev-admin@`, `dev-operator@`, `dev-viewer@crystal-forge.local`) created by `ensure_dev_users`.

## Authorization (RBAC)

### Roles

| Role | What They Can Do |
|------|------------------|
| **Viewer** | Read-only access, limited to the environments in the user's memberships |
| **Operator** | Deploy, rollback, sync flakes, manage systems |
| **Admin** | All of above + user management, audit log |

### Authorization Middleware

> **Status (documentation stale, corrected):** The code sample below is the source's illustration. The server has no `Session` extractor or `user.role == "viewer"` field. Each handler calls the helpers in `handlers/api/rbac.rs` (`authenticated_user_roles`, `require_admin`, `require_operator_or_admin`, `require_viewer_or_above`, `has_admin_role`, ...). A user can hold several roles, and the helpers test whether any held role qualifies. The helpers read the `__Host-cf-session` cookie, hash the token, load the session row, reject an expired or invalidated session, require the user to be active, and then load the role assignments. A failed check returns `None`, and the handler returns 403 `forbidden`.

Every protected endpoint uses middleware to check permissions (original source illustration):

```rust
// Example: Operator or Admin only
async fn handler(
    State(state): State<AppState>,
    Session(user): Session,  // Gets current user from cookie
) -> Result<Json<...>, Error> {
    // Check role
    if user.role == "viewer" {
        return Err(Error::forbidden("Viewers cannot do this"));
    }
    // ... handler logic
}
```

### Environment Scoping

Users can only see **systems in their assigned environments**.

```sql
-- Query includes WHERE environment_id IN (user's environments)
SELECT * FROM systems 
WHERE environment_id IN (
  SELECT environment_id 
  FROM user_environment_memberships 
  WHERE user_id = ?
);
```

**Exception:** Admins can see all systems regardless of environment.

Snapshot APIs preserve non-disclosure. An unknown resource, a resource in a
hidden environment, and a revision outside the resource's active source use the
same not-found response. See [Flake outputs, system reconciliation, and count
authority](../evaluation/flake-outputs-and-count-authority.md).

> **Status:** The `user_environment_memberships` table exists (`migrations/0073_user_environment_memberships.sql`). The SQL above is the source illustration. The code applies the rule through `Role::can_access_system_environment` (`auth/models.rs`): Admin always passes; every other role requires the system's environment ID to be in the user's memberships, and a system with no environment is hidden from non-Admin users. This applies to Operators as well as Viewers.

## Related concepts

- [Backend API overview, error codes, and WebSocket streaming](../api/api-overview-errors-and-streaming.md)
- [Systems API](../api/systems-api.md)
- [Builders, queues, environments, dashboard, and admin APIs](../api/builders-queues-environments-dashboard-admin-api.md)

## Migration verification notes

Checked against commit `3b23d36f`: route registration in `bin/server.rs`, `handlers/api/rbac.rs`, `auth/models.rs`, `handlers/api/auth_{dev,local,status,whoami,session}.rs`, `auth/dev_mode.rs`, `config/server.rs`, and the NixOS `auth_mode` option. Not checked: OIDC callback internals (see [OIDC role mapping](oidc-role-mapping.md)). No `verified` field is set because the cited sample code is illustrative.

- Claim: login routes are `/auth/login`, `/auth/logout`, `/auth/status`, `/dev/login` under `/api/v1`.
  Finding: routes are `/api/auth/{whoami,setup-status,logout,dev/login,local/login,local/register,oidc/login,oidc/callback}`.
  Evidence: `bin/server.rs`.
  Case: documentation stale.
- Claim: dev login takes `{"role": "admin"}`.
  Finding: it takes `{"email": ...}` for a fixture user.
  Evidence: `handlers/api/auth_dev.rs` `DevLoginRequest`; `auth/dev_mode.rs`.
  Case: documentation stale.
- Claim: `AUTH_MODE=dev` configures dev mode.
  Finding: true for the server; release builds refuse it; the NixOS option excludes it.
  Evidence: `bin/server.rs` startup guard; `modules/nixos/crystal-forge/default.nix` `auth_mode` enum.
  Case: implemented (not a defect; module restriction is intentional).
- Claim: three roles; Viewer read-only, Operator mutates, Admin manages users and audit.
  Finding: matches `Role::can_mutate_systems`, `can_manage_environments`, `can_manage_admin_console`; some Admin-only actions are broader (builders, environments, CVE dashboard).
  Evidence: `auth/models.rs`; `handlers/api/builders.rs`.
  Case: implemented (partially; per-endpoint gates are owned by each API concept).
- Claim: environment scoping applies to users, Admin exempt.
  Finding: matches `Role::can_access_system_environment`.
  Evidence: `auth/models.rs`.
  Case: implemented.
- Claim: link to `../evaluation-flake-snapshots.md`.
  Finding: broken path after migration; retargeted to the evaluation concept.
  Evidence: bundle layout.
  Case: documentation stale.
