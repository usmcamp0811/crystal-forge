---
id: TASK-298
title: Add per-flake autosync settings with backend enforcement
status: Backlog
assignee: []
created_date: '2026-05-16 14:06'
labels:
  - flakes
  - backend
  - scheduler
milestone: Flakes
dependencies: []
priority: high
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
The Flakes edit UI has autosync intent, but there is no server-backed per-flake autosync configuration. Any client-only controls are not authoritative and can mislead users.

Desired Outcome
Implement proper per-flake autosync support with database persistence, API read/write, and scheduler semantics enforced on the server so autosync behavior is deterministic across users/sessions.

Scope notes
- Add DB fields/migration for autosync enabled + interval per flake.
- Extend flake API models/endpoints for these fields.
- Update server sync scheduler to honor per-flake settings.
- Add UI wiring once backend exists.
- Include validation, tests, and migration safety checks.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Database schema stores per-flake autosync enabled flag and interval with migration.
- [ ] #2 Flake API returns and accepts autosync settings with validation.
- [ ] #3 Server-side sync scheduler uses per-flake autosync settings to determine cadence.
- [ ] #4 UI can read and persist per-flake autosync settings via API without client-only behavior.
- [ ] #5 Tests cover API validation and scheduler behavior for enabled/disabled and interval changes.
<!-- AC:END -->
