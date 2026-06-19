---
id: TASK-348.1
title: 'CVEs: delete legacy cves_old.rs and bring CVEs view to parity'
status: Review
assignee:
  - opencode-agent
created_date: '2026-06-10 13:34'
updated_date: '2026-06-19 03:44'
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
  - packages/web-ui/src/views/cves.rs
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
Executed: confirmed legacy file removal + mod.rs cleanliness; verified view parity; added missing CVE triage review/target date field in disabled deferred-persistence state; strengthened web-ui check for grouped + drawer + triage date labels + disabled state; created TASK-348.1.1 for persistence; ran targeted fmt/check and nix web-ui check; updated MR !280; task remains in Review.
<!-- SECTION:PLAN:END -->
