---
id: TASK-326
title: Relocate CVEs nav entry and add Scanning view from design reference
status: In Progress
assignee: []
created_date: '2026-05-31 02:20'
updated_date: '2026-05-31 03:27'
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
- [ ] #1 Sidebar places CVEs in the new requested section/order
- [ ] #2 Old CVEs sidebar location now routes to a new Scanning view
- [ ] #3 Scanning view structure and interaction closely match ScanningView.jsx design reference
- [ ] #4 Route wiring is complete and direct navigation to Scanning view works
- [ ] #5 Existing CVEs page remains accessible at its new navigation location
- [ ] #6 Empty/loading/error states in Scanning view follow existing app style patterns
- [ ] #7 No unrelated view/sidebar regressions introduced
- [ ] #8 Web UI builds successfully with the changes
- [ ] #9 Backend: scan_schedule_policy persisted via new migration (singleton row): on_build, deployed_interval, recent_interval, archived_interval, archived_enabled, rebuild_to_scan
- [ ] #10 Backend: admin-gated GET/PUT /api/v1/scanning/schedule returns and updates the policy
- [ ] #11 Backend: admin-gated GET /api/v1/scanning/stats returns live scanning-now/queued/stale/never-scanned/failed/coverage derived from cve_scans
- [ ] #12 Backend: admin-gated GET /api/v1/scanning/queue returns active & recent scans (joins cve_scans + evaluation_targets + commits/flakes + systems)
- [ ] #13 Backend: admin-gated GET /api/v1/scanning/systems returns per-system grouped configs with per-commit scan rows (freshness, status, findings, last scan)
- [ ] #14 Backend: admin-gated GET /api/v1/scanning/activity returns recent scan activity derived from cve_scans (started/completed/failed) using completed_at/scheduled_at timestamps
- [ ] #15 Backend: new handlers registered in bin/server.rs and cargo sqlx prepare metadata regenerated and committed
- [ ] #16 Backend: query/handler unit tests cover aggregation and auth gating
- [ ] #17 Frontend: Scanning view consumes the new endpoints via use_resource with loading/empty/error states (no fabricated data)
- [ ] #18 Frontend: All-configs view supports expandable per-system rows showing the nested per-commit scan table (structural parity with ScanningView.jsx)
- [ ] #19 Frontend: Schedule modal is bound to live policy (GET) and saves via PUT
- [ ] #20 Freshness semantics: freshness = recency of vulnix scan; recent cutoff = 30 days; stale = last successful scan older than configured interval for the class; needs-build = derivation not in any cache
- [ ] #21 Worker enforcement of policy intervals/flags is explicitly out of scope and tracked by TASK-327
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
<!-- SECTION:NOTES:BEGIN -->

LOCK: gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-326-scanning-view-nav-relocation

<!--
SECTION:NOTES:END
-->

Confirmed design decisions (user, 2026-05-31):

- Freshness = recency of the vulnix CVE scan (not config age). 'recent' scan cutoff = 30 days.

- Stale = last successful scan older than configured interval for the freshness class.

- needs-build = derivation no longer in any cache; must be rebuilt before it can be scanned.

- Activity feed derived from cve_scans; cve_scans.completed_at is the authoritative scan timestamp (scheduled_at/created_at/scan_duration_ms also available; index on completed_at).

- Schedule policy: minimal work now = persist + expose only; worker enforcement deferred to TASK-327 (high priority, gated on this view merging).

Backend grounding: cve_scans -> evaluation_targets (evaluation_target_id) -> commits -> flakes; systems join via target_name = hostname. 'config' = commit/derivation target; per-system grouping maps to reference per-system -> per-commit layout. Existing reusable assets: queries/cve_scans.rs, services/cve_scans.rs, builder/cve_worker.rs, view_systems_cve_summary, view_system_vulnerabilities, dashboard::cve_scan_freshness. Fleet rescan enqueue remains TASK-325 (still 501).
<!-- SECTION:NOTES:END -->
