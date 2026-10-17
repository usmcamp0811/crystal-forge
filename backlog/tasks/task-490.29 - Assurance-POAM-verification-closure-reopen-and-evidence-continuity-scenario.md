---
id: TASK-490.29
title: >-
  Assurance: POA&M verification, closure, reopen, and evidence continuity
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 20:31'
updated_date: '2026-10-17 20:33'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.21
  - TASK-490.14
parent_task_id: TASK-490
priority: high
type: task
ordinal: 542000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam-verification. Exclusive scenario writes: checks/integration-assurance/scenarios/remediation-poam-verification.{nix,py,cjs}.

WHY: verification and closure begin with an existing POA&M and have higher authority requirements than triage or plan entry. Legacy 29g-29m and canonical POA&M lifecycle use SQL-seeded failure evidence; close/verify must instead follow real Current evidence after remediation.

OBJECTIVE: starting from a POA&M and exact linked failed evidence produced by TASK-490.21, prove only the verification/closure/reopen boundary. Non-goals: creating initial plans, finding triage, waivers, acceptance renewal, exports, product changes.

SCENARIO: after a real deployment or scan changes Current evidence to passing (producer from TASK-490.14 and/or TASK-490.16), authorized verifier requests verification; expected decision and history match an accepted owner contract. Verification before Current evidence changes is denied and changes no rows. Successful verification permits closure; reopen retains original finding/evidence identities and creates the documented state transition; stale revisions are rejected with no mutation. UI shows Current vs historical/built identity without merging concepts, and returns to the exact finding/system/plan. Expected evidence facts come independently from target state/scanner input, not the CF response.

AUTHORITY: read docs/design/CrystalForge/docs/crystal-forge-systems-design/cve-poam-evidence-continuity-design-spec.md and check its status and exact approved sections, plus any accepted owner decision named there. Read docs/knowledge/poam/* as as-built only. If role authority, verification success criteria, allowed transitions, or UI state lacks an accepted source, mark the dependent case blocked: decision/design gap and escalate; do not infer from PoamStatus enum. Production sources: cf-server handlers/api/poam.rs (`verify`, `close`, `reopen`, transition/history); models/poam.rs (status and revision); queries for evidence identity/currentness; router; web-ui PoA&M detail and system CVE evidence drawers; design PoamsView.jsx/PoamViews.jsx/EvidenceDrawer.jsx only for presentation where approved.

HARNESS: use actual POA&M/evidence receipts from TASK-490.21, target state from TASK-490.14, and scan output from TASK-490.16. No SQL seeding of verification result or successful evidence. Shared helpers exposed by `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}`; coordinator owns helper/default.nix. Scenario API follows coverage map; explicit coordinator registration is mandatory.

WRITABLE (exclusive): checks/integration-assurance/scenarios/remediation-poam-verification.{nix,py,cjs} and checks/integration-assurance/fixtures/poam-verification-* only. All other code, schemas, helpers, design, owner registration, CI are read-only.

DEPENDENCIES/READINESS: TASK-490.21 and .14 plus the actually tested foundation and producer commits, integrated on the overhaul branch. Current foundation VM run fails before migrations due product defects TASK-490.25/.26. Remain Backlog; no dependency commit is yet tested for this scenario.

FOCUSED COMMAND once implemented: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-poam-verification -L`; append `.run` for evidence only. Not verified yet. Coordinator owns `nix build .#checks.x86_64-linux.integration-assurance -L`. One VM run at a time pending measured capacity.

Completion: both premature and valid verification paths assert authoritative evidence and no-side-effects; closure/reopen and stale revision cases assert DB/API/browser identity; all scenarios explicitly registered and included in full owner run; real VM commands, source/dependency commits, result and evidence paths recorded. Preserve correct failing product assertions and link a Backlog defect. Escalate authority/interface questions to coordinator. This task remains Backlog until prerequisites and commands are real and verified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Premature verification before Current evidence changes is rejected with no state mutation
- [ ] #2 Verification after a real remediation/current scan succeeds only under approved role and evidence rules
- [ ] #3 Closure and reopen preserve exact finding/evidence identity; stale revision has no side effect
- [ ] #4 UI shows actual Current versus historical evidence and exact return destination for approved states
- [ ] #5 Scenario is explicitly registered and included in the complete Assurance owner run
- [ ] #6 Real VM execution, revision, exact command, outcome, and evidence paths are recorded
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Isolation clarification: TASK-490.21/.14/.16 outputs, rows, generations, and VMs cannot be consumed here. In this scenario's own fresh VM, generate failed evidence through a real evaluation/scan, create the POA&M through the real workflow, and perform the real remediation/Current evidence transition before asserting verification/closure. Reuse code and immutable facts only, never runtime state. Remain Backlog until accepted verification/closure authority and a tested foundation/dependency commit exist.
<!-- SECTION:NOTES:END -->
