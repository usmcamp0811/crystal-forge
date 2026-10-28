---
id: TASK-487
title: Make the complexity gate strict and remediate Rust complexity debt
status: Backlog
assignee: []
created_date: '2026-10-15 20:28'
labels:
  - ci
  - tech-debt
  - code-metrics
dependencies: []
references:
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/331'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2952372861'
  - packages/code-metrics/default.nix
  - >-
    docs/knowledge/operations/build-invalidation-graph-and-ci-latency-analysis.md
priority: medium
type: chore
ordinal: 509000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The `complexity-check` job passes on `dev` without verifying complexity. Its script (`packages/code-metrics/default.nix`) runs `cargo clippy ... || true` and then greps the output for lint names. If Clippy aborts (for example on a deny-by-default lint, or a missing pkg-config/OpenSSL for a build script), the output has no lint lines and the job passes. The shell exit also ignored the 500-line file clause, which only wrote JUnit failures. `docs/knowledge/operations/build-invalidation-graph-and-ci-latency-analysis.md` already records that the script "still appends `|| true`".

A strict implementation exists in commit 9f0cb90cea4c7c2c5929fcbccd1ed058fa5fa255 (branch `TASK-487-strict-complexity-remediation`, `packages/code-metrics/{default.nix,test-report.py}`). It sets pkg-config/OpenSSL and `SQLX_OFFLINE`, limits cargo jobs, writes a structured per-package Clippy status, fails on a failed Clippy run, and reflects the 500-line clause in the shell exit. It was removed from MR !331 (TASK-470) on the user's decision so that !331 ships with the same gate behavior as `dev`.

Measured with that strict gate on the TASK-470 tree (536 Rust files, native packages only; WASM lints and coverage not assessed):
- 733 lint findings: 429 too_many_lines, 199 cognitive_complexity, 71 too_many_arguments, 32 type_complexity, 2 fn_params_excessive_bools, 0 vec_box.
- 195 files over 500 lines: 462,585 of 520,540 lines (about 89%). About 129 files are in cf-server and 51 in web-ui.
- Inline `#[cfg(test)]` modules are only 128,071 of those lines. Extracting them would bring only 18 files under 500 lines. 177 files would still hold about 328,000 lines of production code, which needs responsibility-based splitting (more than 900 new modules at about 450 lines each).
- Largest files: cf-server tests/poam_workflows.rs (20,875), handlers/api/compliance.rs (16,564), queries/evaluation_snapshots.rs (16,125), web-ui views/system_detail.rs (13,519), models/evaluate_with_policies.rs (12,559), services/poam.rs (11,743).

The job should not claim a verified result until the strict gate is restored. Decide the final policy first: full remediation, or a committed per-file baseline that fails only on new or worsened findings. A baseline is a gate policy change and needs explicit maintainer approval.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Maintainer decides the gate policy (full remediation or an approved baseline ratchet) and records it on this task
- [ ] #2 The strict launcher fails the job when Clippy fails, when any selected lint is reported, and when any Rust file exceeds 500 lines (subject to the approved policy)
- [ ] #3 Required-gate decisions never depend on `|| true` or on missing lint output
- [ ] #4 Remediation work is split into reviewable MRs by responsibility, each with focused tests
- [ ] #5 No threshold change, lint suppression, or exclusion of application or test code is used to reach a pass unless the approved policy explicitly allows it
- [ ] #6 The final exact-head pipeline shows complexity-check and coverage-check both passing
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Strict gate behavior is covered by launcher fixture tests (Clippy failure, lint finding, oversized file, clean tree)
<!-- DOD:END -->
