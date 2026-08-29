---
id: TASK-433.9
title: >-
  TASK-433 Phase 8: Regression, browser E2E workflows, visual parity, and final
  verification
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-08-29 15:40'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 8 final verification plan

1. Reconfirm the Phase-7 dependency gate, current branch/worktree, MR conflict state, and exact-head pipeline. Keep AC1-AC11 unchecked until objective final-head evidence exists.
2. Read TASK-433 and TASK-433.1 through TASK-433.9 completely, plus verification/database/workspace contracts. Rebuild the exact `c2f5db08..ae20da816edb1cb14275db9cd646010e69d88cd8` design inventory and inspect every changed product and demo/harness file.
3. Build a phase-ownership map and parent AC1-AC40 evidence matrix. For each AC identify current production code, current discriminating unit/server/DB/browser proof, artifact, exact-head status, and remaining concern.
4. Review the complete MR against current `origin/dev` for unrelated task drift, deleted/generated/debug files, design-file contamination, migration/SQLx hygiene, stale documentation, prohibited browser authority, and nonfunctional controls.
5. Perform separate requirements, backend/data-integrity, performance/query, authorization/security, state/error, backward-compatibility, exact-identity, audit, notification-dedupe, dashboard-layout, and Setup Coach reviews. Inspect direct APIs and current tests, not task checkboxes.
6. Enumerate every exposed enforcement kind and build the required create/validate/persist/reload/phase/outcome/edit/evidence/import-export matrix. Build the POA&M backend lifecycle/concurrency matrix and run all relevant ignored/live-DB tests against repository-isolated PostgreSQL.
7. Review and strengthen the six canonical production-path browser workflows: large catalog; unmapped custom policy; imported STIG refinement; exact multiline DoD banner; mixed Nix+CVE enforcement; and complete POA&M lifecycle. Fixtures may arrange state, but accepted operations must use production HTTP, Dioxus routing/components, and persistence.
8. If a product gap is found, return its owning TASK-433.2 through TASK-433.8 phase to In Progress before fixing it. Record ownership, add focused regression coverage, run focused verification, obtain independent re-review, return the owner to Review, then resume Phase 8. Do not add new product features.
9. Capture deterministic screenshots for meaningful intermediate/final states of all six workflows. Compare all affected production surfaces to the authoritative design in desktop/narrow layouts and representative dark/light themes. Record behavior, hierarchy, metadata, interaction, accessibility, and true cosmetic differences.
10. Produce the complete state matrix, backward-compatibility findings, changed-design-file classification, visual review, performance/security reports, artifact inventory, and parent/Phase-8 AC evidence in durable task notes or documentation without modifying authoritative design files.
11. Freeze production/test/Nix/migration code. Record the candidate SHA and run every required exact-final-SHA command, SQLx preparation on an isolated database, clean migration path, JavaScript syntax, and diff checks. Inspect screenshot/reconciliation artifacts and underlying exit statuses.
12. Re-open the complete MR diff after verification and perform final independent P0-P3 review. Any production/test/Nix/migration change restarts affected exact-SHA verification.
13. Commit and push intended Phase-8 test/parity evidence, require an exact-final-branch-head green GitLab pipeline, and confirm clean synchronized local/remote state with no conflicts.
14. Reconcile TASK-433.2 through TASK-433.8, parent TASK-433 AC1-AC40, and TASK-433.9 AC1-AC11 from current-head evidence only. Keep all tasks in Review until merge and do not mark Done.
15. Rewrite MR !318 description to summarize the full eight-phase MR, invariants, migrations, compatibility, six workflows, visual review, exact verification, final pipeline, and accepted P3 differences. Stop without merging and issue the required maintainer report.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Initial Phase-8 independent audits on production-equivalent head `4e77d7db` (bookkeeping head `f0471b6e`) found merge blockers. Canonical browser workflows are incomplete or non-gating: the large catalog and bulk delete are route-mocked; Unmapped+real Nix enforcement is split across tests; imported STIG refinement does not add enforcement/reopen lineage; mixed Nix+CVE outcomes are inserted directly into SQL; and the POA&M workflow stops at closure rejection while successful rollup fixtures directly change assessment outcome. Required policy workflows are absent from the Web UI check's blocking list. Phase-8 AC1, AC4, AC6, AC7, AC9, AC10, and AC11 remain unproven.

Owning-phase product gaps were also found and will be remediated under their original tasks before Phase 8 resumes: Phase 1 bulk-delete CSRF/direct API proof; Phase 2 policy/mapping mutation CSRF plus common editor dialog keyboard/accessibility and reference hierarchy; Phase 4 arbitrary unknown `/nix/store` rollback authorization, bounded composite validation, stale unsupported-composite helper semantics, direct rollback/deploy CSRF, and complete exposed-kind outcome matrix; Phase 5 source-neutral legacy finding verification/closure/rollups, direct Open→Awaiting Verification transition, bounded rollup history, authoritative POA&M suite gating, and upgrade coverage; Phase 7 notification producer scaling/gating. Each owner will return to Review only after focused tests and independent re-review.

Current static parent matrix: 31 PASS, 8 FAIL, 1 UNVERIFIED. Confirmed failures are AC28 state matrix, AC31 exact-final-SHA verification, AC33 canonical Unmapped+Nix workflow, AC34 imported STIG refinement workflow, AC36 real mixed execution browser workflow, AC37 complete POA&M lifecycle browser workflow, AC38 complete exposed-kind matrix, and AC40 visual/artifact report. AC29 backward compatibility remains UNVERIFIED pending final-head reconciliation. Parent checkboxes remain unchanged.
<!-- SECTION:NOTES:END -->
