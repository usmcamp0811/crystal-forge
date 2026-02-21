---
id: m-14
title: "Identity and Access Management"
---

## Description

Implement end-to-end authentication and authorization for Crystal Forge across web UI and backend APIs using OIDC and role-based access control. This milestone includes a developer-friendly auth mode for local iteration and preserves machine-authenticated agent endpoints.

## Success Criteria

- OIDC login works with generic OIDC providers and is validated against Authentik, Keycloak, Entra, and Okta.
- Backend authorization is enforced with Admin/Operator/Viewer roles.
- UI route and action guards reflect role permissions.
- JIT user provisioning creates or updates users at first login.
- Devshell defaults to `AUTH_MODE=dev` with a local selector login screen.
- `AUTH_MODE=dev` is blocked in non-dev environments.
- All `/api/agent/**` endpoints remain accessible via existing key-auth flows and are not gated by user OIDC session auth.

## Tasks

- TASK-65: Feature: End-to-end OIDC authentication and multi-user RBAC
  - TASK-65.0: Developer auth mode with local selector screen
  - TASK-65.1: Identity and RBAC data model plus migrations
  - TASK-65.2: OIDC provider integration foundation
  - TASK-65.3: Server session and secure cookie lifecycle
  - TASK-65.4: JIT provisioning and claim-to-role mapping
  - TASK-65.5: API authorization enforcement and machine-auth carve-outs
  - TASK-65.6: Web UI auth UX and role-based guards
  - TASK-65.7: Provider compatibility and security validation
  - TASK-65.8: Operations documentation and runbooks for auth and RBAC

## Dependencies

- Requires stable UI and API baseline from m-3 and related backend API tasks.
