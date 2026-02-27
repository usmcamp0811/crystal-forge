---
id: TASK-130
title: Implement Flake Sync From Source Button Behavior
status: Review
assignee: []
created_date: '2026-02-26 08:45'
updated_date: '2026-02-27 05:10'
labels:
  - ui
  - flakes
  - sync
dependencies: []
priority: high
ordinal: 80000
---

## Problem

The Flakes view "Sync from Source" button does not enforce scoped sync behavior in the UI flow:

- If a flake is selected, users expect to sync only that flake.
- If no flake is selected, users expect a full sync across all tracked flakes.
- The control needs stronger visual emphasis for a primary destructive/refresh-style action.

## Goal

Implement deterministic sync behavior and clearer visual styling for the Flakes view sync action.

## Non-Goals

- Changing backend sync semantics beyond existing sync endpoints.
- Reworking flake selection behavior outside sync action handling.
- Refactoring unrelated Flakes view layout/components.

## Acceptance Criteria

1. If a flake is selected in the Flakes view, pressing "Sync from Source" triggers sync for only that flake.
2. If no flake is selected, pressing "Sync from Source" triggers sync for all flakes.
3. Sync action success/error messaging remains clear and accurate for scoped vs full sync paths.
4. Sync button uses a filled/emphasized style aligned with the existing theme (danger-style token as requested).

## Architectural Constraints

- Keep business logic in view/state handlers, not presentational-only component fragments.
- Reuse existing API client methods where possible.
- Preserve separation of UI and infrastructure concerns.

## Verification Plan

- `nix develop -c cargo check` (from `packages/web-ui`)
- Manual behavior check in Flakes view for both selected and unselected scenarios.

## Impact Areas

- `packages/web-ui/src/views/flakes_list.rs`
- `packages/web-ui/src/api/client.rs` (only if scoped endpoint call helpers are needed)
- `packages/web-ui/src/theme.rs` (if button token usage needs adjustment)

## Risk Level

Low: isolated UI action behavior and styling change.

## Dependencies

None.

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-130-implement-flake-sync-from-source

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/138

Commit: c3984c4
<!-- SECTION:NOTES:END -->

## Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: OpenCode on gray in ~/code/crystal-forge/TASK-130-implement-flake-sync-from-source
MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/138
<!-- SECTION:NOTES:END -->
