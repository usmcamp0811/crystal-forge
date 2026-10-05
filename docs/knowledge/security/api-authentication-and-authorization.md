---
type: Security Model
title: "API authentication, sessions, and role-based authorization"
description: "Explains cookie-session login, the authentication modes and routes, the Viewer/Operator/Admin roles, the two authorization patterns, and environment scoping with non-disclosing not-found behavior; open it when changing who may call an API endpoint."
tags:
  - crystal-forge
  - security
  - authentication
  - rbac
  - sessions
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
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
  - id: code-8
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/auth/extractors.rs at commit 3b23d36f"
    title: "Role guard extractors"
  - id: code-7
    resource: "Crystal Forge repository file modules/nixos/crystal-forge/default.nix at commit 3b23d36f"
    title: "default.nix implementation"
---

# API authentication, sessions, and role-based authorization

## Authentication

### How sessions work

1. A user logs in through one authentication mode (`oidc`, `local`, or `dev`).
2. The server creates a session row in `user_sessions`. It stores only the SHA-256 hash of the session token.
3. The server sets two cookies: `__Host-cf-session` (HTTP-only) and `__Host-cf-csrf` (for double-submit CSRF protection). See [Session cookies and CSRF](session-cookies-and-csrf.md).
4. Later requests carry the cookies. A state-changing request also carries the `x-csrf-token` header.
5. Each handler validates the session. It rejects an expired or invalidated session, requires the user to be active, and loads the user's role assignments.

### Authentication modes

The server reads the mode from the `auth_mode` setting (`AUTH_MODE`, default `oidc`). Only the routes of the active mode are registered.

| Mode | Use | Notes |
| --- | --- | --- |
| `oidc` | Production | OIDC authorization-code login. See [OIDC role mapping](oidc-role-mapping.md). |
| `local` | Production without an identity provider | Username and password. The first registered user becomes Admin. A later user has no role until an Admin assigns one. |
| `dev` | Local development only | Fixture users. A release build of the server refuses to start with `dev`. |

The NixOS option `services.crystal-forge.server.auth_mode` accepts only `local` (default) and `oidc`. The NixOS module cannot enable `dev` mode.

### Authentication routes

These routes are **not** under `/api/v1/`.

| Method | Endpoint | Mode | Description |
|--------|----------|------|-------------|
| GET | `/api/auth/whoami` | All | Public. Returns the current auth context. |
| GET | `/api/auth/setup-status` | All | Public. Returns `requires_setup`, `allow_registration`, `user_count`, and `auth_mode`. |
| POST | `/api/auth/logout` | All | Invalidates the session. Requires the CSRF header. Clears both cookies. |
| POST | `/api/auth/local/login` | `local` | Body `{"username": "...", "password": "..."}`. The `username` field accepts a username or an email. |
| POST | `/api/auth/local/register` | `local` | Body `{"username", "email", "password", "first_name"?, "last_name"?}`. |
| GET | `/api/auth/oidc/login` | `oidc` | Starts the OIDC redirect. |
| GET | `/api/auth/oidc/callback` | `oidc` | Completes login, assigns roles, and sets the session cookies. |
| POST | `/api/auth/dev/login` | `dev` | Body `{"email": "<dev fixture email>"}`. |

### Dev mode

Dev mode needs a debug build of the server and `AUTH_MODE=dev`. It uses three persisted fixture users that the server creates at start (`ensure_dev_users`):

- `dev-admin@crystal-forge.local` (Admin)
- `dev-operator@crystal-forge.local` (Operator)
- `dev-viewer@crystal-forge.local` (Viewer)

Log in with the fixture email, not with a role name:

```bash
curl -i -X POST http://localhost:3000/api/auth/dev/login \
  -H "Content-Type: application/json" \
  -d '{"email": "dev-admin@crystal-forge.local"}'
```

The response sets the session cookies and returns `user_id`, `email`, and `display_name`. Any other email returns HTTP 400 with `invalid_email`. Replace the port with your `server.port` value (the NixOS default is `3000`).

## Authorization (RBAC)

### Roles

| Role | What they can do |
|------|------------------|
| **Viewer** | Read-only access, limited to the environments in the user's memberships |
| **Operator** | Everything a Viewer can do, plus mutations of systems, flakes, builds, and queues. Limited to the user's environments for system access. |
| **Admin** | Everything an Operator can do, plus user management, the audit log, builders, environments, OIDC mappings, and the CVE dashboard. Not limited by environment. |

A user can hold several roles. The checks accept a user when **any** held role qualifies. The per-endpoint gates are in each API document, for example [Builders, queues, environments, dashboard, and admin APIs](../api/builders-queues-environments-dashboard-admin-api.md).

### How handlers enforce roles

The server uses two patterns. Both read the same session and the same role assignments.

**Pattern 1: guard extractors** (`auth/extractors.rs`). A handler lists an extractor as an argument. Axum runs the check before the handler body.

```rust
pub async fn create_flake(
    RequireOperator(_user): RequireOperator,
    State(pool): State<PgPool>,
    Json(payload): Json<CreateFlakeRequest>,
) -> impl IntoResponse {
    // Only an Operator or an Admin reaches this line.
}
```

- `RequireAuth`: any authenticated user.
- `RequireOperator`: Operator or Admin.
- `RequireAdmin`: Admin.

A missing or invalid session returns HTTP 401 `unauthorized`. A valid session without the role returns HTTP 403 `forbidden`.

**Pattern 2: rbac helpers** (`handlers/api/rbac.rs`). A handler calls a helper and returns early when it gets `None`.

```rust
let Some((user_id, roles)) = authenticated_user_roles(&pool, &headers).await else {
    return forbidden();
};
if !has_viewer_or_above_role(&roles) {
    return forbidden();
}
```

The helpers are `authenticated_user_roles`, `require_admin`, `require_operator_or_admin`, `require_viewer_or_above`, and the `has_*_role` functions. A missing, expired, or invalidated session also returns HTTP 403 `forbidden`. A handler that changes state also calls `require_csrf`.

New handlers SHOULD use a guard extractor when the endpoint needs only a role check. Use the helpers when the handler needs the roles for scoping, as the systems and dashboard handlers do.

### Environment scoping

A user sees **systems in the user's assigned environments**. The membership table is `user_environment_memberships` (migration `0073`).

`Role::can_access_system_environment` (`auth/models.rs`) applies the rule:

- Admin always passes.
- Every other role requires the system's environment ID to be in the user's memberships.
- A system with no environment is hidden from non-Admin users.

The rule applies to Operators as well as Viewers.

Snapshot APIs preserve non-disclosure. An unknown resource, a resource in a hidden environment, and a revision outside the resource's active source use the same not-found response. See [Flake outputs, system reconciliation, and count authority](../evaluation/flake-outputs-and-count-authority.md).

## Related concepts

- [Backend API overview, error codes, and WebSocket streaming](../api/api-overview-errors-and-streaming.md)
- [Systems API](../api/systems-api.md)
- [Builders, queues, environments, dashboard, and admin APIs](../api/builders-queues-environments-dashboard-admin-api.md)
- [OIDC role mapping](oidc-role-mapping.md)
- [Session cookies and CSRF](session-cookies-and-csrf.md)
