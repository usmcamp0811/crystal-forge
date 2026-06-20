---
id: TASK-336.9
title: 'Admin Users: expose real MFA enrollment/status for users'
status: Backlog
assignee: []
created_date: '2026-06-20 16:24'
labels:
  - admin
  - users
  - mfa
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies:
  - TASK-336
references:
  - TASK-336.2
parent_task_id: TASK-336
priority: low
ordinal: 315000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The Admin Users tab design includes MFA status, but the current admin user summary API does not expose MFA enrollment/status. Desired outcome: Add backend/API support for real MFA status and wire the Users tab to display actual values instead of unavailable placeholders.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Admin user summary API exposes real MFA enrollment/status
- [ ] #2 Users tab displays real MFA status without inference from username
<!-- AC:END -->
