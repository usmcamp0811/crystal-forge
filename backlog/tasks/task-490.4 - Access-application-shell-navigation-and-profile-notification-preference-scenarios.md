---
id: TASK-490.4
title: >-
  Access: application shell navigation and profile/notification preference
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
updated_date: '2026-10-17 17:25'
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
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 516000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract IDs: access.shell, access.configuration.preferences. Existing steps 07-11a, 09g-09i, 17 and 31 are mocked. Prove real shell/navigation and per-user persistence; first design-reference comparison belongs here because Shell.jsx is approved.

Exclusive writes: checks/integration-access/scenarios/shell-navigation.{nix,py,cjs} and configuration-preferences.{nix,py,cjs}. Non-goals: event notification delivery (490.5), dashboard domain values (Delivery/Assurance), all viewport/theme combinations, production changes. All other paths remain read-only under TASK-490 test-only scope.

Corrected organization: native checks/integration-access/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator-owned scenarios, and pkgs.linkFarm for ALL registered gates. Helpers: lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }. Owner-local scripts/fixtures/assets; no root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix, shared lib, CI/docs and explicit child registration. Meaningful WIP can be discoverable; only Access exists now and no placeholder owners are added.

Authority review: docs/knowledge/ui/frontend-navigation-and-shared-patterns.md, design-system-accessibility-responsive-motion.md, alerts-and-notifications-decision.md (preferences only) are as-built evidence, not accepted D1 authority. Approved presentation references remain docs/design/CrystalForge/components/Shell.jsx and ProfileView.jsx, read-only. Uncertain expected behavior remains blocked pending owner decision; no Authority: as-built header.

Read-only source: web-ui routes.rs Route/AppShell/NotFoundView, components/layout/app_shell.rs and sidebar.rs (Expand sidebar, Collapse sidebar, mobile Close menu), profile.rs; cf-server handlers/api/user_preferences.rs get/patch/initialize and user_notifications.rs notification preferences; models/user_notifications.rs InApp|Email|Both.

Verify concrete foundation interfaces before dispatch. Pattern: checks/integration-access/scenarios/lifecycle-local-bootstrap. Required browser controller, narrow run_browser viewport and ctx.visual.compare(name, {view, theme, referenceSelector, actualSelector, maxDifferentRatio}). Justify the threshold in the spec; never raise it to pass. visual.compare has not run in an executed scenario: harness faults stop this case and go to coordinator, no workaround. Facts: u1 admin and u2 second user via localUserFromDesign.

shell-navigation: unauthenticated protected route -> /login; signed-in navigation reaches every AppShell route and page heading; unknown route has NotFound and return path; sidebar collapse persists according to accepted requirements. Keyboard Tab reaches primary navigation, Enter activates; narrow viewport mobile menu and Close menu work. Compare desktop dark shell chrome with Shell.jsx using justified threshold and preserve reference/actual/diff images.

configuration-preferences: user A changes profile and notification preferences through /profile; DB rows and reload agree; user B unchanged. Invalid input, if exposed by UI, retains draft and persists nothing according to accepted requirements. Missing approved visual states remain blocked with component, condition and missing fact.

Candidate commands, NOT verified: nix build .#checks.x86_64-linux.integration-access.scenarios.shell-navigation -L and configuration-preferences; append .run for evidence. Coordinator aggregate: nix build .#checks.x86_64-linux.integration-access -L. Run one VM test at a time until measured capacity supports a higher limit. Explicit registration and real VM execution required, with exact commands/revision/outcomes/evidence in notes. Expectations derive from authored facts/accepted requirements, never CF responses. Preserve correct failing assertions, link Backlog product bugs and escalate interface/authority gaps; no product fixes.

READINESS: foundation dependency commit unresolved; two existing Rust package tests block foundation and no VM execution occurred. Backlog, not ready. Future packet requires actual dependency commit(s), implemented signatures/fixtures/registration and verified commands before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both scenarios exist only under checks/integration-access/scenarios/ and are explicitly registered by the coordinator in the normal pkgs.linkFarm aggregate
- [ ] #2 Every AppShell route is reached through navigation and renders its heading; unknown routes show NotFound and protected routes redirect unauthenticated users to /login
- [ ] #3 Keyboard, sidebar and narrow-viewport navigation satisfy accepted requirements; uncertain cases remain blocked pending owner decision
- [ ] #4 At least one Shell.jsx visual comparison runs with a justified threshold and preserved reference/actual/diff images
- [ ] #5 Profile and notification preferences persist per user through reload without affecting another user
- [ ] #6 Both scenarios execute in real VMs with exact commands, revision, outcomes and .run evidence recorded after actual dependency commits and verified packet commands exist
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior layout, commands and D1 authority instructions. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented helpers and verified commands are required before To Do. No Luna delegation or executed slice is claimed. Coordinator owns owner default.nix, shared lib and CI/docs; scenario files stay disjoint.
<!-- SECTION:NOTES:END -->
