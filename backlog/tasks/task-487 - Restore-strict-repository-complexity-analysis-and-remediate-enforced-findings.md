---
id: TASK-487
title: Restore strict repository complexity analysis and remediate enforced findings
status: In Progress
assignee:
  - openai-agent
created_date: '2026-10-15 00:39'
updated_date: '2026-10-15 00:43'
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

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Use dedicated branch TASK-487-strict-complexity-remediation and worktree /home/mcamp/code/crystal-forge/TASK-487-strict-complexity-remediation from TASK-470's published 5ebfa887. Preserve the parent's uncommitted metrics repair and owner lock. Dev has no tracked changes; existing unrelated untracked TASK-478 lock/session are preserved; no main worktree is registered.
2. Inventory the current report honestly: 529 source files, 714 partial warning diagnostics, 196 files over 500 physical lines, 13 fatal locations in four files. Analysis is incomplete until backend/UI Clippy both succeed. Original shell parsing missed codes; 500-line clause was already reported in JUnit but not reflected in shell exit. User authorized strict remediation, so make the existing reported clauses truthful without adding thresholds, suppressions, or exclusions.
3. First unblock analysis with behavior-preserving fixes: conditional pagination uses if plus loop; replace vacuous deployment-policy test assertion with explicit fixture membership; put browser polling loops under whole-loop wasm cfg and preserve native no-op. Coordinate source ownership and retain all authorization, transaction, pagination, refresh and timeout contracts.
4. Validate focused regressions and offline/native/WASM compilation through pinned Nix toolchain with two Cargo jobs. Rerun complete strict analysis only after fatal blockers are resolved; record full inventory and any newly reached targets.
5. Create responsibility-based remediation batches for server compliance/POAM/evaluation snapshots/builders, UI tabs/state/DTOs, config/protocol/agent and tests. No arbitrary chunk splitting or abstractions solely to hide metrics. Keep physical-line and six selected lint clauses unchanged; application code remains inventoried.
6. Run strict analysis, coverage and complete required exact-head CI before declaring parent MR331 ready. Do not merge, deploy, change production credentials/data, or overwrite other agents' work. Do not integrate this child work into TASK-470 until verified and explicitly recorded.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dedicated worktree created and verified clean at 5ebfa8876e797e4aac1097a3263ecb2e51cdd965. User selected Authorize strict remediation. Scope can take substantially longer than the parent integration repair; no readiness claim. Parent metrics artifacts /tmp/opencode/task470-complexity-repaired remain the partial inventory source, not a passing report.
<!-- SECTION:NOTES:END -->
