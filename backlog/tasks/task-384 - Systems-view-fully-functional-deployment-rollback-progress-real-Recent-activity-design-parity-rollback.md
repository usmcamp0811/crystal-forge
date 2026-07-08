---
id: TASK-384
title: >-
  Systems view: fully functional deployment/rollback progress, real Recent
  activity, design-parity rollback
status: In Progress
assignee:
  - '@gpt-5.5'
created_date: '2026-07-08 02:49'
updated_date: '2026-07-08 19:26'
labels:
  - design-parity
  - systems
  - deployment
  - web-ui
  - backend
  - agent
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/views/systems_list.rs
  - packages/default/src/handlers/api/systems.rs
  - packages/default/src/handlers/agent/heartbeat.rs
  - packages/default/src/queries/system_events.rs
  - packages/default/src/deployment/agent.rs
  - packages/default/migrations/0155_system_events_timeline.sql
  - checks/web-ui/coverage-manifest.json
  - packages/default/src/fixtures/seed.rs
documentation:
  - >-
    backlog/docs/specs/doc-17 -
    Spec-Systems-view-live-deployment-progress-real-recent-activity-working-rollback.md
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/design/CrystalForge/components/Systems.jsx
  - docs/design/CrystalForge/styles.css
priority: high
ordinal: 321000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Systems surfaces (System Detail page and the Systems list slide-out panel) do not match the design example in `docs/design/CrystalForge/` for three critical behaviors, and two of them are not just UI gaps — the backend cannot support them yet:

1. **No "Deployment in progress" experience.** The design shows a `PendingDeployBanner` with a live 4-stage tracker (Queued → Picked up → Applying → Activated) plus a heartbeat-metric override while a deploy/rollback is in flight. Today NOTHING renders during a deployment, and the server cannot even report the stage: `pending_system_deployments` has no delivered/applying tracking, nothing emits the (already schema-allowed) `cf_deployment_started` event, and there is no deployment-status API.
2. **Recent activity is fake.** The Overview tab renders 3 synthetic rows (record updated / registered / heartbeat) instead of the real `system_events` history that already exists (TASK-378.1). The list panel's activity section has the same problem.
3. **Rollback needs design parity + verified end-to-end function.** The header Rollback button and `GenerationRollbackModal` exist and call real endpoints, but the modal lacks the design's policy-bypass warning and the production type-to-confirm hostname guard, and nothing proves the full request → agent pickup → activation → history loop works.

## Goal

Ship the full pull-based deployment progress pipeline (DB migration + heartbeat delivery tracking + agent deployment-started report + deployment-status API + polling UI banner), real-event Recent activity feeds, and a design-parity rollback flow with production guard — on BOTH the System Detail page and the Systems list slide-out panel — verified end-to-end with seeded fixtures and web-ui check screenshots.

**The complete step-by-step implementation guide is doc-17** (`backlog/docs/specs/doc-17 - Spec-Systems-view-live-deployment-progress-real-recent-activity-working-rollback.md`). Read it FIRST and follow it top to bottom; it names every file, pattern, SQL statement, CSS class, and test. Do not improvise different architecture.

## Key decisions already made (do not relitigate)

- Full 4-stage fidelity: `queued` → `picked_up` → `applying` → `activated` (+ `failed`), derived server-side from `pending_system_deployments` columns per the stage table in doc-17 §1.
- New migration `0156_deployment_progress_tracking.sql` adds ONLY `delivered_at` + `applying_at` columns. NEVER edit existing migrations.
- Agent reports "applying" via new fire-and-forget `POST /agent/deployment-started` (5s timeout, failure never blocks a deployment). Old-agent/old-server version skew must degrade gracefully (stage skips are OK; UI tolerates `picked_up` → `activated` jumps).
- UI gets live updates by POLLING `GET /api/v1/systems/:id/deployment-status` every 4s while active (204 = idle). No SSE.
- Recent activity = REAL events only from `/api/v1/systems/:id/history` (no synthetic rows), capped at 9 (detail) / 6 (panel), "View all" switches to the History tab.
- Rollback modal: keep real generation candidates, add design warning callout + production type-to-confirm reusing the exact `remove_system_dialog.rs` pattern. On confirm, the amber rollback banner variant must appear.
- Deploy vs rollback banners distinguished via the pending row `source` values threaded per doc-17 §3.1.

## Non-Goals

- SSE/websocket streaming; SSH modal; tags persistence (TASK-353.1); Deploy tab full parity; dashboard widgets; `auto_latest` scheduler semantics; systems list cards/table visual parity (owned by TASK-330/TASK-353 — this task only touches the panel's banner + activity sections); editing any existing migration.

## Architectural Constraints

- Stage derivation is a pure, unit-tested server-side function; the UI never derives stages from raw timestamps.
- New wire/DTO fields use `#[serde(default)]`; UI DTOs mirror server models (`packages/web-ui/src/api/models.rs`).
- No business logic in Dioxus views: the event→activity-row mapping and prod-guard enablement are pure testable helpers; `PendingDeployBanner` is a reusable component in `packages/web-ui/src/components/system/`.
- New queries prefer `sqlx::query_as`; sqlx offline metadata MUST be regenerated (`db-only up` + `cargo sqlx prepare` from the devshell — never a shared DB).
- Follow the existing agent auth pattern (`handlers/agent/state.rs`) for the new agent endpoint; follow existing route/handler organization in `bin/server.rs` + `handlers/api/systems.rs`.

## Impact Areas

- DB: new migration 0156 (additive columns only).
- Server: heartbeat handler, new agent endpoint, new API endpoint, history event mapping, deployment source threading, api/models.rs.
- Agent: deployment-started report in `deployment/agent.rs` (+ request helper reuse from `bin/agent.rs`).
- Web UI: `system_detail.rs`, `systems_list.rs`, new banner component, models/client, possibly ported `.deploy-pending*` CSS.
- Fixtures/checks: fixture JSON, `fixtures/seed.rs`, `checks/web-ui/coverage-manifest.json`.

## Risk

**Medium.** Touches agent↔server protocol (mitigated: additive, fire-and-forget, serde defaults, graceful skew) and the heartbeat hot path (mitigated: delivery mark is best-effort, never fails the heartbeat). UI changes are additive to two existing views. Coordinate with in-progress TASK-330/TASK-353 to avoid merge collisions in `systems_list.rs`.

## Dependencies

- TASK-378.1 (system_events timeline) — MERGED, available on dev. No blocking dependencies.

## Verification Plan (Tier 2)

Per doc-17 §8: fmt + clippy `-D warnings` + tests in both crates; `db-only up` + `cargo sqlx prepare`; `nix build .#packages.x86_64-linux.{server,web-ui}`; `nix build .#checks.x86_64-linux.web-ui` (must produce the new screenshots); `nix flake check --keep-going` (required: migration + agent + server surface). MR must attach web-ui check screenshots (banner mid-deploy, real activity feed, rollback modal with disabled-until-typed confirm) via GitLab uploads — never committed to the repo.

## Notes

- Supersedes TASK-280 (rollback generation picker modal — already implemented; this task finishes parity + verification).
- Manual E2E sanity (optional but encouraged): `server-stack-mock up` + `run-agent --dev`, trigger a deploy, watch the banner walk the stages.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Migration 0156 adds delivered_at and applying_at to pending_system_deployments; no existing migration file is modified; cargo sqlx prepare metadata is regenerated and consistent
- [ ] #2 Heartbeat handler sets delivered_at (once, best-effort) on the matching pending deployment when it serves desired_target; a DB test proves only pending+NULL rows are touched and heartbeats never fail on marker errors
- [ ] #3 POST /agent/deployment-started (agent-authenticated) sets applying_at and idempotently inserts one cf_deployment_started system_events row (dedupe on pending id); double-post inserts exactly one event; unknown/no-pending reports return 200 without error
- [ ] #4 Agent posts deployment-started before running switch-to-configuration, fire-and-forget with 5s timeout; any report failure is logged and never blocks or fails the deployment
- [ ] #5 GET /api/v1/systems/:id/deployment-status returns the derived stage (queued|picked_up|applying|activated|failed) per the doc-17 stage table, includes kind deploy|rollback derived from source, returns completed rows only within 2 minutes, and 204 when idle; stage derivation is unit-tested for all stages plus expiry
- [ ] #6 Manual deploy/rollback endpoints record distinct pending-deployment source values (manual_deploy, manual_rollback_commit, manual_rollback_generation) while the auto scheduler keeps auto_desired_target
- [ ] #7 PendingDeployBanner renders on the System Detail page between metric strip and tabs during an active deployment: correct title, mono target chip, per-stage sub text, 4-step tracker with past-check/current-pulse, amber rollback variant, done state with dismiss, failed state with danger treatment; heartbeat metric shows the stage override while active
- [ ] #8 System Detail polls deployment-status every ~4s only while a banner-worthy state exists, refetches immediately after a successful deploy/rollback request, and tolerates stage skips (picked_up to activated) without error
- [ ] #9 Overview Recent activity renders only real events from the system history endpoint with design-mapped icon/color/title (cf_deployment_started maps to kind cf_deployment with title Deployment started), capped at 9, with a View all button that switches to the History tab, and a muted empty state
- [ ] #10 Rollback modal reaches design parity: real generation candidates, policy-bypass warning callout, production type-to-confirm hostname guard (confirm disabled until exact hostname; reuses remove_system_dialog pattern; falls back to name-match when is_production flag absent); on confirm the view switches to Overview and the rollback banner appears
- [ ] #11 Systems list SystemPreviewPanel shows the PendingDeployBanner as its first section for a system with an active deployment (polling while open, stopping on close) and its Recent activity section shows the newest 6 real events
- [ ] #12 Fixtures seed one system with an in-flight pending deployment (applying stage, relative-to-now timestamps) plus at least 5 system_events spanning succeeded/started/failed/reboot/local_rebuild; checks/web-ui coverage-manifest asserts and screenshots the banner (text Deployment in progress + stage Applying), the real activity feed (3+ event rows), the rollback modal guard behavior, and the list-panel banner
- [ ] #13 All verification passes: fmt + clippy -D warnings + cargo test in packages/default and packages/web-ui; nix build of server, web-ui package, and checks web-ui; nix flake check --keep-going; MR attaches the web-ui check screenshots via GitLab uploads (not committed)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Backend schema + queries
   - Add migration `packages/default/migrations/0156_deployment_progress_tracking.sql`.
   - Extend `PendingSystemDeployment` and related queries in `packages/default/src/queries/system_events.rs`.
   - Add `mark_pending_deployment_delivered`, `mark_pending_deployment_applying`, `get_system_deployment_progress`, and pure stage/kind derivation helpers with unit tests.

2. Backend API + routes
   - Add `SystemDeploymentProgress` to `packages/default/src/api/models.rs`.
   - Add `GET /api/v1/systems/:id/deployment-status`.
   - Add `POST /agent/deployment-started`.
   - Update route table in `packages/default/src/bin/server.rs`.
   - Update `event_history_kind` / `event_history_title` for `cf_deployment_started`.
   - Thread source labels: `manual_deploy`, `manual_rollback_commit`, `manual_rollback_generation`; keep `auto_desired_target` for scheduler.

3. Agent reporting
   - Add a fire-and-forget deployment-started report before `switch-to-configuration`.
   - Reuse existing signed agent request patterns.
   - Timeout at 5 seconds; log and continue on any failure.

4. Web API + reusable UI
   - Mirror DTO in `packages/web-ui/src/api/models.rs`.
   - Add `get_system_deployment_progress` in `packages/web-ui/src/api/client.rs`.
   - Add reusable `PendingDeployBanner` component under `packages/web-ui/src/components/system/`.
   - Port missing `.deploy-pending*`, `.deploy-step*`, `hb-waiting`, and timeline pulse CSS from design if needed.

5. System Detail integration
   - Poll deployment-status while active.
   - Render banner between metric strip and tabs.
   - Override heartbeat metric while pending/applying/activated.
   - Refetch immediately after successful deploy/rollback.
   - Replace synthetic Recent activity with real `/history` events only.
   - Add View all → History tab behavior.
   - Add rollback warning + production type-to-confirm.

6. Systems list panel integration
   - Fetch/poll deployment status while panel is open.
   - Render banner as first panel section.
   - Replace synthetic panel Recent activity with newest real history events.

7. Fixtures and web-ui checks
   - Extend fixture JSON + seeding for one in-flight applying deployment and event rows.
   - Extend `checks/web-ui/coverage-manifest.json` assertions/screenshots for detail banner applying, real activity feed, rollback guard, and panel banner.

8. Verification + MR
   - Run required fmt/clippy/tests/builds/sqlx/Nix checks.
   - Capture/upload screenshots to MR.
   - Move task to Review only after MR is opened.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.5 on reckless in /home/mcamp/code/crystal-forge/TASK-384-systems-deployment-progress

Approved implementation plan recorded. Starting code work in /home/mcamp/code/crystal-forge/TASK-384-systems-deployment-progress.

Follow-up runtime polish pushed in `ea26dd14 feat: report deployment failures to systems UI` on branch `TASK-384-systems-deployment-progress`. Adds migration `0157_deployment_failure_details.sql`, agent `POST /agent/deployment-failed` reporting, persisted `failed_at`/`failure_message`, deployment-status failure details, UI failed-banner messaging, queued heartbeat countdown text, and Recent activity wrapping polish. Updated agent failure reporting to send the expanded anyhow chain (`{:#}`), so root causes like `nix copy failed: ... no substituter that can build it` should reach the UI. Verification run for follow-up: `cargo sqlx prepare` succeeded after applying migration 0157 to local dev DB; `git diff --check` passed; backend and web-ui fmt checks passed; `SQLX_OFFLINE=true cargo check --all-targets` for packages/default passed; web-ui `cargo check --all-targets` passed before final backend-only tweak; `cargo test deployment_progress_tests --lib` passed; `cargo test pending_deploy_banner` passed; `nix build .#packages.x86_64-linux.server --no-link` passed; `nix build .#packages.x86_64-linux.web-ui --no-link` passed. Clippy `-D warnings` remains blocked by known baseline TASK-80 warning debt, not this follow-up. MR remains deferred pending maintainer runtime evaluation, per request.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: gpt-5.5
created: 2026-07-08 18:20
---
Branch `TASK-384-systems-deployment-progress` pushed for maintainer runtime evaluation. Latest commits: `745199eb feat: add systems deployment progress pipeline`, `ee683369 test: include systems deployment progress check`. Verification already completed as noted in session: fmt/check/tests/Nix builds/web-ui check/flake check passed, with clippy `-D warnings` blocked by existing TASK-80 baseline lint debt. MR creation is intentionally deferred pending maintainer evaluation/screenshots on a running server.
---
<!-- COMMENTS:END -->
