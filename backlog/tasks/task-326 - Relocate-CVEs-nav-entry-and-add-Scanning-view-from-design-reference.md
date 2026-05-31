---
id: TASK-326
title: Relocate CVEs nav entry and add Scanning view from design reference
status: Backlog
assignee: []
created_date: '2026-05-31 02:20'
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
1) Update sidebar/navigation so CVEs appears in its new section.
2) Add a new Scanning view at the previous CVEs sidebar location.
3) Implement Scanning view UI to match `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/ScanningView.jsx`.

## Non-Goals
- No backend scanning engine redesign.
- No unrelated refactors in other views.
- No broad styling/token refactors outside what is needed for parity.

## Scope
- Navigation updates (route/label/order/section placement).
- New Scanning view component/page in web-ui.
- Wiring route + sidebar item to new view.
- Data can use existing API endpoints/models where available; if missing, implement minimal integration path or explicit placeholder states consistent with current app patterns.

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
<!-- AC:END -->
