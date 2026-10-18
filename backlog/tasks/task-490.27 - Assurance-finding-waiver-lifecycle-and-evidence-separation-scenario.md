---
id: TASK-490.27
title: 'Assurance: finding-waiver lifecycle and evidence separation scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-18 02:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.27-waiver.py
  - checks/integration-assurance/scenarios/fragments/task-490.27-waiver.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 540000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.finding-waiver. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.27-waiver.py and task-490.27-waiver.cjs plus optional owner-local fixtures. Coordinator owns evidence-remediation-lifecycle.nix, Python composition, browserSpecs mapping and owner registration. No NixOS derivation is assigned to this packet.

Compose after an actual finding workflow earlier in the same fresh Assurance VM; reuse generated finding identities through receipts. Never read findings, output rows or receipts from TASK-490.16's separate derivation and never seed findings or waiver states. Assert only waiver transitions supported by accepted authority; a waiver remains distinct from remediation and Current evidence, and unauthorized actions have no side effect. If exact waiver authority/UI states are not approved, retain the case as blocked: decision/design gap.

The same scenario composes TASK-490.16/.17/.18/.19 evidence producers and TASK-490.20/.21/.28/.29 remediation cases, then exports with .22. Writable paths are the named waiver fragments and owner-local fixtures only. Production, shared helpers, scenario composition, owner registration, CI and docs are read-only. Foundation and TASK-490.25/.26 must pass before dispatch; the coordinator records the one composed VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real scan-produced finding earlier in the Assurance scenario is used; waiver state is never seeded
- [ ] #2 Only approved waiver request/review/revoke/expiry transitions are asserted; unresolved authority remains blocked
- [ ] #3 Accepting a waiver never marks the finding remediated or changes Current scan evidence
- [ ] #4 Unauthorized waiver actions are rejected with no side effect
- [ ] #5 The coordinator composes this fragment in evidence-remediation-lifecycle; no packet-specific VM derivation is added
- [ ] #6 The coordinator records the actual Assurance VM revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Isolation clarification: TASK-490.16 is a code/setup dependency only; its VM, scan rows, receipts, or outputs cannot be reused. This scenario must produce the real scan finding it needs in its own fresh VM as setup, without asserting the complete scan contract a second time and without seeding the finding/waiver. Remain Backlog until a passing/tested foundation dependency and accepted waiver authority exist.
<!-- SECTION:NOTES:END -->
