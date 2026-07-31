---
id: TASK-350
title: >-
  Resolve remaining duplicate task IDs (141, 142, 178, 209, 210, 214, 238, 327,
  337)
status: Backlog
assignee: []
created_date: '2026-06-11 03:26'
labels:
  - backlog
  - maintenance
  - cleanup
milestone: 'm-1: Development Infrastructure'
dependencies: []
priority: medium
ordinal: 297000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
TASK-341 identified 9 pairs of task files with duplicate IDs where both files contain substantial content, making it unclear which is canonical. These require human review to determine which should be kept and which should be renumbered or archived.

## Affected Task IDs
- TASK-141: "Consolidate-repeated-web-ui-inline-styles..." (DONE) vs "Add-UI-based-Binary-Cache-Management..." (Review)
- TASK-142, 178, 209, 210, 214, 238, 327, 337: Each has 2 files with different content

## Goal
For each duplicate pair, determine the canonical task and either renumber or archive the other to ensure MCP task operations are unambiguous.

## Recommended Approach
1. For each task ID, identify which file is the "real" task based on:
   - Status (prefer Done/In Progress over Backlog)
   - References in other tasks/docs
   - Merge request history
   - Creation date
2. Renumber the non-canonical task to the next available ID
3. Update any references to the renumbered task
4. Verify MCP task operations work unambiguously

## Non-Goals
- No content changes to tasks themselves
- No automatic decision-making (human judgment required)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All 9 duplicate task ID pairs are reviewed and resolved
- [ ] #2 Each task ID maps to exactly one task file
- [ ] #3 MCP task operations can unambiguously identify tasks by ID
- [ ] #4 Any renumbered tasks have their references updated in related tasks/docs
<!-- AC:END -->
