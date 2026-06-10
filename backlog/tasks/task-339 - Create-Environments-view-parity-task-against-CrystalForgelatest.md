---
id: TASK-339
title: Create Environments view parity task against CrystalForgelatest
status: Backlog
assignee: []
created_date: '2026-06-10 02:58'
labels:
  - design-parity
  - environments
  - web-ui
  - api-integration
milestone: m-20
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/EnvironmentsView.jsx
modified_files:
  - packages/web-ui/src/views/environments.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1710
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Environments functionality exists, but the backlog does not currently have a dedicated task to bring the Environments surface into full parity with the CrystalForgelatest design reference.

Desired Outcome: A dedicated parity task exists for the Environments view so layout, status chips, environment actions, and real-data states can be aligned deliberately under the new parity plan.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Environments view layout and controls match CrystalForgelatest
- [ ] #2 Primary Environments content and actions are backend-driven in production path
- [ ] #3 web-ui checks include screenshot and assertion coverage for Environments states
<!-- AC:END -->
