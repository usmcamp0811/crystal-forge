---
id: TASK-326.2
title: Bring Scanning queue and scan-log interactions to updated design parity
status: Backlog
assignee: []
created_date: '2026-09-09 03:32'
updated_date: '2026-09-09 03:33'
labels:
  - scanning
  - web-ui
  - design-parity
  - navigation
  - accessibility
dependencies:
  - TASK-326.1
  - TASK-337
  - TASK-448
references:
  - git commit e1b7434899e23f43770632e59d80a76a8fc8459e
  - TASK-448
documentation:
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/data-scanning.js
  - docs/design/CrystalForge/styles.css
  - docs/design/CrystalForge/app.jsx
modified_files:
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/src/api/client.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/state/navigation_focus.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
parent_task_id: TASK-326
priority: high
type: enhancement
ordinal: 473000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the Scanning surface introduced by design commit `e1b74348` after the authoritative wait-state and log contracts are available. Replace the activity side panel and ambiguous Active & Recent view with focused Deployed, All scans, and By system views. Add real filtering, sorting, exact scan-log details, and actionable navigation while preserving loading, empty, error, authorization, selection, and cancellation behavior. The frontend must consume server data only and must not generate vulnix output or fake progress.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Scanning provides Deployed All scans and By system views with real count badges and no obsolete activity side panel
- [ ] #2 Deployed and All scans support query status revision freshness and latest-per-flake filtering with a visible result count and resettable empty state
- [ ] #3 Sortable columns use deterministic status severity revision and timestamp semantics and communicate sort state accessibly
- [ ] #4 Rows open the exact scan detail tray while modifier-based multi-selection remains limited to cancellable queued or running scans
- [ ] #5 The detail tray shows real status trigger timestamps scanner metadata findings failure context and bounded execution log content for the exact configuration and full revision
- [ ] #6 Log search previous and next match navigation download and live-running presentation use server-provided content and never fabricate lines or percentage progress
- [ ] #7 Waiting and failed states provide truthful Check now Retry scan Build and cache navigation outcomes with exact deep-link context
- [ ] #8 By system expansion preserves per-revision history and opens the exact available scan while unscanned and needs-build rows remain explicit
- [ ] #9 Loading empty partial error stale and authorization states remain usable at desktop and narrow widths in light and dark themes with keyboard focus restoration and Escape behavior
- [ ] #10 The WASM build and authoritative web-ui check pass with assertion coverage and screenshots for filters sorting selection waiting running failed completed log search download deep links and responsive states
<!-- AC:END -->
