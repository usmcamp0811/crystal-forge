---
id: TASK-385
title: >-
  Flakes sync-error surfaces (real backend errors) + sidebar alert badge system
  with attention flash
status: Backlog
assignee: []
created_date: '2026-07-08 07:26'
labels:
  - design-parity
  - flakes
  - sidebar
  - alerts
  - web-ui
  - backend
dependencies: []
references:
  - packages/default/src/flake/commits.rs
  - packages/default/src/handlers/api/flakes.rs
  - packages/default/src/queries/flakes.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/views/flakes_list.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/coverage-manifest.json
  - packages/default/src/fixtures/seed.rs
documentation:
  - >-
    backlog/docs/specs/doc-18 -
    Spec-Flakes-sync-error-surfaces-and-sidebar-alert-badge-system.md
  - docs/design/CrystalForge/components/FlakesView.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/styles.css
priority: high
ordinal: 328000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Two design-example behaviors are missing, and one of them has NO backend support at all:

1. **Flake sync failures are invisible.** The design (`FlakesView.jsx`) shows a synced/syncing/error status chip per flake, an error callout on cards, and a "Sync failed" banner in the commit tray with the real `nix flake metadata` error output, last-good-commit, and a Retry button. Today the server runs `sync_commits_for_flake` and **throws the error away after logging** — nothing is persisted, so the UI has literally no sync-status data to render. Users cannot tell a flake stopped syncing.
2. **No sidebar alert system.** The design (`Shell.jsx` lines 167-304) puts count badges on sidebar entries — red "needs attention" (systems critical/offline, flakes failing sync, environments w/ critical systems, failed builds/evals 24h, critical CVEs) with tooltips; visiting a view acknowledges the badge (per page load) and the alerting rows pulse once (`attention-flash`). For Builds/Evals the badge clears only when the failures tab is opened. None of this exists.

## Goal

Fully functional, real-data implementation of both: (a) flake sync status persisted by the backend (migration + recording wrapper around every sync call site + API fields) and rendered per design (chip, card callout, tray "Sync failed" banner with retry); (b) a sidebar badge system driven by one new aggregate endpoint (`GET /api/v1/navigation/badges`, polled every 30s) with design-exact acknowledgment and attention-flash behavior across Systems, Flakes, Environments, Builds, Evals, and CVEs views.

**The complete step-by-step implementation guide is doc-18** (`backlog/docs/specs/doc-18 - Spec-Flakes-sync-error-surfaces-and-sidebar-alert-badge-system.md`). Read it FIRST and follow it top to bottom. Do not improvise different architecture.

## Key decisions already made (do not relitigate)

- New migration (next free number; 0156 is reserved by TASK-384) adds `sync_status` ('unknown'|'synced'|'syncing'|'error', default 'unknown'), `last_sync_at`, `last_sync_error` to `flakes`. NEVER edit existing migrations.
- One recording wrapper `sync_flake_recorded` in `flake/commits.rs`; ALL FOUR existing sync call sites route through it; status writes are best-effort and never mask the sync result.
- One aggregate endpoint `GET /api/v1/navigation/badges` (new `handlers/api/navigation.rs` + `queries/navigation.rs`); counts MUST reuse existing semantics (systems health = same logic as Systems list; cves_critical = same query as /api/v1/cves/stats). Sidebar polls it every 30s — one request, not per-link.
- Badge acknowledgment exactly per design: attention badge hides once its view is visited this page load; Builds/Evals acknowledge only when the failures tab opens; alerting rows flash once per page load.
- Alert state is a pure, unit-testable module (`packages/web-ui/src/alerts/`), not scattered view logic.
- New flake UI pieces go in `packages/web-ui/src/components/flake/` — `flakes_list.rs` is already over the module-size limit and must not grow with new component bodies.

## Non-Goals

- The all-views visual drift audit (companion task, doc-19). Topbar notification bell. Flake env-span data (TASK-357.1) and auto-sync interval persistence (TASK-357.2). Changing sync mechanics/cadence. Badges for Dashboard/Scanning/Policies/Compliance/Builders/Caches/Admin (design shows none).

## Architectural Constraints

- New wire/DTO fields `#[serde(default)]`; UI DTOs mirror server models.
- Badge visibility + flash-once logic are pure functions with unit tests; no business logic in Dioxus views.
- New queries prefer `sqlx::query_as`/`query_scalar`; sqlx offline metadata regenerated via devshell (`db-only up` + `cargo sqlx prepare`); never a shared DB.
- Error text truncated to 4000 chars before persisting.
- Port `.nav-count-alert`, `.attention-flash`, `.fl-sync-error*` CSS from the design stylesheet only if missing from `packages/web-ui/assets/app.css`.

## Impact Areas

- DB: one additive migration on `flakes`.
- Server: `flake/commits.rs`, `handlers/api/flakes.rs` (call sites + registry DTO), new `handlers/api/navigation.rs`, new `queries/navigation.rs`, `api/models.rs`, `bin/server.rs` route.
- Web UI: `components/layout/sidebar.rs`, new `alerts/` module, new `components/flake/` components, view wiring in systems_list/flakes_list/environments_list/builds/evaluations/cves, `api/models.rs` + `api/client.rs`, `assets/app.css`.
- Fixtures/checks: fixture JSON, `fixtures/seed.rs`, `checks/web-ui/coverage-manifest.json`.

## Risk

**Medium.** Migration is additive; sync recording wraps existing calls without changing sync behavior; badge endpoint is read-only COUNTs. Main risk is touching six views for flash/ack wiring — mitigated by keeping the logic in one shared module and only adding a conditional class per view.

## Dependencies

None blocking. Coordinate with TASK-384 (also touches systems_list.rs/system_detail.rs) — whichever merges second resolves conflicts.

## Verification Plan (Tier 2)

Per doc-18 §8: fmt + clippy `-D warnings` + tests in both crates; `db-only up` + `cargo sqlx prepare`; `nix build` server, web-ui, and checks web-ui (must produce new screenshots: sidebar badges, flakes error chip/callout, tray "Sync failed" banner); `nix flake check --keep-going` (migration + server surface). MR attaches the screenshots via GitLab uploads — never committed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 New additive migration adds sync_status/last_sync_at/last_sync_error to flakes with the status CHECK constraint; no existing migration modified; cargo sqlx prepare regenerated and consistent
- [ ] #2 All four sync call sites route through sync_flake_recorded, which persists syncing -> synced (clears error) and syncing -> error (stores truncated error text + last_sync_at); status write failures are logged and never mask the sync result; DB tests cover both transitions
- [ ] #3 Flakes registry API returns sync_status, last_sync_at, last_sync_error for every flake; a real failed sync (e.g. unreachable repo URL) produces status error with the actual error text retrievable via the API
- [ ] #4 GET /api/v1/navigation/badges returns all nine counts per the doc-18 DTO; systems_attention reuses the Systems-list health semantics; cves_critical reuses the /api/v1/cves/stats query logic; a DB test asserts each count against seeded fixtures
- [ ] #5 Sidebar renders count badges per design: red nav-count-alert when attention, gray informational otherwise, exact tooltips from Shell.jsx (e.g. 'N of M flakes failing to sync'), badges visible in collapsed rail mode, driven by one 30s-polled fetch
- [ ] #6 Badge acknowledgment matches design: visiting Systems/Flakes/Environments/CVEs hides that attention badge for the page load; Builds and Evaluations badges clear only when the tab containing failures is opened; visibility rule is a unit-tested pure function
- [ ] #7 Attention flash matches design: alerting rows/cards (flake sync errors, critical/offline systems, attention environments, failed builds, failed evals, critical CVEs) get the attention-flash pulse exactly once per page load on first visit; flash-once logic is unit tested
- [ ] #8 Flakes view renders FlakeSyncChip (synced/syncing/error/unknown) in table + cards with last_sync_error tooltip, and errored cards show the design error callout
- [ ] #9 Flake tray shows the design Sync failed banner for errored flakes: warn icon + 'Sync failed' + relative last_sync_at + pre block with 'nix flake metadata {url}' and the real error + last-good-commit and remote meta + working 'Retry sync' button that re-syncs and refreshes status
- [ ] #10 Flakes page subtitle synced count uses the new real sync_status field
- [ ] #11 Fixtures seed one errored flake (realistic multi-line error, relative timestamps), one synced, one unknown; checks/web-ui coverage-manifest asserts + screenshots: sidebar badge counts, flakes error chip/callout with attention-flash class, tray Sync failed banner with seeded error text, and badge hidden after view acknowledgment
- [ ] #12 All verification passes: fmt + clippy -D warnings + cargo test in packages/default and packages/web-ui; sqlx prepare; nix build of server, web-ui, checks web-ui; nix flake check --keep-going; MR attaches web-ui check screenshots via GitLab uploads (not committed)
<!-- AC:END -->
