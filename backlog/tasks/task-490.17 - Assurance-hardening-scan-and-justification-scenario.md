---
id: TASK-490.17
title: 'Assurance: hardening scan and justification scenario'
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
  - checks/integration-assurance/scenarios/fragments/task-490.17-hardening.py
  - checks/integration-assurance/scenarios/fragments/task-490.17-hardening.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 529000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.scanning.hardening. This packet owns only checks/integration-assurance/scenarios/fragments/task-490.17-hardening.py and task-490.17-hardening.cjs. The coordinator owns evidence-remediation-lifecycle.nix, Python composition, browserSpecs mapping and Assurance registration; no packet-specific derivation.

Provision targetA and authored systemd service settings in the same fresh Assurance VM. Run the real hardening scan and compare findings to independently authored unit facts; persist justification and prove a rescan after a real target change updates only the changed service. Use receipts for the real target identity. Do not use another scenario's target, DB or output.

Compose with CVE, policy, import, triage, waiver, acceptance, POA&M, verification and export fragments in the shared Assurance VM. Writable paths are these fragments and optional owner-local fixtures. Production code, shared helpers, composition, registration, CI and docs are read-only. Requires TASK-490.7 target role and accepted authority; foundation and TASK-490.25/.26 must pass before dispatch. Coordinator records one VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Hardening findings match independently authored per-service unit settings on real targetA
- [ ] #2 Justifications persist through reload
- [ ] #3 After a real target service change, rescan updates only the changed service
- [ ] #4 The coordinator composes this fragment into evidence-remediation-lifecycle; no packet derivation is added
- [ ] #5 The coordinator records the actual Assurance VM tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Scenario isolation correction: TASK-490.14 target/DB/output cannot be reused. Provision the real target and authored systemd service facts inside this scenario's own fresh VM, then scan. Reuse only immutable fixture definitions. Foundation VM fails on TASK-490.25/.26; remain Backlog.
<!-- SECTION:NOTES:END -->
