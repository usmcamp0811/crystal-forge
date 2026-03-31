---
id: TASK-229
title: Show expected/current config store paths in Flakes commit details
status: Review
assignee: []
created_date: '2026-03-30 03:17'
updated_date: '2026-03-31 01:54'
labels:
  - flakes
  - ui
  - backend
  - deployment
  - sprint-ready
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
In the Flakes view, commit details currently show system configuration names but not the resolved store paths per configuration. Operators cannot inspect expected path data from processed commits alongside agent-reported current paths from one place.

## Goal
Update Flakes commit details so each listed system configuration can show path details (expected path from eval/processed commit data and current path reported by matching agent system when available).

## Non-Goals
- Do not change deployment-status classification logic (up-to-date/unknown behavior is out of scope for this task).
- No change to agent payload schema.
- No broad redesign of the Flakes page beyond commit-details presentation.
- No replacement of build-complete `store_path` behavior.

## Scope
- Extend commit-details data model/API to include per-configuration path details.
- Display expected path values for each config in commit details.
- Display current agent-reported path context when the config maps to a Crystal Forge system.
- Keep missing-path states explicit and readable.

## Architectural Constraints
- Keep business logic in backend query/service layers; UI presents mapped data.
- Reuse existing source-of-truth semantics for expected path vs built path vs current path.
- Preserve DTO boundary alignment between server and web UI.

## Verification Plan
- Backend/API tests for per-config path fields in commit-details responses.
- UI test coverage for rendering config names plus expected/current paths.
- Targeted Nix dev checks for touched backend and web-ui modules.

## Impact Areas
- Flakes timeline query/handler commit payload projection
- API models used by server and web-ui
- Flakes commit-details component(s) in web UI

## Risk Level
Medium (incorrect path projection could mislead operators).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Commit details display each listed system configuration with an expected store path when available from processed commit/eval data.
- [ ] #2 Commit details display an agent-reported current store path when a Crystal Forge system maps to that configuration.
- [ ] #3 Commit details keep configuration names visible while adding path details in a readable layout (tab/section/panel acceptable).
- [ ] #4 Missing expected/current paths are rendered explicitly (e.g., unavailable/not reported) instead of omitted silently.
- [ ] #5 Backend/API and UI tests cover path-present and path-missing scenarios.
- [ ] #6 Task notes document exactly which path sources are shown in commit details and in what precedence.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved Backlog -> To Do per explicit human sprint selection request in chat.

Task authored to Sprint-Ready quality: includes problem, goal, non-goals, constraints, verification plan, impact areas, risk, dependencies, and objective acceptance criteria.

LOCK: opencode-gpt5 on reckless in /home/mcamp/code/crystal-forge/TASK-229-commit-config-paths

Scope adjustment per user: remove up-to-date/unknown status-fix expectations from this task; focus only on exposing path details in Flakes commit details.

Implementation detail: commit-details now project per-config path data with precedence `expected_store_path := derivations.store_path for commit/config`, `current_store_path := latest system_states.store_path for mapped active CF system (flake_id + system_configuration_name|hostname match)`.

UI behavior: Flakes commit details retain config names and now render expected/current path lines per config, including explicit `unavailable` / `not reported` states when data is missing.

Verification run (non-production): `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge` PASS; `nix develop -c env SQLX_OFFLINE=true cargo test --package crystal-forge mark_cf_system_matches_appends_marker_when_config_maps_to_cf_system` PASS; `nix develop -c env SQLX_OFFLINE=true cargo test --package crystal-forge build_commit_system_paths_includes_path_details_and_unavailable_states` PASS; `nix build .#checks.x86_64-linux.web-ui --no-link` PASS; `nix build .#checks.x86_64-linux.server --no-link` PASS.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/198

Moved In Progress -> Review after implementation and verification; branch `TASK-229-commit-config-paths` pushed and MR opened.

MR includes web-ui check screenshot attachment: `![13d-flakes-stress-dataset](/uploads/15af37ee011ef8ad4ac8a75abc9a909d/13d-flakes-stress-dataset.png)`.

Addressed reviewer blocker: current path semantics are now host-scoped and explicit in UI (`current path (<hostname>)`), with `mapped_host_count` surfaced when multiple hosts share a configuration.

Backend selection for displayed host/path changed from `systems.updated_at DESC LIMIT 1` to deterministic most-recent `system_states` report across mapped active hosts (hostname tie-break).

Resolved merge conflicts after merging `origin/dev` into branch (conflicts were backlog task markdown add/add); kept `origin/dev` versions for conflicted backlog files.

Follow-up commits pushed to MR-198: `94263c8e` (host-scoped fix) and `4a6d11ac` (merge conflict resolution merge commit).
<!-- SECTION:NOTES:END -->
