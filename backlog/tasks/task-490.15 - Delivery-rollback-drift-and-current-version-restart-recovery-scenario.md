---
id: TASK-490.15
title: 'Delivery: rollback, drift, and current-version restart recovery scenario'
status: In Progress
assignee:
  - openai
created_date: '2026-10-17 16:37'
updated_date: '2026-10-20 05:26'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.14
  - TASK-499
modified_files:
  - checks/integration-delivery/scenarios/fragments/recovery.py
  - checks/integration-delivery/scenarios/fragments/recovery.cjs
parent_task_id: TASK-490
priority: medium
type: task
ordinal: 527000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.activation.recovery. This packet owns only recovery Python/browser fragments at `checks/integration-delivery/scenarios/fragments/recovery.py` and `recovery.cjs`. Coordinator owns `delivery-lifecycle.nix`, composition and registration; no packet-specific derivation.

Compose after actual deployment generations and pending work created by TASK-490.14 cases in the same fresh Delivery VM/target. Prove rollback to prior target generation, independent drift detection, and bounded recovery after server/builder/agent restart during work. Reuse same-run target/job receipts; do not depend on TASK-490.14's separate run state. Version transition remains TASK-490.23 under the Upgrade owner.

Requires TASK-490.7/.14 role and code prerequisites, accepted authority, and passing foundation/.25/.26. Writable paths are named fragments only. Production, shared helper, scenario composition/registration, CI and docs are read-only. Coordinator records one Delivery VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 UI rollback restores the previous generation on the real target
- [ ] #2 Out-of-band changes are reported as drift with actual running identity
- [ ] #3 Server/builder/agent restarts during work recover to specified state
- [ ] #4 The coordinator composes this fragment after .14 in delivery-lifecycle; no packet derivation is added
- [ ] #5 The coordinator records actual Delivery VM revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Owner-selected retained-generation rollback contribution in existing coordinator worktree/branch TASK-490.1-integration-foundation and draft MR335. Use same five VMs/database/receipts after .14 manual A then B deployments. No new task-specific VM/worktree per explicit user continuation.
1. Require first CF-managed deployment A has server-owned retained generation snapshot and rollback_eligible=true; do not assume initial harness generation is eligible or seed retention.
2. After actual B activation, navigate existing production History rollback action to preselected Deploy plan; assert selected retained A identity and send normal UI rollback-generation operation. No duplicate API/UI full lifecycle.
3. Independently observe actual running A marker/path/profile and actual rollback generation (not assumed equal original number), real agent new state/history/succeeded request and two later heartbeats on correct current state. Inspect reload/system/history and screenshot.
4. Compose semantic recovery.py/recovery.cjs only after activation in same existing lifecycle. Run required normal owner command, Access regression where required, docs/appropriate lower-level checks and exact committed hosted pipeline. Keep broader drift/restart recovery acceptance open; no unrelated repair or parent completion.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: native owner-local layout and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Scenario isolation correction: TASK-490.14's generation/output/VM cannot be reused. Set up and perform the prior real deployment(s) in this scenario's own fresh targetA VM before exercising rollback and drift; reuse only immutable fixture definitions or role/helper code. Current foundation VM execution fails on TASK-490.25/.26, so remain Backlog.

Owner explicitly selected retained-generation rollback after two actual CF-managed manual deployments in the same Delivery lifecycle. No drift/restart-recovery completion is implied. Use semantic recovery.py/.cjs filenames, not historical task-numbered paths. Coordinator owns shared fixture/composition.

Mandatory rollback case executed after two completed manual CF-managed deployments and failed at fresh eligibility: firstgeneration1 is_current=false, generation_snapshot_id=null, rollback_eligible=false. No seeded snapshot/authority, alternatecommitrollback, skip/expectedfailure or allow_failure used. Actual restoration/UIrollbackrequest notexecuted. TASK499 is the newly observed blocking product dependency; its repair requires separate authorization and accepted artifact-authority strategy.

Latest normal owner command sh packages/ci/integration-owner-build.sh integration-delivery exit1 on x86_64-linux at3db47c89-dirty,13passingcases and1mandatoryfailure,driver303.80s. Evidence /nix/store/mrs8rm4yvf5lk9vklqs01qvmzp1zl9n7-vm-test-run-cf-it-delivery-delivery-lifecycle-run and integration-evidence/integration-delivery. Semantic recovery.py/recovery.cjs composed into existing lifecycle only. Fresh before/after request identity checks and exact retained binding checks are implemented below the failing prerequisite but remain unexecuted. Broader drift/restart recovery notimplemented. KeepInProgress/draftMR335; neverreportrollbackpassed.
<!-- SECTION:NOTES:END -->
