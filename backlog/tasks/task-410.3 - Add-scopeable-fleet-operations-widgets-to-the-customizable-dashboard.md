---
id: TASK-410.3
title: Add scopeable fleet operations widgets to the customizable dashboard
status: To Do
assignee: []
created_date: '2026-08-31 02:21'
labels:
  - dashboard
  - web-ui
  - design-parity
  - fleet-operations
dependencies:
  - TASK-410.2
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - TASK-410.2
documentation:
  - docs/design/CrystalForge/components/DashboardWidgetsOps.jsx
  - docs/design/CrystalForge/components/DashboardView.jsx
  - docs/design/CrystalForge/data-dashboard.js
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/dashboard/
  - packages/web-ui/src/components/widget_grid.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
parent_task_id: TASK-410
priority: high
type: feature
ordinal: 454000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Bring the Rust dashboard into parity with the fleet-operations dashboard changes in design commit ac582592 after real telemetry contracts are available. Add Configuration Drift, Closure and Disk Pressure, Rollback Readiness, Deploy State, Reboot Required, and Fleet Year widgets. Preserve saved dashboards through a versioned migration, support multiple scoped instances where the design allows them, and render explicit loading, unavailable, empty, and partial-data states without fixture-derived values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The dashboard provides Configuration Drift Closure and Disk Pressure Rollback Readiness Deploy State Reboot Required and Fleet Year widgets backed only by the real fleet-operations API
- [ ] #2 Each widget matches the authoritative design's labels hierarchy status thresholds legends rows and responsive behavior in light and dark themes
- [ ] #3 Host-scoped widgets can be limited to all visible systems or one visible environment and the active scope is clear in both customization controls and rendered content
- [ ] #4 Scopeable widgets can be added more than once with stable instance identity so different environment scopes coexist without remove resize reorder or persistence collisions
- [ ] #5 Fleet Year supports combined compliance-only and drift-only metrics per widget and retains its selected metric
- [ ] #6 Saved version-two dashboard layouts migrate additively without duplicate instances and retired prototype-only cache-hit and secret-expiry widgets are not introduced
- [ ] #7 Width height scope metric reorder add remove reset and picker interactions persist and restore correctly across reloads
- [ ] #8 Loading unknown partial-data empty error and authorization-scoped states are explicit and no missing backend value is replaced with fixture or deterministic mock data
- [ ] #9 Keyboard focus controls accessible labels and narrow layout remain usable for every new customization and widget interaction
- [ ] #10 The WASM check and authoritative web-ui check pass with assertion-based desktop narrow light dark customization persistence and navigation coverage plus MR screenshot evidence
<!-- AC:END -->
