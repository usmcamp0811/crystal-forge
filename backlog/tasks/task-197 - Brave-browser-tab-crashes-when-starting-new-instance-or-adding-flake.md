---
id: TASK-197
title: Brave browser tab crashes when starting new instance or adding flake
status: In Progress
assignee: []
created_date: '2026-03-19 12:38'
updated_date: '2026-03-28 13:58'
labels:
  - bug
  - ui
  - browser-compatibility
  - crash
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Crystal Forge web UI is causing Brave browser tabs to crash. The crash occurs either when:
- Starting a new Crystal Forge instance, OR
- Adding a new flake to an existing instance

The exact trigger is unclear and needs investigation.

## Current Behavior

- User loads Crystal Forge UI in Brave browser
- Tab crashes (either immediately on load or when adding a flake)
- Browser shows "Aw, Snap! Something went wrong" or equivalent crash message
- No clear error message about what caused the crash

## Expected Behavior

- UI should load and remain stable in Brave browser
- Adding flakes should not cause tab crashes
- Any errors should be gracefully handled without crashing the entire tab

## Investigation Needed

- [ ] Reproduce the crash consistently in Brave
- [ ] Check browser console for errors before crash
- [ ] Test in other Chromium-based browsers (Chrome, Edge)
- [ ] Test in Firefox to see if it's Chromium-specific
- [ ] Check for infinite loops, excessive re-renders, or memory leaks
- [ ] Review WebSocket connection handling
- [ ] Check for large data payloads that might overwhelm the browser
- [ ] Review Dioxus rendering logic for potential issues

## Potential Causes

- Memory leak in WebSocket connection or log streaming
- Infinite render loop in Dioxus components
- Large data payload causing browser OOM
- JavaScript interop issue specific to Chromium/Brave
- WASM memory issue
- React to state changes causing cascading updates

## Impact Areas

- Web UI (Dioxus frontend)
- WebSocket connections (especially eval logs)
- State management and component rendering
- Browser compatibility

## User Impact

**High** - Users on Brave browser (popular privacy-focused browser) cannot use the application at all if tabs crash.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Opening `/flakes` no longer freezes or crashes the browser in Firefox and Brave during a 2-minute observation window.
- [ ] #2 Flakes view first render completes and remains interactive (scroll, select flake, open details) with production-like dataset.
- [ ] #3 CPU usage does not spike into sustained runaway behavior caused by client-side render/update loops when entering `/flakes`.
- [ ] #4 Regression check: progressive timeline loading, generation-guard behavior, and rewrite warning modal flow still function.
- [ ] #5 A targeted UI test or integration check covers the failure path that previously caused lock-up.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Reproduce the freeze locally using current `dev` dataset/profile and capture browser console/performance symptoms.
2. Identify the hot path triggered by `/flakes` mount (likely polling/subscription/timeline batching/render loop interaction).
3. Apply minimal-scope fix in flakes view/client state flow to stop runaway updates while preserving existing UX.
4. Add/adjust targeted check to prevent recurrence.
5. Run targeted verification and prepare hotfix MR.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Goal: Restore flakes view stability immediately so entering `/flakes` does not lock the browser.

Non-goals:
- No redesign of flakes UX.
- No broad refactor outside flakes-view hot path.
- No unrelated performance work outside the lock-up root cause.

Architectural constraints:
- Keep business logic out of UI components.
- Keep fix scoped to flakes page state/effects and related API client paths only.
- Preserve existing server contracts and avoid schema/API changes unless strictly required.

Verification plan:
- Reproduce before fix, then verify no freeze after fix in local browser run.
- Run targeted check(s) for web UI flakes flow.
- Run `nix build .#checks.x86_64-linux.web-ui` if needed to validate integration-level behavior.

Impact areas: `packages/web-ui/src/views/flakes_list.rs`, related API client calls, web-ui checks.

Risk level: High (production browser lock-up, user-blocking).

Dependencies: None.

LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-197-fix-flakes-view-browser-lockup
<!-- SECTION:NOTES:END -->
