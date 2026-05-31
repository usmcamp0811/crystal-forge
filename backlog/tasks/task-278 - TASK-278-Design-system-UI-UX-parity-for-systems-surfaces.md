---
id: TASK-278
title: TASK-278 Design-system UI/UX parity for systems surfaces
status: Done
assignee:
  - '@ai-agent'
created_date: '2026-04-19 17:54'
updated_date: '2026-04-20 20:33'
labels:
  - systems
  - ui
  - ux
  - web-ui
  - design-system
milestone: UI/UX parity
dependencies: []
references:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/system/system_card_v2.rs
  - packages/web-ui/src/components/tables/systems_table.rs
  - packages/web-ui/src/components/onboarding/coach_panel.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - packages/web-ui/src/components/systems_stat_strip.rs
  - checks/web-ui/default.nix
documentation:
  - >-
    /home/mcamp/code/crystal-forge/design-example-systems/components/SystemDetail.jsx
  - /home/mcamp/code/crystal-forge/design-example-systems/styles.css
priority: medium
ordinal: 2780
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Complete systems UI/UX parity updates including density/view toggles, sidebar behavior, setup coach positioning, environment color fidelity, flake metadata display, timer/activity live updates, and web-ui check stabilization for merge.
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
Task moved to Done after merge confirmation from user (MR #243).
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Merged MR #243 completed TASK-278 UI/UX parity improvements for systems workflows.

Highlights:
- Functional density/default view toggles and fixed sidebar/topbar scroll behavior
- Correct environment color mapping from API for spark bar + badges (cards/table/preview drawer)
- Replaced placeholder flake/commit rendering with real flake name + latest commit context
- Live timers/activity in systems preview and system detail views
- Setup coach now side drawer with proper minimized tab position
- Sidebar/logo and navigation ordering polish
- Dropdown/tweaks outside-click dismiss behavior and native select filter alignment
- Stabilized web-ui check gate for merge while preserving screenshot coverage
<!-- SECTION:FINAL_SUMMARY:END -->

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
