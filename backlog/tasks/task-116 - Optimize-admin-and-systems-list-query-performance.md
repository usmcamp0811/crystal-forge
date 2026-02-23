---
id: TASK-116
title: Optimize admin and systems list query performance
status: Backlog
assignee: []
created_date: '2026-02-22 19:23'
labels: []
dependencies: []
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: admin users list performs N+1 role/environment queries and audit/system list endpoints fetch broad datasets then filter/paginate in memory, which can degrade with larger fleets. Desired Outcome: push filters/pagination into SQL and batch/aggregate user role+environment lookups to avoid N+1 access patterns.
<!-- SECTION:DESCRIPTION:END -->
