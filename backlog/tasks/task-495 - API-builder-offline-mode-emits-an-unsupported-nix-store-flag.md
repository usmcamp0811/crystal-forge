---
id: TASK-495
title: API builder offline mode emits an unsupported nix-store flag
status: Review
assignee:
  - '@openai'
created_date: '2026-10-19 05:37'
updated_date: '2026-10-19 19:20'
labels: []
dependencies: []
references:
  - packages/default/crates/cf-builder/src/derivations/build.rs
  - modules/nixos/crystal-forge/default.nix
  - checks/integration-delivery/scenarios/fragments/source-build.py
  - /tmp/opencode/delivery-local-run12.log
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/pipelines/2964502717'
modified_files:
  - packages/default/crates/cf-config/src/config/build.rs
  - modules/nixos/crystal-forge/default.nix
priority: high
type: bug
ordinal: 546000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A real API-only builder received and materialized an authorized NixOS derivation, then failed before realization because offline=true passed --offline to nix-store 2.34.8, which rejects that flag. The owner explicitly authorized a separate product fix after this defect blocked the unchanged Delivery proof. This is not the already-resolved state-machine CI networking/Bash issue.

The correction lives in shared BuildConfig argument generation. It replaces the modern-only flag with compatible acquisition settings for both legacy nix-store and modern Nix, while preserving resource/sandbox and ordinary online behavior. Offline mode disables substitution, substituters, remote builders and fallback; it is not an OS firewall for fixed-output builders or a prohibition on required API/publication traffic. Full network isolation remains a host responsibility.

The implementation was developed in its dedicated TASK-495 worktree and committed separately. It is applied to the coordinator branch with offline=true and all actual realization/output assertions retained. No schema or protocol transition is involved.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 A configured offline API-only builder can realize an authorized derivation using the supported Nix CLI without an unsupported-flag error.
- [x] #2 Offline realization cannot acquire external substitutes or use remote builders or network fallback.
- [x] #3 Tests cover the effective invocation and a real successful local realization with its output absent before execution.
- [x] #4 Affected configuration and maintainer documentation describe the actual offline guarantee and missing-prerequisite failure.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
Owner-authorized separate product correction. Worktree /home/mcamp/code/crystal-forge/TASK-495-builder-offline, branch TASK-495-builder-offline, base dev at f3db8618. No main worktree is checked out; dev has no tracked modifications and its unrelated locks/session file remain untouched. Task lock is active.
1. Correct the shared BuildConfig::apply_to_command offline branch in cf-config, which currently appends modern-only --offline even when the caller is nix-store --realise. Use legacy-compatible Nix options that explicitly disable substitution, external substituters, remote builders and fallback. Keep ordinary online/resource/sandbox behavior unchanged and preserve all actual build assertions.
2. Add focused ordinary cf-config argument tests for direct nix-store, systemd-scoped nix-store and modern Nix callers, including offline overriding use_substitutes=true and online behavior unchanged. No new gate or testing framework.
3. Update adjacent public Rust contracts and the existing NixOS build.offline option documentation. Distinguish Nix input-acquisition policy from an OS network firewall and from API/publication traffic.
4. Run targeted cf-config tests, formatting and rustdoc through Nix, then build the real builder package. Commit the bounded product fix separately. Apply that reviewed product commit to the coordinator branch without modifying its Delivery assertions/configuration. Run the same offline=true Delivery proof to establish actual absent-before/present-after Nix realization; retain precise failures and no xfail. Do not merge/deploy or mark Done.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
The owner explicitly authorized a separate product fix in response to the Delivery blocker question. The test must keep offline=true and retain all build, output and network assertions. Use a dedicated task worktree; do not treat the failure as expected success.

## Implementation and local verification

- Dedicated worktree: `/home/mcamp/code/crystal-forge/TASK-495-builder-offline`; product commit `6844d185`, applied to the coordinator branch as `fc0c5836`.
- Focused command: `nix develop .#sqlx -c env SQLX_OFFLINE=true cargo test --offline --manifest-path packages/default/Cargo.toml --package cf-config --lib config::build::tests`: 5 passed, including new legacy/modern/scoped argument and online-policy regressions.
- `cargo test --offline --manifest-path packages/default/Cargo.toml --package cf-config --doc` through the SQLx shell: 1 doctest passed. `cargo doc --offline ... --package cf-config --no-deps` succeeded with three pre-existing bare-URL warnings in builder.rs/oidc.rs; no documentation suppression added.
- Targeted rustfmt and whitespace checks passed. The final emitted options were accepted by nix-store 2.34.5 without the unsupported flag or unknown-setting warning.
- Normal production builder package in the coordinator build passed 99 library, 34 binary and 2 integration tests. Agent package passed 15 library, 11 binary and 1 integration tests. These completed before the later embedded-UI preparation timeout; that timeout launched no VM.
- The unchanged offline=true Delivery path subsequently performed a real NixOS realization from an independently absent nonce output. The latest full normal aggregate passed all 8 cases, including signed publication, exact job/session/output completion and independent second builder session. Evidence: `/nix/store/bys8qx6hix4kvf1yqycjbiqq2pmr1sc4-vm-test-run-cf-it-delivery-delivery-lifecycle-run`, working tree based on fc0c5836; not a clean-revision claim.
- Final clean-revision/hosted verification remains pending recording. Keep In Progress; no merge, deployment or Done.

## Verified code and review handoff

Code revision `bea0c29936c2220a8c1ba8dbfeb5750dacc18cde` passed hosted pipeline 2964502717. All eight required jobs succeeded: Access, Delivery, component, state-machine, CVE, dashboard, complexity and coverage. The archived Delivery result records all 8 cases passed with offline=true, outputAbsentBeforeBuild=true, outputRealized=true, buildCompleted=true and publicEgressDenied=true. It preserves exact source, native attempt, drv, job, builder-session and target identities.

The same clean revision passed both normal aggregates locally (Access 23/23, Delivery 8/8), all 96 cf-config tests and the documented package/schema/tool checks. The hosted state-machine trace totals 515 passed, 0 failed, 0 ignored across 227 summaries. Earlier preparation/cold-store failures remain recorded; they are not counted as successful tests.

Open review vehicle: draft MR !335. Product code remains the separate commit 6844d185, applied as fc0c5836; no offline=false workaround, expected failure or production test bypass was added. All four task criteria are verified. Move to Review, not Done; the MR is not merged and the task worktree remains available.
<!-- SECTION:NOTES:END -->
