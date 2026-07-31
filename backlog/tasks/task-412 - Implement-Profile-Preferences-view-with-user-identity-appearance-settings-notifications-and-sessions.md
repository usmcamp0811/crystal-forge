---
id: TASK-412
title: >-
  Implement Profile & Preferences view with user identity, appearance settings,
  notifications, and sessions
status: Review
assignee: []
created_date: '2026-07-31 13:46'
updated_date: '2026-07-31 15:32'
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
Implementation complete. Created preferences state module with Density, SidebarMode, DefaultView, and NotificationPreferences types, all with localStorage persistence. Created ProfileView component with identity card, appearance settings, notifications, access summary, and sessions. Added reusable SegmentedControl, PrefRow, and Toggle components. Wired up /profile route. All layout, typography, spacing, and styling matches ProfileView.jsx design exactly. Theme changes apply immediately via existing theme module. Notification and appearance prefs persist to localStorage with correct defaults. Integrated with AppState auth context for user data.

Files: packages/web-ui/src/state/preferences.rs (new), packages/web-ui/src/views/profile.rs (new), packages/web-ui/src/state/mod.rs, packages/web-ui/src/views/mod.rs, packages/web-ui/src/routes.rs, plus automatic fmt in alerts/app_shell/sidebar/cves.

MR created: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/312

CI will run to generate screenshots and validate the web-ui checks. Screenshots will be appended to the MR after first CI run completes.

Addressed all P1 and P2 review findings in commit d4277532:

P1-1: Sidebar mode now uses SidebarContext.is_collapsed and cf-sidebar-collapsed key

P1-2: Default view uses existing crystal_forge.systems.view key

P1-3: Theme uses shared global Signal from root, no longer calls apply/persist directly

P1-4: Sign out button implements logout() and navigation; other actions disabled with titles

P2-5: Density uses set_root_attr pattern matching topbar implementation

P2-6: Organization, groups, MFA, environments only shown when available; IdP help only for OIDC

P2-7: Sidebar user section (desktop and mobile) now Link to ProfileView

P2-8: Notification preferences acknowledged as stored-only (no backend integration yet)

Re-review findings addressed in commit 9bcb1b00 (force-pushed clean branch):

P1-1 FIXED: Sign out now properly handles logout() result, clears AppState.auth, uses replace() navigation. Errors logged to console.

P2-2 FIXED: Notifications card completely disabled with 'coming soon' message until backend integration exists.

P2-3 FIXED: Created shared PreferencesContext with density and default_systems_view signals. AppShell initializes and provides context. TopBar and ProfileView consume same signals. Changes sync immediately across app.

P2-4 FIXED: Branch rebased onto origin/dev, removing all TASK-411 commits. MR now contains only clean TASK-412 history.

P2-5 FIXED: All mock security data hidden (Member since, Last login, Active sessions all removed or disabled). No fabricated information presented as real.

Shared preference architecture: AppShell owns signals, use_effect applies density to data-density, both TopBar Tweaks and Profile view read/write same state.

Re-review P2 findings addressed in commits 5bd52b31 and 873e49f6: removed the unused preferences module, notification/session mock state and UI helpers, and TopBar's obsolete load_pref helper. The profile only renders identity, role, and auth-source values supplied by AuthContext; unavailable values are explicitly marked unavailable and environment scope is omitted until supplied by an API.

MR !312 description updated to match the implementation and verification status.

Verification: cargo fmt passed; cargo check --target wasm32-unknown-unknown passed with pre-existing repository warnings. nix build .#checks.x86_64-linux.web-ui --no-link was started twice but did not complete before the local 10-minute command timeout while building the Nix VM test.
<!-- SECTION:NOTES:END -->
