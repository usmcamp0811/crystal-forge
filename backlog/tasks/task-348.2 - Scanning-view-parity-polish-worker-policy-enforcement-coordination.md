---
id: TASK-348.2
title: 'Scanning: view parity polish + worker policy enforcement coordination'
status: Backlog
assignee: []
created_date: '2026-06-10 13:34'
labels:
  - design-parity
  - scanning
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-348
  - TASK-327
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/ScanningView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/scanning.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1772
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of CVEs/Scanning umbrella TASK-348. Follow guide doc-14 standard procedure.

## Problem
The Scanning view (`views/scanning.rs`) already exists (merged via MR !267) but needs a parity polish pass; and the backend worker enforcement of the schedule policy is tracked separately (TASK-327).

## Goal
Polish the Scanning view to full parity with `CrystalForgelatest/components/ScanningView.jsx` and ensure the schedule controls reflect real policy state, while leaving worker enforcement to TASK-327.

## Exact scope
1. Verify Scanning view layout/stats/queue/per-system rows match the design within doc-8 tolerances.
2. Confirm schedule modal binds to live GET policy and saves via PUT.
3. Ensure freshness/status icons match design semantics using real data.
4. Note any worker behavior gaps and ensure they are captured under TASK-327 (do not implement worker logic here).

## Non-goals
- CVEs view (sibling TASK-348.x).
- Worker scheduling logic (TASK-327).

## Files
- packages/web-ui/src/views/scanning.rs
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse/extend step `16c-scanning-view`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Scanning view layout/stats/queue/per-system rows match the design within doc-8 tolerances
- [ ] #2 Schedule modal binds to live GET policy and saves via PUT
- [ ] #3 Freshness/status icons match design semantics using real data
- [ ] #4 Any worker behavior gaps are captured under TASK-327 (not implemented here)
- [ ] #5 Step 16c-scanning-view passes with parity assertions
<!-- AC:END -->
