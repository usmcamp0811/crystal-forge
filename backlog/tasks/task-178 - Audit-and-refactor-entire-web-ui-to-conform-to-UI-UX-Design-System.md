---
id: TASK-178
title: Audit and refactor entire web-ui to conform to UI/UX Design System
status: Backlog
assignee: []
created_date: '2026-03-11 01:31'
updated_date: '2026-03-11 01:32'
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
- Light theme compatibility issues requiring `!important` overrides in app.css
- Inconsistent card density and layout patterns across views

These inconsistencies cause:
- Poor light theme rendering
- Maintenance burden when changing design tokens
- Inconsistent user experience across views
- Accessibility gaps (missing focus indicators)

## Goal

Every view and component in the web-ui adheres to the standards defined in `docs/ui-ux-design-system.md`, creating a consistent, accessible, and maintainable UI that works correctly in both dark and light themes.

## Non-Goals

- Adding new features or functionality
- Changing the visual design language (colors, brand, etc.)
- Restructuring the component architecture beyond what's needed for compliance
- Adding new views or components
- Implementing accessibility features beyond focus states (full ARIA audit is separate work)
- Performance optimization (unless directly related to fixing violations)

## Scope

All files in `packages/web-ui/src/`:
- `views/` - All page-level components (~15 views)
- `components/` - All reusable components (~40+ components across subdirectories)
- `theme.rs` - Verify all tokens are correctly defined and used
- `assets/app.css` - Remove/reduce temporary light theme compatibility overrides

### Files to Audit (estimated count)

| Directory | Est. Files | Focus |
|-----------|------------|-------|
| `views/` | ~15 | Layout hierarchy, spacing, color tokens |
| `components/layout/` | ~5 | Container patterns, card structure |
| `components/forms/` | ~5 | Input styling, validation, focus states |
| `components/status/` | ~8 | Badge colors, semantic tokens |
| `components/dashboard/` | ~10 | Widget density, grid patterns |
| `components/builds/` | ~8 | Split-pane layout, status chips |
| `components/tables/` | ~3 | Table patterns, headers |
| Other components | ~10 | Various patterns |

## Architectural Constraints

- Use existing semantic tokens from `theme.rs` - do not create new tokens unless absolutely necessary
- Follow the three-layer styling hierarchy: Tailwind base → CSS variables (app.css) → Rust constants (theme.rs)
- Maintain backwards compatibility - UI behavior must remain unchanged
- All changes must render correctly in both dark and light themes

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Visual regressions | Medium | High | Screenshot comparison before/after each view |
| Breaking light theme | Medium | Medium | Test both themes after each component change |
| Scope creep into redesign | Medium | Medium | Strict adherence to non-goals; create new tasks for improvements |
| Missing edge cases | Low | Medium | Test all views with real/mock data |

## Impact Areas

- All web-ui views and components
- User experience (consistency, accessibility)
- Developer experience (maintainable, predictable patterns)
- Theme switching functionality
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Phase 1: Audit (Document All Violations)

1. **Create audit checklist** based on design system anti-patterns:
   - Hardcoded Tailwind colors (`bg-gray-*`, `text-white`, `border-gray-*`)
   - Non-standard spacing (`p-5`, `p-7`, `mb-5`, `gap-5`, etc.)
   - Missing `cf-focus-ring` on interactive elements
   - Container hierarchy violations
   - Inline styles for static values

2. **Audit each view** - document violations per file:
   - `dashboard.rs`
   - `systems_list.rs`, `system_detail.rs`
   - `flakes_list.rs`
   - `environments_list.rs`
   - `builds.rs`
   - `evaluations.rs`
   - `cves.rs`
   - `deployment_policies.rs`
   - `admin.rs`
   - `login.rs`, `dev_login.rs`
   - `style_guide.rs`

3. **Audit shared components** - document violations:
   - `components/layout/` (AppShell, Card, Sidebar, TopBar)
   - `components/forms/`
   - `components/status/` (badges, indicators)
   - `components/modals/`
   - `components/dashboard/` (widgets)
   - `components/builds/`
   - `components/tables/`

4. **Compile audit report** in implementation notes

### Phase 2: Refactor (Fix Violations Systematically)

Work in order of dependency (shared components first, then views):

1. **Fix `theme.rs`** - ensure all needed tokens exist
2. **Fix `components/layout/`** - Card, container patterns
3. **Fix `components/forms/`** - inputs, buttons, focus states
4. **Fix `components/status/`** - badges, use semantic colors
5. **Fix remaining shared components**
6. **Fix each view** - apply corrected components, fix view-specific issues
7. **Clean up `app.css`** - remove unnecessary `!important` overrides

### Phase 3: Verification

1. **Visual review of each view** in dark theme
2. **Visual review of each view** in light theme  
3. **Test interactive states** - hover, focus, active
4. **Test responsive breakpoints** - mobile, tablet, desktop
5. **Update style guide** if new patterns were introduced
6. **Screenshot key views** for documentation
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Verification Plan

### Tier 0: Fast Local Confidence

```bash
# Format and lint check
nix develop -c cargo fmt --package web-ui -- --check
nix develop -c cargo clippy --package web-ui -- -D warnings

# Build check (ensures no compile errors)
nix develop -c cargo build --package web-ui --target wasm32-unknown-unknown
```

### Tier 1: Visual Verification (Required)

1. Start the development server:
   ```bash
   nix develop
   full-stack up  # or just web-ui if available
   ```

2. Manual visual review checklist:
   - [ ] Dashboard - dark theme
   - [ ] Dashboard - light theme
   - [ ] Systems list - both themes
   - [ ] System detail - both themes
   - [ ] Builds view - both themes
   - [ ] All modals open correctly
   - [ ] Focus rings visible on tab navigation
   - [ ] No layout breaks at lg/md breakpoints

3. Style guide verification:
   - [ ] `/style-guide` renders all tokens correctly
   - [ ] Both themes display properly

### Tier 2: Nix Integration (If Applicable)

Only required if `theme.rs` or `app.css` changes affect the build:
```bash
nix flake check
```

## Audit Findings Template

Use this format when documenting violations:

```markdown
### [filename.rs]

| Line | Violation | Fix |
|------|-----------|-----|
| 42 | `bg-gray-900` | `cf-card-bg` |
| 58 | `p-5` | `p-6` |
| 103 | missing focus-ring | add `cf-focus-ring` |
```
<!-- SECTION:NOTES:END -->
