---
id: TASK-65.4
title: JIT provisioning and claim-to-role mapping
status: To Do
assignee:
  - MiniMax M2.5
created_date: ''
updated_date: '2026-02-21 04:25'
labels:
  - security
  - auth
  - rbac
  - backend
milestone: m-14
dependencies:
  - TASK-65.1
  - TASK-65.2
  - TASK-65.3
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
Authenticated users need deterministic local identity and role assignment on first and subsequent logins.

Goal
Implement JIT provisioning and configurable claim mapping into Admin, Operator, and Viewer roles.

Non-Goals
- Manual invite workflow.
- Arbitrary custom role editor in v1.

Architectural Constraints
- Mapping logic must be pure domain or service code with tests.
- Role decisions cannot depend on UI state.
- Explicit error types for mapping failures.

Verification Plan
- `nix develop -c cargo test --package default auth::jit_provisioning`
- `nix develop -c cargo test --package default auth::role_mapping`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: validate first-login create and role-change update flow.

Impact Areas
- Domain, API, Database, Security

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 First successful login creates local user record
- [ ] #2 Subsequent logins update mapped role bindings when claims change
- [ ] #3 Default claim mapping uses `groups`
- [ ] #4 Claim source key is configurable for `groups`, `roles`, or a custom key
- [ ] #5 Mapping failures result in safe-deny behavior with auditable error
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: finer role granularity as a separate backlog item.
<!-- SECTION:NOTES:END -->
