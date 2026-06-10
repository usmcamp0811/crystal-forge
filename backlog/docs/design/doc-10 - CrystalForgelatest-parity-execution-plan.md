---
id: doc-10
title: CrystalForgelatest parity execution plan
type: specification
created_date: '2026-06-10 02:59'
tags:
  - design-parity
  - planning
  - crystalforgelatest
  - milestones
---
# CrystalForgelatest parity execution plan

## Recommended delivery strategy
Use **foundation first, then vertical slices**:
1. Finish shared parity contract and shell/token baseline.
2. Do each surface as a **UI + required real backend data** slice.
3. Reserve final screenshot/assertion completeness for a closing audit pass.

This minimizes rework better than placeholder-first UI while still keeping visible progress moving.

## Milestone plan

### m-18: Design Parity Foundation
Purpose: establish the rules and shared primitives that prevent later rework.

Primary tasks:
- TASK-328 — parity matrix and interaction inventory
- TASK-329 — shell/tokens/shared primitives parity
- TASK-332 — shared API contract gaps only

### m-19: Design Parity Existing Surfaces
Purpose: bring already-existing product surfaces to parity through vertical slices.

Primary tasks:
- TASK-330 — Systems parity closure
- TASK-331 — Flakes / Builds / Evals / CVEs / Caches parity closure
- TASK-338 — System Detail parity umbrella
- TASK-277 — stale log/time-filter work should be reconciled under this milestone if retained

Execution note:
- Prefer completing one surface fully enough to prove parity before starting the next.
- Surface-specific API gaps should usually land inside the owning slice instead of expanding TASK-332.

### m-20: Design Parity Missing Surfaces
Purpose: add or complete design-reference surfaces that are missing or materially incomplete.

Primary tasks:
- TASK-334 — Compliance
- TASK-335 — Profile
- TASK-336 — Admin
- TASK-339 — Environments parity task
- TASK-340 — Policies parity task

### m-21: Design Parity Final Audit
Purpose: close the loop with objective evidence and scorecard updates.

Primary tasks:
- TASK-333 — full screenshot/assertion harness closure
- re-score against `doc-9`
- document any accepted deltas explicitly

## Prioritized execution order
1. TASK-328
2. TASK-329
3. TASK-332
4. TASK-330
5. TASK-338
6. TASK-331
7. TASK-334 / TASK-336 / TASK-335 / TASK-339 / TASK-340
8. TASK-333

## Cleanup notes captured during backlog maintenance
- Several stale In Progress/Review items had already merged and were moved toward Done/completed cleanup.
- Deleted worktrees were pruned.
- Some historical task metadata remains inconsistent, especially duplicate active task IDs and malformed legacy task files; these should be corrected before heavy future grooming.
