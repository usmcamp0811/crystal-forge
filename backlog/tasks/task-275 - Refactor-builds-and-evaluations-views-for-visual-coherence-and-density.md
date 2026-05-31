---
id: TASK-275
title: Refactor builds and evaluations views for visual coherence and density
status: Backlog
assignee: []
created_date: '2026-04-18 01:56'
labels:
  - ui
  - ux
  - refactor
  - builds
  - evaluations
  - dioxus
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The builds and evaluations views currently have divergent layouts and interaction patterns despite representing similar workflow processes (queue → active → completed). This task unifies their visual design and information architecture to create a coherent, dense, and refined UI.

## Current State

**Builds View** (`packages/web-ui/src/views/builds.rs`):
- Tabs: Active Queue / Completed Builds
- Active queue: Card-based display with extensive filters
- Split view: queue list (left) / detail pane (right)
- Completed: Full-width table with status filters and sorting
- Worker strip metrics at top

**Evaluations View** (`packages/web-ui/src/views/evaluations.rs`):
- No tabs - everything on one page
- Full-width logs section at top (collapsible)
- Split view: active queue cards with drag-drop (left) / metrics + selected commit + completed list (right)
- Active queue: card-based with Up/Down buttons

## Design Direction

Both views should adopt a consistent pattern:

### Layout Structure
1. **Page header** with metrics strip
2. **Tabs**: "Active Queue" | "History" (or "Completed")
3. **Active Queue tab**:
   - Left half: Table view of queue items (sortable, filterable, searchable)
   - Right half: Detail pane for selected item
   - Auto-select first row on load
4. **History/Completed tab**:
   - Full-width table with filters, sorting, search
   - Click row to view details (could open detail pane or modal)

### Builds-Specific Considerations
- Keep worker status/control strip
- Preserve existing queue actions (prioritize, stop, restart, etc.)
- Maintain builder status controls

### Evaluations-Specific Considerations  
- Preserve drag-and-drop reordering in active queue table (or provide Up/Down in row actions)
- Keep live log streaming panel (position TBD - possibly as detail pane tab or dedicated section)
- Maintain websocket connection for real-time updates
- System-level status chips in detail pane

### Table Features (Both Views)
- Minimal columns: status, system/config, commit (short), time/duration
- Sortable by column click
- Search/filter controls above table
- Row selection highlights and updates detail pane
- Pagination controls (builds already has this)

## Development Workflow

This task will be worked **interactively** using Dioxus hot-reload:

```bash
AUTH_MODE=dev nix run .#web-ui.dx-serve
```

The implementing agent will:
1. Make incremental UI changes
2. Commit at logical checkpoints (e.g., "builds: convert active queue to table", "evals: add tab navigation")
3. Iterate based on user feedback during the session
4. Use existing mock data infrastructure

## Scope

**In scope:**
- Restructure both views for visual consistency
- Convert card-based queues to table views
- Implement/refine table features (sort, filter, search)
- Unify split-pane detail view pattern
- Maintain all existing functionality (actions, websockets, metrics)
- Improve information density and visual hierarchy

**Out of scope:**
- New API endpoints (unless minor additions needed)
- Changes to backend logic or data models
- Performance optimization beyond UI rendering
- New features not related to layout/density

## Success Criteria

- Both views use consistent tab-based navigation (Active / History)
- Both active queues display as tables with similar column structures
- Both detail panes show relevant information in similar layout patterns
- All existing actions and functionality remain working
- Visual density is noticeably improved (less whitespace, more scannable)
- User can quickly identify status, system, and key metadata at a glance
- Hot-reload workflow allows rapid iteration and feedback
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Both builds and evaluations views have consistent tab navigation (Active Queue / History or Completed)
- [ ] #2 Active queue displays as a table (not cards) with columns for status, system/config, commit, and time
- [ ] #3 Table rows are selectable and highlight when clicked
- [ ] #4 Selecting a queue item auto-populates the detail pane on the right
- [ ] #5 First row auto-selects on page load when queue is not empty
- [ ] #6 Detail pane shows relevant information (logs, metadata, actions) appropriate to the view
- [ ] #7 History/Completed tab shows full-width table with sorting and filtering
- [ ] #8 All existing functionality is preserved (worker controls, queue actions, drag-reorder for evals, websocket logs)
- [ ] #9 Visual design is coherent between the two views (similar spacing, typography, color usage)
- [ ] #10 Information density is improved - key data is visible without excessive scrolling or clicking
- [ ] #11 Search and filter controls work and are consistently positioned
- [ ] #12 Running AUTH_MODE=dev nix run .#web-ui.dx-serve provides hot-reload during development
<!-- AC:END -->
