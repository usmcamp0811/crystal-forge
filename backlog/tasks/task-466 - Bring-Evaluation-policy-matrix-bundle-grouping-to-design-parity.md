---
id: TASK-466
title: Bring Evaluation policy matrix bundle grouping to design parity
status: In Progress
assignee:
  - openai-agent
created_date: '2026-09-21 16:18'
updated_date: '2026-09-21 16:35'
labels:
  - web-ui
  - server
  - evaluations
  - compliance
  - design-parity
  - performance
dependencies: []
references:
  - MR !329
  - TASK-326.2
documentation:
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/styles.css
modified_files:
  - >-
    packages/default/crates/cf-server/migrations/0269_evaluation_policy_provenance_snapshots.sql
  - packages/default/crates/cf-server/src/compliance/resolver.rs
  - packages/default/crates/cf-server/src/models/deployment_policies.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/queries/commits.rs
  - packages/default/crates/cf-server/src/server/mod.rs
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/handlers/api/commits.rs
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/assets/app.css
  - checks/web-ui/tests/integration-test.js
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/default.nix
priority: high
type: feature
ordinal: 484000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The production Evaluation drawer renders one column per policy and becomes unusable for compliance bundles with more than 100 controls. The matrix also loses evaluation-time bundle provenance, so the browser cannot group controls authoritatively or preserve historical bundle membership. Implement the real-data design behavior without changing evaluator, scheduling, deployment-gating, scanning, or CVE semantics, and without modifying TASK-326.2 or MR !329.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The policy-matrix API exposes deterministic structured policy columns with stable policy identity policy version identity and policy lineage identity when authoritative data exists while preserving required compatibility fields.
- [ ] #2 The API exposes deterministic many-to-many bundle memberships with exact evaluation-time bundle lineage version name framework scope and synthetic/unbundled metadata and does not infer membership from names or mutable current catalog state.
- [ ] #3 The smallest correct additive persistence preserves future evaluation-time bundle provenance without changing evaluator semantics deployment gating evaluation scheduling or build scheduling and legacy rows remain truthful.
- [ ] #4 The server uses bounded set-based data access without one request or query per bundle or control and viewer authorization remains unchanged.
- [ ] #5 A matrix with more than 14 controls defaults to bundle columns and Failing only while a small matrix defaults to simple control columns.
- [ ] #6 Bundle mode renders one roll-up column per applicable real bundle plus an explicit synthetic Crystal Forge platform group for unbundled checks and does not initially render systems multiplied by controls cells.
- [ ] #7 Bundle rollups preserve pass warning failure infrastructure error Nix evaluation failure not-assigned legacy-unknown and no-applicable-control semantics without treating unknown error or N/A as pass.
- [ ] #8 Many-to-many controls contribute once to each applicable bundle rollup without duplicate policy evaluation and the UI reports shared-control counts explicitly.
- [ ] #9 Operators can switch between Bundles and Controls focus a bundle from its header or cell clear focus and restore all controls hidden by Failing only using the visible hidden-column note.
- [ ] #10 Row filters counts Worst first and Name sorting host health Top issues control filtering host expansion detailed findings Open system and View policy definition remain correct.
- [ ] #11 Authoritative short control identifiers and exact compliance-evidence navigation are used only when the destination can consume bundle control system and evaluation revision identity; no fake deep links are added.
- [ ] #12 Focused server and pure web-UI tests cover small and 100-plus-control matrices overlap differing per-system applicability ordering legacy unknown not assigned infrastructure failures historical immutability filters sorting focus and failing-only behavior.
- [ ] #13 A production-shaped fixture with at least 60 systems 100 policies two overlapping bundles and unbundled checks proves bounded initial bundle rendering and materially reduced control columns under Failing only.
- [ ] #14 The authoritative browser workflow covers initial dense mode drilldown Failing only overlap row filters host expansion coherent mode transitions and manually inspected desktop light desktop dark and narrow desktop dark screenshots.
- [ ] #15 Production is compared against the full EvalDrawer design and matching CSS with no unresolved P0 P1 or P2 differences in the requested policy-matrix sections.
- [ ] #16 Focused formatting server SQL or migration SQLx-offline frontend unit WASM browser and git diff checks pass as applicable and any schema migration is additive and supports legacy rows.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add an immutable attempt-scoped provenance snapshot. Extend resolver provenance with exact assignment-version and scope identity. Keep Nix evaluation inputs deduplicated by configuration, but carry a separate per-system capture so systems that share one configuration retain distinct bundle applicability. Persist a marker per evaluated system/configuration plus normalized policy-contribution rows in the successful attempt transaction. Use additive migration 0269 with immutable foreign-keyed rows; never reconstruct legacy evaluations from current assignments.
2. Extend the matrix read path with two bounded set-based reads: existing derivation outcomes and the latest matching completed-attempt provenance snapshot joined to immutable policy/bundle version metadata. Return compatibility labels/results/details plus typed policy columns, deterministic bundle groups, stable system identities, per-system policy-to-bundle applicability, and an explicit legacy-unavailable provenance state. Duplicate a shared configuration outcome for each snapshotted system only at response projection time; never duplicate evaluator execution.
3. Extract pure Web UI matrix domain helpers for status classification, dense defaults (>14), row filters/sort, bundle aggregation, many-to-many/shared counts, bundle focus, visible control projection, and Failing-only trimming over visible rows. Unit-test pass/warn/fail/error/unknown/N/A behavior and a production-shaped 60-system, 120-control fixture before wiring RSX.
4. Rework EvalDrawerPolicyTab to consume typed metadata. Preserve Top issues, host health, existing detailed findings and policy navigation. Add bundle/control segmented mode, bundle headers/distribution bars, compact host rollups, drilldown/focus clearing, hidden-column restoration, explicit platform/unbundled grouping, mode-specific legends, semantic button interactions, and narrow-layout behavior. Add exact compliance-evidence navigation only when bundle version, policy, system, and evaluation revision are all available and supported.
5. Port the matching EvalDrawer policy-matrix CSS hierarchy from the design rather than inventing a parallel visual system. Keep small matrices simple and avoid constructing systems × controls cells in default dense bundle mode.
6. Add server resolver/persistence/query/handler tests for ordinary, >100-control, overlap, direct policy, per-system applicability, status preservation, ordering, no duplicate evaluation, historical immutability, legacy rows, and viewer authorization. Update SQLx offline metadata if checked queries require it.
7. Add focused authoritative browser step 26e with intercepted production-shaped data (60 systems, 120-150 controls, two overlapping real bundles, platform checks), semantic cell-count/performance assertions, bundle drilldown, Failing-only restoration, overlap, filters, expansion, navigation, mode coherence, and named desktop light/dark plus narrow-dark screenshots. Update the coverage manifest and critical workflow list.
8. Keep the isolated devenv preview current, restarting only this task's backend after backend changes. Run targeted formatting/tests first, then SQL/migration and SQLX_OFFLINE checks, Web UI unit/WASM checks, focused browser workflow and screenshot inspection, git diff --check, and separate completeness/correctness/performance/parity/state/workflow/regression review passes. Record exact evidence, commit, push, open a new MR to dev, and leave it unmerged.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-agent on localhost in /home/mcamp/code/crystal-forge/TASK-466-eval-policy-matrix-bundles

Preflight: dedicated branch TASK-466-eval-policy-matrix-bundles was created at exact fetched origin/dev SHA 4fb7d960909bb7dc75d5f859af43e38f7a3c657e. The worktree is clean. TASK-326.2 has uncommitted scanning/CVE work in its own worktree, including api/models.rs and app.css; this task will not modify that worktree or MR !329. Separate worktree isolation prevents loss, though later integration may require normal conflict resolution.
<!-- SECTION:NOTES:END -->
