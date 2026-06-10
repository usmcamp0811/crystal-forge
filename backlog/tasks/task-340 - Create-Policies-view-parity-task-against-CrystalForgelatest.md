---
id: TASK-340
title: Create Policies view parity task against CrystalForgelatest
status: Backlog
assignee: []
created_date: '2026-06-10 02:58'
labels:
  - design-parity
  - policies
  - web-ui
  - api-integration
milestone: m-20
dependencies: []
references:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/PoliciesView.jsx
modified_files:
  - packages/web-ui/src/views/policies.rs
  - checks/web-ui/tests/integration-test.js
priority: high
ordinal: 1700
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: Policies functionality exists, but the backlog does not currently have a dedicated task to bring the Policies surface into full parity with the CrystalForgelatest design reference.

Desired Outcome: A dedicated parity task exists for the Policies view so layout, filters, chips, rule affordances, and backend-backed states can be closed deliberately under the new parity plan.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policies view layout and controls match CrystalForgelatest
- [ ] #2 Primary Policies content and actions are backend-driven in production path
- [ ] #3 web-ui checks include screenshot and assertion coverage for Policies states
<!-- AC:END -->
