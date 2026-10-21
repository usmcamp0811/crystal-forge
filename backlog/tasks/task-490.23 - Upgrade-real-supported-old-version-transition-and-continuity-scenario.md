---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-21 08:24'
labels:
  - testing
  - integration
  - blocked-decision
dependencies:
  - TASK-490.7
modified_files:
  - checks/integration-upgrade/scenarios/version-transition.py
  - checks/integration-upgrade/scenarios/version-transition.cjs
  - checks/integration-upgrade/fixtures/*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 535000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-upgrade. Contract: upgrade.continuity. Coordinator owns version-transition.nix, pinned old/current package wiring, shared roles, browserSpecs and explicit native registration. The child owns semantic version-transition.py/.cjs and immutable fixtures. No per-task derivation or cross-scenario mutable state is permitted.

Current pushed checkpoint is 204a475287b62087e28d2e9e7f1b0ff2631daaf3, plus the continuing Assurance increment. Foundation startup and Rust-test blockers are resolved and are not active dependencies. Real native build/deployment/retained rollback capabilities passed locally at 9f53fade-dirty. Pipeline 2970142621 at clean204a4752 passed Access23 and isolated database525 but failed Delivery test plumbing; it is not a clean hosted rollback pass. Upgrade itself remains unimplemented and unexecuted.

D2 remains the specific decision gap. Recommended server predecessor: v0.3.1 at f9dd6b9c79530d3f911203645207bf6251552aba. The owner must approve or replace this pin and specify actual agent/builder revisions, operations, compatibility windows, start/expiry/refusal behavior and predecessor in-flight recovery outcomes. The proposal is builders upgraded before server cutover, with no old-builder/current-server window or database credential exception; old v0.3.1 agents may send signed heartbeats and perform the selected supported Nix deployment for 24 hours from successful current-server startup only if approved. This must reconcile AC4, which currently includes old builders. The v0.3.1 builder uses PostgreSQL even in nominal API mode. A tag does not grant a support promise or legacy-DB trust exception.

After approval, run the actual predecessor package and its own migrations/API/UI against a fresh Upgrade database. Create identities, credentials, history and in-flight work through real workflows. Stop old writers, start the current package normally against the same PostgreSQL data directory, and let normal migrations run. Verify approved continuity/recovery, mixed-version operations/refusal and one new real operation. No synthetic old schema, truncated migrations, seeded successes, same-version restart substitute or empty owner.

Use checks/integration-upgrade/scenarios/version-transition.{nix,py,cjs}. Run nix build .#checks.x86_64-linux.integration-upgrade.scenarios.version-transition -L and the normal integration-upgrade aggregate only after meaningful implementation exists. Record exact predecessor/current commits, platform, statuses, execution/reuse and artifacts. D2 blocks only dependent Upgrade cases; independent Assurance continues.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The owner decision D2 names supported source release, compatibility window and pinned source
- [ ] #2 Real old-release data survives in-place current-version migration with identities, credentials and history intact
- [ ] #3 In-flight work recovers across version transition and one new operation completes
- [ ] #4 Old agents/builders within the declared compatibility window remain usable
- [ ] #5 The coordinator composes the fragments into one `version-transition` scenario; no separate derivation per implementation packet, and runtime evidence is recorded
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
D2 research inspected accepted release/support records and pinned historical builder source. No approved predecessor, client matrix or compatibility window was found. Git tag identity is not support policy. Candidate v0.3.1 resolves to f9dd6b9c79530d3f911203645207bf6251552aba; candidate v0.3.0 resolves to f4a53b850d84dc3f6b36444ec6e0f474d344735c.

A compact request proposes v0.3.1, builders upgraded before server cutover without a database-credential exception, and a 24-hour window for selected old-agent heartbeat and Nix deployment operations from successful current-server startup. The owner must approve or replace the pin and define expiry/refusal and in-flight recovery. The proposal must reconcile AC4's old-builder requirement. No compatibility window, Upgrade implementation or VM test has passed.

Active guidance uses version-transition.{nix,py,cjs} and the matching focused gate. Foundation startup and earlier Rust-test failures are resolved historical evidence, not active blockers. Independent Assurance execution continued while D2 remained outstanding.
<!-- SECTION:NOTES:END -->
