---
id: TASK-200
title: Add user profile dropdown with role display and admin role masquerade
status: In Progress
assignee:
  - Claude
created_date: '2026-03-20 13:40'
updated_date: '2026-03-20'
labels:
  - frontend
  - auth
  - rbac
  - ux
  - high-priority
dependencies: []
references:
  - packages/web-ui/src/components/header.rs
  - packages/web-ui/src/state/auth.rs
priority: high
ordinal: 1100
---

# Add user profile dropdown with role display and admin role masquerade

---

# Problem Statement

Users cannot see their current role assignment in the UI. Admins have no way to test the UI from different role perspectives (Operator, Viewer) without logging in as different users, making it difficult to verify RBAC implementation.

---

# Goal

Add a user profile dropdown in the top-right corner of the header that displays current user information and role. For Admin users only, provide a role masquerade feature to view and interact with the UI as if they had Operator or Viewer roles.

---

# Non-Goals

- Changing actual database role assignments via masquerade
- Persisting masquerade state across sessions
- Allowing non-Admins to masquerade
- Implementing impersonation (acting as a different user)
- Multi-role assignments (users have one role)

---

# Acceptance Criteria

- [ ] Top-right header contains user profile dropdown (click to open)
- [ ] Dropdown displays:
  - User email/name
  - Current role badge (Admin, Operator, or Viewer)
  - Logout button
- [ ] For Admin users only:
  - "View as" section in dropdown
  - Radio buttons or select for Operator/Viewer roles
  - "Return to Admin" button when masquerading
- [ ] When masquerading:
  - Visual indicator shown (banner or badge in header)
  - UI elements hidden/shown according to masquerade role
  - API calls include masquerade role in header (`X-Masquerade-Role`)
  - Backend enforces real Admin role for authorization
  - Backend may use masquerade role for data filtering/scoping
- [ ] Masquerade state:
  - Stored in component state only (not localStorage)
  - Reset on logout
  - Reset on page refresh (returns to real role)
- [ ] Logout button clears session and redirects to login
- [ ] Dropdown accessible via keyboard navigation
- [ ] Design matches existing UI theme and patterns

---

# Architectural Constraints

- No business logic in UI components
- Masquerade state managed in global AppState or auth context
- API header (`X-Masquerade-Role`) is optional and advisory only
- Backend MUST always check real user role for authorization
- Backend MAY use masquerade role for filtering data (e.g., show only Operator-visible items)
- No database changes required
- Follow existing header/navigation component patterns

---

# Verification Plan

Automated:
- `nix develop -c cargo test` (backend authorization tests)
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`
- UI build: `nix build .#web-ui`

Manual:
- Login as Admin user
- Verify dropdown shows correct user info and Admin role
- Select "View as Operator"
  - Verify visual indicator appears
  - Verify admin-only UI elements are hidden
  - Verify operator UI elements are visible
  - Check browser dev tools: API calls include `X-Masquerade-Role: Operator`
- Select "View as Viewer"
  - Verify viewer-level UI restrictions
  - Verify read-only behavior
- Click "Return to Admin"
  - Verify indicator removed
  - Verify admin UI elements visible again
- Refresh page while masquerading
  - Verify masquerade resets to real Admin role
- Logout and login as Operator
  - Verify no masquerade option shown
- Test keyboard navigation for dropdown

---

# Impact Areas

UI | API

- Header/navigation component
- Auth state management
- API client (add masquerade header)
- Backend: optional masquerade header parsing
- RBAC enforcement (must still use real role)

---

# Risk Level

Low

Masquerade is purely additive and only affects Admin users. Backend always enforces real role for authorization. The feature improves developer and admin UX without changing security model.

Risks:
- Potential confusion if masquerade indicator not clear
- Could accidentally test as wrong role if indicator missed

Mitigations:
- Clear, persistent visual indicator when masquerading
- Masquerade resets on page refresh (fail-safe to real role)
- Backend ignores masquerade for authorization

---

# Dependencies

None

---

# Follow-Up Tasks

- Add ability to masquerade as specific user (full impersonation) for admin debugging
- Add audit log for masquerade actions
- Persist masquerade in session cookie (optional, if desired later)

---

# Implementation Notes

LOCK: Claude on reckless in /home/mcamp/code/crystal-forge/TASK-200-user-profile-dropdown

Task moved to In Progress. Creating dedicated worktree.

## Implementation Summary

Implemented full admin role masquerade feature:

1. **State Management**: Added `masquerade_role: Option<Role>` to AppState
2. **Auth Helpers**: Created `get_effective_role()` and updated all auth helper functions to respect masquerade
3. **TopBar UI**: Added role selector dropdown for Admins, visual banner when masquerading, and current role badge
4. **API Integration**: Added `X-Masquerade-Role` HTTP header to all requests when masquerading
5. **View Updates**: Updated all 12 usages of auth helpers across views and components to pass masquerade_role
6. **Testing**: Added masquerade-specific unit tests to verify behavior

Commit: facc3366

## Manual Testing Required

Need to verify:
- Start server-stack and login as Admin
- Profile dropdown appears with role selector
- Selecting "View as Operator" shows banner and hides admin-only UI elements  
- Selecting "View as Viewer" further restricts UI
- "Return to Admin" restores full access
- Page refresh clears masquerade state
- API calls include X-Masquerade-Role header (check dev tools)
- Non-admin users don't see masquerade controls
