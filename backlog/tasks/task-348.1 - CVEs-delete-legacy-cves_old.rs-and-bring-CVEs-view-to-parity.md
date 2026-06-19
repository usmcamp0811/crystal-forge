---
id: TASK-348.1
title: 'CVEs: delete legacy cves_old.rs and bring CVEs view to parity'
status: Review
assignee:
  - opencode-agent
created_date: '2026-06-10 13:34'
updated_date: '2026-06-19 02:09'
labels:
  - design-parity
  - cves
  - web-ui
  - child
milestone: 'm-19: Design Parity Existing Surfaces'
dependencies: []
references:
  - TASK-348
  - /home/mcamp/code/crystal-forge/CrystalForgelatest/components/CvesView.jsx
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
documentation:
  - design/doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md
  - design/doc-14 - Parity-execution-playbook-agent-proof.md
modified_files:
  - checks/web-ui/tests/integration-test.js
parent_task_id: TASK-348
priority: high
ordinal: 1771
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Child of CVEs/Scanning umbrella TASK-348. Follow guide doc-14 standard procedure.

## Problem
The CVEs view (`views/cves.rs`) must match `CrystalForgelatest/components/CvesView.jsx`. A legacy `views/cves_old.rs` exists and should be removed once confirmed unused.

## Goal
Bring the CVEs view to parity (grouped + flat views, severity filter, stat strip) backed by real API data, and remove the legacy file.

## Exact scope
1. Confirm `views/cves_old.rs` is unused; remove it and update `views/mod.rs` (coordinate with TASK-341).
2. Stat strip + severity breakdown match design.
3. Grouped and flat view modes match design; severity filter re-issues the API query with `?severity=`.
4. Triage status surfaces match design using real data.

## Non-goals
- Scanning view (sibling task TASK-348.x) — separate surface.

## Files
- packages/web-ui/src/views/cves.rs
- packages/web-ui/src/views/mod.rs
- packages/web-ui/src/components/cve/mod.rs
- checks/web-ui/tests/integration-test.js

## Verification
- nix develop -c cargo fmt -- --check
- nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown
- nix build .#checks.x86_64-linux.web-ui
- Reuse steps `16-cves` and `16b-cves-severity-filter`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Legacy cves_old.rs is confirmed unused and removed; views/mod.rs updated
- [x] #2 CVEs stat strip + severity breakdown match the design
- [x] #3 Grouped and flat views match the design and severity filter re-issues the API query
- [x] #4 Triage statuses render from real data with no fabricated values
- [x] #5 Steps 16-cves and 16b-cves-severity-filter pass with parity assertions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Executed: confirmed legacy file removal + mod.rs cleanliness; verified view parity; strengthened web-ui check for grouped + drawer; ran nix web-ui check; opened MR !280; moved to Review.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Confirmed legacy cves_old.rs already removed and not referenced in views/mod.rs (removed under TASK-341). CVEs view + components/cve already at parity and backed by real API resources, so no Rust source changes were needed.

Extended web-ui check step 16-cves to assert grouped default surface and open/assert the CVE detail drawer; kept 16b-cves-severity-filter.

Verification: nix build .#checks.x86_64-linux.web-ui passed; steps 16-cves and 16b-cves-severity-filter reported OK with screenshots.

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/280
<!-- SECTION:NOTES:END -->
