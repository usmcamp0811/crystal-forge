---
id: TASK-225
title: Track expected Nix store paths from eval and match agent state pre-build
status: Review
assignee: []
created_date: '2026-03-29 23:39'
updated_date: '2026-03-30 00:14'
labels:
  - backend
  - nix
  - deployment
  - data-model
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
Today, evaluation stores derivation (`.drv`) paths, while deployment matching relies on built `store_path` values. This means we cannot always determine whether a system is already running the intended configuration before Crystal Forge builds that target.

## Goal
Persist an expected Nix store output path for each evaluated NixOS configuration at eval time (without requiring a build), and use that expected path in system-state matching so we can report whether `/run/current-system` is on the intended target earlier and more accurately.

## Non-Goals
- No redesign of deployment UI beyond minimal status updates needed for this feature.
- No replacement of existing build-complete `store_path` tracking.
- No changes to agent payload format unless strictly required.

## Architectural Constraints
- Keep eval logic in backend domain/service/query layers; no business logic in UI views.
- Preserve current derivation/build pipeline semantics; expected-path tracking augments, not replaces, build outputs.
- Keep data model explicit: separate expected path (eval-time) from built store path (build-time) unless a clear schema migration plan consolidates fields safely.

## Verification Plan
- Unit tests for parsing/deriving output store paths from `.drv` metadata.
- Integration tests for eval pipeline storing expected paths for discovered `nixosConfigurations`.
- Integration/view/query tests proving system deployment matching uses expected path when build store path is absent.
- Run targeted backend checks/tests in nix dev environment.

## Impact Areas
- `packages/default/src/models/evaluate_with_policies.rs`
- `packages/default/src/queries/derivations.rs`
- `packages/default/src/queries/system_states.rs`
- deployment status views/migrations affecting `view_system_deployment_status` and related system detail/list views
- backend API models/handlers that surface deployment matching status

## Risk Level
High (incorrect matching could misreport deployment state and affect rollout safety).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For each successfully evaluated NixOS configuration, the backend persists an expected output store path derived at eval time without building.
- [ ] #2 Expected-path persistence is deterministic and covered by tests for supported repo/derivation modes in this codebase.
- [ ] #3 Deployment matching compares agent-reported current store path against expected path when built store path is not yet available.
- [ ] #4 When both expected path and built store path are available, matching behavior is well-defined, deterministic, and documented in task notes.
- [ ] #5 System status/deployment views and API responses expose correct matching outcomes for: not built yet, built and matches, built and diverged, and unknown path cases.
- [ ] #6 Existing build pipeline behavior remains intact; build-complete store_path updates still work and are tested.
- [ ] #7 Targeted backend tests pass in nix develop, including new unit/integration coverage for this feature.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Moved to To Do explicitly per user sprint selection request on 2026-03-29.

LOCK: claude-sonnet-4-6 on reckless in /home/mcamp/code/crystal-forge/TASK-225-expected-store-paths

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/196

cargo check: 0 errors (SQLX_OFFLINE=true). cargo sqlx prepare: success. 3 unit tests pass.

Fix pushed (ef54e688): corrected latest_derivation join key (derivation_name not derivation_target) and per-system policy log level. cargo check: 0 errors. 3 unit tests pass.
<!-- SECTION:NOTES:END -->
