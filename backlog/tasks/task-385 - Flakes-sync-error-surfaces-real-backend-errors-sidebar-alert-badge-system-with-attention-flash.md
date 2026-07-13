---
id: TASK-385
title: >-
  Flakes sync-error surfaces (real backend errors) + sidebar alert badge system
  with attention flash
status: Review
assignee: []
created_date: '2026-07-08 07:26'
updated_date: '2026-07-08 13:13'
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on host in /home/mcamp/code/crystal-forge/TASK-385-flakes-sync-errors-sidebar-badges

Branch: TASK-385-flakes-sync-errors-sidebar-badges
Base: dev (ddef201c)
MR: !298 (https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/298)

## Latest Changes (2026-07-11)

Three UX fixes added in commit 95f19a20:

1. **Attention-flash on eval history rows** - Failed evaluation rows now receive the `attention-flash` CSS class (red glow + left-border pulse) on first page load, matching the pattern already used in flake error rows.
2. **Attention-flash on build completed rows** - Failed build rows in the Completed tab receive the `attention-flash` CSS class via a new `flash_failed` prop on `BuildQueuePane`.
3. **Eval history default-all selected** - Evaluation history checkbox column auto-selects all items on first successful data load.
4. **Sidebar badge query alignment** - Navigation badges endpoint now queries `build_jobs` (instead of `derivations`) for the builds badge, and the arbitrary 24-hour time filter was removed from both builds and evals badge queries to match view semantics.

## Latest Changes (2026-07-13)

1. **Rebase onto origin/dev** - Rebased branch onto dev (33 commits), resolving conflicts in `app.css`, `builds.rs`, and `components/builds/build_queue_pane.rs` (duplicate `flash_failed` prop from the parallel TASK-390 merge).
2. **Completed/History tab flash acknowledgment fixed** - `acked_hist` was a component-local signal that reset on every remount, so the tab badge pulse restarted on every re-visit to Builds/Evaluations. Added `alerts::is_acknowledged()` and gated the flash condition on it (persists for the page load via the existing global `ALERT_STATE`).
3. **Tab flash re-triggers on new failures** - Added peak-failed-count tracking (`peak_failed` signal) in both `builds.rs` and `evaluations.rs`; when the failed count increases after acknowledgment, `alerts::reset_acknowledge()` clears the acknowledged/flashed flags so the badge pulses again for genuinely new failures.
4. **Fixed "History Rewrite Detected" modal infinite loop (user-reported bug)** - `extract_history_rewrite_conflict()` in `flakes_list.rs` matched ANY 500-status sync failure containing the substring "failed to sync" as a rewrite conflict. Since every generic sync failure is formatted as `"Failed to sync {name} from source: {err}"`, this misclassified normal errors (network issues, bad credentials, "Failed to initialize commits for {url}") as history rewrites. Clicking "Accept rewrite and resync" then purged the flake's healthy commit history and retried the sync; if the real underlying error persisted, the retry failed with the same message, which was misclassified again — an infinite loop that destroyed commit lineage on every iteration. Fix: only trust the backend's canonical signal (`code == 409` + explicit `history_rewrite_detected` marker text, per `is_history_rewrite_error` in `flake/commits.rs` / TASK-216). Added 3 unit tests (genuine 409 case, the exact reported regression case, and marker-text-on-wrong-status-code). Reported root cause for the specific flake in the bug report ("boterf-config", `https://gitlab.com/michaelboterf/nix-configurations`) is a separate, still-unresolved connectivity/credentials issue — the fix stops the misleading modal loop but the underlying sync failure itself needs separate investigation (repo reachability / credentials for that flake).
<!-- SECTION:NOTES:END -->
