---
id: TASK-285
title: 'UI/UX refresh umbrella: Builds and Evaluations views'
status: To Do
assignee: []
created_date: '2026-04-30 21:35'
updated_date: '2026-06-10 02:53'
labels:
  - ui
  - ux
  - web-ui
  - design-system
  - umbrella
milestone: UI/UX Design System
dependencies: []
references:
  - TASK-283
  - TASK-284
  - /home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js
priority: medium
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Builds and Evaluations views are being redesigned from the latest UI/UX mockups, but implementation is currently tracked as separate workstreams without a shared execution umbrella.

## Goal
Create a single umbrella task to coordinate and track the full Builds + Evaluations UI/UX refresh initiative, while execution occurs through linked child tasks.

## Scope
- Coordinate delivery of:
  - TASK-283 (Builds view UI/UX refactor)
  - TASK-284 (Evaluations view UI/UX refactor)
- Ensure consistency of visual language, interaction behavior, and screenshot-based verification across both views.

## Non-Goals
- No direct implementation in this umbrella task.
- No backend/API feature work unless separately scoped in child tasks.

## Coordination Constraints
- Child tasks remain the implementation units.
- Shared UI patterns should be aligned across both surfaces.
- Verification evidence should include updated web-ui check screenshots for both views before initiative closure.

## Verification Plan
- Confirm child tasks reach Review/Done with passing:
  - `nix develop -c cargo check` (web-ui)
  - `nix build .#checks.x86_64-linux.web-ui`
- Confirm both child MRs include screenshot evidence from web-ui checks.

## Risk Level
Medium: parallel UI refactors can diverge without explicit coordination.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 TASK-283 and TASK-284 are linked as child tasks of this umbrella task.
- [x] #2 Builds and Evaluations redesign work is coordinated under one milestone and consistent design direction.
- [x] #3 Both child tasks complete with passing web-ui compile/check targets and updated screenshot/assertion coverage.
- [x] #4 No out-of-scope backend/API changes are introduced without separate explicit tasks.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Completion Summary

Both child tasks have been successfully completed and merged:

- **TASK-283** (Builds view): Merged via MR !248 on 2026-05-03
  - Commit: 3c78ea35 "Merge branch 'TASK-283-refactor-builds-view-ui-ux' into 'dev'"
  - Full JSX parity achieved with proper data mapping, icon system, and visual styling
  - web-ui checks passed with screenshot evidence

- **TASK-284** (Evaluations view): Merged via branch 'TASK-284-refactor-evaluations-view' on 2026-05-01
  - Commit: 5357e90e "Merge branch 'TASK-284-refactor-evaluations-view' into 'dev'"
  - Complete drawer UI with policy matrix, dependency graphs, and real log streaming
  - web-ui checks passed with full tab coverage

Both refactors maintain consistent design language, interaction patterns, and screenshot-based verification as required by the umbrella coordination goals.
<!-- SECTION:NOTES:END -->
