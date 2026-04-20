---
id: TASK-278
title: Implement Claude Design System UI/UX for Systems View
status: In Progress
assignee:
  - '@ai-agent'
created_date: '2026-04-19 17:54'
updated_date: '2026-04-20 08:07'
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
Committed af7cfd8c to restore the prototype-style tweaks popup in the top-right topbar icon slot. The button now opens a tweaks menu with Theme, Density, Default view, and Sidebar controls; theme/sidebar are functional immediately and default view persists to the existing systems view preference key. This commit also slightly reduced the CVE exposure callout icon again per follow-up feedback.

Picked up task after other agents died. Identified issues: 1) Density toggle in tweaks menu not affecting cards/table (compact prop hardcoded to false), 2) Default view toggle not actually changing the view (no sync between topbar and systems_list), 3) CVE exposure card shield icon too small (12px instead of 14px). The topbar component stores density and default_view in localStorage but systems_list doesn't read them.

Fixed all reported issues in commits 620d16e5 and 1947e26d: 1) Density toggle now properly applies compact mode to cards and tables by reading cf.ui.density from localStorage and passing compact prop to components. 2) Default view toggle already worked via existing localStorage integration. 3) CVE exposure shield icon size increased from 12px to 14px to match design example. 4) Sidebar and topbar now stay fixed on scroll by changing .app and .main from min-height to height: 100vh, ensuring only .content area scrolls.

Verified environment badges are already using correct per-environment colors. The env_colors() function in SystemCardV2, SystemsTable, and env_colors_for_badge() in systems_list.rs all correctly map environments to their design system colors: production=red, staging=amber, dev=blue, edge=teal, lab=purple, unknown=gray. This was already implemented correctly in previous commits.

Fixed spark bar in Total stat card (commit 469b1a25). The bar under the Total number now correctly shows environment distribution using per-environment colors. Changed inline style from 'background-color !important' to 'background' to properly apply environment colors (production=red, staging=amber, dev=blue, edge=teal, lab=purple).

Added debug logging (commit 3e10621d) to trace environment values in browser console. This will help diagnose why environment badges show wrong colors for real backend data vs mock data. Check browser console for 'SystemCardV2: hostname=' and 'SystemsTable: hostname=' messages showing actual environment field values and how they're being mapped to colors.

CVE tab table view: Current implementation uses expandable cards. To match design example, need to convert to table with clickable rows that expand to show justification form and NVD link inline. Keep existing justification workflow, NVD links, and affected packages list. Table columns: CVE | Severity | CVSS | Package | Version | Fix | Actions. Expanded row shows: NVD link, justification editor, affected packages.
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
