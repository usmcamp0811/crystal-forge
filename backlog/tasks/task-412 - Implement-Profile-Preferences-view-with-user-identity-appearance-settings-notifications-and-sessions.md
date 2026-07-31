---
id: TASK-412
title: >-
  Implement Profile & Preferences view with user identity, appearance settings,
  notifications, and sessions
status: Review
assignee: []
created_date: '2026-07-31 13:46'
updated_date: '2026-07-31 16:05'
labels:
  - web-ui
  - design-parity
  - user-preferences
  - settings
milestone: design-parity-missing-surfaces
dependencies: []
references:
  - 'https://github.com/DioxusLabs/dioxus'
  - docs/design/CrystalForge/styles.css
documentation:
  - docs/design/CrystalForge/components/ProfileView.jsx
  - packages/web-ui/src/state/theme.rs
  - packages/web-ui/src/state/auth.rs
  - packages/web-ui/src/routes.rs
modified_files:
  - packages/web-ui/src/components/layout/app_shell.rs
  - packages/web-ui/src/components/layout/mod.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - packages/web-ui/src/routes.rs
  - packages/web-ui/src/state/mod.rs
  - packages/web-ui/src/views/mod.rs
  - packages/web-ui/src/views/profile.rs
priority: high
type: feature
ordinal: 401000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Profile & Preferences view is missing from the Crystal Forge web UI. Existing appearance controls are distributed between the top bar and sidebar, while users have no central place to view the identity data supplied by the authentication context or to adjust shared UI settings.

## Desired Outcome

Provide a reachable `/profile` view matching the design where supported by the current client contract:
- Render only authenticated identity, role, and auth-source data; omit authorization and security attributes that the API does not provide.
- Centralize working appearance preferences (theme, density, sidebar mode, default Systems view) using the application’s shared state and canonical persistence keys.
- Provide reliable sign out behavior.
- Clearly identify notification delivery and session management as unavailable until their server-side integrations exist.

The view follows `docs/design/CrystalForge/components/ProfileView.jsx` for applicable layout and presentation without fabricating security or authorization data.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Route /profile exists inside AppShell and is reachable through the desktop and mobile sidebar user sections
- [ ] #2 Profile identity displays only values supplied by AuthContext; missing user name, email, role, or auth source is explicitly unavailable or omitted
- [ ] #3 Appearance controls update shared application state: theme uses the root UiTheme signal, sidebar uses SidebarContext, and density/default Systems view use PreferencesContext
- [ ] #4 Appearance changes persist through existing canonical keys and remain synchronized between the profile page and TopBar Tweaks
- [ ] #5 Successful sign out calls logout(), clears AppState.auth, marks auth fetch state Loaded, and replaces the route with LoginView; failure keeps the user on the page and displays an error
- [ ] #6 Notification preferences and active-session management are visibly unavailable until backend support exists; no non-functional controls claim to configure behavior
- [ ] #7 Access scope, organization, groups, MFA, last-login, and session data are omitted unless the API provides actual values
- [ ] #8 cargo fmt and cargo check --target wasm32-unknown-unknown pass
- [ ] #9 The Nix web-ui check completes successfully in CI or an equivalent environment; local timeout results are recorded without claiming success
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### 1. Extend state management for UI preferences
- Create `packages/web-ui/src/state/preferences.rs` for density, sidebar mode, default view persistence
- Follow the same localStorage pattern as `UiTheme` (load, apply, persist functions)
- Add enums for `Density`, `SidebarMode`, `DefaultView`, `NotificationChannel`
- Create notification preferences struct with individual toggle states

### 2. Create ProfileView component
- Create `packages/web-ui/src/views/profile.rs`
- Import existing components: Icon, chip classes, kv-grid pattern
- Build component hierarchy matching ProfileView.jsx:
  - Page header
  - Identity card with avatar, user info, action buttons
  - Two-column grid container
  - Appearance card with 4 PrefRow segmented controls
  - Notifications card with toggles and channel selector
  - Access summary card with kv-grid
  - Active sessions card with mock data

### 3. Implement reusable UI components
- `SegmentedControl` component for preference selectors
- `PrefRow` component for consistent preference layout
- `Toggle` component for notification switches

### 4. Wire up route
- Add ProfileView to routes.rs inside AppShell layout
- Add route title in Route::title() match
- Update views/mod.rs to export profile module

### 5. Connect state
- Read auth context from AppState for user data
- Initialize preference signals from localStorage on mount
- Hook up onChange handlers to persist and apply changes
- Ensure theme changes trigger state/theme.rs functions

### 6. Verification
- cargo fmt --all
- cargo clippy --all-targets -- -D warnings
- cargo test in packages/web-ui
- nix build .#checks.x86_64-linux.web-ui
- Manual testing: navigate to /profile, verify all controls work, check localStorage persistence
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/312

Current implementation:
- `/profile` is reachable from desktop and mobile sidebar user sections.
- Theme, density, sidebar mode, and default Systems view share application state with existing controls and use canonical storage keys.
- Identity, role, and authentication source are derived only from AuthContext; unavailable values are omitted or labelled unavailable.
- Sign out handles failure with an inline error and clears AppState authentication only after a successful logout.
- Notifications and active-session management are visibly unavailable pending backend support.
- SystemsListView now uses PreferencesContext for default Systems view and density; changes sync immediately with Profile and TopBar without polling.

Review fixes are in commits 0219c523, 9bcb1b00, 5bd52b31, 873e49f6, and 08b17b9b. The branch was rebuilt from origin/dev, so the MR contains only TASK-412 work.

Verification:
- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml` passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed, with pre-existing repository warnings.
- `nix build .#checks.x86_64-linux.web-ui --no-link` was started multiple times but did not complete locally: the Nix VM build exceeded the command timeout. Do not treat it as passed.
<!-- SECTION:NOTES:END -->
