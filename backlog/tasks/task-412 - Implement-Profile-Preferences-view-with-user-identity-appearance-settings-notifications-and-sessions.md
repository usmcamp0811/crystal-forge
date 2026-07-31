---
id: TASK-412
title: >-
  Implement Profile & Preferences view with user identity, appearance settings,
  notifications, and sessions
status: In Progress
assignee: []
created_date: '2026-07-31 13:46'
updated_date: '2026-07-31 14:00'
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
priority: high
type: feature
ordinal: 401000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Profile & Preferences view (`ProfileView.jsx` in the design example) is completely missing from the Crystal Forge web UI. Users have no way to view their account information, customize their UI experience (theme, density, sidebar mode, default views), manage notification preferences, or view/revoke active sessions. The theme toggle exists in the sidebar but there's no centralized settings surface.

## Desired Outcome

A fully functional `/profile` route that displays:
- User identity card with avatar (gradient circle with initials), name, email, role chip, auth source, organization, OIDC groups, and MFA status
- Appearance preferences: theme (Dark/Light), density (Comfortable/Compact), sidebar mode (Full/Rail), default systems view (Cards/Table)
- Notification toggles: deploy failures, build failures, critical CVEs, policy violations, heartbeat lost, weekly digest, plus delivery channel (In-app/Email/Both)
- Access summary: role, environments, auth source, member since, last login
- Active sessions list with current device indicator and revoke buttons
- Sign out and "Sign out everywhere" actions

All appearance preferences should persist to localStorage and apply immediately when changed, matching the existing UiTheme pattern in `packages/web-ui/src/state/theme.rs`.

The view must match the design reference (`docs/design/CrystalForge/components/ProfileView.jsx`) pixel-for-pixel: card layout, two-column grid for Appearance/Notifications and Access/Sessions, segmented button controls, toggles, chips, typography, spacing, and the kv-grid pattern for the access summary.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Route /profile exists in routes.rs with ProfileView component inside AppShell layout
- [ ] #2 Page header shows 'Profile & Preferences' title and 'Personal settings for your Crystal Forge account' subtitle matching design
- [ ] #3 Identity card displays user avatar (64px gradient circle with initials from display_name or email), full name, email (mono font), role chip (chip-critical style), auth source chip (chip-unknown), organization chip (chip-info), OIDC groups chips (chip-unknown mono), and MFA status chip (chip-healthy) when applicable
- [ ] #4 Identity card includes 'Change password' and 'Sign out' buttons (btn-ghost xs) aligned to the right
- [ ] #5 Appearance card contains four PrefRow controls with segmented buttons: Theme (Dark/Light), Density (Comfortable/Compact), Sidebar (Full/Rail), Default systems view (Cards/Table)
- [ ] #6 Theme preference changes apply immediately via state/theme.rs apply() and persist() functions, updating the document data-theme attribute and localStorage
- [ ] #7 Density, sidebar mode, and default view preferences persist to localStorage using dedicated keys (cf.ui.density, cf.ui.sidebarMode, cf.ui.defaultView) and apply state changes
- [ ] #8 Notifications card contains seven PrefRow controls: six toggle switches (deploy failures, build failures, critical CVEs, policy violations, heartbeat lost, weekly digest) and one segmented button for delivery channel (In-app/Email/Both)
- [ ] #9 All notification preferences persist to localStorage under cf.ui.notifications with default values matching design (deployFailed: true, buildFailed: true, criticalCve: true, policyFail: true, heartbeatLost: false, weeklyDigest: true, channel: in-app)
- [ ] #10 Access summary card uses kv-grid class with rows for Role (chip), Environments (chip showing 'all' or specific envs), Auth source (text with groups), Member since (static mock), Last login (static mock), plus help text about IdP group control
- [ ] #11 Active sessions card displays mock session list with device/browser, IP (mono font), timestamp, 'this device' chip (chip-healthy) for current session, and 'Revoke' button (btn-ghost xs) for other sessions
- [ ] #12 Sign out everywhere button styled with amber warning color (color: #fbbf24, borderColor: rgba(251,191,36,0.3)) appears below sessions list
- [ ] #13 All layout, typography, spacing, chips, buttons, and card structure match ProfileView.jsx exactly: two-column grid (1fr 1fr) for cards, PrefRow styling with borderBottom dividers, segmented button (.seg) controls, checkbox toggles with brand purple accent
- [ ] #14 View works correctly with existing AppState auth context, reading user.display_name, user.email, roles array, and auth_mode; displays appropriate default values when fields are missing
- [ ] #15 cargo fmt, clippy -D warnings, and cargo test pass in packages/web-ui
- [ ] #16 nix build .#checks.x86_64-linux.web-ui passes with any required baseline updates for the new route
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
<!-- SECTION:NOTES:END -->
