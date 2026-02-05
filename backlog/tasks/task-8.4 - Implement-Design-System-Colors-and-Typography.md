---
id: TASK-8.4
title: Implement Design System - Colors and Typography
status: To Do
assignee: []
created_date: '2026-02-05 14:15'
labels:
  - ui
  - design-system
dependencies:
  - TASK-8.3
parent_task_id: TASK-8
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create CSS variables and Rust constants for the design system.

Steps:
1. Create web/styles.css with CSS custom properties
2. Define color palette (dark mode): backgrounds, accents, status colors, CVE severity
3. Define typography scale and font families
4. Define spacing system (4px base unit)
5. Create src/utils/colors.rs with terminal color mappings for TUI
6. Document design tokens in README
7. Test colors render correctly in both web and TUI

Reference: UI_WIREFRAMES.md design system section
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CSS variables defined for all colors
- [ ] #2 Typography scale implemented
- [ ] #3 Spacing system documented
- [ ] #4 TUI color mappings created
- [ ] #5 Design tokens documented
<!-- AC:END -->
