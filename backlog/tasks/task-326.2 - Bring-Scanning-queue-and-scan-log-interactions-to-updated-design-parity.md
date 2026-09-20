---
id: TASK-326.2
title: Bring Scanning and per-system CVE triage to updated design parity
status: In Progress
assignee:
  - Matt Camp
created_date: '2026-09-09 03:32'
updated_date: '2026-09-20 02:17'
labels:
  - scanning
  - web-ui
  - design-parity
  - cve-triage
dependencies:
  - TASK-326.1
  - TASK-337
references:
  - e79d0ad6
  - ad6589e1
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/329'
documentation:
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/data-scanning.js
modified_files:
  - checks/web-ui/tests/integration-test.js
  - packages/default/.sqlx/query-43a8f1b05cacc24d800c9e8e393b1c5c666c277bb0.json
  - packages/default/.sqlx/query-90fc9f824b565ab238a995ad4a4c5a85dea79525ee.json
  - >-
    packages/default/crates/cf-server/migrations/0269_scanning_lifecycle_archive.sql
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/bin/server.rs
  - packages/default/crates/cf-server/src/builder/cve_worker.rs
  - packages/default/crates/cf-server/src/handlers/api/cves.rs
  - packages/default/crates/cf-server/src/handlers/api/poam.rs
  - packages/default/crates/cf-server/src/handlers/api/scanning.rs
  - packages/default/crates/cf-server/src/handlers/api/systems.rs
  - packages/default/crates/cf-server/src/models/cve_scans.rs
  - packages/default/crates/cf-server/src/queries/cve_scan_diagnostics.rs
  - packages/default/crates/cf-server/src/queries/cve_scan_leases.rs
  - packages/default/crates/cf-server/src/queries/cve_scans.rs
  - packages/default/crates/cf-server/src/queries/scanning.rs
  - packages/default/crates/cf-server/src/queries/scanning_tests.rs
  - packages/default/crates/cf-server/src/services/cve_scans.rs
  - packages/default/crates/cf-server/src/services/poam.rs
  - packages/default/crates/cf-server/tests/poam_workflows.rs
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/cve/mod.rs
  - packages/web-ui/src/components/cve/triage.rs
  - packages/web-ui/src/views/cves.rs
  - packages/web-ui/src/views/poam_api.rs
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/views/system_detail.rs
parent_task_id: TASK-326
priority: high
type: enhancement
ordinal: 473000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rewrite the production Scanning view to align with e79d0ad6 (Active/Completed/BySystem tabs with wait states, archive/restore, failure diagnostics) and add unified CVE triage modal to System Detail per ad6589e1 (environment-scoped outstanding/accepted/scheduled with POA&M create/reuse, typed assignee).

## Scanning: Active vs Completed (not Deployed/All)

**Active tab**: scanning/queued/awaiting states; "awaiting" = blocked on build/cache (not failure). Selection limits to cancellable rows (scanning/queued). Stats show "scanning now" with breakdown: scanning · queued · awaiting closure.

**Completed tab**: terminal results + history. Archive/restore without deletion; archived rows hidden and visually distinct. Selection for history ops (archive/restore), not cancellation. Filtered empty views caused by retention say so, not "no scans exist".

**BySystem**: per-revision history, newest-first, with superseded config terminology; hidden count + link; unscanned/needs-build explicit.

**Controls**: Live indicator, Schedule button, no global Rescan All (check if rescan actions move elsewhere).

**Filtering/Sorting**: text search, status, revision/freshness, latest-per-flake; deterministic sortable columns (status/severity/revision/timestamp); visible result count; resettable empty state.

**Detail tray**: real status, trigger, scanner identity, findings, failure context, bounded log content (no fake progress).

**Failed actionability**: stat card click jumps to Completed, opens failing scan's log.

**By-system history**: exact per-revision scans, needs-build/never-scanned explicit, hidden older scans show count.

**Wait states**: explicit "awaiting: <reason>"; stale/failed provide Check now/Retry/Build with exact deep links.

## System Detail CVE: Unified Triage Modal

Replace separate Justify + Create POA&M with one CveTriageModal (extracted from fleet CVE) scoped to current system's environment.

**Environment-scoped decision**: each environment gets outstanding/accepted/scheduled choice. Accepted requires justification + optional review date. Scheduled creates/reuses POA&M with owner, due, plan, optional milestones.

**Presentation**: add Triage column (outstanding/accepted/scheduled state); row action opens modal; no separate Justify/Create buttons.

**Authority (CRITICAL)**: preserve TASK-440 rules—exact current evidence can support triage, legacy not promoted, missing/no-scan explicit, accepted/scheduled do NOT false-claim remediation/verification, only exact evidence/verification supports closure.

## Dependencies & Overlap

- TASK-326.1 (wait states, logs) & TASK-337 (trigger) are blockers for detail/log implementation
- TASK-348.2 superseded by this task; make canonical and note supersession
- Reuse existing TASK-440 CVE triage domain/client; preserve authority constraints
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Active/Completed/BySystem tabs with correct state classification, selection semantics, and no activity side panel; Live indicator and Schedule button in header
- [x] #2 Active: scanning/queued/awaiting; Completed: terminal + history with archive/restore; BySystem: per-revision newest-first with superseded terminology
- [x] #3 Filters: text search, status, revision/freshness, latest-per-flake; sortable deterministic columns; visible result count; resettable empty state
- [x] #4 Detail tray: real status, trigger, scanner identity, findings, failure context, bounded log (no fake progress); log has search/download; running state shows elapsed time
- [x] #5 Failed stat card actionable (count > 0): click jumps to Completed tab, opens log; stale/failed rows show Check now/Retry/Build with exact deep links
- [x] #6 Selection guards: Active limits to scanning/queued (cancellation); Completed allows all (archive/restore); BySystem no selection
- [x] #7 Archive/restore on Completed: rows hidden not deleted, archived visually distinct, filtered counts honest, retention-caused empty states labeled
- [x] #8 System Detail CVE: unified triage modal (extracted from fleet view), environment-scoped, with Triage column and row action; no separate Justify/Create buttons
- [x] #9 Triage modal: outstanding/accepted/scheduled per environment; accepted needs justification; scheduled creates/reuses POA&M with owner/due/plan/milestones; typed assignee
- [x] #10 Authority: exact current evidence supports triage, legacy not promoted, missing/no-scan explicit, accepted/scheduled do NOT false-claim remediation/verification
- [x] #11 Browser assertions: Active/Completed switching, wait state rendering, failure actionability, archive/restore selection, log content, triage disposition, POA&M reuse, responsive/narrow/dark behavior
- [x] #12 Playwright: Active/Completed filtering, wait/failed transitions, log drawer, triage modal outstanding→accepted→scheduled, POA&M reuse, responsive tests; desktop/narrow/light/dark screenshots
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Started implementation in dedicated worktree `/home/mcamp/code/crystal-forge/TASK-326.2-scanning-cve-triage-parity` on branch `TASK-326.2-scanning-cve-triage-parity`, based exactly on `origin/dev` at `e304867c43abc8a7d8efe1f01e71af64cca67d8b`. The integration worktree contains unrelated untracked `session-ses_f927.md`; it does not overlap task scope and will not be modified.
<!-- SECTION:NOTES:END -->
