---
id: TASK-312
title: Create first-class Compliance Bundle and Control domain model
status: Backlog
assignee: []
created_date: '2026-05-24 02:32'
updated_date: '2026-05-24 02:38'
labels:
  - compliance
  - domain-model
  - backend
milestone: m-16
dependencies: []
priority: high
ordinal: 3110
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Current deployment policies can enforce gates, but Crystal Forge lacks a first-class model for compliance bundles and controls. Without this, STIG/NIST-aligned compliance is implicit and hard to audit.

## Goal
Introduce domain models and persistence for compliance bundles and controls so formal frameworks can be represented directly (not only inferred from policy sets).

## Non-Goals
- No UI implementation beyond minimal API compatibility.
- No import/export formats in this task.
- No full compliance evaluation engine in this task.

## Architectural Constraints
- Keep compliance domain separate from deployment policy execution internals.
- Maintain clear mapping boundaries between framework metadata and policy refs.
- Preserve existing deployment policy behavior.

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default compliance::bundle
- nix develop -c cargo test --package default queries::compliance

## Impact Areas
- packages/default/src/domain/**
- packages/default/src/queries/**
- packages/default/src/handlers/api/** (minimal)
- migrations/** (if schema required)

## Risk Level
Medium (new schema/domain objects with future dependency surface)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Compliance bundle model exists with id, name, framework, version, and metadata fields.
- [ ] #2 Compliance control model exists with id, title, severity, and framework mapping metadata.
- [ ] #3 Persistence/query layer supports create/read/list for bundles and controls.
- [ ] #4 Data model supports linking controls to bundles without duplicating control metadata.
- [ ] #5 Unit tests cover model validation and primary query behavior.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sprint sequencing: execute in Sprint 1 with TASK-313. Do not proceed to layered evaluator work until this gate passes.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Risk-First Gate (Sprint 1 / Model Integrity): verify bundle/control CRUD, stable identifiers/version fields, and strict validation for missing/invalid framework metadata before promoting dependent tasks.
<!-- SECTION:NOTES:END -->
