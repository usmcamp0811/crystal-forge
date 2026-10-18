---
id: TASK-490.4
title: >-
  Access: application shell navigation and profile/notification preference
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-18 14:41'
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
  - checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.py
  - >-
    checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.cjs
  - checks/integration-access/scenarios/fragments/task-490.4-preferences.py
  - checks/integration-access/scenarios/fragments/task-490.4-preferences.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 516000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.shell, access.configuration.preferences. Goal: prove real shell navigation and per-user profile/notification preference persistence with an approved Shell.jsx comparison. Risk: medium (presentation authority and identity isolation). Non-goals: event notification delivery (.5), Delivery/Assurance dashboard domain values, all theme/viewport combinations and product changes.

Writable paths (only):
- checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.py
- checks/integration-access/scenarios/fragments/task-490.4-shell-navigation.cjs
- checks/integration-access/scenarios/fragments/task-490.4-preferences.py
- checks/integration-access/scenarios/fragments/task-490.4-preferences.cjs

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
Current readiness: TASK-490.1 is Review in draft MR !335. Tested controller pin 81d65be745cc9f9777d368900d01fe0f207cb052 passed focused Access and aggregate with 10 cases, 292 migrations and database.unixSocketClients=22; runtime i84kvs1lc5nqx0lmsqinwbmvy0ix8mvh. The controller packet was directly inspected for disjoint paths, lifecycle-local-bootstrap placement, FACTS["admin"]/admin.userId receipts, independent run_browser contexts and coordinator composition/registration ownership. No future role dependency is required.

Still Backlog, unselected and unexecuted. Exact authority gaps: accepted sidebar persistence and keyboard/narrow responsive states, invalid-input draft behavior where exposed, missing approved visual reference/threshold facts. The visual.compare interface has not been proved by an executed comparison; the session screenshot is not that proof. These gaps remain limited to their affected cases; no shell/preferences criterion is checked from bootstrap/session results.

### Historical
The 2026-10-17 review recorded unresolved foundation/package prerequisites and no VM execution at that time. Former separate shell/preferences derivations are not the current layout. Bootstrap passed at b1095c8f and then rebased bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab; runtime riky1mm0nn185wb26abw7zzshnbml0fp had 292 migrations. Those results covered bootstrap only.
<!-- SECTION:NOTES:END -->
