---
id: TASK-338.2
title: 'System Detail: Deploy, CVEs, and Config tab parity'
status: Backlog
assignee: []
created_date: '2026-06-10 13:31'
labels:
  - design-parity
  - system-detail
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-338
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1692
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of System Detail umbrella TASK-338. Follow guide doc-14 standard procedure.

## Problem
The Deploy, CVEs, and Config tabs of System Detail must match `CrystalForgelatest/components/SystemDetail.jsx` and be backed by real API data.

## Goal
Bring the Deploy, CVEs, and Config tabs to visual + interaction parity with real backend actions/data.

## Exact scope
1. Deploy tab: commit list + deploy/generation actions call the real API; loading/success/error states; matches design.
2. CVEs tab: grouped/justification view matches design (coordinate with existing step `12h-system-detail-cves-grouped-justification`).
3. Config tab: shows real config/store-path info per design; no fabricated values.

## Non-goals
- History tab (TASK-268), Logs tab (TASK-277), tab icons (TASK-295), Overview/tab-bar (sibling TASK-338.x), Hardening (merged).

## Files
- packages/web-ui/src/views/system_detail.rs
- packages/web-ui/src/components/system/deploy_system_modal.rs (if reused)
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `12j-system-detail-deploy-generation-list` and `12h-system-detail-cves-grouped-justification`; add a Config-tab screenshot.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Deploy tab commit list + deploy/generation actions use the real API with loading/success/error states
- [ ] #2 CVEs tab grouped/justification view matches the design
- [ ] #3 Config tab shows real config/store-path info with no fabricated values
- [ ] #4 web-ui steps capture Deploy, CVEs, and Config tabs and assert a real interaction
- [ ] #5 cargo fmt, web-ui cargo check (wasm), and nix build .#checks.x86_64-linux.web-ui pass
<!-- AC:END -->
