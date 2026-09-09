---
id: TASK-433.8
title: 'TASK-433 Phase 7: Dashboard, notifications, and setup-coach POA&M integration'
status: Done
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-09-02 02:41'
labels:
  - design-parity
  - poam
  - web-ui
  - dashboard
  - phase-7
dependencies:
  - TASK-433.7
references:
  - TASK-433
  - TASK-433.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - >-
    https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318#note_3755751599
documentation:
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/components/SetupCoach.jsx
  - docs/design/CrystalForge/data-dashboard.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 7 of 8 (contextual only). Extends the existing notification and six-step setup-wizard conventions to POA&M, and wires the dashboard POA&M Summary/Watchlist to the Phase 5 API.

## Explicit scope
- Dashboard: real POAM Summary/Watchlist widgets using real batched APIs, preserving existing layouts and opening detail views (layout migration only where required).
- Notifications: real deduplicated overdue/awaiting-verification POAM events with target/read/dismiss/navigation and no render/poll spam.
- Setup coach: production-derived policy, bundle and Track-a-POAM steps added without breaking existing coach progress state.

## Explicit non-scope
No POA&M API changes (Phase 5 owns the API). No localStorage/`window.__cfCoach`/`CustomEvent` state; use existing coach/notification persistence conventions.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Dashboard POAM Summary and Watchlist use real batched APIs, preserve existing layouts and open detail.
- [ ] #2 POAM notifications are real deduplicated events with target/read/dismiss/navigation and no render/poll spam.
- [x] #3 Setup coach adds production-derived policy, bundle and Track a POAM steps without breaking existing progress.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 7 implementation plan

1. Add additive migration `0235_poam_notifications.sql`. Extend canonical attention occurrences with the `poams` category, enforce one unresolved overdue episode per POA&M, and extend `notification_visible_to_user` with the existing all-context POA&M visibility rule. Preserve every existing notification row, category, preference, and email behavior.
2. Add bounded server-side overdue reconciliation to the existing 120-second attention worker. One uninterrupted `status != completed && target_date < server_today` interval is one occurrence; resolving the condition closes that occurrence, and a later overdue interval receives a new occurrence ID. Use the existing subject advisory-lock convention plus a partial unique-index backstop.
3. Extend the existing notification producer, not GET handlers or the browser. Materialize overdue notifications from POA&M attention occurrence IDs and awaiting-verification notifications from immutable `poam_activity.status_changed` IDs. Reuse `policy_violations` preferences/delivery boundaries, exact server authorization, per-user durable inbox rows, existing read/dismiss/email semantics, and database `ON CONFLICT` uniqueness. Remove notification-generation side effects from notification GET.
4. Add one AppShell-owned notification refresh path with auth-generation stale-response protection and overlap suppression. Extend typed Dioxus notification routing for `/compliance?poam=<UUID>` so the common Phase-6 detail tray reauthorizes and opens the exact POA&M. Do not add a POA&M-specific timer or derive events from live watchlist state.
5. Add typed Web UI adapters for the existing Phase-5 `/poams/dashboard` and `/poams/dashboard/watchlist` sources. Load one summary request and one bounded watchlist request at DashboardView scope, preserve independent loading/error/empty states, and never aggregate the broad POA&M list in the browser.
6. Register `poam-summary` and `poam-watchlist` in the production Security widget library, render authoritative counts and ordered attention rows, make rows keyboard-actionable, and route exact UUIDs to the common detail tray. Include both widgets in fresh defaults because TASK-433.8 requires both.
7. Increment `StoredLayout` from version 2 to 3 and add a pure idempotent migration. Preserve recognized pre-v3 order and dimensions, add each new widget once, persist version 3, and never re-add a widget removed after migration.
8. Extend the admin-only server-derived setup progress response with `policy`, `bundle`, `poam`, and an additive nine-step aggregate while retaining the original `all_required_complete` five-infrastructure-step contract. Policy completes only for a distinct policy lineage with an attributable user-created/imported version (`created_by`); update manual creation attribution so seeded defaults do not count. Bundle completes for any persisted bundle lineage. POA&M completes for any persisted POA&M in any lifecycle state, including completed history.
9. Extend the existing single eight-second coach-progress resource to nine steps and typed Dioxus navigation. Preserve the original six rules, server-backed dismiss, presentation state, and one refresh path. Policy navigates to Deployment Policies, Bundle to Compliance, and Track a POA&M to Compliance with explicit finding-context instructions; no generic POA&M create action or client completion state is added.
10. Add database/server regressions for overdue episode lifecycle, awaiting transition/re-entry, concurrent reconciliation/materialization dedupe, preferences/visibility, read/dismiss persistence, historical retention, and read-only GETs; setup-progress production counts and original-six preservation; dashboard DTO/layout/presentation/navigation tests; and real server-backed browser workflows for dashboard/layout/error/navigation, notifications/history/polling, and nine-step coach behavior.
11. Run targeted checks, isolated migration and SQLx preparation, required server/Web UI builds and suites, authoritative Web UI check, JavaScript syntax, diff checks, and full flake check when focused checks pass. Then perform independent requirements, notification, polling, dashboard, coach, authorization, backward-compatibility, and design-comparison reviews. Resolve every P0/P1/P2.
12. Only after AC1-AC3 are objectively proven: check exactly three criteria, move TASK-433.8 to Review, update MR !318 with the Phase-7 SHA and verification, commit/push all intended changes, confirm clean matching local/remote heads, and stop before TASK-433.9.

Phase-8 owner remediation: replace per-user repeated full-history notification materialization with a bounded set-oriented producer that preserves per-user visibility/preferences, event identity, read/dismiss history, awaiting re-entry semantics, and idempotent uniqueness. Add query-count/history-scale regression and ensure overdue reconciliation plus all notification ignored DB tests run in authoritative server-regressions. Preserve the single AppShell poll and no-side-effect GET contract.

AC2 P1 history-scale remediation (migration 0243 only): add a durable bounded active-user initialization queue with trigger-fed start cursors; consume at most the producer user limit and remove all-active preference/schedule/cursor initialization plus correlated per-user history MIN scans. Replace notification source bootstrap with indexed keyset limits in each attention/activity/system source branch before merging the bounded candidates. Bound immediate-email cursor initialization similarly. Add large many-user and large-history plan/state regressions, preserve combined POA&M visibility/preferences/read-dismiss/re-entry semantics, and keep the full notification plus overdue ignored suites in server-regressions. Verify formatting, SQLX_OFFLINE checks, focused isolated PostgreSQL tests, migration compatibility, and server-regressions where feasible. Do not commit.

2026-08-31 responsive parity remediation in `/tmp/opencode/TASK-433-publish`: limit edits to dashboard, topbar notifications, Setup Coach, shared CSS, and focused Rust/source tests. Render every watchlist row's authoritative POA&M ID, lifecycle status, urgency, owner, title, and optional due date from `PoamSummary`, including narrow layouts. Keep notification content limited to server-provided title/summary/created timestamp, retain all read/dismiss/load/settings actions, and make the fixed panel viewport-safe. Keep Setup Coach nonmodal, add explicit action labels, and use a bottom-anchored, height-bounded scrolling presentation at narrow widths so page-header actions remain available at 560px and 375px. Improve dark secondary/disabled tokens and avoid opacity-reduced locked coach text. Verify web-ui rustfmt, focused tests, a WASM check, and `git diff --check`; do not run or edit the browser harness and do not commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-7 preflight passed at branch head `7d6eef77be055f05ead86c7ac2eecc1d44ff1b18`: local and remote branch heads match, the worktree is clean, MR !318 is conflict-free, and TASK-433.7 is Review with AC1-AC4 checked. The authoritative Phase-6 Web UI check passed at implementation head `ff183d032a8dcc4794acf7064eb7acd8e61cdcb6`; `7d6eef77` is bookkeeping only. Phase-7 scope is dashboard, durable notifications, and production-derived setup coach steps. TASK-433.9 and final Phase-8 parity work remain prohibited.

Architecture audit: `fetch_setup_wizard_progress()` calls the admin-only `GET /api/v1/admin/setup-progress`; `handlers/api/setup_wizard.rs` derives the five infrastructure steps from production entity counts and reads the per-user dismissal/agent-acknowledgment flags from `users`. The coach owns one eight-second progress refresh loop. The original rules remain unchanged: environment, flake, any builder, any cache destination, linked system, and per-user agent acknowledgment.

Notification audit: `user_notifications` is the durable per-user inbox; `user_notification_preferences` owns category/channel boundaries; `run_user_notification_email_producer_pass` already materializes in-app events for all active users; read/dismiss/list/email all recheck `notification_visible_to_user`. The bell currently loads on mount/open without a timer, and notification GET currently materializes rows. Phase 7 will move all generation to the existing server workers, use the existing `policy_violations` preference category, and add one AppShell-owned refresh path.

Completion rules selected from authoritative storage: Policy counts distinct lineages with a user-attributed policy version so migration-seeded defaults do not falsely complete the step; manual policy creation will set the trigger-created version's `created_by`, while existing imports already attribute versions. Bundle counts persisted `compliance_bundles` lineages. Track a POA&M counts persisted `poams` regardless of current status so completed historical use remains complete.

AC3 setup coach implemented in the dedicated task worktree without commit or push. Extended server/Web setup-progress DTOs with policy, bundle, POA&M, and `all_coach_steps_complete`; Web additions default incomplete for rolling compatibility. Consolidated setup entity counts into one query while preserving the original five `all_required_complete` rules and agent acknowledgment. Policy counts distinct lineages with an attributed version; manual policy creation now attributes the trigger-created draft in the same transaction. Coach now renders nine ordered steps, retains the single eight-second refresh resource and server dismissal/minimize/force-show behavior, uses typed Dioxus routes, and gives finding/evidence-specific POA&M instructions. Added server scenario/query tests and Web DTO/order/status/navigation tests. Verification: scoped rustfmt passed; `git diff --check` passed; `SQLX_OFFLINE=true nix develop -c cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml setup_progress --lib` passed 12 tests. Full Web tests/check are currently blocked by concurrent out-of-scope notification-refresh compile errors in `components/layout/topbar.rs:157` and `components/layout/app_shell.rs:243-245`; those files were not modified for AC3.

Focused Phase-7 browser coverage added only in `checks/web-ui/tests/integration-test.js`: real fixture/server-backed dashboard summary and watchlist rendering with exact detail routing; v2 layout migration to idempotent v3 with both POA&M widgets; complete nine-step coach rendering and typed policy/bundle/POA&M destinations; fixture-backed durable bell read/dismiss requests and exact POA&M detail navigation. Updated the shared setup-progress mock and existing 06h payload for Phase-7 fields. Verification: `nix develop -c node --check checks/web-ui/tests/integration-test.js` passed with exit 0; `git diff HEAD --check -- checks/web-ui/tests/integration-test.js` passed. Full browser check was not requested or run.

Phase 7 final implementation and verification completed. Dashboard uses the existing server-computed summary/watchlist endpoints, shared view-owned resources keyed to authentication generation, independent retry/error/empty states, exact POA&M detail routing, and an idempotent v2-to-v3 layout migration that preserves recognized layouts and restores full defaults for empty/obsolete legacy layouts. Durable notifications use canonical overdue attention occurrence IDs and immutable awaiting-verification activity IDs, server-only materialization, all-active-user preference initialization, exact POA&M authorization, persistent read/dismiss state, account-safe AppShell polling, and exact detail navigation. Setup Coach now uses production-derived policy, bundle, and POA&M counts while retaining original-six semantics and falling back to the legacy six-step contract when an older server omits Phase-7 fields.

Database evidence used the repository-owned PostgreSQL service on 127.0.0.1:3042 with the fresh task-specific database `crystal_forge_task4338_phase7`. The complete migration chain through 0235 applied successfully. Ignored focused tests passed: overdue reconciliation lifecycle/concurrent dedupe (1), notification query suite including POA&M materialization/read/dismiss/dedupe and read-only preference behavior (7), and production setup-progress counts (1). `cargo sqlx prepare --workspace` succeeded and produced no SQLx metadata drift.

Final verification passed: Web UI formatting and full unit suite (246 passed, 1 ignored); targeted server setup/dashboard/notification suites; server all-target offline check; JavaScript syntax and `git diff --check`; server and Web UI Nix package builds; authoritative `nix build .#checks.x86_64-linux.web-ui --no-link`; and `nix flake check --keep-going` (`all checks passed`). The first authoritative rerun encountered a pre-existing nondeterministic Phase-6 fixture assertion in workflow 29g; an immediate identical rerun passed, and the final full flake check also passed. Browser evidence covers real dashboard APIs/layout/detail routing, all nine coach steps and typed destinations, POA&M notification read/dismiss/detail routing, and one non-overlapping AppShell polling interval. No TASK-433.9 work was started.

Final post-fix review found one pagination/polling overlap risk. The shared notification loader now mutually excludes first-page polling and pagination requests, and the Load more control is disabled during either request. This prevents a poll response from replacing the list while an older page appends. Post-fix verification passed: Web UI formatting, full Web UI unit suite (247 passed, 1 ignored), and `git diff --check`. A final independent re-review found no remaining P0, P1, or P2 blockers.

Phase-7 implementation commit `eb4097e7` was pushed to `TASK-433-policy-poam-workflows`. MR !318 was updated with the exact implementation SHA, verification evidence, and dashboard/Setup Coach and notification inbox screenshots. The exact-head authoritative Web UI Nix check passed after the final polling guard: `/nix/store/di9f5mmy77zxw3dgkc1mk3v8a6yw8nw3-vm-test-run-crystal-forge-web-ui-mega-integration`.

Phase-8 dependency gate on 2026-08-29 found the exact-head pipeline 2801611582 green at `a0d149c2`, but MR !318 had become conflict-blocked because `origin/dev` advanced to `90992d86`. Phase 7 is temporarily returned to In Progress for branch synchronization. The synchronization will merge current `origin/dev`, preserve all unrelated task/design updates exactly, resolve only overlapping TASK-433 files by current canonical state, run focused checks for any affected production files, push, and require a new exact-head green pipeline before TASK-433.9 starts.

Phase-7 dependency synchronization completed at merge commit `4e77d7db253d4f47a5ab128cea2e8dbcbe7a67b9`. Current `origin/dev` (`90992d86`) was merged. The only conflicts were stale TASK-433.6 and TASK-433.7 task metadata; each conflict was inspected and resolved to preserve the later Review state, checked acceptance criteria, and remediation evidence from the task branch. All unrelated backlog and authoritative design updates from `origin/dev` were retained unchanged. MR !318 is conflict-free. Exact-head pipeline 2802078582 passed at `4e77d7db`: https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2802078582. Phase 7 returns to Review; TASK-433.9 may now begin.

Phase 8 returned Phase 7 to In Progress for a notification producer performance defect. Each producer pass currently loops all active users and rescans complete eligible attention/activity history per user, then email selection repeats broad scans. Uniqueness prevents duplicates but not unbounded users×history work. AC2 is temporarily unchecked pending bounded set-oriented materialization, scale/dedupe/history regressions, and authoritative execution of the relevant ignored notification and overdue tests.

AC2 P1 history-scale remediation implemented without commit in `/tmp/opencode/TASK-433-publish`. Added additive migration `0243_bounded_notification_initialization_and_bootstrap.sql`; migrations 0233-0242 were not edited. The producer now claims at most 32 durable user-initialization queue rows, removes correlated per-user source-history `MIN` scans, retains fair source/event cursors, and bounds immediate-email cursor initialization. The source bootstrap now applies indexed keyset cursor plus limit independently to attention, POA&M activity, and system-event branches before merging at most three bounded batches. New regressions cover 96-user initialization fairness, an exact production `EXPLAIN ANALYZE` over 4,096 resolved attention rows (including required attention keyset index and four branch/merge Limit nodes), and combined per-user POA&M visibility, preference boundary, awaiting re-entry identity, and read/dismiss persistence. Existing `checks/server-regressions/default.nix` already runs the complete ignored notification query group, email producer group, and overdue lifecycle test, so no Nix edit was required.

Verification: fresh isolated PostgreSQL migration chain through 0243 passed. `SQLX_OFFLINE=true nix develop -c cargo check --offline --manifest-path packages/default/crates/cf-server/Cargo.toml --lib` passed with existing repository warnings. Scoped rustfmt and `git diff --check` passed. Isolated DB results: `queries::user_notifications::tests::` 25/25 passed; `tasks::user_notification_email::tests::` 6/6 passed; `poam_overdue_reconciliation_deduplicates_resolves_and_opens_new_episode` 1/1 passed. `nix build .#checks.x86_64-linux.server-regressions --no-link` applied the migration/upgrade rehearsal but stopped before the notification section because concurrently modified, out-of-scope `poam_workflows.rs` failed at lines 631 and 3068 (25 passed, 2 failed). The initial repository-wide cargo fmt check was also blocked by an unrelated pre-existing formatting difference in `tests/poam_workflows.rs:629`. AC2 remains unchecked and the task remains In Progress pending reviewer disposition of those unrelated authoritative-check blockers.

2026-08-31 responsive dashboard/notification/Setup Coach parity remediation implemented in `/tmp/opencode/TASK-433-publish` without browser-harness edits or commit. Dashboard watchlist rows now retain the authoritative `PoamSummary` human ID, title, lifecycle status, overdue/awaiting urgency, owner, and optional due date at narrow widths, with one complete accessible action label. The notification panel now uses fixed safe-area-aware `dvw`/`dvh` bounds and an internally scrolling list; all mark-read, item navigation, dismiss, pagination, and settings actions remain. Notification rendering uses only the server DTO title, summary, and exact `created_at` timestamp, with unread and action labels. Setup Coach remains a nonmodal complementary region with no backdrop or dimming; at <=640px it becomes a bottom-anchored 56dvh-bounded scrolling sheet so 560px/375px page-header actions remain available, and minimize/dismiss/refresh have explicit labels. Dark secondary/muted/disabled tokens now meet the focused AA assertions, and locked coach content no longer loses contrast through opacity. Verification passed: direct scoped Nix `rustfmt --check`; `cargo test --manifest-path packages/web-ui/Cargo.toml task433_responsive` (3 passed); `cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`; scoped `git diff --check`. Repository-wide `cargo fmt -- --check` remains blocked only by pre-existing out-of-scope formatting differences in `components/compliance/mod.rs` and `views/policies.rs`. No browser harness was run.
<!-- SECTION:NOTES:END -->
