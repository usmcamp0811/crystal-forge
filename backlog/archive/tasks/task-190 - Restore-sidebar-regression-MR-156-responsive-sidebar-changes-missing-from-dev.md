---
id: TASK-190
title: 'Restore sidebar regression: MR-156 responsive sidebar changes missing from dev'
status: Cancelled
assignee: []
created_date: '2026-03-13 02:17'
updated_date: '2026-03-16 23:15'
labels:
  - regression
  - web-ui
  - sidebar
  - task-158
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/156'
priority: high
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
Responsive sidebar changes merged via MR !156 (TASK-158) are no longer present on `dev`. The merge commit `af5fc075` exists but is not reachable from current `origin/dev`, and the MR head branch (`refs/merge-requests/156/head` -> `95f91c64`) contains the expected sidebar commits.

Evidence:
- `git show --no-patch af5fc075` confirms merge commit exists.
- `git branch -a --contains af5fc075` shows no active branch contains it.
- `git log mr-156-head` shows sidebar implementation commits (edge toggle, grouped nav, responsive behavior, screenshot checks).

Desired outcome:
Reintroduce TASK-158 sidebar functionality onto `dev` in a dedicated restoration MR with targeted verification (web-ui check + sidebar screenshots).

Scope:
- Reapply sidebar layout/component/CSS changes from MR !156.
- Ensure no unrelated web-ui formatting-only drift is included.
- Confirm screenshot checks include sidebar states.

Non-goals:
- No new sidebar features beyond the already-reviewed MR !156 behavior.
- No unrelated refactors in web-ui files.
<!-- SECTION:DESCRIPTION:END -->
