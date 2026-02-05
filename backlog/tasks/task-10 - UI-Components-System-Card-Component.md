---
id: TASK-10
title: UI Components - System Card Component
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - ui
  - components
dependencies:
  - TASK-8.4
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build SystemCard component to display system summary.

Steps:
1. Create src/components/system/system_card.rs
2. Accept props: hostname, environment, health, deployment_status, cve_count
3. Use StatusBadge for health indicator
4. Display CVE count with severity colors
5. Add click handler to navigate to system detail
6. Style with design system
7. Test with mock data

Expected: Card shows all system info clearly
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SystemCard component created
- [ ] #2 All props displayed correctly
- [ ] #3 Click navigation works
- [ ] #4 Styled per design system
<!-- AC:END -->
