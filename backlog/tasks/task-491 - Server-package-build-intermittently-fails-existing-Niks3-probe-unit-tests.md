---
id: TASK-491
title: Server package build intermittently fails existing Niks3 probe unit tests
status: Backlog
assignee: []
created_date: '2026-10-17 17:21'
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
