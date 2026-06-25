---
id: TASK-369
title: Add System Detail compliance evidence drawer View bundle navigation
status: Backlog
assignee: []
created_date: '2026-06-25 23:56'
labels:
  - compliance
  - system-detail
  - ui
  - follow-up
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-356
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/286'
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/components/compliance/mod.rs
  - packages/web-ui/src/views/compliance.rs
priority: medium
ordinal: 0
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-356/MR !286 replaced the System Detail Compliance tab with real API-backed data, but the acceptance item for a `View bundle` navigation affordance in the evidence drawer was left as a future improvement because the shared `EvidenceDrawer` component did not expose that button/action.

## Desired Outcome

When evidence is opened from the System Detail Compliance tab, users can navigate from the evidence drawer to the Compliance view for the relevant bundle, preferably with the bundle pre-selected via a supported query parameter or equivalent route state.
<!-- SECTION:DESCRIPTION:END -->
