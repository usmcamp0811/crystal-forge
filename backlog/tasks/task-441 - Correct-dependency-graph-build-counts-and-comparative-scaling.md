---
id: TASK-441
title: Correct dependency graph build counts and comparative scaling
status: In Progress
assignee:
  - '@openai-gpt-5.6-sol'
created_date: '2026-08-29 16:26'
updated_date: '2026-09-01 20:53'
labels:
  - backend
  - frontend
  - nix
  - dependency-graph
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/322'
documentation:
  - 'https://nix.dev/manual/nix/2.35/command-ref/nix-store/realise.html'
  - 'https://manpages.ubuntu.com/manpages/jammy/man1/nix-store.1.html'
modified_files:
  - checks/server-regressions/default.nix
  - checks/web-ui/coverage-manifest.json
  - checks/web-ui/design-fixtures.json
  - checks/web-ui/tests/integration-test.js
  - docs/design/CrystalForge/components/EvalDrawer.jsx
  - docs/design/CrystalForge/styles.css
  - docs/nix-cli-invocations.md
  - >-
    packages/default/.sqlx/query-485a8f662b4e24c88b683f6612ad515e55ef9e5853b21d31b6fd1110fd106f58.json
  - >-
    packages/default/.sqlx/query-5280b273da2bdc0ecf6ef947ff5c68906f54eb2d04097d5144044d0877d86f41.json
  - >-
    packages/default/.sqlx/query-dc0147c412932e7f674ca051b52165a04c1e2673c762c15009fb25683db2254b.json
  - >-
    packages/default/.sqlx/query-e56f17ec60541ec1624e3000bae6c1d69ea17f331c24f1f5b3d81afea9cdf934.json
  - packages/default/crates/cf-builder/src/derivations/utils.rs
  - >-
    packages/default/crates/cf-server/.sqlx/query-73dfa0c67fb6960c5435ebcdbc139d2a29d9689557624f0cf2d734bd4e576f9b.json
  - >-
    packages/default/crates/cf-server/migrations/0233_dependency_build_plan_counts.sql
  - >-
    packages/default/crates/cf-server/migrations/0234_explicit_dependency_plan_state.sql
  - packages/default/crates/cf-server/src/api/models.rs
  - packages/default/crates/cf-server/src/derivations/utils.rs
  - packages/default/crates/cf-server/src/handlers/api/commits.rs
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/queries/build_jobs.rs
  - packages/default/crates/cf-server/src/queries/build_reservations.rs
  - packages/default/crates/cf-server/src/queries/builders.rs
  - packages/default/crates/cf-server/src/queries/commits.rs
  - packages/default/crates/cf-server/src/queries/derivations.rs
  - packages/default/crates/cf-server/src/server/mod.rs
  - packages/default/crates/cf-server/tests/dependency_build_plan_lifecycle.rs
  - packages/web-ui/assets/app.css
  - packages/web-ui/src/api/models.rs
  - packages/web-ui/src/views/evaluations.rs
priority: high
type: bug
ordinal: 450000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The evaluation dependency graph currently conflates closure paths, locally absent outputs, substitutions, and source builds. This can make every evaluated NixOS system appear to require its full closure to be built and can produce full-width bars for every row. Correct the graph so that “to build” means the number of dependency derivations that Nix reports it would build when realizing the evaluated system with Crystal Forge’s effective build configuration. Exclude the top-level NixOS system derivation because the graph measures dependencies. Preserve a separate, accurate dependency-derivation total where the product still needs that value. Align the API contract and UI terminology with the fact that each row represents a NixOS system, not a package. Present build work on a shared scale across systems so bar lengths compare absolute build counts. A zero-build plan, unavailable plan data, and plan failure must remain distinct states.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For each successfully evaluated NixOS system, the graph reports the number of dependency derivations that the server's Nix environment would build under the server's effective substitute and offline configuration at evaluation finalization time; the API and UI identify this as a server-side estimate rather than the eventual remote builder's exact work.
- [ ] #2 The reported build count excludes the evaluated top-level NixOS system derivation.
- [ ] #3 Dependencies that Nix would download from a substituter do not increase the reported build count.
- [ ] #4 A system whose realization requires no dependency builds reports zero builds.
- [ ] #5 Any retained dependency total counts derivations only and does not count source files, patches, configuration files, outputs, or other non-derivation store paths.
- [ ] #6 Failure or unavailability of build-plan calculation is represented separately from a valid zero-build result and does not silently become zero, the closure total, or a successful count.
- [ ] #7 The dependency graph API names rows and fields as systems and dependency/build counts rather than packages, and all in-repository consumers use the corrected contract.
- [ ] #8 Every evaluated system’s build-work bar uses one common maximum build count across the response, so equal build counts have equal widths and a count of 100 is ten times the width of a count of 10 when 100 is the maximum.
- [ ] #9 When all evaluated systems have zero builds, the graph renders a stable zero-work state without division errors or misleading full-width bars.
- [ ] #10 Failed systems remain visually and semantically distinct from systems with valid build-plan data.
- [ ] #11 Backend regression tests cover mixed derivation and non-derivation closure paths, build and substitution plans, no-op plans, singular and plural build counts, top-level derivation exclusion, and plan failure.
- [ ] #12 Frontend regression coverage verifies equal-count widths, proportional 10-to-100 widths, all-zero data, unavailable plan data, and failed-system presentation.
- [ ] #13 Affected API and maintainer-facing documentation defines the build-count semantics, top-level exclusion rule, server environment/configuration dependency, estimate limitation for remote builders, and failure behavior.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Add a durable evaluation-attempt plan barrier that records whether all graph-relevant systems for one commit evaluation attempt have reached `complete` or `failed`. Activation and every recovery/claim path must require both an individually terminal plan and the evaluation-wide terminal barrier.
2. Refactor streaming and fallback evaluation into discovery/persistence, concurrent plan capture, barrier completion, GC-root establishment, job activation, and one post-activation queue notification phase. Include build-eligible, graph-only, and fallback systems in the same attempt barrier.
3. Define re-evaluation and existing-job behavior so prior queued/building work cannot invalidate a claimed coherent snapshot. Preserve generation, lease, stale-writer, cancellation, supersession, failed-plan, and rolling-upgrade protections.
4. Add deterministic three-system semaphore, shared-store race, graph-only, fallback, failed-plan, cancellation/supersession, and recovery regressions that fail on `bcbeeb62`. Audit all insert, requeue, retry, recover, reserve, and claim paths for barrier enforcement.
5. Re-run an explicit production/design comparison for all requested content, state, interaction, hierarchy, narrow-layout, and dark/light dimensions. Update implementation only if the intended design requires it.
6. Synchronize the actual tracked TASK-441 task file into the MR branch with a clearly identified superseding verification section. Preserve historical notes, and check each acceptance criterion only after objective verification.
7. Fetch and integrate current `origin/dev`; run focused lifecycle tests, isolated migrations and SQLx checks, complete server and Web UI checks, and the authoritative browser check.
8. Perform separate final requirements, backend lifecycle, database, concurrency/recovery, query/claim, Nix semantics, frontend polling, UI/design, error-state, end-to-end, and regression reviews. Resolve every P0/P1/P2 finding.
9. Commit and push implementation and task-record updates, wait for exact-head GitLab CI, verify the remote task file and MR conflict state, then return TASK-441 to Review without merging.

Research decision: migration 0235 will place the durable barrier on immutable `evaluation_attempts` and tag each graph-relevant derivation with the attempt UUID. New attempts enter `planning`; finalization verifies the exact current-attempt derivation set and terminal `complete|failed` states, records expected/terminal counts, releases the barrier, inserts/reconciles rooted eligible jobs, marks the attempt and commit complete, and commits atomically. Claim/reservation paths require the current completed commit, matching derivation attempt UUID, and a released barrier; database triggers provide defense in depth. Existing queued jobs are held while re-evaluation is pending/in progress. Manual re-evaluation must reject active building/cancelling jobs or reservations so an in-flight build cannot mutate the snapshot. Streaming, graph-only, and fallback systems use one structured planning set; no detached planner or per-system activation remains. Recovery may activate only released attempts and can release a legacy completed attempt only after all tagged graph rows are terminal.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai-gpt-5.6-sol on gray in /home/mcamp/code/crystal-forge/TASK-441-dependency-graph-counts

Backend implementation: added migration 0233 with nullable nonnegative dependency_build_count and unavailable/calculating/complete/failed state; replaced cf-server closure cache inference with strict requisites plus configured nix-store --realise --dry-run parsing; scheduled non-blocking calculation across server/mod.rs and evaluate_with_policies.rs finalization paths; updated backend query/API to system-oriented fields and explicit serde statuses. Focused parser/config tests pass with SQLX_OFFLINE=true. SQLx preparation and migration application remain pending because the configured online database denied table access and was not verified as an isolated repository database. Frontend files in the worktree were modified concurrently and were not changed as part of this backend work.

Frontend/browser-check slice implemented in the task worktree without modifying concurrent backend files. Updated the Web UI mirror to `{ commit_id, total_systems, systems }`, added typed plan/system statuses, extracted pure shared-maximum/percentage/row-state helpers, and rendered complete zero, unavailable, calculating, plan failure, and system failure distinctly with system/dependency-derivation/build-work terminology. Added six focused Rust regressions and manifest-driven browser fixture/assertions for equal widths, 10:100 scaling, zero width/`0 to build`, all distinct states, terminology, and dark/light screenshot capture setup. Updated `checks/web-ui/design-fixtures.json` and the coverage manifest. Verification passed: targeted Web UI Rust tests (6 passed), wasm32 Web UI `cargo check`, touched-file rustfmt check, Node syntax check, JSON parsing, and `git diff --check`. Focused authoritative command `CF_UI_TEST_STEPS=26bb-evaluation-dependency-graph nix build .#checks.x86_64-linux.web-ui --no-link --impure` timed out after 15 minutes while still building the Web UI/server/VM derivation set, before browser execution; no screenshot result was produced. Existing repository warnings remained. No commit or task status transition was performed.

Implemented the TASK-441 backend/API/UI/browser slice in the dedicated worktree. The server now filters dependency `.drv` requisites, excludes the top-level system derivation, runs configured `nix-store --realise --dry-run`, counts only Nix's build section, and persists unavailable/calculating/complete/failed separately. Migration 0233 clears inaccurate legacy closure values and constrains valid count/state combinations. The API and UI now expose systems, dependency derivation counts, dependency build counts, plan status, and system status. UI bars share one maximum and preserve complete zero, unavailable, plan-failed, and system-failed states.

Local verification passed: 7 focused backend regressions (mixed requisites; singular/plural, fetched-path, no-op, malformed, failed-command, top-level and BuildConfig behavior); 6 frontend graph regressions; `SQLX_OFFLINE=true cargo check --manifest-path packages/default/Cargo.toml -p cf-server --tests`; Web UI wasm cargo check; targeted changed-file rustfmt; JS syntax and JSON parsing; `git diff --check`; isolated PostgreSQL migration application through 0233; isolated `cargo sqlx prepare --workspace` (removed one obsolete closure-count query metadata file); server and Web UI Nix package builds; server rustdoc; and focused authoritative `CF_UI_TEST_STEPS=26bb-evaluation-dependency-graph nix build .#checks.x86_64-linux.web-ui --no-link --impure`, including browser assertions and screenshot capture. Existing repository compiler/rustdoc warnings remain. No commit, push, MR, or task status transition to Review was performed.

Committed as `331ac40c` (`fix: correct dependency graph build counts`), pushed to `origin/TASK-441-dependency-graph-counts`, and opened MR !322: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/322. All declared local verification passed before review. The task now awaits MR pipeline and human review; no merge was performed.

Review remediation started for MR !322 at maintainer request. TASK-441 is returned to In Progress and all acceptance criteria are unchecked pending fresh objective verification. Required remediation includes pre-activation dependency-plan capture with deterministic concurrency proof, terminal unavailable polling, authoritative design parity, schema/legacy audit, real persistence/API coverage, current-dev synchronization, exact-head CI, and an independent P0/P1/P2 review. LOCK: openai-gpt-5.6-sol on gray in /home/mcamp/code/crystal-forge/TASK-441-dependency-graph-counts

Remediation implementation is in progress in the dedicated TASK-441 worktree. Added forward migration 0234 with explicit dependency derivation count, generation, and lease state; generation/path/attempt CAS persistence; terminal-plan gates on insertion and all claim strategies; pre-activation planning in streaming/fallback/recovery paths; explicit graph query column; deterministic PostgreSQL lifecycle/API regressions; terminal-only Web UI polling with teardown coverage; and authoritative EvalDrawer system-oriented design updates. Local server and wasm checks compile. `nix build path:.#checks.x86_64-linux.server-regressions --no-link` completed successfully after the ordinary flake invocation correctly demonstrated that untracked migration/test files are excluded. Existing repository warnings remain.

Implementation is blocked on a material acceptance-criterion decision. Independent architecture tracing confirmed that an evaluation-time server dry run cannot report what an eventual API builder will build: assignment occurs later, API builders use independent stores, Nix daemon/substituter settings, BuildConfig, and architecture, and retries can select another builder. Exact criterion #1 requires attempt-specific planning on the assigned builder after strategy preparation and before realization. Keeping the current evaluation-time graph requires redefining it as a server-side projection/estimate. These meanings cannot be combined without a new homogeneous-builder/shared-state deployment contract. Awaiting product direction before further implementation.

Final remediation verification on the uncommitted worktree passed: `nix build path:.#checks.x86_64-linux.server-regressions --no-link`; `nix build path:.#checks.x86_64-linux.web-ui --no-link`; isolated PostgreSQL migration through 0234 plus `cargo sqlx prepare --workspace --check`; targeted Rust formatting; Node syntax; and `git diff --check`. One full Web UI attempt failed after the onboarding coach obstructed unrelated CVE/policy steps; the dependency-graph step passed in that run, and an unchanged rerun of the full check passed. Migration coverage now includes 0233 rolling writes, stale old/new generation interleaving, constraints, queued/building-job expiry, and graph-only expiry. Three independent backend/schema/frontend rereviews report no remaining P0/P1/P2 findings. Existing unrelated compiler warnings and Nix store hard-link warnings remain. Exact-head CI is still pending commit and push.

Committed remediation as `bcbeeb62` (`fix: harden dependency build planning`) and pushed it to `origin/TASK-441-dependency-graph-counts`. Exact-head GitLab pipeline 2751 passed all jobs for SHA `bcbeeb62542e65202b1de95a763e523c74be869d`. MR !322 is mergeable, has no conflicts or unresolved blocking discussions, and `origin/dev...HEAD` is `0 3` after a fresh fetch. The MR description now reflects migration 0234, lifecycle hardening, resilient polling, current verification, and the server-estimate limitation. LOCK RELEASED: implementation is awaiting review; no merge was performed.

REVIEW REMEDIATION REOPENED (2026-08-30): MR !322 at `bcbeeb62` has a P1 evaluation-wide snapshot defect. Per-system terminal gating can activate system A while B/C or graph-only/fallback systems still calculate against the mutable Nix store. This remediation will add a durable evaluation-attempt barrier, deterministic multi-system ordering tests, and explicit recovery/re-evaluation semantics. The prior Review status, checked acceptance criteria, final summary, and pipeline evidence are superseded until new exact-head verification passes. LOCK: openai-gpt-5.6-sol in /home/mcamp/code/crystal-forge/TASK-441-dependency-graph-counts.

Architecture research found the exact defect at `bcbeeb62`: streaming preparation tasks perform plan -> root -> activate independently; graph-only planning uses detached `tokio::spawn`; fallback calls the combined single-system finalizer; and recovery plans/activates candidates sequentially. Claim queries gate only on each derivation's terminal state. The selected fix uses an immutable evaluation-attempt UUID barrier plus derivation tagging, atomic barrier release/job activation/commit completion, claim and reservation predicates plus trigger enforcement, and explicit re-evaluation rejection while builds are active. Historical pre-0235 attempts will be marked as legacy-released for rolling-upgrade continuity; every new attempt uses strict barrier semantics.

WIP checkpoint committed and pushed as 86a1ddc6 (`fix: enforce evaluation-wide planning barrier`) at user request. Backend release-A verification passed before commit: cf-server formatting, SQLX_OFFLINE cf-server test check, server-regressions, and diff check. Earlier focused Web UI 26bb verification passed before the last independent review. This is not final review evidence: the final frontend remediation agent was cancelled, remaining frontend P2 findings are unresolved, the tracked task file is not synchronized into the branch, exact-head CI is pending, and a fresh fetch shows the branch is 107 commits behind origin/dev.

The user selected a two-release zero-downtime transition for the legacy global derivation_path unique constraint. Release A in 86a1ddc6 retains the constraint and represents cross-commit conflicts as explicit failed plans. TASK-443 tracks release B, which removes the constraint only after release A is deployed to every server process.

Local branch rebased onto origin/dev at 701151f4. Conflicts preserved both TASK-433 and TASK-441 behavior. TASK-441 migrations were renumbered after rebase to the next available sequence: 0245 dependency counts, 0246 explicit plan state, and 0247 evaluation-wide barrier. Post-rebase formatting, SQLX_OFFLINE cf-server test check, wasm Web UI check, Node syntax check, server-regressions, and diff checks passed; existing warnings remain. Local rebased head is f2f472bc. Remote update requires an explicitly authorized force-with-lease push because rebase rewrote branch history.

User authorized a force-with-lease update after the verified rebase. Remote branch and MR !322 now point to f2f472bcf7b34a7bd2af6c8af3287dfebda9d5c6. The worktree is clean and origin/dev is an ancestor of the branch. Exact-head CI is pending.
<!-- SECTION:NOTES:END -->
