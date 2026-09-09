---
id: TASK-445
title: Replace compliance bundle forms with the scalable sectioned bundle editor
status: To Do
assignee: []
created_date: '2026-08-31 02:21'
labels:
  - web-ui
  - compliance
  - policies
  - modals
  - design-parity
dependencies:
  - TASK-433
references:
  - git commit ac582592e8ffd787f103578c272d9f30162a9480
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
documentation:
  - docs/design/CrystalForge/components/BundleEditor.jsx
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/src/components/compliance/
  - packages/web-ui/src/components/policy/
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
priority: high
type: feature
ordinal: 456000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After TASK-433 merges, replace the flat compliance bundle create/edit forms with the sectioned bundle editor from design commit ac582592. The editor must scale to large real policy catalogs, support grouped catalog browsing and bulk membership changes, and preserve all merged bundle persistence, revision, requirement, authorization, deletion, and POA&M semantics. Creating a policy from inside the bundle workflow must return to the same draft without losing state.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bundle create/edit uses separate Basics and Controls sections with authoritative header context control count validation and footer state summary
- [ ] #2 Basics preserves bundle name version framework description applicable environments and all merged TASK-433 bundle requirements and revision semantics
- [ ] #3 Controls browse the real non-deprecated policy catalog by supported grouping schemes and show membership search severity framework identifiers descriptions and enforcement details without requiring navigation away
- [ ] #4 Operators can add or remove one control or all visible controls in a group and membership remains correct for catalogs larger than 100 controls
- [ ] #5 The editor can open the real policy authoring workflow and return to the same unsaved bundle draft with newly created policies available and selected when creation succeeds
- [ ] #6 Save persists the exact resolved control membership and requirements through existing authorized APIs and reports conflicts validation failures and transport errors without silently closing
- [ ] #7 Edit-only deletion retains merged preflight eligibility safeguards dependency messaging and confirmation behavior
- [ ] #8 Policy and bundle authorization publication-state filtering environment visibility and POA&M relationships from TASK-433 remain intact
- [ ] #9 Keyboard navigation focus trap nested policy-editor layering Escape order focus restoration and desktop/narrow light/dark layouts are accessible and match the design
- [ ] #10 Focused Rust tests and the authoritative web-ui check pass with assertion coverage and screenshots for large-catalog grouping bulk membership nested policy creation save error deletion and keyboard states
<!-- AC:END -->
