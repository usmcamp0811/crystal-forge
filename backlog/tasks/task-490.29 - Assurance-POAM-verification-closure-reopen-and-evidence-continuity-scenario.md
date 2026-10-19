---
id: TASK-490.29
title: >-
  Assurance: POA&M verification, closure, reopen, and evidence continuity
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-19 02:15'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.21
  - TASK-490.14
modified_files:
  - checks/integration-assurance/scenarios/fragments/verification.py
  - checks/integration-assurance/scenarios/fragments/verification.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 542000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam-verification. Goal: prove verification/closure/reopen after real remediation changes Current evidence. Risk: high (verification authority, evidence and stale writes). Non-goals: plan-entry assertions (.21), triage (.20), waiver/acceptance (.27/.28), exports (.22), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/verification.py
- checks/integration-assurance/scenarios/fragments/verification.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal aggregation. Use actual .21 plans and .16/.18 failed evidence earlier in this fresh Assurance VM/database through receipts. Earlier same-scenario real plans/evidence/generations MAY be consumed; another derivation's live VMs, rows, outputs, generations or receipts MUST NOT be consumed. No packet-specific derivation and no seeded successful evidence/verification outcomes.

Premature verification before Current evidence changes is denied with no mutation. Perform real target/scanner/evaluation remediation that changes Current evidence to passing, then request authorized verification under approved role/evidence rules. Expected decision/history must match accepted owner contract and independent target/scanner observations, not CF responses. Successful verification permits closure; reopen follows its documented transition and retains original finding/evidence identity. Stale revisions have no side effect. UI distinguishes Current from historical/built evidence and returns to the exact approved finding/system/plan identity. Unclear authority, role, lifecycle or UI state remains blocked: decision/design gap.

Dependencies/readiness: TASK-490.21 plan producer and TASK-490.14 remediation/deployment code/interfaces, with .16/.18 real failures and .7 target roles, must be integrated at actual tested revisions. Reuse code/setup and earlier real Assurance cases, not Delivery live state. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26; those old startup failures are not current blockers. Await selection, accepted verification/closure/reopen authority and coordinator ordering. No verification fragment execution is claimed.

Read-only authority/source: docs/design/CrystalForge/docs/crystal-forge-systems-design/cve-poam-evidence-continuity-design-spec.md approved sections; docs/knowledge/poam/* as as-built evidence only; cf-server handlers/api/poam.rs verify/close/reopen/transitions/history, models/poam.rs, evidence queries/routes; web-ui PoA&M/evidence views and approved PoamsView.jsx/PoamViews.jsx/EvidenceDrawer.jsx. Do not infer requirements from PoamStatus. Use documented case/assert/receipt/http/sql_json/run_browser interfaces; production, schemas, shared helpers, composition, registration, CI, docs and design are read-only.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. Preserve correct failures and linked defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Premature verification before Current evidence changes is rejected with no state mutation
- [ ] #2 After actual target/scanner/evaluation remediation changes Current evidence to passing, verification succeeds only under approved role/evidence rules and expected decision/history
- [ ] #3 Closure follows successful verification; closure/reopen preserve original exact finding/evidence identities and documented transitions, while stale revisions have no side effect
- [ ] #4 UI distinguishes independently observed Current from historical/built evidence and returns to the exact approved finding/system/plan identity
- [ ] #5 Coordinator composes named verification fragments after real .21/.16/.18 cases in evidence-remediation-lifecycle through receipts, without seeded success, packet derivation or another derivation's live state
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence; unresolved authority/role/UI states remain blocked
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: foundation bba718be passed. Real plan/failed-evidence producers, target remediation interfaces and accepted verification/closure/reopen authority remain prerequisites for selection and coordinator ordering. No verification run is claimed.

### Historical
The packet split higher-authority verification/closure/reopen from plan entry. Legacy canonical/29g-29m flows used SQL-seeded failure evidence. Earlier isolation notes assumed separate runs and unresolved foundation startup. Current fragments consume earlier real same-scenario plans/evidence through receipts, perform real remediation and prohibit cross-derivation live-state reuse.
<!-- SECTION:NOTES:END -->
