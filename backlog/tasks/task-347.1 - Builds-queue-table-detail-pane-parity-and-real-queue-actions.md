---
id: TASK-347.1
title: 'Builds: queue/table + detail pane parity and real queue actions'
status: Review
assignee: []
created_date: '2026-06-10 13:33'
updated_date: '2026-06-15 17:51'
labels:
  - design-parity
  - builds
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-347
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/BuildsView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/builds.rs
  - packages/web-ui/src/components/builds/build_queue_pane.rs
  - packages/web-ui/src/components/builds/build_detail_pane.rs
  - packages/web-ui/src/components/builds/helpers.rs
  - packages/web-ui/assets/app.css
parent_task_id: TASK-347
priority: high
ordinal: 1751
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Builds umbrella TASK-347. Follow guide doc-14 standard procedure.

## Problem
The Builds view (`packages/web-ui/src/views/builds.rs`) does not match `CrystalForgelatest/components/BuildsView.jsx`. Key gaps:
- Stat strip labels/values do not match (Building, Queued, Failed 24h, Workers, Slot usage).
- Worker cards layout and slot progress bar are missing or differ.
- Queue table columns differ from the reference (`#`, System configuration, Status, Worker, Derivations progress bar, Queued, Duration, Reorder·actions).
- Derivations column shows a dual-segment progress bar (cached=green, built=status-color) in the reference; not present in current.
- Row drag-to-reorder with visual drop indicators (`q-drop-before`/`q-drop-after`) exists in current builds.rs but may diverge from reference CSS classes.
- Detail side panel (`BuildDetailPanel`) layout, kv-grid, derivation progress section, and panel-actions differ.
- Build log modal (`BuildLogModal`) head/pre/foot layout may differ.
- `LiveIndicator` component (pulsing dot + "updated Ns ago") is absent.
- The "Queue build" action is explicitly mocked (line ~462) — must be replaced with a real disabled state or wired to real API.
- Multi-select bulk bar for batch cancel is absent.

## Goal
Pixel-align the full Builds surface to `BuildsView.jsx`:
- Page head + LiveIndicator
- Stat strip (5 stats: Building, Queued, Failed 24h, Workers, Slot usage)
- Worker cards section with slot progress bar
- Active / Completed tabs
- Queue table with all reference columns, dual-segment derivation bar, drag-reorder, multi-select bulk cancel bar
- Build detail side panel matching kv-grid and actions
- Build log modal matching reference structure
- Remove or truthfully disable the mocked queue-build action

## Non-Goals
- Evaluations view (covered by TASK-345).
- New backend API endpoints beyond what already exists for builds/queue/workers.
- Mobile-first layout changes beyond desktop parity.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Page head shows title + subtitle (N building · N queued · N/N workers active) + LiveIndicator (pulsing dot + updated Ns ago)
- [ ] #2 Stat strip has exactly 5 stats matching reference: Building, Queued, Failed 24h, Workers (N/N), Slot usage (%)
- [ ] #3 Build Workers section renders one card per worker: name, host mono, status chip (color-matched), arch+cores+mem, slots used/total label, slot progress bar
- [ ] #4 Active queue table has columns: # (position), System configuration (name+flake+commit+arch+currentPkg sub-rows), Status chip, Worker mono, Derivations dual-segment bar + N/N label, Queued, Duration, Reorder·actions
- [ ] #5 Drag-to-reorder rows with visual drop-before/drop-after indicators work in Active tab
- [ ] #6 Multi-select with shift-click works on cancellable rows; bulk cancel bar appears at bottom
- [ ] #7 Build detail side panel matches reference: panel-head, panel-body kv-grid, optional derivation-progress section, panel-actions (Logs, Cancel/Force-kill/Retry per status)
- [ ] #8 Build log modal matches reference: head, pre.sd-log-stream with sd-log-line/t/lvl/m spans and caret, foot (Download + Close)
- [ ] #9 Completed/History tab shows full-width table with appropriate columns and no reorder controls
- [ ] #10 The mocked 'Queue build' action is removed or replaced with a truthfully disabled button (no fake success messaging in production)
- [ ] #11 web-ui check steps 15-builds, 15d, 15e, 15g, 15h pass and capture evidence screenshots
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on host in /home/mcamp/code/crystal-forge/TASK-347.1-builds-parity

MR !278: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/278
<!-- SECTION:NOTES:END -->
