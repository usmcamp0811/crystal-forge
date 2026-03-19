---
id: TASK-196
title: >-
  Dashboard pipeline readiness widget doesn't support scrolling for multiple
  errors/warnings
status: Backlog
assignee: []
created_date: '2026-03-19 12:36'
labels:
  - bug
  - ui
  - dashboard
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The dashboard widget that displays pipeline readiness errors/warnings does not have scrolling functionality. When there are multiple errors or warnings, they overflow the widget boundaries without any way to view all of them.

This makes it difficult or impossible to see all the issues that need to be addressed, especially when there are many evaluation errors or policy violations.

## Current Behavior

- Pipeline readiness widget shows errors/warnings in a fixed-height container
- When multiple items exist, they overflow without scroll capability
- Users cannot see all errors/warnings
- No visual indication that there are more items below the fold

## Expected Behavior

- Widget should have a scrollable area when content exceeds available height
- All errors/warnings should be accessible by scrolling
- Visual indication (scrollbar or gradient fade) that more content exists
- Consistent with other scrollable widgets on the dashboard

## Impact Areas

- Dashboard UI component
- Pipeline readiness widget styling
- CSS/layout for scrollable containers

## User Impact

**Medium** - Users with multiple pipeline issues cannot see all problems, making it harder to understand what needs to be fixed.
<!-- SECTION:DESCRIPTION:END -->
