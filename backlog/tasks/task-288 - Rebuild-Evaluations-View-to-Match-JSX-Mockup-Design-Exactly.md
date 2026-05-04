---
id: TASK-288
title: Rebuild Evaluations View to Match JSX Mockup Design Exactly
status: Backlog
assignee: []
created_date: '2026-05-04 01:18'
updated_date: '2026-05-04 01:20'
labels:
  - ui
  - web-ui
  - evaluations
  - design-system
  - mockup-alignment
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/crystal-forge/project/components/EvalsView.jsx
  - /home/mcamp/code/crystal-forge/crystal-forge/project/styles.css
  - /home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js
  - /home/mcamp/code/crystal-forge/dev/packages/web-ui/src/views/evaluations.rs
  - >-
    /home/mcamp/code/crystal-forge/dev/packages/web-ui/src/components/eval_log_modal.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Overview

The current Dioxus implementation of the evaluations view (`packages/web-ui/src/views/evaluations.rs`) differs significantly from the design mockup (`/home/mcamp/code/crystal-forge/crystal-forge/project/components/EvalsView.jsx`). This task requires a complete rebuild to achieve pixel-perfect alignment with the mockup.

## Current State

- Two-column split layout with cards
- Card-based active queue with inline selection
- Inline log panel with maximize option
- Tailwind utility classes throughout
- Missing stat strip, icons, and several UI elements

## Target State (from Mockup)

- Single-column layout
- Table-based active queue (8 columns)
- Modal-only log viewer
- Design system CSS classes from `styles.css`
- Complete stat strip with 5 metrics
- Icon components throughout

## Scope

**Files to modify:**
- `packages/web-ui/src/views/evaluations.rs` (complete rewrite)
- Potentially create new Icon component if not exists

**Reference files:**
- Design mockup: `/home/mcamp/code/crystal-forge/crystal-forge/project/components/EvalsView.jsx`
- CSS design system: `/home/mcamp/code/crystal-forge/crystal-forge/project/styles.css`
- Mock data: `/home/mcamp/code/crystal-forge/crystal-forge/project/data-builds.js`

## Implementation Sections

This is broken into 15 major sections (A-O) with ~150 specific tasks. See acceptance criteria for complete checklist.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A. Page Structure: Single-column layout with 16px gaps (not 24px), remove .cf-builds-split two-column
- [ ] #2 B. Page Header: Use .page-head class, stats subtitle (not descriptive text), add 'Sync flakes' button (ghost), change 'Refresh' to 'Queue eval' button (primary) with plus icon
- [ ] #3 C. Stat Strip: Move to top-level after header, 5-column grid (.stat-strip), create 5 stats (Active #60a5fa, Completed #34d399, Failed #f87171, Total text-secondary, Flakes tracked #a78bfa=5), each with .stat-accent bar
- [ ] #4 D. Tab Bar: Wrap tabs+content in .card with overflow:hidden, use .sd-tabs class with padding 0 16px, add badge to Active Queue tab showing count, change 'Eval History' to 'History'
- [ ] #5 E. Active Queue Table: Replace cards with <table class='sys-table'>, 8 columns: # | Flake·commit | Branch | Status | Systems | Policy | Started | Actions
- [ ] #6 E1. Queue position column: Show ev.queuePos (width:40, muted, fontSize:12)
- [ ] #7 E2. Flake·commit column: Stack flake name (bold 13px) and commit (mono 11px muted)
- [ ] #8 E3. Branch column: Display as chip (.chip.chip-unknown)
- [ ] #9 E4. Status column: Chip with colored dot (.chip-dot with background color) and translated label
- [ ] #10 E5. Systems column: Show '{count} hosts' (fontSize:12, text-secondary)
- [ ] #11 E6. Policy column: Flex container with green chip (pass ✓) and red chip (fail ✗ if >0)
- [ ] #12 E7. Started column: Relative time (fontSize:12, muted)
- [ ] #13 E8. Actions column: .row-actions div with ↑↓ btn-icon, terminal icon btn-icon, Cancel/Force Cancel btn
- [ ] #14 E9. Remove all card-based queue, Selected Commit card, Completed Evaluations card, inline Evaluation Logs card
- [ ] #15 F. Empty State: Use .empty class with margin:24, h3 'No active evaluations', div 'All flake evaluations are complete.'
- [ ] #16 G. History Filter Bar: Padding 12px 16px with border-bottom, .seg segmented control for status (all/complete/failed/cancelled), select.filter-select for flakes, .filter-count showing entry count
- [ ] #17 H. History Table: Reorder to 8 columns: Flake·commit | Branch | Status | Systems | Policy | Duration | Completed | Actions
- [ ] #18 H1. Combine commit+flake into one column (stacked: bold flake, mono muted commit)
- [ ] #19 H2. Branch as chip (.chip.chip-unknown)
- [ ] #20 H3. Status with colored dot
- [ ] #21 H4. Add Policy column with pass/fail chips (fontSize:10)
- [ ] #22 H5. Actions: .row-actions with terminal icon (Logs) and sync icon (Re-evaluate for non-complete)
- [ ] #23 I. Log Modal: Remove inline log card, create modal-only on 'View logs' click, use .modal-backdrop + .modal (width:min(800px,98vw))
- [ ] #24 I1. Modal header: Status chip + flake in h2, commit·branch·systems in p, .seg for Concise/Verbose, close btn-icon
- [ ] #25 I2. Log stream: .sd-log-stream (minHeight:360, maxHeight:520) with .sd-log-line containing .sd-log-t, .sd-log-lvl, .sd-log-m
- [ ] #26 I3. Blinking cursor (.sd-log-caret) for in_progress status
- [ ] #27 I4. Modal footer: line count (muted, marginRight:auto), Download btn-ghost xs, Close btn-primary
- [ ] #28 J. Icons: Implement or import Icon component, use sync/plus/terminal/x/download icons (keep ↑↓ as text)
- [ ] #29 K. CSS Migration: Replace ALL Tailwind classes with design system classes from styles.css (.btn, .chip, .card, .sys-table, .modal, .sd-*, .stat-*, etc)
- [ ] #30 L. Data Models: Add queuePos, policyPass, policyFail, startedAt, canCancel, canForceCancel, meta:{label,color,cls} to data structures
- [ ] #31 M. Behavior: Implement onMove(id,direction), onLog(ev) for modal, cancelEval(id,force), remove drag-drop, remove click-to-select
- [ ] #32 N. Typography: Exact font sizes (13px main, 11px secondary, 12px timestamps), fontWeight:600 for flakes, .mono for commits
- [ ] #33 O. Pagination: Verify existing pagination uses design system classes (appears correct)
- [ ] #34 Visual verification: Side-by-side comparison with mockup shows identical layout, spacing, colors, and typography
- [ ] #35 All 15 structural differences resolved (see task description)
- [ ] #36 ~150 detailed implementation tasks completed (see full checklist in references)
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## IMPLEMENTATION APPROACH

This is a **complete rewrite**, not an incremental update. The existing card-based implementation must be replaced with a table-based design.

### Phase 1: Structure (Priority 1)
1. Replace two-column layout with single-column
2. Build stat strip at page top (5 metrics)
3. Restructure page header with correct buttons
4. Wrap tabs in card container

### Phase 2: Active Queue Table (Priority 1)
1. Delete all card-based queue components
2. Build HTML table with 8 columns
3. Implement each column as specified (see detailed checklist in notes)
4. Wire up move/cancel/log actions

### Phase 3: History Table (Priority 2)
1. Reorder and combine columns
2. Add Policy column
3. Replace action buttons with icons
4. Update filter bar to use segmented control + select

### Phase 4: Log Modal (Priority 2)
1. Remove inline log card
2. Build modal-only log viewer
3. Implement structured log line display
4. Add verbosity filtering and blinking cursor

### Phase 5: CSS & Icons (Priority 3)
1. Create or import Icon component
2. Replace ALL Tailwind classes with design system classes
3. Apply exact typography and spacing

### Phase 6: Data & Behavior (Priority 3)
1. Update data models with new fields
2. Implement move/cancel/log functions
3. Remove drag-drop and selection behavior

### Phase 7: Testing (Priority 4)
1. Visual comparison with mockup
2. Interaction testing
3. Responsive testing
4. Real-time log streaming verification
<!-- SECTION:PLAN:END -->
