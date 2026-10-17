---
id: TASK-491
title: Server package build intermittently fails existing Niks3 probe unit tests
status: In Progress
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 17:21'
updated_date: '2026-10-17 18:25'
labels:
  - testing
  - build-blocker
dependencies: []
references:
  - TASK-490.1
  - packages/default/default.nix
  - packages/default/crates/cf-server/src/handlers/api/builders.rs
priority: high
type: bug
ordinal: 537000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem: production cf-server-drv at application baseline 33de73b013b735304dfac9c69745a074a3e76ebd failed its normal Crane check phase in two builds, preventing TASK-490.1's real VM slice from starting. No overhaul code changes production sources or package selection. Desired outcome: determine why the existing process-probe tests fail in the normal package check environment and make the production package build reliable without disabling assertions.

Observed command (before organization correction): nix build .#integration.access.scenarios.lifecycle-local-bootstrap -L --out-link /tmp/opencode/task490/slice-result. The corrected equivalent is nix build .#checks.x86_64-linux.integration-access.scenarios.lifecycle-local-bootstrap -L. Both depend on unchanged /nix/store/9pihfg4v8mp6rb4qnp148acv5cqsrb43-cf-server-0.3.0.drv, output /nix/store/7chrsgk02f37jymvb4q4dxcxkh40xy07-cf-server-0.3.0.

Attempt 1: handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition failed at crates/cf-server/src/handlers/api/builders.rs:7275: expected RateLimited, actual Process. 1793 passed, 1 failed, 682 ignored. Package builder exit 101; Nix command exit 1.
Attempt 2: the prior test passed, but handlers::api::builders::tests::niks3_basic_probe_applies_guard_to_path_info_only failed at builders.rs:7076: Result::unwrap on Err(Configuration). Same totals and exit statuses.

Evidence on sledge: /tmp/opencode/task490/slice-run1.log and slice-run2.log. Retain nix log /nix/store/9pihfg4v8mp6rb4qnp148acv5cqsrb43-cf-server-0.3.0.drv for the last attempt. Root cause (environment, concurrency, or product code) is not established. No VM ran and no auth defect was reproduced. Do not weaken or bypass the package check phase. Test-only overhaul does not authorize fixing cf-server production or unit-test sources here. Linked blocked work: TASK-490.1.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Root cause of the two observed package-check failures is established with reproducible evidence
- [ ] #2 Normal production cf-server-drv builds without skips, weakened assertions, or disabled checks
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
<!-- SECTION:NOTES:END -->
