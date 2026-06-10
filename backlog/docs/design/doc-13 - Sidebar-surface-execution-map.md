---
id: doc-13
title: Sidebar surface execution map
type: specification
created_date: '2026-06-10 03:34'
updated_date: '2026-06-10 03:35'
tags:
  - planning
  - sidebar
  - epics
  - design-parity
---
# Sidebar surface execution map

## Purpose
This plan reorganizes UI/design work around the product navigation order so work is easy to scan top-to-bottom.

## Execution rule
Treat each major sidebar surface as an **epic/umbrella surface**.

Implementation still happens through **separate child tasks**, each with its own branch/worktree, but progress should be tracked surface-by-surface in this order.

## Sidebar-ordered surface map

### 1. Dashboard
Primary umbrella:
- `TASK-342` — remaining Dashboard parity cleanup

Related tasks:
- `TASK-321` — completed major dashboard parity work
- malformed duplicate dashboard record is tracked under `TASK-341`

### 2. Systems
Primary umbrella:
- `TASK-330` — Systems parity closure

Related tasks:
- system modal/edit/CVE cleanup tasks already completed historically

### 3. System Detail
Primary umbrella:
- `TASK-338` — System Detail parity umbrella

Related discrepancy tasks:
- `TASK-268` — History refinements
- `TASK-277` — logs day delineation/time filter
- `TASK-295` — tab icon parity

### 4. Environments
Primary umbrella:
- `TASK-339` — Environments parity task

### 5. Flakes
Primary umbrella:
- `TASK-343` — Flakes sidebar surface umbrella

Related discrepancy tasks:
- `TASK-297.1` — remove legacy flakes list
- any remaining flakes parity defects under `TASK-331`

### 6. Builds
Primary umbrella:
- `TASK-347` — Builds sidebar surface umbrella

Related discrepancy tasks:
- `TASK-275` — builds/evaluations coherence+density

### 7. Evaluations
Primary umbrella:
- `TASK-345` — Evaluations sidebar surface umbrella

Related discrepancy tasks:
- `TASK-275` — builds/evaluations coherence+density

### 8. CVEs / Scanning
Primary umbrella:
- `TASK-348` — CVEs and Scanning sidebar surface umbrella

Related discrepancy tasks:
- `TASK-327` — scanning worker policy enforcement
- `TASK-326` — completed scanning surface introduction

### 9. Policies
Primary umbrella:
- `TASK-340` — Policies parity task

### 10. Compliance
Primary umbrella:
- `TASK-344` — Compliance sidebar surface umbrella

Related implementation tasks:
- `TASK-320` — compliance proof system epic
- `TASK-319` — backend-backed compliance UI skeleton
- `TASK-334` — final compliance parity pass

### 11. Admin
Primary umbrella:
- `TASK-336` — Admin parity task

### 12. Builders
Primary umbrella:
- `TASK-346` — Builders sidebar surface umbrella

Related discrepancy tasks:
- `TASK-204` — builders view 403 fix
- `TASK-291` — builder concurrency/runtime configurability

### 13. Caches
Primary umbrella:
- `TASK-349` — Caches sidebar surface umbrella

Related discrepancy tasks:
- stale merged metadata for old caches parity work is tracked by `TASK-341`

## Cross-cutting foundations
These are not sidebar surfaces, but they gate multiple surfaces:
- `TASK-328` — parity matrix
- `TASK-329` — tokens/shell/shared primitives
- `TASK-332` — shared API contracts
- `TASK-333` — final screenshot/assertion harness
- `TASK-341` — backlog hygiene / duplicate metadata cleanup

## Working model
For each surface:
1. keep one umbrella task as the easy-to-read surface record
2. add detailed discrepancy tasks beneath it over time
3. land discrepancy tasks one by one
4. mark the umbrella done only when all intended child work is merged
