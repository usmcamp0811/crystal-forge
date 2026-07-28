---
id: TASK-397
title: Evaluation errors silently drop systems from nixosConfigurations count
status: In Progress
assignee:
  - opencode
created_date: '2026-07-24 00:29'
updated_date: '2026-07-28 20:10'
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
  - modules/nixos/crystal-forge/default.nix
  - checks/integration/default.nix
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
- [ ] #5 Evaluator subprocess groups are terminated when bulk or standalone evaluation futures time out are cancelled or are dropped
- [ ] #6 The server service has default hard memory and bounded swap containment so evaluator descendants cannot trigger a host-wide OOM
- [ ] #7 Evaluator diagnostics are drained without retaining unbounded stderr in server memory
- [ ] #8 The dotfiles nixos branch configures reckless with one evaluator worker a 24G high watermark a 32G hard server limit a 1G server swap limit and no local Crystal Forge builder
- [ ] #9 The vendored downstream module exposes configurable server MemoryHigh MemoryMax MemorySwapMax and TasksMax and places Crystal Forge services beneath a bounded aggregate parent slice
- [ ] #10 The downstream server uses control-group cleanup OOMPolicy stop on-failure restart and rate limiting
- [ ] #11 Nix evaluator guards reject invalid PGIDs and remain armed until bounded pipe readers finish
- [ ] #12 Evaluator stdout stderr WebSocket and persisted records are bounded before allocating an oversized line or JSON record
- [ ] #13 Active Nix subprocesses and service cgroup memory composition are observable with process kind ownership timing and concurrency fields
- [ ] #14 A NixOS VM test proves Crystal Forge cgroup OOM containment leaves an outside sentinel and the VM responsive
- [ ] #15 Remote derivation materialization preserves a complete actionable error chain and changes transport mode for one eligible retry without falling back on authorization failures
- [ ] #16 A real campground evaluation under the deployed reckless limits causes no global OOM no unrelated process kill no unbounded swap and no orphan evaluator descendants
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
P0 OOM and remote-materialization follow-up requested for MR !309 and dotfiles:nixos.

1. Downstream first: update `/config/modules/nixos/services/crystal-forge/default.nix` with configurable server MemoryHigh/MemoryMax/MemorySwapMax/TasksMax, aggregate `crystal-forge.slice`, server slice assignment, OOM-safe restart policy, conservative evaluator defaults, and correct threshold documentation. Update `/config/systems/x86_64-linux/reckless/default.nix` to one evaluator worker, 24G/32G/1G limits, and disable the local builder. Preserve the existing committed containment block while replacing it with configurable settings.
2. Verify downstream evaluation from the dotfiles `nixos` branch without deploying or using sudo. Record exact intended rendered properties.
3. Collect previous-boot and current OOM evidence on reckless using unprivileged journal/cgroup access; do not assume cgroup peak equals Rust heap.
4. Upstream guard correction: centralize nonzero PID/PGID validation, keep cleanup armed until bounded pipe drain finishes, add bounded record readers that cap before allocation, and add regression tests for leader-exits/descendant-holds-pipe behavior.
5. Add a shared Nix subprocess registry and cgroup telemetry, conservative shared concurrency, overlap prevention, and memory-aware evaluator cancellation. Add a NixOS VM containment test with a sentinel outside the Crystal Forge slice.
6. Diagnose the daly/webb delta materialization failure and preserve the full error chain. Authorization failures must not fall back; retryable transport failures switch once from delta to full closure. Add chunk/cross-reference/truncation/import tests before changing transport architecture.
7. Run targeted Rust/SQLx/Nix checks, commit and push both requested branches, then perform only unprivileged runtime verification. Do not merge MR !309. Do not claim real-host acceptance until a campground evaluation has run with the deployed downstream limits.

Scope note: TASK-390 currently records a lock in a separate worktree for remote-builder transport. Complete P0 downstream/OOM work first; before editing overlapping transport files, resolve that active-task/worktree ownership conflict rather than duplicating changes.
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

Commit 5613786b pushed to MR !309.

Changes:
- Migration 0184 rewritten to use status-precedence deduplication (building > queued > cancelling > cancelled > failed > success) rather than oldest-first heuristic. Applied to dev DB: deleted 310 duplicate build_jobs rows, created unique index.
- 9 new ignored DB regression tests covering: agent-disabled no-queue, multiple strict failures no-queue, cancellation after first system queued (first job survives), broken+healthy isolation (healthy queued before commit finalize, broken recorded, commit completes), retry idempotency via ON CONFLICT, and migration_0184 unique-index idempotency.
- 3 pure-Rust unit tests for migration 0184 status ordering (run without DB).
- State-machine CI script extended to run finalize_system_ and migration_0184_ ignored test groups.

Local verification (commit 5613786b):
- cargo check -p cf-server --all-targets: exit 0
- cargo test …::tests --lib: 10 passed, 14 ignored
- cargo test …::tests --lib -- --ignored --test-threads=1: 14 passed
- cargo sqlx prepare --check -- --all-targets: exit 0
- nix build .#packages.x86_64-linux.server --no-link: exit 0
- nix build .#devScripts.state-machine-test --no-link: exit 0
- git diff --check: exit 0

Not yet verified locally (requires clean CI DB):
- Full state-machine-test process-compose run (port conflict with dev DB on port 3042).
- queries::commits::tests::stale_cancellation_finalizer_does_not_affect_newer_attempt: pre-existing dev DB pollution (commit 697 stuck in_progress); unrelated to these changes; passes on clean CI DB.

Commit fc7bdfba pushed to MR !309.

## Three substantive fixes

### 1. seen_systems bug — the actual nix-builder-1 root cause

A system that emits a JSON error line from nix-eval-jobs was inserted into `seen_systems` unconditionally (line 1638-1639 pre-fix). This made it appear 'accounted for', preventing standalone fallback evaluation. The system silently disappeared from all accounting.

Fix: only insert into `seen_systems` when `has_error == false && drv_path.is_some()`. Systems with eval errors now fall through to policy-aware standalone fallback, which will classify them as either `ConfirmedSystemFailure` (if standalone also fails) or recover them as successful systems with derivation+build-job.

New pure-Rust test: `error_result_is_not_seen_so_falls_through_to_fallback`

### 2. Migration 0184 terminal-status ordering corrected

Previous: `building(1) > queued(2) > cancelling(3) > cancelled(4) > failed(5) > success(6)`. A stale failed row was kept over a successful one when both existed as duplicates.

Fixed: `success(4)` now ranks above `cancelled(5)` and `failed(6)`, so the row with a valid output path is retained.

New test: `migration_0184_keeps_success_over_failed` + updated `migration_0184_status_precedence_order`.

### 3. enqueue_build_job_for_derivation race window eliminated

Replaced racy `NOT EXISTS` subquery with `ON CONFLICT (derivation_id) DO NOTHING`. Concurrent callers can no longer produce a unique constraint violation. Also added `cf_agent_enabled = TRUE` guard (matching `create_build_job_for_derivation_tx`). SQLx metadata regenerated.

### New DB regression tests (both pass)
- `finalize_system_build_already_exists_does_not_re_queue`: second call returns non-Queued outcome, exactly one build_jobs row
- `finalize_system_error_result_does_not_block_standalone_finalization`: simulates the standalone fallback path for a previously-errored system

Local verification (commit fc7bdfba):
- cargo check --all-targets: exit 0
- cargo test …::tests --lib: 12 passed, 16 ignored
- cargo test …::tests --lib -- --ignored --test-threads=1: 16 passed
- cargo sqlx prepare --check -- --all-targets: exit 0
- nix build .#packages.x86_64-linux.server --no-link: exit 0
- nix build .#devScripts.state-machine-test --no-link: exit 0
- git diff --check: exit 0

Remaining for full task completion:
- CI exact-head pipeline on fc7bdfba
- Live reproduction with c0782ce6 (nix-builder-1 recovery confirmed via standalone fallback)
- Workflow-level test: system A claimable build job while B still evaluating (requires mock evaluator hook or test harness extension)

Continuing investigation in worktree TASK-397-eval-errors-silently-drop (HEAD f4865c44). Root cause identified: commit f4865c44 scoped policies per configuration but lost the unconditional cfAgentEnabled emission introduced in 8a9d0b78. Configurations with zero assigned policies now produce an empty policies attrset, so PolicyCheckResult::from_assigned leaves cf_agent_enabled = None. The build-job insert predicate `derivations.cf_agent_enabled = TRUE` then rejects those derivations, producing a successful eval but empty build queue. Fix: always emit cfAgentEnabled in both bulk and standalone Nix expressions, always read it in from_assigned, and update tests/log messaging accordingly.

Fix implemented and verified. Changes:

1. `models/deployment_policies.rs`:
   - `build_nix_eval_expression` now emits `cfAgentEnabled` unconditionally for every nixosConfiguration via a `cfAgentEnabledExpr` helper merged into each configuration's `policies` attrset.
   - `PolicyCheckResult::from_assigned` now parses `cfAgentEnabled` unconditionally and returns an infrastructure error if the key is missing, preventing silent `cf_agent_enabled = None` that blocks build-job insertion.
   - Updated `test_build_expression_no_policies` to assert `cfAgentEnabled` and `cfAgentEnabledExpr` are present.

2. `models/evaluate_with_policies.rs`:
   - `build_single_system_eval_expression` now emits `cfAgentEnabled` unconditionally in `policyResults`.
   - Standalone fallback path no longer short-circuits to a `cf_agent_enabled: None` result when no policies are assigned; it always parses the unconditional metadata.
   - Streaming fallback for configurations with no assigned policies now reads `cfAgentEnabled` from `meta.policies` and only synthesizes a passing result when the metadata is present.
   - Updated log/broadcast messages to say "configurations with assigned policies" instead of "registered configurations" to avoid implying zero systems are registered.
   - Updated unit tests (`different_environments_use_different_policy_sets`, `no_policy_configuration_passes_evaluation`) to include unconditional `cfAgentEnabled` JSON and assert missing-key behavior.

Verification (worktree TASK-397-eval-errors-silently-drop):
- `env SQLX_OFFLINE=true cargo check -p cf-server --all-targets` exit 0
- `env SQLX_OFFLINE=true cargo test -p cf-server --lib` exit 0; 651 passed, 195 ignored
- `cargo test -p cf-server models::evaluate_with_policies::tests::finalize_system_ --lib -- --ignored --test-threads=1` against dev DB: 11 passed
- `cargo sqlx prepare --check -- --all-targets` against dev DB: exit 0
- `nix build .#packages.x86_64-linux.server --no-link` exit 0
- `nix build .#devScripts.state-machine-test --no-link` exit 0
- `git diff --check` exit 0

Remaining: commit, push to MR !309, and run CI exact-head pipeline.

Second fix pushed: unified policy expression `cfg` scope.

## Problem
The per-configuration bulk checker from `dab3b5b7` / `f4865c44` bound its checker argument as `config` and passed only `cfg.config`, while built-in policy fragments (`RequirePackages`, `RequireCrystalForgeAgent`) are generated against the full `cfg` object. Production evaluation failed immediately with `undefined variable 'cfg'` for any configuration with an assigned package or agent policy (e.g. the `gray` configuration in the `campground` flake).

## Fix
- `build_policy_fields_for_config_indented` now routes every built-in Nix-evaluated policy through `to_nix_expression_with_index`, so all fragments use the documented `cfg.config.*` scope.
- Bulk checker binds `cfg` and is invoked with the full `nixosConfigurations.<name>` object.
- Unconditional `cfAgentEnabled` helper uses `cfg:` and `cfg.config.*` consistently.
- `PolicyCheckResult::from_assigned` rejects non-boolean values for `cfAgentEnabled` and every assigned boolean policy field, treating them as infrastructure/parser mismatches.
- Added regression tests for bulk/standalone `cfg` scope and synthetic `nix eval` execution.

## Verification
- `cargo fmt --check` exit 0
- `env SQLX_OFFLINE=true cargo check -p cf-server --all-targets` exit 0
- `env SQLX_OFFLINE=true cargo test -p cf-server --lib` exit 0; 658 passed, 196 ignored
- `cargo test -p cf-server models::deployment_policies::tests::generated_policy_fields_evaluate_without_undefined_variables --lib -- --ignored` (Nix evaluator): 1 passed
- `cargo test -p cf-server models::evaluate_with_policies::tests::finalize_system_ --lib -- --ignored --test-threads=1` (dev DB): 11 passed
- `cargo sqlx prepare --check -- --all-targets` (dev DB): exit 0
- `nix build .#packages.x86_64-linux.server --no-link` exit 0
- `nix build .#devScripts.state-machine-test --no-link` exit 0
- `git diff --check` exit 0

## Commit / MR
- Commit: `40ab94f2`
- Branch: `TASK-397-eval-errors-silently-drop`
- MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/309

Remaining: runtime acceptance with the `campground` commit `3ea42835959d49fab431f07470dfc93fb7f7a52d` to confirm no `undefined variable 'cfg'` and that `gray` evaluates/queues correctly.

2026-07-28 OOM follow-up investigation on reckless was read-only and used no sudo. Current unit settings report MemoryHigh/MemoryMax/MemorySwapMax=infinity. A recent real evaluation instance reached a 28.7G service-cgroup peak in 1m53s; previous supplied incident evidence recorded 115.1G memory and 14.9G swap. Kernel OOM entries were not readable from the current user journal. Code review found standalone fallback children had process groups but no RAII drop guard, bulk stderr was retained without a bound and duplicated on failure, standalone stdout/stderr used unbounded read_to_end, NixOS eval_workers default remained 4, and server cgroup limits remained opt-in.

Implemented the OOM-containment follow-up without changing the running reckless service: standalone and fallback evaluators now use NixEvalProcessGuard, evaluator stderr retention is capped at 256 KiB while pipes continue draining, standalone stdout is capped at 8 MiB, and truncated diagnostics are labeled. NixOS defaults now use two eval workers plus MemoryHigh=60%, MemoryMax=75%, and MemorySwapMax=2G; all remain overridable. Integration coverage asserts that the service starts with finite memory/swap limits.

Verification:
- `nix develop -c cargo fmt --check`: passed.
- `nix develop -c env SQLX_OFFLINE=true cargo check -p cf-server --all-targets`: passed with existing warnings.
- Targeted capped-output tests: 2 passed.
- Targeted process-guard tests: 3 passed.
- `nix develop -c env SQLX_OFFLINE=true cargo test -p cf-server models::evaluate_with_policies::tests --lib`: 26 passed, 20 ignored.
- `nix build .#packages.x86_64-linux.server --no-link`: passed.
- `nix build .#checks.x86_64-linux.integration --no-link`: passed, including finite MemoryHigh/MemoryMax and 2 GiB MemorySwapMax assertions.
- `nix flake check --keep-going`: all checks passed for x86_64-linux.
- `git diff --check`: passed.

A full `cargo test -p cf-server --lib` run was not clean: 684 passed, 200 ignored, and 3 unrelated runtime-database CVE tests failed with `PoolTimedOut` because the available DATABASE_URL did not accept connections. Runtime acceptance remains pending: reckless currently still reports infinity for MemoryHigh/MemoryMax/MemorySwapMax because this branch has not been deployed or the service restarted. Its last observed service instance peak remains 30,853,607,424 bytes.

User explicitly prohibited merging MR !309 until both OOM containment and remote materialization runtime gates pass. Immediate downstream work targets `usmcamp0811/dotfiles` branch `nixos`; upstream remains `TASK-397-eval-errors-silently-drop`. No sudo or destructive deployment commands will be run by the agent.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Continued MR !309 fixes in worktree `TASK-397-eval-errors-silently-drop`.

## First fix (commit dab3b5b7)
Commit `f4865c44` scoped deployment policies per NixOS configuration but lost the unconditional `cfAgentEnabled` emission from `8a9d0b78`. Systems without an assigned `require_cf_agent` policy produced an empty `policies` attrset, leaving `cf_agent_enabled = None`. The build-job insert predicate `derivations.cf_agent_enabled = TRUE` then rejected those derivations, producing successful evaluations with an empty build queue.

Fix: emit `cfAgentEnabled` unconditionally in bulk and standalone Nix expressions, parse it unconditionally, stop defaulting to `None` in fallback paths, and clarify log wording.

## Second fix (commit 40ab94f2)
The per-configuration bulk checker bound its argument as `config` and passed only `cfg.config`, while built-in policy fragments (`RequirePackages`, `RequireCrystalForgeAgent`) are generated against the full `cfg` object. Production evaluation failed with `undefined variable 'cfg'` for any configuration with an assigned package or agent policy.

Fix: unify all Nix-evaluated policy fragments on the `cfg` lexical contract, where the checker receives the full `nixosConfigurations.<name>` object as `cfg` and accesses options via `cfg.config.*`.

## Verification
- `cargo fmt --check` exit 0
- `env SQLX_OFFLINE=true cargo check -p cf-server --all-targets` exit 0
- `env SQLX_OFFLINE=true cargo test -p cf-server --lib` exit 0; 658 passed, 196 ignored
- `cargo test -p cf-server models::deployment_policies::tests::generated_policy_fields_evaluate_without_undefined_variables --lib -- --ignored` (Nix evaluator): 1 passed
- `cargo test -p cf-server models::evaluate_with_policies::tests::finalize_system_ --lib -- --ignored --test-threads=1` (dev DB): 11 passed
- `cargo sqlx prepare --check -- --all-targets` (dev DB): exit 0
- `nix build .#packages.x86_64-linux.server --no-link` exit 0
- `nix build .#devScripts.state-machine-test --no-link` exit 0
- `git diff --check` exit 0

## Commit / MR
- Branch: `TASK-397-eval-errors-silently-drop`
- Commits: `dab3b5b7`, `40ab94f2`
- MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/309

Remaining: runtime acceptance with the `campground` commit `3ea42835959d49fab431f07470dfc93fb7f7a52d` to confirm no `undefined variable 'cfg'` and that `gray` evaluates/queues correctly.
<!-- SECTION:FINAL_SUMMARY:END -->
