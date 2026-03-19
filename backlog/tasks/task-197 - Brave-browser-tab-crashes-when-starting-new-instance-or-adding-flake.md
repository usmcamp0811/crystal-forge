---
id: TASK-197
title: Brave browser tab crashes when starting new instance or adding flake
status: Backlog
assignee: []
created_date: '2026-03-19 12:38'
updated_date: '2026-03-19 12:39'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Browser Testing Notes (2026-03-19)

**Initial report**: Firefox seemed fine, but crash MAY also occur in Firefox
**Brave**: Crashes (either on load or when adding flake)
**Firefox**: Uncertain - may also crash, needs more testing

**Status**: Issue is NOT browser-specific - appears to be a general UI/rendering problem that can affect multiple browsers.

This suggests the root cause is likely:
- Memory leak or excessive memory usage
- Infinite render loop in Dioxus components
- Large data payload causing browser OOM
- WASM memory issue
- Cascading state updates causing excessive re-renders

Next steps for investigation:
1. Try to reproduce crash consistently in both browsers
2. Monitor browser memory usage when loading UI
3. Check browser console for errors/warnings before crash
4. Look for infinite loops or excessive re-renders in Dioxus components
5. Profile memory usage and component render cycles
6. Check if adding flake triggers large data fetch or render loop
<!-- SECTION:NOTES:END -->
