---
id: TASK-8.5
title: Build API Client - Define Data Models
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-05 14:15'
updated_date: '2026-03-13 01:24'
labels:
  - ui
  - api
  - backend
milestone: m-3
dependencies:
  - TASK-8.3
parent_task_id: TASK-8
priority: high
ordinal: 52000
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
- [x] #1 #1 #1 #1 All API models defined
- [x] #2 #2 #2 #2 Serde serialization works
- [x] #3 #3 #3 #3 Unit tests pass (16 tests)
- [x] #4 #4 #4 #4 Doc comments on all public types

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
- Created `src/api/mod.rs` and `src/api/models.rs`
- Registered `api` module in `src/lib.rs`
- DTOs defined: DashboardSummary, FleetHealthSummary, DeploymentStatusSummary, CveSummary, RecentDeployment, SystemSummary, SystemDetail, SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo, FlakeSummary, PaginatedResponse<T>, SystemsListParams, ApiError
- Enums: HealthStatus, DeploymentStatus, CveSeverity, PipelineStage, SortOrder
- All enums use `#[serde(rename_all = "snake_case")]` for consistent JSON
- ApiError uses `#[serde(skip_serializing_if = "Option::is_none")]` for optional details
- PaginatedResponse is generic and includes `total_pages()` helper
- All types have doc comments explaining their purpose and data source
<!-- AC:END -->
<!-- SECTION:NOTES:END -->
