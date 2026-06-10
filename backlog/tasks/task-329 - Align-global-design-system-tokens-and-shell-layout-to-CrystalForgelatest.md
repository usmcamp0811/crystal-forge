---
id: TASK-329
title: Align global design system tokens and shell layout to CrystalForgelatest
status: Backlog
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 02:57'
labels:
  - design-parity
  - design-system
  - web-ui
milestone: m-18
dependencies:
  - TASK-328
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
modified_files:
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/theme.rs
priority: high
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Global tokens and shell primitives in dev have drifted from CrystalForgelatest, causing cross-view inconsistency and blocking pixel parity.

Goal: Bring shared design primitives (color tokens, typography, spacing, radii, shadows, shell layout, sidebar/topbar frame behavior) into exact parity with CrystalForgelatest standards while preserving theme behavior.

Non-goals: Per-view business logic changes; endpoint additions.

Replan note: reset to Backlog as m-18 foundation work. This task should land before deep per-view parity tasks restart.

Scope details:
- Normalize CSS variable/token definitions in assets/app.css and theme token mapping.
- Align shell-level components (sidebar, top area, section spacing, base card/input/button primitives).
- Remove token conflicts and duplicate style paths that produce divergent rendering.
- Ensure light/dark themes both match the design source standards.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All shared color/spacing/typography/radius/shadow tokens match CrystalForgelatest values
- [ ] #2 Dark and light theme base surfaces render to parity for shell and primitive components
- [ ] #3 No duplicate conflicting token definitions remain for shared primitives
- [ ] #4 web-ui check captures shell + primitive screenshot set for both themes
- [ ] #5 web-ui check includes assertions that token-driven styles render expected computed values/classes for core primitives
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Reset to Backlog for milestone-driven planning. Shared shell/token parity remains a prerequisite for reliable downstream screenshot parity.
<!-- SECTION:NOTES:END -->
