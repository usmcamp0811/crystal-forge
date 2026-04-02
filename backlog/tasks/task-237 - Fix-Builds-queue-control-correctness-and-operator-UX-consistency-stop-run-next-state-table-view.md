---
id: TASK-237
title: >-
  Fix Builds queue control correctness and operator UX consistency
  (stop/run-next/state/table view)
status: Backlog
assignee: []
created_date: '2026-04-02 01:09'
labels:
  - builds
  - queue
  - backend
  - frontend
  - ux
  - sprint-ready
dependencies: []
references:
  - packages/default/src/handlers/api/builders.rs
  - packages/default/src/queries/builders.rs
  - packages/default/src/queries/dashboard.rs
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/components/builds/build_queue_pane.rs
  - packages/web-ui/src/components/builds/build_detail_pane.rs
  - checks/web-ui/tests/integration-test.js
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem Statement
The Builds view has multiple operator-critical inconsistencies:
- Clicking **Stop** on a build queue card returns "Stop build is not implemented by API yet".
- Build stop/cancel behavior is unclear and may be delayed while underlying systemd/nix processes unwind.
- **Run Next** does not reliably move jobs to the front of effective queue order.
- **Restart** appears for jobs where restart action is not meaningful (e.g., not currently building).
- Builder panel queue depth can show `0` despite active work and paged queue evidence.
- Queue card running time is shown as raw seconds rather than human-readable duration.
- Queue-card-only layout is inefficient at scale; operators need an at-a-glance table mode.

## Goal
Ship one coherent Builds queue reliability + UX pass so operator actions are correct, state is truthful, and high-volume queue operation is efficient.

## Non-Goals
- No scheduler redesign beyond action correctness and deterministic ordering updates required for Run Next/Stop behavior.
- No unrelated deployment pipeline refactors outside Builds queue/control paths.
- No visual redesign outside ergonomics improvements needed for table mode and clear action/state feedback.

## Scope
1. **Stop Build API + behavior**
   - Implement backend stop/cancel endpoint behavior used by Builds UI.
   - Define action semantics:
     - queued job: cancel/remove from queue
     - building job: request stop/interrupt and transition through explicit stopping/canceled/final state
   - Handle slow termination paths (systemd/nix unwind) with clear intermediate UI state and polling/refresh strategy.

2. **Run Next correctness**
   - Ensure Run Next mutates persisted ordering/priority so the selected queued job actually moves to effective next-claim position under documented precedence.
   - Ensure queue UI reflects this immediately and after refresh.

3. **Action availability correctness**
   - Show/enable Restart only for statuses where restart is valid.
   - Prevent invalid actions with clear UX hints instead of no-op behavior.

4. **Queue depth correctness**
   - Fix builder summary/query so queue depth reflects real queued + active context (including paginated queue data) consistently.

5. **Duration formatting**
   - Replace raw seconds display with human-readable elapsed duration format (e.g., `1m 32s`, `2h 04m`).

6. **Queue table view**
   - Add a table/list mode toggle for build queue alongside cards.
   - Include essential columns for operational triage (status, host/system, flake/config, commit, builder, queued/running time, actions).
   - Preserve current filtering/pagination behavior across view mode changes.

7. **Operational notes**
   - Document expected stop lifecycle behavior and known delay characteristics so operators understand what "stopping" means.

## Architectural Constraints
- Queue control business logic must remain backend-authoritative; UI must not fake state transitions.
- Maintain clear separation between action commands, queue/order query logic, and UI presentation.
- Preserve deterministic ordering and concurrency safety for stop/run-next races.
- No hidden global mutable state in UI for queue/action status.

## Verification Plan
### Tier 0
- `nix develop -c env SQLX_OFFLINE=true cargo check --package crystal-forge`
- `nix develop -c cargo check --package crystal-forge-ui`
- Targeted backend tests for stop/run-next/order/depth behavior.
- Targeted frontend tests for action enablement, duration formatting, and table-mode rendering.

### Tier 1
- End-to-end local validation with active build(s):
  - Stop queued and running jobs; verify lifecycle states and final outcomes.
  - Run Next moves selected job to effective top position.
  - Queue depth matches builder/queue reality.
  - Table mode supports practical queue operations at scale.

### UI Evidence Requirement
- Update `web-ui` check scenario(s) to capture Builds queue table mode and action-state behavior.
- MR must include screenshots sourced from `web-ui` check artifacts showing:
  - queue table mode
  - stop-in-progress state
  - corrected duration format

## Impact Areas
- Build control API handlers and queue mutation/query paths.
- Builds UI queue components (cards + new table mode + action affordances).
- Builder summary/depth query/model projection.
- Web UI integration test/check assets for Builds evidence.

## Risk Level
High (operator controls and build-state correctness directly affect production operations).

## Dependencies
- Existing builder heartbeat/job claim pipeline must be functional for stop/run-next validation.
- Any required permissions/authz behavior for stop/restart actions must remain enforced.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Clicking Stop from Builds queue no longer returns 'not implemented'; backend executes defined stop semantics for queued and building jobs.
- [ ] #2 For building jobs, UI shows an explicit intermediate 'stopping' (or equivalent) state while termination is in progress, then reaches a terminal status without silent failure.
- [ ] #3 Run Next moves the selected queued job to effective next-claim order under documented precedence and remains correct after page refresh.
- [ ] #4 Restart action is only shown/enabled for statuses where restart is valid; invalid statuses do not present misleading restart actions.
- [ ] #5 Builder queue depth displayed in Builds view matches backend queue reality and remains consistent with paginated queue data.
- [ ] #6 Running/elapsed times in queue UI are human-readable (not raw seconds) and update correctly while active.
- [ ] #7 Build queue supports both card and table modes; table mode includes key operational columns and retains filters/pagination when toggled.
- [ ] #8 Automated backend/frontend tests cover stop semantics, run-next ordering, action availability, queue depth correctness, and duration formatting.
- [ ] #9 `web-ui` check captures Builds evidence for table mode and action states; MR includes those screenshots from check outputs.
- [ ] #10 Task notes document stop lifecycle behavior, expected delays, and operator-facing UX decisions.
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 No API path used by Builds queue controls returns unimplemented placeholders for supported actions.
- [ ] #2 Out-of-scope issues discovered during implementation are captured as separate Backlog tasks.
<!-- DOD:END -->
