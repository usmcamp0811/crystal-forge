---
id: TASK-296
title: Fix evaluation log modal rendering in main page instead of as overlay
status: Backlog
assignee: []
created_date: '2026-05-13 02:28'
labels:
  - bug
  - ui
  - evaluations
dependencies: []
priority: medium
ordinal: 251000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The evaluation log modal on the evaluations view is not rendering correctly as a modal overlay. Instead, it appears embedded in the main page content.

**Current Behavior:**
When clicking to view evaluation logs, the log content appears inline within the main page rather than as an overlay modal dialog.

**Expected Behavior:**
The evaluation log should appear as a proper modal overlay that:
- Appears above the main content with a backdrop
- Can be dismissed by clicking outside or using a close button
- Does not push other page content around

**Context:**
This is affecting the evaluations view user experience and may be related to modal component implementation or CSS/layout issues.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Evaluation log displays as a proper modal overlay with backdrop
- [ ] #2 Modal appears centered on screen above main content
- [ ] #3 Modal can be dismissed by clicking backdrop or close button
- [ ] #4 Main page content does not shift when modal opens
- [ ] #5 Modal styling is consistent with other modals in the application
<!-- AC:END -->
