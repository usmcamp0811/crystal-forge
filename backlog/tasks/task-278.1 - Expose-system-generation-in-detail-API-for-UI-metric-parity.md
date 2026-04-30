---
id: TASK-278.1
title: Expose system generation in detail API for UI metric parity
status: Review
assignee:
  - '@ai-agent'
created_date: '2026-04-20 00:34'
updated_date: '2026-04-30 20:25'
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
Follow-up fix for reviewer merge blocker pushed in commit `0bc2f7e9`: removed independent latest-by-hostname lateral query in `get_system_detail_by_id` and now source generation fields from `view_system_detail` itself.

Added migration `0120_project_generation_from_view_system_detail_state_row.sql` to project `generation` and `generation_matches_current_store_path` from the same `latest_system_state` row as `current_store_path` in `view_system_detail`.

Added regression coverage in `packages/default/src/queries/systems.rs` verifying query shape and migration projection (`generation_projection_migration_updates_view_system_detail`).

Verified ingestion-path concern by code inspection: `deserialize_system_state_versioned` tries `SystemState` (current schema) first and only falls back to `SystemStateV1` compatibility path (which intentionally leaves generation fields null), so current agents can populate generation while legacy v1 payloads remain backward-compatible.
<!-- SECTION:NOTES:END -->
