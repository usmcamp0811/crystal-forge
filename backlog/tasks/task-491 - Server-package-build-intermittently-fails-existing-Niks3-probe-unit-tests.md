---
id: TASK-491
title: Isolate raw-fork executable-writer regression from parallel Rust tests
status: Review
assignee:
  - claude-opus-5-5
created_date: '2026-10-17 17:21'
labels:
  - testing
  - build-blocker
dependencies: []
references:
  - TASK-490.1
  - packages/default/default.nix
  - packages/default/crates/cf-server/src/handlers/api/builders.rs
  - https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/336
priority: high
type: bug
ordinal: 537000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The failing probe assertions were caused by parallel-test descriptor inheritance in `evaluator_fixture_reproduces_inherited_cloexec_writer`: its forked child retains another test thread's FD_CLOEXEC executable writer across the barrier, making exec fail with ETXTBSY. The repair isolates the raw-fork regression in an exact-filtered subprocess and preserves the proof/assertion and cleanup. Only cfg(test) code changes.

Controlled reproduction and full verification are recorded in the task's implementation notes. Commit af39284bdf16d56402f01a64c916f17948e43b02 is in MR !336. The normal package server build passed. This resolves the package prerequisite only; the Access VM still fails on TASK-490.25/.26 and no owner passes are claimed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Root cause of the two observed package-check failures is established with reproducible evidence
- [x] #2 Normal production cf-server-drv builds without skips, weakened assertions, or disabled checks
<!-- AC:END -->

## Implementation Notes

LOCK retired for Review. TASK-491 commit af39284bdf16d56402f01a64c916f17948e43b02; MR !336 open. Normal package build passed: `nix build .#packages.x86_64-linux.server --no-link -L --show-trace`, cf-server library tests 1794 passed / 0 failed / 682 ignored. Focused parallel builders tests: 74 passed / 0 failed / 1 ignored; nested exact raw-fork regression passed. The integration Access scenario was then executed in the dependent TASK-490.1 branch and failed before migrations on separately tracked product issues TASK-490.25/.26. No integration-owner pass is implied.
