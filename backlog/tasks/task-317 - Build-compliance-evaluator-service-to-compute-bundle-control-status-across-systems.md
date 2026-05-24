---
id: TASK-317
title: >-
  Build compliance evaluator service to compute bundle/control status across
  systems
status: Backlog
assignee: []
created_date: '2026-05-24 02:33'
updated_date: '2026-05-24 02:39'
labels:
  - compliance
  - evaluation-engine
  - backend
milestone: m-16
dependencies:
  - TASK-313
  - TASK-314
  - TASK-315
  - TASK-316
priority: high
ordinal: 3160
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Crystal Forge lacks a unified evaluator that composes policy outcomes, evidence, and waivers into auditable control-level compliance decisions.

## Goal
Add a compliance evaluator service that computes control and bundle status per system with explicit reasoning and per-layer outcomes.

## Non-Goals
- No heavy reporting/export in this task.
- No broad UI implementation in this task.

## Architectural Constraints
- Evaluator must be deterministic and replayable for historical snapshots.
- Service layer owns aggregation logic; handlers remain thin.
- Must support fail-closed semantics when required evidence or referenced policy data is unavailable.

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default services::compliance_evaluator
- nix develop -c cargo test --package default compliance::integration

## Impact Areas
- packages/default/src/services/**
- packages/default/src/domain/**
- packages/default/src/queries/**

## Risk Level
High (core decision engine)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluator computes per-control status using layered assertions and waiver state.
- [ ] #2 Evaluator computes per-bundle rollup status with transparent reason metadata.
- [ ] #3 Evaluation results are persisted and queryable with timestamps/history.
- [ ] #4 Missing required policy/evidence inputs are handled with fail-closed behavior and explicit error reasoning.
- [ ] #5 Integration tests cover mixed outcomes (pass/warn/fail/waived) across multiple systems.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sprint sequencing: execute after TASK-316 in Sprint 3. This task is the MVP readiness gate before any interop/UI work.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Risk-First Gate (Sprint 3 / Evaluator): deterministic replayable rollups, fail-closed handling for missing policy/evidence inputs, and explicit reason metadata are mandatory.
<!-- SECTION:NOTES:END -->
