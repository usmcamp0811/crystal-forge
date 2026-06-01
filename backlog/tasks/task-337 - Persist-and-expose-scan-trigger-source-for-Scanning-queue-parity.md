---
id: TASK-337
title: Persist and expose scan trigger source for Scanning queue parity
status: Backlog
assignee: []
created_date: '2026-06-01 02:01'
labels:
  - backend
  - scanning
  - api
  - database
milestone: UI/UX Refresh
dependencies: []
references:
  - TASK-326
  - packages/default/src/queries/scanning.rs
  - packages/default/src/handlers/api/scanning.rs
  - packages/default/src/api/models.rs
  - packages/web-ui/src/api/models.rs
modified_files:
  - packages/default/migrations
  - packages/default/src/queries/scanning.rs
  - packages/default/src/handlers/api/scanning.rs
  - packages/default/src/api/models.rs
  - packages/default/src/bin/cve_worker.rs
priority: medium
ordinal: 3300
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-326 added `trigger` to scanning queue API/UI contracts, but server currently returns `None` because `cve_scans` has no persisted trigger source field. This prevents full backend parity with the Scanning reference behavior.

## Desired Outcome

Persist scan trigger source in backend scan records and expose it through scanning queue endpoints so the Scanning UI can render real trigger values (for example `on_build`, `manual`, `scheduled`).

## Scope candidates

- Add migration to persist trigger source on `cve_scans` (or equivalent canonical location).
- Update scan writers (worker/scheduler/manual enqueue paths) to set trigger source.
- Update scanning queries/handlers to return trigger from DB instead of placeholder `None`.
- Add/adjust tests for query and handler mapping.
- Ensure sqlx metadata stays in sync.

## Non-Goals

- Broad refactors outside scan trigger data flow.
- Changing schedule policy semantics unrelated to trigger attribution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A persisted trigger source exists for scan records and is populated for new scans
- [ ] #2 Scanning queue endpoints return trigger values from persisted data
- [ ] #3 Manual, scheduled, and on-build scan paths set an appropriate trigger value
- [ ] #4 Query/handler tests cover trigger mapping
- [ ] #5 SQLx metadata is regenerated and committed when required
<!-- AC:END -->
