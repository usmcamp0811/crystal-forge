---
id: TASK-309
title: Add waiver/exception workflow for deployment policy compliance gates
status: Backlog
assignee: []
created_date: '2026-05-23 23:53'
labels:
  - compliance
  - waiver
  - exceptions
  - stig
  - nist-800-53
  - deployment-policies
  - sprint-ready
milestone: STIG policy readiness
dependencies:
  - TASK-307
  - TASK-308
priority: high
ordinal: 256000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
There is no first-class waiver workflow for temporary policy exceptions with accountability. STIG-aligned operations require controlled exceptions (POA&M style) with expiration and approval.

## Goal
Implement waiver/exception support for deployment policy gates with admin-only approval, expiration, and justification.

## Non-Goals
- No multi-step role-chain approvals in v1
- No broad UI exception dashboard
- No automated POA&M export format in this task

## Scope
- Add waiver entity linked to policy + deployment context
- Required fields: reason/justification, compensating_control (optional), expires_at, requested_by, approved_by, status
- Admin-only waiver approval in v1
- Policy evaluation checks active approved waiver and records waiver usage in result reason/evidence metadata
- Expired waivers automatically stop applying (runtime check based on expires_at)

## Architectural Constraints
- Keep authz checks in API/handler boundary
- Keep waiver semantics explicit and deterministic
- Preserve existing policy evaluation behavior when no waiver exists

## Verification Plan (Tier 0)
- Unit tests for waiver state transitions and expiration logic
- API tests for admin-only approval enforcement
- Targeted service tests proving waiver bypass behavior only for valid active approved waivers
- cargo check + targeted tests for waiver and policy modules

## Impact Areas
- New query/service modules for waivers
- Policy evaluation services to consult waiver state
- API handlers for create/approve/revoke/list waiver records
- DB migration for waiver persistence

## Risk Level
High: introduces exception path in enforcement logic; requires careful authz and expiration checks.

## Dependencies
- TASK-307
- TASK-308
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Waiver records can be created with required justification and expiration metadata
- [ ] #2 Only admins can approve waivers; non-admin approval attempts are rejected
- [ ] #3 Policy evaluation bypasses gate only when waiver is approved and unexpired for matching context/policy
- [ ] #4 Expired waivers are ignored automatically without manual cleanup requirement
- [ ] #5 Unit/API/service tests cover happy path, unauthorized approval, and expiration edge cases
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Document waiver semantics including non-retroactive and expiration behavior in deployment policy docs
<!-- DOD:END -->
