---
id: TASK-9
title: UI Components - Layout Components
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-05 14:25'
updated_date: '2026-02-21 03:28'
labels:
  - ui
  - components
milestone: m-3
dependencies:
  - TASK-8
  - TASK-8.4
priority: high
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build core layout components for the Dioxus web UI using Tailwind CSS. Web-only target.

Steps:
1. Create src/ui/components/layout/app_shell.rs - main layout with sidebar + content area
2. Create sidebar.rs - navigation menu with links (Dashboard, Systems, Builds, CVEs)
3. Create topbar.rs - header bar with page title and optional search input
4. Create card.rs - reusable card container with header/body/footer slots
5. Apply Tailwind dark theme classes (bg-gray-900, text-gray-100, etc.)
6. Implement responsive layout (sidebar collapses on mobile via Tailwind breakpoints)
7. Set up Dioxus Router for page navigation between views
8. Test layout in Chrome and Firefox at various viewport sizes

Architecture notes:
- All layout components in src/ui/components/layout/
- Use Tailwind utility classes directly in RSX (class: "...")
- Sidebar navigation drives Dioxus Router

Expected: Responsive dark-themed shell with working navigation between placeholder pages
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 AppShell component with sidebar + content area
- [ ] #2 Sidebar with navigation links (Dioxus Router integrated)
- [ ] #3 TopBar with page title
- [ ] #4 Card component reusable with header/body slots
- [ ] #5 Responsive layout (sidebar collapses on mobile)
- [ ] #6 Dark theme applied via Tailwind
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Closed after in-progress review: core layout components (app shell, sidebar, topbar, card) exist and are integrated in web-ui layout module.
<!-- SECTION:NOTES:END -->
