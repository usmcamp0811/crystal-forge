---
id: TASK-278
title: Implement Claude Design System UI/UX for Systems View
status: To Do
assignee: []
created_date: '2026-04-19 17:54'
labels:
  - ui-ux
  - frontend
  - design-system
  - systems-view
  - refactor
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Refactor the Systems view UI/UX to match the design example from `/home/mcamp/code/crystal-forge/design-example-systems`. This design represents a polished, production-ready interface with improved visual hierarchy, better spacing, and refined component styling.

## Context

A complete design example has been created that demonstrates the intended look and feel for Crystal Forge's Systems view. The design includes:
- Modern card-based and table-based layouts
- Refined stat strip with visual indicators
- Enhanced filter bar with segmented controls
- Improved typography and spacing
- Consistent color tokens and theming
- Heartbeat visualizations and status indicators

## Scope

**Primary Focus**: Systems view (`/systems` route)

**Permitted Scope Extensions**:
- Sidebar component (shared layout element)
- Topbar component (shared layout element)  
- Shared card, chip, badge, and layout components
- Theme tokens and CSS variables

**Explicitly Out of Scope** (will be handled in separate tasks):
- Full refactoring of Dashboard view
- Full refactoring of Builds view
- Full refactoring of Flakes view
- Full refactoring of Environments view
- Full refactoring of CVEs view
- Full refactoring of Policies view
- Other views not directly impacted

**Minimal Touch Rule**: If changes to shared components cause minor visual impacts on other views, those adjustments are acceptable. The goal is to avoid leaving the application in a broken state, not to fully redesign all views.

## Design Reference Location

Source files: `/home/mcamp/code/crystal-forge/design-example-systems/`
- `Systems View.html` - Main HTML entry point
- `app.jsx` - React app structure showing Systems view
- `components/Shell.jsx` - Sidebar and Topbar
- `components/Systems.jsx` - Systems cards, table, filters
- `components/SystemDetail.jsx` - Detail view (for reference)
- `styles.css` - Complete design system tokens and component styles
- `data.js` - Mock data structure (for understanding data shape)

## Key Visual Changes

1. **Sidebar**:
   - Updated branding section with CF logo mark (gradient purple box with "CF" text)
   - Keep existing CF logo image if present
   - Three-section navigation: Fleet, Pipeline, System
   - Rail mode support (collapsible to icon-only)
   - User profile section at bottom

2. **Stat Strip**:
   - Five metrics displayed horizontally: Total, Healthy, Warning/Drift, Critical/Offline, CVEs
   - Each stat card has a colored accent rail on the left
   - Total systems card includes a spark bar showing distribution across environments
   - Larger, more prominent numbers
   - Subtle background colors and borders

3. **Filter Bar**:
   - Search input with icon
   - Environment, status, and flake dropdown filters
   - Segmented control for Cards/Table view toggle
   - Count indicator showing filtered results

4. **Cards View**:
   - Refined card styling with better shadows and borders
   - Colored status rail indicator on left side (visible on hover)
   - Two-column layout for system metadata
   - Environment badge with custom colors per environment
   - CVE chips with semantic colors
   - Heartbeat status in compact form

5. **Table View**:
   - Clean header styling with uppercase labels
   - Hover states for rows
   - Selected row highlighting
   - Row actions (icons) revealed on hover
   - Consistent column widths

6. **Typography & Spacing**:
   - Use defined font families: `--font-sans` and `--font-mono`
   - Consistent letter-spacing on labels
   - Refined line heights
   - Better vertical rhythm

7. **Color Tokens**:
   - Move to CSS custom properties for all colors
   - Support both light and dark themes
   - Semantic color naming (e.g., `--cf-brand-purple`, `--cf-emerald`, `--cf-card-bg`)

## Technical Implementation Notes

- Preserve existing Dioxus component architecture
- Update or create reusable components: Card, Chip, Badge, StatusDot, EnvBadge
- Migrate inline styles to CSS classes where appropriate
- Ensure responsive behavior is maintained
- Keep data fetching and state management logic unchanged
- Logo: The design example doesn't include the CF logo image; preserve any existing logo image usage in the implementation

## Non-Goals

- Backend changes
- API changes
- Data model changes
- Complete redesign of non-Systems views
- New features or functionality beyond visual refinement
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems view matches the visual design from design-example-systems (cards view)
- [ ] #2 Systems view table layout matches the design example
- [ ] #3 Stat strip displays 5 metrics with colored accent rails and spark bar for Total
- [ ] #4 Filter bar includes search, dropdowns, and Cards/Table segmented control
- [ ] #5 Sidebar matches design with three navigation sections (Fleet, Pipeline, System)
- [ ] #6 Sidebar supports rail mode (icon-only collapsed state)
- [ ] #7 Environment badges use per-environment colors as defined in design
- [ ] #8 Status indicators (chips, dots) use semantic color coding
- [ ] #9 Cards and table rows show appropriate hover and selected states
- [ ] #10 Typography uses defined font families and sizing from design system
- [ ] #11 Color tokens are defined as CSS custom properties
- [ ] #12 Both light and dark themes are supported
- [ ] #13 Existing CF logo image is preserved if present
- [ ] #14 Component is responsive and doesn't break at common viewport sizes
- [ ] #15 All existing functionality (filtering, sorting, navigation) continues to work
- [ ] #16 No regressions in other views (may have minor cosmetic adjustments)
<!-- AC:END -->
