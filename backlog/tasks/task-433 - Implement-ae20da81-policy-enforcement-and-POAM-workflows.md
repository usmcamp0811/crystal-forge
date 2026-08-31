---
id: TASK-433
title: Implement ae20da81 policy enforcement and POA&M workflows
status: In Progress
assignee:
  - claude-agent
created_date: '2026-08-23 01:35'
updated_date: '2026-08-31 13:33'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Breakdown (agreed with user 2026-08-22)

TASK-433 is an umbrella. Real implementation happens in 9 subtasks, dot-suffixed, one dedicated worktree/branch each (per repo convention: parent stays organizational, only leaf subtasks get worktrees):

- TASK-433.1 Phase 0: baseline inventory (this plan) — spike, no code, done in dev worktree directly (backlog-only writes).
- TASK-433.2 Phase 1: policy catalog scaling (collapse/chunk/select/bulk-delete) — depends on .1
- TASK-433.3 Phase 2: unified policy editor (Basics/Enforcement/Compliance/Evidence/Provenance) — depends on .2
- TASK-433.4 Phase 3: NixOS option metadata + composite policy serializer — depends on .3
- TASK-433.5 Phase 4: composite heterogeneous enforcement execution — depends on .4
- TASK-433.6 Phase 5: POA&M DB schema/API/auth/audit/server tests — depends on .5
- TASK-433.7 Phase 6: POA&M integration in evidence/finding/system/bundle/assignment UI — depends on .6
- TASK-433.8 Phase 7: dashboard/notifications/setup-coach POA&M integration — depends on .7
- TASK-433.9 Phase 8: regression/browser E2E/visual parity/final verification — depends on .8

Dependency chain is strict (0->1->2->...->8) per TASK-433's own "Implementation phases in dependency order" section. AC #1-40 are distributed across subtasks with zero overlap (recorded on each subtask). Non-code discoveries or scope questions go back to this parent plan before a subtask proceeds.

## Baseline confirmed (TASK-433.1 findings, full detail on that subtask)

- `c2f5db08` and `ae20da816edb1cb14275db9cd646010e69d88cd8` are commits in THIS repo's own `dev` history (not an external repo). Exact diff: `git diff c2f5db08..ae20da81 -- docs/design/CrystalForge`. 21 files changed, +2582/-534.
- **Zero POA&M code exists anywhere** in cf-server or web-ui (only 4 hits, all doc-comments/a test in `api/models.rs` explicitly deferring `assignment_poam` serialization "until a real backing domain model exists" — POA&M was deliberately deferred once already).
- Current `DeploymentPolicy` enum (models/deployment_policies.rs) has 8 flat kind variants, evaluated independently per `AssignedPolicy`; no cross-kind composite aggregation unit exists today (only same-kind `PolicyRule`+`RuleMode` inside `CustomCheck`).
- Evidence is 100% computed-on-read (`resolve_control_evidence_with_context`, compliance.rs ~L3957); no findings/evidence persistence table exists.
- Next migration number: **0233**. Audit pattern: raw `INSERT INTO admin_audit_events (actor_user_id, action, target, metadata)` inside the owning transaction (see handlers/api/compliance.rs ~L3080). Notification pattern: extend `user_notifications`/`user_notification_preferences` CHECK-constrained category columns (0226/0227). Coach: `web-ui/src/components/onboarding/coach_panel.rs`, driven by `fetch_setup_wizard_progress()` deriving from entity counts (exact server-side source not yet traced — Phase 7 to confirm before adding steps).

## File classification (design delta, contributes to parent AC #30 — finalized in TASK-433.9)

Product behavior (covered by an AC, phase noted): app.jsx (finding cross-nav state, Phase 6) · ComplianceView.jsx (POAM filters/column/view-mode/FindingPoamBar, Phase 6) · DashboardView.jsx (poamSummary/poamWatchlist, Phase 7) · PoamViews.jsx (entire POA&M UI surface, Phase 6+7) · PoliciesView.jsx (catalog scaling Phase 1; editor invocation Phase 2) · PolicyEditor.jsx (editor shell Phase 2; NixOS typing Phase 3; enforcement kinds Phase 4) · SetupCoach.jsx (3 new steps, Phase 7) · Shell.jsx (POAM notifications, Phase 7) · SystemDetail.jsx (SystemPoamSection, Phase 6) · data-dashboard.js (widget registry, Phase 7) · data-enforcement.js (rule vocabulary Phase 4; NixOS option metadata Phase 3) · data-poam.js (POA&M domain model/lifecycle semantics — real impl in Phase 5) · data-policies.js CONTROL_FAMILIES expansion (Phase 1) · styles.css (new class categories, implemented per-phase, not ported verbatim).

Demo-only, NOT ported (mechanism, not behavior — real equivalents built per-phase): `.thumbnail` (binary preview) · `crystal-forge.html` (cache-busting/script loader) · `fixtures/crystal-forge.fixtures.{js,json}` · `data.js` fixture ID edit · `data-compliance.js` `POAM_FINDING_STATUS_OVERRIDE` fixture hack · `data-policies.js` `POLICY_STIG_BULK` (715-entry synthetic stress dataset — Phase 1/8 build our own real test data instead) and `POLICY_EDITOR_DEMO` (4 showcase policies — Phase 8 browser workflows construct equivalent real scenarios, not copy fixture data) · `CustomEvent("cf-poam-open"/"cf-poam-change")` pub/sub bus (real state via Dioxus signals + server refetch) · `window.__cfCoach` global escape hatch (real coach completion via proper state channel, Phase 7) · in-memory mutable arrays as "database" (real persistence, Phase 5) · `setTimeout(...,60)` sequencing hacks (Dioxus routing handles this natively) · `localStorage` POA&M/dashboard state.

## Open architecture decisions flagged for the owning phase (raise for review before implementing)

1. **Phase 3/4 — composite rule-set structure.** Design's PolicyEditor lets ONE policy hold multiple heterogeneous enforcement rules (e.g. a Nix option rule + a CVE-block rule together) aggregated with `all` semantics and per-rule outcomes. Today, one DB policy row = exactly one `DeploymentPolicy` kind; heterogeneous composition today only happens across separate `AssignedPolicy` rows evaluated independently. TASK-433's own architecture section calls for "the smallest repository-consistent versioned composite rule-set" — this is the single largest structural decision in the task and must be scoped/reviewed before Phase 3 implementation starts, not assumed.
2. **Phase 4 — CVE kind mapping.** Design has one `cve_block {severity, maxAllowed}` kind; server has two existing kinds (`RequireCveCheck`, `CveThreshold`). Recommend reusing `CveThreshold` (closest superset) rather than adding a third CVE variant, but confirm before implementing.
3. **Phase 4 — new kinds needed.** `nixos_option` (typed option assertion, distinct from `CustomCheck`'s raw expression), `packages_absent` (new or `RequirePackages` + prohibited flag), `eval_passed`, `pin_required` do not exist as enum variants today and need additions to `DeploymentPolicy` plus `is_nix_evaluated()` phase wiring.
4. **Phase 7 — coach backing.** Confirm exact server source behind `fetch_setup_wizard_progress()` before adding policy/bundle/poam steps, since no dedicated coach-state table was found in this audit.

## Review checkpoint

Per user instruction, this plan is presented for review after Phase 0 before any Phase 1 code is written. Awaiting go-ahead to open the TASK-433.2 worktree/branch and begin Phase 1 implementation.

## Worktree/MR decision (user, 2026-08-22)
All phase subtasks (TASK-433.2 through TASK-433.9) share ONE dedicated worktree/branch/eventual MR under this parent task, rather than one worktree per subtask. Worktree: /home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows, branch: TASK-433-policy-poam-workflows, based on dev @ c60b5799. TASK-433 itself now carries the lock/In Progress status for the shared implementation effort; individual TASK-433.x subtasks are moved through In Progress/Review/Done to track granular AC completion, but all commits land on this one branch and are shipped via a single MR opened once phase work is ready for review. Composite rule-set decision (open question #1) resolved: build the composite/multi-rule-kind structure now in Phase 3/4 as originally scoped, not deferred.

Focused final-audit remediation: review the concurrent environment-scoped policy-version usage implementation without touching agent-owned UI/browser files; add live-DB handler/query regressions proving non-admin membership filtering and admin visibility; extend production-backed composite enforcement tests to cover phase, evidence, and pass/fail/error-or-not-checked outcomes for all eight exposed kinds where each production rule can produce them; add server-side PolicyDrawer owner and exact-version usage hydration coverage; run targeted formatting and Rust tests through the Nix development environment. Preserve all concurrent worktree changes and do not commit.

2026-08-30 backend/database audit remediation: Preserve concurrent worktree edits and do not edit migrations 0233-0241. Improve additive migration 0242 so cleanup requires a non-null bootstrap completion marker, add the global deployment-failure bootstrap cursor index, and remove only the two proven duplicate POA&M indexes. Extend source-neutral waiver requests and persistence with observation-bound legacy evidence while retaining composite assessment compatibility and database-enforced immutable closure evidence. Add bounded per-request historical relationship limits with backward-compatible defaults and explicit truncation metadata. Expand PostgreSQL tests for cleanup before/after completion, legacy Fail waiver acceptance/closure, and relationship bounds. Strengthen the server-regressions pre-0233 rehearsal with populated CVE scan, desired target, immutable assignment/version overlays, deployed state, attention source, notification preferences/inbox state, then apply and assert the full current migration range. Run formatting, SQLX_OFFLINE checks, focused migrated-DB tests, migration rehearsal/server-regressions when feasible, and inspect the final scoped diff.

2026-08-30 UI re-review remediation: correct compliance evidence route initialization so deep-link reload and browser history do not transiently downgrade to bundle overview; add a state-transition unit regression where practical. Complete notification menu focus entry, keyboard event isolation, stable browser selectors, and focus restoration. Prevent policy card/row container activation from descendant keyboard events. Add reusable repository-local focus capture, trap, and restoration behavior to TASK-433 policy, compliance, evidence, and POA&M drawers. Make Setup Coach agent prerequisite and copy accurately describe the existing server-backed acknowledgement lifecycle. Preserve concurrent worktree changes; run only focused formatting/unit/static checks and diff inspection; do not commit.

2026-08-31 focused eval_passed remediation: Replace the unconditional per-system provisional Pass with the exact eval_passed outcome carried by the authoritative PolicyCheckResult. Add a discriminating regression that persists a metadata-terminal Error, simulates finalization failure handling, and proves no active eval_passed row can remain Pass. Preserve concurrent changes, run only rustfmt and the targeted fast/live-DB test when available, and do not commit.

2026-08-30 browser/harness re-review remediation: Make production re-evaluation polling use an unscaled wall-clock delay; strengthen static source contracts so each canonical mixed and POA&M workflow body contains production re-evaluation and no direct assessment/result SQL; reject duplicate coverage-manifest names; omit diagnostic screenshots from baseline statistics; process structured results, journal output, and artifact export before rejecting a nonzero browser exit; replace stale generic PolicyDrawer accessible-name selectors with stable title-aware locators. Preserve concurrent changes, do not run the Nix browser check, and verify only Node syntax/static contracts.

2026-08-31 final UI review remediation: Render assignment-list fetch failures before the empty state. Stop Escape propagation in nested POA&M and assignment dialogs so only the topmost dialog closes and busy children cannot close their parent. Apply the shared dialog focus capture, boundary trap, restoration, labeling, and Escape behavior to the new evidence-source, assignment-link, and system-detail evidence/context overlays. Synchronize `SystemDetailView` tab state from route props and browser history changes. Add focused pure/unit assertions where practical, preserve concurrent changes, and run only web-ui formatting, targeted inexpensive tests, and diff inspection.

2026-08-31 final browser/harness review remediation: Track every unhandled promise rejection as fatal browser-run state while preserving step/result and diagnostic artifact generation, force a nonzero exit after reports are written, and strengthen static contracts by isolating the `runTask433ProductionEvaluation` helper body and rejecting direct INSERT/UPDATE writes to composite assessment or rule-result tables. Preserve concurrent changes, run only Node syntax and static-contract checks, and do not commit.

2026-08-31 relationship pagination P2 remediation: Extend web relationship DTOs with rolling-compatible default pagination metadata. Fetch relationship batches page-by-page with the server's bounded history limit, merge each relationship by stable identity, and reject incoherent or excessive pagination through the existing visible API error path. Add pure deserialization and page-merge regressions. Preserve concurrent worktree changes; run only web-ui rustfmt and targeted fast unit tests; do not commit.

2026-08-31 final relationship-pagination compatibility remediation: Preserve legacy all-relationships responses when both pagination parameters are absent; reject offset-only requests; retain explicit bounded pagination for the current web client. Order paginated finding history by immutable link retirement/link identity and assignment relationships by immutable reference addition identity, while preserving active finding handling. Add focused PostgreSQL regressions for legacy no-parameter behavior and update-stable page order; add supporting indexes only in unapplied migration 0242 if query plans require them. Run targeted Rust/web formatting, focused backend tests when practical, and git diff --check. Preserve concurrent worktree changes and do not commit.

2026-08-30 notification bootstrap cast remediation: Add a `CREATE OR REPLACE FUNCTION backfill_user_notification_source_events` correction to unapplied migration 0242 only. Guard every system/POA&M `subject_id` UUID conversion with category-aware `CASE` plus UUID-shape validation, preserving the 0241 cursor ordering, batch bound, locking, insert-only behavior, and completion semantics. Add a focused isolated PostgreSQL regression with a numeric CVE subject and malformed system/POA&M subjects; prove the CVE source is queued, invalid scoped sources are skipped, and the cursor records completion. Run SQL/diff formatting checks and only the focused ignored database test when the repository-owned isolated database is available. Do not commit.

2026-08-31 five-failure focused web-ui remediation: make hidden-state assertions wait for DOM reconciliation; use the policy drawer's real accessible close name; scope Create POA&M actions to the nested dialog's accessible name; make the canonical lifecycle system clone include the required derivation; differentiate the two minimal test-flake outputs so production re-evaluation cannot violate the global derivation-path uniqueness constraint; attach immediate rejection handling to the affected response waiter without weakening its later awaited result. Run Node syntax/static contracts, Rust/Nix formatting, and the exact five-step CF_UI_TEST_STEPS web-ui check. Preserve concurrent work and do not touch TASK-440/441/442 files or commit.

2026-08-31 remaining canonical lifecycle waiter remediation: Match the unlink response by URL pathname because the production client appends the required revision query parameter. Await link and unlink click/response operations together so a click failure cannot leave a response waiter to reject during teardown. Run Node syntax/static contracts, then the canonical lifecycle browser step and the exact five-step focused check. Preserve concurrent work and do not commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-08-30 UI audit remediation: Added bidirectional compliance query synchronization so bundle/evidence/POA&M drawer state uses browser history and restores from back/reload without forcing the default bundle into a bare URL. Added keyboard activation for policy cards/table rows, compliance system rows, and POA&M rows; keyboard dashboard widget move controls; notification menu roles, Escape, arrow navigation, Enter/Space activation, and bell focus restoration. Added dialog labels/modal semantics/Escape/initial focus for policy, bundle, evidence, and POA&M drawers and modals. Evidence navigation/detail now stacks at <=640px. Imported policy details identify the importer instead of presenting imported content as manually owned. Setup Coach locks Deploy agent until a system exists and uses heartbeat-based completion copy. Added focused route, dashboard reorder, and coach prerequisite Rust tests. Verification passed: `nix develop -c cargo fmt --manifest-path packages/web-ui/Cargo.toml -- --check`; `nix develop -c cargo test --manifest-path packages/web-ui/Cargo.toml` (254 passed, 1 ignored); focused tests for compliance route round-trip, keyboard widget movement, and agent prerequisite; `nix build .#packages.x86_64-linux.web-ui --no-link`; `git diff --check`. The first two-minute Nix build attempt timed out while falling back from unavailable remote builders; the ten-minute retry succeeded. Concurrent backend, migration, docs, check, and browser-test edits remained untouched. No commit created.

2026-08-30 focused policy usage regression: added a live HTTP/SQLx test for PolicyDrawer owner hydration and exact policy-version usage authorization. The fixture now follows production publication invariants by atomically updating accepted versions and lineage pointers and by finalizing a non-pending bundle digest. Against the repository-owned isolated PostgreSQL stack on 127.0.0.1:3042, `nix develop -c env DATABASE_URL=postgresql://mcamp@127.0.0.1:3042/crystal_forge CRYSTAL_FORGE_TEST_DATABASE_URL=postgresql://mcamp@127.0.0.1:3042/crystal_forge SQLX_OFFLINE=true cargo test --offline --manifest-path packages/default/Cargo.toml --package cf-server --test policy_counts_defect policy_drawer_owner_and_usage_respect_environment_visibility -- --test-threads=1` passed (1 passed). The regression proves a Viewer receives only systems in their environment memberships, an Admin retains fleet-wide usage visibility, and exact-version owner display resolves from the creating user. Earlier attempts exposed and corrected fixture-only schema length, publication-pointer, and pending-digest violations; no production authorization relaxation was made.

2026-08-31 backend/database audit verification: Added rustdoc for the new waiver and relationship pagination fields. Restored non-disclosing NotFound behavior for mismatched waiver finding/assessment context. Updated the source-neutral forgery regression to assert migration 0242's stronger insert-time effective-attestation constraint. Corrected the bounded notification-history fixture to use production-shaped UUID build subject IDs. Verification passed: cleanup bootstrap test with --ignored (1 passed); relationship pagination regression (1 passed); legacy source-neutral closure regression (1 passed); waiver/closure evidence matrix (1 passed); bounded notification materialization regression (1 passed); `cargo fmt --manifest-path packages/default/Cargo.toml --all --check`; SQLX_OFFLINE `cargo check -p cf-server --test poam_workflows`; `git diff --check`; and `nix build path:.#checks.x86_64-linux.server-regressions --no-link`. The `path:.` form was required because migration 0242 remains intentionally untracked and ordinary Git-flake evaluation omitted it. The full server-regressions check included the populated pre-0233 rehearsal through migration 0242, 24 POA&M workflow tests, and selected ignored PostgreSQL notification tests. Nix emitted non-fatal unavailable remote-builder and store hard-link warnings. No commit created.

2026-08-30 browser/harness remediation implemented without a browser/Nix run. `runTask433ProductionEvaluation` now waits with native `setTimeout`, independent of the page timeout scaling. Static contracts isolate each canonical workflow and require its expected production-evaluation call count while continuing to forbid direct assessment/result SQL. Manifest validation rejects duplicate step names. Diagnostic screenshots remain exported but are excluded from visual baseline counts. The Nix driver now parses results, prints failed-step journals, copies reports/screenshots, records critical and visual failures, and only then rejects a nonzero integration exit. All stale generic PolicyDrawer dialog names in the browser test now use exact rendered policy titles; the editor-only hidden-kind assertion uses `policy-editor-modal`. Verification passed: `node --check checks/web-ui/tests/integration-test.js`; `CF_WEB_UI_SOURCE_DIR=checks/web-ui CF_UI_STATIC_CONTRACTS=1 node checks/web-ui/tests/integration-test.js` (`web-ui harness static contracts OK`); `git diff --check`. No expensive Nix/browser check and no commit.

2026-08-31 eval_passed P1 remediation: `persist_eval_passed_for_system_in_tx` now persists each exact eval_passed outcome, detail, evidence, policy-version ID, and rule ID from the authoritative `PolicyCheckResult`; it no longer accepts or writes a caller-provided provisional Pass. The production evaluation persistence path passes its policy check directly. Expanded the live-DB attempt-evidence regression with a terminal metadata Error followed by `mark_commit_evaluation_failed`; because failure handling only repairs NotChecked rows, the assertion discriminates against the former false-Pass bug and proves the original metadata Error remains authoritative. Verification passed: targeted `composite_policy` regression (1 passed), backend rustfmt check, and `git diff --check`. Existing unrelated compiler warnings remain. No commit created.

2026-08-30 UI re-review remediation follow-up: Evidence routing now tracks the requested system independently of asynchronous evidence payload state, does not initialize the bundle drawer behind evidence, closes competing bundle state when evidence opens, and clears route identity on dismissal. Added a state-level route regression plus browser reload/Back/Forward assertions. Added shared WASM dialog focus capture, boundary trapping, fallback focus, and opener restoration to policy, compliance/evidence, and POA&M dialogs. Notification menu opening now moves focus into the menu; rows use stable test IDs; dismiss keyboard events do not activate parent navigation; browser assertions cover ArrowDown, Escape, focus restoration, and keyboard dismissal. Policy child controls stop key propagation, with browser assertions for card Enter, drawer restoration, and keyboard Edit isolation. Setup Coach now accurately states that agent completion requires administrator acknowledgement after report-in while retaining the existing registered-system prerequisite and server behavior. Verification passed: web-ui rustfmt check; targeted evidence-route and coach tests; wasm32 cargo check; browser harness static contracts; Node syntax check; git diff check. Existing warnings remained. Playwright, Nix builds, and full suites were intentionally not run. No commit created.

2026-08-31 final browser/harness review remediation: `integration-test.js` now captures full unhandled-rejection diagnostics, sets `process.exitCode = 1` immediately, and emits a synthetic failed harness result after browser shutdown so normal JSON/Markdown reports remain available. The static contract now isolates the `runTask433ProductionEvaluation` body, requires its `/re-evaluate` call, and rejects INSERT, UPDATE, DELETE, or MERGE writes to composite assessment/rule-result tables in both the helper and canonical workflow bodies. Verification passed: `nix develop -c node --check checks/web-ui/tests/integration-test.js`; `nix develop -c env CF_WEB_UI_SOURCE_DIR=checks/web-ui CF_UI_STATIC_CONTRACTS=1 node checks/web-ui/tests/integration-test.js` (`web-ui harness static contracts OK`); focused `git diff --check`. No browser/Nix integration check and no commit.

2026-08-31 final UI review remediation implemented: assignment-list API failures now render before the empty state with a focused retry path. Nested POA&M create/link and assignment-link dialogs stop keyboard propagation before handling Escape, so busy children cannot dismiss parent evidence/bundle drawers. Compliance evidence-source, assignment-link, and system-detail evidence loading/error/context overlays now use shared focus capture, boundary trapping, opener restoration, modal labeling, and topmost Escape behavior. `SystemDetailView` reconciles tab state when router props change and listens for browser popstate tab changes; a pure route/query precedence regression covers the mapping. Verification passed: web-ui rustfmt check; focused route-tab unit test; wasm32 cargo check; git diff check. Existing unrelated warnings and concurrent worktree changes remain untouched. No browser/full suite, expensive build, or commit was run.

2026-08-31 relationship pagination P2 remediation implemented in `packages/web-ui/src/views/poam_api.rs`. Finding and assignment relationship DTOs now deserialize server pagination metadata with missing-field defaults for older responses. Both relationship clients request 100-row pages, validate shared forward cursors, merge and deduplicate by stable relationship/POA&M identity, and fail through `PoamApiError` rather than return a partial list after the 10,000-record safety boundary. Existing assignment counts/lists and finding history now consume the complete merged vectors without component changes. Added pure compatibility, finding-page merge, assignment-page merge, deduplication, and invalid-cursor tests. Verification passed: web-ui rustfmt check; `cargo test --manifest-path packages/web-ui/Cargo.toml relationship_` (3 passed); `git diff --check`. Existing warnings remain. No expensive checks or commit were run.

2026-08-31 final relationship-pagination compatibility remediation implemented. Relationship services now preserve the original complete response only when both history parameters are absent, with an explicit `COMPATIBILITY` contract; offset-only requests return `invalid_relationship_pagination`, while the current web client continues to send bounded limit/offset pages. Finding pages now order by immutable `linked_at` and link ID while keeping active relationships first; assignment pages order by immutable `added_at` and POA&M ID. Migration 0242 adds matching relationship-order indexes. The focused PostgreSQL regression proves no-parameter assessment, stable-finding, and assignment calls return all rows and that large `poams.updated_at` changes do not alter page order. Verification passed: backend rustfmt check; focused live-DB `relationship_services_batch_active_history_and_immutable_assignments` test (1 passed); web rustfmt check; web `relationship_` tests (3 passed); `git diff --check`. Existing unrelated warnings and concurrent worktree changes remain untouched. No broad checks and no commit were run.

2026-08-30 notification bootstrap cast remediation implemented without editing applied migration 0241. Unapplied migration 0242 now replaces `backfill_user_notification_source_events` with category-and-UUID-shape `CASE` guards for every system/POA&M conversion in eligibility and materialization joins while preserving the advisory lock, tuple cursor, 256-row bound, insert-only queue behavior, and completion updates. Added an isolated PostgreSQL regression with `cves.subject_id='5'` and historical malformed system/POA&M subjects; it proves the CVE event queues, invalid scoped rows do not queue, and the cursor completes at the valid source. Verification passed: backend `cargo fmt --all --check`; `git diff --check`; migration syntax/application and focused test via `notification_bootstrap_safely_skips_malformed_scoped_subject_ids` against repository PostgreSQL at 127.0.0.1:3042 (1 passed). Existing unrelated compiler warnings remained. No broad suite and no commit.
<!-- SECTION:NOTES:END -->

## Comments

<!-- COMMENTS:BEGIN -->
author: openai-agent
created: 2026-08-28 16:42
---
Parallel-work coordination (2026-08-28): TASK-440 is intended to proceed in parallel but merge only after TASK-433. TASK-440 overlaps System Detail/Compliance UI, server models/queries/handlers, migrations, SQLx metadata, and browser checks. Please record migration numbers as they are allocated so TASK-440 can rebase onto TASK-433's merged result and use subsequent additive migration numbers without editing TASK-433 migrations.
---
<!-- COMMENTS:END -->
