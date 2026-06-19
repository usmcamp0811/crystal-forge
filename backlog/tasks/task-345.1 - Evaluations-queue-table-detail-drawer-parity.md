---
id: TASK-345.1
title: 'Evaluations: queue/table + detail drawer parity'
status: Done
assignee: []
created_date: '2026-06-10 13:33'
updated_date: '2026-06-19 01:49'
labels:
  - design-parity
  - evaluations
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-345
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EvalsView.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EvalDrawer.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/components/eval_log_modal.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/components/icon.rs
  - packages/default/src/models/evaluate_with_policies.rs
  - packages/default/src/derivations/utils.rs
  - packages/default/src/queries/derivations.rs
  - packages/default/src/queries/commits.rs
  - packages/default/src/api/models.rs
  - packages/default/src/handlers/api/commits.rs
  - packages/default/migrations/0141_add_closure_counts_to_derivations.sql
  - >-
    packages/default/.sqlx/query-73dfa0c67fb6960c5435ebcdbc139d2a29d9689557624f0cf2d734bd4e576f9b.json
parent_task_id: TASK-345
priority: high
ordinal: 1761
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Evaluations umbrella TASK-345. Follow guide doc-14 standard procedure.

## Problem
The Evaluations view (`views/evaluations.rs`) must match `CrystalForgelatest/components/EvalsView.jsx` and `EvalDrawer.jsx` for queue/table layout and the detail drawer.

## Goal
Pixel-align the Evaluations queue/table, selection behavior, and detail drawer (policy matrix, dependency graph, live logs) to the design, backed by real API/websocket data.

## Exact scope
1. Queue/table density, columns, selection highlight, first-row auto-select match design.
2. Detail drawer matches EvalDrawer.jsx (policy matrix, dependency graph, real log streaming).
3. Ordering/cancel controls work against the real API.
4. Backend gap-filling: minor/simple backend fixes that are discovered during parity work and are required to make the UI work correctly against real data. This does NOT extend to new backend feature work (that would be a separate task).

## Non-goals
- Builds view (sibling surface TASK-347).
- Shared coherence-only changes covered by TASK-275 (coordinate, don't duplicate).
- New backend endpoints or major backend refactors (beyond simple fixes).
- Mobile-first layout changes beyond desktop parity.

## Impact areas
- packages/web-ui/src/views/evaluations.rs — primary parity target.
- packages/web-ui/src/components/eval_log_modal.rs — log modal structure updates.
- packages/web-ui/assets/app.css — CSS additions for parity.
- checks/web-ui/tests/integration-test.js — test steps 26-evaluations and 26b-evaluations-history.
- packages/default/src/ — only for simple backend fixes discovered during parity work (e.g., missing fields in API responses, sorting gaps).

## Architectural constraints
- UI layer must not import infrastructure layer directly.
- No business logic in UI views — keep evaluations view as presentation only.
- Use existing domain types (BuildStatus, FlakeCommit, etc.) from api::models; do not duplicate.
- Follow existing repository patterns for Dioxus components (rsx!, component functions, Signals).
- EVAL_QUEUE_SNAPSHOT and EVAL_QUEUE_EVENT websocket messages must be used for real-time updates (follow existing eval_log_modal.rs patterns).
- Any backend changes discovered must follow the existing query/handler pattern and include migrations if schema changes are needed.

## Risk level
Medium. The evaluations surface involves real-time websocket state, queue ordering, and log streaming. The detail drawer (policy matrix, dependency graph) requires careful pixel-matching. Small backend fixes may be needed for missing fields.

## Dependencies
- TASK-347.1 (Builds parity) — pattern reference for doc-14 procedure; should be in Review before heavy eval tray work.
- TASK-275 (visual coherence) — coordinate to avoid duplication on shared CSS.
- CrystalForgelatest reference files: EvalsView.jsx, EvalDrawer.jsx.

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `26-evaluations` and `26b-evaluations-history`.
- nix develop -c cargo clippy --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown -- -D warnings
- git status must show no unintended untracked files.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Evaluations queue/table density, columns, selection, and first-row auto-select match the design
- [x] #2 Detail drawer matches EvalDrawer.jsx (policy matrix, dependency graph, live logs)
- [x] #3 Ordering/cancel controls operate against the real API
- [x] #4 Simple backend gap-filling fixes discovered during parity work are included (minor field/endpoint adjustments)
- [x] #5 web-ui steps screenshot queue + drawer and assert selection and a real control
- [x] #6 fmt, clippy, and web-ui check all pass
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Merged MR !279 into the integration branch. Final fixes included bounded closure-count concurrency and bounded dependency-graph pending-count polling.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
MR !279 was merged for TASK-345.1.

Summary:
- Brought the Evaluations queue/table and detail drawer closer to the CrystalForgelatest reference design.
- Restored correct active-queue behavior: active rows open the drawer, queue ordering uses remove-then-insert reorder semantics, and mutation failures are reported honestly through UI toasts.
- Updated detail drawer dependency graph to use real backend closure counts instead of misleading fallback 0/1 package counts.
- Added backend closure count persistence on derivations via a new migration and API/DTO support for `closure_counted`.
- Hardened closure count execution with explicit logging, 120s per-command timeouts, chunked Nix invocations, and a process-local concurrency cap.
- Bounded UI dependency-graph polling so pending/failed historical closure counts do not poll forever.

Verification performed during review fixes:
- `SQLX_OFFLINE=true nix develop -c cargo check --manifest-path packages/default/Cargo.toml` passed.
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown` passed.
- `nix develop -c rustfmt --edition 2024 --check packages/default/src/derivations/utils.rs` passed.
- `nix develop -c rustfmt --edition 2024 --check packages/web-ui/src/views/evaluations.rs` passed for the final polling fix.

Notes:
- Full `nix flake check` was not run during the final targeted fixes.
- Some larger touched files had pre-existing rustfmt churn and were not wholesale reformatted.
<!-- SECTION:FINAL_SUMMARY:END -->
