---
id: TASK-25
title: Dashboard build queue widget
status: Done
assignee: []
created_date: '2026-02-15 19:17'
updated_date: '2026-02-19 04:06'
labels:
  - ui
  - dashboard
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add dashboard widget showing active builds and queued items, plus timeline markers for build activity (using mock data initially).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added build queue widget + timeline build markers with mock data. Updated dashboard/server DTOs to include optional build_queue, plus handler/test builder adjustments. Ran: nix develop -c bash -c "cd packages/default && SQLX_OFFLINE=true cargo test --lib" (passes; existing warnings remain).

Reworked timeline build indicator to colored ring around commit node. Build queue now sorts in execution order (active first, then queued) and labels rows as Active/Next/Queued #n for clarity.

Added build ring legend entries for Building/Queued. Adjusted drag drop reflow to pack rows without collisions so widgets rearrange when moved.

Closed after in-progress review: build queue widget and timeline markers are implemented and reflected in UI/component code and prior task notes.
<!-- SECTION:NOTES:END -->
