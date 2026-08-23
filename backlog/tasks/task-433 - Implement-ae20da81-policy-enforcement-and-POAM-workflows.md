---
id: TASK-433
title: Implement ae20da81 policy enforcement and POA&M workflows
status: To Do
assignee: []
created_date: '2026-08-23 01:35'
labels:
  - design-parity
  - policy
  - enforcement
  - compliance
  - poam
  - web-ui
  - server
  - database
dependencies: []
references:
  - >-
    https://gitlab.com/crystal-forge/crystal-forge/-/commit/ae20da816edb1cb14275db9cd646010e69d88cd8
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/commit/c2f5db08'
documentation:
  - docs/design/CrystalForge/
  - docs/agents/verification.md
  - packages/default/WORKSPACE.md
  - docs/agent/database-safety.md
modified_files:
  - packages/web-ui/src/views/policies.rs
  - packages/web-ui/src/views/policies_api.rs
  - packages/web-ui/src/views/compliance.rs
  - packages/web-ui/src/views/system_detail.rs
  - packages/web-ui/src/views/dashboard.rs
  - packages/web-ui/src/components/policy
  - packages/web-ui/src/components/compliance
  - packages/web-ui/src/components/dashboard
  - packages/default/crates/cf-server/src/models/deployment_policies.rs
  - packages/default/crates/cf-server/src/queries/deployment_policies.rs
  - packages/default/crates/cf-server/src/queries/compliance.rs
  - packages/default/crates/cf-server/src/queries/framework_requirements.rs
  - packages/default/crates/cf-server/migrations
  - checks/web-ui/tests/integration-test.js
priority: high
type: feature
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Implement all real product behavior in `c2f5db08..ae20da816edb1cb14275db9cd646010e69d88cd8` on current `dev` using Dioxus/server/PostgreSQL. Do not edit or port the React harness.

## User/problem statement
Users need a scalable policy catalog, one editor for every policy origin, composable enforcement at correct phases, authoritative mappings/provenance, and durable remediation from failed finding through verified closure without changing Pass/Fail.

## Design source and exact SHAs
Repository `https://gitlab.com/crystal-forge/crystal-forge`; design `ae20da816edb1cb14275db9cd646010e69d88cd8`; parent `c2f5db08`; exact delta `c2f5db08..ae20da816edb1cb14275db9cd646010e69d88cd8`; source `docs/design/CrystalForge/`.

## Current implementation findings
Existing catalog pagination/deletion eligibility, policy versions, mappings, import/export, bundles, framework/requirement/assignment versions and computed evidence must be preserved. Server has several rule structs but no proven heterogeneous composite execution or first-class NixOS metadata/evaluation. Evidence is transient and finding identity is an implicit bundle/version/system/policy context. Production has no POA&M persistence/API/audit/dashboard/notification/coach support; extend existing notification and six-step setup-wizard conventions.

## Explicit scope
Policy catalog scaling; unified editor; category-guided composable enforcement; typed NixOS values/metadata; authoritative mappings/read-only imported provenance; complete backward-compatible enforcement execution; normalized POA&M persistence/API/lifecycle/history/verification; finding/evidence, system, bundle, assignment, dashboard, notification and setup-coach integration; migrations/SQLx/auth/audit/indexes/batched queries; tests/artifacts/parity.

## Explicit non-scope
Do not port fixtures, seeded POAMs/status overrides, synthetic policies, fake IDs/users/findings, in-memory mutations, localStorage POAM/dashboard/coach state, `CustomEvent`, `window.__cfCoach`, hard-coded option lists as sole source, cache-busting/thumbnail changes, or fixture import behavior. Do not weaken deletion/immutable history, turn POAM into waiver/Pass, make provenance editable, flatten phases into Nix, rewrite unrelated TASK-422/compliance architecture, or expose nonfunctional controls.

## Architecture/data-model changes
Add the smallest repository-consistent versioned composite rule-set with stable rule IDs, typed kind/config, deterministic serialization/digest, `all` semantics, per-rule outcomes, policy aggregation, evidence, read-back/edit and import/export. Legacy single-type policies remain compatible without rewriting immutable history. Provide production NixOS option search/type/enum metadata with unknown/custom fallback; store semantic values and safely preserve difficult strings. Add normalized POA&M records with stable DB/human IDs, title/plan/owner/target/risk/creator/timestamps/closure reference, statuses `open`, `in_progress`, `blocked`, `awaiting_verification`, `completed`, stable finding links, milestones, activity/history and closure verification. Derive overdue; enforce one active remediation per finding if retained; assignment references never mutate immutable versions.

## Database changes
New migrations only, current FK/type/index conventions, valid-link/invariant constraints, indexes for status/date/human ID/system/policy/bundle/requirement/active links/activity, SQLx refresh against isolated PostgreSQL. Add stable finding/evidence identity only if required while retaining computed-evidence compatibility.

## API changes
Authenticated APIs for POAM create-from-real-finding, detail/list/filter/search, updates/status, milestones, notes/history, link/unlink, compatible search, verify/close/reopen, system/bundle rollups and dashboard summary/watchlist. Validate context, compatibility, stale/conflict state, active invariant, authorization and CSRF. Closure transactionally rechecks linked results: only current Pass or documented accepted waiver; Fail/Error/Unknown/NotChecked/stale cannot close. Use typed errors and existing audit/session conventions.

## Frontend changes by page/component
Policies: exact threshold 150/chunk 60, collapse/search restoration, cards/table, logical Shift/group/cross-chunk selection, selected export and server-reasoned partial delete. Editor: common Basics/Enforcement/Compliance/Evidence shell, read-only Provenance, category guidance without deletion, readable rule management, valid Unmapped, distinct mapped-no-enforcement/No enforcement, imported read-only mappings. Compliance/evidence: failed rows stay FAIL and expose Create/Link POAM with exact navigation. System: committed POAM filters/counts. Bundle: open/on/no POAM/overdue/awaiting/closed rollups and batched actions. Dashboard: real Summary/Watchlist with layout migration. Notifications: real deduplicated overdue/awaiting events. Coach: production-derived policy/bundle/Track a POAM steps.

## Enforcement execution semantics
Verify before exposing: `nixos_option`, `packages_installed`, `packages_absent`, `custom_eval`, `cve_block`, `eval_passed`, `pin_required`, `time_window`, `approval_required`, `rollout_percent`. Execute at correct evaluation/package, scan/build, source/evaluation or deployment phase; do not flatten into Nix. Each visible kind requires UI->DTO->validation->storage->execution->result/evidence->read-back/import/export and pass/fail/error/not-checked tests. Recommendations are suggestions only.

## POA&M lifecycle/state model
A failed finding remains FAIL. Creation prepopulates real context. Design default milestones are server-dated offsets 14/28/35/49/56 days. Link validates compatibility, not title. Open/In Progress/Blocked -> Awaiting Verification -> Completed; milestone checks never close; all linked findings require authoritative Pass/accepted waiver; close stores verification; reopen retains history; overdue derives from date/status.

## Authorization/audit requirements
Follow session/CSRF/compliance scope/environment membership/admin conventions. Audit create, field changes, status, milestones, notes, links, verify, close, reopen and assignment relationships using existing audit patterns.

## Query/performance requirements
No POAM query per system/policy/finding. Batch active links, system/bundle rollups, dashboard/detail; add indexes/query regressions. Preserve catalog API pagination; chunking is client rendering.

## Error/loading/empty-state requirements
Implement/test catalog loading/empty/no-results/collapse/partial/all-blocked/export/delete; editor load/mapping/validation/unknown option/metadata/provenance/unmapped/no-enforcement/serialization; POAM none/detail/create/stale/incompatible/no-eligible/overdue/awaiting/still-failing/success/unauthorized/conflict/closed historical. No fabricated fallbacks.

## Migration/backward compatibility
Existing policy forms/immutable versions/mappings/provenance/bundles/assignments/evidence/exports/dashboards/notifications remain readable and semantically compatible. New data is additive/versioned; never edit applied migrations or historical versions.

## Implementation phases in dependency order
0 exact SHA/design inventory/baseline; 1 catalog; 2 editor; 3 Nix metadata/serializer; 4 composite enforcement; 5 POAM DB/API/auth/audit/server tests; 6 finding/system/bundle UI; 7 dashboard/notifications/coach; 8 regression/browser artifacts/parity/omission pass.

## Required automated tests
Acceptance criteria define required unit, Rust integration, API, DB and browser coverage for catalog, editor, enforcement, Nix serialization, POAM lifecycle, rollups, auth, concurrency and all six end-to-end workflows.

## Visual parity checklist
Compare hierarchy/order/metadata/interaction/spacing/density/loading/empty/error states for every affected surface and retain artifacts.

## Exact verification commands
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

## Final design-file omission pass
Product behavior from `app.jsx`, `components/ComplianceView.jsx`, `DashboardView.jsx`, `PoamViews.jsx`, `PoliciesView.jsx`, `PolicyEditor.jsx`, `SetupCoach.jsx`, `Shell.jsx`, `SystemDetail.jsx`, `data-compliance.js`, `data-dashboard.js`, `data-enforcement.js`, `data-mappings.js`, `data-poam.js`, `data-policies.js`, `data.js`, and `styles.css` is covered by criteria. `.thumbnail` is binary preview only; `crystal-forge.html` is cache-busting/script-loader harness only; `fixtures/crystal-forge.fixtures.js/.json` are seeded demo IDs only; do not port. Verify against final `git diff --name-only`.

## Definition of Done
All criteria checked with exact SHA; only intended production/test files changed; no design files modified; additive migrations and SQLx refreshed; required checks/artifacts recorded; auth/audit/immutable history/failure semantics/race safety/performance/compatibility/file classifications documented; no mock/localStorage/window-global POAM persistence or knowingly nonfunctional controls remain.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy groups independently collapse; groups larger than 150 default collapsed; group counts and selection state are visible.
- [ ] #2 Large groups initially render at most 60 items and provide current/total plus Show more and Show all.
- [ ] #3 Search reveals matches in collapsed groups and clearing search restores prior explicit collapse state.
- [ ] #4 Cards and table views preserve equivalent policy semantics and logical selection.
- [ ] #5 Individual, Shift-range, group, cross-chunk, clear, selected export and selected delete work on filtered logical order.
- [ ] #6 Bulk delete uses server eligibility, reports deleted/skipped/reasons, handles partial/all-blocked/failure, and preserves immutable blockers.
- [ ] #7 All policy origins use one editor with Basics, Enforcement, Compliance, Evidence and read-only Provenance.
- [ ] #8 Category changes preserve every rule and change guidance only; cross-category rules remain composable.
- [ ] #9 Zero mappings save as valid Unmapped; mapped/no-enforcement and No enforcement are distinct states.
- [ ] #10 Manual mappings have permitted CRUD; imported mappings/provenance remain read-only and survive reload.
- [ ] #11 Every visible enforcement control has a complete DTO, validation, storage, execution, result/evidence, read-back and import/export path or is hidden.
- [ ] #12 Mixed Nix/evaluation-phase plus non-Nix rule sets persist and evaluate with all semantics and visible constituent outcomes.
- [ ] #13 NixOS option editor supports boolean, enum, numeric, short, multiline and unknown/custom fallback from real metadata or safe fallback.
- [ ] #14 Long semantic values round-trip exact difficult strings including the DoD multiline banner.
- [ ] #15 Composite and legacy policy representations have deterministic digest/round-trip and preserve immutable history.
- [ ] #16 Normalized POAM tables, links, milestones, activity/history and verification references exist through additive migrations with constraints/indexes.
- [ ] #17 Authenticated APIs implement POAM creation, detail/list/filter/search, update, transitions, milestones, notes, links, verification, close, reopen, system/bundle rollups and dashboard sources.
- [ ] #18 Server validates finding context, compatibility, active-link invariant if applicable, authorization/CSRF and stale/conflict conditions.
- [ ] #19 POAM creation/linking never changes the underlying evaluation result; FAIL remains FAIL.
- [ ] #20 Closure is authoritative and race-safe, requires current Pass or documented accepted waiver for all linked findings, stores verification and rejects failing/error/unknown/not-checked/stale findings.
- [ ] #21 Evidence supports Create POAM and Link existing with real prefilled context and exact navigation to/from finding/bundle/system/evidence.
- [ ] #22 System compliance provides real committed POAM filters/counts and exact finding navigation.
- [ ] #23 Bundle compliance provides real open/on-POAM/no-POAM/overdue/awaiting-verification/closed rollups and no N+1 visible-list queries.
- [ ] #24 Assignment POAM references are first-class relationships and do not mutate immutable assignment versions.
- [ ] #25 Dashboard POAM Summary and Watchlist use real batched APIs, preserve existing layouts and open detail.
- [ ] #26 POAM notifications are real deduplicated events with target/read/dismiss/navigation and no render/poll spam.
- [ ] #27 Setup coach adds production-derived policy, bundle and Track a POAM steps without breaking existing progress.
- [ ] #28 All affected loading, empty, error, unauthorized, conflict, stale, partial-success, overdue, awaiting-verification, verification-failure and historical states are covered.
- [ ] #29 Existing policies, versions, mappings, provenance, bundles, assignments, evidence, exports, dashboards and notifications remain backward compatible.
- [ ] #30 Every changed design file is classified as product behavior covered by criteria or demo-only and explicitly not ported.
- [ ] #31 Required exact final-SHA verification commands and browser artifacts are run and recorded.
- [ ] #32 Large catalog browser workflow proves deep search, collapse/expand, cards/table and range selection with more than 60 policies.
- [ ] #33 Unmapped custom policy browser workflow adds real Nix enforcement, saves/reopens, and remains valid Unmapped.
- [ ] #34 Imported STIG refinement workflow proves read-only provenance/mappings, added enforcement, save/reopen and lineage preservation.
- [ ] #35 Long DoD banner workflow proves exact multiline edit/save/reopen semantic preservation.
- [ ] #36 Mixed enforcement workflow proves Nix plus CVE constituent and policy-level results through the applicable evaluation path.
- [ ] #37 POAM browser workflow starts at failed evidence, creates/links, retains FAIL, edits milestones/links, shows system/bundle rollups, reaches Awaiting Verification, blocks closure while failing, closes after passing evaluation, and retains history.
- [ ] #38 For every exposed enforcement kind tests cover create, validate, persist, reload, correct phase, pass, fail, error/not-checked, edit, evidence and import/export.
- [ ] #39 POAM server tests cover real finding creation, multi-finding links, invalid links, active invariant, milestones, activity, transitions, overdue, closure rejection/acceptance, verification storage, reopen, filters, auth and concurrency.
- [ ] #40 Visual review records differences and artifacts for every affected production surface and the changed-design-file omission classification.
<!-- AC:END -->
