---
id: TASK-487
title: Restore strict repository complexity analysis and remediate enforced findings
status: In Progress
assignee:
  - openai-agent
created_date: '2026-10-15 00:39'
updated_date: '2026-10-15 02:27'
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

Completed the bounded ten-loop UI fatal-blocker pass in evaluations.rs (five loops) and system_detail.rs (five loops). Each whole loop now has #[cfg(target_arch = "wasm32")]. Removed native-only breaks and retained the two existing native no-op capture expressions outside their loops. Browser loop bodies, delays, termination conditions, poll bounds and hook registration remain unchanged. No suppressions, thresholds, exclusions, public API or backend changes were made by this pass.

Verification passed (exit 0): nix develop --command nix shell --inputs-from . nixpkgs#clippy nixpkgs#rustfmt --command sh -c 'CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/opencode/task487-ui-native cargo clippy --manifest-path packages/web-ui/Cargo.toml --locked --offline --all-targets --all-features --message-format=short -- -D clippy::never_loop'. Log: /tmp/opencode/task487-ui-native-clippy.log. Native bin and test targets finished with other warnings. The first attempt exceeded the 120-second tool timeout; rerun completed successfully.

Verification passed (exit 0): nix develop --command sh -c 'CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/opencode/task487-ui-wasm cargo check --manifest-path packages/web-ui/Cargo.toml --locked --offline --target wasm32-unknown-unknown --all-features --message-format=short'. Log: /tmp/opencode/task487-ui-wasm-check.log. Formatting check passed: nix develop --command nix shell --inputs-from . nixpkgs#rustfmt --command rustfmt --edition 2024 --check packages/web-ui/src/views/evaluations.rs packages/web-ui/src/views/system_detail.rs. git diff --check passed. Documentation review: no public contracts or polling semantics changed; existing lifecycle comments remain valid.

Concurrent changes appeared in cf-server handlers/api/deployment_policies.rs and services/register_export_selection.rs during verification. This pass did not edit those files. UI changes remain uncommitted in the dedicated TASK-487 worktree. Full strict metrics, coverage and exact-head CI remain outstanding; TASK-487 stays In Progress.

Verified partial checkpoint committed and pushed to dedicated branch as9f0cb90cea4c7c2c5929fcbccd1ed058fa5fa255 (`TASK-487: Unblock strict analysis and separate CA trust responsibilities`). No integration into TASK-470/MR331, no merge/deploy. Parent TASK-470 remains published5ebfa887; its original owned metrics WIP remains preserved and byte-identical to the child port. Child worktree tracked files clean, owner lock untracked/intentionally retained.

Fatal-blocker verification: complete native Clippy for both backend workspace and UI nowexit0/build-finished success, zero error-severitydiagnostics. Whole-loop wasm cfg preserves10browserpolling loops and native no-op, WASMcompilationpassed. Backend conditionalpagination loops retaintransaction/auth/caps/order; deployment-policy fixture assertsrealID/count insteadvacuouslen>=0. Server-regressions isolatedsandboxpassed1183s:59normallyignoredlib+83nonignoredexact+7register-export integrationcases allsuccess; cluster cleanlystopped. No production/localdefaultDB used.

CA responsibility refactor verified:8source/testfilesallunder500lines, publicsignatures/modulepathsunchanged, no selectedcomplexitydiagnostics. cf-config94unitpassed incl13CA,14doctestspassed,rustdocbrokenlinksdeniedpassed, originalClippygroup+6clausesexecuted, nativeGo CA3datasets match122ordinaryroots/172AUXeach. No suppressions, exclusions, threshold changes, dependencies, or trust-selection behavior changes. Evidence /tmp/opencode/task487-ca-verified/verification.md.

Fresh self-contained checkpoint inventory using child launcher: both Clippy exits0 (backend139s679findings/UI1s54cached), STRICTMETRICSexit1/FAILED, notINCOMPLETE. Actualcomplete-native counts733selectedfindings:199cognitive/71args/429lines/32types/2bools/0vec_box. 536trackedRustfiles allincluded;195over500. Previous complete-native734/196 became733/195afterCA. Initial714waspartial, notregressioncomparison. JUnit539cases196failures0errors0skips. Five launcherfixturecases+actualartifactvalidationpassed; rawJSONL0600. Artifacts /tmp/opencode/task487-checkpoint-inventory, log /tmp/opencode/task487-checkpoint-inventory-run.log. WASMlintscopeandcoverageNOTassessed bynativeinventory.

Strict remediation and parent readiness remain incomplete. Nextcoherentbatches: servercompliance/POAM/evaluation snapshots/builders, UI tab/render/state/DTO modules, remainingconfig/protocol/agent/tests. Do not arbitrarysplitfiles,hidelints,lowerthresholds,excludeapplicationtestsorclaimMRready. Requiredcomplexityremainsred andcoverageunverified untilremainingfindingsare remediated andcompleteexact-headCIpasses.

User corrected branch placement: all strict-remediation implementation must be on TASK-470/MR331. Parent TASK-470 worktree was verified at published5ebfa887 with identical owned metrics WIP. Those exact files were preserved in a named stash; TASK-470 then fast-forwarded to this verified checkpoint9f0cb90c and pushed normally. No history rewrite, MR merge, or deployment. This task's remaining implementation scope is consolidated into active TASK-470; this record is archived as duplicate planning, not marked complete. Do not implement further in the TASK-487 branch/worktree. Strict findings remain733/195, parent not merge-ready.
<!-- SECTION:NOTES:END -->
