---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
updated_date: '2026-10-20 22:48'
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
Owning check: integration-upgrade. Contract ID: upgrade.continuity. Coordinator owns version-transition.nix, pinned old/current package wiring, shared roles, browserSpecs and explicit owner registration. Child owns semantic version-transition.py/.cjs case modules and immutable fixtures. No per-task derivation or cross-scenario mutable-state reuse.

Current usable integration base is df11c8005de5ab7088015757dafc7fa7602da188 and subsequent separately attributable authorized repairs. Foundation startup/Rust-test blockers are resolved and MUST NOT be treated as active dependencies. Access23 and real Delivery native build/deploy/state reporting execute. Upgrade itself remains unexecuted.

D2 is the actual blocker. The owner must select the oldest supported server predecessor and immutable source, actual agent/builder revisions, permitted operations, each compatibility window and its start/expiry/refusal behavior. Candidate v0.3.1 is f9dd6b9c79530d3f911203645207bf6251552aba; v0.3.0 is f4a53b850d84dc3f6b36444ec6e0f474d344735c. Neither tag is support policy. The v0.3.1 builder still uses direct PostgreSQL access in nominal API mode; do not present it as API-only or provide DB credentials merely to pass. A proposed 24-hour old-agent window plus builder-upgrade-first rule awaits owner approval.

After D2, start the actual predecessor package and its own migrations/API/UI against one fresh Upgrade VM database. Use its real data-creation workflows to create identities, credentials, history and in-flight work. Stop old writers, then start the current package normally against the same PostgreSQL data directory and let normal migrations run. Verify identity/password/history preservation, explicit in-flight recovery outcomes, approved old-client compatibility/refusal and one new real operation. No synthetic old schema, truncated current migrations, seeded successful predecessor results, same-version restart substitute or placeholder owner.

Use checks/integration-upgrade/scenarios/version-transition.{nix,py,cjs} with sequential old/current phases in the one registered version-transition scenario. Run its gate and normal aggregate only once meaningful implementation exists, recording exact old/current commits, platform, command statuses, executed/reused evidence and artifacts. Unknown decisions block only dependent Upgrade cases; continue independent owners.
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
Current D2 research checked accepted release/support records and verified exact tags with git ls-remote. No approved predecessor/window found. Recommendation submitted: v0.3.1 f9dd6b9c79530d3f911203645207bf6251552aba; explicitly tested 24-hour old-agent heartbeat/supported Nix deployment window; builders upgrade first because old nominal API mode accesses PostgreSQL. Broader old-builder support requires explicit legacy-DB compatibility authority or a separate product decision, not a test credential shortcut. No Upgrade implementation or VM success claimed. Historical bootstrap/Rust failure instructions and obsolete task-numbered paths were removed from active guidance.
<!-- SECTION:NOTES:END -->
