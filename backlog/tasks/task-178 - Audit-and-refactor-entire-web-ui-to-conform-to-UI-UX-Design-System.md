---
id: TASK-178
title: Audit and refactor entire web-ui to conform to UI/UX Design System
status: Backlog
assignee: []
created_date: '2026-03-11 01:31'
labels:
  - ui
  - refactor
  - design-system
  - tech-debt
dependencies: []
references:
  - docs/ui-ux-design-system.md
  - docs/web-ui-coding-standards.md
  - packages/web-ui/src/theme.rs
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/views/style_guide.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Comprehensive audit of all views and components in `packages/web-ui` to ensure full compliance with the newly established UI/UX Design System (`docs/ui-ux-design-system.md`).

## Problem

The current web-ui has accumulated inconsistencies over time:
- Hardcoded Tailwind color classes (e.g., `bg-gray-900`, `text-white`) instead of semantic tokens
- Inconsistent spacing values (non-standard values like `p-5`, `mb-7`)
- Container hierarchy violations (cards without proper grid/section wrappers)
- Missing focus states on interactive elements
- Light theme compatibility issues requiring `!important` overrides
- Inconsistent card density and layout patterns

## Goal

Every view and component in the web-ui adheres to the standards defined in `docs/ui-ux-design-system.md`, creating a consistent, accessible, and maintainable UI.

## Scope

All files in `packages/web-ui/src/`:
- `views/` - All page-level components
- `components/` - All reusable components
- `theme.rs` - Verify all tokens are used correctly
- `assets/app.css` - Remove temporary compatibility overrides where possible

## Approach

1. **Audit Phase**: Document all violations per file
2. **Refactor Phase**: Fix violations systematically by view/component
3. **Verification Phase**: Visual review in both dark and light themes
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All hardcoded Tailwind color classes replaced with semantic tokens (cf-* classes or theme.rs constants)
- [ ] #2 All spacing uses approved scale (gap-1/2/3/4/6, p-4/6/8, etc.) - no non-standard values
- [ ] #3 Container hierarchy followed: Page > Section/Grid > Card > Content
- [ ] #4 All interactive elements have cf-focus-ring class for visible focus states
- [ ] #5 Light theme compatibility overrides in app.css reduced or eliminated
- [ ] #6 All buttons follow hierarchy: one primary per area, danger requires confirmation
- [ ] #7 All forms validate on blur with proper error styling
- [ ] #8 Loading states use skeleton loaders matching content structure
- [ ] #9 Style guide view (/style-guide) updated with any new patterns introduced
- [ ] #10 Visual review completed in both dark and light themes with no broken layouts
<!-- AC:END -->
