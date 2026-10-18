---
id: TASK-490.18
title: >-
  Assurance: policy versions, assignments, and enforce versus report-only
  admission scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:38'
updated_date: '2026-10-18 13:58'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
modified_files:
  - checks/integration-assurance/scenarios/fragments/task-490.18-policy.py
  - checks/integration-assurance/scenarios/fragments/task-490.18-policy.cjs
parent_task_id: TASK-490
priority: high
type: task
ordinal: 530000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-assurance. Contract ID: assurance.policy.enforcement. Goal: prove real versioned policy admission, not only policy persistence. Risk: high (enforcement/evidence invariants). Non-goals: every composite rule kind, STIG-import assertions (.19), product changes.

Writable paths (only):
- checks/integration-assurance/scenarios/fragments/task-490.18-policy.py
- checks/integration-assurance/scenarios/fragments/task-490.18-policy.cjs

Composition target: checks/integration-assurance/scenarios/evidence-remediation-lifecycle.nix. Coordinator owns Nix/Python composition, browserSpecs and explicit registration/normal owner aggregation. No packet-specific derivation. One fresh Assurance VM/database contains producers and remediation. Earlier real imported policies, targets, evaluations or builds MAY be used through same-scenario receipts. Another derivation's live target/DB/output/receipt state MUST NOT be consumed.

Admin creates/publishes authored versions with one or two fixture-violable composite rule kinds (for example packages_absent and nixos_option), assigning versioned environment/system scope. A real violating commit evaluation produces expected failed evidence. Enforce blocks real deployment with no target change; ReportOnly records the same violation and permits real deployment. Publishing a new version does not rewrite old-version evidence. Use independently authored effective-policy/rule/scope expectations and same-run identities, never seed evaluation/deployment failures or take expected facts from CF results. .21/.29 may consume these actual failures in this same scenario.

Dependencies/readiness: TASK-490.14 and its .12/.10/.7 role/build/publication code/interfaces must provide real evaluation/deployment setup at recorded tested revisions. Recreate runtime setup inside Assurance or consume earlier real cases within that scenario. Foundation bba718befbc183863781656df2df272e5f735d5a on origin/dev 5ae7feab passed with .25/.26; historical startup failures are resolved. This packet awaits selection, concrete deployment/policy setup, accepted policy/enforcement authority and coordinator composition.

Read-only source/authority: cf-server models/deployment_policies.rs DeploymentPolicy/CompositeRuleKind/AssignedPolicyEnforcementMode, compliance/resolver.rs AssignmentMode/AssignmentTarget, api/compliance.rs assignments/publish/trust, deployment_policies.rs and deployments.rs; docs/knowledge/compliance/assignments-and-report-only-enforcement.md; approved PoliciesView.jsx/PolicyEditor.jsx. As-built D1 is evidence, not acceptance authority; uncertain cases remain blocked pending owner decision. Production, shared helpers, composition, registration, CI, docs and design are read-only.

Focused verification after composition: `nix build .#checks.x86_64-linux.integration-assurance.scenarios.evidence-remediation-lifecycle -L`. Coordinator records exact tested revision, command, result and evidence. No policy fragment run is claimed. Retain correct failures and linked defects; escalate helper/authority gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Published authored composite-rule policy versions and versioned environment/system assignments resolve to independently expected effective policies
- [ ] #2 Enforce blocks a real violating deployment and leaves the real target unchanged
- [ ] #3 ReportOnly records the violation and permits the real deployment
- [ ] #4 New policy versions do not rewrite existing evidence; evidence stays bound to its producing policy version and exact scope
- [ ] #5 Coordinator composes named policy fragments with producers/remediation into evidence-remediation-lifecycle; same-scenario receipts may carry real prior results, with no seeded failure, packet-specific derivation or cross-derivation live state
- [ ] #6 Coordinator executes the focused evidence-remediation-lifecycle gate and records exact tested revision, command, result and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Current readiness: foundation bba718be passed. Real evaluation/deployment interfaces from .14/.12/.10/.7, authored rule facts, accepted authority and coordinator composition remain prerequisites. No policy packet execution is claimed.

### Historical
2026-10-17 organization/isolation review recorded package/startup blockers before .25/.26 repair verification. Former separate policy runs could not consume Delivery live state. Current policy cases may consume earlier real Assurance results through receipts; they still cannot import another derivation's runtime state.
<!-- SECTION:NOTES:END -->
