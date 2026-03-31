---
id: TASK-230
title: >-
  Improve build queue UX: ordering, pagination, search/filter, and build log
  parity
status: In Progress
assignee: []
created_date: '2026-03-31 02:48'
updated_date: '2026-03-31 02:49'
labels:
  - builds
  - queue
  - frontend
  - backend
  - ux
  - sprint-ready
dependencies: []
references:
  - packages/web-ui/src/views/builds.rs
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - >-
    backlog/tasks/task-155 -
    Improve-build-queue-UI-with-drag-and-drop-reordering-and-better-card-design.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The build queue experience is hard to operate at scale:
- Queue ordering appears counterintuitive (older items often shown first).
- Operators cannot reliably browse the entire queue.
- Search and filtering are limited for pinpointing specific builds.
- Build log viewing does not match the evaluation log experience (live + historical context).

## Goal
Deliver a single operator-focused build queue upgrade that provides:
1. Correct default queue ordering (newest queued items first).
2. Full queue accessibility via server-side pagination.
3. Practical search/filter controls for day-to-day troubleshooting.
4. Build log viewer parity with evaluation logs (live stream + historical log viewing).

## Non-Goals
- No drag-and-drop queue reordering in this task.
- No scheduler policy redesign beyond deterministic display/query ordering.
- No major visual redesign outside queue usability and log-viewer parity.
- No changes to evaluation queue behavior.

## Scope
### Build queue listing
- Add deterministic default ordering: newest-first for queue display.
- Support server-side pagination for queue results.
- Ensure UI can navigate all pages and preserves active filters/search across pagination.

### Search and filtering
- Add queue search/filter by:
  - commit hash (full/partial)
  - flake/repo name
  - system/config name
  - time range
- Keep filter behavior explicit and composable.

### Build logs parity
- Add/upgrade build log panel so selected build supports:
  - live streaming updates while build is active
  - historical log browsing for retained output
- Match eval-log interaction expectations where practical (scrollable, readable, stable while streaming).

## Architectural Constraints
- Keep business/query logic in backend handlers/query modules; UI remains presentation + interaction.
- Reuse existing log transport/storage patterns where possible.
- Avoid hidden global state; keep queue/search state localized to build queue view/state.
- Preserve DTO boundary consistency between server and web-ui models.

## Verification Plan
- Backend tests for ordering, pagination, and filter query behavior.
- Frontend tests for:
  - default newest-first ordering presentation
  - pagination navigation and state retention
  - search/filter combinations
  - build log live + historical rendering behavior
- Targeted Nix dev checks for touched packages and relevant web-ui check.

## Impact Areas
- `packages/default/src/handlers/api/*build*`
- `packages/default/src/queries/*build*`
- `packages/default/src/api/models.rs` (if listing/log DTO shape changes)
- `packages/web-ui/src/views/*build*`
- `packages/web-ui/src/api/*`
- `checks/web-ui/tests/integration-test.js` (or equivalent build-queue scenario)

## Risk Level
Medium-High (operator workflow and queue observability behavior).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Build queue default order is newest-first and deterministic under equal-priority conditions.
- [ ] #2 Operators can access the full queue using server-side pagination controls.
- [ ] #3 Queue search/filter supports commit hash, flake/repo name, system/config name, and time range.
- [ ] #4 Pagination works correctly with active filters/search (no silent reset or inconsistent counts).
- [ ] #5 Selected build shows live log updates while active and supports historical log browsing for retained output.
- [ ] #6 Queue and log UI remain responsive with production-like queue sizes.
- [ ] #7 Targeted backend/frontend tests and relevant web-ui checks pass for new behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved Backlog -> To Do per explicit human sprint selection in chat.

User-selected defaults captured: single combined task; newest-first queue order; server-side pagination; must-have filters = commit hash, flake/repo, system/config, time range; build logs require live + historical behavior.

LOCK: claude-sonnet-4-6 on reckless in /home/mcamp/code/crystal-forge/TASK-230-build-queue-ux
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 MR documents before/after queue ordering semantics and log-viewer parity behavior.
- [ ] #2 Any out-of-scope follow-ups discovered during implementation are captured as separate Backlog tasks.
<!-- DOD:END -->
