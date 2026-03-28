---
id: TASK-220
title: Hotfix flakes view browser crash persists after MR191
status: In Progress
assignee: []
created_date: '2026-03-28 15:44'
updated_date: '2026-03-28 16:02'
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

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-220-fix-flakes-browser-crash

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/192

Applied hotfix commit ed84201a (same stabilization pattern as prior patch) onto fresh post-merge dev state for emergency redeploy.

LOCK: claude-sonnet-4-5 on reckless in /home/mcamp/code/crystal-forge/TASK-220-fix-flakes-browser-crash

Diagnosis: MR192 was a duplicate of MR191. Real issue is cascading re-renders from:
- Unmemoized build_flake_commits() running on every render
- Timeline batch updates triggering multiple signal updates
- FlakeHistoryExplorer re-rendering for each batch

Fix approach:
1. Memoize commit building
2. Debounce timeline updates
3. Guard early renders

Starting implementation now.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Hotfix committed and merged to dev.
- [ ] #2 Deployment verified by opening `/flakes` on production-like instance without browser lockup.
- [ ] #3 Any remaining out-of-scope findings captured as separate Backlog tasks.
<!-- DOD:END -->
