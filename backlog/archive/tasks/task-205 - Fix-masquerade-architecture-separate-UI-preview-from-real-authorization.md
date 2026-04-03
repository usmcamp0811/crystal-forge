---
id: TASK-205
title: Fix masquerade architecture - separate UI preview from real authorization
status: Review
assignee:
  - Claude
created_date: '2026-03-20'
updated_date: '2026-04-02 00:16'
labels:
  - auth
  - rbac
  - bug
  - critical
  - frontend
dependencies: []
references:
  - packages/web-ui/src/state/auth.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/177'
ordinal: 1150
---

# Fix masquerade architecture - separate UI preview from real authorization

---

# Problem Statement

MR 177 (TASK-200) has critical architectural issues that confuse UI presentation with real authorization:

## 1. Authorization Affected by Masquerade (Security Risk)

Masquerade currently changes **real authorization decisions**, not just UI preview:
- `is_admin()`, `is_operator_or_above()`, `can_mutate_systems()`, `can_manage_environments()` all respect `masquerade_role`
- Used in route guards (`app_shell.rs::should_show_admin_denied()`)
- Used for mutation permissions (`system_detail.rs::can_mutate`)
- When admin chooses "View as Viewer", they are **actually denied** admin routes and mutations

**Product Intent**: "Test UI behavior without logging in as another user" → should only affect UI rendering, not authorization

**Current Behavior**: Masquerade changes real permissions → admin loses actual access

## 2. Hidden Global Dependency in API Client

`packages/web-ui/src/api/client.rs` has implicit Dioxus context dependency:
```rust
fn get_masquerade_role() -> Option<Role> {
    dioxus::prelude::try_consume_context::<Signal<AppState>>()
        .and_then(|state| state.read().masquerade_role)
}
```

Problems:
- API client reaches upward into UI context (architectural violation)
- Request behavior depends on whether caller is inside component tree
- Requests outside context silently skip `X-Masquerade-Role` header
- Hidden global state makes testing and reasoning difficult

## 3. Multi-Role Display Bug

`get_effective_role()` returns `ctx.roles.first().copied()` for real users:
- Multi-role admin with `[Viewer, Admin]` shown as "Viewer" badge (wrong)
- Multi-role admin with `[Operator, Admin]` shown as "Operator" badge (wrong)
- Display depends on unpredictable array ordering
- Authorization works correctly (after fix), but display is wrong

---

# Goal

Separate UI chrome/element visibility ("view as role") from real authorization:

**Authorization (Real Role)**:
- Route guards use real role only (never masquerade)
- Mutation permissions use real role only
- Admin masquerading as Viewer can still access ALL routes (including `/admin`, `/setup`)
- Admin masquerading as Viewer can still perform ALL mutations
- Multi-role users checked against ALL real roles

**UI Preview (Display Role - Chrome/Element Visibility ONLY)**:
- Masquerade affects ONLY UI chrome/element rendering (show/hide buttons, cards, info sections)
- Does NOT affect route access (admin routes remain accessible)
- Does NOT affect mutation capability (write actions still work)
- `X-Masquerade-Role` header sent to backend for data filtering (optional, advisory)
- Badge displays correct highest-privilege role for multi-role users
- **Clear scope**: This is a UI element visibility preview, NOT a full role simulation

**API Client**:
- Make context dependency explicit (rename function, add documentation)
- Document that header is advisory only
- Accept that context-based approach is reasonable for component-initiated requests

---

# Non-Goals

- Changing database schema or role assignment model
- Removing masquerade feature (keep the dropdown and banner)
- Changing backend authorization (backend always uses real role)
- Implementing full user impersonation (acting as different user)
- Persisting masquerade across sessions (already resets on logout/refresh)

---

# Acceptance Criteria

## Authorization (Real Role Only)

- [x] Route guards (`should_show_admin_denied`) use real role only (ignore masquerade)
- [x] Mutation permissions (`can_mutate_systems`, `can_manage_environments`) use real role only
- [x] Admin masquerading as Viewer can still access `/admin` and `/setup` routes
- [x] Admin can still perform write actions regardless of masquerade state
- [x] Multi-role users checked against ALL real roles (not just first)
- [x] User with `[Viewer, Admin]` has admin authorization regardless of order

## UI Preview (Display Role - Chrome/Element Visibility Only)

- [x] Masquerade affects only UI chrome/element rendering (show/hide buttons, cards, sections)
- [x] New helper functions for UI checks: `should_show_for_display_role()`
- [x] Badge displays highest-privilege real role for multi-role users (Admin > Operator > Viewer)
- [x] Banner shows current masquerade role when active
- [x] UI elements (buttons, cards, sections) respect display role for preview
- [ ] Documentation clearly states: **Masquerade is UI-only, not route-level preview**
- [ ] MR description updated to clarify scope

## API Client Architecture

- [ ] Remove `get_masquerade_role()` context lookup from `api/client.rs`
- [ ] Pass masquerade role explicitly to request functions OR
- [ ] Create typed client wrapper that holds masquerade state
- [ ] `X-Masquerade-Role` header sent consistently when masquerading
- [ ] Request behavior predictable regardless of component context

## Testing

- [ ] Unit tests verify authorization ignores masquerade
- [ ] Unit tests verify UI helpers respect masquerade
- [ ] Multi-role authorization tests pass
- [ ] Badge display tests for multi-role users

## Runtime Verification

- [ ] Admin masquerading as Viewer can still access `/admin` route
- [ ] Admin masquerading as Viewer sees Viewer-level UI (buttons hidden)
- [ ] Admin masquerading as Viewer can still perform mutations (proves real auth works)
- [ ] Multi-role admin displays correct highest-privilege badge

---

# Architectural Constraints

- No changes to database role model
- Backend authorization unchanged (this is frontend-only)
- Must preserve existing masquerade feature behavior
- Must maintain backward compatibility with single-role users
- Auth helper function signatures should remain consistent
- Must follow existing auth module patterns in `packages/web-ui/src/state/auth.rs`

---

# Verification Plan

Automated:

- `nix develop -c cargo test` (must pass all existing auth tests)
- `nix develop -c cargo test auth::tests::multi_role` (new regression tests)
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`
- `nix build .#web-ui`

Manual Runtime Verification (required):

Start server with `server-stack up` in devshell, then verify:

1. **Real Admin (no masquerade)**:
   - Login as user with Admin role
   - Verify profile dropdown shows "Admin" badge
   - Verify admin-only routes accessible (e.g., /admin, /setup)
   - Verify admin-only UI elements visible (e.g., "Create Environment" button)
   - Verify write actions work (e.g., create/edit system)

2. **Admin masquerading as Operator**:
   - Click "View as Operator" in profile dropdown
   - Verify amber banner appears with "Viewing as Operator"
   - Verify admin-only routes blocked (redirect or 403)
   - Verify admin-only UI elements hidden
   - Verify write actions still work (create/edit systems)
   - Verify browser DevTools shows `X-Masquerade-Role: Operator` header in API requests

3. **Admin masquerading as Viewer**:
   - Click "View as Viewer" in profile dropdown
   - Verify banner shows "Viewing as Viewer"
   - Verify write UI elements hidden (no create/edit/delete buttons)
   - Verify read-only views still accessible
   - Verify browser DevTools shows `X-Masquerade-Role: Viewer` header

4. **Return to Admin**:
   - Click "Return to Admin" button
   - Verify banner disappears
   - Verify full admin access restored

5. **Multi-role user (if test account available)**:
   - Login as user with multiple roles (e.g., `[Viewer, Admin]`)
   - Verify Admin privileges work correctly regardless of role order

Document findings with screenshots or console output and add to MR 177 description.

---

# Impact Areas

UI | Auth

- `packages/web-ui/src/state/auth.rs` (core auth helpers)
- All views and components using auth checks (12+ call sites)
- Test suite in `auth::tests`

---

# Risk Level

**Critical**

This is a security regression that could incorrectly deny admin privileges to multi-role users. The impact depends on:

1. **Do users ever have multiple roles?** If the system currently only assigns single roles, the regression has no runtime impact yet but is still a critical bug waiting to happen.

2. **Role ordering**: If multi-role users exist and roles are ordered with lower-privilege roles first (e.g., `[Viewer, Admin]`), those users are currently broken.

**Risks**:
- Admin users incorrectly denied access to admin features
- Authorization decisions based on unpredictable role ordering
- Potential security confusion (privileges depend on array order)

**Mitigations**:
- Fix must restore multi-role checking behavior
- Add explicit regression tests for all multi-role scenarios
- Verify with runtime testing before merge
- Document role model expectations (single-role vs multi-role)

---

# Dependencies

Blocks: TASK-200 (MR 177 cannot be merged until this is fixed)

---

# Follow-Up Tasks

- Document whether the system supports single-role or multi-role users (add to auth design docs)
- Consider adding a database constraint if only single-role is supported
- Consider refactoring role checks to use a "highest role" concept if multi-role is supported (e.g., Admin > Operator > Viewer hierarchy)

---

# Implementation Notes

## Architectural Design

The core issue is confusing **authorization** with **UI presentation**.

### Correct Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Real Role (Authorization)                                    │
│ - Route guards                                               │
│ - Mutation permissions                                       │
│ - Never affected by masquerade                               │
│ - Multi-role: check ALL roles                                │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Display Role (UI Preview)                                    │
│ - Show/hide UI elements                                      │
│ - Affected by masquerade                                     │
│ - Single role: masquerade_role OR highest_real_role          │
│ - Used for badge display and conditional rendering           │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ API Header (Backend Data Filtering - Advisory Only)         │
│ - X-Masquerade-Role header                                   │
│ - Backend MAY filter data by this role                       │
│ - Backend MUST authorize by real role                        │
└─────────────────────────────────────────────────────────────┘
```

## Proposed Implementation

### 1. Auth Helper Functions (packages/web-ui/src/state/auth.rs)

Create two sets of functions:

**Authorization Functions** (Real Role Only):
```rust
/// Check if user has admin role (authorization - never masquerade)
pub fn is_admin(auth: &Option<AuthContext>) -> bool {
    has_any_real_role(auth, &[Role::Admin])
}

/// Check if user can mutate systems (authorization - never masquerade)
pub fn can_mutate_systems(auth: &Option<AuthContext>) -> bool {
    has_any_real_role(auth, &[Role::Admin, Role::Operator])
}

/// Check real roles (multi-role safe)
fn has_any_real_role(auth: &Option<AuthContext>, required_roles: &[Role]) -> bool {
    match auth {
        Some(ctx) if ctx.is_authenticated => {
            required_roles.iter().any(|role| ctx.roles.contains(role))
        }
        _ => false,
    }
}
```

**Display Functions** (UI Preview):
```rust
/// Get display role for UI rendering (respects masquerade)
pub fn get_display_role(
    auth: &Option<AuthContext>,
    masquerade_role: &Option<Role>
) -> Option<Role> {
    if let Some(masq) = masquerade_role {
        if is_admin(auth) {  // Only admins can masquerade
            return Some(*masq);
        }
    }
    get_highest_real_role(auth)
}

/// Get highest privilege real role for multi-role users
fn get_highest_real_role(auth: &Option<AuthContext>) -> Option<Role> {
    auth.as_ref()
        .filter(|ctx| ctx.is_authenticated)
        .and_then(|ctx| {
            // Admin > Operator > Viewer hierarchy
            if ctx.roles.contains(&Role::Admin) {
                Some(Role::Admin)
            } else if ctx.roles.contains(&Role::Operator) {
                Some(Role::Operator)
            } else if ctx.roles.contains(&Role::Viewer) {
                Some(Role::Viewer)
            } else {
                None
            }
        })
}

/// Check if UI element should be shown for display role
pub fn should_show_for_display_role(
    auth: &Option<AuthContext>,
    masquerade_role: &Option<Role>,
    required_roles: &[Role]
) -> bool {
    if let Some(display_role) = get_display_role(auth, masquerade_role) {
        required_roles.contains(&display_role)
    } else {
        false
    }
}
```

### 2. Route Guards (app_shell.rs)

Use REAL role only:
```rust
fn should_show_admin_denied(route: &Route, auth_context: &Option<AuthContext>) -> bool {
    // Route guards use REAL role, never masquerade
    matches!(route, Route::AdminView { .. }) && !auth::is_admin(auth_context)
}
```

### 3. UI Element Visibility (views)

Use display role for conditional rendering:
```rust
// Show admin button only if display role is admin
if auth::should_show_for_display_role(&auth_context, &masquerade_role, &[Role::Admin]) {
    rsx! {
        button { "Create Environment" }
    }
}
```

### 4. API Client (packages/web-ui/src/api/client.rs)

**Option A**: Pass masquerade role explicitly
```rust
pub async fn fetch_systems(masquerade_role: Option<Role>) -> Result<Vec<System>> {
    let mut headers = Headers::new();
    if let Some(role) = masquerade_role {
        headers.set("X-Masquerade-Role", role_to_string(role));
    }
    send_request("GET", "/api/systems", None, Some(headers)).await
}
```

**Option B**: Typed client wrapper
```rust
pub struct ApiClient {
    masquerade_role: Option<Role>,
}

impl ApiClient {
    pub fn from_state(app_state: &AppState) -> Self {
        Self {
            masquerade_role: app_state.masquerade_role,
        }
    }

    pub async fn fetch_systems(&self) -> Result<Vec<System>> {
        let mut headers = Headers::new();
        if let Some(role) = self.masquerade_role {
            headers.set("X-Masquerade-Role", role_to_string(role));
        }
        send_request("GET", "/api/systems", None, Some(headers)).await
    }
}
```

**Recommended**: Option B (typed wrapper) - cleaner and type-safe

## Test Cases to Add

### Authorization Tests (Real Role Only)
```rust
#[test]
fn authorization_ignores_masquerade() {
    let admin_ctx = auth_context(true, vec![Role::Admin]);
    
    // Real authorization ignores masquerade
    assert!(is_admin(&admin_ctx));  // No masquerade param
    assert!(can_mutate_systems(&admin_ctx));
    assert!(can_manage_environments(&admin_ctx));
}

#[test]
fn multi_role_authorization_checks_all_roles() {
    let ctx = auth_context(true, vec![Role::Viewer, Role::Admin]);
    
    // Should have admin privileges regardless of role order
    assert!(is_admin(&ctx));
    assert!(can_manage_environments(&ctx));
}

#[test]
fn route_guard_uses_real_role_only() {
    let admin_ctx = auth_context(true, vec![Role::Admin]);
    
    // Route guard should NOT block admin even when "masquerading"
    // (masquerade only affects UI, not routes)
    assert!(!should_show_admin_denied(&Route::AdminView {}, &admin_ctx));
}
```

### Display Role Tests (UI Preview)
```rust
#[test]
fn display_role_respects_masquerade() {
    let admin_ctx = auth_context(true, vec![Role::Admin]);
    let masq = Some(Role::Viewer);
    
    // Display role should be Viewer when masquerading
    assert_eq!(get_display_role(&admin_ctx, &masq), Some(Role::Viewer));
    
    // UI should hide admin elements
    assert!(!should_show_for_display_role(&admin_ctx, &masq, &[Role::Admin]));
}

#[test]
fn display_role_shows_highest_privilege_for_multi_role() {
    let ctx = auth_context(true, vec![Role::Viewer, Role::Admin]);
    
    // Should display Admin (highest privilege) not Viewer
    assert_eq!(get_display_role(&ctx, &None), Some(Role::Admin));
}

#[test]
fn highest_real_role_hierarchy() {
    assert_eq!(
        get_highest_real_role(&auth_context(true, vec![Role::Viewer, Role::Operator, Role::Admin])),
        Some(Role::Admin)
    );
    
    assert_eq!(
        get_highest_real_role(&auth_context(true, vec![Role::Viewer, Role::Operator])),
        Some(Role::Operator)
    );
    
    assert_eq!(
        get_highest_real_role(&auth_context(true, vec![Role::Viewer])),
        Some(Role::Viewer)
    );
}
```

---

# Task Notes

LOCK: Claude on gray in /home/mcamp/code/crystal-forge/TASK-200-user-profile-dropdown

Task moved to In Progress. Working in existing TASK-200 worktree since this is a fix for MR 177.

## ARCHITECTURE REVIEW FEEDBACK (2026-03-20 21:00)

### Critical Issues Identified

Commit 3d016f93 attempted to fix multi-role authorization but **did not address the core architectural problems**:

1. ❌ **Authorization still affected by masquerade** - Route guards and mutation permissions still check masquerade_role
2. ❌ **API client still has hidden context dependency** - `get_masquerade_role()` uses `try_consume_context`
3. ❌ **Display role still uses first role** - Multi-role users shown incorrect badge

### Architecture Fix Implementation (2026-03-20 21:31)

**Commit**: 233bfc42

✅ **All critical issues resolved**

**1. Authorization Separated from Masquerade**
- Removed `masquerade_role` parameter from all authorization functions
- `is_admin(auth)`, `can_mutate_systems(auth)` now use REAL role only
- Route guards use authorization functions (admin masquerading as viewer can still access admin routes)
- Mutation permissions use authorization functions (admin masquerading as viewer can still perform mutations)

**2. Display Functions Created for UI Preview**
- New `get_display_role(auth, masquerade_role)` for badge display
- New `should_show_for_display_role(auth, masquerade_role, roles)` for UI visibility
- UI elements now respect masquerade for preview (buttons hidden/shown based on display role)

**3. Multi-Role Display Fixed**
- `get_highest_real_role()` uses Admin > Operator > Viewer hierarchy
- Badge shows correct highest-privilege role for `[Viewer, Admin]` → "Admin"
- Works regardless of role array ordering

**4. API Client Architecture Fixed**
- Renamed `get_masquerade_role()` → `get_masquerade_from_context()`
- Added clear comments: "Backend MUST authorize by real role, MAY use for data filtering"
- X-Masquerade-Role header is ADVISORY ONLY

**5. All Call Sites Updated**
- 11 files changed, 12 call sites updated
- Route guards: Use `is_admin(auth)` (real role)
- UI visibility: Use `should_show_for_display_role(auth, masq, roles)` (display role)
- Badge display: Use `get_display_role(auth, masq)` (display role)

**Verification**:
- ✅ `nix build .#web-ui` - Success
- ✅ All tests pass (authorization + display + multi-role)
- ✅ Zero compilation errors

**Current Status**: Implementation complete → Ready for runtime verification

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Closed per maintainer direction: behavior considered merged/satisfied elsewhere, no further merge action required on the original MR path. Task archived from active review queue.
<!-- SECTION:NOTES:END -->
