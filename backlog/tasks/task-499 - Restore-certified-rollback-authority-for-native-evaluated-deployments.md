---
id: TASK-499
title: Restore certified rollback authority for native-evaluated deployments
status: Backlog
assignee: []
created_date: '2026-10-20 04:21'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-server/src/models/primary_evaluation.nix
  - packages/default/crates/cf-server/src/services/composite_enforcement.rs
  - packages/default/crates/cf-server/src/queries/evaluation_snapshots.rs
  - packages/default/crates/cf-server/src/queries/systems.rs
  - docs/knowledge/evaluation/evaluation-snapshot-retention-and-rollback.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: high
type: bug
ordinal: 550000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The connected Delivery workflow performed real Git/native evaluation/build/publication, production UI deployment, cold candidate acquisition and live activation. The agent reported the new running generation and two later heartbeats correlated to its new state; CF marked the deployment succeeded. However pending_system_deployments.evaluation_snapshot_id was null, preventing certified generation retention and the production History rollback operation. Current primary native evaluation does not produce an available V1 artifact. Normal full Config inspection produces V2 artifacts with an independent selector, while deployment binding and retained rollback require available integrity-version-1 artifacts. Waiting or requesting V2 inventory cannot legitimately create V1 authority. Failed gate evidence /nix/store/qbzpy52pl92whcxx28p6i6v5wfwgkp5b-vm-test-run-cf-it-delivery-delivery-lifecycle-run. No repair is authorized by TASK-496; do not seed snapshots or weaken lineage. Define the supported artifact authority before repair.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accepted native deployment workflow obtains certified rollback authority with exact system/configuration/commit/derivation/output identity.
- [ ] #2 Two actual CF-managed native deployments permit authorized retained-generation UI rollback and actual target restoration without seeded authority.
- [ ] #3 Artifact schema versions and selector/retention boundaries remain explicit and are covered by regressions and updated source documentation.
- [ ] #4 Missing or mismatched certified lineage remains ineligible and cannot be bypassed by a supplied store path.
<!-- AC:END -->
