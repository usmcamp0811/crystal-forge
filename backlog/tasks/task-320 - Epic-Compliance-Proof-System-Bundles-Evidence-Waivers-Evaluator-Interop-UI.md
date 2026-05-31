---
id: TASK-320
title: >-
  Epic: Compliance Proof System (Bundles, Evidence, Waivers, Evaluator, Interop,
  UI)
status: Backlog
assignee: []
created_date: '2026-05-24 02:39'
labels:
  - compliance
  - epic
  - planning
milestone: m-16
dependencies: []
priority: high
ordinal: 3105
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge policy enforcement exists, but the platform still needs a complete compliance-proof system to represent formal controls, prove outcomes with layered evidence, and support audit interoperability.

## Goal
Deliver a coherent compliance subsystem that sits between policy execution and external audit ecosystems: first-class bundles/controls, control-policy mappings, layered evaluations, evidence taxonomy, waiver governance, evaluator rollups, and interoperability/UI surfaces.

## Non-Goals
- Becoming an official STIG authoring platform.
- Replacing external ecosystem tools for profile authoring or broad GRC workflows.

## Architectural Constraints
- Keep compliance modeling/evaluation separate from deployment-policy runtime internals.
- Preserve fail-closed semantics for required compliance decisions.
- Maintain historical traceability to flake/build/deploy/runtime identifiers.

## Verification Plan
- Completion is tracked through child task acceptance criteria and verification plans.
- Milestone gate checks must pass at Sprint 1/2/3/4 boundaries.

## Impact Areas
- domain/query/service layers for compliance
- API DTO/endpoints for compliance objects/results
- UI navigation and evidence detail surfaces
- export adapters for OHDF/HDF and OSCAL

## Risk Level
High (cross-cutting product capability)

## Child Scope
- TASK-312, TASK-313, TASK-314, TASK-315, TASK-316, TASK-317, TASK-318, TASK-319
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Milestone 'Compliance MVP - Prove, Don't Just Display' exits with TASK-312 through TASK-317 completed and validated.
- [ ] #2 Milestone 'Compliance Interop + UX' exits with TASK-318 and TASK-319 completed and validated.
- [ ] #3 Risk-first sprint gates are documented on child tasks and used during sprint reviews.
- [ ] #4 Compliance subsystem can explain pass/warn/fail/waived outcomes with traceable evidence links.
<!-- AC:END -->
