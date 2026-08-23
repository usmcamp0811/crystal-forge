---
id: TASK-433.2
title: >-
  TASK-433 Phase 1: Policy catalog scaling (chunking, collapse, selection, bulk
  delete)
status: To Do
assignee: []
created_date: '2026-08-23 01:42'
labels:
  - design-parity
  - policy
  - web-ui
  - server
  - phase-1
dependencies:
  - TASK-433.1
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/data-policies.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 434000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 1 of 8 (contextual only, not an execution-blocking dependency framework beyond the explicit dependency below). Implements the policy catalog scaling behavior from `PoliciesView.jsx`/`data-policies.js` in the design delta, preserving existing catalog pagination and deletion-eligibility server behavior.

Implement policy group collapse/expand, chunked rendering of large groups, search-aware collapse restoration, cards/table view parity, logical multi-select (individual/Shift-range/group/cross-chunk/clear), selected export, and server-reasoned bulk delete with partial/all-blocked/failure reporting.

## Explicit scope
- Groups independently collapse; groups larger than 150 policies default collapsed; visible group counts and selection state.
- Large groups initially render at most 60 items with current/total plus Show more / Show all.
- Search reveals matches inside collapsed groups; clearing search restores prior explicit collapse state.
- Cards and table views preserve equivalent policy semantics and logical selection state.
- Individual, Shift-range, group, cross-chunk, clear, selected export, and selected delete all operate on filtered logical order (not just rendered DOM order).
- Bulk delete uses server-side eligibility, reports deleted/skipped/reasons, handles partial success/all-blocked/failure, and preserves immutable-history blockers (do not weaken deletion/immutable-history rules).
- Preserve existing catalog API pagination; chunking/collapse is client-side rendering only.

## Explicit non-scope
No changes to policy editor, enforcement execution, Nix metadata, or POA&M. No fixtures/seeded data. Do not weaken deletion/immutable-history semantics.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#checks.x86_64-linux.web-ui --no-link
```
Add/extend a browser workflow proving deep search, collapse/expand, cards/table, and range selection with more than 60 policies (contributes to parent TASK-433 AC #32, finalized in TASK-433.9).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy groups independently collapse; groups larger than 150 default collapsed; group counts and selection state are visible.
- [ ] #2 Large groups initially render at most 60 items and provide current/total plus Show more and Show all.
- [ ] #3 Search reveals matches in collapsed groups and clearing search restores prior explicit collapse state.
- [ ] #4 Cards and table views preserve equivalent policy semantics and logical selection.
- [ ] #5 Individual, Shift-range, group, cross-chunk, clear, selected export and selected delete work on filtered logical order.
- [ ] #6 Bulk delete uses server eligibility, reports deleted/skipped/reasons, handles partial/all-blocked/failure, and preserves immutable blockers.
- [ ] #7 Existing catalog API pagination is preserved; chunking remains client rendering only.
<!-- AC:END -->
