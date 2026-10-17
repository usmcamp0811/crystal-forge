---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: Backlog
assignee: []
created_date: '2026-10-17 16:39'
labels:
  - testing
  - integration
  - blocked-decision
dependencies:
  - TASK-490.7
parent_task_id: TASK-490
priority: high
type: task
ordinal: 535000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Owning check: integration-upgrade. Contract ID: upgrade.continuity. Exclusive write paths: tests/integration/upgrade/scenarios/continuity.{nix,py,cjs} and the pinned old-version input wiring that the coordinator approves.

BLOCKED: owner decision D2 (coverage map, Upgrade section). No document defines the supported upgrade source or the agent/builder compatibility window. Tags v0.3.1, v0.3.0, v0.2.x exist; all crates report 0.3.0; a tag alone is not a support promise. The owner must name: (a) the oldest supported server release for in-place upgrade; (b) whether agents and builders from that release must keep working against the current server, and for how long; (c) the pinned source (tag/commit) to build that release. Adding the old release as a pinned flake input or fetch must not change any current application dependency.

SCOPE once unblocked: the old package (built from its own pinned source, using its own migrations) creates users and credentials, history (evaluations, builds, deployments), and in-flight work (queued build, pending deployment) through its own API/UI; the current package then starts through the normal module on the same PostgreSQL data directory and migrations apply; the same users sign in with their old passwords; history identities are unchanged; in-flight work recovers to the specified state; one representative new operation (evaluate + build) completes; within the declared window, the old agent/builder still operates against the new server. Prohibited substitutes: same-version restart, synthetic old schema, current-version seeded data. Non-goals: current-version restart recovery (Delivery 490.15); migration-prefix rehearsals in server-regressions (kept as non-integration).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The owner decision D2 is recorded with the supported source release, compatibility window, and pinned source
- [ ] #2 Data created by the real old release survives migration with identities, credentials, and history intact
- [ ] #3 In-flight work recovers across the version transition and one new operation completes
- [ ] #4 Old agents/builders within the declared window remain usable
- [ ] #5 Executed in real VMs with recorded evidence
<!-- AC:END -->
