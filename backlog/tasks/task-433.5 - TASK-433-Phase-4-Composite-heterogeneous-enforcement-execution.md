---
id: TASK-433.5
title: 'TASK-433 Phase 4: Composite heterogeneous enforcement execution'
status: Review
assignee:
  - '@opencode-agent'
created_date: '2026-08-23 01:42'
updated_date: '2026-09-02 02:40'
labels:
  - design-parity
  - policy
  - enforcement
  - server
  - phase-4
dependencies:
  - TASK-433.4
references:
  - TASK-433
  - TASK-433.1
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/318'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2791038401'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2791067463'
documentation:
  - docs/design/CrystalForge/data-enforcement.js
parent_task_id: TASK-433
priority: high
type: feature
ordinal: 471000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Objective
Parent umbrella: TASK-433, phase 4 of 8 (contextual only). Implements complete backward-compatible enforcement execution across all visible enforcement kinds at the correct evaluation phase, with full DTO -> validation -> storage -> execution -> result/evidence -> read-back -> import/export coverage.

## Explicit scope
- Verify and correctly phase-execute: `nixos_option`, `packages_installed`, `packages_absent`, `custom_eval`, `cve_block`, `eval_passed`, `pin_required`, `time_window`, `approval_required`, `rollout_percent` at the correct evaluation/package, scan/build, source/evaluation or deployment phase.
- Every visible enforcement kind has a complete UI -> DTO -> validation -> storage -> execution -> result/evidence -> read-back/import/export path, or is hidden if incomplete.
- Mixed Nix/evaluation-phase plus non-Nix rule sets (e.g. Nix option rule plus CVE-block rule in one policy) persist and evaluate together with all-semantics aggregation and visible per-rule outcomes.
- Recommendations remain suggestions only (never silently enforced).
- For every exposed enforcement kind: tests cover create, validate, persist, reload, correct phase, pass, fail, error/not-checked, edit, evidence and import/export.

## Explicit non-scope
No POA&M changes. Do not flatten phases into Nix. Do not weaken Pass/Fail semantics.

## Verification
```bash
nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml -- --check
nix develop -c cargo test --manifest-path packages/default/Cargo.toml
nix build .#packages.x86_64-linux.server --no-link
nix build .#checks.x86_64-linux.server-regressions --no-link
```
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Every visible enforcement control has a complete DTO, validation, storage, execution, result/evidence, read-back and import/export path or is hidden.
- [ ] #2 Mixed Nix/evaluation-phase plus non-Nix rule sets persist and evaluate with all semantics and visible constituent outcomes.
- [ ] #3 For every exposed enforcement kind tests cover create, validate, persist, reload, correct phase, pass, fail, error/not-checked, edit, evidence and import/export.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Phase 4 implementation plan

1. Preserve legacy policy execution unchanged and fix the current enforced-composite loader fallback so malformed/unsupported enforced policies cannot degrade to an empty policy set.
2. Keep composite schema version 1 and stable rule UUIDs. Add typed variants only for kinds completed in this phase: `packages_absent`, `eval_passed`, `pin_required`, and `time_window`, alongside the existing four. Keep `approval_required` and `rollout_percent` hidden because repository inspection found no authoritative exact-target approval workflow and no production caller that advances canary phases; exposing either would create a control that saves but does not safely govern deployment. Do not add arbitrary JSON variants or start TASK-415/Phase 5.
3. Add a phase-neutral outcome model with explicit Pass/Fail/Error/NotChecked, stable policy-version/rule identity, phase, detail, evidence, and deterministic `all` aggregation. Add additive normalized assessment/rule-result persistence scoped by exact system, derivation/target, policy lineage/version, and effective config/set identity, with guarded transactional phase merges so later phases preserve earlier outcomes and stale versions/results cannot authorize deployment.
4. Extend the Nix/config executor for `nixos_option`, `packages_installed`, `packages_absent`, and `custom_eval` using stable rule-ID keys, shared legacy package-name semantics, target-authoritative dynamic option lookup, one safe typed semantic-value-to-Nix encoder, and contained try-eval/non-boolean Error outcomes. Record `eval_passed` from real per-configuration evaluator completion/failure and `pin_required` from Nix-resolved immutable source revision metadata rather than display strings.
5. Add scan-phase `cve_block` evaluation against the newest scan attempt for the exact derivation. Missing/active is NotChecked, failed/unavailable is Error, completed counts evaluate Pass/Fail; batch/load once per derivation and persist scan identity/evidence.
6. Add deployment-phase `time_window` through the existing timezone-aware service with strict create/import validation and deterministic injected-time tests. Create one authoritative final composite authorization aggregate consumed by deployment entry points; Fail/Error/due-phase NotChecked blocks. Preserve explicit legacy behavior and keep approval/rollout controls hidden.
7. Expose constituent outcomes through the smallest existing API/evidence surfaces and Web UI result presentation: overall status plus ordered rule kind/phase/status/detail/evidence. Update Add Enforcement recommendations/chooser so only the eight complete kinds are visible and recommendations remain suggestions.
8. Preserve JSON/TOML/CF-native interchange envelopes and add exact round-trip tests for every exposed kind. Add create/validate/persist/reload/edit/phase/pass/fail/error-or-notchecked/evidence/import-export coverage, plus the central mixed `nixos_option + cve_block + time_window` lifecycle regression and failure permutations.
9. Update enforcement architecture documentation, SQLx metadata if schema/checked queries change, authoritative browser workflow, and the task's per-kind coverage matrix.
10. Run all required server/Web UI/Nix/SQLx/browser/flake gates. Then stop coding for independent requirements, data integrity, performance, UI/error-state, E2E, and regression-adequacy review. Resolve every P0/P1/P2 before checking exactly AC1-AC3, moving TASK-433.5 to Review, updating MR !318, committing/pushing, and stopping before Phase 5.

Independent evaluator/security review remediation (2026-08-25): keep changes limited to evaluator/domain backend files. Replace invalid option lookup with strict quoted-path parsing plus fold-based target config traversal; make bulk/standalone policy expressions share canonical `config`; contain malformed composite custom_eval by authoritative Nix parse validation and a static Error result rather than embedding invalid syntax; compare exact requested and Nix-resolved immutable revisions and fix flake rev replacement; derive eval_passed outcomes from terminal evaluation/error helpers; add a dedicated Nix string encoder and direct environment.systemPackages pname contract/validation; classify malformed enforced policy data as deterministic while DB/infrastructure loader failures remain transient and restore legacy conflict behavior. Verify with cargo fmt, focused CARGO_BUILD_JOBS=1 Rust tests, and ignored authoritative Nix evaluation tests.

Review synchronization: fetch current `origin/dev`, merge it into the dedicated TASK-433 branch, resolve every conflicting file manually while preserving `origin/dev` backlog/task state outside TASK-433, verify no TASK-422/TASK-432 branch contamination, review enforcement mutation/delivery semantics, run focused server regressions and only rerun composite browser workflows if relevant UI/test files conflict, then update TASK-433.5/MR !318, commit, push, and leave exact-head CI running. No Phase-4 feature changes and no Phase 5.

Phase-8 owner remediation: remove the arbitrary unknown-store-path authorization fallback. No-policy historical compatibility may authorize only an exact store path with immutable evidence that the same system previously observed or was authorized for that target; known uncached/failed and completely unknown paths fail closed. Add direct generation-rollback and delivery regressions. Bound composite rule count and move potentially blocking custom Nix validation off async request workers according to existing server patterns. Require CSRF on TASK-433-touched manual deployment and rollback mutations. Correct stale public helper semantics for Composite policies. Reconcile the complete eight-kind validation/storage/phase/outcome/evidence/interchange matrix with discriminating current tests, and add missing coverage rather than weakening the criterion.

Exact-head Phase-4 P1 remediation (2026-08-31): keep the existing attempt-evidence tables and composite assessment model. On generic evaluation-attempt failure, transactionally convert current Pass/NotChecked eval_passed attempt evidence to Error, update the matching exact-commit assessment rule rows to Error under the established POA&M system/finding lock order, and recompute aggregates before retry scheduling commits. When a queued newer attempt starts, initialize its NotChecked evidence and transactionally reset matching prior composite eval_passed rules to NotChecked under the same locks, so authorization cannot consume an earlier Pass while evaluation is active. Add PostgreSQL regressions that reach these transitions through mark_commit_evaluation_failed and mark_commit_evaluation_started plus initialize_eval_passed_attempt, assert both evidence layers and authorize_deployment_at, and add a direct pin_required terminal-Pending NotChecked assertion. Run targeted rustfmt and the exact composite_policy test through nix develop; do not commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Phase-4 preflight began from clean shared task worktree `/home/mcamp/code/crystal-forge/TASK-433-policy-poam-workflows`, branch `TASK-433-policy-poam-workflows`, HEAD/base branch history containing reviewed Phase-3 remediation `34ceb84a`. Exact Phase-3 MR pipeline 2787525864 was verified successful at that SHA. No Phase-4 implementation commit exists after `34ceb84a`; Phase 5 remains out of scope.

## Pre-implementation enforcement coverage matrix (current head `34ceb84a`)

| Kind | UI | DTO/validation | Storage/read-back/import-export | Existing executor/result/evidence | Phase-4 decision |
|---|---|---|---|---|---|
| nixos_option | visible | typed composite | exact JSONB + JSON/TOML/CF-native envelope | composite rejected; no constituent outcome | implement target-authoritative Nix/config execution |
| packages_installed | visible | typed composite | exact envelope | only legacy require_packages expression; no composite outcome | implement shared package executor |
| packages_absent | absent | none | none | none | add typed complete path |
| custom_eval | visible | typed composite | exact envelope | legacy custom_check only; no contained constituent Error | implement contained Nix/config execution |
| cve_block | visible | typed composite | exact envelope | legacy CVE paths are incompatible/global or broken; no persisted outcome | implement exact-derivation scan executor |
| eval_passed | hidden UI-only | no composite variant | none | evaluator has per-system success/failure but no rule outcome | add typed complete path from evaluator state |
| pin_required | absent | none | none | requested commit exists but Nix-resolved source revision is not persisted | add typed path using resolved source metadata |
| time_window | hidden UI-only | legacy standalone config only | legacy envelope only | timezone-aware service exists; transient auto-latest gate only | add typed complete path and centralized final authorization |
| approval_required | hidden UI-only | legacy standalone config only | legacy envelope only | mutable commit+lineage approval rows lack exact target/version, immutable audit, environment authorization, and delivery authorization | remain hidden; document architecture gap, do not fabricate completion |
| rollout_percent | hidden UI-only | legacy standalone config only | legacy envelope only | canary state scaffolding exists but `complete_phase` has no production caller, so rollout cannot advance | remain hidden; document architecture gap, do not fabricate completion |

Cross-cutting findings to correct: current composite loader errors are caught and replaced with an empty policy set; persisted evaluation results use policy-version UUIDs while compliance lookup uses lineage UUIDs; deployment-phase outcomes are transient; derivation-only result JSON is not safe for system-specific multi-phase accumulation; manual/rollback target writes bypass advanced gates; no existing constituent outcome model distinguishes Fail/Error/NotChecked. No production code was modified before recording this matrix and plan.

Phase 4 implemented all eight exposed kinds (`nixos_option`, `packages_installed`, `packages_absent`, `custom_eval`, `cve_block`, `eval_passed`, `pin_required`, `time_window`) across typed DTO/validation, normalized persistence, phase execution, deterministic all-semantics aggregation, evidence/read-back, interchange, and UI authoring. `approval_required`, `rollout_percent`, and `build_succeeded` remain hidden because no complete authoritative workflow exists.

Final independent requirements/security review found no remaining P0/P1/P2. Final remediation requires completed cache-push evidence for the exact derivation and exact store path, blocks known uncached targets for both set and claim paths, and preserves only genuinely absent historical store-path fallback when no composite policy is enforced.

Verified locally on 2026-08-26: `cargo fmt --manifest-path packages/default/Cargo.toml --all --check`; server package tests (1218 unit tests plus integration suites, including composite 18/18); Web UI tests (223 passed, 1 ignored); SQLx prepare check for the workspace/all targets; server package build; `checks.x86_64-linux.server-regressions`; focused Web UI workflows `20ab2` and `29b` (2/2 screenshots); and `nix flake check --keep-going -L` exit 0. The full Web UI profile internally reported unrelated failed manifest steps while the derivation returned success; TASK-438 tracks making that check fail correctly.

Phase-4 implementation committed as `ed822ee470599defa9e716b3c6028201d848115b` (`TASK-433.5: enforce composite policies end to end`) and pushed to MR !318. Exact-head pipeline 2791038401 started for that SHA and was running when the task moved to Review; CI will provide the final remote result.

Conflict-resolution synchronization (2026-08-26): fetched `origin/dev` at `50a7dafb1a2ca8c300a5a2d02db9f2d64cc65b49` and merged it into the dedicated TASK-433 worktree. The only textual conflict was TASK-433.4's backlog record; each status/plan/notes/final-summary section was inspected and resolved to the later canonical Review state. All unrelated incoming backlog files exactly match `origin/dev`; TASK-433.5 preserves the current canonical `dev` Review state and all three checked ACs. No application, Web UI, coverage-manifest, or browser-test file conflicted, and no TASK-422/TASK-432 file appears in the branch diff against `origin/dev`.

Post-resolution semantic inspection confirmed `composite_enforcement.rs` is unchanged from the reviewed Phase-4 implementation: exact derivation/store-path completed-cache evidence remains required; known uncached targets fail; only genuinely absent historical paths retain the no-policy compatibility bridge; auto-latest, manual deploy, commit rollback, generation rollback, and heartbeat delivery still use the centralized atomic authorization/claim path. Focused `nix build .#checks.x86_64-linux.server-regressions --no-link -L` passed after resolution. Composite Web UI workflows were not rerun because no relevant UI/test file conflicted, per the explicit conditional verification instruction.

Conflict-resolution merge committed and pushed as `b238ff9e969b525aa44d46ef27beb4faefc30e12` (`Merge origin/dev into TASK-433 policy work`). Local HEAD and the remote branch match; `origin/dev` is an ancestor; MR !318 reports `has_conflicts: false`. Exact-head pipeline 2791067463 started for SHA `b238ff9e969b525aa44d46ef27beb4faefc30e12` and is running. CI is intentionally left to complete remotely; Phase 5 has not started.

Exact-head Phase-4 dependency gate confirmed successful: GitLab pipeline 2791067463 completed successfully for `b238ff9e969b525aa44d46ef27beb4faefc30e12` at 2026-08-26 04:09 UTC. MR !318 is conflict-free and mergeable. TASK-433.5 remains in Review with AC1-AC3 checked; it is not moved to Done.

Phase 8 returned Phase 4 to In Progress. A P1 security defect allows a generation rollback to an arbitrary unknown `/nix/store/...` string when no composite policies are effective; the existing compatibility test codifies this unsafe fallback. Additional gaps are unbounded per-rule custom Nix validation in async handlers, missing CSRF on TASK-433-touched deployment/rollback mutations, stale public Composite helper behavior, and incomplete consolidated proof for AC3's per-kind matrix. AC1-AC3 are temporarily unchecked until focused security, execution, and matrix regressions pass.

2026-08-31 exact-head eval_passed fail-closed remediation: `mark_commit_evaluation_failed` now converts current Pass/NotChecked attempt evidence and matching exact-commit assessment rules to Error under the established POA&M system/finding lock order, then recomputes assessment aggregates in the same transaction. `mark_commit_evaluation_started` now resets every exact-commit eval_passed assessment to NotChecked in its claim transaction, before policy loading or attempt-evidence initialization, so deployment authorization cannot consume a prior Pass during a retry. Existing terminal Fail/Error evidence remains authoritative. Added live PostgreSQL regressions that create a real per-system Pass, invoke the production failure handler and manual reset/start paths, and assert attempt evidence, assessment/rule evidence, and `authorize_deployment_at` all fail closed. Added direct pin_required terminal-Pending NotChecked coverage. Verification: `nix develop -c cargo fmt --manifest-path packages/default/Cargo.toml --all -- --check` passed; full isolated-PostgreSQL `composite_policy` suite passed 21/21 with `DATABASE_URL` and `CRYSTAL_FORGE_TEST_DATABASE_URL` set to the repository-owned 127.0.0.1:3042 database and `SQLX_OFFLINE=true`; `git diff --check` passed. An initial suite invocation without the isolated URL compiled but failed all SQLx database cases at setup due `_sqlx_test` schema permission, while the direct non-DB pin test passed. Existing unrelated warnings and concurrent worktree changes remain; no commit created.
<!-- SECTION:NOTES:END -->
