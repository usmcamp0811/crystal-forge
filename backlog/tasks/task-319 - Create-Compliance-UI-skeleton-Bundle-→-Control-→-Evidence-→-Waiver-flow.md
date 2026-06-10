---
id: TASK-319
title: 'Create Compliance UI skeleton: Bundle → Control → Evidence → Waiver flow'
status: Backlog
assignee: []
created_date: '2026-05-24 02:34'
updated_date: '2026-06-10 03:27'
labels:
  - compliance
  - ui
  - dioxus
milestone: 'm-17: Compliance Interop + UX'
dependencies:
  - TASK-317
documentation:
  - design/doc-11 - CrystalForgelatest-design-source-index.md
priority: medium
ordinal: 3180
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Without a dedicated compliance UX, users cannot inspect layered outcomes or evidence provenance even when backend data exists.

## Goal
Implement the initial backend-backed compliance information architecture for navigating bundles, controls, per-system evaluations, evidence details, and waiver state.

## Non-Goals
- No final CrystalForgelatest design-parity polish in this task; that belongs to TASK-334.
- No broad editing workflows beyond minimal read/view interactions unless backend already supports them.
- No replacement of evaluator/domain work with frontend-only placeholders.

## Scope
- Add Compliance navigation and route structure.
- Implement bundle list and bundle detail pages backed by real compliance DTOs.
- Implement control detail, evidence detail/panel, and waiver state presentation using evaluator outputs.
- Establish the baseline screenshot/assertion coverage needed before final design parity work.

## Architectural Constraints
- UI must keep business logic in backend/services; frontend should render DTOs.
- Reuse existing Crystal Forge layout/navigation conventions.
- Status rendering must preserve layered assertions (no lossy single badge only).

## Verification Plan
- `nix develop -c cargo check --package default`
- `nix develop -c cargo test --package default web`
- `nix develop -c cargo test --package default handlers::api::compliance`
- web-ui check updated with screenshot coverage for the compliance information flow

## Impact Areas
- packages/default/src/ui/**
- packages/default/src/handlers/api/** (DTO support)
- web-ui checks/tests

## Risk Level
Medium

## Dependencies
- Depends on TASK-317 for truthful evaluator-backed data.
- Should be completed before TASK-334 final parity polish.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Navigation path exists for Compliance with bundle list and bundle detail pages
- [ ] #2 Control detail view shows mapped policies, layered status fields, and current system-level evaluation summary
- [ ] #3 Evidence panel shows evidence type/source/freshness/strength and provenance references
- [ ] #4 Waiver panel displays active/expired waiver state for the selected control/system context
- [ ] #5 Web UI verification includes screenshots/assertions that capture the backend-backed compliance flow
- [ ] #6 No primary compliance surface in this task relies on placeholder-only data
<!-- AC:END -->
