---
id: TASK-270
title: 'Refine System Logs view: filtering, sticky tabs, full logs action'
status: Backlog
assignee: []
created_date: '2026-04-14 01:31'
updated_date: '2026-04-14 01:32'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The System Detail → Logs tab needs similar UX improvements to the History view (TASK-268):

1. **Filter not working** — The filter options at the top of the Logs view don't actually filter the displayed events.

2. **Tabs don't stick** — When scrolling through a long log list, the tab navigation (Overview | History | Logs) scrolls away. It should stay visible like in the History view.

3. **"View full logs" does nothing** — There's a "View full logs" link/button that doesn't actually do anything meaningful.

## Goal

Refine the Logs view with functional filters, sticky tab navigation, and working "View full logs" action.

## Scope

1. **Functional filters** — Make filter options actually filter the log events. Common filter types for agent/system logs:
   - Event type (deployment, heartbeat, error, etc.)
   - Severity level (info, warning, error)
   - Date/time range
   - Search in log message text

2. **Sticky tabs** — Keep the tab navigation (Overview | History | Logs) visible while scrolling through logs, similar to the History view fix.

3. **"View full logs" action** — Make this actually do something useful:
   - Could open a full-page log viewer
   - Could download logs as file
   - Could open external log viewer/integration
   - At minimum should show all logs, not just a preview

4. **Design consistency** — Match site design system for filters, buttons, and layout.

## Non-Goals

- No changes to log ingestion or storage
- No redesign of log entry content

## Impact Areas

- `packages/web-ui/src/views/system_detail.rs` (Logs tab component)
- `packages/web-ui/src/components/system/logs*` (if separate component)
<!-- SECTION:DESCRIPTION:END -->
