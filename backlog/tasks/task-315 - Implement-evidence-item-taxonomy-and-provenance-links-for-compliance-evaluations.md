---
id: TASK-315
title: >-
  Implement evidence item taxonomy and provenance links for compliance
  evaluations
status: Backlog
assignee: []
created_date: '2026-05-24 02:33'
updated_date: '2026-05-24 02:38'
labels:
  - compliance
  - evidence
  - backend
milestone: m-16
dependencies:
  - TASK-308
  - TASK-314
priority: high
ordinal: 3140
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Evidence is currently not normalized for compliance assertions, making it difficult to prove why a control passed/failed and how strong/fresh that proof is.

## Goal
Implement a compliance evidence model with explicit kind/source/freshness/strength fields and provenance pointers to config/build/deploy/runtime artifacts.

## Non-Goals
- No full external format export in this task.
- No screenshot/doc attachment workflow in this task.

## Architectural Constraints
- Evidence must be attachable to control evaluations and queryable independently.
- Evidence strength taxonomy must distinguish declared config vs runtime observation vs human attestation.
- Provenance references should be stable IDs (not brittle text blobs).

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default compliance::evidence
- nix develop -c cargo test --package default queries::compliance

## Impact Areas
- packages/default/src/domain/**
- packages/default/src/queries/**
- packages/default/src/services/**
- migrations/**

## Risk Level
Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evidence item model includes kind, source, captured_at, freshness metadata, and strength classification.
- [ ] #2 Evidence items can be linked to control evaluations and retrieved by control/system/time window.
- [ ] #3 Provenance fields support references to flake commit, derivation/build, deployment generation, runtime check, and scan IDs.
- [ ] #4 Validation enforces allowed evidence kinds/strength values.
- [ ] #5 Automated tests verify persistence, retrieval filters, and taxonomy validation.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sprint sequencing: execute in Sprint 2 after TASK-314 model is stable. Validate retrieval by control/system/time window as sprint gate.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Risk-First Gate (Sprint 2 / Evidence): evidence must remain normalized with taxonomy enforcement and stable provenance IDs. No opaque blob-only records.
<!-- SECTION:NOTES:END -->
