---
id: TASK-41
title: Builds control center UI
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-17 04:42'
updated_date: '2026-02-21 03:28'
labels:
  - ui
  - web-ui
  - builds
milestone: m-11
dependencies: []
priority: high
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement Builds view as operational control center with global and per-worker controls, queue list, build detail panel, and modal-confirmed actions (start/pause/drain/cancel/restart/prioritize).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented a full Builds control center UI in packages/web-ui/src/views/builds.rs: global queue controls (Start All/Pause All/Drain All), per-worker controls, queue list with build actions (Stop/Restart/Run Next), split-pane detail view with tabs (Live Logs, Events, Artifacts), log controls (follow/pause/wrap/search), and confirmation modals for queue/worker/build actions. Added realistic mock states for multi-worker operation and action state transitions.
<!-- SECTION:NOTES:END -->
