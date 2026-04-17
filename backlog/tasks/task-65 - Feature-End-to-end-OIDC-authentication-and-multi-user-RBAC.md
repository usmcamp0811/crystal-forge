---
id: TASK-65
title: 'Feature: End-to-end OIDC authentication and multi-user RBAC'
status: Done
assignee:
  - Claude Opus 4.5
created_date: ''
updated_date: '2026-03-13 01:24'
labels:
  - security
  - auth
  - oidc
  - rbac
  - ui
  - api
milestone: m-14
dependencies:
  - TASK-65.0
  - TASK-65.1
  - TASK-65.2
  - TASK-65.3
  - TASK-65.4
  - TASK-65.5
  - TASK-65.6
  - TASK-65.7
  - TASK-65.8
priority: high
ordinal: 71000
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

Selected for auth micro-sprint planning (2026-02-20).

Backlog structure fix: TASK-65 treated as epic umbrella; child tasks drive completion.
<!-- SECTION:NOTES:END -->
