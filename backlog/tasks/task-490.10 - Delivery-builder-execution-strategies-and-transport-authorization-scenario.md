---
id: TASK-490.10
title: 'Delivery: builder execution strategies and transport authorization scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-24 23:07'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.7
modified_files:
  - >-
    checks/integration-delivery/scenarios/fragments/task-490.10-build-strategies.py
  - >-
    checks/integration-delivery/scenarios/fragments/task-490.10-build-strategies.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 522000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.build.strategies. This packet owns only builder strategy Python/browser fragments under `checks/integration-delivery/scenarios/fragments/task-490.10-*`. Coordinator owns `delivery-lifecycle.nix`, composition, browserSpecs mapping and owner registration. No packet-specific NixOS derivation.

Within the Delivery lifecycle VM, build real fresh outputs using ServerDerivation on builderA and SourceReEvaluateVerified on builderB. Prove absence before each output, authorized builder completion, job-scoped downloads and rejected wrong/revoked sessions without job mutation. Later scheduling/publication/activation cases may consume same-run outputs and receipts; do not reuse another derivation's output, database or builder session.

Requires TASK-490.7 real builder roles, production code dependencies, accepted authority, and passing foundation/.25/.26. Writable paths are fragments only. Production/shared helper/scenario composition/registration/CI/docs are read-only. Coordinator records the composed Delivery VM result.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Each supported strategy builds a real output on its real builder that was independently absent beforehand
- [ ] #2 Unauthorized/revoked sessions and downloads are rejected without job state change
- [ ] #3 Builds UI shows exact job/builder/output identity
- [ ] #4 The coordinator composes this fragment in delivery-lifecycle; no packet-specific derivation is added
- [ ] #5 Same-run output receipts may feed later Delivery cases; no cross-derivation runtime reuse
- [ ] #6 The coordinator records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths, shared lib/integration-test and explicit coordinator registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Merge reconciliation retains source-packet transport constraints for future acceptance: SourceReEvaluateVerified uses ServerBundledArchive only; both derivation and source-archive downloads require server-issued job authorization; wrong-key or revoked-session completion is rejected without job mutation. Scheduling/cancellation and cache publication remain separately scoped. Preserve canonical same-lifecycle fragments, independent builder identity/stores, absent-before/exact-output checks and Builds UI binding. No future criterion is completed by this metadata resolution.
<!-- SECTION:NOTES:END -->
