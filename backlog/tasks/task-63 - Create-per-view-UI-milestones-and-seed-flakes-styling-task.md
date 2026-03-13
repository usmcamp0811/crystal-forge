---
id: TASK-63
title: Create per-view UI milestones and seed flakes styling task
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-19 04:59'
updated_date: '2026-03-13 01:24'
labels: []
milestone: m-3
dependencies: []
priority: medium
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
UI work is tracked under a broad milestone, making it hard to measure completion by top-level view.

Goal
Create a consistent per-view UI milestone set and add an initial flakes-view polish task to the flakes milestone.

Non-Goals
- Do not implement UI code changes.
- Do not re-scope existing non-UI milestones.

Verification Plan
- Confirm new milestone files exist under backlog/milestones.
- Confirm new flakes styling task exists and is linked to the flakes milestone.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Create milestones for each top-level UI view with consistent naming
- [x] #2 Add initial flakes git history card styling task
- [x] #3 Link initial flakes task to flakes view milestone
- [x] #4 No product source code files changed
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on gray in /home/mcamp/code/crystal-forge/TASK-63-ui-view-milestones

Completed: created UI view milestones m-6..m-13, assigned milestone field across active tasks, and added TASK-64 for flakes git history card density/styling under m-10.
<!-- SECTION:NOTES:END -->
