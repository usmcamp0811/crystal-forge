---
id: TASK-297
title: Rebuild Flakes View to Match JSX Design Mockup Exactly
status: Done
assignee: []
created_date: '2026-05-13 02:56'
updated_date: '2026-05-26 03:34'
labels:
  - ui
  - web-ui
  - flakes
  - design-system
  - mockup-alignment
milestone: Flakes UX Rebuild
dependencies: []
references:
  - 'https://example.com/flakes-view-jsx'
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
  - packages/web-ui/src/views/flakes_list.rs
priority: high
ordinal: 37000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Overview

The current Dioxus implementation of the flakes view must be completely rebuilt to achieve pixel-perfect alignment with the design mockup at `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/FlakesView.jsx`.

This is a complete UI/UX overhaul requiring table/cards toggle view, side-tray commit explorer with timeline, file diff viewer, and pipeline status visualization.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->

- [ ] #1 Main view has table/cards toggle matching design exactly
- [ ] #2 Filter bar with search, view mode segmented control, and count display
- [ ] #3 Side tray slides in from right with backdrop on flake selection
- [ ] #4 Tray header shows flake name, branch chip, sync status, URL, and action buttons
- [ ] #5 Commit timeline (left pane) has search input with result count
- [ ] #6 Commits grouped by time buckets: Today, This week, Earlier
- [ ] #7 Timeline rail with dots and connecting stems matches design
- [ ] #8 Each commit shows SHA (purple when selected), message, timestamp, author, pipeline dots
- [ ] #9 Active commit highlighted with purple accent
- [ ] #10 Commit detail (right pane) shows SHA, message, author, timestamp, +/- stats
- [ ] #11 Pipeline strip shows Eval pill → Build pill → Rollout pill with status colors
- [ ] #12 Files changed section displays grid of file cards
- [ ] #13 File cards show filename, path, +add (green), -del (red), visual proportion bar
- [ ] #14 Clicking file card opens DiffModal with full diff display
- [ ] #15 DiffModal shows line-by-line diff with syntax highlighting and line numbers
- [ ] #16 All fl-\* CSS classes used correctly matching styles.css
- [ ] #17 Table view (when selected) shows flakes in table format
- [ ] #18 Cards view (when selected) shows flakes in card grid
- [ ] #19 ESC key closes tray and modals
- [ ] #20 Click backdrop closes tray
- [ ] #21 All typography (sizes, weights, colors) matches design exactly
- [ ] #22 All spacing and gaps match design exactly
- [ ] #23 All hover states, focus states, active states match design
- [ ] #24 Responsive behavior matches design patterns
- [ ] #25 Empty states for no flakes, no commits, no file changes
- [ ] #26 Loading states while fetching data
- [ ] #27 Visual appearance is pixel-perfect match to FlakesView.jsx
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Merged in MR !255 and task worktree cleanup initiated.
<!-- SECTION:NOTES:END -->
