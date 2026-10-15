---
id: TASK-487
title: Restore strict repository complexity analysis and remediate enforced findings
status: In Progress
assignee:
  - openai-agent
created_date: '2026-10-15 00:39'
updated_date: '2026-10-15 01:02'
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

Current bounded pass owns only the ten target-specific polling loops in packages/web-ui/src/views/evaluations.rs and system_detail.rs. Put the wasm32 cfg on each whole loop and retain native no-op captures outside the loop. Preserve browser loop bodies, delays, bounds and hook order. Verify native Clippy and wasm32 compilation with bounded Cargo jobs through the pinned Nix environment. This pass does not complete repository-wide strict remediation.

Initial source-only ownership: services/register_export_selection.rs and the fatal deployment_policies.rs list test. Replace immutable while conditions with if plus loop without changing loop bodies or SQL. Convert the vacuous list test to an isolated sqlx::test using create_test_policy; assert baseline-plus-one count and exact fixture identity despite migration seed data. Preserve public rustdoc contracts. Run light formatting/diff checks only; hand off existing pagination/scope/cap integration regressions and the isolated fixture test to the parent's serial runner. No full server-regressions or concurrent Cargo compilation in this initial batch.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Dedicated worktree created and verified clean at 5ebfa8876e797e4aac1097a3263ecb2e51cdd965. User selected Authorize strict remediation. Scope can take substantially longer than the parent integration repair; no readiness claim. Parent metrics artifacts /tmp/opencode/task470-complexity-repaired remain the partial inventory source, not a passing report.

Resumed existing TASK-487 branch/worktree at 5ebfa887 with matching openai-agent task lock. No tracked changes in the task worktree or dev; dev has existing unrelated untracked lock/session files. No main worktree is registered. The referenced docs/agent/worktrees.md and verification.md paths are absent at this base; inspect available repository tooling instead. This is a target-compilation-only correction with unchanged browser behavior.

Server fatal-blocker source handoff ready in /home/mcamp/code/crystal-forge/TASK-487-strict-complexity-remediation at base 5ebfa887. This actor edited only packages/default/crates/cf-server/src/services/register_export_selection.rs and src/handlers/api/deployment_policies.rs. The two immutable while conditions now use if plus loop; original loop bodies, repeatable-read transaction, authorization, deduplication, row ordering/caps, next_offset failure, and acceptance offset increment remain intact. Expected removal: two while_immutable_condition diagnostics and one absurd_extreme_comparisons diagnostic; Clippy has NOT been executed by this actor.

Replaced test_list_deployment_policies_empty with ignored isolated sqlx::test test_list_deployment_policies_includes_fixture. Uses existing create_test_policy helper and a dynamic COUNT(*) query; migration seed count plus one must match the returned total. Fixture UUID appears exactly once, with expected name/type/enabled fields. Does not use the legacy hardcoded localhost test_pool. No assertions weakened; no SQLx checked-query/schema changes or metadata updates. No new/materially changed public Rust items; existing public rustdoc contracts remain accurate and preserved.

Light verification: nix develop --command rustfmt --edition 2024 --check packages/default/crates/cf-server/src/services/register_export_selection.rs packages/default/crates/cf-server/src/handlers/api/deployment_policies.rs initially exited 1 for fixture chain formatting; corrected with a scoped patch and reran successfully (exit 0). git diff --check passed (exit 0). Reviewed owned-file diff for documentation and behavior preservation. Concurrent UI actor changes in evaluations.rs and system_detail.rs are present; this actor did not edit them.

Parent single-runner handoff: through pinned Nix environment, SQLX_OFFLINE=true CARGO_BUILD_JOBS=2, and verified disposable PostgreSQL only, run cargo test --manifest-path packages/default/Cargo.toml -p cf-server --lib handlers::api::deployment_policies::tests::test_list_deployment_policies_includes_fixture -- --ignored --exact --test-threads=1; run cargo test --manifest-path packages/default/Cargo.toml -p cf-server --test register_export_selection -- --ignored --test-threads=1. Existing integration cases cover multiple acceptance pages/rechecked authorization, acceptance-only scope, Plans excluding acceptance rows, cap rejection, linked-context visibility and historical context. Runtime tests and actual fatal-Clippy removal remain pending. No Cargo compilation/full server-regressions started, no commits or pushes, no task-completion claim.
<!-- SECTION:NOTES:END -->
