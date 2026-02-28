---
id: TASK-141
title: Consolidate repeated web-ui inline styles into shared CSS classes
status: To Do
assignee: []
created_date: '2026-02-28 18:05'
updated_date: '2026-02-28 18:05'
labels:
  - web-ui
  - frontend
  - css
  - maintainability
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The web UI currently mixes Tailwind/theme classes with many repeated inline `style` fragments (especially repeated color badges, chip styles, and modal/layout snippets). This increases duplication and makes visual updates error-prone.

## Goal
Improve style maintainability by moving repeated **static** inline style declarations into shared CSS classes/tokens, while preserving runtime-calculated inline styles that are required for dynamic rendering.

## Non-Goals
- No visual redesign or theme change
- No broad component refactor unrelated to styling duplication
- No migration of truly dynamic computed styles (pixel-positioned timeline/layout math, computed colors) out of inline style attributes
- No changes to API/domain logic

## Architectural Constraints
- Keep UI rendering behavior identical
- Continue using existing design token approach (`theme.rs`) and shared stylesheet (`assets/app.css`)
- Prefer semantic, reusable class names for extracted styles
- Maintain clear separation of presentation from non-UI logic

## Verification Plan
- Run web-ui formatting/lint checks already used by project
- Build web-ui target in repository dev environment (`nix develop`)
- Manually verify key views where styles are extracted (cards, badges, modals, build/flakes views) for no visual regressions

## Impact Areas
- `packages/web-ui/src/views/*`
- `packages/web-ui/src/components/*`
- `packages/web-ui/assets/app.css`
- Potentially `packages/web-ui/src/theme.rs` for token alignment

## Risk Level
Medium: style extraction can introduce subtle regressions if selectors/classes are misapplied.

## Dependencies
None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Repeated static inline styles used in multiple web-ui components are replaced with shared reusable classes and/or CSS variables.
- [ ] #2 Dynamic inline styles that depend on runtime-calculated values remain inline and are documented in task notes as intentionally retained.
- [ ] #3 All affected web-ui views/components render with no intentional visual changes compared to baseline.
- [ ] #4 No application/domain logic changes are introduced; scope remains styling-maintainability only.
- [ ] #5 Local verification (build/check + targeted manual UI checks) is completed and recorded in task notes.
<!-- AC:END -->
