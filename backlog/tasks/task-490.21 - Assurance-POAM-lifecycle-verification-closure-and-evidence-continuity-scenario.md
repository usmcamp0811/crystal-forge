---
id: TASK-490.21
title: >-
  Assurance: POA&M lifecycle verification, closure, and evidence continuity
  scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-17 17:28'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.16
  - TASK-490.18
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 533000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.remediation.poam. Exclusive writes currently scoped to checks/integration-assurance/scenarios/remediation-poam.{nix,py,cjs}. Legacy 29g-29m and canonical POA&M lifecycle start from SQL-seeded FAIL evidence; results under test must instead originate in real scan/policy evaluation.

Preserved scope: create POA&Ms from real failed policy evidence (TASK-490.18) and CVE findings (TASK-490.16), link compatible findings, persist milestones/notes, traverse PoamStatus lifecycle. Verification succeeds only after real remediating deployment changes Current evidence to passing. Closure/reopen preserves finding identity; reject stale revisions; authored system/bundle rollups stay exact. Non-goals: waiver decisions (490.20), exports (490.22). TASK-490 test-only scope applies.

NOT DISPATCH READY: TASK-490.20/.21 still need finer splitting of finding triage versus policy waiver/acceptance versus verification/closure. Existing separation is not sufficient dispatch preparation. Note blocker without expanding scope or creating new slices now. Integration of 490.16/.18 alone does not make this ready.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator registrations and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; explicit registration of disjoint child files required. Meaningful WIP may be discoverable; only Access exists now, other owners require meaningful scenarios, no placeholders.

Read-only source: cf-server api/poam.rs transition/verify/close/reopen/milestones/links/rollups, models/poam.rs PoamStatus/PoamRisk; owner-approved docs/design/CrystalForge cve-poam-evidence-continuity-design-spec.md, docs/knowledge/poam/*; PoamsView.jsx/PoamViews.jsx/PoamRegisterParts.jsx. As-built D1 is evidence, not accepted authority; uncertain lifecycle requirements blocked pending owner decision.

Candidate only, NOT verified or dispatch authorization: nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-poam -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-assurance -L. Final packet commands/boundaries need revalidation after refinement. Completion requires explicit registration, real VM execution and revision/commands/outcomes/evidence; independent expectations and retained correct failing assertions with linked Backlog bugs.

READINESS: actual foundation commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Finer splitting, actual dependency commit(s), implemented helpers/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 POA&Ms are created from real failed evidence, never seeded results
- [ ] #2 Lifecycle transitions, milestones, and links persist with exact finding identity
- [ ] #3 Verification succeeds only after a real remediating deployment changes Current evidence
- [ ] #4 Closure and reopen preserve identity; stale revisions are rejected
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands. Additional blocker: TASK-490.20/.21 need finer splitting of triage versus policy waiver/acceptance versus verification/closure; scope preserved, no new slices or scope expansion now. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
