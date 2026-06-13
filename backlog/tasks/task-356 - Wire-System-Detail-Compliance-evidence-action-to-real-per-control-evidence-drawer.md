---
id: TASK-356
title: >-
  Wire System Detail Compliance evidence action to real per-control evidence
  drawer
status: Backlog
assignee: []
created_date: '2026-06-13 20:28'
labels:
  - compliance
  - system-detail
  - ui
  - api-integration
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies:
  - TASK-355
references:
  - TASK-353
  - TASK-355
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

TASK-353 renders a System Detail Compliance tab using maintainer-authorized temporary mock data. The design reference includes a `View evidence` action that opens a per-control evidence drawer. In TASK-353, the `View evidence` button is intentionally a placeholder with a title noting it is temporary.

## Desired Outcome

Implement real evidence navigation/drawer behavior for System Detail Compliance bundles. Opening a bundle should show per-control evidence for the selected system, including collected proof such as config output, systemd unit state, audit results, and waivers when available.

## Notes

- Follow-up from TASK-353.
- Related to TASK-355, which replaces mock Compliance bundle rollups with real backend data.
- Keep the current design-parity layout and replace only the placeholder behavior/data path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 View evidence opens real evidence UI for the selected bundle and system
- [ ] #2 Evidence content is API-backed and no mock evidence remains in production render path
- [ ] #3 Drawer/action handles loading, empty, and error states
- [ ] #4 web-ui check covers opening evidence from System Detail Compliance
<!-- AC:END -->
