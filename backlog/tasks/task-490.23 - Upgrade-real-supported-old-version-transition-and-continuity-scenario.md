---
id: TASK-490.23
title: 'Upgrade: real supported old-version transition and continuity scenario'
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-17 16:39'
updated_date: '2026-10-23 01:40'
labels:
  - testing
  - integration
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
Owning check: integration-upgrade. Contract: upgrade.continuity. D2 is approved by the owner, not a remaining policy blocker. Predecessor is real Crystal Forge v0.3.1 at f9dd6b9c79530d3f911203645207bf6251552aba; local tag relationship verified. Pin actual source and required inputs, use genuine packages and normal supported configuration. Do not patch predecessor behavior or substitute current code/version strings, synthetic old schema, historical-looking seeded outcomes or another owner's live state.

Approved controlled maintenance sequence: establish the predecessor and representative persisted state through real workflows; stop predecessor server-side writers/builders at the cutover boundary; upgrade builder packages/config but keep current builders stopped; start current server normally against the SAME installation/database, letting normal migrations run; verify readiness then start current API-only builders; exercise old-agent/current-server signed heartbeat/state and one actual ordinary Nix deployment; deliberately upgrade agents preserving registered system/signing identity/history. No concurrent old/new writers, old-builder/current-server window, post-cutover builder DB-credential exception, invented timed expiry or new version-refusal policy.

Old-agent compatibility is limited to this exact approved pin and selected operations during the operator's agent rollout until its deliberate completion. Record actual executable/version/protocol identities; deployment must not replace the old agent before compatibility proof. Request acceptance is not target activation or reporting proof.

Preserve completed history/identity relationships and representative credentials. Establish queued/interrupted work through actual predecessor activity with controlled pause/stop/transport interruption. Recover through current execution, retaining logical identity/audit and legitimate successor-attempt relationships. Stale predecessor claims must not commit after replacement ownership; recovered operations must reach intended outcome without duplicate authoritative success or contradictory terminal history. Original evidence remains attributable. Incomplete publication and failed/missing evidence must not become deployment/completion authority. Physical retries need not execute exactly once. No new queue/recovery/compatibility framework.

Use one meaningful native Snowfall integration-upgrade owner with version-transition.{nix,py,cjs}, existing harness/roles/evidence and explicit registration. Preserve installed state across transition. No placeholder or per-criterion VM/check. Coordinator owns package/wiring/composition/CI; semantic cases and necessary immutable fixtures belong under this owner. D2 authorizes test/fixture/wiring implementation, not blanket production fixes. Any discovered compatibility/recovery defect keeps its correct failing assertion and requires applicable separate authority.

Run the normal Upgrade aggregate and inspect actual old/current identities, normal migration, recovery, old-agent observation, deliberate agent upgrade and new current operation. Record exact inputs/commands/platform/exits/names/counts/fresh-or-reused/accessibility. Then integrated four-owner/lower-level/hosted verification is required. MR335 remains draft until all concrete merge blockers are resolved; no merge/deployment outside disposable tests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Approved D2 predecessor is v0.3.1 at f9dd6b9c79530d3f911203645207bf6251552aba with real packages, pinned required inputs and the recorded controlled maintenance sequence.
- [ ] #2 Real predecessor-created data survives normal in-place current-server migration with representative credentials, registered system/signing identities, completed records and history relationships intact.
- [ ] #3 Real queued/interrupted predecessor activity recovers through current ownership/retry without stale authoritative completion or duplicate authoritative success; incomplete publication remains unauthorized and one new current operation completes.
- [ ] #4 Builders are upgraded before current-server cutover and held stopped until readiness with no old-builder/current-server operating window or DB-credential exception; the exact predecessor agent supports signed heartbeat/state and one actual ordinary Nix deployment during the operator's rollout, followed by deliberate agent upgrade preserving identity/history, without timed expiry or new version-refusal policy.
- [ ] #5 Coordinator composes one real version-transition scenario under native integration-upgrade using existing harness/evidence, retains the same installed state, executes the normal owner gate and records exact versions, inputs, commands, outcomes and accessible evidence.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Verify the approved exact predecessor source/tag and normal package/module/configuration interfaces; pin required inputs without modifying predecessor behavior or silently updating current inputs.
2. Build one real version-transition scenario under the native Upgrade owner and existing harness/evidence. Establish representative persisted state plus queued/interrupted activity through actual predecessor workflows; preserve the same PostgreSQL installation and target identities.
3. Implement controlled stop/fence boundary, upgraded-but-stopped builders, normal current-server migrations/readiness, then current API-only builders. Verify history/credentials/identity relationships and authoritative recovery, including stale claim/incomplete-publication rejection.
4. Observe the genuine old agent signed heartbeat/state and actual selected Nix deployment before deliberate agent replacement; upgrade its package preserving signing/system identity and verify a new current operation.
5. Execute focused/normal Upgrade gates, inspect actual versions/commands/rows/target outcomes and failures, then integrate meaningful owner/CI wiring into MR335. No placeholder, new framework, fifth check, invented window/refusal, or unapproved production fix. Coordinator performs integrated four-owner/lower-level/hosted verification.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Approved D2 decision

Owner explicitly approved v0.3.1@f9dd6b9c79530d3f911203645207bf6251552aba and clarified the complete contract now in this description and amended AC4. `git rev-parse v0.3.1^{commit}` returned that exact commit in the coordinator tree. There is no24-hour assumption and no new version-refusal behavior. A tag alone is not support policy; this owner instruction is the approval. Compatibility/recovery have not been verified yet.

## Historical proposals

Earlier records lacked an accepted predecessor/window and proposed24hours; those proposals are superseded. v0.3.0@f4a53b850d84dc3f6b36444ec6e0f474d344735c is not selected. Historical old builder DB usage is permitted only inside the genuine predecessor phase, not after current-server cutover. Existing current-version migration rehearsals or restart tests are not genuine Upgrade proof.

Preflight: dedicated clean worktree/branch TASK-490.23-version-transition created from integration c21a0d512c8c9006c8373c62b48532459c8f6cc4; owner task lock recorded. Current application production source matches the accepted wrap-up baseline, with test-only STIG preparation already present. Main coordinator retains its STIG composition change and other ownership; no resetting/rebasing/overwriting occurred. No main integration worktree is registered. Use only invocation-owned disposable Upgrade VMs/database; existing task preview and persistent developer instances are forbidden targets.
<!-- SECTION:NOTES:END -->
