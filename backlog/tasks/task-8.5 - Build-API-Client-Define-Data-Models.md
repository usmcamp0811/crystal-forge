---
id: TASK-8.5
title: Build API Client - Define Data Models
status: To Do
assignee: []
created_date: '2026-02-05 14:15'
labels:
  - ui
  - api
  - backend
dependencies:
  - TASK-8.3
parent_task_id: TASK-8
priority: high
milestone: m-3
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create Rust structs for all API request/response models with serde.

Steps:
1. Create src/api/models.rs
2. Define enums: HealthStatus, DeploymentStatus, BuildStatus, CveSeverity
3. Define structs: SystemSummary, DashboardSummary, SystemDetail, Flake, Commit, Build
4. Add serde Serialize/Deserialize derives
5. Implement helper methods (e.g., CveCount::total())
6. Write unit tests for model serialization
7. Document all public types with doc comments

Expected: All models compile, tests pass, no warnings
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All API models defined
- [ ] #2 Serde serialization works
- [ ] #3 Unit tests pass
- [ ] #4 Doc comments on all public types
<!-- AC:END -->
