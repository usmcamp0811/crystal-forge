---
id: TASK-411
title: >-
  Builds sidebar alert badge (64) not dismissing when viewing completed/failed
  builds
status: To Do
assignee: []
created_date: '2026-07-31 04:08'
updated_date: '2026-07-31 04:09'
labels:
  - builds
  - sidebar
  - alerts
  - web-ui
  - ux
dependencies: []
references:
  - TASK-385
  - TASK-391
documentation:
  - packages/web-ui/src/alerts/mod.rs
  - packages/web-ui/src/pages/builds.rs
  - packages/web-ui/src/components/shell/navigation.rs
priority: high
type: bug
ordinal: 400000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

On the dev server, the Builds sidebar navigation item shows a red alert badge with the number 64. When navigating to the Builds page and viewing the completed/failed builds, the badge count does not dismiss or update. The alert remains stuck at 64 regardless of user interaction with the builds.

## Current Behavior

- Builds sidebar shows persistent badge with count of 64
- Viewing the builds page does not clear or reduce the count
- Badge persists across navigation and page views

## Expected Behavior

The badge should acknowledge/dismiss when the user views the relevant failed/completed builds, reducing or clearing the count appropriately (even if only temporarily in the current session, per the existing in-memory acknowledgment logic from TASK-385).

## Environment

Observed on the dev server during normal usage.

## Related Work

This appears to be a regression or defect in the alert badge acknowledgment system implemented in TASK-385. The broader design question of persistent acknowledgment across refreshes is tracked separately in TASK-391, but the current issue is that dismissal is not working at all in the current session.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 When navigating to the Builds page, the sidebar alert badge count updates to reflect only unacknowledged failed builds
- [ ] #2 Clicking on the Completed tab in the Builds view acknowledges failed builds and reduces/clears the badge count
- [ ] #3 The acknowledge() function in packages/web-ui/src/alerts/mod.rs is properly called when viewing builds
- [ ] #4 The ALERT_STATE GlobalSignal updates correctly when builds are acknowledged
- [ ] #5 No console errors or warnings appear related to alert state management
- [ ] #6 Manual testing on dev server confirms badge dismisses from 64 to 0 (or appropriate count) when viewing failed builds
- [ ] #7 Verify acknowledgment works for multiple navigation flows: direct navigation, tab switching within builds view, and return navigation
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Investigation Areas

### Alert System Components (from TASK-385)
- `packages/web-ui/src/alerts/mod.rs` - Core alert state management and acknowledge()
- `packages/web-ui/src/components/shell/navigation.rs` - Sidebar navigation with badge rendering
- `packages/web-ui/src/pages/builds.rs` - Builds page that should trigger acknowledgment

### Key Questions to Answer
1. Is the `acknowledge(AlertCategory::BuildsFailed)` being called when viewing builds?
2. Is the acknowledgment being triggered at the right time (page mount, tab switch)?
3. Is the ALERT_STATE signal propagating the acknowledgment correctly?
4. Are the build counts being calculated correctly vs. the acknowledged state?

### Likely Root Causes
- Missing `use_effect` or event handler to call acknowledge() when builds view mounts
- Acknowledgment call placed incorrectly (wrong lifecycle, wrong tab)
- Alert category mismatch (using wrong AlertCategory variant)
- Signal not triggering re-render of navigation badge

### Verification Commands
```bash
# Run web-ui checks in the builds worktree
nix develop --command cargo test -p web-ui
nix develop --command cargo clippy -p web-ui

# Build and verify no compilation errors
nix build .#packages.x86_64-linux.web-ui

# Manual testing on dev server after fix
```

## Risk Assessment
Low-medium risk. This is a UI-only change affecting client-side state management. No database, API, or backend changes required. The existing alert infrastructure from TASK-385 should already be in place; this is likely a missing or incorrect acknowledgment trigger.
<!-- SECTION:NOTES:END -->
