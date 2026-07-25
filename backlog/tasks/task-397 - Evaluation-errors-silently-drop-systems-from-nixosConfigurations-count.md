---
id: TASK-397
title: Evaluation errors silently drop systems from nixosConfigurations count
status: In Progress
assignee: []
created_date: '2026-07-24 00:29'
updated_date: '2026-07-25 20:20'
labels:
  - evaluator
  - reporting
  - ux
dependencies:
  - TASK-398
references:
  - 'lib/stig/default.nix:100'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/309'
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/queries/commits_artifacts.rs
modified_files:
  - packages/default/crates/cf-server/src/models/evaluate_with_policies.rs
  - packages/default/crates/cf-server/src/queries/commits_artifacts.rs
  - packages/default/crates/cf-server/src/queries/commits.rs
  - packages/default/crates/cf-server/src/queries/derivations.rs
  - packages/default/crates/cf-server/src/server/mod.rs
  - packages/default/crates/cf-server/src/flake/commits.rs
priority: high
type: bug
ordinal: 396000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

When a `nixosConfiguration` fails to evaluate (e.g. due to a NixOS module type error), Crystal Forge silently drops the system entirely from the evaluation results. It does not appear as a failed system — it simply disappears from the total count.

**Observed behavior:**
- UI header shows "Systems: 12" (Crystal Forge discovered 12 nixosConfigurations in the flake)
- Evaluation summary shows "Total: 4 nixosConfigurations" and "Successful: 4"
- `nix-builder-1` is completely absent from both the success list and any failure list
- The log shows an ERROR for the system (`services.timesyncd.enable` not of type boolean, caused by a double-wrapping bug in `mkStigModule`/`overrideAttrs`) but Crystal Forge does not record it as a failed system

**Expected behavior:**
- Systems that fail to evaluate should appear as **failed** in the evaluation summary with their error
- "Total" should reflect all discovered systems (12), not just the ones that returned a derivation (4)
- A user should never have to debug why a system "disappeared" — the error and system name should be surfaced clearly

## Root Cause (nix-builder-1 specifically)

The immediate cause for `nix-builder-1` is a bug in `crystal-forge`'s `mkStigModule` helper (`lib/stig/default.nix:100`):

```nix
overrideAttrs = attrs: mapAttrsRecursive (_: v: mkOverride 1000 v) attrs;
```

When `stigConfig` already contains `mkForce` calls (e.g. `services.timesyncd.enable = mkForce true` in the timesyncd stig module), `overrideAttrs` double-wraps the value — producing an override-of-an-override attrset instead of a plain boolean. The NixOS module system then rejects `services.timesyncd.enable` as not being of type `boolean`.

However, this bug exposed a **separate Crystal Forge reporting bug**: a system that crashes the evaluator should still be recorded as a failed/errored system, not silently removed from the total.

## Evidence

```
# CF evaluation log — error is logged but system is not counted:
[ERROR] error: A definition for option `services.timesyncd.enable' is not of type `boolean'.
        - In `.../stig-modules/modules/timesyncd/default.nix':
            { _type = { _type = "override"; content = "override"; priority = 1000; ... }

# Summary — nix-builder-1 not mentioned anywhere:
✅ Successful: 4 systems
📦 Total: 4 nixosConfigurations   ← should be at least 5 (4 success + 1 failed)

# The system evaluates fine locally (no stig-modules conflict in local store):
nix eval .#nixosConfigurations.nix-builder-1.config.system.build.toplevel
→ «derivation /nix/store/clx6f...-nixos-system-nix-builder-1-26.05...drv»
```

## Scope

Two separate fixes are needed:

1. **Crystal Forge evaluator** (this task): When `nix-eval-jobs` or the evaluation wrapper catches an error for a specific system, record it as a failed system with the error message rather than omitting it from output entirely. The "Total" count should equal successes + failures, matching the discovered system count.

2. **`mkStigModule` double-wrap bug** (separate task): `overrideAttrs` should strip existing `mkOverride`/`mkForce` wrappers before re-wrapping, or `stigConfig` values should not use `mkForce` internally since `overrideAttrs` already applies `mkOverride 1000`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Systems that fail to evaluate appear as failed/errored entries in the evaluation summary with their system name and error message
- [ ] #2 The 'Total' nixosConfigurations count in the evaluation summary equals successes + failures (matches the number of systems discovered in the flake)
- [ ] #3 A system that crashes the evaluator never silently disappears — it is always accounted for in either the success or failure column
- [ ] #4 The evaluation UI shows nix-builder-1 (and similar systems) as failed with the root cause error, not absent
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Reviewer-directed architecture correction for MR !309: replace commit-wide successful-result persistence/queueing with incremental per-system transactional finalization. Add `finalize_evaluated_system` that locks the commit attempt, records one successful derivation with existing state-preserving rules, gates build eligibility on strict policy semantics plus CF-agent requirement, and inserts/fetches an idempotent build job in the same transaction. During stdout and standalone fallback success processing, call this finalizer immediately, then only after commit run queue notification, `QueuedForBuild` broadcast, GC root, closure-count scheduling, and hardening-scan trigger. Keep commit-level finalization limited to summary/status transition and synthetic confirmed failures. Preserve fallback after nonzero `nix-eval-jobs` exit, add authoritative expected/seen/missing logging, and add focused regression tests for strict vs non-strict policy queue gating, fallback success queueing, deterministic missing failure recording, retry idempotency, and cancellation/duplicate finalize races where practical within the existing DB test harness.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Sprint readiness notes

**What this task is:** A Crystal Forge server-side evaluator bug. When `nix-eval-jobs` returns an error for a specific `nixosConfiguration`, the evaluator discards the system entirely instead of recording it as a named failure. The user sees fewer systems in "Total" than were discovered, with no indication which ones failed or why.

**Where to look:** The evaluation pipeline in the Crystal Forge server — specifically the code that:
1. Enumerates `nixosConfigurations` from the flake (this works — UI showed "Systems: 12")
2. Hands each system to `nix-eval-jobs` for evaluation
3. Collects results and builds the summary

Step 3 is where the bug lives: errors from `nix-eval-jobs` for a specific system are logged (the ERROR lines are visible in the log) but not added to any failed-systems list, so they fall out of the total count.

**Concrete trigger:** `nix-builder-1` in `ATALLC/nix-config` fails evaluation with:
```
error: A definition for option `services.timesyncd.enable' is not of type `boolean'
```
This error appears in the Crystal Forge log but `nix-builder-1` is absent from both success and failure lists.

**Dependency:** TASK-398 fixes the underlying Nix evaluation error for `nix-builder-1`. Once that is fixed, this task's fix can be verified end-to-end. However, this task's fix (surface the failure, don't drop it) is independent and should be implemented regardless.

**Verification:** Trigger a Crystal Forge evaluation of `ATALLC/nix-config`. Before TASK-398 is merged, `nix-builder-1` should appear as a **named failed system** with its error. After TASK-398, it should appear as a success.

---

## P1 review fixes (commit 07771d5c)

**P1-1 (cancellation race):** Fallback phase now returns `Err(EvaluationCancelled.into())` instead of `bail!` (typed error). `force_cancel_commit_evaluation` no longer clears `cancellation_requested` so the evaluator's poll loop can still detect cancellation.

**P1-2 (non-atomic transitions):** `mark_commit_evaluation_started` returns `EvalStartOutcome`. `mark_commit_evaluation_complete` returns `EvalCompleteOutcome` with CAS guard on `evaluation_attempt_count`. `mark_commit_evaluation_failed` returns `EvalFailureOutcome` with CAS guard. All server/mod.rs callers updated to handle typed outcomes and pass `attempt`/`expected_attempt`.

**P1-3 (retries downgrading builds):** `insert_derivation_with_target` preserves statuses 7 (BuildPending) and 8 (BuildInProgress) alongside 5, 6, 10, 12. SQL bind parameters renumbered (`$6`–`$11` for preserved statuses, `$12` for `cf_agent_enabled`). SQLx metadata regenerated.

**P1-4 (inline-discovery orphan):** `load_commit_nixos_configurations_with_creds` already had `kill_on_drop(true)` and `BuildConfig.apply_to_command`, completing the P1-4 requirement.

## SQLx metadata fix (commit a5ddcd33)

The initial `cargo sqlx prepare` only captured queries from non-test code. Test-only query! macros in cve_worker.rs and cve_scans.rs required `cargo sqlx prepare -- --all-targets` to be included. Without them, the Nix build's `cf-server (lib test)` target failed with 7 `set DATABASE_URL...` errors.

Second-pass P1 fixes (commit 8ce85715): P1-1 CAS before side effects; P1-2 record_successful_eval_result atomic; P1-3 cancel API UPDATE...RETURNING; P1-4 reset clears flag + finalizer guarded by attempt. 14 new DB regression tests. nix build exit 0.

Continued MR !309 P1 finalization fix in worktree `TASK-397-eval-errors-silently-drop`: refactored real and mock evaluator paths to return an in-memory `EvaluationPlan`; added `finalize_evaluation_attempt` as the single transaction that locks the commit row, checks attempt/cancellation state, writes successful derivations plus synthetic failures, updates commit metadata cache, and marks the attempt complete. Moved build-job creation, GC roots, closure counts, hardening scans, and `QueuedForBuild` broadcasts to the server after `EvaluationFinalizeOutcome::Completed`. Added timeout + `kill_on_drop(true)` around `nix-store --query --outputs`. Added ignored DB regression tests for cancellation-vs-finalization races, rollback on failed success/synthetic writes, and no build jobs before finalization.

Verification run from the task worktree:
- `nix develop -c cargo check -p cf-server --all-targets` (exit 0; existing warnings only)
- `nix develop -c cargo sqlx prepare --check -- --all-targets` from `packages/default/crates/cf-server` (exit 0; existing warnings only)
- `nix develop -c cargo test -p cf-server models::evaluate_with_policies::tests --lib` (exit 0; 7 passed, 5 ignored)
- `nix develop -c cargo test -p cf-server models::evaluate_with_policies::tests::finalize_attempt_ --lib -- --ignored --test-threads=1` (exit 0; 5 passed)
- `nix build .#packages.x86_64-linux.server --no-link` (first 120s run timed out/interrupted by shell timeout; rerun with 600s timeout exited 0)
- `git diff --check` (exit 0)

Post-review pass for MR !309: routed evaluator/finalizer errors through the attempt-aware failure CAS, moved build-job insertion into the finalization transaction with queued-build IDs returned for broadcasts, added rollback/orchestration DB regressions, restored bounded closure-count concurrency, and added the ignored finalize-attempt DB tests to the state-machine CI script. Verification after trimming unrelated rustfmt-only changes: `nix develop -c cargo check -p cf-server --all-targets` (exit 0), `nix develop -c cargo sqlx prepare --check -- --all-targets` from `packages/default/crates/cf-server` (exit 0), `nix develop -c cargo test -p cf-server models::evaluate_with_policies::tests --lib` (exit 0; 7 passed/7 ignored), `nix develop -c cargo test -p cf-server models::evaluate_with_policies::tests::finalize_attempt_ --lib -- --ignored --test-threads=1` (exit 0; 7 passed), `nix build .#packages.x86_64-linux.server --no-link` (exit 0), and `git diff --check` (exit 0). Existing warnings only.

CI state-machine-tests on commit `26f59f43` failed before running tests because the new dev script line did not escape `$DB_URL` inside the generated `bash -c` string; ShellCheck SC2027 failed the Nix build of `run-state-machine-tests`. Fixed the escaping in `packages/devScripts/default.nix`, verified `nix build .#devScripts.state-machine-test --no-link` (exit 0) and `git diff --check` (exit 0), committed `e042d40b fix: escape state machine test database url`, and pushed it to MR !309. New exact-head CI for `e042d40b` is expected to start; not waiting per user instruction.

Continued reviewer-directed MR !309 architecture correction in worktree `TASK-397-eval-errors-silently-drop`: adjusted `finalize_evaluated_system` to the requested public signature, kept per-system transactional derivation write/build-job insert, added CF-agent SQL backstop to single-build insertion, added idempotent retry DB regression coverage, and made migration 0184 deduplicate existing build_jobs per derivation before creating the unique index needed for `ON CONFLICT (derivation_id) DO NOTHING`.

Verification from this pass:
- `nix develop -c cargo check -p cf-server --all-targets` exited 0 (existing warnings only).
- `nix develop -c cargo test -p cf-server models::evaluate_with_policies::tests --lib` exited 0 (7 passed, 9 ignored DB tests).
- `nix develop -c cargo sqlx prepare --check -- --all-targets` from `packages/default/crates/cf-server` exited 0 (existing warnings only).
- `git diff --check` exited 0.

Blocked/partial verification:
- `nix develop -c cargo sqlx migrate run --source migrations` against the existing local dev DB still fails before the new migration at migration 182 because `cves.fleet_relevant_since` already exists; did not reset or destructively repair that DB.
- Attempted a scratch DB for ignored DB tests, but local DB role lacks CREATE DATABASE permission, so ignored `finalize_system_` tests remain unrun in this pass. Earlier failure mode was the expected missing unique constraint before migration 0184 is applied.
<!-- SECTION:NOTES:END -->
