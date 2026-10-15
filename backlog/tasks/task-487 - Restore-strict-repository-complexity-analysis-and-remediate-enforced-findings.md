---
id: TASK-487
title: Restore strict repository complexity analysis and remediate enforced findings
status: To Do
assignee: []
created_date: '2026-10-15 00:39'
labels: []
dependencies: []
references:
  - packages/code-metrics/default.nix
  - packages/code-metrics/test-report.py
  - /tmp/opencode/task470-complexity-repaired/
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/331'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/jobs/17185484674'
priority: high
type: chore
ordinal: 509000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
User explicitly authorized strict remediation after TASK-470's newly unblocked required complexity job17185484674 failed. Preserve thresholds, no lint suppressions, no skipped/optional required jobs. Baseline is TASK-470 remote5ebfa8876e797e4aac1097a3263ecb2e51cdd965. Current owned uncommitted metrics repair in TASK-470 supplies pkg-config/OpenSSL and bounds Cargo to2, generates honest structured reports. Actual run ends101 for both workspace/backend and frontend, with while_immutable_condition/absurd_extreme_comparisons/never_loop fatal diagnostics; partial diagnostics record714 distinct complexity findings and196 files over500lines. The historical137 signal sender is unknown. Determine original gating/report semantics before changing enforcement; no silent policy expansion or weakening. Inventory complete findings after fixing compiler blockers, then split remediation by responsibility with independent tests. Preserve TASK-470 credentials, authorization, publication, and UI semantics, and coordinate evaluation-related code ownership. No production access, merge, or deploy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Complexity command has reproducible native/toolchain inputs and bounded resources and produces complete truthful HTML JSON and JUnit artifacts
- [ ] #2 Compiler and fatal Clippy blockers are fixed with unchanged observable contracts and focused regression coverage
- [ ] #3 All existing enforced complexity clauses are met without threshold changes lint suppressions or exclusions that conceal application code
- [ ] #4 Repository-wide remediation is divided by real responsibilities with preserved security persistence and UI behavior rather than arbitrary file-size splitting
- [ ] #5 Required exact-head complexity and coverage jobs pass with the complete required CI matrix before declaring MR331 ready
<!-- AC:END -->
