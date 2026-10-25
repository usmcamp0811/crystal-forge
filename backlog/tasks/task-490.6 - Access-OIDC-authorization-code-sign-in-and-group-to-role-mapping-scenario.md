---
id: TASK-490.6
title: 'Access: OIDC authorization-code sign-in and group-to-role mapping scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:36'
updated_date: '2026-10-24 23:07'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
modified_files:
  - checks/integration-access/scenarios/fragments/task-490.6-oidc.py
  - checks/integration-access/scenarios/fragments/task-490.6-oidc.cjs
  - checks/integration-access/fixtures/oidc-realm.json
parent_task_id: TASK-490
priority: high
type: task
ordinal: 518000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-access. Contract ID: access.lifecycle.oidc. This packet owns disjoint OIDC case fragments and local provider fixture only: checks/integration-access/scenarios/fragments/task-490.6-oidc.py, task-490.6-oidc.cjs, and checks/integration-access/fixtures/oidc-realm.json. The coordinator owns lifecycle-oidc.nix, Python composition, browserSpecs mapping, and owner registration. Do not add a derivation per packet.

Prove a real browser authorization-code flow against the local OIDC provider with the normal production module configuration `auth_mode = "oidc"`. Assert callback, persisted CF session/external identity, bootstrap-admin and admin-created group-to-role/environment mapping, unmapped-user behavior where accepted authority defines it, and sign-out invalidation. The OIDC scenario is separate from local bootstrap because auth mode and provider/redirect machine configuration are incompatible. Do not depend on local-bootstrap VM state.

Requires TASK-490.7 provider/browser-trusted origin and passing TASK-490.1 and TASK-490.25/.26. Read OIDC server handlers/routes and accepted role-mapping requirements; as-built docs are not D1 authority. No production, shared helper, scenario composition, owner registration, CI, or docs edits. The coordinator records the actual composed VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The coordinator composes OIDC fragments in lifecycle-oidc; no packet-specific derivation is added
- [ ] #2 Browser authorization-code sign-in creates a CF session and external identity
- [ ] #3 Approved group mappings yield expected role and environment scope; unmapped users follow accepted requirements
- [ ] #4 Sign-out invalidates the OIDC session
- [ ] #5 Normal module auth_mode=oidc is used with no test-only override
- [ ] #6 The coordinator records actual VM revision, command, outcome and evidence; legacy oidc-auth assertions remain until cutover
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces prior paths and registration model. Actual foundation commit unresolved; foundation blocked by two existing Rust package tests, no VM execution. Remain Backlog; actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Merge reconciliation retains source-packet specificity for future OIDC proof: persist user_sessions.auth_source=oidc and the external identity binding; distinguish bootstrap-admin mappings from mappings created through Admin; a legacy redirect-only test does not prove callback/session/mapping. External-provider variants and token-refresh timing remain non-goals. The source packet proposed no role for an unmapped user; keep that proposal distinguishable from accepted mapping policy and confirm authority before choosing an expectation. Canonical fragment-only/coordinator composition boundaries supersede obsolete exclusive lifecycle-oidc scenario-file ownership. This future task remains Backlog, not a foundation merge blocker or a claimed pass.
<!-- SECTION:NOTES:END -->
