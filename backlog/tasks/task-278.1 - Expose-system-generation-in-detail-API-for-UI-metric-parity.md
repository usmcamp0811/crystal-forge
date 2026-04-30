---
id: TASK-278.1
title: Expose system generation in detail API for UI metric parity
status: Review
assignee:
  - '@ai-agent'
created_date: '2026-04-20 00:34'
updated_date: '2026-04-30 03:29'
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
- [x] #1 System detail API/DTO exposes a generation field for the current deployed system state.
- [x] #2 Web UI System Detail metric strip renders a non-placeholder generation value when data is available.
- [x] #3 No regression in existing system detail fetch/parsing behavior when generation is absent (backward compatibility handled).
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/245

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check` (packages/default), `nix develop -c cargo check` (packages/web-ui), `nix build .#checks.x86_64-linux.web-ui` (includes `12i-system-detail-generation-metric.png`).
<!-- SECTION:NOTES:END -->
