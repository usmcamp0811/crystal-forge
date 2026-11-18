---
id: TASK-530
title: Investigate Niks3 child startup failures on the build farm
status: Backlog
assignee: []
created_date: '2026-11-17'
labels: [server, ci, tests, niks3]
dependencies: []
priority: medium
---

## Description
<!-- SECTION:DESCRIPTION:BEGIN -->
The farm-only core build for TASK-529.2's rebased immutable-grant checkpoint
failed before the later OIDC source changes. Besides the parser failures already
tracked by TASK-526, it reported failures in:

- `builder::cve_worker::tests::niks3_basic_materialization_requires_guard_and_child_settings`
- `handlers::api::builders::niks3_input_owner_tests::niks3_input_owner_acknowledges_reap_and_cleanup_after_detach`

Derivation: `d38m6c5nagl5yzrxb24skvbqd148l7fn-cf-server-core-0.3.0.drv`.
Library result: 1554 passed, 18 failed, 853 ignored. Inspect the full Nix log to
establish each failure's cause; this capture does not assert that both failures
share one cause. TASK-425 previously recorded an intermittent child-readiness
failure in the second test, and TASK-470 owns the process lifecycle contract.

Desired outcome: reproduce the failures on the farm, distinguish fixture timing
or isolation defects from production regressions, and verify the relevant
process guard/cleanup assertions without relaxing production authority checks.
No local compilation, production credential access, or unrelated PAT/MCP changes
are authorized by this capture.
<!-- SECTION:DESCRIPTION:END -->

## Additional observation (2026-11-17, TASK-529.2)

`handlers::api::builders::tests::niks3_probe_failure_never_polls_success_transition` (builders.rs, around line 7283) failed on the farm for derivation `i125vc8vzll56iii1wgqvx1y6rhq89yx-cf-server-0.3.0.drv` in two consecutive attempts, then passed on a third attempt with no source change. The expected failure kind differed each run (`MissingPath`, `Network`, `AuthenticationFailed`) while the observed kind was always `Process`. The test writes a fake `nix` shell script and executes it immediately inside a multithreaded test run; the observed `Process` failure is consistent with the exec failing, but no log establishes that cause. Investigate this alongside the process-lifecycle tests above without relaxing the assertions.
