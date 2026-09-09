---
id: TASK-447
title: >-
  Scale evaluation policy results by compliance bundle and preserve evidence
  navigation
status: To Do
assignee: []
created_date: '2026-08-31 02:22'
updated_date: '2026-08-31 02:23'
labels:
  - web-ui
  - server
  - evaluations
  - compliance
  - design-parity
dependencies:
  - TASK-433
  - TASK-440
  - TASK-441
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/323'
documentation:
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/components/EvalsView.jsx
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/app.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/default/crates/cf-server/src/
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
type: feature
ordinal: 458000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After TASK-433 and TASK-440 merge, bring the evaluation policy drawer into parity with the bundle-scale matrix in design commit ac582592. Real compliance bundles can contain more than 100 controls, so the evaluation contract and UI must provide bundle rollups, a bounded control view, and exact evidence navigation without relying on policy-name prefixes or client-side inference. Returning from compliance evidence must restore the evaluation drawer state and selected revision.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The evaluation policy-matrix API identifies each control by stable policy identity and provides its assigned compliance bundle identity and display metadata without name-prefix inference
- [ ] #2 The policy drawer defaults to one aggregate column per real bundle when the result set is bundle-scale and reports pass warning fail and not-assigned counts truthfully per system
- [ ] #3 Operators can switch to individual control columns and hide controls that pass for every currently visible non-greenfield system without changing underlying result counts
- [ ] #4 Row filters sorting bundle rollups control filters and expanded failure details remain correct for bundles larger than 100 controls and systems with incomplete or legacy results
- [ ] #5 Selecting a bundle rollup reveals its controls without discarding the selected system revision or existing filters
- [ ] #6 A failed bundled control can open the exact system bundle and control in Compliance evidence and closing evidence restores the prior evaluation tab expanded row detail and column mode
- [ ] #7 Opening a system from an evaluation preserves the exact evaluation commit and opens Config for that revision when available
- [ ] #8 Unknown missing hidden or unauthorized policy and bundle identities do not disclose protected metadata and render explicit unavailable states
- [ ] #9 The matrix and evidence round trip support accessible keyboard operation focus restoration and maximized or narrow drawer layouts without impractical thousands-cell rendering
- [ ] #10 Focused server and frontend tests plus the authoritative web-ui check pass with large-bundle partial-result navigation restoration error keyboard light dark and narrow coverage and screenshot evidence
<!-- AC:END -->
