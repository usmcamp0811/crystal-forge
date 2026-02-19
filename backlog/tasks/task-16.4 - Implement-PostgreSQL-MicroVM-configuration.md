---
id: TASK-16.4
title: Implement PostgreSQL MicroVM configuration
status: Backlog
assignee: []
created_date: '2026-02-05 15:16'
updated_date: '2026-02-19 03:39'
labels:
  - implementation
  - postgresql
  - microvm
milestone: m-1
dependencies: []
parent_task_id: TASK-16
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create MicroVM configuration for PostgreSQL database. Configure PostgreSQL to accept connections from other VMs, set up initial database and user.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create PostgreSQL VM configuration
- [ ] #2 Configure PostgreSQL to listen on VM IP
- [ ] #3 Set up crystal_forge database and user
- [ ] #4 Configure pg_hba.conf for network access
- [ ] #5 Add health check script
- [ ] #6 Test connection from host and other VMs
<!-- AC:END -->
