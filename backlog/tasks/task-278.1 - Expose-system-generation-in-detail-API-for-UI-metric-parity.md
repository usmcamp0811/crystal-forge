---
id: TASK-278.1
title: Expose system generation in detail API for UI metric parity
status: Review
assignee:
  - '@ai-agent'
created_date: '2026-04-20 00:34'
updated_date: '2026-04-30 19:58'
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
Follow-up fix pushed in MR iteration: commit `2bb0e42f` renumbers task migrations from `0113/0114` to `0118/0119` to avoid collision with existing dev migrations (`0113_add_eval_cancellation_support`, `0114_create_hardening_tables`).

Updated test migration include reference in `packages/default/src/queries/systems.rs` to `0118_add_generation_to_system_states.sql`.

Local verification executed after rename: `nix develop -c env SQLX_OFFLINE=true cargo check` (packages/default), `nix develop -c env SQLX_OFFLINE=true cargo test queries::systems::tests::generation_migration_adds_generation_column_to_system_states` (packages/default), `nix develop -c env SQLX_OFFLINE=true cargo test queries::systems::tests::system_detail_query_does_not_derive_generation_from_store_path_regex` (packages/default), `nix develop -c cargo check` (packages/web-ui).
<!-- SECTION:NOTES:END -->
