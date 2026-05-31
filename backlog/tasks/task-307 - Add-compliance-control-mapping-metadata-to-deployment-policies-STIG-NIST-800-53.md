---
id: TASK-307
title: >-
  Add compliance control mapping metadata to deployment policies (STIG + NIST
  800-53)
status: Backlog
assignee: []
created_date: '2026-05-23 23:53'
labels:
  - compliance
  - stig
  - nist-800-53
  - deployment-policies
  - sprint-ready
milestone: STIG policy readiness
dependencies: []
priority: high
ordinal: 254000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Current deployment policies can enforce behavior but lack first-class compliance traceability metadata. This makes it hard to map policy outcomes to DISA STIG and NIST 800-53 controls during audits.

## Goal
Extend deployment policy data model and API contract to support explicit control mapping metadata for STIG and NIST 800-53 so every policy can declare which controls it satisfies.

## Non-Goals
- No reporting/export bundle generation in this task
- No UI implementation beyond API/model support
- No deployment-manager enforcement wiring changes beyond metadata pass-through

## Scope
- Add policy metadata fields (framework, control_ids, severity, rationale, evidence_required)
- Support framework values for at least: DISA-STIG and NIST-800-53
- Persist metadata in existing policy storage model
- Validate metadata in create/update policy APIs
- Ensure backward compatibility for existing policies without metadata

## Architectural Constraints
- Keep business logic out of UI
- Reuse existing policy-as-data patterns
- Keep policy type DTO/server model alignment
- Preserve compatibility with existing policy records and migrations

## Verification Plan (Tier 0)
- Targeted unit tests for metadata validation/parsing
- Targeted API handler tests for create/update acceptance/rejection cases
- cargo check and relevant targeted test modules for policy model + handlers

## Impact Areas
- packages/default/src/models/deployment_policies.rs
- packages/default/src/handlers/api/deployment_policies.rs
- packages/default/src/queries/deployment_policies.rs
- migrations and/or JSON config schema handling (if required)

## Risk Level
Medium: schema/model/API contract change with backward compatibility requirements.

## Dependencies
None
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy model supports compliance metadata fields: framework, control_ids, severity, rationale, evidence_required
- [ ] #2 Framework validation accepts DISA-STIG and NIST-800-53
- [ ] #3 Existing policies without compliance metadata continue to load and evaluate without failure
- [ ] #4 Create/update policy APIs validate and persist compliance metadata correctly
- [ ] #5 Unit/API tests cover valid and invalid metadata cases
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Document field semantics for STIG and NIST 800-53 mapping in deployment policy docs
<!-- DOD:END -->
