---
id: TASK-320
title: >-
  Epic: Compliance Proof System (Bundles, Evidence, Waivers, Evaluator, Interop,
  UI)
status: Backlog
assignee: []
created_date: '2026-05-24 02:39'
updated_date: '2026-06-10 03:27'
labels:
  - compliance
  - epic
  - planning
milestone: 'm-22: Compliance MVP - Prove, Don''t Just Display'
dependencies: []
documentation:
  - design/doc-10 - CrystalForgelatest-parity-execution-plan.md
  - design/doc-11 - CrystalForgelatest-design-source-index.md
priority: high
ordinal: 3105
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge policy enforcement exists, but the platform still needs a complete compliance-proof system to represent formal controls, prove outcomes with layered evidence, and support audit interoperability.

## Goal
Deliver a coherent compliance subsystem that sits between policy execution and external audit ecosystems: first-class bundles/controls, control-policy mappings, layered evaluations, normalized evidence, waiver governance, evaluator rollups, interoperability, and UI surfaces.

## Non-Goals
- Becoming an official STIG authoring platform.
- Replacing external ecosystem tools for profile authoring or broad GRC workflows.

## Architectural Constraints
- Keep compliance modeling/evaluation separate from deployment-policy runtime internals.
- Preserve fail-closed semantics for required compliance decisions.
- Maintain historical traceability to flake/build/deploy/runtime identifiers.

## Delivery Structure
### Phase 0 — Policy bridge prerequisites
- TASK-307: add compliance metadata to deployment policies
- TASK-308: add structured evidence capture to policy evaluations
- TASK-309: add waiver/exception workflow for deployment policy gates

These tasks are bridge work that makes current deployment-policy behavior more audit-friendly, but they do not replace the first-class compliance domain.

### Phase 1 — Compliance MVP domain and evaluator
- TASK-312: bundles and controls domain model
- TASK-313: control-to-policy mapping model/APIs
- TASK-314: layered control evaluation model
- TASK-315: evidence taxonomy and provenance links
- TASK-316: waiver lifecycle API
- TASK-317: compliance evaluator service

### Phase 2 — UX and interop
- TASK-319: backend-backed compliance UI skeleton / information architecture
- TASK-318: OHDF/HDF and OSCAL export endpoints
- TASK-334: final CrystalForgelatest compliance-view parity pass

## Verification Plan
- Completion is tracked through child task acceptance criteria and verification plans.
- MVP readiness gate: TASK-312 through TASK-317 complete with backend verification.
- UX readiness gate: TASK-319 exposes truthful backend-backed flows before final parity polish.
- Interop/export readiness gate: TASK-318 validates export fidelity against stored evaluations/evidence.

## Impact Areas
- domain/query/service layers for compliance
- API DTO/endpoints for compliance objects/results
- UI navigation and evidence detail surfaces
- export adapters for OHDF/HDF and OSCAL

## Risk Level
High (cross-cutting product capability)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Compliance MVP exits with TASK-312 through TASK-317 completed and validated
- [ ] #2 Bridge tasks TASK-307 through TASK-309 are either completed or explicitly superseded by the first-class compliance domain design
- [ ] #3 Compliance UI skeleton and final parity work are sequenced so backend-backed flows exist before final design polish
- [ ] #4 Compliance subsystem can explain pass/warn/fail/waived outcomes with traceable evidence and waiver links
- [ ] #5 Interop/export tasks preserve identifiers and evidence provenance without recomputing compliance outcomes
<!-- AC:END -->
