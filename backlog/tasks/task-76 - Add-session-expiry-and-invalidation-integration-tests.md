---
id: TASK-76
title: Add session expiry and invalidation integration tests
status: Backlog
assignee: []
created_date: '2026-02-21 04:17'
labels:
  - sessions
dependencies:
  - TASK-65.3
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add database integration tests for session lifecycle: expired sessions cannot be used, invalidated sessions cannot be used, session cleanup/garbage collection works correctly. Currently only unit tests exist for cookie handling and CSRF validation.
<!-- SECTION:DESCRIPTION:END -->
