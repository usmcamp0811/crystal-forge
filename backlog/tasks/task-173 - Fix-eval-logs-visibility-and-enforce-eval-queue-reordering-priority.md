---
id: TASK-173
title: Fix eval logs visibility and enforce eval queue reordering priority
status: In Progress
assignee: []
created_date: '2026-03-04 23:22'
updated_date: '2026-03-07 23:36'
labels:
  - bug
  - eval-queue
  - web-ui
  - server
dependencies:
  - TASK-174
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/151'
priority: high
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Two interconnected regressions are affecting evaluation operations:

1) **Eval logs visibility gap**
- Evaluation logs in the Evaluations view are often empty unless the user first navigates to Flakes and clicks the running-eval chip.
- This indicates the log stream/subscription initialization is inconsistent across entry points.

2) **Eval queue reordering not affecting execution order**
- Reordering commits in the eval queue UI does not appear to change the actual evaluation order.
- Users expect queue priority/order changes to directly influence the next commit selected for evaluation.

## Goal
- Ensure eval logs are visible/streaming directly from Evaluations view without requiring navigation to Flakes.
- Ensure eval worker selection respects persisted queue order so manual reorder is authoritative.

## Non-Goals
- No redesign of build queue behavior.
- No broad refactor of websocket architecture beyond what is needed to make stream init reliable.
- No unrelated UX changes outside eval logs/queue ordering behavior.

## Architectural Constraints
- Preserve event-driven eval triggering behavior.
- Keep queue ordering source-of-truth in the database and use deterministic ordering when claiming next commit.
- UI should not contain business logic; server decides next commit based on queue state.

## Verification Plan
- Reproduce from clean browser session:
  - Open Evaluations directly while an eval is running and confirm logs appear without visiting Flakes.
- Queue ordering validation:
  - Create multiple pending commits, reorder them in UI, verify next evaluated commit follows reordered priority.
- Regression checks:
  - Existing eval websocket reconnect behavior still works.
  - Existing log verbosity toggle and maximize modal continue to function.

## Impact Areas
- packages/web-ui (Evaluations log stream initialization/state)
- packages/default (next-commit selection query/order semantics)
- API contract for queue reorder + queue listing if required

## Risk Level
- Medium: touches both UI stream init and queue selection logic; requires careful end-to-end validation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Opening Evaluations directly (without first visiting Flakes) shows live eval logs for the selected running commit.
- [ ] #2 Eval websocket/log stream is initialized consistently when selected commit changes in Evaluations.
- [ ] #3 Reordering pending commits in eval queue changes actual evaluation execution order for subsequent claims.
- [ ] #4 Server-side next-commit selection uses deterministic queue ordering that matches displayed queue order.
- [ ] #5 No regressions in existing eval log UI controls (collapse/expand, maximize modal, concise/verbose toggle, refresh/reconnect).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Execution order update: TASK-173 is now dependent on TASK-174 so eval-log and queue-ordering bug validation can run against deterministic mock eval/build mode.
<!-- SECTION:NOTES:END -->
