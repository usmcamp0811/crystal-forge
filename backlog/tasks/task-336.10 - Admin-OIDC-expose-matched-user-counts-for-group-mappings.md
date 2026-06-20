---
id: TASK-336.10
title: 'Admin OIDC: expose matched-user counts for group mappings'
status: Backlog
assignee: []
created_date: '2026-06-20 16:24'
labels:
  - admin
  - oidc
  - api
  - backend
milestone: 'm-20: Design Parity Missing Surfaces'
dependencies:
  - TASK-336
references:
  - TASK-336.2
parent_task_id: TASK-336
priority: low
ordinal: 316000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: The Admin OIDC Mappings tab design includes matched-user counts, but the current OIDC mapping API does not expose how many users match each mapping. Desired outcome: Add backend/API support for real matched-user counts and wire the OIDC tab to display actual values instead of unavailable placeholders.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 OIDC mappings API exposes real matched-user counts
- [ ] #2 OIDC tab displays real matched-user counts without group-name heuristics
<!-- AC:END -->
