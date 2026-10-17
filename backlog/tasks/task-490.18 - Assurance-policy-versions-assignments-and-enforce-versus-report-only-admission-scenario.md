---
id: TASK-490.18
title: >-
  Assurance: policy versions, assignments, and enforce versus report-only
  admission scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
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
Owning check: integration-assurance. Contract ID: assurance.policy.enforcement. Exclusive write path: tests/integration/assurance/scenarios/policy-enforcement.{nix,py,cjs}.

WHY: policy creation steps (19-20e, 20ab2) prove persistence, but no scenario proves that an assigned policy version actually blocks (Enforce) or only records (ReportOnly) a real admission decision.

SCOPE: an admin creates and publishes a policy version (composite rule kinds limited to one or two that the authored fixture can violate, e.g. packages_absent and nixos_option), assigns it to an environment and to one system with versioned scope; evaluation of a violating commit produces the expected failed evidence; with Enforce the deployment is blocked and the target does not change; with ReportOnly the deployment proceeds and the violation is recorded; a new policy version does not rewrite evidence for the previous version. Non-goals: every rule kind (unit/Rust tests own per-kind logic); STIG import (490.19).

READY WHEN: TASK-490.14 is integrated. Source to read then: cf-server models/deployment_policies.rs (DeploymentPolicy, CompositeRuleKind, AssignedPolicyEnforcementMode), compliance/resolver.rs (AssignmentMode, AssignmentTarget), handlers/api/compliance.rs (assignments, policy-versions publish/trust), deployment_policies.rs, deployments.rs; docs/knowledge/compliance/assignments-and-report-only-enforcement.md; design PoliciesView.jsx, PolicyEditor.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Published policy versions and environment/system assignments resolve to the expected effective policies
- [ ] #2 Enforce blocks a violating deployment and the real target is unchanged
- [ ] #3 ReportOnly records the violation without blocking
- [ ] #4 Evidence remains bound to the policy version that produced it
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
