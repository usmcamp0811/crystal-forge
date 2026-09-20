---
id: TASK-326.2
title: Bring Scanning and per-system CVE triage to updated design parity
status: To Do
assignee: []
created_date: '2026-09-09 03:32'
updated_date: '2026-09-20 02:08'
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
documentation:
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/CvesView.jsx
  - docs/design/CrystalForge/data-scanning.js
modified_files:
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/cve/mod.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
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
- [ ] #1 Active/Completed/BySystem tabs with correct state classification, selection semantics, and no activity side panel; Live indicator and Schedule button in header
- [ ] #2 Active: scanning/queued/awaiting; Completed: terminal + history with archive/restore; BySystem: per-revision newest-first with superseded terminology
- [ ] #3 Filters: text search, status, revision/freshness, latest-per-flake; sortable deterministic columns; visible result count; resettable empty state
- [ ] #4 Detail tray: real status, trigger, scanner identity, findings, failure context, bounded log (no fake progress); log has search/download; running state shows elapsed time
- [ ] #5 Failed stat card actionable (count > 0): click jumps to Completed tab, opens log; stale/failed rows show Check now/Retry/Build with exact deep links
- [ ] #6 Selection guards: Active limits to scanning/queued (cancellation); Completed allows all (archive/restore); BySystem no selection
- [ ] #7 Archive/restore on Completed: rows hidden not deleted, archived visually distinct, filtered counts honest, retention-caused empty states labeled
- [ ] #8 System Detail CVE: unified triage modal (extracted from fleet view), environment-scoped, with Triage column and row action; no separate Justify/Create buttons
- [ ] #9 Triage modal: outstanding/accepted/scheduled per environment; accepted needs justification; scheduled creates/reuses POA&M with owner/due/plan/milestones; typed assignee
- [ ] #10 Authority: exact current evidence supports triage, legacy not promoted, missing/no-scan explicit, accepted/scheduled do NOT false-claim remediation/verification
- [ ] #11 Browser assertions: Active/Completed switching, wait state rendering, failure actionability, archive/restore selection, log content, triage disposition, POA&M reuse, responsive/narrow/dark behavior
- [ ] #12 Playwright: Active/Completed filtering, wait/failed transitions, log drawer, triage modal outstanding→accepted→scheduled, POA&M reuse, responsive tests; desktop/narrow/light/dark screenshots
<!-- AC:END -->
