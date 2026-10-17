---
id: TASK-490.18
title: >-
  Assurance: policy versions, assignments, and enforce versus report-only
  admission scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-17 17:27'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
parent_task_id: TASK-490
priority: high
type: task
ordinal: 530000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.enforcement. Exclusive writes: checks/integration-assurance/scenarios/policy-enforcement.{nix,py,cjs}. Existing 19-20e/20ab2 prove policy persistence but not real Enforce versus ReportOnly admission.

Scope: admin creates/publishes version with one or two authored-fixture-violable composite rule kinds (e.g. packages_absent and nixos_option), assigns versioned environment/system scope. Evaluating violating commit produces expected failed evidence. Enforce blocks real deployment and leaves target unchanged; ReportOnly records violation and permits deployment. New policy version never rewrites old-version evidence. Non-goals: every rule kind (Rust/unit tests), STIG import (490.19). TASK-490 test-only restrictions apply.

Organization: native checks/integration-assurance/default.nix uses { lib, inputs, pkgs, ... }, explicit coordinator-owned registrations and normal pkgs.linkFarm aggregate of ALL registered gates. Shared helpers lib/integration-test via lib.crystal-forge.makeIntegrationTestHarness { pkgs, inputs }; scripts/fixtures/assets owner-local. No root tests/, legacyPackages.integration, mkOwner or custom discovery. Coordinator owns default.nix/shared lib/CI/docs; child files stay disjoint, integration requires explicit registration. Meaningful WIP may be discoverable; only Access exists now; add other owners only with meaningful scenarios, no placeholders.

Prerequisite: TASK-490.14 integrated. Read cf-server models/deployment_policies.rs DeploymentPolicy/CompositeRuleKind/AssignedPolicyEnforcementMode, compliance/resolver.rs AssignmentMode/AssignmentTarget, api/compliance.rs assignments/policy-version publish/trust, deployment_policies.rs and deployments.rs; docs/knowledge/compliance/assignments-and-report-only-enforcement.md; PoliciesView.jsx/PolicyEditor.jsx. As-built D1 is evidence, not accepted authority; uncertain cases blocked pending owner decision.

Candidate, NOT verified: nix build .#checks.x86_64-linux.integration-assurance.scenarios.policy-enforcement -L; append .run for evidence; aggregate nix build .#checks.x86_64-linux.integration-assurance -L. Completion requires explicit registration, real VM execution and revision/commands/outcomes/evidence notes, independent fixture facts and retained correct failing assertions with linked Backlog product bugs. Escalate helper/authority gaps, no product changes.

READINESS: actual foundation dependency commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s), implemented interfaces/fixtures/registration and verified commands required before To Do. No delegation or executed slice is claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Published policy versions and environment/system assignments resolve to the expected effective policies
- [ ] #2 Enforce blocks a violating deployment and the real target is unchanged
- [ ] #3 ReportOnly records the violation without blocking
- [ ] #4 Evidence remains bound to the policy version that produced it
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local paths, shared lib/integration-test and explicit coordinator registration. Foundation commit unresolved; two existing Rust package tests block foundation; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
