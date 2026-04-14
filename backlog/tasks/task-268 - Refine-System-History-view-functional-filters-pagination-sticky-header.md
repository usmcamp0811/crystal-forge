---
id: TASK-268
title: 'Refine System History view: functional filters, pagination, sticky header'
status: To Do
assignee: []
created_date: '2026-04-14 01:16'
updated_date: '2026-04-14 01:38'
labels:
  - ui
  - systems
  - ux
  - sprint-ready
milestone: System Details Hardening
dependencies: []
priority: high
ordinal: 2680
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

## Non-Goals

- No changes to the underlying data model or backend queries (unless needed for pagination).
- No redesign of the timeline entry content itself (already addressed in TASK-267).
- No changes to the Overview or Logs tabs beyond sticky header behavior.

## Scope

1. **Define better filter groupings** based on actual history statuses in data model
2. **Functional filters** — Make filters actually work and show correct subset
3. **Pagination** — Load timeline in pages (20-50 entries), with "Load More" or infinite scroll
4. **Sticky header** — Keep filter bar and tabs visible while scrolling
5. **Design consistency** — Style to match site design system

## Architectural Constraints

- Follow existing web-ui component patterns and design system.
- Keep pagination/filter logic in UI layer unless backend support is required.
- Preserve existing timeline entry rendering; this task focuses on filtering/layout and navigation persistence.

## Verification Plan

- Functional: Click each filter button and verify only matching entries display.
- Pagination: Verify timeline loads in pages; older entries load on scroll or "load more" click.
- Sticky: Scroll down and verify filter bar and tabs remain visible.
- Design: Visual inspection against site design standards.
- Automated: extend web-ui integration checks to assert filters + sticky navigation behavior.

## Impact Areas

- `packages/web-ui/src/views/system_detail.rs` (or related History component)
- `packages/web-ui/src/components/system/history*` (if separate component)
- API query layer only if pagination requires backend changes.

## Risk Level

Medium (user-facing UX + possible data-query pagination changes).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Selecting a history filter updates the displayed timeline entries to the matching subset
- [ ] #2 #2 Active filter has clear visual selected state aligned with site standards
- [ ] #3 #3 History timeline renders with pagination or incremental loading (no unbounded full-list render)
- [ ] #4 #4 Overview | History | Logs navigation remains visible while scrolling history content
- [ ] #5 #5 Filter row remains visible while scrolling history content
- [ ] #6 #6 Revert events are visually identifiable in timeline entries
- [ ] #7 #7 web-ui verification includes checks for filter functionality and sticky navigation
<!-- AC:END -->
