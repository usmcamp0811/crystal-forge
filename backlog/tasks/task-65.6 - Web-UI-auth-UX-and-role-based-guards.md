---
id: TASK-65.6
title: Web UI auth UX and role-based guards
status: Done
assignee:
  - KimiK2.5
created_date: ''
updated_date: '2026-03-13 01:24'
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
ordinal: 57000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
<!-- SECTION:DESCRIPTION:BEGIN -->
# TASK: Implement Authenticated Navigation, Protected Routes, and Role-Aware UI

## Problem

The UI currently lacks authenticated navigation, protected routes, and role-aware action visibility. Users can access routes and UI actions without a structured authentication and authorization UX layer.

## Goal

Implement:

* Login and logout UX
* Authenticated navigation
* Protected routes
* Role-aware action visibility and behavior

The UI must reflect backend authorization state while relying on backend enforcement for security decisions.

---

## Non-Goals

* Full IAM administration console in v1
* UI-only authorization without backend enforcement
* Role editing or user management UI
* Session management dashboard
* Infrastructure or backend policy changes beyond required API consumption

---

## Architectural Constraints

* UI composes reusable components; no policy logic duplication inside views.
* Role checks must consume backend-provided auth context.
* No infrastructure imports from the UI layer.
* All policy decisions originate from backend role data.
* UI must gracefully handle 401 and 403 responses from backend.

---

## Auth Context Contract

UI must consume backend-provided auth context via a single source of truth.

Expected shape (example):

```json
{
  "is_authenticated": true,
  "user": {
    "id": "...",
    "email": "...",
    "display_name": "..."
  },
  "roles": ["Viewer", "Operator", "Admin"],
  "auth_mode": "oidc" | "local"
}
```

Requirements:

* Auth context fetched at app start.
* Refetched on 401 responses.
* No hard-coded role logic in route components.
* Centralized `can(role)` helper or equivalent.

---
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Unauthenticated users are routed to login flow
- [x] #2 Authenticated users see role-appropriate navigation and actions
- [x] #3 Unauthorized actions and views show clear 403 or permission feedback
- [x] #4 Dev selector mode and OIDC mode both pass through consistent UI guard logic
- [x] #5 UI displays current session mode indicator where applicable
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: UX polish for auth states and empty states.

LOCK: claude-sonnet-4.5 on gray in /home/mcamp/code/crystal-forge/TASK-65.6-web-ui-auth-ux

---

MR #126 created: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/126

Implementation complete. All acceptance criteria satisfied.

**What was delivered:**

Backend:
- /api/auth/whoami endpoint for auth context
- AuthContext, AuthMode, Role DTOs
- Dev login session establishment
- Local auth route registration
- Session validation helpers

Frontend:
- Unified LoginView adapting to auth mode
- Auth context in global state
- Auth helper functions for role checks
- Authentication guards in AppShell
- User menu with logout and role display
- Auth context auto-loaded on init

**Ready for review and manual testing.**

---

MR #126 merged into dev on 2026-02-21.
Task complete. LOCK released.
<!-- SECTION:NOTES:END -->
