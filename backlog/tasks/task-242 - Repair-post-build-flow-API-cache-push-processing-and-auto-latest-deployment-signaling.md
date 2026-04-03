---
id: TASK-242
title: >-
  Repair post-build flow: API cache push processing and auto-latest deployment
  signaling
status: Backlog
assignee: []
created_date: '2026-04-03 14:26'
labels:
  - builds
  - cache
  - deployment
  - api-builder
  - hotfix
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Completed system-config builds are not reliably progressing through the expected post-build pipeline:

1. **API-mode builder queues cache push jobs but does not process them**
   - In API mode, successful builds call `create_cache_push_job(...)` in `packages/default/src/bin/builder.rs`.
   - However, API mode only starts heartbeat + job polling loops; it does **not** start `run_cache_push_loop(...)`.
   - Legacy direct-DB mode does start `run_cache_push_loop(...)`, which is why the logic exists but is not active in the current builder mode.
   - Result: builds can complete and cache push jobs can be queued, but nothing consumes them, so artifacts never reach the cache.

2. **Deployment policy manager is crashing before it can set `desired_target`**
   - Server logs show: `Policy update failed: Failed to fetch systems with auto_latest policy: no column found for name: system_configuration_name`
   - `queries/deployment.rs::get_systems_with_auto_latest_policy()` selects a `System` row without `system_configuration_name`, but `models/systems.rs::System` requires it.
   - The deployment policy manager then fails before it can update `systems.desired_target` for auto-latest systems.

3. **Auto-deploy depends on completed cache push, not just build-complete**
   - `queries/derivations.rs::get_latest_deployable_targets_for_flake_hosts()` only considers derivations with a `cache_push_jobs` row in `completed` state.
   - Therefore, if cache push jobs are never processed, deployment readiness never advances, even when the build itself succeeded.

This produces the observed operator experience: completed builds with no visible cache push success and no agents being told to deploy.

## Goal

Restore the full post-build path for NixOS system configs in API builder mode:

`build complete -> cache push queued -> cache push processed -> desired_target updated -> agent sees desired_target and deploys`

## Non-Goals

- No redesign of deployment policy semantics
- No changes to manual/pinned deployment policy behavior
- No new UI redesign work beyond minimal correctness fixes if required
- No changes to TASK-239 (cancelling log append follow-up)

## Scope

### A. Start cache push processing in API builder mode

In `packages/default/src/bin/builder.rs`, `run_api_mode(...)` must start the cache push worker loop when cache pushing is enabled, mirroring legacy mode behavior.

Requirements:
- If `cache_config.push_after_build` is true, spawn `run_cache_push_loop(pool.clone())` (or the equivalent worker entrypoint used by current cache processing)
- Ensure the loop runs alongside heartbeat + job polling and participates in shutdown behavior
- Avoid duplicate worker startup if API mode is restarted

### B. Fix deployment policy query/model mismatch

In `packages/default/src/queries/deployment.rs`, update `get_systems_with_auto_latest_policy()` so the selected columns fully match `models/systems.rs::System`, including:
- `system_configuration_name`

Requirements:
- The query must populate the `System` struct correctly
- The deployment policy manager in `packages/default/src/deployment/mod.rs` must stop failing on this query and continue processing auto-latest systems

### C. Verify post-build state transitions are actually connected

Confirm and, if needed, minimally fix the glue so the intended flow works:
- successful API-mode build queues cache push via `create_cache_push_job(...)`
- cache push worker marks cache push `completed`
- derivation reaches / is recognized as `cache-pushed`
- deployment policy manager uses the latest cache-pushed deployable target
- agent heartbeat sees updated `desired_target`

This should be minimal-scope work: fix wiring gaps rather than redesigning the flow.

### D. Minimal reporting sanity check

If the underlying logic is repaired but the relevant existing UI surfaces still fail to reflect reality due to obvious mapping/query omissions, include the smallest correctness fix needed.

Primary UI surfaces to verify (not redesign):
- `packages/web-ui/src/views/caches.rs`
- `packages/web-ui/src/views/system_detail.rs`
- any build/deployment status surface directly affected by the repaired data path

## Architectural Constraints

- Follow existing API-mode builder architecture; do not reintroduce legacy direct-DB behavior into the wrong code path
- Keep cache push processing as background worker logic, not inline in the request path
- Preserve the requirement that deployable targets come from successfully cache-pushed NixOS derivations
- Prefer minimal, targeted fixes over refactors

## Verification Plan

### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- Targeted tests covering:
  - `get_systems_with_auto_latest_policy()` shape / row mapping
  - API-mode startup includes cache push worker when enabled
  - post-build cache push job creation path still succeeds

### Tier 1
Using dev stack / mock stack as appropriate:
1. Start server + API builder with cache push enabled
2. Complete a NixOS system-config build in API mode
3. Verify a cache push job is created and then transitions to `completed`
4. Verify deployment policy manager no longer errors on `system_configuration_name`
5. Verify `systems.desired_target` updates for an auto-latest system
6. Verify agent heartbeat returns the updated `desired_target`

## Impact Areas

- `packages/default/src/bin/builder.rs`
- `packages/default/src/builder/cache_worker.rs`
- `packages/default/src/queries/deployment.rs`
- `packages/default/src/deployment/mod.rs`
- possibly minimal related UI/API reporting files if needed for correctness

## Risk Level

High

This touches the production post-build pipeline, but the actual code changes should be narrow and well-bounded.

## References

- `packages/default/src/bin/builder.rs`
- `packages/default/src/builder/cache_worker.rs`
- `packages/default/src/queries/deployment.rs`
- `packages/default/src/deployment/mod.rs`
- `packages/default/src/queries/derivations.rs`
- `packages/default/src/handlers/agent/heartbeat.rs`
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 API-mode builder starts cache push processing when cache push is enabled, and queued cache push jobs are actually consumed.
- [ ] #2 A successful API-mode NixOS build results in a cache push job that can reach `completed` state.
- [ ] #3 `get_systems_with_auto_latest_policy()` correctly maps to `System`, including `system_configuration_name`, and the deployment policy manager no longer errors on that query.
- [ ] #4 For an auto-latest system, once a deployable target has a completed cache push, `systems.desired_target` is updated to that target.
- [ ] #5 Agent heartbeat returns the updated `desired_target` for the affected system.
- [ ] #6 Existing relevant UI/API surfaces reflect the repaired state correctly, or the minimal required correctness fix is included.
- [ ] #7 Targeted verification demonstrates the full path: build complete -> cache push completed -> desired_target updated -> agent sees desired_target.
<!-- AC:END -->
