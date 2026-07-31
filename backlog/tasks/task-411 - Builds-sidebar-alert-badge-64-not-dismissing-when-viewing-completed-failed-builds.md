---
id: TASK-411
title: >-
  Builds sidebar alert badge (64) not dismissing when viewing completed/failed
  builds
status: Backlog
assignee: []
created_date: '2026-07-31 04:08'
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
- [ ] #1 Viewing the Builds page and the Completed tab with failed builds reduces or clears the sidebar badge count
- [ ] #2 Badge acknowledgment works correctly for the current browser session (matches TASK-385 behavior)
- [ ] #3 Verify the acknowledge() function in alerts module is being called when navigating to/viewing failed builds
- [ ] #4 Test that the ALERT_STATE signal is properly updating when builds are viewed
- [ ] #5 Confirm no console errors or warnings related to alert state management
<!-- AC:END -->
