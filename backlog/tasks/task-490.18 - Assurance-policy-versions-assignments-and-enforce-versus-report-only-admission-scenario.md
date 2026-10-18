---
id: TASK-490.18
title: >-
  Assurance: policy versions, assignments, and enforce versus report-only
  admission scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-18 02:38'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.18-policy.py
  - checks/integration-assurance/scenarios/fragments/task-490.18-policy.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 530000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.enforcement. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.18-policy.py and task-490.18-policy.cjs. Coordinator owns the shared evidence-remediation-lifecycle.nix, Python composition, browserSpecs mapping, and owner registration. No per-packet NixOS derivation.

Create/publish authored policy versions and assignments within the same fresh Assurance VM. Produce real violating evaluation/deployment evidence; Enforce blocks deployment without target change, ReportOnly records violation and permits it, and new versions do not rewrite old evidence. Use generated identities through receipts; never reuse another scenario's target/DB/output or seed the result under test. Later .21/.29 cases may use the actual policy failures in this same VM.

Writable paths are named fragments only. Production, helpers, scenario composition, owner registration, CI and docs are read-only. Require TASK-490.7 target role and accepted authority. Foundation and TASK-490.25/.26 must pass before dispatch; coordinator records one Assurance VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Published policy versions and assignments resolve to authored expected effective policies
- [ ] #2 Enforce blocks a real violating deployment and leaves target unchanged
- [ ] #3 ReportOnly records the violation and permits the real deployment
- [ ] #4 Evidence remains bound to the policy version that produced it
- [ ] #5 The coordinator composes this fragment with remediation in evidence-remediation-lifecycle; no packet derivation is added
- [ ] #6 The coordinator records tested revision, exact command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths, shared lib/integration-test and explicit coordinator registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Scenario isolation correction: no target/evaluation/deployment output from TASK-490.14 or another scenario is reusable. This scenario must author/publish/assign its own policy and create its own violating evaluation and target state in a fresh VM; the evaluated failure is real setup, not a seeded result. Foundation VM fails on TASK-490.25/.26; remain Backlog.
<!-- SECTION:NOTES:END -->
