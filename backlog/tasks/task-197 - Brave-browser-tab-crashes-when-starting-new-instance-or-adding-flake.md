---
id: TASK-197
title: Brave browser tab crashes when starting new instance or adding flake
status: Backlog
assignee: []
created_date: '2026-03-19 12:38'
updated_date: '2026-03-19 12:38'
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

**Firefox**: Does NOT crash - works correctly
**Brave**: Crashes (either on load or when adding flake)

This suggests the issue is **Chromium-specific** or **Brave-specific** (Brave is Chromium-based with additional privacy/security features).

Next steps for investigation:
1. Test in vanilla Chrome to see if it's Chromium-wide or Brave-specific
2. Test in Edge (also Chromium-based)
3. Check if Brave's Shield settings (ad blocking, script blocking) are interfering
4. Look for Chromium-specific rendering issues in Dioxus
5. Check if WASM memory limits differ between browsers
<!-- SECTION:NOTES:END -->
