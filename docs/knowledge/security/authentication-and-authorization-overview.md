---
type: Security Model
title: "Authentication, authorization, and system registration"
description: "Explains how a system is registered with a public key, the OIDC and dev authentication modes, and the Viewer, Operator, and Admin role matrix with environment scoping; open it when changing login, roles, or registration."
tags:
  - crystal-forge
  - security
  - authentication
  - rbac
  - registration
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/00-system-overview.md at commit 3b23d36f"
    title: "Crystal Forge - System Overview"
---

# Authentication, Authorization, and System Registration

> **Status:** Partial. OIDC and development login routes and RBAC exist. This page records the current login entry points; see [API authentication and authorization](api-authentication-and-authorization.md) for the full API and RBAC contract.

## 1. Registering a System

1. Admin creates a system entry in CF (or via TOML config - TASK-142)
2. System receives a public key for authentication
3. System runs the Crystal Forge agent
4. Agent connects and sends heartbeat
5. System now appears as "online" in UI

**Key API:** `POST /systems` - Register new system

## 4. Authentication

CF supports two auth modes:

**OIDC:**
1. The user opens `/login` and follows its link to `GET /api/auth/oidc/login`.
2. The server redirects to the configured identity provider.
3. The provider returns the authorization code to `GET /api/auth/oidc/callback`.
4. The server exchanges the code, validates the identity, maps groups, and
   creates the session.

**Dev Mode (Development):**
1. Set `AUTH_MODE=dev` in configuration.
2. Open the `/login` UI and use the development login form.
3. Submit an existing development fixture user's email to
   `POST /api/auth/dev/login`.
4. The server authenticates the persisted fixture user and creates a session.

## 5. Authorization (RBAC)

Three roles with increasing permissions:

| Action | Viewer | Operator | Admin |
|--------|--------|----------|-------|
| View systems/flakes | ✅ | ✅ | ✅ |
| View deployments | ✅ | ✅ | ✅ |
| Deploy/Rollback | ❌ | ✅ | ✅ |
| Create/Edit flakes | ❌ | ✅ | ✅ |
| Register systems | ❌ | ✅ | ✅ |
| Manage users | ❌ | ❌ | ✅ |
| View audit log | ❌ | ❌ | ✅ |
| Manage OIDC mappings | ❌ | ❌ | ✅ |

**Environment Scoping:**
- Users can only see systems in their assigned environments
- Admin sees all environments

## Related concepts

- [System overview](../overview/system-overview.md) - product orientation
- [Server configuration reference](../operations/server-configuration-reference.md) - auth mode and secret configuration
