---
id: TASK-337
title: Persist and expose scan trigger source for Scanning queue parity
status: To Do
assignee: []
created_date: '2026-06-01 02:01'
updated_date: '2026-06-01 02:02'
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
## Problem Statement

TASK-326 added `trigger` to scanning queue API/UI contracts, but the server currently returns `None` because `cve_scans` has no persisted trigger source field. This blocks full backend parity with the Scanning reference behavior and makes trigger chips non-informative.

## Goal

Persist scan trigger source in backend scan records and expose it through scanning queue endpoints so the Scanning UI can render real trigger values (for example `on_build`, `manual`, `scheduled`).

## Explicit Non-Goals

- Broad refactors outside scan trigger data flow.
- Changing schedule policy semantics unrelated to trigger attribution.
- Reworking Scanning UI layout/styling beyond consuming real trigger values.

## Architectural Constraints

- Keep trigger attribution in backend/domain/query layers; no business logic in UI views.
- Preserve current API model layering (`queries` -> `handlers` -> DTOs).
- Backward compatibility: existing queue consumers must continue to deserialize safely.
- Any schema change MUST include a migration and sqlx metadata sync.

## Impact Areas

- Database schema/migrations for `cve_scans` (or canonical scan-record source).
- Scan creation/enqueue paths (manual, scheduled, on-build).
- Scanning queue queries and API handlers.
- Server and web-ui API DTO contracts.
- Query/handler tests and sqlx offline metadata.

## Dependencies

- Depends on existing TASK-326 queue contract introducing `trigger` field in DTOs.
- No external service dependency expected; uses existing local dev DB + sqlx workflow.

## Verification Plan

- `nix develop -c cargo test -p crystal-forge-default scanning`
- `nix develop -c cargo test -p crystal-forge-default handlers::api::scanning`
- `nix develop -c cargo sqlx prepare`
- `nix build .#packages.x86_64-linux.default --no-link`
- Optional UI contract sanity: `nix build .#packages.x86_64-linux.web-ui --no-link`

## Risk Level

Medium — touches DB schema and multiple scan write paths; risk is incorrect or missing trigger attribution if any enqueue path is skipped.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A persisted trigger source exists for scan records and is populated for new scans
- [ ] #2 Scanning queue endpoints return trigger values from persisted data
- [ ] #3 Manual, scheduled, and on-build scan paths set an appropriate trigger value
- [ ] #4 Query/handler tests cover trigger mapping
- [ ] #5 SQLx metadata is regenerated and committed when required
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Promoted to To Do and upgraded to Sprint-Ready per maintainer request.
<!-- SECTION:NOTES:END -->
