---
id: TASK-433.8
title: 'TASK-433 Phase 7: Dashboard, notifications, and setup-coach POA&M integration'
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-08-29 02:51'
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
documentation:
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/components/SetupCoach.jsx
  - docs/design/CrystalForge/data-dashboard.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 440000
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
- [ ] #1 Dashboard POAM Summary and Watchlist use real batched APIs, preserve existing layouts and open detail.
- [ ] #2 POAM notifications are real deduplicated events with target/read/dismiss/navigation and no render/poll spam.
- [ ] #3 Setup coach adds production-derived policy, bundle and Track a POAM steps without breaking existing progress.
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
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-7 preflight passed at branch head `7d6eef77be055f05ead86c7ac2eecc1d44ff1b18`: local and remote branch heads match, the worktree is clean, MR !318 is conflict-free, and TASK-433.7 is Review with AC1-AC4 checked. The authoritative Phase-6 Web UI check passed at implementation head `ff183d032a8dcc4794acf7064eb7acd8e61cdcb6`; `7d6eef77` is bookkeeping only. Phase-7 scope is dashboard, durable notifications, and production-derived setup coach steps. TASK-433.9 and final Phase-8 parity work remain prohibited.

Architecture audit: `fetch_setup_wizard_progress()` calls the admin-only `GET /api/v1/admin/setup-progress`; `handlers/api/setup_wizard.rs` derives the five infrastructure steps from production entity counts and reads the per-user dismissal/agent-acknowledgment flags from `users`. The coach owns one eight-second progress refresh loop. The original rules remain unchanged: environment, flake, any builder, any cache destination, linked system, and per-user agent acknowledgment.

Notification audit: `user_notifications` is the durable per-user inbox; `user_notification_preferences` owns category/channel boundaries; `run_user_notification_email_producer_pass` already materializes in-app events for all active users; read/dismiss/list/email all recheck `notification_visible_to_user`. The bell currently loads on mount/open without a timer, and notification GET currently materializes rows. Phase 7 will move all generation to the existing server workers, use the existing `policy_violations` preference category, and add one AppShell-owned refresh path.

Completion rules selected from authoritative storage: Policy counts distinct lineages with a user-attributed policy version so migration-seeded defaults do not falsely complete the step; manual policy creation will set the trigger-created version's `created_by`, while existing imports already attribute versions. Bundle counts persisted `compliance_bundles` lineages. Track a POA&M counts persisted `poams` regardless of current status so completed historical use remains complete.
<!-- SECTION:NOTES:END -->
