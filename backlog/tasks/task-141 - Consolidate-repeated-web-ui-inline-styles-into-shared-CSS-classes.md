---
id: TASK-141
title: Consolidate repeated web-ui inline styles into shared CSS classes
status: In Progress
assignee: []
created_date: '2026-02-28 18:05'
updated_date: '2026-02-28 18:53'
labels:
  - web-ui
  - frontend
  - css
  - maintainability
  - documentation
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The web UI currently mixes Tailwind/theme classes with many repeated inline `style` fragments (badges, chips, modal shells, gradients, and status colors). This duplication increases maintenance cost, causes visual-token drift, and makes future theme expansion harder.

## Goal
Perform an **exhaustive** pass across `packages/web-ui` to move repeated **static** inline styles into shared CSS classes/tokens, while preserving runtime-calculated inline styles required for dynamic rendering. The result must make it straightforward to support a dark theme, a light theme, and additional custom themes. Deliver as one large PR.

## Non-Goals
- No visual redesign or intentional look-and-feel change
- No migration of truly dynamic computed styles (pixel-positioned timeline/layout math, runtime color interpolation) out of inline style attributes
- No API/domain/business-logic changes
- No requirement to ship a complete theme-switcher UX in this task (theme-ready foundation is required)

## Architectural Constraints
- Keep rendering behavior and UX equivalent to baseline
- Continue using existing design token approach (`theme.rs`) and shared stylesheet (`assets/app.css`)
- Prefer CSS variables/tokens and semantic reusable classes for extracted static styles
- Organize token layers so dark/light/custom theme maps can be added without component rewrites
- Keep presentation concerns isolated from non-UI logic

## Verification Plan
- Run repository web-ui checks/build in the Nix dev environment
- Manually verify representative screens/components touched by extraction for visual regressions
- Record verification commands and outcomes in task notes

## Impact Areas
- `packages/web-ui/src/views/*`
- `packages/web-ui/src/components/*`
- `packages/web-ui/assets/app.css`
- `packages/web-ui/src/theme.rs` (if token alignment is needed)
- Repository coding standards documentation (update to codify this rule)

## Risk Level
Medium-High: large-scope style extraction can introduce subtle regressions if class mapping is inconsistent.

## Dependencies
None.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An exhaustive inventory of repeated static inline style fragments across `packages/web-ui/src` is completed and addressed in this task.
- [ ] #2 Repeated static inline styles are replaced with shared reusable classes and/or CSS variables, with no intentional visual redesign.
- [ ] #3 A theme-token layer is established for extracted static primitives that supports dark and light themes and enables future custom themes with minimal component-level changes.
- [ ] #4 Dynamic inline styles that depend on runtime-calculated values remain inline and are documented in task notes as intentionally retained.
- [ ] #5 Coding standards documentation is updated to require shared classes/tokens for repeated static styles and to define when inline styles are acceptable.
- [ ] #6 All affected web-ui views/components render equivalently to baseline in local verification.
- [ ] #7 Local verification (build/check + targeted manual UI checks) is completed and recorded in task notes.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-gpt-5.3-codex on gray in /home/mcamp/code/crystal-forge/TASK-141-themeable-web-ui-css-extraction
<!-- SECTION:NOTES:END -->
