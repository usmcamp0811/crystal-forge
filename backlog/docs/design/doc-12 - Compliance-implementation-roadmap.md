---
id: doc-12
title: Compliance implementation roadmap
type: specification
created_date: '2026-06-10 03:28'
tags:
  - compliance
  - planning
  - roadmap
  - design-parity
---
# Compliance implementation roadmap

## Purpose
This roadmap clarifies how the existing compliance backlog fits together so execution can proceed without overlap between deployment-policy bridge work, first-class compliance domain work, and final UI parity work.

## Recommended sequencing

### Phase 0 — Policy bridge prerequisites
These tasks improve current deployment-policy auditability and can either land first or be explicitly superseded by the first-class compliance model if implementation converges.

- `TASK-307` — add compliance control mapping metadata to deployment policies
- `TASK-308` — add evidence capture model for deployment policy evaluations
- `TASK-309` — add waiver/exception workflow for deployment policy compliance gates

### Phase 1 — Compliance MVP domain and evaluator
This is the real backend foundation for a first-class compliance subsystem.

1. `TASK-312` — bundles and controls domain model
2. `TASK-313` — control-to-policy mapping model and APIs
3. `TASK-314` — layered control evaluation model
4. `TASK-315` — evidence taxonomy and provenance links
5. `TASK-316` — waiver lifecycle API
6. `TASK-317` — evaluator and persisted bundle/control rollups

### Phase 2 — Backend-backed UX and interop
These tasks should not start from placeholder UI assumptions.

- `TASK-319` — compliance UI skeleton / information architecture
- `TASK-318` — OHDF/HDF + OSCAL export endpoints

### Phase 3 — Final design parity
Once the backend-backed compliance UX exists, finish the CrystalForgelatest parity pass.

- `TASK-334` — final Compliance view parity

## Key overlap decisions

### TASK-309 vs TASK-316
- `TASK-309` is a deployment-policy-specific waiver bridge
- `TASK-316` is the first-class compliance waiver lifecycle
- Prefer implementing `TASK-309` in a way that can be migrated or folded into `TASK-316`

### TASK-308 vs TASK-315
- `TASK-308` adds structured evidence to current deployment policy results
- `TASK-315` adds normalized compliance evidence with provenance taxonomy
- Prefer stable IDs and taxonomy values in `TASK-308` so the later compliance evidence model can absorb or map them cleanly

### TASK-307 vs TASK-313
- `TASK-307` adds compliance metadata directly to deployment policies
- `TASK-313` introduces explicit control-to-policy mappings
- Treat `TASK-307` as a bridge, not the final long-term mapping architecture

## Readiness gates

### MVP gate
Do not treat Compliance as implemented until:
- `TASK-312` through `TASK-317` are complete
- control outcomes are persisted
- evidence and waiver semantics are queryable
- evaluator behavior is deterministic and fail-closed where required

### UX gate
Do not start final parity polish until:
- `TASK-319` exposes truthful backend-backed flows
- baseline web-ui screenshots/assertions exist for the compliance flow

### Final parity gate
Do not mark compliance design work complete until:
- `TASK-334` lands with CrystalForgelatest parity
- relevant `checks/web-ui` assertions/screenshots prove the intended flow

## Suggested milestone alignment
- `m-16` — compliance MVP backend foundation
- `m-17` — compliance interop + backend-backed UX
- `m-20` — final design parity pass for the missing Compliance surface

## Related docs
- `doc-10` — CrystalForgelatest parity execution plan
- `doc-11` — CrystalForgelatest design source index
