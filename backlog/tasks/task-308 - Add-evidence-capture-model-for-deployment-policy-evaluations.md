---
id: TASK-308
title: Add evidence capture model for deployment policy evaluations
status: Backlog
assignee: []
created_date: '2026-05-23 23:53'
labels:
  - compliance
  - evidence
  - stig
  - nist-800-53
  - deployment-policies
  - sprint-ready
milestone: STIG policy readiness
dependencies:
  - TASK-307
priority: high
ordinal: 255000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
Policy evaluation currently returns allow/deny reasoning but does not persist structured compliance evidence artifacts. This limits auditability for STIG and NIST 800-53 use cases.

## Goal
Introduce a structured evidence capture model for policy evaluations so each result can reference evidence type and evidence location/identifier.

## Non-Goals
- No full reporting/export generation in this task
- No external SIEM integration
- No UI rendering for evidence browsing

## Scope
- Add evidence fields to policy evaluation result model (evidence_type, evidence_ref, evaluated_at, evaluator_version)
- Persist evidence metadata for advanced deployment policy evaluations where applicable
- Ensure evidence fields are optional and backward compatible
- Define allowed evidence types (command_output, file_hash, config_snapshot, attestation)

## Architectural Constraints
- Keep policy engine/service layer as source of truth
- Do not introduce hidden global state
- Maintain clear separation of API models vs domain/service logic

## Verification Plan (Tier 0)
- Unit tests for evidence type validation/normalization
- Targeted service tests ensuring evidence metadata is attached to evaluation results
- cargo check + targeted test modules for policies/evaluation services

## Impact Areas
- packages/default/src/models/deployment_policies.rs
- packages/default/src/services/*policy*.rs
- packages/default/src/queries/deployment_policies.rs (if persistence updated)

## Risk Level
Medium: result contract change affecting policy service consumers.

## Dependencies
- TASK-307 (control mapping metadata baseline)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy evaluation results support optional fields: evidence_type, evidence_ref, evaluated_at, evaluator_version
- [ ] #2 Evidence type validation accepts command_output, file_hash, config_snapshot, attestation
- [ ] #3 Existing policy evaluation flows continue to function when evidence fields are absent
- [ ] #4 At least one advanced policy service path emits structured evidence metadata
- [ ] #5 Unit/service tests cover evidence validation and result attachment behavior
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Document evidence field usage and retention expectations for auditors
<!-- DOD:END -->
