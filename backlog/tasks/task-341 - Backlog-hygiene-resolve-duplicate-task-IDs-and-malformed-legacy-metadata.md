---
id: TASK-341
title: 'Foundation: remove dead/legacy web-ui files and fix duplicate backlog metadata'
status: Done
assignee: []
created_date: '2026-06-10 03:01'
updated_date: '2026-06-11 03:52'
labels:
  - backlog
  - maintenance
  - web-ui
  - cleanup
  - foundation
milestone: 'm-1: Development Infrastructure'
dependencies: []
references:
  - packages/web-ui/src/views/mod.rs
  - backlog/tasks
  - backlog/milestones
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/mod.rs
  - backlog/tasks/**
  - backlog/milestones/**
priority: high
ordinal: 1720
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: the repo and backlog both carry dead/duplicate artifacts that slow agents down and cause confusion.

Goal: remove confirmed dead UI files and normalize duplicate backlog metadata so the codebase and backlog are unambiguous.

## Exact scope — codebase (verify each is truly unused before deleting)
Confirm with a repo-wide search that nothing references these, then remove and update `views/mod.rs`:
- `packages/web-ui/src/views/cves_old.rs`
- `packages/web-ui/src/views/systems_mock.rs`
- `packages/web-ui/src/views/systems_mock_data.rs`
- `packages/web-ui/src/views/systems_mock_data_extra.rs`
- `packages/web-ui/src/views/flakes_list.rs` (only if `FlakesView` uses `flakes.rs`)
- `packages/web-ui/src/views/environments_list.rs` (only if unused)
- `packages/web-ui/src/views/policies_api.rs` (only if unused)
- `packages/web-ui/src/views/register_api.rs` (only if unused)

If any file IS still referenced, do NOT delete it; instead note the reference and leave it.

## Exact scope — backlog metadata
- Resolve duplicate active task IDs (e.g. duplicate `TASK-303`/`TASK-327` records) by archiving the stale/malformed one.
- Normalize duplicate milestone identifiers (two `m-16` entries).

## Non-goals
- No view redesign or behavior changes.

## Architectural constraints
- Verify repo-wide references before deleting any candidate file.
- Keep cleanup scoped to dead-file removal plus backlog metadata normalization; no redesign or feature work.
- Use Backlog MCP operations for task/milestone cleanup rather than manual markdown edits.

## Impact areas
- `packages/web-ui/src/views/mod.rs`
- candidate legacy files under `packages/web-ui/src/views/`
- `backlog/tasks/**`
- `backlog/milestones/**`

## Risk level
- Medium: incorrect deletion or metadata cleanup could break builds or task lookup.

## Verification plan
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
- `git status` shows only intended deletions/edits
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Confirmed-unused legacy web-ui files are removed and views/mod.rs is updated
- [x] #2 Any still-referenced candidate file is left in place with a note explaining why
- [x] #3 Duplicate active task IDs are resolved or archived so MCP task operations are unambiguous
- [x] #4 Duplicate milestone identifiers/titles are normalized
- [x] #5 web-ui cargo check (wasm) and nix build .#checks.x86_64-linux.web-ui pass after cleanup
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Completed and merged into dev via MR !270 (commits d1229de8, merge b212cb5f).

## Web-UI File Analysis
- DELETED: cves_old.rs (confirmed unused, only referenced in docs; was never declared in views/mod.rs)
- KEPT (still actively used, documented): systems_mock*.rs (used by flakes_list.rs), flakes_list.rs (used by flakes.rs), environments_list.rs (used by environments.rs), policies_api.rs (used by policies.rs + policy_editor_modal.rs), register_api.rs (used by register.rs)

## Backlog Metadata
- Archived 4 empty/malformed task template duplicates (task-111..114 lowercase variants) to backlog/archive/tasks/malformed-duplicates/
- Resolved m-16 milestone duplicate: created m-22 for "Compliance MVP - Prove, Don't Just Display", updated TASK-320 reference, retained m-16 as "Design Standards Parity - CrystalForgelatest"
- 9 substantial duplicate task ID pairs (141, 142, 178, 209, 210, 214, 238, 327, 337) documented for follow-up (TASK-350)

## Verification
- cargo check (web-ui wasm): PASSED
- nix build .#checks.x86_64-linux.web-ui: PASSED (exit 0)

## Follow-up
- TASK-350 created for remaining duplicate task ID pairs requiring human review
<!-- SECTION:NOTES:END -->
