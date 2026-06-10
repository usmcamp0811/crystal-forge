---
id: TASK-349.1
title: 'Caches: view parity + remove deterministic mock storage'
status: Backlog
assignee: []
created_date: '2026-06-10 13:34'
labels:
  - design-parity
  - caches
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-349
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/CachesView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/caches.rs
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-349
priority: high
ordinal: 1801
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of Caches umbrella TASK-349. Follow guide doc-14 standard procedure.

## Problem
The Caches view (`views/caches.rs`) generates deterministic MOCK storage values per cache ID (around line ~1552: "Generate deterministic mock storage based on cache ID"). Production must show real values or a truthful unknown state.

## Goal
Remove fabricated storage/usage values from the production path and confirm the Caches list/modals remain at design parity (view already largely landed via TASK-303/MR !257).

## Exact scope
1. Replace deterministic mock storage with real API values, or render a clear "unknown" placeholder when unavailable.
2. Verify list, stat strip, filter bar, and add/edit/credential modals still match `CrystalForgelatest/components/CachesView.jsx`.
3. No fabricated paths-cached/usage numbers in production.

## Non-goals
- Credential-test SSRF hardening (already done in TASK-303).

## Files
- packages/web-ui/src/views/caches.rs
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse steps `21-caches`, `22/23/24/25-caches-modal-*`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deterministic mock storage values are removed from the production path
- [ ] #2 Storage/usage render real API values or a truthful unknown placeholder
- [ ] #3 Caches list/stat strip/filter bar/modals match the design
- [ ] #4 Caches web-ui steps pass with parity assertions
<!-- AC:END -->
