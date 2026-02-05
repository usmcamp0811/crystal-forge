---
id: TASK-9
title: UI Components - Layout Components
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - ui
  - components
dependencies:
  - TASK-8
  - TASK-8.4
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build core layout components: AppShell, Sidebar, TopBar, Card.

Steps:
1. Create src/components/layout/app_shell.rs with main layout
2. Create sidebar.rs with navigation menu
3. Create topbar.rs with search bar and user menu
4. Create card.rs for content containers
5. Apply design system colors and spacing
6. Test in both web and TUI
7. Write component documentation

Expected: Layout components work in both targets
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 AppShell component complete
- [ ] #2 Sidebar with navigation
- [ ] #3 TopBar with search
- [ ] #4 Card component reusable
- [ ] #5 Works in web and TUI
<!-- AC:END -->
