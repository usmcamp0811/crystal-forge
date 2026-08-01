---
id: TASK-413
title: >-
  Implement Profile & Preferences view with user identity, appearance settings,
  notifications, and sessions
status: In Progress
assignee: []
created_date: '2026-07-31 13:46'
updated_date: '2026-08-01 02:48'
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
- [ ] #10 Preferences are stored server-side in a `user_preferences` table keyed only by `users.id` with theme, density, sidebar_collapsed, default_systems_view, and updated_at columns
- [ ] #11 GET `/api/v1/user/preferences` and PATCH `/api/v1/user/preferences` derive the target user exclusively from `AuthenticatedUser.user_id`; requests cannot specify another user ID
- [ ] #12 PATCH accepts partial updates and updates only supplied fields so concurrent browser sessions do not overwrite unrelated preferences
- [ ] #13 Authenticated app startup applies server preferences before rendering the normal shell and populates theme, density, sidebar, and Systems view shared signals
- [ ] #14 LocalStorage is used only as a startup cache and one-time legacy import source for users without a database preference row; after import, server values are authoritative
- [ ] #15 Preference changes send PATCH requests and display a visible error when saving fails
- [ ] #16 Tests prove same-user persistence across sessions, isolation between users, same OIDC issuer/subject reuse, no cross-user modification, server override of stale localStorage, one-time legacy import, failed-save error display, and second-browser survival for theme/density/sidebar/default Systems view
- [ ] #17 The exact acceptance behavior is covered: given the same OIDC user on two computers, selecting Light theme on computer A causes a new login/application load on computer B to use Light theme
- [ ] #18 Existing profile route, identity display, sign-out behavior, and unavailable notification/session messaging remain intact
- [ ] #19 Server checks, web-ui checks, SQLx metadata, and migration verification are run or explicitly reported if an environment limitation prevents them
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Revised implementation plan: account-scoped user preferences

### 1. Server persistence and API
- Add a new migration for `user_preferences`, keyed only by `users.id` with `ON DELETE CASCADE`, CHECK-constrained text values, and `updated_at`.
- Add typed server/API models for theme, density, sidebar collapsed, and default Systems view preferences.
- Implement database helpers that get/create preferences by `AuthenticatedUser.user_id`, import legacy values when supplied for a missing row, and partially update only supplied fields.
- Add authenticated routes:
  - `GET /api/v1/user/preferences`
  - `PATCH /api/v1/user/preferences`
- Do not accept any user ID in request bodies or query parameters.

### 2. Web UI startup and state authority
- Add a web client API for the new endpoints.
- On authenticated startup, fetch preferences before rendering the normal AppShell content.
- If the server reports no row or supports import, send legacy localStorage values once and then treat server values as authoritative.
- Populate root theme, SidebarContext, and PreferencesContext from server data, mirroring successful values into canonical localStorage keys only as cache.
- On preference changes from Profile/TopBar/sidebar/Systems view, update UI state optimistically or after success according to existing patterns, send PATCH, and show a visible save error on failure.

### 3. Tests and verification
- Add server tests for same-user persistence, user isolation, OIDC issuer/subject reuse through persistent users.id, partial PATCH isolation, and no request-specified user ID path.
- Add web UI/unit or browser tests for stale localStorage being overridden by server values, missing server row importing legacy local values once, visible failed-save error, and second-browser persistence for all four settings.
- Run targeted server/web-ui checks, SQLx preparation/metadata updates if required, and CI web-ui verification before moving back to Review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/312
Pipeline: https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2722327841
SHA: 08b17b9b971e6c8a7e22ee7e87257316ffa64835

Current implementation:
- `/profile` is reachable from desktop and mobile sidebar user sections.
- Theme, density, sidebar mode, and default Systems view share application state with existing controls and use canonical storage keys.
- Identity, role, and authentication source are derived only from AuthContext; unavailable values are omitted or labelled unavailable.
- Sign out handles failure with an inline error and clears AppState authentication only after a successful logout.
- Notifications and active-session management are visibly unavailable pending backend support.
- SystemsListView now uses PreferencesContext for default Systems view and density; changes sync immediately with Profile and TopBar without polling.

Review fixes are in commits 0219c523, 9bcb1b00, 5bd52b31, 873e49f6, and 08b17b9b. The branch was rebuilt from origin/dev, so the MR contains only TASK-412 work.

Local verification:
- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml` passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed, with pre-existing repository warnings.
- `nix build .#checks.x86_64-linux.web-ui --no-link` was started multiple times but did not complete locally: the Nix VM build exceeded the command timeout.

CI verification:
- Pipeline 2722327841 succeeded at SHA 08b17b9b971e6c8a7e22ee7e87257316ffa64835.
- All jobs passed, including `flake-check: [web-ui]`.

Continued account-scoped preferences work:
- Added current-state-aware legacy import so a missing server row imports the active theme/density/sidebar/default Systems view instead of forcing hard-coded sidebar defaults; this preserves responsive/sidebar startup behavior while still making the server row authoritative after import.
- Updated web-ui integration harness to set account preferences through `PATCH /api/v1/user/preferences` instead of mutating localStorage for sidebar/theme setup, because server preferences now override localStorage after bootstrap.
- Added `11a-profile-preferences` web-ui coverage-manifest step to assert server override of stale localStorage for theme/density/sidebar/default Systems view, profile identity/unavailable notifications/sessions messaging, and visible failed preference-save errors.
- Verification run:
  - `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml && cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
  - `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui preferences::tests'` passed: 3 tests.
  - `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_preferences --lib'` passed: 3 tests, 4 ignored live-DB tests.
  - `nix develop -c bash -c 'node --check checks/web-ui/tests/integration-test.js && cd packages/web-ui && cargo test preferences::tests --lib'` partially ran: `node --check` passed, then failed because `crystal-forge-ui` has no library target; reran with `--bin crystal-forge-ui` successfully.
- Live DB ignored tests, SQLx metadata refresh, and full Nix web-ui check remain to run in an appropriate environment.

Addressed re-review races:
- Added insert-only legacy preference initialization via `POST /api/v1/user/preferences/initialize`, backed by `initialize_user_preferences()` using `ON CONFLICT (user_id) DO NOTHING` followed by an authoritative fetch. AppShell now uses this initialization endpoint only for missing preference rows; ordinary preference saves still use partial PATCH/upsert.
- Added ignored live-DB tests for sequential and concurrent legacy initialization so a competing import returns/keeps one authoritative row instead of overwriting with a later browser's values.
- Reworked web-ui `save_update()` into a shared serialized/coalescing save worker. It keeps at most one PATCH in flight, merges pending updates while the current request is running, and sends queued updates only after the prior save finishes so later user actions reach the server after earlier actions.
- Added unit coverage for coalescing latest preference values and extended the `11a-profile-preferences` web-ui check step to delay the first PATCH, issue a second same-field theme update, and assert the final server value is the last-selected theme.
- Verification run:
  - `nix develop -c bash -c 'rustfmt --edition 2024 packages/default/crates/cf-server/src/bin/server.rs packages/default/crates/cf-server/src/handlers/api/user_preferences.rs packages/default/crates/cf-server/src/queries/user_preferences.rs packages/web-ui/src/api/client.rs packages/web-ui/src/components/layout/app_shell.rs packages/web-ui/src/state/preferences.rs && node --check checks/web-ui/tests/integration-test.js'` passed.
  - `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml && SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_preferences --lib && cd packages/web-ui && cargo check --target wasm32-unknown-unknown && cargo test --bin crystal-forge-ui preferences::tests'` passed with existing warnings; server command ran 3 non-ignored user_preferences tests and reported 6 ignored live-DB tests.
  - Attempted required ignored DB command with `CRYSTAL_FORGE_TEST_DATABASE_URL`, but the variable is not set in this shell: `CRYSTAL_FORGE_TEST_DATABASE_URL is not set` (exit 2).

Addressed final P2 re-review item:
- `save_update()` now clears `save_error` after every successful response that includes preferences, and reports an explicit error if the server returns no preferences.
- Strengthened the browser ordering test by proxying delayed PATCHes through `route.fetch()`/`route.fulfill()` and waiting on a named first-request-completed promise before reading final preferences, so an unserialized implementation cannot pass by asserting before the delayed first request reaches the server.
- Verification run: `nix develop -c bash -c 'rustfmt --edition 2024 packages/web-ui/src/state/preferences.rs && node --check checks/web-ui/tests/integration-test.js && cd packages/web-ui && cargo check --target wasm32-unknown-unknown && cargo test --bin crystal-forge-ui preferences::tests'` passed with existing warnings.

Uncommitted deployed-login hotfix attempt:
- Added a 15s AppShell account-preference bootstrap timeout so a hung `GET /api/v1/user/preferences` or initialization request no longer leaves authenticated users indefinitely on `Loading account preferences...`.
- Timeout now transitions to the existing preference-bootstrap error screen with a visible message and Retry button.
- Retry invalidates stale in-flight bootstrap attempts, clears the message, and starts a new fetch/initialize attempt.
- Verification run: `nix develop -c bash -c 'rustfmt --edition 2024 packages/web-ui/src/components/layout/app_shell.rs && cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- This does not yet prove the deployed root cause; collect browser Network details for `/api/v1/user/preferences` and `/api/v1/user/preferences/initialize` plus server logs if the deployed issue still appears after this hotfix.

Fixed the deployed-login root cause in `AppShell`: the preference bootstrap effect now reads `app_state` inside the `use_effect`, so it subscribes to `/whoami` auth-state completion and starts `GET /api/v1/user/preferences` after authentication becomes loaded. Preferences no longer block authenticated app rendering; the shell renders with cached/local defaults while server preferences load, and preference timeout/API errors appear via the existing warning banner. Verified with `nix develop -c bash -c 'rustfmt --edition 2024 packages/web-ui/src/components/layout/app_shell.rs && cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` (passed with existing warnings). Committed `25c3e954 Fix preference bootstrap auth reactivity` and pushed to MR !312 source branch `TASK-412-profile-preferences`.

Addressed remaining P1 save-worker cancellation issue. Added `PreferenceSaveWorkerGuard` around the thread-local preference save worker; if a Dioxus-owned spawned save task is canceled during an in-flight PATCH, `Drop` clears `in_flight` and requeues the current update merged with any pending latest values. This prevents later preference changes from being stuck behind a permanently true `in_flight` flag. Added unit coverage for dropping the guard with an in-flight current update and pending update. Verification passed: `nix develop -c bash -c 'rustfmt --edition 2024 packages/web-ui/src/state/preferences.rs && cd packages/web-ui && cargo check --target wasm32-unknown-unknown && cargo test --bin crystal-forge-ui preferences::tests'` (5 preference tests passed; existing warnings). Committed `c0930eb6 Make preference save worker cancellation-safe` and pushed to MR !312 source branch `TASK-412-profile-preferences`.

Addressed P1 persistence review by moving preference save queue ownership out of thread-local state and under `AppShell`. `PreferencesContext` now exposes a `Callback<UpdateUserPreferences>`; Profile, TopBar, Sidebar, and Systems send updates through that AppShell-owned callback. AppShell owns pending updates, in-flight state, and an authenticated-user/generation guard. Navigation between child views no longer cancels persistence because the save worker is spawned from AppShell state, and auth changes clear pending queued values so saves cannot cross accounts. Removed the prior thread-local `PREFERENCE_SAVE_STATE` and cancellation guard. Verification passed: `nix develop -c bash -c 'rustfmt --edition 2024 packages/web-ui/src/components/layout/app_shell.rs packages/web-ui/src/components/layout/sidebar.rs packages/web-ui/src/components/layout/topbar.rs packages/web-ui/src/views/profile.rs packages/web-ui/src/views/systems_list.rs packages/web-ui/src/state/preferences.rs && cd packages/web-ui && cargo check --target wasm32-unknown-unknown'`; `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui preferences::tests'` (4 tests passed); `nix develop -c bash -c 'node --check checks/web-ui/tests/integration-test.js'`. Committed `c2518b10 Move preference saves under AppShell` and pushed to remote branch `TASK-412-profile-preferences` for MR !312. `git ls-remote origin TASK-412-profile-preferences` confirmed remote SHA `c2518b10112f2dfdd1c5f0ea2abdd2116552c31e`; GitLab MR API was still reporting the prior head immediately after push, likely due processing lag.

Fixed collapsed sidebar logo layout separately from preference persistence. Kept `assets/cf.png`, replaced sidebar brand CSS so rail mode uses a column layout with the 28px contained logo above the 26px bordered expand button, and removed the collapsed-only inline `justify-content` style from `sidebar.rs`. Verification passed: `nix develop -c bash -c 'rustfmt --edition 2024 packages/web-ui/src/components/layout/sidebar.rs && cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` (existing warnings). Committed `8b34196e Fix collapsed sidebar logo layout` and pushed to MR !312 source branch `TASK-412-profile-preferences`; `git ls-remote origin TASK-412-profile-preferences` confirmed remote SHA `8b34196ee2cf7d295b99c20215a601bdb6cea5f4`.
<!-- SECTION:NOTES:END -->
