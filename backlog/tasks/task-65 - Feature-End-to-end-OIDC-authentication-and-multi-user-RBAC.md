---
id: TASK-65
title: "Feature: End-to-end OIDC authentication and multi-user RBAC"
status: Backlog
assignee: ["Claude Opus 4.5"]
labels:
  - security
  - auth
  - oidc
  - rbac
  - ui
  - api
milestone: m-14
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Crystal Forge does not yet enforce user authentication and authorization for web UI and user-facing APIs, which blocks secure multi-user operation.

Goal
Deliver end-to-end authn and authz with OIDC SSO, server-managed sessions, and RBAC (Admin, Operator, Viewer), while preserving machine-authenticated agent endpoints.

Non-Goals
- Multi-tenant runtime support in v1.
- Local break-glass admin path in production.
- Replacing existing agent key-auth with OIDC.

Architectural Constraints
- No business logic in UI views.
- Authorization decisions must be enforced server-side.
- Session handling must use secure server-side session patterns.
- Agent machine-auth routes remain independent of user session auth.

Verification Plan
- Validate task decomposition and dependencies with `backlog task list --plain`.
- Confirm task set covers UI, API, Domain, Infrastructure, Database, and documentation scope.

Impact Areas
- UI, API, Domain, Infrastructure, Database, Documentation

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Auth roadmap is decomposed into executable tasks with clear dependencies
- [ ] #2 RBAC model and permission boundaries are explicitly defined for Admin, Operator, and Viewer
- [ ] #3 `/api/agent/**` machine-auth carve-out is explicitly documented and covered in downstream tests
- [ ] #4 Dev auth mode requirements are explicitly defined and scoped to development only
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up backlog item expected: future multi-tenant IAM design and migration strategy.
<!-- SECTION:NOTES:END -->
