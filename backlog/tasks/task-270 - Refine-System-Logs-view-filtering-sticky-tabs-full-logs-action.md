---
id: TASK-270
title: 'Refine System Logs view: filtering, sticky tabs, full logs action'
status: In Progress
assignee: []
created_date: '2026-04-14 01:31'
updated_date: '2026-04-18 00:47'
labels:
  - ui
  - systems
  - ux
  - sprint-ready
milestone: System Details Hardening
dependencies: []
priority: high
ordinal: 2700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The System Detail → Logs tab needs UX improvements similar to History:

1. **Filter options are non-functional** — UI controls do not actually filter logs.
2. **Tabs don't stick** — Overview | History | Logs navigation scrolls away during long log browsing.
3. **"View full logs" does nothing** — The action has no effective behavior.

## Goal

Refine Logs view with working filters, sticky navigation, and a meaningful full-logs action.

## Non-Goals

- No changes to log ingestion/storage pipeline.
- No redesign of individual log entry content.

## Scope

1. Functional logs filtering by event type/severity and text search.
2. Sticky tabs/navigation while scrolling logs.
3. Implement working "View full logs" behavior (open full log view/page or equivalent complete log rendering path).
4. Design consistency with existing site standards.

## Architectural Constraints

- Keep filtering behavior in UI unless backend query-side filtering is required for scale.
- Reuse existing System Detail tab/navigation patterns.
- Keep scope focused on Logs tab behavior and wiring.

## Verification Plan

- Functional: filters narrow logs correctly by type/severity/text.
- Sticky behavior: tabs remain visible while scrolling long logs.
- Action behavior: "View full logs" navigates/displays full log content.
- Automated: web-ui integration checks cover filter + sticky + full logs action.

## Impact Areas

- `packages/web-ui/src/views/system_detail.rs` (Logs tab)
- `packages/web-ui/src/components/system/logs*` (if split component)
- API/query layer only if needed for full-log retrieval or scalable filtering.

## Risk Level

Medium (user-facing UX + potential log retrieval path wiring).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 #1 Logs filters (type/severity/text) actually filter displayed entries
- [ ] #2 #2 Filter controls have clear active visual state consistent with design system
- [ ] #3 #3 Overview | History | Logs navigation remains visible while scrolling logs
- [ ] #4 #4 Clicking "View full logs" opens complete log view/content (not a no-op)
- [ ] #5 #5 web-ui verification includes assertions for filtering, sticky tabs, and full logs action
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: gpt-5.3-codex on reckless in ~/code/crystal-forge/TASK-270-system-logs-refinement
<!-- SECTION:NOTES:END -->
