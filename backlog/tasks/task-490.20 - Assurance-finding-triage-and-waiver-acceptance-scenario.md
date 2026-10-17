---
id: TASK-490.20
title: 'Assurance: real finding triage and batch-triage persistence scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-17 17:28'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 532000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.triage. Exclusive scenario writes: checks/integration-assurance/scenarios/remediation-triage.{nix,py,cjs}.

Start from real scan-produced findings created as setup in this scenario's own fresh VM. Single/fleet triage and batch triage persist actor, exact scope, status and justification; the unselected findings remain unchanged. Viewer is denied with no row change. Browser navigation reaches the exact CVE/system identity. No finding waiver or acceptance-register lifecycle here; those are TASK-490.27 and .28. POA&M plan creation is TASK-490.21; verification/closure is TASK-490.29.

Read-only production sources: cf-server api/cves.rs triage/batch/justification, related queries/models and routes; web UI CVE views and triage dialogs. Read the owner-approved cve-poam-evidence-continuity spec only for its stated scope; D1/as-built docs are not accepted authority. Any uncertain behavior is blocked pending an owner decision.

Shared helper `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}` and `checks/integration-assurance/default.nix` registration are coordinator-owned. Writes are restricted to this scenario's files and owner-local triage fixtures. Each run has a fresh database/VM: TASK-490.16 runtime scan rows/receipts cannot be reused. Create scan evidence as real setup in this scenario; do not seed findings.

Candidate command, not verified: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-triage -L`; `.run` for evidence; coordinator owns the full Assurance check. Remain Backlog: foundation VM run fails on TASK-490.25/.26 and exact command/authority remain unverified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Triage and batch triage persist with actor and exact scope
- [ ] #2 Batch triage affects exactly the authored selected set; unselected findings are unchanged
- [ ] #3 Viewer triage is rejected with no database side effect
- [ ] #4 Browser navigation reaches and displays the exact finding/system identity
- [ ] #5 Scenario is registered in and included by the complete Assurance owner
- [ ] #6 Executed in a real VM with exact command, revision, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction and later packet decomposition: triage only; finding waiver TASK-490.27, acceptance register TASK-490.28, POA&M plan TASK-490.21, and verification/closure TASK-490.29. Runtime findings must be generated within this scenario's fresh VM. TASK-490.25/.26 currently block foundation startup; no packet is ready.
<!-- SECTION:NOTES:END -->
