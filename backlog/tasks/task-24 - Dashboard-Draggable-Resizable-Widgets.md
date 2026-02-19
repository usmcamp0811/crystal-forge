---
id: TASK-24
title: Dashboard Draggable Resizable Widgets
status: Done
assignee: []
created_date: '2026-02-14'
updated_date: '2026-02-19 04:12'
labels:
  - ui
  - dashboard
  - enhancement
dependencies:
  - TASK-23
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add drag-and-drop reordering and resize handles to dashboard widgets. Users should be able to customize their dashboard layout by:

1. **Dragging widgets** to reorder them within the masonry grid
2. **Resizing widgets** by dragging corner/edge handles
3. **Persisting layout** to localStorage (and eventually user preferences in the backend)

This would allow users to prioritize the information most relevant to them and create personalized dashboard views.

Current widgets that would benefit:
- Fleet Health (pie chart)
- CVE Summary (badge grid)
- Deployment Status (pie chart)
- Recent Deployments (list)
- Flake Commit Timeline (wide horizontal)

Technical considerations:
- May need a library like `dnd-kit` or custom drag implementation
- Resize constraints (min/max sizes per widget type)
- Responsive behavior on mobile (disable drag/resize?)
- Animation/transitions during drag operations
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Widgets can be dragged to reorder within the dashboard grid
- [ ] #2 Widgets have visible drag handle (icon or entire header)
- [ ] #3 Widgets can be resized via corner drag handles
- [ ] #4 Each widget type has min/max size constraints
- [ ] #5 Layout persists to localStorage on change
- [ ] #6 Reset to default layout option available
- [ ] #7 Drag/resize disabled on mobile (touch-friendly alternative or simple disable)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
This is a future enhancement identified during dashboard layout work. Current implementation uses CSS columns masonry which provides reasonable auto-layout without user customization.

Refinement (2026-02-19 sprint review): keep In Progress. Remaining scope: implement actual drag/drop interaction model, resize handles with per-widget constraints, and persistence wiring (localStorage) plus reset action. Current state is conceptual/future-enhancement notes only; no completion evidence in this review.

Suggested verification before closing: UI interaction test for drag reorder + resize + persistence reload, and web-ui build check (nix build .#checks.x86_64-linux.web-ui).
<!-- SECTION:NOTES:END -->
