---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-18 03:31'
labels:
  - testing
  - integration
  - blocked-decision
dependencies:
  - TASK-490.7
modified_files:
  - >-
    checks/integration-upgrade/scenarios/fragments/task-490.23-version-transition.py
  - >-
    checks/integration-upgrade/scenarios/fragments/task-490.23-version-transition.cjs
  - checks/integration-upgrade/fixtures/*
parent_task_id: TASK-490
priority: high
type: task
ordinal: 535000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-upgrade. Contract ID: upgrade.continuity. This packet owns only old/current version transition Python/browser case fragments and immutable test source fixtures beneath `checks/integration-upgrade/scenarios/fragments/` and `fixtures/`. The coordinator owns `version-transition.nix`, composition, browserSpecs mapping, pinned old source input wiring and owner registration. An implementation packet does not automatically receive a NixOS test derivation.

BLOCKED D2: no accepted support decision identifies the oldest supported in-place server release, agent/builder compatibility window, or exact pinned source. Existing tags and crate versions are not support promises. Once D2 is resolved, one fresh Upgrade VM/database runs the real old package to create identities, credentials, history and in-flight work; then the current package migrates that same database and proves continuity/recovery. The old/current phases are sequential in the same VM; separate derivations cannot share live DB state. Upgrade remains separate from current-version owners because the in-place version/schema transition mutates state that must not contaminate their proofs.

Do not change application dependencies, run synthetic old schema or reuse another check's data. D1 uncertainty remains blocked. The coordinator owns package-input wiring, scenario composition and explicit registration. Requires TASK-490.7 shared role API, resolved D2, passing foundation and TASK-490.25/.26. Remain Backlog until all are concrete.
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
2026-10-17 organization correction: native Upgrade owner-local layout, coordinator-owned registration/shared/CI/docs and input wiring. D2 remains unresolved, as does actual foundation dependency commit. Foundation blocked by two existing Rust package tests; no VM execution. Backlog, not ready. Actual dependency commit(s) and verified commands plus owner support decision required before To Do. No Luna delegation or executed slice is claimed.
<!-- SECTION:NOTES:END -->
