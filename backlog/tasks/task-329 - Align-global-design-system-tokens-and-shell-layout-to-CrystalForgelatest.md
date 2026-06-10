---
id: TASK-329
title: 'Foundation: shell, tokens, topbar, and sidebar parity to CrystalForgelatest'
status: Backlog
assignee: []
created_date: '2026-05-31 15:56'
updated_date: '2026-06-10 13:26'
labels:
  - design-parity
  - design-system
  - web-ui
  - foundation
milestone: m-18
dependencies:
  - TASK-328
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/styles.css
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/Shell.jsx
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/theme.rs
  - packages/web-ui/src/components/layout/sidebar.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: shared shell primitives drift from CrystalForgelatest, which blocks reliable pixel parity on every downstream surface.

Goal: bring global tokens + shell chrome (sidebar groups, topbar, base primitives) into parity so all later view work is fast and consistent.

This is the FIRST task to execute. See guide doc-14 for the standard procedure.

## Exact scope (do all of these)
1. Tokens: make `packages/web-ui/assets/app.css` token values match `CrystalForgelatest/styles.css` exactly (colors, radii 6/10/14/16/999, font sizes, shadows, focus ring) for BOTH dark and light.
2. Sidebar groups: align `components/layout/sidebar.rs` section grouping to the design in `components/Shell.jsx`: Fleet (Dashboard, Systems, Flakes, Environments), Pipeline (Builds, Evaluations, Scanning), Compliance (CVEs, Policies, Compliance), System (Builders, Caches, Server/Admin). Keep existing role-gating.
3. Topbar: in `components/layout/topbar.rs`, add the notifications dropdown panel matching `Shell.jsx` Topbar (bell + unread badge + panel list + settings link). Keep theme + tweaks behavior.
4. Primitives: verify `.btn`, `.input`, `.card`, `.chip`, `.env-badge` render to the design CSS values; remove duplicate/conflicting definitions.

## Non-goals
- No per-view business logic changes.
- Classification banners are OUT of scope here (separate task if desired).

## Files
- packages/web-ui/assets/app.css
- packages/web-ui/src/theme.rs
- packages/web-ui/src/components/layout/sidebar.rs
- packages/web-ui/src/components/layout/topbar.rs

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Add/confirm web-ui steps that screenshot sidebar (expanded/collapsed) and topbar notifications open, in both themes.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Shared color/spacing/typography/radius/shadow tokens match CrystalForgelatest values for dark and light
- [ ] #2 Sidebar section grouping matches the design (Fleet / Pipeline / Compliance / System) while preserving role gating
- [ ] #3 Topbar includes a working notifications dropdown matching Shell.jsx (bell, unread badge, panel, settings link)
- [ ] #4 No duplicate conflicting token/primitive definitions remain for btn/input/card/chip/env-badge
- [ ] #5 web-ui check captures shell + topbar screenshots for both themes and asserts notifications panel opens
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
First execution task. Foundation for all surface parity.
<!-- SECTION:NOTES:END -->
