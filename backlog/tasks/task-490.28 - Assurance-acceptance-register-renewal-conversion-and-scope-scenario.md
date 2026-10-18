---
id: TASK-490.28
title: 'Assurance: acceptance-register renewal, conversion, and scope scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-18 02:38'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.27
  - TASK-490.21
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.py
  - checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 541000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.acceptance-register. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.28-acceptance.py and task-490.28-acceptance.cjs. Coordinator owns the single evidence-remediation-lifecycle.nix composition, Python entrypoint, browserSpecs mapping, and Assurance registration. Do not create a packet-specific NixOS derivation.

Compose after an actual source-owned acceptance is produced earlier in the same fresh Assurance VM by an approved waiver/plan flow. Use receipts for exact source and acceptance IDs; never seed records or consume live records from another scenario. Prove list/filter source identity and scope, renew only the selected record, convert to POA&M with exact source/evidence identity, preserve the other record, reject unauthorized writes, and retain browser return navigation. If authority for source or transitions is not approved, keep that assertion blocked: decision.

The same scenario composes evidence producers (.16-.19), triage/plan/waiver (.20/.21/.27), verification (.29), and exports (.22) on one controller/target/service configuration. Writable paths are named fragments and optional owner-local fixtures only. Production, shared helpers, scenario composition, registration, CI and docs are read-only. Foundation and TASK-490.25/.26 must pass before dispatch; coordinator records the single Assurance VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A real source-owned acceptance is created earlier in the Assurance scenario; no acceptance row is seeded
- [ ] #2 List/filter results and source identity match independent authored facts or producer receipts
- [ ] #3 Renew affects only the selected acceptance and persists approved fields
- [ ] #4 Convert creates a POA&M with the exact source/evidence identity and does not alter another record
- [ ] #5 Unauthorized actions fail without side effects and browser return navigation preserves source identity
- [ ] #6 The coordinator composes the fragment in evidence-remediation-lifecycle with no separate packet derivation and records the VM evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Isolation clarification: TASK-490.27/.21 can provide source semantics and code prerequisites only. Their runtime acceptance/waiver/POA&M rows are not shared. In this scenario, generate an actual source-owned acceptance in the same fresh VM through its supported producer flow before testing register list/renew/convert. Do not seed acceptance records. Remain Backlog until a tested foundation and accepted source/renew/convert authority exist.
<!-- SECTION:NOTES:END -->
