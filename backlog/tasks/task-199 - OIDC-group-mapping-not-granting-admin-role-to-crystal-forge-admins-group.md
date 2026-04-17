---
id: TASK-199
title: OIDC group mapping not granting admin role to crystal-forge-admins group
status: Done
assignee: []
created_date: '2026-03-20 13:40'
updated_date: '2026-03-31 01:56'
labels:
  - auth
  - oidc
  - rbac
  - bug
  - high-priority
dependencies: []
references:
  - packages/default/src/auth/dev_mode.rs
  - packages/default/src/config/oidc.rs
  - packages/default/src/handlers/api/auth_oidc.rs
priority: high
ordinal: 1000
---

# OIDC group mapping not granting admin role to crystal-forge-admins group

---

# Problem Statement

User authenticated via Authentik OIDC with confirmed membership in `crystal-forge-admins` group is not receiving Admin role in Crystal Forge. Admin configuration options are not visible in the UI after successful login.

Deployed instance: `/home/mcamp/code/campground` on reckless

---

# Goal

Users in the `crystal-forge-admins` Authentik group receive Admin role upon OIDC login and can access all admin configuration options in the Crystal Forge UI.

Additionally, improve OIDC debugging by adding structured logging to help diagnose authentication and role mapping issues.

---

# Non-Goals

- Changing the overall OIDC authentication flow
- Adding a full admin UI for managing OIDC group mappings (separate task)
- Supporting multiple OIDC providers simultaneously
- Implementing SAML or other auth methods

---

# Acceptance Criteria

- [ ] User in `crystal-forge-admins` Authentik group receives Admin role after OIDC login
- [ ] Admin UI elements (configuration, user management, etc.) are visible to admin users
- [ ] Bootstrap admin mapping is created on server startup if `CRYSTAL_FORGE_OIDC_BOOTSTRAP_ADMIN_GROUP` is set
- [ ] Server logs include structured debug output showing:
  - Groups extracted from OIDC ID token
  - Group-to-role mapping results
  - Final role assignment for user
- [ ] Error messages clearly indicate when:
  - No groups claim found in token
  - No matching group-to-role mapping
  - Role assignment fails
- [ ] Existing OIDC login flow continues to work for all roles

---

# Architectural Constraints

- Follow existing OIDC authentication patterns in `handlers/api/auth_oidc.rs`
- Use existing `OidcConfig::bootstrap_admin_group()` function
- Database changes must use migrations (no schema changes in code)
- Logging must use structured tracing (not println)
- No secrets in logs (never log tokens or secrets)

---

# Verification Plan

Automated:
- `nix develop -c cargo test auth::` (OIDC auth tests)
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo fmt -- --check`

Manual:
- Deploy to reckless instance
- Login via Authentik as user in `crystal-forge-admins` group
- Verify Admin role assigned (check `/api/auth/whoami`)
- Verify admin UI elements visible
- Check server logs for structured OIDC debug output
- Test with user NOT in admin group (should not get admin role)
- Restart server and verify bootstrap mapping is idempotent

---

# Impact Areas

API | Domain | Infrastructure

- OIDC authentication flow
- Role assignment logic
- Bootstrap configuration
- Logging infrastructure

---

# Risk Level

Medium

This affects authentication and authorization, which are security-critical. However, the change is additive (better logging) and fixes existing broken functionality. Risk is mitigated by:
- Testing on deployed instance before marking done
- No changes to core authentication flow
- Bootstrap mapping is idempotent

---

# Dependencies

None

---

# Follow-Up Tasks

- Add admin UI for managing OIDC group mappings (instead of environment variables only)
- Add integration tests for OIDC authentication flow
- Document OIDC troubleshooting procedures
