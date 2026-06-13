---
id: TASK-353
title: Close full Systems surface parity gaps against CrystalForgelatest
status: Review
assignee: []
created_date: '2026-06-13 14:53'
updated_date: '2026-06-13 20:28'
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
  - TASK-355
  - TASK-356
documentation:
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/Systems.jsx
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
modified_files:
  - packages/default/migrations/0135_add_system_reachability.sql
  - packages/default/src/api/models.rs
  - packages/default/src/handlers/api/systems.rs
  - packages/default/src/queries/systems.rs
  - packages/default/src/services/systems.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/views/system_detail.rs
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
- Replacing authoritative backend data with mock-only UI shortcuts in production paths, except where explicitly authorized below

## Explicit Mock/Placeholder Authorization
On 2026-06-13, the maintainer explicitly authorized temporary mock/placeholder data for System Detail parity gaps that do not currently have backend support, specifically to complete the Compliance tab surface while the real Compliance view/backend plumbing is deferred. Any such mock data must be clearly commented in code, documented in task notes, and tracked by follow-up Backlog tasks to replace with real backend data.

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
- Reachability field persisted and surfaced for SSH modal/host details

### C. Data + verification parity
- Ensure displayed values come from authoritative backend APIs in production paths unless explicitly covered by the temporary mock authorization above
- Add/expand screenshot and assertion coverage so the Systems list and Systems detail surfaces are both provably covered by `checks/web-ui`
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->
<!-- AC:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems list header, stat strip, filters, cards mode, and table mode materially match CrystalForgelatest on desktop
- [ ] #2 Systems list side panel and add/edit/deploy modal flows materially match CrystalForgelatest on desktop
- [ ] #3 Systems list loading, empty, error, and populated states are styled and behaved per the reference with no production-path mock fallback rendering
- [ ] #4 System detail header, metric strip, badges/chips, and action cluster materially match CrystalForgelatest on desktop
- [ ] #5 System detail tab rail matches the reference in structure, ordering, iconography, active states, and badge treatment
- [ ] #6 System detail Overview, Deploy, History, Logs, Config, CVEs, and Hardening surfaces materially match the reference for core states
- [ ] #7 All displayed Systems list and Systems detail values are sourced from authoritative backend APIs in production paths unless explicitly tracked as backend follow-up gaps or authorized temporary Compliance placeholder data
- [ ] #8 `checks/web-ui` captures screenshot evidence and behavior assertions for the full Systems surface, including both `/systems` and `/systems/{id}`
- [ ] #9 A human reviewer can compare the implemented Systems surface against the CrystalForgelatest reference and find no remaining material parity gaps
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Mocked data introduced under explicit maintainer authorization: System Detail Compliance tab bundle rollups only (`mocked_compliance_bundles()` in `packages/web-ui/src/views/system_detail.rs`). Mocked fields: applicable bundle names, framework/version labels, owner, control counts, score percentage, pass/warn/fail/waiver counts, and placeholder View evidence action. Follow-ups created: TASK-355 replaces mock bundle rollups with real backend API data; TASK-356 wires View evidence to real per-control evidence UI. Reachability is not mocked: added persisted `systems.reachability` DB field projected through `view_system_detail` and API/Web UI models.
<!-- SECTION:NOTES:END -->
