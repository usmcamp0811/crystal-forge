---
id: TASK-313
title: Implement control-to-policy mapping model and APIs
status: Backlog
assignee: []
created_date: '2026-05-24 02:33'
updated_date: '2026-05-24 02:38'
labels:
  - compliance
  - policy-mapping
  - backend
  - api
milestone: m-16
dependencies:
  - TASK-307
  - TASK-312
priority: high
ordinal: 3120
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Even with controls defined, Crystal Forge needs explicit and auditable mappings from controls to enforceable deployment policies. Today this relationship is incomplete and not first-class.

## Goal
Add a mapping layer that relates compliance controls to one or more Crystal Forge policies, including mapping rationale and expected evidence requirements.

## Non-Goals
- Do not implement full control evaluation aggregation.
- Do not implement waiver lifecycle in this task.

## Architectural Constraints
- Mapping must be many-to-many (one control can map to multiple policies and one policy can satisfy multiple controls).
- Mapping records must be immutable/auditable enough for historical review.
- Avoid embedding business logic in handlers.

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default compliance::mapping
- nix develop -c cargo test --package default handlers::api::compliance

## Impact Areas
- packages/default/src/domain/**
- packages/default/src/queries/**
- packages/default/src/handlers/api/**
- migrations/**

## Risk Level
Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A control-to-policy mapping table/model exists and supports many-to-many relationships.
- [ ] #2 Each mapping stores rationale/notes and required evidence categories.
- [ ] #3 API endpoints exist to create/list/remove mappings for a control.
- [ ] #4 Validation prevents mapping to non-existent controls or policies.
- [ ] #5 Automated tests cover mapping CRUD and validation behavior.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sprint sequencing: complete with TASK-312 in Sprint 1. Include integration test for bundle+control+mapping linkage.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Risk-First Gate (Sprint 1 / Model Integrity): mapping cardinality and referential integrity must be proven (control↔policy many-to-many, no orphan mappings). Hard gate before TASK-314.
<!-- SECTION:NOTES:END -->
