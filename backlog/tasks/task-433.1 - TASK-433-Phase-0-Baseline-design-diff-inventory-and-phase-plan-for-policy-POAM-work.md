---
id: TASK-433.1
title: >-
  TASK-433 Phase 0: Baseline design-diff inventory and phase plan for
  policy/POA&M work
status: To Do
assignee: []
created_date: '2026-08-23 01:42'
labels:
  - design-parity
  - policy
  - enforcement
  - compliance
  - poam
  - phase-0
dependencies: []
references:
  - >-
    https://gitlab.com/crystal-forge/crystal-forge/-/commit/ae20da816edb1cb14275db9cd646010e69d88cd8
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/commit/c2f5db08'
  - TASK-433
documentation:
  - docs/design/CrystalForge/
  - docs/agent/database-safety.md
  - docs/agent/verification.md
  - packages/default/WORKSPACE.md
parent_task_id: TASK-433
priority: high
type: spike
ordinal: 433000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433. This is contextual only and is not an execution-blocking dependency framework, but this subtask's output is a hard prerequisite input for TASK-433 Phase 1-8 subtasks.

Produce the authoritative, exact-SHA design-diff inventory for `c2f5db08..ae20da816edb1cb14275db9cd646010e69d88cd8` against current `dev`, classify every changed design file (product behavior vs demo-only per TASK-433's explicit non-scope list), and document current server/web-ui implementation state for the areas TASK-433 touches (policy catalog, policy editor, deployment_policies models/queries, compliance queries, framework_requirements queries, web-ui policies/compliance/system_detail/dashboard views). Then record a detailed phase-by-phase implementation plan in the TASK-433 parent record, confirming/refining the phase-to-subtask mapping (TASK-433.2 through TASK-433.9) so later subtasks can proceed without re-deriving this context.

This is a research/documentation subtask only. No production code changes.

## Explicit scope
- Exact diff/stat between the two design SHAs (`c2f5db08` and `ae20da816edb1cb14275db9cd646010e69d88cd8`) for `docs/design/CrystalForge/`.
- File-by-file classification: product behavior covered by a specific TASK-433 acceptance criterion, or demo-only/not-ported (fixtures, `.thumbnail`, `crystal-forge.html`, `CustomEvent`/`window.__cfCoach`, localStorage POAM state, etc. per TASK-433 non-scope).
- Current-state audit of: `packages/default/crates/cf-server/src/models/deployment_policies.rs`, `packages/default/crates/cf-server/src/queries/deployment_policies.rs`, `packages/default/crates/cf-server/src/queries/compliance.rs`, `packages/default/crates/cf-server/src/queries/framework_requirements.rs`, `packages/web-ui/src/views/policies.rs`, `packages/web-ui/src/views/policies_api.rs`, `packages/web-ui/src/views/compliance.rs`, `packages/web-ui/src/views/system_detail.rs`, `packages/web-ui/src/views/dashboard.rs`, and existing migrations.
- Confirm no POA&M persistence/API exists today (baseline for Phase 5).
- Record the reviewed phase-by-phase plan in TASK-433's plan field.

## Explicit non-scope
No production code, migrations, or tests are added or modified in this subtask.

## Verification
Documentation/plan review only; no automated verification commands apply to this subtask beyond confirming no files outside `backlog/` were changed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Exact diff/stat between `c2f5db08` and `ae20da816edb1cb14275db9cd646010e69d88cd8` for docs/design/CrystalForge/ is captured and recorded.
- [ ] #2 Every changed design file (app.jsx, ComplianceView.jsx, DashboardView.jsx, PoamViews.jsx, PoliciesView.jsx, PolicyEditor.jsx, SetupCoach.jsx, Shell.jsx, SystemDetail.jsx, data-*.js, styles.css, and others) is classified as product-behavior-covered-by-criteria or demo-only-not-ported, contributing to parent TASK-433 AC #30.
- [ ] #3 Current implementation state of policy catalog, policy editor, deployment_policies models/queries, compliance queries, framework_requirements queries, and web-ui policies/compliance/system_detail/dashboard views is documented with concrete file/function references and identified gaps against TASK-433 scope.
- [ ] #4 Confirms and documents that no POA&M persistence/API/UI exists in current dev baseline.
- [ ] #5 A detailed phase-by-phase implementation plan for TASK-433 Phases 1-8 (mapped to subtasks TASK-433.2 through TASK-433.9) is recorded in the parent TASK-433 task record and presented for review before any subtask begins implementation.
- [ ] #6 No production/test files are modified by this subtask.
<!-- AC:END -->
