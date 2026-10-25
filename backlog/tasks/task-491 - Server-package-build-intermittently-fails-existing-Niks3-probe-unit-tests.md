---
id: TASK-491
title: Server package build intermittently fails existing Niks3 probe unit tests
status: Review
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 17:21'
updated_date: '2026-10-24 23:10'
labels:
  - testing
  - build-blocker
dependencies: []
references:
  - TASK-490.1
  - packages/default/default.nix
  - packages/default/crates/cf-server/src/handlers/api/builders.rs
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/336'
priority: high
type: bug
ordinal: 537000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The production `cf-server-drv` check phase intermittently failed two otherwise valid Niks3 probe tests. The cause was test-process interference: the Linux raw-fork regression holds an inherited executable writer while it waits at a barrier; unrelated tests running on other harness threads can have executable fixture writers open at fork time. The child inherits those descriptors even when they carry FD_CLOEXEC. A concurrent exec then fails with ETXTBSY, which the production probe maps to Process or Configuration rather than the scripted provider result.

The bounded test-only repair runs the raw-fork regression in an exact-filtered child invocation of the current unit-test binary. It preserves the inherited-writer proof, ETXTBSY assertion, and child cleanup/reaping. No production behavior or package configuration changes.

Controlled reproduction, exact verification commands, results, and log paths are recorded in implementation notes. The normal production server package build passed. TASK-491 is in Review at MR !336. This resolves only the server package prerequisite; no Access VM or integration-owner pass is implied.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Root cause of the two observed package-check failures is established with reproducible evidence
- [x] #2 Normal production cf-server-drv builds without skips, weakened assertions, or disabled checks
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Confirm the existing failure mechanism by running a controlled raw-fork proof: a FD_CLOEXEC writer is held only in a fork child while a concurrent exec attempts that executable; verify ETXTBSY and reliable child release/reap. Relate to the two tests: probe_cache_read maps feature-probe launch errors to Configuration; wait_cache_verification_command maps spawn failures to Process rather than provider RateLimited.
2. Keep evaluator_fixture_reproduces_inherited_cloexec_writer's raw fork and ETXTBSY assertion, but run the fork regression in a child invocation of the current unit-test binary with --exact and an environment recursion guard. The child test process then has no other test threads whose writable fixture descriptors it can inherit. Parent .status() must wait/reap; existing InheritedWriter Drop continues cleanup on assertion failure.
3. Run focused Niks3 tests and the fork regression in normal harness parallelism; run full `nix build .#packages.x86_64-linux.server` (resolve the actual package alias from flake) with ordinary Rust checks enabled.
4. Review that only #[cfg(test)] builders.rs and TASK-491 records changed, record exact evidence, then commit.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: claude-opus-5-5 on sledge in /home/mcamp/code/crystal-forge/TASK-491-isolate-fork-fixture

Preflight 2026-10-17: dedicated branch/worktree TASK-491-isolate-fork-fixture created clean from origin/dev 33de73b013b735304dfac9c69745a074a3e76ebd. TASK-491 had no lock and no dependencies. The dev worktree is not clean (pre-existing TASK-490.13 staged edit, .locks/TASK-478.lock, .locks/TASK-490.lock, .locks/TASK-490.1.lock, session-ses_f927.md); preserved. No main branch/worktree found. No changes will be made there. Scope is only relevant cf-server #[cfg(test)] fixture/isolation code and TASK-491 records. Plan: inspect specified helpers/tests and package test phase; establish FD-inheritance interference via direct controlled evidence; apply minimal isolated repair preserving raw-fork proof and cleanup/reaping; run focused tests in normal parallel execution plus Nix package cf-server-drv normal check. Record failures exactly; commit the fix and return tested prerequisite without touching TASK-490.1.

Investigation evidence (2026-10-17): inspected specified source at baseline. Existing `evaluator_fixture_reproduces_inherited_cloexec_writer` (builders.rs:7718) raw-forks a child that intentionally waits while retaining the inherited `nix` fixture writer through a barrier; parent then closes its copy and continues. Other unit tests use `std::fs::write` to create executable fixtures (including both affected tests at current lines 7015 and 7243). In the same test process, raw fork inherits descriptors opened by concurrent test threads; FD_CLOEXEC does not close them before the child execs.

Controlled reproduction outside the repository: compiled and ran `/tmp/opencode/task491/etxtbsy-repro.c` via `nix develop -c bash -c 'cc ... && ...'`. The program creates an executable with FD_CLOEXEC, forks a child that retains its writer while blocked on a pipe, closes the parent's descriptor, then attempts a concurrent exec; it confirmed ETXTBSY and released/reaped the child before cleanup.

Error mapping confirmed: `probe_nix_read_features` launch failure becomes a static anyhow error and `probe_cache_read` maps it to CacheProbeFailure::Configuration; `wait_cache_verification_command` maps process spawn errors to Process, explaining why transient ETXTBSY can masquerade as both observed results, including instead of intended provider RateLimited. No production functions will be changed.

Pre-fix targeted run: `SQLX_OFFLINE=true nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib niks3_'` completed 87 passed / 0 failed / 31 ignored in normal Rust harness parallelism. It did not include the `evaluator_fixture...` test by name and was not expected to deterministically reproduce the race.

Confirmed cause: evaluator_fixture_reproduces_inherited_cloexec_writer deliberately raw-forks a child that retains an inherited writable executable descriptor while it waits on the barrier. FD_CLOEXEC does not close the descriptor at fork. Other parallel test threads create executable fixtures; the raw-fork child can inherit their writer, so closing the parent descriptor leaves ETXTBSY until the barrier releases. `nix develop -c bash -c 'cc /tmp/opencode/task491/etxtbsy-repro.c -o /tmp/opencode/task491/etxtbsy-repro && /tmp/opencode/task491/etxtbsy-repro'` confirmed this controlled case with exit 0 and ETXTBSY observed. `probe_nix_read_features` maps spawn failure to Configuration; `wait_cache_verification_command` maps spawn failure to Process, which accounts for both original mismatches.

Repair is test-only: `packages/default/crates/cf-server/src/handlers/api/builders.rs` wraps the raw-fork regression in a fresh invocation of the current unit-test binary with `--exact` and an environment recursion guard. Parent `Command::status()` waits/reaps; the existing `InheritedWriter` Drop still releases/reaps the forked child on failures. The raw-fork inherited-writer proof and ETXTBSY assertion are unchanged.

Verification on commit af39284bdf16d56402f01a64c916f17948e43b02:
- `nix develop -c rustfmt --edition 2024 --check packages/default/crates/cf-server/src/handlers/api/builders.rs` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib handlers::api::builders::tests:: > /tmp/opencode/task491-builders-after.log 2>&1; printf "exit=%s\\n" "$?"'` exit 0: 74 passed, 0 failed, 1 ignored under normal Rust test parallelism; nested exact regression 1 passed.
- `nix build .#packages.x86_64-linux.server --no-link -L --show-trace` exit 0 with normal package checks. The existing flake output maps to `pkgs.crystal-forge.default.server` (aggregate); its dependency `/nix/store/9m1dmm43m9r0afy3ci0ljqphlyibzgyi-cf-server-0.3.0.drv` is the production `cf-server-drv` and ran the ordinary test phase: 1794 passed, 0 failed, 682 ignored; all bin test targets passed.
- `git diff --check` passed.

One non-gating diagnostic attempt, `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib > /tmp/opencode/task491-full-lib-after.log 2>&1; printf "exit=%s\\n" "$?"'`, failed eight unrelated `queries::cve_scans_tests` with PoolTimedOut because this shell had no disposable DATABASE_URL. The normal Nix package build above passed in its package environment. MR !336: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/336. This fix resolves only the package prerequisite; no Access VM or integration owner result is implied.

MR !336 is open from TASK-491-isolate-fork-fixture to dev. Commit af39284bdf16d56402f01a64c916f17948e43b02 contains the test-only isolation fix. Task moved to Review after both acceptance criteria passed. Removing the TASK-491 lock for review; no merge or deployment is authorized or performed.

Command-record correction: the pre-fix targeted command actually executed was `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib niks3_ > /tmp/opencode/task491-targeted-before.log 2>&1; printf "exit=%s\\n" "$?"'` (exit 0, 87 passed, 31 ignored). An earlier appended summary placed `SQLX_OFFLINE=true` before `nix develop`; that spelling was not the literal executed command. MR !336 is open at https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/336; GitLab reported its head pipeline pending at handoff. TASK-491 lock removed for Review. No merge or deploy.

Merge reconciliation clarification: LOCK retired for Review. Earlier LOCK text records historical implementation ownership, not an active lock. Preserve the qualified package and focused test results and the separately recorded non-gating database-less diagnostic failure; this record does not establish integration-owner success or merged completion.
<!-- SECTION:NOTES:END -->
