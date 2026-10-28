---
id: TASK-410.4
title: Add account-scoped named dashboards with portable import and export
status: Backlog
assignee: []
created_date: '2026-09-09 03:31'
labels:
  - dashboard
  - web-ui
  - api
  - persistence
  - design-parity
dependencies:
  - TASK-410.3
references:
  - git commit e1b7434899e23f43770632e59d80a76a8fc8459e
documentation:
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/default/crates/cf-server/
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/dashboard/
  - packages/web-ui/src/components/widget_grid.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-410
priority: high
type: feature
ordinal: 471000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Design commit `e1b74348` changes the customizable dashboard from one saved layout into a named dashboard book. Users need separate focused dashboards that persist per account and can be created, renamed, duplicated, deleted, switched, imported, and exported without losing existing layouts. Implement this as the dashboard-management follow-up to TASK-410 after the final widget registry and scopeable-instance contract from TASK-410.3 are available. Imported documents must be treated as untrusted data, and server-side account ownership must be authoritative rather than browser-only local storage.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Authenticated users can create switch rename duplicate and delete named dashboards and at least one dashboard always remains
- [ ] #2 Each dashboard persists its name widget instances order dimensions scope and metric under the authenticated user and cannot be read or changed by another user
- [ ] #3 Existing single-layout dashboard state migrates once into an Overview dashboard without duplicate widgets or loss of supported instance settings
- [ ] #4 The active dashboard and all dashboard-management operations survive reloads and concurrent account sessions with explicit save failures
- [ ] #5 Users can export one dashboard or all dashboards in a documented versioned JSON format that excludes runtime-only state and server identifiers
- [ ] #6 Users can import one dashboard or a dashboard bundle and receive an explicit report for invalid documents and unsupported widget types
- [ ] #7 Import validates schema sizes field ranges names widget identifiers scopes and metrics before persistence and does not trust imported instance identifiers
- [ ] #8 Unknown widget types are skipped without rejecting otherwise valid dashboards and no fixture-derived data is introduced
- [ ] #9 Dashboard tabs and create rename duplicate delete import and export controls are keyboard accessible and usable at desktop and narrow widths in light and dark themes
- [ ] #10 Focused server persistence authorization migration and import-validation tests plus the authoritative web-ui check pass with assertion and screenshot coverage
<!-- AC:END -->
