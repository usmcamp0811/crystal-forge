---
id: TASK-466
title: Bring Evaluation policy matrix bundle grouping to design parity
status: To Do
assignee: []
created_date: '2026-09-21 16:18'
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
  - packages/web-ui/src/views/evaluations.rs
  - packages/web-ui/assets/app.css
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/handlers/api/commits.rs
  - packages/default/crates/cf-server/src/queries/commits.rs
  - packages/default/crates/cf-server/src/models/deployment_policies.rs
  - packages/default/crates/cf-server/src/compliance/resolver.rs
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
