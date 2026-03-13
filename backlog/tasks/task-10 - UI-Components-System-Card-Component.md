---
id: TASK-10
title: UI Components - System Card Component
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-05 14:25'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - components
milestone: m-7
dependencies:
  - TASK-8.4
  - TASK-8.5
priority: high
ordinal: 48000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build SystemCard component to display system summary using Tailwind CSS.

Steps:
1. Create src/ui/components/system/system_card.rs
2. Accept props: hostname, environment, health_status, deployment_status, cve_counts
3. Create StatusBadge sub-component with Tailwind status colors (green/amber/red/gray)
4. Display CVE count with severity color indicators (matching TASK-8.4 tokens)
5. Add click handler to navigate to system detail via Dioxus Router
6. Style with Tailwind dark theme (card bg-gray-800, rounded, shadow, hover states)
7. Test with mock data from MockClient (TASK-8.7)

Props should map to the API DTOs defined in TASK-8.5, not DB models directly.

Expected: Card shows all system info clearly with proper status coloring
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 SystemCard component created
- [ ] #2 All props displayed correctly
- [ ] #3 Click navigation works
- [ ] #4 Styled per design system
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Implemented SystemCard component with status badges, CVE summary, and navigation. Added mock cards on Systems view for layout preview. Created follow-up TASK-MEDIUM.2 for populating table rows.
<!-- SECTION:NOTES:END -->
