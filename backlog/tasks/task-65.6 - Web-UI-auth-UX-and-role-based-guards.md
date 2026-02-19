---
id: TASK-65.6
title: Web UI auth UX and role-based guards
status: Backlog
assignee: ["KimiK2.5"]
labels:
  - ui
  - security
  - auth
  - rbac
milestone: m-14
dependencies:
  - TASK-65.0
  - TASK-65.5
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
The UI currently lacks authenticated navigation, protected routes, and role-aware actions.

Goal
Implement login and logout UX, protected routes, and role-aware action visibility and behavior.

Non-Goals
- Full IAM administration console in v1.
- UI-only authorization without backend enforcement.

Architectural Constraints
- UI composes reusable components; no policy logic duplication in views.
- Role checks consume backend-provided auth context.
- No infrastructure imports from UI layer.

Verification Plan
- `nix build .#checks.x86_64-linux.web-ui`
- `nix develop -c cargo test --package web-ui auth_ui`
- Manual: validate navigation and action behavior for Admin, Operator, and Viewer in both auth modes.

Impact Areas
- UI, API integration, Security UX

Risk Level
- Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Unauthenticated users are routed to login flow
- [ ] #2 Authenticated users see role-appropriate navigation and actions
- [ ] #3 Unauthorized actions and views show clear 403 or permission feedback
- [ ] #4 Dev selector mode and OIDC mode both pass through consistent UI guard logic
- [ ] #5 UI displays current session mode indicator where applicable
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: UX polish for auth states and empty states.
<!-- SECTION:NOTES:END -->
