---
id: TASK-411
title: >-
  TASK-326 - Relocate CVEs nav entry and add Scanning view parity merge-blocker
  fix
status: Done
assignee: []
created_date: '2026-06-01 03:40'
updated_date: '2026-06-02 00:19'
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
- [x] #1 `/api/v1/scanning/queue` does not attempt to decode NULL scan fields for never-scanned derivations
- [x] #2 `/api/v1/scanning/systems/:id/scans` does not attempt to decode NULL scan fields for never-scanned derivations
- [x] #3 Queue endpoint behavior remains consistent for derivations with scan rows
- [x] #4 Targeted verification commands pass for touched backend/web-ui tests
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
User confirmed MR merged; task transitioned to Done and worktree cleanup initiated.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed MR !267 merge blocker by ensuring scanning queue endpoints only return concrete scan rows: changed `LEFT JOIN cve_scans` to `JOIN cve_scans` in both `get_scan_queue` and `get_scan_queue_for_system`, eliminating NULL decode/500 risk for never-scanned derivations. Also corrected per-system `is_current` semantics to `TRUE` for latest-per-derivation rows and tightened Playwright route mocks to distinct `systems?*` and `systems/*/scans*` patterns to validate endpoint-shape consumption explicitly.
<!-- SECTION:FINAL_SUMMARY:END -->
