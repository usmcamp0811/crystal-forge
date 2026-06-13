---
id: TASK-353
title: Close full Systems surface parity gaps against CrystalForgelatest
status: In Progress
assignee: []
created_date: '2026-06-13 14:53'
updated_date: '2026-06-13 15:20'
labels:
  - design-parity
  - systems
  - system-detail
  - web-ui
  - umbrella
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies:
  - TASK-328
  - TASK-329
  - TASK-333
references:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/Systems.jsx
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
  - TASK-330
  - TASK-338
  - TASK-295
  - TASK-281
documentation:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/Systems.jsx
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
modified_files:
  - packages/web-ui/src/views/systems_list.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/system
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1695
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem
The Systems user experience is currently tracked across multiple partial tasks, but there is no single sprint-ready execution record that guarantees the **entire Systems surface** matches the CrystalForgelatest design example end-to-end. In practice, list-view parity, side-panel/modal parity, and detail-page parity are being reviewed together by humans, and gaps on one sub-surface can block acceptance of the whole Systems experience.

## Goal
Bring the full Systems surface into parity with the CrystalForgelatest reference across **both** `/systems` and `/systems/{id}` so a reviewer can compare the implemented UI against the design example and find no material visual or interaction discrepancies on the core desktop flows.

References:
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/app.jsx`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/Systems.jsx`
- `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx`
- Existing related tasks: TASK-330, TASK-338, TASK-295, TASK-281

## Non-Goals
- New product workflows that do not exist in the design example
- Unrelated backend refactors outside parity-driven API/data needs
- Mobile-first redesign beyond responsive behavior already implied by the reference
- Replacing authoritative backend data with mock-only UI shortcuts in production paths

## Scope
This task covers the **full Systems surface**:

### A. Systems list surface (`/systems`)
- Page header, subtitle, stat strip, filter/search bar, view toggle, count text
- Cards mode and table mode geometry, typography, spacing, chip treatment, badge treatment, selected state, density behavior
- Side-panel preview layout and actions
- Add / Edit / Deploy / remove-related modal visual parity and interaction parity
- Loading, empty, error, and populated states

### B. System detail surface (`/systems/{id}`)
- Header, breadcrumb, host title block, badges/chips, action set, metric strip
- Tab rail structure, ordering, icons, badges, active states, sticky behavior
- Overview tab parity
- Deploy tab parity
- History tab parity
- Logs tab parity
- Config tab parity
- CVEs tab parity
- Hardening tab parity
- Compliance-tab entry/placement parity if present in the reference for this surface

### C. Data + verification parity
- Ensure displayed values come from authoritative backend APIs in production paths
- Add/expand screenshot and assertion coverage so the Systems list and Systems detail surfaces are both provably covered by `checks/web-ui`

## Architectural Constraints
- Keep business logic out of views; views compose existing components and adapters
- Reuse shared primitives (Icon, Chip, modal shells, table/list patterns) instead of view-local one-off SVGs or styling hacks
- Preserve clear separation between list-surface concerns and detail-surface concerns even if both are delivered under one umbrella task
- Any backend-dependent placeholder UI must either be fully wired to real data or be explicitly labeled as a temporary placeholder with follow-up tracking
- No hidden global state or route-coupled side effects beyond established patterns in `packages/web-ui/src/views/**`

## Verification Plan
Minimum required verification for acceptance:
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check`
- `nix build .#checks.x86_64-linux.web-ui`
- Capture updated screenshot evidence for both `/systems` and `/systems/{id}` core states
- Extend `checks/web-ui/tests/integration-test.js` with assertions covering:
  - list view cards + table modes
  - side panel open state
  - edit/deploy modal presence and core controls
  - system detail tab rail
  - overview/deploy/history/logs/config/cves/hardening core render states

## Impact Areas
- `packages/web-ui/src/views/systems_list.rs`
- `packages/web-ui/src/views/system_detail.rs`
- `packages/web-ui/src/components/system/**`
- `packages/web-ui/src/components/tables/systems_table.rs`
- `packages/web-ui/src/components/systems_stat_strip.rs`
- `packages/web-ui/src/components/icon.rs`
- `packages/web-ui/src/systems/adapter.rs`
- `checks/web-ui/tests/integration-test.js`
- Design/parity backlog tasks that should be closed or linked as work completes

## Risk Level
High

Why high:
- The surface spans two large user-facing routes with many states
- Visual parity is highly review-sensitive
- Some remaining gaps may require coordinated backend data exposure or careful placeholder handling

## Dependencies
- TASK-328 (parity spec/foundation)
- TASK-329 (shared shell/foundation parity)
- TASK-333 (strict parity verification harness)
- May absorb or close residual scope currently tracked in TASK-330 and TASK-338 once implementation is complete and reviewed

## Acceptance Criteria
- [ ] Systems list header, stat strip, filters, cards mode, and table mode materially match CrystalForgelatest on desktop
- [ ] Systems list side panel and add/edit/deploy modal flows materially match CrystalForgelatest on desktop
- [ ] Systems list loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback rendering
- [ ] System detail header, metric strip, badges/chips, and action cluster materially match CrystalForgelatest on desktop
- [ ] System detail tab rail matches the reference in structure, ordering, iconography, active states, and badge treatment
- [ ] System detail Overview, Deploy, History, Logs, Config, CVEs, and Hardening surfaces materially match the reference for core states
- [ ] All displayed Systems list and Systems detail values are sourced from authoritative backend APIs in production paths unless explicitly tracked as backend follow-up gaps
- [ ] `checks/web-ui` captures screenshot evidence and behavior assertions for the full Systems surface, including both `/systems` and `/systems/{id}`
- [ ] A human reviewer can compare the implemented Systems surface against the CrystalForgelatest reference and find no remaining material parity gaps
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on reckless in ~/code/crystal-forge/TASK-353-full-systems-surface-parity
<!-- SECTION:NOTES:END -->
