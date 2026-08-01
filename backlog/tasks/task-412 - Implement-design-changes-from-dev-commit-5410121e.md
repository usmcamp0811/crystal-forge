---
id: TASK-412
title: Implement design changes from dev commit 5410121e
status: To Do
assignee: []
created_date: '2026-08-01 01:04'
updated_date: '2026-08-01 01:05'
labels:
  - design
  - frontend
  - web-ui
  - compliance
  - policies
  - scanning
dependencies: []
references:
  - docs/design/CrystalForge/components/ComplianceView.jsx
  - docs/design/CrystalForge/components/PoliciesView.jsx
  - docs/design/CrystalForge/components/ScanningView.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/styles.css
  - docs/design/crystal-forge-xccdf-interchange-profile-v0.1.md
  - packages/web-ui/
modified_files:
  - packages/web-ui/src/
priority: high
type: feature
ordinal: 400000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the design updates committed to `dev` at exact commit **5410121ebf4e5eebd64b06d3a78e82d052329e50** (`compliance and policy import/export xml spec and scanning view update`) by Matt Camp on 2026-07-31 20:03 CDT, inside the actual Crystal Forge Dioxus/WASM frontend.

### Exact design files changed in the reference commit
- `docs/design/CrystalForge/.thumbnail` (updated)
- `docs/design/CrystalForge/components/ComplianceView.jsx` (+250 lines — bundle catalog, scoring, evidence drawer, export/import modals, XCCDF export)
- `docs/design/CrystalForge/components/Icon.jsx` (+1 line)
- `docs/design/CrystalForge/components/PoliciesView.jsx` (+269 lines — grouped cards, category filter strip, drawer, form, JSON/TOML import/export)
- `docs/design/CrystalForge/components/ScanningView.jsx` (+14 lines — queue table, activity feed, schedule modal)
- `docs/design/CrystalForge/components/Shell.jsx` (+34 lines — shared IOMenu, BulkBar, multi-select, infinite scroll, DTG, classification, search, notifications, sidebar badges)
- `docs/design/CrystalForge/docs/crystal-forge-xccdf-interchange-profile-v0.1.md` (added — 1082 lines)
- `docs/design/CrystalForge/styles.css` (+17 lines — tokens, callouts, tables, chips, badges, forms)
- `docs/design/CrystalForge/uploads/CLA.md` (added)
- `docs/design/CrystalForge/uploads/crystal-forge-xccdf-interchange-profile-v0.1.md` (added)

### Functional scope
- **ComplianceView** — bundle catalog, per-system control evidence drawer, scoring strip, export evidence modal, bundle create/edit modal, import STIG/bundle modals, XCCDF bundle export.
- **PoliciesView** — grouped policy cards, category stat filter strip, policy detail drawer, custom policy form, import/export JSON/TOML policies.
- **ScanningView** — scan queue table, all-configs expandable table, scan activity feed, schedule modal, bulk selection bar.
- **Shell** — shared `IOMenu`, `BulkBar`, `useMultiSelect`, `useInfiniteScroll`, live `DTG` timestamps, classification banner, attention flash/acknowledgement, global search, topbar notifications, sidebar badges.
- **Styles** — new tokens, callouts, tray/drawer, table, chip, badge, and form styling.
- **XCCDF Interchange Profile** — design document and export format for compliance bundles.

The goal is to translate these design files into real Dioxus components and routes in `packages/web-ui/`, aligned with the server API and existing web-ui architecture, preserving existing UI patterns (loading, empty, error states, etc.) and exercising the changes through the `web-ui` check.

### Commit reference
```
commit 5410121ebf4e5eebd64b06d3a78e82d052329e50
Author: Matt Camp <matt@aicampground.com>
Date:   Fri Jul 31 20:03:04 2026 -0500

    compliance and policy import/export xml spec and scanning view update
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Compliance view matches the design: bundle catalog, score strip, systems matrix, per-control evidence drawer, and export/import modals
- [ ] #2 Policies view matches the design: grouped policy cards, category stat strip, detail drawer, custom policy form, and JSON/TOML import/export
- [ ] #3 Scanning view matches the design: scan queue table, all-configs expandable table, activity feed, and schedule modal
- [ ] #4 Shell components match the design: IOMenu, BulkBar, useMultiSelect, useInfiniteScroll, DTG, classification banner, global search, topbar notifications, sidebar badges
- [ ] #5 Theme/styling tokens from the updated design CSS are applied consistently across the web-ui
- [ ] #6 XCCDF bundle export format aligns with the design document
- [ ] #7 All changes are exercised by the authoritative web-ui check and pass `nix flake check --keep-going`
<!-- AC:END -->
