---
id: TASK-356
title: Wire System Detail Compliance tab to real backend data and evidence drawer
status: Review
assignee: []
created_date: '2026-06-13 20:28'
updated_date: '2026-06-25 21:11'
labels:
  - compliance
  - system-detail
  - ui
  - api-integration
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies:
  - TASK-334
references:
  - TASK-353
  - TASK-355
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/src/components/compliance/mod.rs
documentation:
  - >-
    /home/mcamp/code/crystal-forge/CrystalForgelatest/components/SystemDetail.jsx
priority: high
ordinal: 299000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The System Detail Compliance tab (`ComplianceTab` in `system_detail.rs`) currently renders entirely from `mocked_compliance_bundles()` — hardcoded data authorized as a temporary placeholder in TASK-353. The evidence drawer (`ComplianceEvidenceDrawer`) likewise uses four hardcoded sample controls.

TASK-334 has now delivered real compliance bundle APIs (`GET /api/v1/compliance/bundles`, `/systems`, `/evidence`) and a full evidence data model. The System Detail Compliance tab should use these APIs rather than mock data.

This task absorbs TASK-355 (replace mock rollups) and TASK-356 (wire real evidence drawer) since they are tightly coupled and the blocker (TASK-334 backend) is now resolved.

## Goal

Replace every mock/placeholder in `ComplianceTab` and `ComplianceEvidenceDrawer` with real API-backed data, reusing the components already built for the Compliance view (`EvidenceDrawer`, `ControlEvidenceCard`) so there is no duplicated evidence-rendering logic.

## Non-Goals

- Redesigning the system detail page layout or other tabs
- Adding new compliance bundle management from this tab (create/edit/delete belongs on the Compliance view)
- Changing the Compliance view itself

## Architectural Constraints

- `ComplianceTab` lives in `packages/web-ui/src/views/system_detail.rs`
- Reuse `EvidenceDrawer` and `ControlEvidenceCard` from `packages/web-ui/src/components/compliance/mod.rs` — do not duplicate them
- API client calls already exist: `fetch_compliance_bundles`, `fetch_compliance_bundle_systems`, `fetch_compliance_system_evidence`
- The "View bundle" button in the drawer header should deep-link to `/compliance` with the bundle pre-selected (pass via router state or query param); if the router cannot carry state today, navigate to `/compliance` and let the user select the bundle — do not block this task on that
- Remove `mocked_compliance_bundles`, `ComplianceMockBundle`, and `ComplianceEvidenceDrawer` (the placeholder) once replaced
- No new migrations or backend changes are required — TASK-334 delivered the full backend
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 ComplianceTab fetches applicable compliance bundles for the current system via fetch_compliance_bundle_systems, and renders real bundle name, framework, version, owner, control counts, and score
- [x] #2 Pass/warn/fail/waiver counts and score bar are driven by ComplianceSystemRollup from the API — no mock values
- [x] #3 "View evidence" opens the real EvidenceDrawer component (components/compliance/mod.rs) loaded with fetch_compliance_system_evidence for the selected bundle + system
- [ ] #4 The evidence drawer "View bundle" button navigates to /compliance, deep-linking to the bundle if feasible via router query param
- [x] #5 Loading, empty (no applicable bundles), and error states are handled and rendered
- [x] #6 The sd-callout-info preview banner, mocked_compliance_bundles, ComplianceMockBundle, and the placeholder ComplianceEvidenceDrawer are fully removed from the production render path
- [x] #7 nix build .#packages.x86_64-linux.web-ui passes with no new warnings introduced by this task
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
CI repair plan for MR !286:
1. Keep the existing `pub(crate)` visibility for internal compliance assembly helpers and row types; do not widen production API surface just for tests.
2. Move the behavioral tests currently in `packages/default/tests/system_compliance_test.rs` into a `#[cfg(test)]` unit test module inside `packages/default/src/queries/compliance.rs`, where `pub(crate)`/private internals are accessible.
3. Remove unused imports and the now-invalid integration test file.
4. Run targeted verification with `nix develop -c cargo test --manifest-path packages/default/Cargo.toml system_compliance` or the exact unit test names, then run the relevant Nix check/build for the failed CI path before pushing.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
CI fix session: LOCK confirmed by user as sole active agent. Working in `/home/mcamp/code/crystal-forge/TASK-356-wire-system-detail-compliance` on branch `TASK-356-wire-system-detail-compliance`. Current CI root cause from MR !286 pipeline #2627895427: `packages/default/tests/system_compliance_test.rs` is an integration test importing `pub(crate)` helpers/types from `queries::compliance`, causing E0603 private item compile failures in `flake-check: [web-ui]` and cascading dependent checks.
<!-- SECTION:NOTES:END -->
