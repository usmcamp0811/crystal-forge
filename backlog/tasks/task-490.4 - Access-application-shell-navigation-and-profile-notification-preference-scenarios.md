---
id: TASK-490.4
title: >-
  Access: application shell navigation and profile/notification preference
  scenarios
status: Backlog
assignee: []
created_date: '2026-10-17 16:35'
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
Owning check: integration-access. Contract IDs: access.shell, access.configuration.preferences.

WHY: shell navigation, preferences, and notification preferences are proven only by mocked steps (07-11a, 09g-09i, 17, 31). The first real use of the design-reference comparison (ctx.visual.compare) also belongs here because Shell.jsx is an approved shared reference.

OBJECTIVE: two scenarios under tests/integration/access/scenarios/: `shell-navigation` and `configuration-preferences`. Test-only.

NON-GOALS: notification delivery from domain events (TASK-490.5); domain values shown on the dashboard (owned by Delivery/Assurance); every viewport x theme product; production changes.

BASE: foundation commit in implementation notes (FOUNDATION_COMMIT). Pattern: access/scenarios/lifecycle-local-bootstrap.

AUTHORITY: docs/knowledge/ui/frontend-navigation-and-shared-patterns.md; docs/knowledge/ui/design-system-accessibility-responsive-motion.md; docs/knowledge/ui/alerts-and-notifications-decision.md (preferences only). Approved design: docs/design/CrystalForge/components/Shell.jsx and ProfileView.jsx. As-built docs: `Authority: as-built (D1)`.

PRODUCTION SOURCE TO READ (read-only): packages/web-ui/src/routes.rs (Route enum, AppShell layout, NotFoundView); components/layout/app_shell.rs and sidebar.rs (nav, collapse button aria-labels "Expand sidebar"/"Collapse sidebar", mobile "Close menu"); views/profile.rs; cf-server src/handlers/api/user_preferences.rs (get_preferences, patch_preferences, initialize_preferences; /api/v1/user/preferences) and user_notifications.rs (get/patch_notification_preferences; /api/v1/user/notification-preferences; NotificationDeliveryChannel InApp|Email|Both in src/models/user_notifications.rs).

WRITABLE (exclusive): tests/integration/access/scenarios/shell-navigation.{nix,py,cjs}, tests/integration/access/scenarios/configuration-preferences.{nix,py,cjs}. READ-ONLY: all other paths.

HARNESS: controller { browser = true; }; desktop and narrow viewports via run_browser(node, spec, viewport="narrow"); ctx.visual.compare(name, {view, theme, referenceSelector, actualSelector, maxDifferentRatio}) renders the approved design from the same pinned profile. You MUST state and justify maxDifferentRatio in the spec; do not raise it to pass. visual.compare has not yet run in an executed scenario: if it misbehaves (harness bug), stop and escalate to the coordinator rather than working around it. Facts: admin u1 and a second user u2 via harness.facts.localUserFromDesign.

SCENARIO shell-navigation (actor: admin; unauthenticated visitor): unauthenticated visit to a protected route redirects to /login; signed-in navigation reaches every route listed in routes.rs AppShell and each renders its page heading; unknown route shows NotFound with a return path; sidebar collapse persists per the documented behavior; keyboard: Tab reaches the primary navigation and Enter activates it; narrow viewport uses the mobile menu with a working "Close menu"; one visual comparison of the shell chrome against Shell.jsx (desktop, dark) with a stated threshold.
SCENARIO configuration-preferences (actors: two users): user A changes profile preferences and notification preferences through /profile; DB rows and reload show the change; user B's preferences are unchanged; invalid input (if the UI allows it) keeps the draft and persists nothing.

COMMANDS: nix build .#integration.access.scenarios.shell-navigation -L ; nix build .#integration.access.scenarios.configuration-preferences -L ; .run for evidence. At most two VM tests concurrently. Coordinator runs .#integration.access.all.

COMPLETION, PRODUCT DEFECT, ESCALATION: same rules as TASK-490.2. A required visual state without an approved design state is recorded as blocked: design gap (component, condition, missing fact).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 shell-navigation and configuration-preferences exist only under tests/integration/access/scenarios/ and are discovered
- [ ] #2 Every AppShell route in routes.rs is reached through navigation and renders; unknown routes show NotFound; unauthenticated access redirects to /login
- [ ] #3 Keyboard and narrow-viewport navigation work as documented
- [ ] #4 At least one visual comparison against Shell.jsx runs with an explicitly justified threshold and preserved reference/actual/diff images
- [ ] #5 Preference changes persist per user through reload and do not affect another user
- [ ] #6 Both scenarios executed in real VMs with commands, revision, outcomes, and evidence paths in task notes
<!-- AC:END -->
