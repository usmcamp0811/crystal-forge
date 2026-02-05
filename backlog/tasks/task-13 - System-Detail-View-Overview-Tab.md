---
id: TASK-13
title: System Detail View - Overview Tab
status: To Do
assignee: []
created_date: '2026-02-05 14:25'
labels:
  - ui
  - views
  - systems
dependencies:
  - TASK-8.7
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Build system detail page with overview information.

Steps:
1. Create system detail view with tabs
2. Implement Overview tab showing:
   - Current config (commit hash, NixOS version)
   - Hardware info (CPU, memory, disk)
   - Network info (IP, interfaces)
   - Health status with visual indicator
   - Deployment status badge
3. Add actions: Deploy, Rollback, Force Sync, Delete
4. Fetch data from MockClient
5. Handle action button clicks

Expected: Overview answers "is it up to date? why not?"
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Overview tab complete
- [ ] #2 All info displayed
- [ ] #3 Action buttons work
- [ ] #4 Answers key questions
<!-- AC:END -->
