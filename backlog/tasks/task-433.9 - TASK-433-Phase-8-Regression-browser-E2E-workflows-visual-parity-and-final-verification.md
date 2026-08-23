---
id: TASK-433.9
title: >-
  TASK-433 Phase 8: Regression, browser E2E workflows, visual parity, and final
  verification
status: Backlog
assignee: []
created_date: '2026-08-23 01:43'
labels:
  - design-parity
  - policy
  - poam
  - web-ui
  - server
  - phase-8
dependencies:
  - TASK-433.8
references:
  - TASK-433
  - TASK-433.1
documentation:
  - docs/design/CrystalForge/
  - checks/web-ui/tests/integration-test.js
  - docs/agent/verification.md
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 441000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 8 of 8 (contextual only, final phase). Proves the full ae20da81 delta end-to-end against the exact final SHA: all six browser workflows, backward compatibility, error/loading/empty-state coverage, the final design-file omission classification, and every required verification command with recorded artifacts.

## Explicit scope
- All affected loading, empty, error, unauthorized, conflict, stale, partial-success, overdue, awaiting-verification, verification-failure and historical states are covered across catalog/editor/POAM surfaces.
- Existing policies, versions, mappings, provenance, bundles, assignments, evidence, exports, dashboards and notifications remain backward compatible after all prior phases.
- Every changed design file is classified as product behavior covered by criteria or demo-only and explicitly not ported (finalizes TASK-433.1's initial classification).
- Six end-to-end browser workflows proven with artifacts:
  1. Large catalog: deep search, collapse/expand, cards/table, range selection with >60 policies.
  2. Unmapped custom policy: adds real Nix enforcement, saves/reopens, remains valid Unmapped.
  3. Imported STIG refinement: read-only provenance/mappings, added enforcement, save/reopen, lineage preservation.
  4. Long DoD banner: exact multiline edit/save/reopen semantic preservation.
  5. Mixed enforcement: Nix plus CVE constituent and policy-level results through the applicable evaluation path.
  6. POAM lifecycle: starts at failed evidence, creates/links, retains FAIL, edits milestones/links, shows system/bundle rollups, reaches Awaiting Verification, blocks closure while failing, closes after passing evaluation, retains history.
- All exact final-SHA verification commands listed in TASK-433 are run and results recorded.
- Visual parity review records differences and artifacts for every affected production surface plus the changed-design-file omission classification.

## Explicit non-scope
No new product features; this phase is verification/regression/parity closure for Phases 1-7 only. Any gap found is fixed by returning to the owning phase subtask, not by scope-creeping this subtask.

## Verification
Run against exact final SHA:
```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml
nix build .#packages.x86_64-linux.web-ui --no-link
nix develop -c bash -c 'cd packages/default && cargo sqlx prepare --workspace'
nix build .#checks.x86_64-linux.integration --no-link
nix build .#checks.x86_64-linux.server-regressions --no-link
nix build .#checks.x86_64-linux.web-ui --no-link
nix build .#checks.x86_64-linux.ui-screenshots --no-link
nix build .#checks.x86_64-linux.web-ui-reconciliation --no-link
nix flake check --keep-going
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 All affected loading, empty, error, unauthorized, conflict, stale, partial-success, overdue, awaiting-verification, verification-failure and historical states are covered.
- [ ] #2 Existing policies, versions, mappings, provenance, bundles, assignments, evidence, exports, dashboards and notifications remain backward compatible.
- [ ] #3 Every changed design file is classified as product behavior covered by criteria or demo-only and explicitly not ported.
- [ ] #4 Required exact final-SHA verification commands and browser artifacts are run and recorded.
- [ ] #5 Large catalog browser workflow proves deep search, collapse/expand, cards/table and range selection with more than 60 policies.
- [ ] #6 Unmapped custom policy browser workflow adds real Nix enforcement, saves/reopens, and remains valid Unmapped.
- [ ] #7 Imported STIG refinement workflow proves read-only provenance/mappings, added enforcement, save/reopen and lineage preservation.
- [ ] #8 Long DoD banner workflow proves exact multiline edit/save/reopen semantic preservation.
- [ ] #9 Mixed enforcement workflow proves Nix plus CVE constituent and policy-level results through the applicable evaluation path.
- [ ] #10 POAM browser workflow starts at failed evidence, creates/links, retains FAIL, edits milestones/links, shows system/bundle rollups, reaches Awaiting Verification, blocks closure while failing, closes after passing evaluation, and retains history.
- [ ] #11 Visual review records differences and artifacts for every affected production surface and the changed-design-file omission classification.
<!-- AC:END -->
