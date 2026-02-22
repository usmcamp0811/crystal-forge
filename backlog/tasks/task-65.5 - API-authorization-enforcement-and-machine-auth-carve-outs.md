---
id: TASK-65.5
title: API authorization enforcement and machine-auth carve-outs
status: Done
assignee:
  - Claude Opus 4.5
created_date: ''
updated_date: '2026-02-21 17:15'
labels:
  - security
  - authz
  - rbac
  - api
milestone: m-14
dependencies:
  - TASK-65.3
  - TASK-65.4
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Without API-level authorization enforcement, UI-only guards are insufficient and insecure.

Goal
Enforce Admin, Operator, and Viewer authorization server-side for user APIs while preserving existing key-auth behavior for `/api/agent/**` endpoints.

Non-Goals
- Replacing machine key-auth with user OIDC.
- Endpoint behavior changes unrelated to authz policy.

Architectural Constraints
- Authorization policy lives in backend middleware or service layer.
- No business logic in UI views.
- Machine-auth and user-auth paths must remain explicit and separate.

Verification Plan
- `nix develop -c cargo test --package default auth::authorization_matrix`
- `nix develop -c cargo test --package default handlers::agent_auth_regression`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: validate role-specific API behavior and validate agent heartbeat without user session.

Impact Areas
- API, Domain, Infrastructure, Security

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 User API routes require valid user session and role checks
- [ ] #2 Permission matrix is implemented for Admin, Operator, and Viewer
- [ ] #3 All `/api/agent/**` routes remain accessible with existing key-auth
- [ ] #4 Regression tests verify heartbeat, state, and reporting paths remain operational
- [ ] #5 Denied actions return consistent authorization errors
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Role matrix baseline: Admin (full config plus user and role management), Operator (systems/build/deploy, no auth settings), Viewer (read-only).
<!-- SECTION:NOTES:END -->
