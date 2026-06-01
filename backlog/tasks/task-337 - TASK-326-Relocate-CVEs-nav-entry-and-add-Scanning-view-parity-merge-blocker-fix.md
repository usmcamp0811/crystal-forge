---
id: TASK-337
title: >-
  TASK-326 - Relocate CVEs nav entry and add Scanning view parity merge-blocker
  fix
status: In Progress
assignee: []
created_date: '2026-06-01 03:40'
updated_date: '2026-06-01 03:55'
labels:
  - backend
  - scanning
  - api
  - bugfix
  - sprint-ready
milestone: System Details Hardening
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/267'
modified_files:
  - packages/default/src/queries/scanning.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 3260
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
MR !267 has a merge-blocking defect in scanning queue endpoints: never-scanned derivations can produce NULL scan fields and cause decode failures/500s for `/api/v1/scanning/queue` and `/api/v1/scanning/systems/:id/scans`.

## Goal
Apply a minimal backend query fix so queue endpoints only return concrete scan rows and cannot fail due to NULL `cve_scans` columns for never-scanned derivations.

## Non-Goals
- No redesign of scanning API contracts beyond blocker fix
- No unrelated refactors in scanning handlers/DTOs
- No schema/migration changes unless strictly required

## Scope
- Update `get_scan_queue` query to require matching `cve_scans` rows
- Update `get_scan_queue_for_system` query to require matching `cve_scans` rows
- (Optional small correctness cleanup) set `is_current` semantics correctly for per-system scan rows
- Tighten Playwright route mocks to avoid overly broad match if test file is part of MR scope

## Architectural Constraints
- Preserve existing API DTOs and endpoint shapes
- Keep change minimal and localized to scanning query/test files
- Follow existing SQLx query style and Rust patterns in repository

## Verification Plan
- Run targeted backend tests for scanning query/handler behavior
- Run targeted web-ui check/test only for touched scanning integration mock
- Ensure no 500 decode path remains for never-scanned derivations in queue queries

## Impact Areas
- `packages/default/src/queries/scanning.rs` (or current scanning query location)
- `checks/web-ui/tests/integration-test.js` (if mock pattern adjusted)

## Risk Level
Low-Medium (query join behavior change on queue endpoints)

## Dependencies
- Existing MR !267 branch content and scanning API code present on task branch
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `/api/v1/scanning/queue` does not attempt to decode NULL scan fields for never-scanned derivations
- [ ] #2 `/api/v1/scanning/systems/:id/scans` does not attempt to decode NULL scan fields for never-scanned derivations
- [ ] #3 Queue endpoint behavior remains consistent for derivations with scan rows
- [ ] #4 Targeted verification commands pass for touched backend/web-ui tests
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented minimal blocker fix in scanning queries: changed queue CTE joins from LEFT JOIN cve_scans to JOIN cve_scans in both get_scan_queue and get_scan_queue_for_system; set get_scan_queue_for_system is_current projection to TRUE.

Tightened web-ui integration mock routing for scanning systems endpoints: split `**/api/v1/scanning/systems?*` and `**/api/v1/scanning/systems/*/scans*` patterns and matching unroute calls.

Verification attempt failed due local DB connectivity in sqlx compile-time checks: `nix develop -c cargo test -p crystal-forge handlers::api::scanning::tests -- --nocapture` and `nix develop -c cargo test -p crystal-forge query_scanning -- --ignored --nocapture` both errored with `error communicating with database: Connection refused (os error 111)`. Requires starting repo dev DB stack before rerun.
<!-- SECTION:NOTES:END -->
