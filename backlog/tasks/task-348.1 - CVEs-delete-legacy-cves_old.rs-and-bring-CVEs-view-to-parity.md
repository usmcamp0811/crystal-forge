---
id: TASK-348.1
title: 'CVEs: delete legacy cves_old.rs and bring CVEs view to parity'
status: In Progress
assignee:
  - opencode-agent
created_date: '2026-06-10 13:34'
updated_date: '2026-06-19 01:52'
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
  - packages/web-ui/src/views/mod.rs
  - packages/web-ui/src/components/cve/mod.rs
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
- [ ] #1 Legacy cves_old.rs is confirmed unused and removed; views/mod.rs updated
- [ ] #2 CVEs stat strip + severity breakdown match the design
- [ ] #3 Grouped and flat views match the design and severity filter re-issues the API query
- [ ] #4 Triage statuses render from real data with no fabricated values
- [ ] #5 Steps 16-cves and 16b-cves-severity-filter pass with parity assertions
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Proposed implementation plan (awaiting user approval before code changes):
1. Create and work in dedicated worktree `/home/mcamp/code/crystal-forge/TASK-348.1-cves-parity` on branch `TASK-348.1-cves-parity` from `dev`.
2. Compare current `packages/web-ui/src/views/cves.rs`, `components/cve`, and web-ui check steps against `/home/mcamp/code/crystal-forge/CrystalForgelatest/components/CvesView.jsx`.
3. Confirm whether `packages/web-ui/src/views/cves_old.rs` exists and is unused; remove it and update `views/mod.rs` only if no references remain.
4. Implement scoped CVEs parity: stat strip, severity breakdown, grouped/flat modes, severity filter API refresh, and real triage-status rendering without fabricated values.
5. Update `checks/web-ui/tests/integration-test.js` steps `16-cves` and `16b-cves-severity-filter` to assert the parity behavior and capture the needed UI state.
6. Verify with targeted commands: web-ui cargo check, relevant rustfmt checks, and `nix build .#checks.x86_64-linux.web-ui` if feasible for the UI screenshot requirement.
7. Commit, push, open/update MR, move task to Review only after verification and MR creation.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: opencode-agent on reckless in /home/mcamp/code/crystal-forge/TASK-348.1-cves-parity
<!-- SECTION:NOTES:END -->
