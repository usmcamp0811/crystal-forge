---
id: TASK-490.4
title: >-
  Access: application shell navigation and profile/notification preference
  scenarios
status: To Do
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-20 20:30'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.1
references:
  - docs/knowledge/testing/integration-coverage-map.md
  - docs/knowledge/ui/frontend-navigation-and-shared-patterns.md
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/components/ProfileView.jsx
modified_files:
  - checks/integration-access/scenarios/fragments/shell-navigation.py
  - checks/integration-access/scenarios/fragments/shell-navigation.cjs
  - checks/integration-access/scenarios/fragments/preferences.py
  - checks/integration-access/scenarios/fragments/preferences.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 516000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.shell, access.configuration.preferences. Goal: prove real shell navigation and per-user profile/notification preference persistence with an approved Shell.jsx comparison. Risk: medium (presentation authority and identity isolation). Non-goals: event notification delivery (.5), Delivery/Assurance dashboard domain values, all theme/viewport combinations and product changes.

Writable paths (only):
- checks/integration-access/scenarios/fragments/shell-navigation.py
- checks/integration-access/scenarios/fragments/shell-navigation.cjs
- checks/integration-access/scenarios/fragments/preferences.py
- checks/integration-access/scenarios/fragments/preferences.cjs

Composition target: checks/integration-access/scenarios/lifecycle-local-bootstrap.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal owner aggregation. Compose after real bootstrap with .2/.3 in one fresh local-auth VM/database. Earlier real users may be consumed through same-scenario receipts; another derivation's live state, IDs or cookies may not. No packet-specific derivation. Shared helpers, production, CI, docs, registration and design references are read-only.

Shell: unauthenticated protected navigation redirects to /login. Signed-in navigation reaches every accepted AppShell route and heading. Unknown routes show NotFound and the accepted return path. Sidebar collapse follows accepted persistence semantics; Tab reaches primary navigation, Enter activates it; narrow viewport mobile menu and Close menu work. Compare desktop dark shell chrome against approved Shell.jsx with ctx.visual.compare(name, {view, theme, referenceSelector, actualSelector, maxDifferentRatio}); justify threshold and retain reference/actual/diff images. Do not raise a threshold to pass. The visual comparison helper remains unproved in an executed comparison; report interface faults to coordinator.

Preferences: user A changes profile and notification preferences through /profile; DB reads and reload match authored inputs while user B remains unchanged. Use design users u1/u2 where approved and same-run identity receipts, with independently authenticated browser contexts. Invalid exposed input retains draft and persists nothing where accepted requirements specify it. Missing approved responsive/visual/error states remain blocked with exact component, condition and missing fact.

Authority/source reading: approved docs/design/CrystalForge/components/Shell.jsx and ProfileView.jsx control presentation. docs/knowledge/ui/frontend-navigation-and-shared-patterns.md, design-system-accessibility-responsive-motion.md and alerts-and-notifications-decision.md (preferences) are as-built evidence, not D1 acceptance authority. Read web-ui routes.rs (Route/AppShell/NotFoundView), components/layout/app_shell.rs, sidebar.rs, profile.rs; cf-server handlers/api/user_preferences.rs, user_notifications.rs and models/user_notifications.rs (InApp/Email/Both).

Dependency/readiness: TASK-490.1 is Review. Current tested controller pin is 81d65be745cc9f9777d368900d01fe0f207cb052 with .25/.26 and the .2 session subset verified. Focused Access and aggregate passed 10 cases, 292 migrations and database.unixSocketClients=22; these do not prove shell/preferences fragments or a visual comparison. Use FACTS["admin"], receipts()["admin.userId"], documented driver/browser assertions, sql_json and run_browser narrow viewport with independent sign-in contexts. Named fragment paths and lifecycle-local-bootstrap placement above are concrete. This packet remains Backlog/unselected/unexecuted. Exact gaps are accepted sidebar-persistence/keyboard/narrow responsive states, any unapproved invalid-input draft behavior and missing visual reference states/threshold facts; visual.compare also requires executed interface verification. No future Delivery role or global foundation-completion gate blocks controller shell/preferences proofs.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L`. Record exact revision, command, outcome and evidence. Current bootstrap/session success does not prove these fragments. Preserve correct assertions and linked product defects; escalate authority/helper gaps without product changes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes the named shell/preferences fragments into lifecycle-local-bootstrap; no packet-specific VM derivation is added
- [ ] #2 Every accepted AppShell route is reached through navigation and renders its heading; unknown routes show NotFound/accepted return path and protected routes redirect unauthenticated users to /login
- [ ] #3 Keyboard Tab/Enter, sidebar persistence and narrow mobile menu/Close menu satisfy accepted requirements; unresolved states remain explicitly blocked
- [ ] #4 At least one approved desktop-dark Shell.jsx comparison executes with a justified unchanged threshold and retained reference/actual/diff images
- [ ] #5 Profile and notification preferences persist per user through database read/reload without changing user B; invalid-input behavior is asserted only under accepted authority
- [ ] #6 The coordinator executes the focused lifecycle-local-bootstrap gate with these fragments and records exact revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Owner now explicitly selects the independent shell/profile/notification-preference contribution. Existing Access23 cases and controller setup are implemented at df11c800. No obsolete bootstrap/Rust startup blocker applies. Worker owns only the four named semantic Python/browser fragments; Sol owns facts, composition, registration and VM execution. Approved Shell/Profile references control expectations; missing decisions block only their dependent assertions. No product UI change is authorized by this test-only child.
<!-- SECTION:NOTES:END -->
