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
