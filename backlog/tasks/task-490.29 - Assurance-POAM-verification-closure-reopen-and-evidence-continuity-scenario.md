---
id: TASK-490.29
title: >-
  Assurance: POA&M verification, closure, reopen, and evidence continuity
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-18 02:38'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.21
  - TASK-490.14
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.29-verification.py
  - >-
    checks/integration-assurance/scenarios/fragments/task-490.29-verification.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 542000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam-verification. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.29-verification.py and task-490.29-verification.cjs. Coordinator owns evidence-remediation-lifecycle.nix, Python composition, browserSpecs mapping and Assurance registration. No separate NixOS derivation is assigned.

Use a POA&M and failed evidence created earlier in the same fresh Assurance VM by the .21/.16/.18 workflows; use receipts for all generated identities. Prove premature verification is denied with no mutation; after actual remediation changes Current evidence, authorized verification/closure/reopen preserves finding/evidence identity and rejects stale writes. Current-versus-historical assertions must use independent target/scanner observations, not CF response data. Do not seed successful evidence, verification outcomes or runtime rows. Any unclear authority/role/state stays blocked: decision/design gap.

This fragment composes with .16-.21, .27, .28 and .22 in one controller/target/service configuration. Do not consume another derivation's VM, rows, receipts, or generations. Writable paths are named fragments and optional owner-local fixtures. Production, shared helpers, composition, registration, CI and docs are read-only. Requires the tested foundation, TASK-490.25/.26, TASK-490.7 roles and accepted verification authority; coordinator records actual VM outcome/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Premature verification before Current evidence changes is rejected with no state mutation
- [ ] #2 After real remediation/current evidence changes, verification succeeds only under approved role and evidence rules
- [ ] #3 Closure and reopen preserve exact finding/evidence identity; stale revision has no side effect
- [ ] #4 UI distinguishes Current from historical evidence and returns to the exact approved identity
- [ ] #5 The coordinator composes this fragment with .21 plan cases in evidence-remediation-lifecycle; no packet derivation is added
- [ ] #6 The coordinator records tested revision, command, outcome and evidence for the Assurance scenario
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Isolation clarification: TASK-490.21/.14/.16 outputs, rows, generations, and VMs cannot be consumed here. In this scenario's own fresh VM, generate failed evidence through a real evaluation/scan, create the POA&M through the real workflow, and perform the real remediation/Current evidence transition before asserting verification/closure. Reuse code and immutable facts only, never runtime state. Remain Backlog until accepted verification/closure authority and a tested foundation/dependency commit exist.
<!-- SECTION:NOTES:END -->
