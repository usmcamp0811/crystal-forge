---
id: TASK-2.2
title: Add unit tests for builder/mod.rs - build_task_description
status: Done
assignee: ["KimiK2.5"]
created_date: '2026-02-04 20:38'
updated_date: '2026-02-14 00:05'
labels:
  - testing
  - builder
  - rust
milestone: m-1
dependencies: []
parent_task_id: TASK-2
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Test build_task_description function with various inputs and mock database scenarios.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Test with valid derivation ID
- [x] #2 Test with invalid derivation ID
- [x] #3 Test with missing dependencies
- [x] #4 Mock database responses
- [x] #5 Verify task description format
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Refactored build_task_description using 'Functional Core, Imperative Shell' pattern:

1. Extracted CommitContext enum (None | Unresolved | Resolved) to represent commit lookup results
2. Extracted pure format_task_description() function - no I/O, fully unit testable
3. Extracted resolve_commit_context() async function - handles all DB queries
4. Kept build_task_description() as a thin async wrapper composing the two

18 unit tests added covering:
- format_task_description: all CommitContext variants, edge cases (empty name, short hash, special chars, HEAD~0)
- resolve_commit_context: builder integration for None/Some commit_id paths
- build_task_description: end-to-end formatting without DB for the no-commit path
- CommitContext: equality/inequality for all variants

Also brought test_utils module (from TASK-2.1) onto this branch as a dependency.

All 76 tests pass (18 new + 58 existing).
<!-- SECTION:NOTES:END -->
