---
id: TASK-355
title: >-
  Replace System Detail Compliance mock bundles with real Compliance backend
  data
status: Backlog
assignee: []
created_date: '2026-06-13 20:27'
labels:
  - compliance
  - system-detail
  - backend
  - api-integration
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-353
  - packages/web-ui/src/views/system_detail.rs
documentation:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
priority: medium
ordinal: 299000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-353 explicitly authorized temporary mock/placeholder data for the System Detail Compliance tab to complete design parity before the real Compliance view/backend plumbing exists. The current `ComplianceTab` in `packages/web-ui/src/views/system_detail.rs` uses `mocked_compliance_bundles()` to render bundle rollups, score, pass/warn/fail/waiver counts, owner, framework, version, and control counts.

## Desired Outcome

Replace `mocked_compliance_bundles()` with API-backed Compliance bundle rollups scoped to the selected system. The System Detail Compliance tab should render real applicable bundles, scores, control status counts, owners, framework/version metadata, and a no-applicable-bundles empty state.

## Notes

- Created as follow-up from TASK-353 per maintainer authorization to temporarily mock missing backend data.
- The mock data is isolated and commented in `packages/web-ui/src/views/system_detail.rs`.
- Preserve the design-parity visual structure when replacing the data source.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 System Detail Compliance tab fetches real per-system applicable compliance bundles from backend API
- [ ] #2 Compliance score and pass/warn/fail/waiver counts are API-backed
- [ ] #3 No mock Compliance bundle data remains in production render path
- [ ] #4 Empty state renders when no bundles apply to the system
- [ ] #5 web-ui check covers at least one real-data Compliance tab state
<!-- AC:END -->
