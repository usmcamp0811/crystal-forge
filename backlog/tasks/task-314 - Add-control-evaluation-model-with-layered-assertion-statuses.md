---
id: TASK-314
title: Add control evaluation model with layered assertion statuses
status: Backlog
assignee: []
created_date: '2026-05-24 02:33'
updated_date: '2026-05-24 02:38'
labels:
  - compliance
  - evaluation
  - backend
milestone: m-16
dependencies:
  - TASK-312
  - TASK-313
priority: high
ordinal: 3130
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Current policy decisions are deployment-centric and do not produce a durable compliance control evaluation object with layered assertion semantics.

## Goal
Create a control evaluation model that records per-layer statuses (desired state, build provenance, deployed state, runtime state, vulnerability state, waiver state) and computes an overall control status.

## Non-Goals
- Do not add full reporting/export formats.
- Do not add broad UI beyond API payload support.

## Architectural Constraints
- Evaluation records must reference system + optional eval/build/deployment identifiers.
- Layered statuses must be preserved (no lossy flattening into pass/fail only).
- Overall status computation logic must be centralized in domain/service layer.

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default compliance::evaluation
- nix develop -c cargo test --package default services::compliance

## Impact Areas
- packages/default/src/domain/**
- packages/default/src/services/**
- packages/default/src/queries/**
- migrations/**

## Risk Level
Medium-High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Control evaluation entity exists with layered assertion fields and overall status.
- [ ] #2 Evaluation rows can be persisted and queried by control, bundle, and system.
- [ ] #3 Overall status algorithm is deterministic and tested for mixed pass/warn/fail/waived cases.
- [ ] #4 Evaluation records include references to relevant build/deployment/eval identifiers when available.
- [ ] #5 Unit tests cover edge cases for status aggregation and missing-layer behavior.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sprint sequencing: execute in Sprint 2 with TASK-315 sequencing. Add table-driven aggregation tests before TASK-317.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Risk-First Gate (Sprint 2 / Semantics): preserve layered statuses without lossy flattening. Aggregation must be deterministic with explicit mixed pass/warn/fail/waived tests.
<!-- SECTION:NOTES:END -->
