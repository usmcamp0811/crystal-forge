---
type: Security Model
title: "Authentication, authorization, and system registration"
description: "Explains managed enrollment versus keyless configuration registration, OIDC and dev authentication, and the Viewer, Operator, and Admin environment-scoped role matrix."
tags:
  - crystal-forge
  - security
  - authentication
  - rbac
  - registration
implementation_status: partial
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

1. An Admin or environment-authorized Operator creates a resource through
   `POST /api/v1/systems` and chooses its management mode.
2. Managed enrollment requires a valid public key. The matching private key
   remains on the agent host; the agent signs state/heartbeat requests.
3. External/reference registration has no key and cannot authenticate as an
   agent. Configuration evidence does not grant deployment authority.
4. Reference identity is descriptive, not a running machine hostname. Parent
   associations apply only to external resources with an active managed parent
   in the same environment.
5. Registration, mode and parent edits recheck write authorization under locks
   and audit inside the transaction. Initial conversion to managed requires
   fresh credentials and manual policy and queues no deployment.

TOML `[[systems]]` remains managed enrollment configuration; its public-key
requirement is not the keyless REST registration contract. See
[management modes](../concepts/management-modes-and-configuration-assurance.md)
and [Systems API](../api/systems-api.md) for transitions and pending proof.

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
