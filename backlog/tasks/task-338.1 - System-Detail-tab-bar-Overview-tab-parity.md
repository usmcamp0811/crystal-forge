---
id: TASK-338.1
title: 'System Detail: tab bar + Overview tab parity'
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
  - packages/web-ui/src/components/system/mod.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1691
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of System Detail umbrella TASK-338. Follow guide doc-14 standard procedure.

## Problem
The System Detail header, metric strip, and tab bar (Overview/Deploy/History/CVEs/Hardening/Logs/Config) plus the Overview tab must match `CrystalForgelatest/components/SystemDetail.jsx`.

## Goal
Pixel-align the System Detail shell (header, metrics, tabs) and the Overview tab content to the design, backed by real API data.

## Exact scope
1. Header + metric strip: hostname, status, env, generation, CVE count tiles match design.
2. Tab bar: order/labels/active state/badges match design; use shared Icon component (coordinate with TASK-295 for icons).
3. Overview tab: system info, hardware, network, security sections match design layout.
4. All values come from `SystemDetail` API data (no fabricated values).

## Non-goals
- History tab (tracked by TASK-268), Logs tab (TASK-277), tab icons (TASK-295), Hardening (already merged) — coordinate, don't duplicate.

## Files
- packages/web-ui/src/views/system_detail.rs
- packages/web-ui/src/components/system/**
- packages/web-ui/assets/app.css
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Extend steps `12i-system-detail-generation-metric` and add an Overview-tab screenshot/assertion.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Header + metric strip match the design and use real SystemDetail data
- [ ] #2 Tab bar order/labels/active state/badges match the design
- [ ] #3 Overview tab sections (info/hardware/network/security) match the design layout
- [ ] #4 No fabricated values render in the header or Overview tab
- [ ] #5 web-ui step screenshots the Overview tab and asserts the generation/CVE metrics
<!-- AC:END -->
