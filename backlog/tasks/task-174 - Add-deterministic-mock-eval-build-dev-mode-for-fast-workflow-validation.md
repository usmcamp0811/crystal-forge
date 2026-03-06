---
id: TASK-174
title: Add deterministic mock eval/build dev mode for fast workflow validation
status: In Progress
assignee: []
created_date: '2026-03-04 23:28'
updated_date: '2026-03-06 02:18'
labels:
  - dev-experience
  - eval-queue
  - builder
  - testing
dependencies: []
priority: high
ordinal: 94000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Eval and build phases are high-latency compared to UI iteration speed. This slows validation of queue behavior, ordering, logs, retries, and state transitions. We need a safe way to exercise the full process flow quickly in development without running real `nix-eval-jobs` and `nix build`.

## Goal
Introduce a deterministic **dev-only mock execution mode** for eval and build that simulates realistic timings, statuses, logs, and queue transitions, while preserving the same API/DB/event flow used in production.

## Non-Goals
- No production behavior changes when mock mode is disabled.
- No replacement of real eval/build paths; mock mode is additive and dev-only.
- No weakening of production safety checks.

## Proposed Approach
- Add a server config switch for execution mode:
  - `execution.mode = "real" | "mock"` (default `real`)
- Add a strict safety gate:
  - Mock mode only allowed in explicit dev environment (e.g., `server.dev_mode=true` + non-prod profile).
  - Server must hard-fail startup if `execution.mode=mock` in production profile.
- Implement mock adapters behind existing eval/build interfaces:
  - Mock eval emits per-system events/logs/policy outcomes with deterministic pseudo-random timing.
  - Mock build emits lease/heartbeat/log completion transitions and optional controlled failures/retries.
- Keep event-driven queue semantics intact:
  - Eval completion enqueues build jobs exactly as real mode does.
  - Ordering and reordering behavior can be validated end-to-end.
- Add observability marker in UI/API responses:
  - Clearly show `MOCK MODE` badge to prevent operator confusion.

## Verification Plan
- With `execution.mode=mock` in dev:
  - Queue ingest -> eval -> build transitions complete quickly.
  - Logs stream and status transitions mirror real flow shape.
  - Reordering queue changes next claimed eval/build as expected.
- With `execution.mode=real` (default):
  - Existing behavior unchanged.
- With production profile + `execution.mode=mock`:
  - Server startup fails with explicit error.

## Why Before TASK-173
This enables rapid, deterministic reproduction/validation for the eval-log-visibility and queue-ordering bugs in TASK-173, reducing cycle time and flakiness in verification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A config toggle exists to switch execution mode between `real` and `mock`, defaulting to `real`.
- [ ] #2 Mock mode is blocked in production and startup fails if enabled outside approved dev context.
- [ ] #3 Mock eval and mock build paths drive the same queue/state APIs and DB transitions as real mode.
- [ ] #4 Mock mode emits realistic streaming logs/events for eval and build phases.
- [ ] #5 UI/API clearly indicates when mock mode is active.
- [ ] #6 A short developer guide documents how to enable/disable mock mode and expected behavior.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-174-mock-eval-build-dev-mode

Progress update: implemented server.execution_mode (real|mock, default real), dev safety validation (mock requires auth_mode=dev), startup hard-guard in server/builder binaries, mock eval path wiring + deterministic mock results/log streaming + policy check pass shape, mock build path wiring in API builder loop, eval queue API/UI execution_mode indicator with MOCK MODE badge, and developer docs at docs/mock-execution-mode.md.

Verification run in task worktree: `nix develop -c rustfmt --edition 2021 --check <modified rust files>` (pass), `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` (pass), `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge config::server::tests` (pass), `nix develop -c cargo check` in packages/web-ui (pass).

Note: backend checks/tests were run with `SQLX_OFFLINE=true` to avoid requiring a live DB during targeted validation.

Added deterministic helper unit tests: `models::evaluate_with_policies::tests::mock_systems_fallback_and_filtering` and `tests::mock_store_path_is_deterministic_and_sanitized` (in builder bin).

Targeted verification rerun after helper-test additions: `nix develop -c rustfmt --edition 2021 --check src/models/evaluate_with_policies.rs src/bin/builder.rs`, `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge`, and targeted `cargo test` filters for the new tests (all passing).

Added Nix dev script output `devScripts.server-stack-mock` and shell alias `server-stack-mock` to launch process-compose with mock execution enabled for both server and builder (`AUTH_MODE=dev`, `CRYSTAL_FORGE__SERVER__EXECUTION_MODE=mock`). Updated devshell startup help text to advertise the new command.

Nix verification: `nix build .#devScripts.server-stack-mock` ✅, `nix run .#devScripts.server-stack-mock -- --help` ✅, `nix flake check` ❌ failed in existing VM checks (`vm-test-run-crystal-forge-attic-cache-integration` and `vm-test-run-crystal-forge-server-integration-test`), not in the new devScripts output build itself.

Committed implementation on branch `TASK-174-mock-eval-build-dev-mode` as `cf6999e2` with conventional commit message `feat: add dev-only mock eval/build execution mode`.

Pushed branch to origin: `git push -u origin TASK-174-mock-eval-build-dev-mode` (upstream set).

Follow-up correction per review: switched mock-mode auth requirement from `server.auth_mode=dev` to `server.auth_mode=local` and aligned `devScripts.server-stack-mock` to force `AUTH_MODE=local` for server and builder.

Verification after correction: `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge config::server::tests` ✅, `nix build .#devScripts.server-stack-mock` ✅, `nix run .#devScripts.server-stack-mock -- --help` ✅.

Committed fix as `fd930a64` (`fix: require local auth mode for mock execution`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Addressed startup failure in `server-stack-mock`: process-compose now forces dev binaries for mock stack (`run-server --dev`, `run-builder --dev`) instead of packaged release binaries.

Adjusted runtime safety for mock mode in server/builder binaries: removed release-only rejection and require local DB host (`localhost`/`127.0.0.1`/`::1`) in addition to config-level `auth_mode=local` requirement.

Verification: `nix develop -c rustfmt --edition 2021 --check src/bin/server.rs src/bin/builder.rs` ✅, `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix build .#devScripts.server-stack-mock` ✅, `nix run .#devScripts.server-stack-mock -- --dry-run` ✅. Verified generated process-compose config uses `run-server --dev`/`run-builder --dev` and `AUTH_MODE=local` + `CRYSTAL_FORGE__SERVER__EXECUTION_MODE=mock`.

Committed as `edbebc9c` (`fix: allow mock mode with local auth and local db`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Addressed browser instability under mock mode by capping websocket log buffers in UI hooks (`packages/web-ui/src/hooks/websocket.rs`). Added bounded push helper with max 2000 lines for both build and eval streams to prevent unbounded in-memory log growth.

Verification: `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `16e5c5f7` (`fix: cap websocket log buffers in evaluations UI`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Enhanced mock realism based on feedback: eval mock now emits deterministic staged per-system logs with progress percentages and human-observable pacing (~30s total for default 3-system runs).

Extended mock build coverage to legacy builder workers (not only API builder mode): when `execution_mode=mock`, workers simulate build progression and complete with deterministic synthetic store paths while preserving queue/status transitions.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge models::evaluate_with_policies::tests` ✅, `nix build .#devScripts.server-stack-mock` ✅, `nix run .#devScripts.server-stack-mock -- --dry-run` ✅.

Committed as `497c05c9` (`feat: add realistic paced mock eval and legacy build simulation`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Implemented mock sync simulation for manual flake sync: when `execution_mode=mock` and upstream has no new commits, `sync_flake_handler` now injects a synthetic git-like commit (40-char hex hash) with mock metadata so each sync can trigger a fresh eval/build run.

Added unit test `synthetic_mock_sync_hash_is_git_like_and_stable` and updated mock mode docs to describe synthetic commit injection behavior during manual sync.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge synthetic_mock_sync_hash_is_git_like_and_stable` ✅, `nix build .#devScripts.server-stack-mock` ✅.

Committed as `0ba30001` (`feat: inject synthetic commit on mock flake sync`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Automated test(s) cover production guardrail (mock mode rejected in prod).
- [ ] #2 Automated test(s) cover core mock transition flow (pending -> in_progress -> complete/fail path).
- [ ] #3 Backlog task TASK-173 depends on this task so bug-fix validation uses mock mode.
<!-- DOD:END -->
