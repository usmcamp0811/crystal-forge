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

> **Status:** Split from the system overview. OIDC and dev modes and the `AUTH_MODE` variable exist (`packages/default/crates/cf-server/src/config/server.rs`, `handlers/api/auth_oidc.rs`, `handlers/api/auth_dev.rs`). The role matrix and environment scoping have not been compared with `handlers/api/rbac.rs`. They are verification candidates.

## 1. Registering a System

1. Admin creates a system entry in CF (or via TOML config - TASK-142)
2. System receives a public key for authentication
3. System runs the Crystal Forge agent
4. Agent connects and sends heartbeat
5. System now appears as "online" in UI

**Key API:** `POST /systems` - Register new system

## 4. Authentication

CF supports two auth modes:

**OIDC (Production):**
1. User clicks "Login with Google/Okta/etc"
2. Redirects to Identity Provider
3. User authenticates
4. Callback with OIDC tokens
5. CF creates session, maps groups to roles

**Dev Mode (Development):**
1. Set `AUTH_MODE=dev` in config
2. Visit `/dev/login`
3. Click "Login as Admin/Operator/Viewer"
4. Dev user created in-memory

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
