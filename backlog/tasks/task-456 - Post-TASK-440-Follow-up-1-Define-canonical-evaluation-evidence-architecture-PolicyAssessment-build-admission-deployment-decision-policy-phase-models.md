---
id: TASK-456
title: >-
  Post-TASK-440 Follow-up 1: Define canonical evaluation evidence architecture
  (PolicyAssessment, build-admission, deployment-decision, policy-phase models)
status: To Do
assignee: []
created_date: '2026-09-06 23:07'
labels: []
dependencies:
  - TASK-440
documentation:
  - >-
    backlog/docs/doc-24 -
    Crystal-Forge-Evaluation-Evidence-Build-Admission-and-Deployment-Gating-Architecture.md
priority: high
type: spike
ordinal: 465000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Crystal Forge currently has overlapping mechanisms that independently answer related questions about a NixOS configuration: the fast `nix-eval-jobs` evaluator, deployment-policy evaluation (`cfAgentEnabled`, required packages, arbitrary config expressions), the TASK-440 Config Inspector, and Flake Explorer. Build/deployment eligibility is partly represented by overlapping derived fields (`derivations.cf_agent_enabled`, `derivations.policy_requirements_met`, `derivations.policy_results`) with no single reproducible source of truth.

This is the first of five follow-up tasks from the "Crystal Forge Evaluation, Evidence, Build Admission, and Deployment Gating Architecture" design document (doc-24). It corresponds to "Follow-up Task 1 — Canonical evaluation evidence architecture" (design §21). It is a design/spike task: the deliverable is an inventory, a set of documented data/decision models, and a migration compatibility plan. It does not change runtime evaluator, policy, scheduler, or build behavior.

## Non-goals

- Do not modify the fast evaluator, Config Inspector runner, policy engine execution, scheduler, or build admission behavior in this task.
- Do not remove or repurpose `derivations.cf_agent_enabled`, `policy_requirements_met`, or `policy_results` in this task.
- Do not implement the Config-artifact-backed policy migration (Follow-up Task 2), build-admission modes (Follow-up Task 3), evaluator reduction (Follow-up Task 4), or artifact/job infrastructure unification (Follow-up Task 5); this task only produces the design/contract these depend on.

## Scope

1. Inventory every current evaluation path in the repository that reads or evaluates configuration facts used for build/deployment/policy decisions (fast evaluator outputs, deployment policy evaluation, TASK-440 Config Inspector/ConfigArtifactV2, Flake Explorer, CVE scanning, build/closure inspection).
2. Classify each inventoried fact/path into one of: fast derivation/identity fact; Config artifact fact; build/closure fact; CVE fact; runtime/deployment fact; arbitrary Nix escape-hatch fact.
3. Define, as a written architecture/design contract (in a Backlog document or `docs/` specification, per repository documentation conventions):
   - the evidence identity/version contract (how an evidence artifact is identified, versioned, and bound to a policy assessment);
   - the `PolicyAssessment` model (inputs, versioning, reproducibility contract);
   - the build-admission decision model (states/reasons per design §11, e.g. `held: cf_agent_disabled`, `eligible: policy_verified`);
   - the deployment-decision model (fail-closed semantics per design §19-20);
   - the policy-phase model (pre-build, build-priority, pre-deploy per design §14);
   - a migration compatibility plan for existing derivation policy fields (`cf_agent_enabled`, `policy_requirements_met`, `policy_results`) describing how they are superseded without breaking currently deployed agents/builders or in-flight deployments.
4. Explicitly reconcile the proposed models against TASK-440's shipped `ConfigArtifactV2` shape and V2 selector/reader design so Follow-up Tasks 2-5 have an unambiguous starting contract.

## Verification plan

- Documentation-only task: verification is peer/architectural review of the produced inventory and model documents against the design document's invariants (doc-24 §3, §13, §14, §20, §24) and against TASK-440's actual shipped `ConfigArtifactV2`/reader implementation.
- Confirm no application code, migration, or schema changes were introduced (`git status` / `git diff` limited to documentation).
- Run `nix develop -c cargo check --workspace` (or the narrower package-scoped equivalent) only if any illustrative type stubs are added, to confirm they compile; do not wire them into runtime paths.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 An inventory document lists every current evaluation path/fact source (fast evaluator, deployment policy engine, Config Inspector/ConfigArtifactV2, Flake Explorer, CVE scan, build/closure) with its classification into fast-identity, Config-artifact, build/closure, CVE, runtime/deployment, or Nix-escape-hatch evidence.
- [ ] #2 A documented evidence identity/version contract defines how an evidence artifact is identified, versioned, and referenced by a policy assessment.
- [ ] #3 A documented PolicyAssessment model defines its inputs (evidence artifact identities + exact policy-set/version), reproducibility contract, and its relationship to BuildAdmissionDecision and DeploymentDecision.
- [ ] #4 A documented build-admission decision model enumerates explicit states/reasons (e.g. held/eligible/priority/blocked_deploy classes) consistent with design document §11.
- [ ] #5 A documented deployment-decision model states fail-closed behavior for missing/unsupported/failed required evidence, consistent with design document §19-20.
- [ ] #6 A documented policy-phase model defines pre-build, build-priority, and pre-deploy phases and how a policy declares which phase(s) it applies to.
- [ ] #7 A documented migration compatibility plan explains how derivations.cf_agent_enabled, policy_requirements_met, and policy_results are superseded or retained as compatibility fields without breaking currently deployed agents/builders.
- [ ] #8 The produced models are explicitly reconciled against TASK-440's shipped ConfigArtifactV2 shape and V2 selector/reader design, with any incompatibilities called out.
- [ ] #9 No runtime evaluator, policy engine, scheduler, migration, or schema behavior is changed by this task.
<!-- AC:END -->
