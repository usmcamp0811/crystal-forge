---
id: TASK-332
title: >-
  Align shared backend API contracts to support design-required real-data fields
  and states
status: Backlog
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 02:57'
labels:
  - design-parity
  - backend-api
  - data-contract
milestone: m-18
dependencies:
  - TASK-328
modified_files:
  - packages/server/src
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/api/client.rs
priority: high
ordinal: 1640
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Some design surfaces require structured data/states not consistently exposed by current APIs, leading to UI placeholders, local derivations, or mismatched semantics.

Goal: Update backend API models/endpoints to provide all shared fields required for parity UI states with authoritative server semantics.

Non-goals: Replacing stable endpoint patterns without need; per-view visual work.

Replan note: this is m-18 shared-contract foundation work. Prefer only truly cross-view API contract additions here; surface-specific API changes should land inside each vertical slice task when practical.

Scope details:
- Audit UI-required fields from parity matrix against current API responses.
- Add/adjust shared endpoint fields for dashboards, systems, flakes, builds, evals, CVEs, compliance/admin indicators as required.
- Review versioning/compatibility where public contracts change.
- Update web-ui model mappings to consume revised contracts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A documented field-level contract diff exists between current API and design-required data
- [ ] #2 All design-required shared fields/states are available from authoritative backend APIs
- [ ] #3 API tests validate new/changed fields and failure/empty-state semantics
- [ ] #4 Affected web-ui surfaces consume backend data without fallback placeholders in production path
- [ ] #5 web-ui check is updated with assertions proving backend-driven values render correctly in target views
- [ ] #6 web-ui check captures screenshots of all affected states that depend on updated API contracts
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Use this task only for shared cross-view contract gaps. Keep surface-specific data changes inside the owning parity slice when possible.
<!-- SECTION:NOTES:END -->
