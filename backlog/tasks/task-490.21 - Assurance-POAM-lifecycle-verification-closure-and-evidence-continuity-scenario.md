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
Owning check: integration-assurance. Contract ID: assurance.remediation.poam-plan. Exclusive scenario writes: checks/integration-assurance/scenarios/remediation-poam-plan.{nix,py,cjs}. Legacy 29g-29m and canonical POA&M lifecycle start from SQL-seeded FAIL evidence; this scenario must generate real failed scan/policy evidence during setup.

Scope: POA&M create/link/milestones/notes and accepted non-terminal plan edits only. Verification, closure and reopen are TASK-490.29; triage is TASK-490.20; finding waiver and acceptance register are TASK-490.27/.28. Task dependencies provide implementation knowledge only; no runtime records or outputs are shared across fresh scenario VMs.

READINESS: Backlog. The foundation scenario has now executed but fails before migrations on TASK-490.25/.26. No product fix is included. Accepted plan-state transitions and authority must be read from owner-approved sources; uncertain behavior stays blocked.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator registrations and normal pkgs.linkFarm aggregate of ALL registered gates. Helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; explicit registration of disjoint child files required. Meaningful WIP may be discoverable; only Access exists now, other owners require meaningful scenarios, no placeholders.

Read-only source: cf-server api/poam.rs transition/verify/close/reopen/milestones/links/rollups, models/poam.rs PoamStatus/PoamRisk; owner-approved docs/design/CrystalForge cve-poam-evidence-continuity-design-spec.md, docs/knowledge/poam/*; PoamsView.jsx/PoamViews.jsx/PoamRegisterParts.jsx. As-built D1 is evidence, not accepted authority; uncertain lifecycle requirements blocked pending owner decision.

Candidate only, NOT verified or dispatch authorization: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.remediation-poam-plan -L`; append `.run` for evidence; coordinator runs the full owner. This scenario must create its failed evidence in its own VM. Completion requires explicit registration, real VM execution, exact revision/commands/outcomes/evidence, independent expectations and retained correct failing assertions with linked Backlog bugs.

Dependencies: TASK-490.16/.18 provide source/implementation context but not runtime outputs; generate actual evidence in this test. The combined TASK-490.1 branch has not passed its Access startup scenario. Remain Backlog until tested prerequisites and focused command are verified.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 POA&Ms are created from real failed evidence, never seeded results
- [ ] #2 Lifecycle transitions, milestones, and links persist with exact finding identity
- [ ] #3 Failed plan creation and stale revision are rejected without partial or persisted side effects
- [ ] #4 UI and stored links retain exact finding/plan identity through reload
- [ ] #5 Scenario is explicitly registered in the complete Assurance owner
- [ ] #6 Executed in a real VM with exact command, revision, outcome and evidence paths
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction replaces old layout/commands. Additional blocker: TASK-490.20/.21 need finer splitting of triage versus policy waiver/acceptance versus verification/closure; scope preserved, no new slices or scope expansion now. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

2026-10-17 superseding packet correction: TASK-490.21 owns POA&M plan creation/linking/milestones/notes and approved non-terminal edits only. TASK-490.29 owns verification/closure/reopen. Finding triage is TASK-490.20; finding waiver is TASK-490.27; acceptance-register renew/convert is TASK-490.28. Runtime evidence/rows from TASK-490.16/.18/other runs cannot be reused; generate real prerequisite evidence in this scenario's own fresh VM. Foundation VM has now executed and fails on product defects TASK-490.25/.26; all child tasks remain Backlog.
<!-- SECTION:NOTES:END -->
