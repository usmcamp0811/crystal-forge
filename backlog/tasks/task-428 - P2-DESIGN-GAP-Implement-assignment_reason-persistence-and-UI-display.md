---
id: TASK-428
title: 'P2 DESIGN GAP: Implement assignment_reason persistence and UI display'
status: Backlog
assignee: []
created_date: '2026-08-20 02:34'
labels:
  - p2
  - compliance
  - design-gap
dependencies: []
priority: high
type: task
ordinal: 423000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement

The TASK-422 compliance redesign (design commit 23c88aba, `ComplianceView.jsx`) explicitly displays `assignment.reason` to users:

> "pinned to this revision instead of the current baseline. {assignment.reason}"

Users can see the reason for why a system is pinned to an older bundle version. The design requires this field to be:

1. **Displayed** in the per-system evidence detail callout
2. **Authored** during assignment creation (implied by "reason" being semantically different from "approved_by")
3. **Persisted** across assignment updates

However, current implementation:

1. **No schema field** in `compliance_bundle_assignments` or `compliance_bundle_assignment_versions` to store reason/justification
2. **No API field** in `CreateAssignmentRequest` or `UpdateAssignmentRequest` to accept reason
3. **No UI component** to input reason during assignment creation/edit
4. **Hard-coded `None`** in `ComplianceSystemRollup.assignment_reason`

The `provenance` field exists in `compliance_bundle_assignment_versions` (defaults to `{}`) but is currently unused for assignments.

## Design Intent

The design mock (`docs/design/CrystalForge/components/ComplianceView.jsx` lines ~XXX) clearly intends for assignment reason to be:

- **User-provided**: entered when assignment is created or modified
- **Displayed prominently**: shown in the assignment status callout
- **Distinct from approval**: separate from `assignment.approvedBy` (which maps to `created_by`)
- **Optional**: but when present, informs users *why* the pinning decision was made

Example use cases from ATO/governance:
- "Vendor testing in progress, EOD Friday rollback scheduled"
- "Migration from RHEL 7 to RHEL 8, holds new baseline pending verification"
- "Performance regression in v2.4.1, pinned to v2.3.9 pending hotfix"

## Required Work

1. **Database**: Add `reason` text field to `compliance_bundle_assignment_versions` (semantic: reason for this version assignment)
2. **API Request**: Add optional `reason: Option<String>` to `CreateAssignmentRequest` and `UpdateAssignmentRequest`
3. **Mutation**: Populate `reason` when inserting assignment version rows
4. **Query**: Load `reason` in `load_assignment_metadata_for_systems()` and wire to `ComplianceSystemRollup.assignment_reason`
5. **UI Input**: Add text field to assignment create/edit modal (location TBD, likely in assignment metadata panel)
6. **UI Display**: Render reason in evidence detail callout (already expected by design)
7. **Migration**: Create new migration for schema change
8. **Tests**: Verify reason persists, survives updates, correctly omitted when None

## Related Fields

- `assignment.approvedBy` (mapped from `created_by`): who made the assignment decision — semantically the approval action
- `assignment.deadline` (not yet implemented): optional target date for migration
- `assignment.poam` (not yet implemented): optional POA&M reference

Deadline and POA&M may remain unimplemented if they are not domain requirements. Reason is different: the design explicitly references it.

## Out of Scope

This task does not modify TASK-418's normalized requirement/mapping model.

## References

- Design: `docs/design/CrystalForge/components/ComplianceView.jsx` (assignment reason display)
- Schema: `packages/default/crates/cf-server/migrations/0204_compliance_assignment_versions.sql`
- API models: `packages/default/crates/cf-server/src/api/models.rs` (ComplianceSystemRollup)
- Query: `packages/default/crates/cf-server/src/queries/compliance.rs` (load_assignment_metadata_for_systems)
<!-- SECTION:DESCRIPTION:END -->
