---
id: TASK-78
title: Implement session garbage collection
status: To Do
assignee: []
created_date: '2026-02-21 04:19'
updated_date: '2026-02-23 03:21'
labels:
  - sessions
dependencies:
  - TASK-65.3
priority: medium
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add background task to clean up expired and invalidated sessions from user_sessions table. Should run periodically (e.g., hourly) and remove sessions where expires_at < NOW() or invalidated_at IS NOT NULL and older than retention period.
<!-- SECTION:DESCRIPTION:END -->
