---
id: TASK-231
title: Repair TASK-229 merge so commit-path detail changes actually land on dev
status: In Progress
assignee: []
created_date: '2026-03-31 03:07'
updated_date: '2026-03-31 03:08'
labels:
  - repair
  - flakes
  - ui
  - backend
  - merge-integrity
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Merge request 198 is marked merged, but `dev` does not contain the TASK-229 implementation commits. The backend/UI code for Flakes commit path details (including host-scoped current-path semantics and `expected_store_path` sourcing) is missing on `dev`.

## Goal
Land the intended TASK-229 code changes onto `dev` via a clean follow-up merge request, preserving the reviewed behavior and fixing merge integrity.

## Non-Goals
- No new feature scope beyond the already reviewed TASK-229 behavior.
- No refactor of unrelated flakes/dashboard code.
- No schema/migration changes.

## Acceptance Criteria
1. A follow-up branch contains the TASK-229 code changes missing from `dev` (API model, handler projection/query, web-ui model, and commit-details rendering).
2. Backend query projects expected path from `derivations.expected_store_path` (not `derivations.store_path`).
3. Current path remains host-scoped with explicit host/multi-host context in UI.
4. Targeted verification passes for backend compile/tests and web-ui check used for UI validation.
5. A new MR targeting `dev` is opened with verification evidence and UI screenshot attachment from the web-ui check.

## Architectural Constraints
- Keep business logic in backend, not UI presentation layer.
- Preserve DTO compatibility with `#[serde(default)]` where needed.
- Keep changes minimal and restricted to TASK-229 intent.

## Verification Plan
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- `nix develop -c env SQLX_OFFLINE=true cargo test --package crystal-forge build_commit_system_paths_includes_path_details_and_unavailable_states`
- `nix develop -c cargo test --package crystal-forge-ui build_flake_commits_preserves_system_path_details`
- `nix build .#checks.x86_64-linux.web-ui --no-link`

## Impact Areas
- `packages/default/src/handlers/api/flakes.rs`
- `packages/default/src/api/models.rs`
- `packages/web-ui/src/api/models.rs`
- `packages/web-ui/src/views/flakes_list.rs`

## Risk Level
Medium (operator-facing path diagnostics can mislead if wrong values/wording are shown).
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved Backlog -> To Do per explicit human sprint selection in chat to repair missing TASK-229 changes on dev.

LOCK: opencode-gpt5 on reckless in /home/mcamp/code/crystal-forge/TASK-231-repair-task229-merge
<!-- SECTION:NOTES:END -->
