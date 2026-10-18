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
Current packet boundary (2026-10-18): this task owns POA&M verification Python/browser fragments only. The coordinator owns the shared `evidence-remediation-lifecycle.nix`, composition and registration. Use the plan and actual remediation evidence created earlier in that same fresh Assurance VM; no per-packet derivation, seeded verification result, or other VM state.

Owning check: integration-assurance. Contract ID: assurance.remediation.poam-verification. Exclusive scenario writes: checks/integration-assurance/scenarios/remediation-poam-verification.{nix,py,cjs}.

WHY: verification and closure begin with an existing POA&M and have higher authority requirements than triage or plan entry. Legacy 29g-29m and canonical POA&M lifecycle use SQL-seeded failure evidence; close/verify must instead follow real Current evidence after remediation.

OBJECTIVE: in this scenario's own fresh VM, generate real failed evidence, create/link a POA&M through the real workflow, and test only verification/closure/reopen. Do not reuse TASK-490.21/.14/.16 runtime rows, outputs, generations, or VMs. Non-goals: plan-entry assertions, finding triage, waiver/acceptance, exports, production changes.

SCENARIO: after real target/scanner/evaluation state changes Current evidence to passing through remediation, authorized verifier requests verification; expected decision/history match an accepted owner contract. Verification before Current evidence changes is denied with no row mutation. Successful verification permits closure; reopen retains original finding/evidence identities and performs the documented transition; stale revisions cause no mutation. UI distinguishes Current from historical/built evidence and returns to the exact finding/system/plan. Expected evidence comes independently from actual target state and authored scanner data.

AUTHORITY: read docs/design/CrystalForge/docs/crystal-forge-systems-design/cve-poam-evidence-continuity-design-spec.md and verify status/approved sections; read docs/knowledge/poam/* as as-built only. If role authority, success criteria, transitions or UI state lack accepted authority, mark blocked: decision/design gap and escalate; do not infer from PoamStatus. Read cf-server handlers/api/poam.rs (`verify`, `close`, `reopen`, transitions/history), models/poam.rs, evidence queries and routes; web UI PoA&M/evidence views; design PoamsView.jsx/PoamViews.jsx/EvidenceDrawer.jsx for approved presentation only.

HARNESS: scenario creates its own actual POA&M and evidence in its fresh VM; no direct SQL seeding of the verification result or successful evidence. Use `lib.crystal-forge.makeIntegrationTestHarness {pkgs,inputs}` and documented driver/browser helpers. Coordinator owns registration and shared library.

WRITABLE (exclusive): checks/integration-assurance/scenarios/remediation-poam-verification.{nix,py,cjs} and checks/integration-assurance/fixtures/poam-verification-* only. Production, schemas, design source, shared lib, registration and CI are read-only.

DEPENDENCIES/READINESS: TASK-490.21/.14 provide code/role knowledge only; actual output must be regenerated. Requires a tested foundation and accepted verification authority. Current Access VM run fails before migrations due TASK-490.25/.26. Remain Backlog.

FOCUSED COMMAND (not verified): `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-poam-verification -L`; `.run` for evidence only. Coordinator runs `nix build .#checks.x86_64-linux.integration-assurance -L`. One VM at a time pending measured capacity.

Completion: premature and valid verification assert actual evidence and no-side-effects; closure/reopen and stale revision assertions preserve identity; explicit coordinator registration and complete owner inclusion; real VM command, revision, outcome and evidence recorded. Keep correct failures and link product defects. Remain Backlog until authority and tested prerequisite are resolved.
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
Isolation clarification: dependent scenario tasks provide code/semantics only. This scenario reconstructs all required runtime data in its own fresh VM. Remain Backlog pending tested foundation, producer prerequisites and accepted verification/closure authority.
<!-- SECTION:NOTES:END -->
