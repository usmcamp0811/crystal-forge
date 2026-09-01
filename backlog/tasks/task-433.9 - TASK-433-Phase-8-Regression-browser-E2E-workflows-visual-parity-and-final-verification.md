---
id: TASK-433.9
title: >-
  TASK-433 Phase 8: Regression, browser E2E workflows, visual parity, and final
  verification
status: In Progress
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:43'
updated_date: '2026-09-01 13:35'
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

Focused browser harness remediation requested 2026-08-30: preserve the existing integration.exit wait/status change and add static source assertions for its ordering; restore an honest visual-report schema by classifying every themed capture against the manifest policy and making any strict policy without a successful comparator a reported failure; validate manifest settings and per-step field types/enums before browser launch; replace SQL-authored composite assessments/results with production commit re-evaluation through POST /api/v1/commits/:id/re-evaluate, using only persisted completed-scan input rows as deterministic external-scanner fixture state because the server exposes no authenticated deterministic scan-result ingestion endpoint; poll for and assert production-authored assessment/rule rows; use the same re-evaluation path for POA&M FAIL and PASS; add and then remove a compatible finding link through the common POA&M UI/API lifecycle; run only Node syntax, manifest/static regression scripts, Nix parse/evaluation as practical, and git diff checks. Do not run the full web-ui Nix check.

Focused non-overlapping review remediation (2026-08-31): extend `policy_counts_defect` with a live HTTP Viewer session that has zero environment memberships and assert that exact-version usage returns no systems while retaining non-sensitive bundle-version metadata. Do not edit PolicyDrawer or browser harness files owned by concurrent agents. Record the current design-file classification and a truthful visual artifact inventory in task evidence; screenshots and visual comparisons remain pending until the final browser gates run. Run only targeted formatting/static checks, not expensive suites.

Final-candidate freeze review found that step-local viewport changes can leak into later strict captures. Reset the shared Playwright page to the manifest viewport before each step, rerun static contracts, regenerate/review any affected strict baselines, and rerun the normal strict gate before freezing the SHA.

Complete the repository-required Rust documentation pass for TASK-433 public APIs and components before freezing. Correct stale SQLx cache rationale, run formatting and rustdoc checks, then inspect the aggregate diff again.

Post-candidate review remediation: after TASK-433.7 fixes explicit remediation-plan persistence, extend the non-blocking design-parity manifest and contracts with deterministic authoritative targets for the common policy editor and POA&M detail tray where production fixture navigation permits. Record a per-surface authoritative design comparison for policy editor, POA&M detail/lifecycle, compliance/evidence, system/bundle rollups, dashboard/watchlist, notifications, Setup Coach, catalog, and responsive themes. Explicitly retain reviewed contract-compatible deviations, including Provenance presentation and production-only server-authoritative plan details/assignment sections. Then freeze a new SHA, run focused and required gates, push, require green exact-head CI, and reconcile TASK-433.1-.9 plus parent AC1-AC40. Pipeline 2809426014 proves only superseded candidate `da7ae7de` once a remediation commit exists.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Initial Phase-8 independent audits on production-equivalent head `4e77d7db` (bookkeeping head `f0471b6e`) found merge blockers. Canonical browser workflows are incomplete or non-gating: the large catalog and bulk delete are route-mocked; Unmapped+real Nix enforcement is split across tests; imported STIG refinement does not add enforcement/reopen lineage; mixed Nix+CVE outcomes are inserted directly into SQL; and the POA&M workflow stops at closure rejection while successful rollup fixtures directly change assessment outcome. Required policy workflows are absent from the Web UI check's blocking list. Phase-8 AC1, AC4, AC6, AC7, AC9, AC10, and AC11 remain unproven.

Owning-phase product gaps were also found and will be remediated under their original tasks before Phase 8 resumes: Phase 1 bulk-delete CSRF/direct API proof; Phase 2 policy/mapping mutation CSRF plus common editor dialog keyboard/accessibility and reference hierarchy; Phase 4 arbitrary unknown `/nix/store` rollback authorization, bounded composite validation, stale unsupported-composite helper semantics, direct rollback/deploy CSRF, and complete exposed-kind outcome matrix; Phase 5 source-neutral legacy finding verification/closure/rollups, direct Open→Awaiting Verification transition, bounded rollup history, authoritative POA&M suite gating, and upgrade coverage; Phase 7 notification producer scaling/gating. Each owner will return to Review only after focused tests and independent re-review.

Current static parent matrix: 31 PASS, 8 FAIL, 1 UNVERIFIED. Confirmed failures are AC28 state matrix, AC31 exact-final-SHA verification, AC33 canonical Unmapped+Nix workflow, AC34 imported STIG refinement workflow, AC36 real mixed execution browser workflow, AC37 complete POA&M lifecycle browser workflow, AC38 complete exposed-kind matrix, and AC40 visual/artifact report. AC29 backward compatibility remains UNVERIFIED pending final-head reconciliation. Parent checkboxes remain unchanged.

Post-remediation independent review found no P0 findings but found three backend P1/P2 gaps: single-policy DELETE still lacks RequireCsrf and direct regression coverage; migration 0238 rejects valid pinned historical policy versions by requiring mutable current-version pointers; notification candidate generation remains users×history before per-user rank limiting. UI review found one P1 focus-containment escape while the dialog container is focused and four P2 accessibility/state issues: tab orientation semantics, mapping draft loss across tabs, unreliable return focus, and incomplete validation/error associations. These findings remain merge blockers and will be fixed under their already-active owning phases before canonical browser workflow work resumes. Browser exploration also confirmed direct TASK-433 mutation fetches must use the existing CSRF-safe phase6Api helper and that only POA&M fragments are currently critical; the six canonical workflows need independent production-path results, meaningful final screenshots, and required-result gating.

Canonical browser remediation now defines six independently named persisted production-path workflows and a seventh critical catalog mutation regression. All direct TASK-433 mutations use CSRF-safe helpers; required-result gating handles normal and focused runs; intermediate, ordinary failure, and fatal artifacts are exported. Independent browser review found the six criteria materially asserted after replacing direct outcome changes with production commit reevaluation/scan ingestion. Backend review loops added queue/cursor bounding, current authorization, authoritative closure checks, clean upgrade coverage through migration 0240, and corrected attention cleanup/bootstrap ordering; server-regressions passes with 22 POA&M tests, notification query/worker suites, clean migration and populated upgrade rehearsal. Editor review loops corrected focus containment/restoration, responsive tab semantics, mapping draft/error behavior, validation focus, pending delete, and post-persistence refresh recovery. The task's exact default-workspace formatting command is mechanically invalid for a virtual Cargo workspace (`Failed to find targets`); final verification will record that exact failure and use the equivalent valid command with `--all` as the authoritative format check. No Phase-8 AC is checked yet because exact candidate SHA and full final matrix are not complete.

Focused browser remediation on 2026-08-30: `task433-canonical-poam-lifecycle` passed in the paired focused web-ui run after verifying persisted milestone data and exact `closed` activity semantics. `20af-policy-catalog-selection-delete-regressions` then passed independently with a real 61-deleted/1-skipped partial deletion after publishing the retained policy through production trust/publish APIs. The catalog workflow now respects search-forced expansion, proves 60-card chunking and Show all persistence, suppresses the onboarding overlay, and retains selection mode for collapsed group selection. Commands: `CF_UI_TEST_PROFILE=ci_fast CF_UI_TEST_STEPS=20af-policy-catalog-selection-delete-regressions,task433-canonical-poam-lifecycle nix build .#checks.x86_64-linux.web-ui --no-link -L --impure` (POA&M passed; catalog exposed remaining harness issues), followed by the same focused build for `20af-policy-catalog-selection-delete-regressions` until green. Final green catalog log: `/home/mcamp/.local/share/opencode/tool-output/tool_051b7c89f001d2cLa0ngLOtVsq`. JavaScript syntax and `git diff --check` passed before these focused runs.

Post-resolver verification on 2026-08-30: the normal unfiltered `nix build .#checks.x86_64-linux.web-ui --no-link -L` succeeded. All six canonical TASK-433 workflows and `20af-policy-catalog-selection-delete-regressions` passed in the shared-state run. The suite captured 69/113 screenshots and copied 32 intermediate workflow artifacts. The output also reported multiple non-critical legacy screenshot checks as failures; these did not fail the derivation and are not recorded as passing. Full log: `/home/mcamp/.local/share/opencode/tool-output/tool_0533d7e59001PKXY7XHaj3kE7U`.

The post-resolver `nix build .#checks.x86_64-linux.server-regressions --no-link -L` succeeded, including migration rehearsals and the bounded bundle-summary query test. Full log: `/home/mcamp/.local/share/opencode/tool-output/tool_0535295ff001m1XJdTWBYP0dtC`. The combined static command `nix develop -c node --check checks/web-ui/tests/integration-test.js && nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check && git diff --check` also succeeded.

Shared-state browser remediation on 2026-08-31: reproduced `20ad-stig-nixos-assertion-roundtrip` after `20ac-stig-import-reconciliation-fixture`. The compliance route's late render replaced the first Import/Export menu instance. Updated 20ad to clear the stale STIG draft and reopen the settled menu trigger when the first menu action does not become visible. Focused sequence passed: `CF_UI_TEST_PROFILE=ci_fast CF_UI_TEST_STEPS=20ac-stig-import-reconciliation-fixture,20ad-stig-nixos-assertion-roundtrip nix build .#checks.x86_64-linux.web-ui --no-link -L --impure`; log `/home/mcamp/.local/share/opencode/tool-output/tool_05513fbde001JBD7dJXtrNA3hB`.

The subsequent unfiltered gate passed: `nix build .#checks.x86_64-linux.web-ui --no-link -L`. All TASK-433 critical workflows passed in shared order, including 20a, 20ad, all six canonical workflows, and 20af. The gate captured 80/113 screenshots and 36 intermediate artifacts; non-critical legacy checks still report failures but do not fail the derivation. Full log: `/home/mcamp/.local/share/opencode/tool-output/tool_055210f73001g8xpvpPWO57pFL`. Post-edit JavaScript syntax, default/web-ui Cargo formatting with `--all`, and `git diff --check` also passed. Exact-final-SHA verification remains pending because the remediation is not committed.

Focused harness/workflow remediation implemented without running the expensive web-ui VM check. `default.nix` now waits for `integration.exit`, rejects nonzero status even when `results.json` exists, runs embedded static harness contracts, and validates the visual report counts/failures schema. `integration-test.js` now exits nonzero after writing diagnostics for any failed browser step; validates manifest field types/enums and canonical production-data metadata; classifies every themed capture into match/diff/new/skipped/error; reports strict non-match failures; and has static guards against canonical SQL-authored assessment/result writes. The large-catalog manifest now accurately declares interactions. Canonical mixed Nix+CVE and POA&M workflows now trigger POST `/api/v1/commits/:id/re-evaluate` and poll production-created assessments/results for FAIL and PASS. SQL arranges only systems and deterministic completed external-scanner input because the server exposes no authenticated deterministic scan-result ingestion endpoint. POA&M now production-evaluates a same-lineage second system and links then unlinks its FAIL finding through the common detail UI. Updated `docs/web-ui-check.md` for failure and optional-baseline behavior. Focused checks passed: `nix develop -c node --check checks/web-ui/tests/integration-test.js`; `nix develop -c env CF_UI_STATIC_CONTRACTS=1 node checks/web-ui/tests/integration-test.js`; `nix-instantiate --parse checks/web-ui/default.nix`; manifest JSON parse; `git diff --check`. The full web-ui Nix check was intentionally not run per user request. Residual architecture boundary: deterministic scanner completion can only be arranged in persistence; a true external ingestion test requires an authenticated scanner/builder result-submission API that validates authorization and calls the existing `save_scan_results`/scan-phase domain transaction.

Current design classification and visual artifact inventory (read-only classification refresh, 2026-08-31): authoritative product-behavior sources are `app.jsx`, `ComplianceView.jsx`, `DashboardView.jsx`, `PoamViews.jsx`, `PoliciesView.jsx`, `PolicyEditor.jsx`, `SetupCoach.jsx`, `Shell.jsx`, `SystemDetail.jsx`, `data-dashboard.js`, `data-enforcement.js`, `data-mappings.js`, `data-poam.js`, applicable non-fixture portions of `data-policies.js`, and `styles.css`. Their covered production surfaces are policy catalog; unified editor and PolicyDrawer; compliance/evidence; POA&M detail/lifecycle; system and bundle rollups; dashboard Summary/Watchlist; notifications; Setup Coach; shell/navigation; and responsive styling. Demo-only mechanisms remain explicitly not ported: `.thumbnail`; `crystal-forge.html`; `fixtures/crystal-forge.fixtures.js/.json`; fixture IDs in `data.js`; `POAM_FINDING_STATUS_OVERRIDE`; synthetic `POLICY_STIG_BULK`; showcase-only `POLICY_EDITOR_DEMO`; in-memory POA&M arrays; localStorage authority; `CustomEvent` POA&M synchronization; `window.__cfCoach`; and timeout sequencing hacks. Current artifact inventory status: the six canonical workflow definitions and capture points exist for large catalog, Unmapped Nix, imported STIG refinement, multiline DoD banner, mixed Nix+CVE evidence, and complete POA&M lifecycle. No screenshot, visual-difference report, montage, baseline comparison, or exact-final-tree artifact was generated or inspected in this focused session. AC40 and TASK-433.9 AC11 remain unproven until the final browser/screenshot checks generate artifacts and a reviewer records per-surface desktop/narrow and light/dark differences. No authoritative design file was modified.

Focused authorization regression update (2026-08-31): `policy_drawer_owner_and_usage_respect_environment_visibility` now creates a second Viewer with zero environment memberships and calls the production exact-version usage HTTP handler. The test requires HTTP 200, retains non-sensitive bundle membership, and requires an empty system list; existing one-membership Viewer and fleet-wide Admin assertions remain. Targeted `rustfmt --check` through `nix develop` and changed-file `git diff --check` passed. The live database test was intentionally not run because the user prohibited expensive checks. PolicyDrawer label UI coverage was not added because `packages/web-ui/src/views/policies.rs` and `checks/web-ui/tests/integration-test.js` are concurrently owned. Safe follow-up after those owners finish: add a pure attribution selector adjacent to PolicyDrawer and unit cases proving manual revisions render `Owner` from `created_by_display`, imported revisions render `Imported by` from `provenance.imported_by_display`, and revision changes select attribution from the displayed exact revision; then update/execute the owning PolicyDrawer browser assertion without changing attribution semantics.

2026-09-01 browser/visual closure checkpoint after rebase onto `origin/dev` `a977a3b9`: corrected stale Enforcement-tab assertions to target the visible exact punctuated callout text, stabilized narrow-tab intersection semantics, waited for the loaded catalog before 20a mapping interactions, and made resumed STIG refinement wait for its restored tab before falling back to reconciliation. Independent review of the complete update-mode artifacts found three P2 presentation defects; fixed policy-card ID wrapping, opaque/cue-separated narrow editor tabs, and dark completed-POA&M action contrast. Focused catalog/multiline/POA&M plus 20a and 20ad runs passed. A complete unfiltered update-mode run then succeeded with all seven strict workflows and 60 strict captures present. Post-fix independent review found no P0-P2 blocker and accepted one P3 narrow-tab clipping cue. Approved all 60 strict PNGs from `/nix/store/vqgg0m5xsqg124wqicskqfdsl3v2ky2v-vm-test-run-crystal-forge-web-ui-mega-integration/screenshots`. The normal gate `nix build path:.#checks.x86_64-linux.web-ui --no-link -L` passed with `60 match, 0 differ`; 34 known non-critical legacy steps still report failure, and the non-blocking design gauge remains 30/34 because system-detail and compliance-evidence pairs are missing. Exact-final-SHA full matrix remains pending because changes are uncommitted.

2026-09-01 pre-freeze review found a deterministic visual blocker: `20ac-policy-editor-category-and-imported-provenance` leaves the shared page at the narrow viewport, so later final baseline dimensions can depend on profile and step order. It also found stale SQLx cache prose and broad missing rustdoc across newly added TASK-433 public Rust APIs. Candidate remains uncommitted and is not ready to freeze. Added the per-step manifest viewport reset and corrected the SQLx cache comment; Node syntax, harness static contracts, and `git diff --check` pass after those two edits. Baselines and Rust documentation still require remediation and verification.

2026-09-01 pre-commit closure: added a per-step manifest viewport reset and disabled finite CSS animations for strict screenshots. Regenerated and independently reviewed all 60 strict captures; no P0-P2 visual defect remains. Accepted P3: POA&M verification timestamps vary without changing layout. Hardened `approve-baselines.sh` so read-only Nix-store captures replace existing baselines and become writable; its fixture test now proves re-approval. Updated all 60 approved baselines. The normal gate `nix build path:.#checks.x86_64-linux.web-ui --no-link -L` passed with `60 match, 0 differ`; all seven TASK-433 strict workflows passed, while 34 known non-critical legacy steps still report failure and the non-blocking design gauge remains 30/34.

Completed the TASK-433 Rust documentation pass across new and materially changed public server and Web UI APIs, including lifecycle transitions, error contracts, fields, and component authority boundaries. Corrected stale SQLx cache and design-parity harness documentation. `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check`, Web UI formatting, Node syntax, harness static contracts, approval-script fixtures, and `git diff --check` pass. `SQLX_OFFLINE=true cargo doc --manifest-path packages/default/Cargo.toml -p cf-server --no-deps` and Web UI `cargo doc --no-deps` pass with pre-existing rustdoc warnings outside TASK-433 scope. A first server rustdoc attempt without `SQLX_OFFLINE=true` failed because the ambient database user lacks table permissions; the offline rerun is the valid documentation build. Independent final review found no P0-P2 pre-commit blocker. Candidate remains uncommitted pending explicit commit authorization; exact-final-SHA matrix has not started.

Exact candidate SHA is `da7ae7de480c485ee79db676f8d99893bf07c572` (`Finalize TASK-433 verification candidate`). The worktree is clean after the commit. Exact source checks: the task-listed default-workspace `cargo fmt` command fails mechanically with `Failed to find targets`; the equivalent `--all` command passes. Web UI formatting, Node syntax, harness static contracts, approval-script fixtures, and `git diff --check` pass. `nix build .#packages.x86_64-linux.server --no-link -L` passes and includes 1,242 cf-server tests with 0 failures. The direct task-listed `cargo test` command failed during SQLx macro expansion because the ambient `DATABASE_URL` user lacks table permissions; this is an environment failure, not a test assertion failure. Per user direction, stop the remaining exhaustive local matrix and use CI for it after pushing the candidate.

Remote verification after the user's push found MR !318 at `3fd41e264f0ec41ff20e94aa791c0f040ad30649`, not candidate `da7ae7de480c485ee79db676f8d99893bf07c572`. `da7ae7de` is not an ancestor of the remote head. The remote tree omits the final remediation commit, migrations 0243-0244, approval tooling, and all 60 strict baselines; GitLab reports `has_conflicts: true`. Pipeline 2809402843 is pending for `3fd41e26` and is not valid exact-candidate evidence. Do not use that pipeline for TASK-433 closure. The likely cause is that the original TASK-433 worktree/branch was pushed instead of `/tmp/opencode/TASK-433-publish` branch `TASK-433-publish-temp`.
<!-- SECTION:NOTES:END -->
