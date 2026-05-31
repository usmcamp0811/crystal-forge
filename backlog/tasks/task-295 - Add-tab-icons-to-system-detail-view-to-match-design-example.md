---
id: TASK-295
title: Add tab icons to system detail view to match design example
status: Backlog
assignee: []
created_date: '2026-05-10 13:28'
updated_date: '2026-05-31 16:04'
labels:
  - ui
  - design-system
  - icons
milestone: m-16
dependencies:
  - TASK-328
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
System detail tabs currently use inline SVG snippets and mixed sizing, which diverges from the design system icon contract and creates inconsistent visual behavior across tabs.

## Goal
Replace inline tab icons with shared Icon component usage and deliver exact icon mapping/size parity with the design example.

## Non-Goals
- No redesign of system detail tab structure or navigation semantics.
- No unrelated styling refactors outside tab icon rendering.
- No backend/API behavior changes.

## Scope
- Extend `packages/web-ui/src/components/icon.rs` with missing tab icon variants required by system detail tabs.
- Replace inline SVG icon rendering in `packages/web-ui/src/views/system_detail.rs` with Icon component calls.
- Normalize icon size and alignment to match design source.
- Ensure badge/layout behavior remains intact after icon replacement.

## Architectural Constraints
- Keep presentation concerns in UI components; no business logic in tab rendering.
- Reuse shared icon primitives (no new per-view inline SVG blocks).
- Preserve existing tab-state/data-flow architecture.

## Verification Plan
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix build .#checks.x86_64-linux.web-ui`
- Update `checks/web-ui` to assert icon presence/mapping for all affected tabs.
- Capture screenshot evidence for all affected system-detail tab states.

## Impact Areas
- `packages/web-ui/src/components/icon.rs`
- `packages/web-ui/src/views/system_detail.rs`
- `checks/web-ui/**`

## Risk Level
Medium (localized UI refactor with high visual-sensitivity requirements).

## Dependencies
- Uses milestone parity specification and verification harness requirements (TASK-328 and TASK-333).
- Requires no unresolved backend dependencies.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Icon component has Dashboard variant added
- [ ] #2 Icon component has Deploy variant added
- [ ] #3 Icon component has History variant added
- [ ] #4 Icon component has Shield variant added
- [ ] #5 Icon component has Key variant added
- [ ] #6 Icon component has File variant added
- [ ] #7 All tab icons use Icon component instead of inline SVG
- [ ] #8 Tab icons are size={13} matching design example
- [ ] #9 Overview tab uses Dashboard icon
- [ ] #10 Deploy tab uses Deploy icon
- [ ] #11 History tab uses History icon
- [ ] #12 CVEs tab uses Shield icon
- [ ] #13 Hardening tab uses Key icon
- [ ] #14 Logs tab uses Terminal icon
- [ ] #15 Config tab uses File icon
- [ ] #16 Visual appearance matches design example
- [ ] #17 Icons render correctly in all tabs
- [ ] #18 web-ui check updates are required with assertions for tab-icon rendering and state behavior
- [ ] #19 web-ui check must capture screenshots for all affected system-detail tab states
<!-- AC:END -->
