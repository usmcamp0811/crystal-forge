---
id: TASK-490.20
title: 'Assurance: finding triage and waiver/acceptance scenario'
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
Owning check: integration-assurance. Contract ID: assurance.remediation.triage. Exclusive writes currently scoped to checks/integration-assurance/scenarios/remediation-triage.{nix,py,cjs}. Existing CVE triage/waiver proofs (16-cves, 12h) are mocked or Rust authorization tests.

Preserved scope: start from real TASK-490.16 scan evidence. System/fleet triage with justification persists actor and exact scope; batch affects exactly selected findings. Finding waiver creation/review Accepted|Rejected/expiry/revocation follows WaiverDecision and never marks remediation. Acceptance register renew/convert persists. Viewer triage returns 403 with no change. Non-goals: POA&M lifecycle (490.21), exports (490.22). TASK-490 test-only scope applies.

NOT DISPATCH READY: TASK-490.20/.21 still need finer splitting between finding triage, policy waiver/acceptance and closure/verification. The current separation from POA&M is insufficient. Record the refinement blocker now; do not expand scope or create new slices in this correction. TASK-490.16 integration alone does not make this task ready.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, coordinator-owned explicit scenarios and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; child files stay disjoint and integration requires explicit registration. Meaningful WIP may be discoverable; only Access exists now, other owners require meaningful scenarios, no placeholders.

Read-only source: cf-server api/cves.rs triage/batch/justification, poam.rs finding-waivers, acceptance_register.rs, models/poam.rs WaiverDecision; owner-approved docs/design/CrystalForge cve-poam-evidence-continuity-design-spec.md; docs/knowledge/cves/fleet-cve-triage*; CveBatchTriage.jsx/PoamViews.jsx. As-built D1 is evidence, not accepted authority; uncertain policy/waiver semantics blocked pending owner decision.

Candidate only, NOT verified or dispatch authorization: nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-triage -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-assurance -L. Final packet boundaries/commands must be reverified after refinement. Completion requires explicit registration, real VM execution and revision/commands/outcomes/evidence; independent expectations and retained correct failing assertions with linked Backlog bugs.

READINESS: foundation dependency commit unresolved; two existing Rust package tests block foundation; no VM execution. Remain Backlog. Finer splitting, actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands are required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Triage and batch triage persist with actor and exact scope
- [ ] #2 Waiver decisions follow their lifecycle and never mark remediation
- [ ] #3 Acceptances renew and convert per the register
- [ ] #4 Unauthorized triage is rejected with no change
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands. Additional dispatch blocker: TASK-490.20/.21 need finer splitting of triage versus policy waiver/acceptance versus closure/verification. Scope preserved, not expanded or split now. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
