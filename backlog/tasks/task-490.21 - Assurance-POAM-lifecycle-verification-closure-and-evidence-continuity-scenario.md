---
id: TASK-490.21
title: 'Assurance: create and maintain POA&M plans from real failed evidence'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 02:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
  - TASK-490.18
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.21-poam-plan.py
  - checks/integration-assurance/scenarios/fragments/task-490.21-poam-plan.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 533000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam-plan. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.21-poam-plan.py and task-490.21-poam-plan.cjs. The coordinator owns the shared evidence-remediation-lifecycle.nix composition, Python entrypoint, browserSpecs mapping and Assurance registration; this packet does not create a NixOS derivation.

Compose after real failed CVE/policy evidence produced earlier in the same fresh Assurance VM. Create/link POA&M plans only through the real workflow; do not seed failed results, findings, plans, or identities. Prove exact linked evidence identity; notes, milestones, and approved nonterminal state edits persist through reload. Failed create and stale revision have no partial effect where accepted authority specifies the state. Verification/closure/reopen remains the .29 fragment in this same scenario and follows actual remediation/current evidence. Use receipts for generated finding/plan identities.

The shared scenario composes TASK-490.16/.17/.18/.19 producers, .20 triage, .27 waiver, .28 acceptance, .29 verification, and .22 exports with a single controller/target/service configuration and evidence chain. Writable files are the named plan fragments and optional owner-local fixtures only. Production, helpers, scenario composition, registration, CI, and docs are read-only. Foundation and TASK-490.25/.26 must pass before dispatch; the coordinator records the one composed VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real failed finding produced earlier in the Assurance scenario is used; no failed result is seeded
- [ ] #2 POA&M links preserve exact finding/evidence identity
- [ ] #3 Milestones, notes, and approved plan transitions persist and survive reload
- [ ] #4 Stale updates and failed creates have no partial effects where the accepted requirement defines them
- [ ] #5 The coordinator composes this plan fragment with TASK-490.29 verification/closure in evidence-remediation-lifecycle; no packet-specific VM derivation is added
- [ ] #6 The coordinator records the real Assurance VM revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands. Additional blocker: TASK-490.20/.21 need finer splitting of triage versus policy waiver/acceptance versus verification/closure; scope preserved, no new slices or scope expansion now. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Packet decomposition/isolation correction 2026-10-17: this task covers POA&M create/link/milestones/notes and approved plan-state edits only. Verification/closure/reopen is TASK-490.29, a separate starting state and authority. No TASK-490.16/.18 VM/database/output is reusable: this scenario must create its own real failed scan/evaluation evidence through actual setup, without seeding the failure under test. Foundation VM run fails on TASK-490.25/.26; no ready dependency commit exists. Remain Backlog.
<!-- SECTION:NOTES:END -->
