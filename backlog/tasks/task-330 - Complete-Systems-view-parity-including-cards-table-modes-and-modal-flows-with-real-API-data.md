---
id: TASK-330
title: >-
  Complete Systems view parity including cards-table modes and modal flows with
  real API data
status: To Do
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 02:53'
labels:
  - design-parity
  - systems
  - api-integration
milestone: m-16
dependencies:
  - TASK-328
  - TASK-329
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
modified_files:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/components/system
  - packages/web-ui/src/systems/adapter.rs
  - packages/web-ui/src/api/models.rs
priority: high
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Systems page is partially aligned but still drifts from design behavior/visuals and includes fallback/local-state patterns that can diverge from backend truth.

Goal: Achieve exact Systems view parity with CrystalForgelatest for both cards and table modes, with all displayed values sourced from real backend APIs.

Non-goals: New domain features not present in design examples.

Scope details:
- Align page header, stat strip, filter bar, cards/table geometry, chip styles, and selected-state behaviors.
- Match detail panel and modal visual/interaction behavior (deploy/edit/remove/update key).
- Remove/contain fallback/mock rendering in production path; ensure API-driven values for counts, statuses, and metadata.
- Ensure loading/empty/error states are styled per design and tied to real API outcomes.

Verification plan:
- Web UI screenshot set: systems default, filtered, card mode, table mode, selected/open panel, each modal state.
- Behavior assertions for filter/search counts and backend-driven content.

Impact areas: systems list/detail views, systems components, systems adapters/API models.
Risk: High.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems view card and table layouts are pixel-aligned with reference for core states
- [ ] #2 Filters/search/view toggles reproduce design behavior and result counts
- [ ] #3 All stat-strip and row/card values are sourced from backend APIs in production path
- [ ] #4 All systems modals/panels match reference spacing/typography/interactions
- [ ] #5 web-ui checks include screenshot + behavior assertions for systems parity scenarios
- [ ] #6 web-ui screenshot set covers loading, empty, error, and populated states for Systems surfaces
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

Based on comprehensive code review, the Systems view implementation is already at very high parity with the JSX design reference. All core components exist:

✅ **Components Implemented:**
- HeartbeatSpinner with real-time countdown and overdue states (amber/red)
- EnvBadge with environment-specific colors
- StatusChip, CveChips, DeploymentChip
- CveBar visualization
- SystemCardV2 matching design (status rail, badges, metadata, chips, deploy button)
- SystemsTable (table mode) with all columns
- SystemPanel (side panel) with all sections
- DeployModal with commit selection

✅ **Styles Complete:**
- All design tokens (colors, radii, shadows) from reference styles.css
- .sys-card and .sys-table styles
- .hb-spinner with animations
- .side-panel with backdrop and slide-in animation
- .panel-head, .panel-body, .panel-actions
- .timeline, .cve-bar, .cve-legend, .kv-grid

✅ **Backend Integration:**
- SystemSummary and SystemDetail models with all required fields
- API adapters using load_systems_with_fallback
- No mock data in production path

**Remaining Work:**

1. **Visual Verification** - Run full-stack and compare with JSX reference pixel-by-pixel
2. **Screenshot Coverage** - Ensure web-ui checks capture:
   - Systems cards mode (default, compact, hover, selected)
   - Systems table mode (default, compact, row hover, selected row)
   - Side panel open with all sections
   - Deploy modal open
   - Edit modal open
   - Filter dropdown active
   - Search active with results
   - Empty state
   - Loading state
   - Error state

3. **Minor Refinements** (if visual comparison shows gaps):
   - Adjust spacing/padding to match design exactly
   - Verify chip colors and sizes
   - Check status rail positioning and opacity
   - Validate heartbeat spinner size variants
   - Confirm modal backdrop and z-index layering

4. **Behavior Assertions** - Add to web-ui checks:
   - Filter by environment updates system count
   - Search by hostname filters table/cards
   - View toggle switches between cards/table
   - Card click opens side panel
   - Deploy button opens modal
   - Edit action opens edit modal

5. **Documentation** - Update if needed:
   - Component usage examples
   - API integration notes

**Verification Commands:**
1. `cargo check -p web-ui`
2. `cargo clippy -p web-ui -- -D warnings`  
3. `server-stack up` → visual check at http://localhost:3000/systems
4. `just web-ui-check` → screenshot/assertion validation
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on gray in ~/code/crystal-forge/TASK-330-systems-view-parity
<!-- SECTION:NOTES:END -->
