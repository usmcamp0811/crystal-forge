---
id: TASK-8.4
title: Implement Design System - Tailwind CSS Dark Theme
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
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the design system using Tailwind CSS with a dark theme. No separate wireframes needed; we are using Tailwind defaults with project-specific customizations.

Steps:
1. Configure Tailwind (via CDN initially, proper build pipeline in TASK-8.10)
2. Set dark mode as default (class-based or media-based)
3. Define custom color tokens for Crystal Forge in tailwind config or CSS custom properties:
   - Status colors: healthy (green-500), warning (amber-500), critical (red-500), offline (gray-500)
   - CVE severity: critical (red-600), high (orange-500), medium (yellow-500), low (blue-400)
   - Deployment status: up-to-date (green), behind (red), pending (amber), unknown (gray)
4. Define typography: use Tailwind defaults (Inter or system fonts), ensure monospace for hashes/paths
5. Define spacing: use Tailwind's default 4px base scale
6. Create a theme.rs or constants module with Rust-side color/status mappings for consistent rendering
7. Create a small style guide page/component showing all tokens in the Dioxus app
8. Test dark theme renders correctly in Chrome and Firefox

Expected: Consistent visual language across all UI components, dark theme by default
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tailwind CSS integrated with dark mode as default
- [ ] #2 Custom status/severity color tokens defined
- [ ] #3 Typography and spacing using Tailwind defaults
- [ ] #4 Rust-side theme constants for status-to-color mapping
- [ ] #5 Style guide component showing all design tokens
<!-- AC:END -->
