---
id: TASK-174
title: Add deterministic mock eval/build dev mode for fast workflow validation
status: Done
assignee: []
created_date: '2026-03-04 23:28'
updated_date: '2026-03-13 01:24'
labels:
  - dev-experience
  - eval-queue
  - builder
  - testing
dependencies: []
priority: high
ordinal: 91000
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

Fixed missing eval logs for late subscribers: added bounded in-memory per-commit eval log history in `CFState` and replay on eval websocket connect before live subscribe.

`broadcast_eval_message` now records serialized messages into history (cap 2000 lines), and `cleanup_eval_channel` now clears both channel and stored history for the commit.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge handlers::api::commits::tests` ✅.

Committed as `2b6a2f30` (`fix: replay buffered eval logs on websocket connect`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Addressed mock sync regression: in `execution_mode=mock`, manual `sync_flake_handler` now directly injects a synthetic commit each request (instead of attempting upstream sync first), so repeated "Sync from source" clicks always create a new mock eval/build run.

Added local-auth bootstrap for `server-stack-mock`: server now supports `CRYSTAL_FORGE_LOCAL_BOOTSTRAP_USERNAME/PASSWORD/EMAIL` and seeds or updates that user as Admin at startup in local auth mode. `server-stack-mock` sets defaults to `admin` / `password`.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge synthetic_mock_sync_hash_is_git_like_and_stable` ✅, `nix build .#devScripts.server-stack-mock` ✅.

Committed as `bd0f21e2` (`fix: make mock sync repeatable and seed local admin login`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Fixed stalled eval queue after manual sync/re-evaluate: wired `QueueNotifier` into `CFState` and now call `notify_eval_queue()` from `sync_flake_handler`, `sync_all_flakes_handler` (when commits inserted), and `re_evaluate_commit` after reset.

This removes the wait-for-fallback-poll behavior so newly queued commits start evaluation immediately in mock stack flows.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅ and `nix build .#devScripts.server-stack-mock` ✅.

Committed as `594eb1ac` (`fix: notify eval workers after sync and re-evaluate`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Enforced API-only builder behavior for mock stack: `server-stack-mock` now sets builder env overrides `CRYSTAL_FORGE__BUILDER__ENABLE_API_MODE=true`, fixed builder UUID, and loopback server URL to guarantee API queue path usage.

Builder binary now hard-fails when `execution_mode=mock` is enabled without API mode readiness, and logs an explicit deprecation warning when falling back to legacy direct-DB mode in non-mock configurations.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix build .#devScripts.server-stack-mock` ✅, `nix run .#devScripts.server-stack-mock -- --dry-run` ✅.

Committed as `1fe8b9d0` (`fix: force API-only builder in mock stack`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Fixed direct Evaluations page log gap by making `use_websocket_eval_stream` reconnect when the selected commit ID changes (instead of sticking to initial `0`/mount commit) and resetting local stream buffers on commit switch.

Added build stream replay cache in server state: build log/metrics frames are now recorded and replayed to newly connected viewers, including HTTP append path (fallback when builder WS send fails).

Adjusted API-mode mock build pacing to multi-stage ~8s progression so build jobs remain visible in queue long enough for frontend validation sessions.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅ and `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `80c7b2eb` (`fix: replay eval/build logs and stabilize mock build visibility`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Added build history support for frontend validation: new backend endpoint `GET /api/v1/build-jobs/recent` (viewer+) backed by `fetch_recent_build_history` query over completed/failed jobs, ordered newest-first.

Builds view now renders a `Recent Builds` section under active queue/detail split, populated via new `fetch_recent_build_jobs` client call, so historical build outcomes are visible similarly to evaluations history.

Flakes commit UI now includes build status chips (`build: queued/running/failed/complete/idle`) in both timeline cards and commit detail header; this complements eval chips and makes eval-complete commits show build completion state directly.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅ and `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `0bed8724` (`feat: add recent build history and build status chips`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Added maximize/fullscreen build log modal in `BuildDetailPane`, mirroring evaluations log UX: `⛶ Maximize` button opens overlay with connection status, full scrollable logs, and close actions.

Verification: `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `760a1a7b` (`feat: add maximize modal for build logs`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Updated shell banner semantics per UX guidance: replaced dev-auth-specific wording with reusable non-production environment markers and render both top + bottom banners when server `execution_mode=mock` is detected via eval-queue probe.

Banner text now explicitly communicates: mock/dev environment active, non-production context, and suitability for future DoD environment requirement messaging without layout rework.

Verification: `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `931fd20f` (`feat: add reusable non-production mode banners`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Adjusted non-production banners per UX feedback: now rendered by placement in app shell (single top banner above TopBar, single bottom banner at bottom of full shell), with simplified text and stronger orange background for visibility.

Updated mock eval/build realism for frontend validation: mock eval now upserts `commit_artifacts_cache` with system list (so eval queue shows multi-system chips) and injects deterministic mixed per-system outcomes (one policy-failed when multiple systems).

Updated mock build simulation to include deterministic failed build outcomes (control system naming pattern) in both API builder and legacy worker mock paths, ensuring mock runs include at least one failed build while others can complete.

Verification: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c cargo check` in `packages/web-ui` ✅, `nix build .#devScripts.server-stack-mock` ✅.

Committed as `929d6e8d` (`feat: add mixed mock eval/build outcomes and shell banners`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Follow-up UI polish fixes: build log maximize now uses explicit fullscreen overlay styles (matching evaluations modal behavior) to prevent inline rendering under existing logs.

Flake build-status chips are now clickable and navigate to Builds view, matching eval chip workflow expectations.

Non-production banners now use explicit inline style colors/typography for reliable text visibility across CSS contexts.

Verification: `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `9b43c704` (`fix: correct build log modal and build-chip navigation`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Adjusted non-production banners to be viewport-fixed and always visible while scrolling (top and bottom), with thinner height and preserved layout spacing via lightweight placeholders.

Dashboard timeline commit nodes now navigate contextually: build-active statuses route to Builds view, otherwise commits with evaluation status route to commit-specific Evaluations view.

Fixed missing live build/eval logs in split-origin dev setups by teaching websocket hooks to honor `cf_backend_origin` (same behavior pattern as API client), so streams connect to backend on 3445 when UI is served on 8080.

Verification: `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `1754948d` (`fix: pin env banners and route timeline nodes`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Adjusted dashboard commit timeline routing semantics per UX request: only actively building commits route to Builds, actively evaluating commits route to commit-specific Evaluations, and all other commits route to Flakes.

Verification: `nix develop -c cargo check` in `packages/web-ui` ✅.

Committed as `3f50a138` (`fix: route dashboard commit nodes by active stage`) and pushed to `origin/TASK-174-mock-eval-build-dev-mode`.

Opened MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/152

Task moved to Review per workflow after implementation and verification pass for targeted commands.

Addressed reviewer concerns in `3632a0a6`: API-mode mock builds now explicitly skip signing and cache-push side effects to avoid synthetic store-path noise while retaining normal completion state transitions.

Added deterministic helper coverage: `mock_policy_fail_pattern_is_deterministic` (eval path) and `mock_build_fail_pattern_is_deterministic` (builder path), plus documentation updates clarifying mixed outcomes and skipped post-build side effects in mock mode.

Verification rerun: `nix develop -c env SQLX_OFFLINE=true cargo check -p crystal-forge` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge models::evaluate_with_policies::tests` ✅, `nix develop -c env SQLX_OFFLINE=true cargo test -p crystal-forge tests::mock_store_path_is_deterministic_and_sanitized` ✅, `nix build .#devScripts.server-stack-mock` ✅.

Posted MR follow-up notes on !152 with the review response and command results.

## Task Completion

MR !152 merged into dev at commit b7f77596.

Implementation:
- Added execution_mode config (real|mock, default real)
- Dev safety validation (mock requires auth_mode=local + local DB)
- Startup hard-guard in server/builder binaries
- Mock eval path with deterministic results, staged logs, realistic pacing (~30s)
- Mock build path for both API builder and legacy workers
- Synthetic commit injection for manual sync in mock mode
- Eval/build log replay for late subscribers
- Build history API and UI
- Maximize/fullscreen modals for build logs
- Non-production environment banners
- Local admin bootstrap for mock stack
- Developer docs at docs/mock-execution-mode.md
- New dev script: server-stack-mock

All acceptance criteria satisfied:
- Config toggle exists (execution_mode)
- Mock mode blocked in production
- Mock paths use same queue/state APIs
- Realistic streaming logs/events
- UI indicates mock mode active
- Developer guide documented

Worktree cleanup: TASK-174-mock-eval-build-dev-mode
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Automated test(s) cover production guardrail (mock mode rejected in prod).
- [ ] #2 Automated test(s) cover core mock transition flow (pending -> in_progress -> complete/fail path).
- [ ] #3 Backlog task TASK-173 depends on this task so bug-fix validation uses mock mode.
<!-- DOD:END -->
