---
id: TASK-278
title: Implement Claude Design System UI/UX for Systems View
status: In Progress
assignee:
  - '@ai-agent'
created_date: '2026-04-19 17:54'
updated_date: '2026-04-20 00:35'
labels:
  - ui
  - systems
  - web-ui
  - design-parity
milestone: UI/UX Design System
dependencies: []
references:
  - /home/mcamp/code/crystal-forge/design-example-systems/Systems View.html
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/243'
documentation:
  - >-
    /home/mcamp/code/crystal-forge/design-example-systems/components/SystemDetail.jsx
  - /home/mcamp/code/crystal-forge/design-example-systems/styles.css
priority: high
ordinal: 3690
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the Claude Design System-inspired UI/UX for the Systems view (`/systems`) in Crystal Forge, using `/home/mcamp/code/crystal-forge/design-example-systems` as the visual and interaction reference. Scope includes systems list/cards/table, filters, topbar/sidebar shell alignment where needed for fidelity, and system detail parity for tabs and interactions directly tied to Systems UX.
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

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Applied follow-up deploy/detail parity fix in commit 64e21322: removed branch dropdown, deduplicated deploy commit list by hash, changed commit list messaging away from configuration identity text, added hover titles for truncated commit fields, reduced policy-callout check icon size, and aligned metric strip to 5-card desktop layout with responsive breakpoints.

Created follow-up backlog task TASK-278.1 to add real generation data in API/DTO so the metric strip can replace #— placeholder.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Code passes `cargo fmt -- --check`
- [ ] #2 Code passes `cargo clippy -- -D warnings`
- [ ] #3 All existing tests pass (`cargo test`)
- [ ] #4 Visual QA: Systems view matches design example in both Cards and Table modes
- [ ] #5 Visual QA: Sidebar appearance matches design in both full and rail modes
- [ ] #6 Visual QA: Stat strip displays correctly with all 5 metrics
- [ ] #7 Visual QA: Filter bar layout and styling matches design
- [ ] #8 Functional test: Theme toggle switches between light and dark correctly
- [ ] #9 Functional test: View toggle switches between Cards and Table
- [ ] #10 Functional test: All filters (search, environment, status, flake) work correctly
- [ ] #11 Functional test: Card and row hover states work as expected
- [ ] #12 Functional test: Sidebar rail mode toggle works
- [ ] #13 Screenshot captured showing the updated Systems view for MR
- [ ] #14 No console errors in browser developer tools
- [ ] #15 Responsive test: Layout works on 1920px, 1366px, and 1024px widths
<!-- DOD:END -->
