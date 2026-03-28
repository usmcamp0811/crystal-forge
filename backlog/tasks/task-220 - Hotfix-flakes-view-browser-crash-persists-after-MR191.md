---
id: TASK-220
title: Hotfix flakes view browser crash persists after MR191
status: To Do
assignee: []
created_date: '2026-03-28 15:44'
updated_date: '2026-03-28 15:44'
labels:
  - bug
  - ui
  - hotfix
  - regression
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Browsers (Brave and Firefox) still freeze/crash when opening `/flakes` after MR191 was merged and deployed. This is a production-blocking regression.

## Goal

Identify and eliminate the remaining client-side lockup path so `/flakes` is stable under real data and background update load.

## Non-Goals

- No UI redesign of the flakes page.
- No unrelated refactors outside the flakes hot path.
- No schema/API contract changes unless strictly required for stability.

## Architectural Constraints

- Keep business logic out of UI components.
- Keep changes scoped to flakes state/effects/render paths and minimal related API calls.
- Preserve existing backend contracts unless a minimal server-side guard is necessary.

## Verification Plan

- Reproduce freeze locally with production-shaped timeline/system payload and live refresh behavior.
- Add/adjust targeted web-ui integration coverage to exercise the failing path.
- Run targeted frontend checks and `nix build .#checks.x86_64-linux.web-ui`.

## Impact Areas

- packages/web-ui/src/views/flakes_list.rs
- packages/web-ui/src/components/flake/* (if needed)
- checks/web-ui/tests/integration-test.js

## Risk

High (production UI becomes unusable on `/flakes`).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Opening `/flakes` does not freeze or crash Brave or Firefox during a 2-minute observation window.
- [ ] #2 Flakes page remains interactive while timeline data and background updates are processed (scroll/select/commit details still work).
- [ ] #3 No runaway render/effect loop occurs when mounting `/flakes` with production-shaped commit/system payload.
- [ ] #4 A targeted web-ui integration scenario reproduces the formerly crashing path and now passes reliably.
- [ ] #5 Existing rewrite modal and progressive timeline behaviors remain functional.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Promoted to To Do per user emergency hotfix request: browsers still crash on /flakes.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Hotfix committed and merged to dev.
- [ ] #2 Deployment verified by opening `/flakes` on production-like instance without browser lockup.
- [ ] #3 Any remaining out-of-scope findings captured as separate Backlog tasks.
<!-- DOD:END -->
