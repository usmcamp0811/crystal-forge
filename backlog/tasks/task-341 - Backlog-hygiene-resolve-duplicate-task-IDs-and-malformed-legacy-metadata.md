---
id: TASK-341
title: 'Foundation: remove dead/legacy web-ui files and fix duplicate backlog metadata'
status: Backlog
assignee: []
created_date: '2026-06-10 03:01'
updated_date: '2026-06-10 13:26'
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

See guide doc-14.

## Exact scope — codebase (verify each is truly unused before deleting)
Confirm with a repo-wide search that nothing references these, then remove and update `views/mod.rs`:
- packages/web-ui/src/views/cves_old.rs
- packages/web-ui/src/views/systems_mock.rs
- packages/web-ui/src/views/systems_mock_data.rs
- packages/web-ui/src/views/systems_mock_data_extra.rs
- packages/web-ui/src/views/flakes_list.rs (only if FlakesView uses flakes.rs)
- packages/web-ui/src/views/environments_list.rs (only if unused)
- packages/web-ui/src/views/policies_api.rs (only if unused)
- packages/web-ui/src/views/register_api.rs (only if unused)

If any file IS still referenced, do NOT delete it; instead note the reference and leave it.

## Exact scope — backlog metadata
- Resolve duplicate active task IDs (e.g. duplicate TASK-303/TASK-327 records) by archiving the stale/malformed one.
- Normalize duplicate milestone identifiers (two m-16 entries).

## Non-goals
- No view redesign or behavior changes.

## Verification
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- git status shows only intended deletions/edits
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Confirmed-unused legacy web-ui files are removed and views/mod.rs is updated
- [ ] #2 Any still-referenced candidate file is left in place with a note explaining why
- [ ] #3 Duplicate active task IDs are resolved or archived so MCP task operations are unambiguous
- [ ] #4 Duplicate milestone identifiers/titles are normalized
- [ ] #5 web-ui cargo check (wasm) and nix build .#checks.x86_64-linux.web-ui pass after cleanup
<!-- AC:END -->
