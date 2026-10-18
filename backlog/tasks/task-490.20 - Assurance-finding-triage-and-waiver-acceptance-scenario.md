---
id: TASK-490.20
title: 'Assurance: real finding triage and batch-triage persistence scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-18 02:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.20-triage.py
  - checks/integration-assurance/scenarios/fragments/task-490.20-triage.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 532000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.triage. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.20-triage.py and task-490.20-triage.cjs. Coordinator owns the single evidence-remediation-lifecycle.nix composition, Python entrypoint, browserSpecs mapping, and Assurance registration. This packet does not define a NixOS derivation.

Compose triage cases after a real scan/import/policy workflow earlier in the same fresh Assurance VM. Reuse real finding IDs through receipts; never use another derivation's DB/receipts, and never seed findings or triage results. Assert single and batch triage actor/scope/status/justification; unselected findings stay unchanged; unauthorized Viewer writes do not change rows; browser navigation returns to exact finding/system identity. Preserve approved-authority blockers.

The shared Assurance scenario also composes TASK-490.16/.17/.18/.19 producers, .21 plan, .27 waiver, .28 acceptance, .29 verification and .22 exports. All use one controller/target/service configuration and actual evidence chain; do not create per-packet VM runs. Writable files remain the named triage fragments and optional owner-local fixtures only. Production, shared lib, scenario composition, registration, CI and docs are read-only. Foundation and TASK-490.25/.26 must pass before dispatch; coordinator records the one composed VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Real scan-produced findings earlier in the Assurance scenario are used; triage results are never seeded
- [ ] #2 Single and batch triage persist actor, scope, status, and justification for exactly the selected authored findings; unselected findings remain unchanged
- [ ] #3 Unauthorized Viewer triage is rejected without a database side effect
- [ ] #4 Browser navigation reaches the exact finding/system identity and displays persisted triage state
- [ ] #5 The coordinator composes the fragment in evidence-remediation-lifecycle; no packet-specific VM derivation is added
- [ ] #6 The coordinator's real Assurance VM run records tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands. Additional dispatch blocker: TASK-490.20/.21 need finer splitting of triage versus policy waiver/acceptance versus closure/verification. Scope preserved, not expanded or split now. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Packet decomposition correction 2026-10-17: this task now covers triage/batch triage only. Do not test or own finding waiver or acceptance-register lifecycle; those are separate TASK-490.27/.28 with distinct starting state/authority. Every run is a fresh VM/database, so TASK-490.16's previous run output cannot be reused; execute a real scan as setup in this scenario using the accepted setup flow, never seed findings. Foundation scenario has executed and failed on TASK-490.25/.26; no ready foundation dependency commit exists. Remain Backlog.
<!-- SECTION:NOTES:END -->
