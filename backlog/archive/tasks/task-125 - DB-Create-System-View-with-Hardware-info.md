---
id: TASK-125
title: 'DB: Create System View with Hardware info'
status: Backlog
assignee: []
created_date: '2026-02-24 02:13'
labels: []
milestone: m-7
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
We should have a view in the db that give you all the system information. So the hardware info comes from agent heartbeats, so we don't have it at first, but we could return some place holder or null values, and this view should have a flag field if the system hardware changed in the past day, and/or ever. 

this view should probably be used to populate the ui view
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 i can query the view and it returns all the values in the SystemDetail model
<!-- AC:END -->
