---
id: TASK-4
title: 'Phase 3: Introduce Service Layer'
status: Backlog
assignee: []
created_date: '2026-02-04 20:15'
updated_date: '2026-02-19 03:39'
labels:
  - refactoring
  - architecture
  - phase-3
milestone: m-2
dependencies:
  - TASK-3
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decouple HTTP handlers from database queries by introducing service layer. Currently handlers directly call query functions, creating tight coupling and making testing difficult.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Create AgentService
- [ ] #2 Create BuildService
- [ ] #3 Create DeploymentService
- [ ] #4 Create FlakeService
- [ ] #5 Create CveService
- [ ] #6 Update handlers to use services
- [ ] #7 Add service layer tests
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Target: Handlers <50 lines each, services >80% coverage, all tests pass
<!-- SECTION:NOTES:END -->
