---
id: TASK-268
title: 'Refine System History view: functional filters, pagination, sticky header'
status: Backlog
assignee: []
created_date: '2026-04-14 01:16'
updated_date: '2026-04-14 01:16'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The System Details → History tab was implemented in TASK-267 but needs UX refinements:

1. **Filter buttons are non-functional** — The status filter buttons (Current, Deployed, Pending, Not Ready, Skipped) at the top of the History view do not actually filter the timeline. They appear as static tabs/buttons but clicking them does nothing.

2. **Aesthetic mismatch** — The filter bar and overall view styling doesn't match the current site design system standards (chips, badges, spacing, typography).

3. **No pagination** — For systems with long history (years of deployments), the entire timeline renders as one long scrollable list. This causes:
   - Severe performance degradation with many entries
   - Poor user experience — scrolling forever to find historical entries
   - Memory/render issues in the browser

4. **Sticky header needed** — The filter bar and view tabs (Overview, History, Logs) should remain visible while scrolling through a long timeline, so users don't lose context.

## Goal

Refine the System History view so it matches site design standards, has working filters, proper pagination/infinite-scroll, and sticky navigation.

## Non-Goals

- No changes to the underlying data model or backend queries (unless needed for pagination).
- No redesign of the timeline entry content itself (already addressed in TASK-267).
- No changes to the Overview or Logs tabs beyond sticky header behavior.

## Scope

1. **Functional filters** — Make the status filter buttons actually filter the displayed timeline entries:
   - Current: shows the current/active state
   - Deployed: shows successful deployment states
   - Pending: shows pending/queued states
   - Not Ready: shows failed or unavailable states
   - Skipped: shows skipped/filtered states

2. **Pagination or virtual scrolling** — Implement a reasonable page size (e.g., 20-50 entries) and load more behavior:
   - Initial load shows recent entries
   - "Load more" button or infinite scroll for older entries
   - Or use virtual scrolling for performance

3. **Sticky header** — Keep the filter bar and tab navigation visible while scrolling the timeline:
   - The filter row (Current/Deployed/Pending/etc.) should stick to top
   - The tabs (Overview | History | Logs) should stick
   - Only the timeline content scrolls

4. **Design consistency** — Style the filter buttons to match site standards:
   - Use existing chip/badge patterns
   - Consistent spacing, colors, hover states
   - Active filter state should be visually distinct

## Architectural Constraints

- Follow existing web-ui component patterns and design system.
- Keep pagination logic in UI layer; backend may need offset/limit support if not already present.
- Preserve existing timeline entry rendering — only change container/layout behavior.

## Verification Plan

- Functional: Click each filter button and verify only matching entries display.
- Pagination: Verify timeline loads in pages; older entries load on scroll or "load more" click.
- Sticky: Scroll down and verify filter bar and tabs remain visible.
- Design: Visual inspection against site design standards.

## Impact Areas

- `packages/web-ui/src/views/system_detail.rs` (or related History component)
- `packages/web-ui/src/components/system/history*` (if separate component)
- API query layer if pagination requires backend changes
<!-- SECTION:DESCRIPTION:END -->
