---
id: TASK-490.11
title: >-
  Delivery: build scheduling, cancellation, reassignment, and stale-session
  fencing scenario
status: Backlog
assignee: []
created_date: '2026-10-17 16:37'
labels:
  - testing
  - integration
  - luna
dependencies:
  - TASK-490.10
parent_task_id: TASK-490
priority: high
type: task
ordinal: 523000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-delivery. Contract ID: delivery.build.scheduling. Exclusive write paths: tests/integration/delivery/scenarios/build-scheduling.{nix,py,cjs}, tests/integration/delivery/fixtures/build-scheduling-*. This packet also provides the real failing-build fixture that TASK-490.5 reuses (document its path and facts in task notes).

WHY: cancel, retry, reassignment, and fencing are proven only by mocked Builds-view steps (15-15k) and Rust state-machine tests. The contract requires the real execution result.

SCOPE: operator cancels a running job from the Builds view -> builder stops and the job reaches the cancelled state with no completion; retry requeues and a real build completes; builder A is stopped with unit_stopped/unit_killed mid-job -> lease expiry reassigns the job to builder B; when builder A returns, its stale session cannot complete or publish the job (fencing); a deliberately failing derivation records a failed job and its log. Non-goals: strategy authorization (490.10); notifications (490.5).

READY WHEN: TASK-490.10 is integrated. Source to read then: cf-server handlers/api/builders.rs (cancel, requeue, force-cancel, prioritize, finalize-cancelled, heartbeat), queries/builders*, job lease/heartbeat modules; docs/knowledge/api/builder-job-lifecycle-api.md; design BuildsView.jsx.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Cancel from the UI stops real execution and records cancellation without completion
- [ ] #2 Retry produces a real completed build
- [ ] #3 A killed builder's job is reassigned and the stale session cannot complete or publish it
- [ ] #4 A failing derivation records a failed job with its log
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
