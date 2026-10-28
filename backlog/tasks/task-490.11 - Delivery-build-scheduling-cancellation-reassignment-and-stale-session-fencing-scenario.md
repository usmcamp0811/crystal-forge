---
id: TASK-490.11
title: >-
  Delivery: build scheduling, cancellation, reassignment, and stale-session
  fencing scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
updated_date: '2026-10-24 23:07'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.10
modified_files:
  - checks/integration-delivery/scenarios/fragments/task-490.11-scheduling.py
  - checks/integration-delivery/scenarios/fragments/task-490.11-scheduling.cjs
  - checks/integration-delivery/fixtures/build-scheduling-*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 523000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.build.scheduling. This packet owns disjoint scheduling Python/browser fragments and authored failing-build fixture definitions only under `checks/integration-delivery/scenarios/fragments/task-490.11-*` and `fixtures/build-scheduling-*`. Coordinator owns `delivery-lifecycle.nix`, composition, browserSpecs mapping and registration. The fragment has no separate test derivation.

Use real jobs/builds from earlier cases in the same fresh Delivery VM where compatible; assert UI cancellation stops execution without completion, retry reaches a real success, builderA loss reassigns to builderB, stale builderA cannot complete/publish, and an authored failing derivation stores its actual failure log. Same-run build receipts may feed the Access notification case only as immutable fixture definitions/code; TASK-490.5 must recreate its own event in its own VM.

Requires TASK-490.7/.10 code and roles plus passing foundation/.25/.26. Writable paths stay named fragments/fixtures. No production, helper, scenario composition, registration, CI or docs changes. Coordinator records one Delivery VM result/evidence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cancel from UI stops real execution and records cancellation without completion
- [ ] #2 Retry produces a real completed build
- [ ] #3 A lost builder job is reassigned and the stale session cannot complete or publish it
- [ ] #4 A real failing derivation records a failed job and log
- [ ] #5 The coordinator composes the fragment into delivery-lifecycle; no packet derivation is added
- [ ] #6 The coordinator records exact VM revision, command, outcome and evidence
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-10-17 organization correction: owner-local scenario/fixture paths and explicit coordinator registration. Foundation dependency commit unresolved; foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands required before To Do. No Luna delegation or executed slice is claimed.

Merge reconciliation retains source-packet future failure-injection specificity: stop/kill builderA via the existing unit_stopped/unit_killed helpers, expire its lease and reassign to builderB, then prove returning stale builderA cannot complete or publish. Notification task .5 may reuse immutable failing-build fixture definitions only and must create its own event in its own scenario. Preserve canonical cancellation/retry/log and same-run identity boundaries. No future case is claimed executed.
<!-- SECTION:NOTES:END -->
