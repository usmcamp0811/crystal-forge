---
id: TASK-412
title: >-
  Resolve duplicate task IDs involving uppercase-prefix TASK-*.md files (212,
  213, 215, 394, 395, 397, 398)
status: Backlog
assignee: []
created_date: '2026-07-31 03:00'
labels:
  - backlog-hygiene
dependencies: []
priority: medium
type: task
ordinal: 400000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Discovery

While verifying the TASK-350 duplicate-ID repair (`backlog doctor --fix`), a case-insensitive scan of `backlog/tasks/*.md` and `backlog/completed/*.md` found **7 more duplicate-ID pairs on dev** that `backlog doctor` cannot repair:

| ID | Files (both claim the ID) | Type |
| --- | --- | --- |
| TASK-212 | `task-212 - HOTFIX-Custom-policy-...md` (Done) + `TASK-212 - HOTFIX Custom policy ...md` (Done) | Same task, duplicate record |
| TASK-213 | `task-213 - CRITICAL-Policy-...md` (Done) + `TASK-213 - CRITICAL Policy ...md` (Done) | Same task, duplicate record |
| TASK-215 | `TASK-215 - Optimize flakes view...md` (Review, active) + `completed/task-215 - Optimize-flakes-view-...md` (completed) | Same task, active vs completed |
| TASK-394 | `TASK-394 - Implement-2026-07-16-design-delta-...md` (In Progress) + `TASK-394 - Implement-2026-07-18-builds-evals-infinite-scroll-...md` (Review) | Different tasks sharing ID |
| TASK-395 | `TASK-395 - Implement-2026-07-17-design-delta-global-search-and-logo.md` (To Do) + `task-395 - Split-backend-into-process-boundary-crates-...md` | Different tasks sharing ID |
| TASK-397 | `task-397 - Evaluation-errors-...md` + `TASK-397-flakes-view-database-only-read-path.md` (Review) | Different tasks sharing ID |
| TASK-398 | `TASK-398-alerts-attention-incidents.md` (Review) + `task-398 - mkStigModule-overrideAttrs-...md` | Different tasks sharing ID |

## Root cause

The doctor's active-task scan glob is `${prefix}-*.md` (e.g. `task-*.md`), which is **case-sensitive**. Uppercase-prefix files (`TASK-*.md`) never match, so they are invisible to `listTasks()` and never form a duplicate group. The cross-branch scan does see them (e.g. `dev:backlog/tasks/TASK-394 - ...` appears in cross-branch findings) but reports them as "cannot be repaired from the current branch" because they are not in the active-task groups.

Example of the operational impact: `backlog task view TASK-394` resolves to only one of the two files (the 07-16 one), so MCP operations silently target an arbitrary file.

## Notes

- TASK-350 covered the original 9 pairs (141, 142, 178, 209, 210, 214, 238, 327, 337) plus completed task-3 pairs; those are now renamed to TASK-401..411 (verified).
- The uppercase-prefix files were created by commits like `94347d5e` ("chore: Create TASK-394 ...") and `a1fc048a` ("add infinite scroll task") — the MCP/Web-UI creation path uses `TASK-<id> - <title>.md` naming, while the CLI uses `task-<id> - <slug>.md`.
- Fixing these requires manual rename/archive (doctor cannot do it), and possibly normalizing the MCP/Web-UI file-creation naming to lowercase-prefix so future files match the CLI glob.
<!-- SECTION:DESCRIPTION:END -->
