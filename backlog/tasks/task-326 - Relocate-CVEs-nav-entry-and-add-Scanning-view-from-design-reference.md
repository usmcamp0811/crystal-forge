---
id: TASK-326
title: Relocate CVEs nav entry and add Scanning view from design reference
status: Review
assignee: []
created_date: '2026-05-31 02:20'
updated_date: '2026-06-01 01:57'
labels:
  - ui
  - navigation
  - cve
  - scanning
  - web-ui
milestone: UI/UX Refresh
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/ScanningView.jsx
  - packages/web-ui/src
modified_files:
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/api/client.rs
  - packages/default/src/api/models.rs
  - packages/default/src/handlers/api/scanning.rs
  - packages/default/src/handlers/api/mod.rs
  - packages/default/src/bin/server.rs
  - packages/default/src/queries/scanning.rs
  - packages/default/src/queries/scanning_tests.rs
  - packages/default/src/queries/mod.rs
  - packages/default/migrations/0124_add_scan_schedule_policy.sql
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The sidebar information architecture has changed: CVEs is moving to a different sidebar section, and the old CVEs slot should now open a new Scanning view. The current UI does not reflect this navigation/layout change and does not implement the new Scanning page.

## Goal

1. Update sidebar/navigation so CVEs appears in its new section.
2. Add a new Scanning view at the previous CVEs sidebar location.
3. Implement Scanning view UI to match `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/ScanningView.jsx`.

## Non-Goals

- No unrelated refactors in other views.

## Scope

- Navigation updates (route/label/order/section placement).
- New Scanning view component/page in web-ui.
- Wiring route + sidebar item to new view.
- Data can use existing API endpoints/models where available; if missing, implement requiered endpoints inorder to give the UI dat

## Architectural Constraints

- Keep business logic out of view rendering.
- Follow existing web-ui view patterns and routing conventions.
- Keep change set focused to nav + new Scanning view only.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Sidebar places CVEs in the new requested section/order
- [x] #2 Old CVEs sidebar location now routes to a new Scanning view
- [x] #3 Scanning view structure and interaction closely match ScanningView.jsx design reference
- [x] #4 Route wiring is complete and direct navigation to Scanning view works
- [x] #5 Existing CVEs page remains accessible at its new navigation location
- [x] #6 Empty/loading/error states in Scanning view follow existing app style patterns
- [x] #7 No unrelated view/sidebar regressions introduced
- [x] #8 Web UI builds successfully with the changes
- [x] #9 Backend: scan_schedule_policy persisted via new migration (singleton row): on_build, deployed_interval, recent_interval, archived_interval, archived_enabled, rebuild_to_scan
- [x] #10 Backend: admin-gated GET/PUT /api/v1/scanning/schedule returns and updates the policy
- [x] #11 Backend: admin-gated GET /api/v1/scanning/stats returns live scanning-now/queued/stale/never-scanned/failed/coverage derived from cve_scans
- [x] #12 Backend: admin-gated GET /api/v1/scanning/queue returns active & recent scans (joins cve_scans + evaluation_targets + commits/flakes + systems)
- [x] #13 Backend: admin-gated GET /api/v1/scanning/systems returns per-system grouped configs with per-commit scan rows (freshness, status, findings, last scan)
- [x] #14 Backend: admin-gated GET /api/v1/scanning/activity returns recent scan activity derived from cve_scans (started/completed/failed) using completed_at/scheduled_at timestamps
- [x] #15 Backend: new handlers registered in bin/server.rs and cargo sqlx prepare metadata regenerated and committed
- [x] #16 Backend: query/handler unit tests cover aggregation and auth gating
- [x] #17 Frontend: Scanning view consumes the new endpoints via use_resource with loading/empty/error states (no fabricated data)
- [x] #18 Frontend: All-configs view supports expandable per-system rows showing the nested per-commit scan table (structural parity with ScanningView.jsx)
- [x] #19 Frontend: Schedule modal is bound to live policy (GET) and saves via PUT
- [x] #20 Freshness semantics: freshness = recency of vulnix scan; recent cutoff = 30 days; stale = last successful scan older than configured interval for the class; needs-build = derivation not in any cache
- [x] #21 Worker enforcement of policy intervals/flags is explicitly out of scope and tracked by TASK-327
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Updated queue/UI parity per user request: added backend queue fields freshness/is_current/trigger contract + icon-level parity conversion in Scanning view.

Commit: 2a3c27ea pushed to MR !267.
<!-- SECTION:NOTES:END -->
