---
id: TASK-295
title: Add tab icons to system detail view to match design example
status: Review
assignee: []
created_date: '2026-05-10 13:28'
updated_date: '2026-06-13 14:22'
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
ordinal: 1000
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
- [x] #1 Icon component has Dashboard variant added
- [x] #2 Icon component has Deploy variant added
- [x] #3 Icon component has History variant added
- [x] #4 Icon component has Shield variant added
- [x] #5 Icon component has Key variant added
- [x] #6 Icon component has File variant added
- [x] #7 All tab icons use Icon component instead of inline SVG
- [x] #8 Tab icons are size={13} matching design example
- [x] #9 Overview tab uses Dashboard icon
- [x] #10 Deploy tab uses Deploy icon
- [x] #11 History tab uses History icon
- [x] #12 CVEs tab uses Shield icon
- [x] #13 Hardening tab uses Key icon
- [x] #14 Logs tab uses Terminal icon
- [x] #15 Config tab uses File icon
- [x] #16 Visual appearance matches design example
- [x] #17 Icons render correctly in all tabs
- [x] #18 web-ui check updates are required with assertions for tab-icon rendering and state behavior
- [x] #19 web-ui check must capture screenshots for all affected system-detail tab states
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on reckless in ~/code/crystal-forge/TASK-295-system-detail-tab-icons

Implemented: added Dashboard/Deploy/History/Key/File variants to IconName (Shield+Terminal already existed) with design-parity SVG paths from CrystalForgelatest/components/Icon.jsx. Replaced inline tab SVGs in system_detail.rs tab rail with Icon component at size=13. Added web-ui check step 12k-system-detail-tab-icons asserting each of the 7 tabs renders an SVG icon at width=13, plus added it to CI_FAST set. Verified via local Playwright screenshot harness that all 7 tab icons render correctly with active state + CVE badge preserved. cargo check + cargo fmt clean. NOTE: cargo clippy surfaces pre-existing doc-comment-on-param errors in add_system_form.rs/key_pair_modal.rs (not my files) -> tracked as TASK-352.

MR opened: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/274 (TASK-295-system-detail-tab-icons -> dev). Moved to Review.
<!-- SECTION:NOTES:END -->
