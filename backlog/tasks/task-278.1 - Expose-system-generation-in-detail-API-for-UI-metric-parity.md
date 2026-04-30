---
id: TASK-278.1
title: Expose system generation in detail API for UI metric parity
status: In Progress
assignee:
  - '@ai-agent'
created_date: '2026-04-20 00:34'
updated_date: '2026-04-30 02:25'
labels:
  - ui
  - systems
  - api
milestone: UI/UX Design System
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/TASK-278-design-system-ui-ux/packages/web-ui/src/api/models.rs
  - >-
    /home/mcamp/code/crystal-forge/TASK-278-design-system-ui-ux/packages/web-ui/src/views/system_detail.rs
parent_task_id: TASK-278
priority: medium
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: TASK-278 design parity requires the System Detail metric strip to display a real Generation value, but the current web-ui SystemDetail DTO does not include a generation field and the UI currently renders a placeholder (#—).

Desired outcome: Add generation data to the relevant API/DTO path so the System Detail view can render an actual generation number (and related display text) without placeholders.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 System detail API/DTO exposes a generation field for the current deployed system state.
- [ ] #2 Web UI System Detail metric strip renders a non-placeholder generation value when data is available.
- [ ] #3 No regression in existing system detail fetch/parsing behavior when generation is absent (backward compatibility handled).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-278.1-expose-system-generation-api
<!-- SECTION:NOTES:END -->
