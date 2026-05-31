---
id: TASK-329
title: Align global design system tokens and shell layout to CrystalForgelatest
status: To Do
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-05-31 16:07'
labels:
  - design-parity
  - design-system
  - web-ui
milestone: m-16
dependencies:
  - TASK-328
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
modified_files:
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/theme.rs
priority: high
ordinal: 1610
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Global tokens and shell primitives in dev have drifted from CrystalForgelatest, causing cross-view inconsistency and blocking pixel parity.

Goal: Bring shared design primitives (color tokens, typography, spacing, radii, shadows, shell layout, sidebar/topbar frame behavior) into exact parity with CrystalForgelatest standards while preserving theme behavior.

Non-goals: Per-view business logic changes; endpoint additions.

Scope details:
- Normalize CSS variable/token definitions in assets/app.css and theme token mapping.
- Align shell-level components (sidebar, top area, section spacing, base card/input/button primitives).
- Remove token conflicts and duplicate style paths that produce divergent rendering.
- Ensure light/dark themes both match the design source standards.

Verification plan:
- Theme snapshot comparisons for dark/light modes against reference images.
- Component primitive screenshots (buttons, chips, cards, inputs, badges).

Impact areas: packages/web-ui/assets/app.css, src/theme.rs, shared components.
Risk: High (shared primitives affect all views).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All shared color/spacing/typography/radius/shadow tokens match CrystalForgelatest values
- [ ] #2 Dark and light theme base surfaces render to parity for shell and primitive components
- [ ] #3 No duplicate conflicting token definitions remain for shared primitives
- [ ] #4 web-ui check captures shell + primitive screenshot set for both themes
- [ ] #5 web-ui check includes assertions that token-driven styles render expected computed values/classes for core primitives
<!-- AC:END -->
