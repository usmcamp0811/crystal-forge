---
id: TASK-319
title: 'Create Compliance UI skeleton: Bundle → Control → Evidence → Waiver flow'
status: Backlog
assignee: []
created_date: '2026-05-24 02:34'
updated_date: '2026-05-24 02:39'
labels:
  - compliance
  - ui
  - dioxus
milestone: m-17
dependencies:
  - TASK-317
priority: medium
ordinal: 3180
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Without a dedicated compliance UX, users cannot inspect layered outcomes or evidence provenance even when backend data exists.

## Goal
Implement an initial compliance UI information architecture and pages for navigating bundles, controls, per-system evaluations, evidence details, and waiver state.

## Non-Goals
- No full design polish or custom reporting dashboards.
- No editing workflows beyond minimal read/view interactions unless backend already supports them.

## Architectural Constraints
- UI must keep business logic in backend/services; frontend should render DTOs.
- Reuse existing Crystal Forge layout/navigation conventions.
- Status rendering must preserve layered assertions (no lossy single badge only).

## Verification Plan
- nix develop -c cargo check --package default
- nix develop -c cargo test --package default web
- nix develop -c cargo test --package default handlers::api::compliance
- web-ui check updated with screenshot coverage for compliance flow

## Impact Areas
- packages/default/src/ui/**
- packages/default/src/handlers/api/** (DTO support)
- web-ui checks/tests

## Risk Level
Medium
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Navigation path exists for Compliance with bundle list and bundle detail pages.
- [ ] #2 Control detail view shows mapped policies, layered status fields, and current system-level evaluation summary.
- [ ] #3 Evidence panel shows evidence type/source/freshness/strength and provenance references.
- [ ] #4 Waiver panel displays active/expired waiver state for the selected control/system context.
- [ ] #5 Web UI verification includes screenshot(s) that capture the compliance flow and assertions.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Sprint sequencing: execute in Sprint 4 after TASK-317 and pair with web-ui screenshot assertions for compliance flow.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Risk-First Gate (Sprint 4 / UX): UI must preserve layered assertion visibility and evidence context; avoid flattening into a single pass/fail badge.
<!-- SECTION:NOTES:END -->
