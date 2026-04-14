---
id: TASK-268
title: 'Refine System History view: functional filters, pagination, sticky header'
status: Backlog
assignee: []
created_date: '2026-04-14 01:16'
updated_date: '2026-04-14 01:21'
labels:
  - ui
  - systems
  - ux
  - sprint-ready
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The System Details → History tab was implemented in TASK-267 but needs UX refinements:

1. **Filter buttons are non-functional** — The current filter labels (Current, Deployed, Pending, Not Ready, Skipped) are poorly named and don't actually filter anything. The labels themselves may not even make sense for the data.

2. **Aesthetic mismatch** — The filter bar and overall view styling doesn't match the current site design system standards.

3. **No pagination** — For systems with long history (years of deployments), the entire timeline renders as one long scrollable list, causing performance and usability issues.

4. **Sticky header needed** — The filter bar and view tabs should remain visible while scrolling.

5. **No revert indicator** — When a system reverts to a previous configuration, there's no visual indication in the timeline.

## Goal

Refine the System History view with proper filtering, pagination, sticky headers, and better classification of history entries.

## Scope

1. **Define better filter groupings** — The current labels are unclear. Proposed new grouping:
   - **Current** — The active/current system state (always at top)
   - **History** — All past state transitions (could further classify as: successful deploys, failed deploys, reverts, skipped)
   - Consider marking **reverts** explicitly in the timeline (config returning to a previous hash)

2. **Functional filters** — Make filters actually work and show correct subset

3. **Pagination** — Load timeline in pages (20-50 entries), with "Load More" or infinite scroll

4. **Sticky header** — Keep filter bar and tabs visible while scrolling

5. **Design consistency** — Style to match site design system

## Proposed filter labels (TBD based on actual system_state data)

- **Current** — active state at the top
- **Deployments** — successful/complete deployments
- **Reverts** — times the system went back to a previous config
- **Failed** — failed deployment attempts
- **Skipped** — skipped/filtered entries

Or simpler: Just filter by status values present in the data.

## Non-Goals

- No changes to underlying data model unless needed for classification
- No redesign of timeline entry content

## Impact Areas

- System Detail History component and styling
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Clicking each status filter button (Current, Deployed, Pending, Not Ready, Skipped) filters the timeline to show only matching entries
- [ ] #2 #2 Active filter button has distinct visual state (e.g., different background color, underline)
- [ ] #3 #3 Timeline loads with a reasonable page size (20-50 entries); older entries load via 'Load More' button or infinite scroll
- [ ] #4 #4 Scrolling the timeline keeps the filter bar pinned to the top of the view
- [ ] #5 #5 Scrolling the timeline keeps the tab navigation (Overview | History | Logs) pinned
- [ ] #6 #6 Filter bar styling matches site design system (chips/badges, spacing, colors)
- [ ] #7 #7 Performance is acceptable with 100+ historical entries (no UI freeze or excessive memory)
- [ ] #8 #8 Verification: functional test that each filter shows correct subset of entries
<!-- AC:END -->
